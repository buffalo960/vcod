//! The server Start New Server and a console `map` run: `vcod-server`'s
//! frame loop (`vcod_server::host::run`) on a thread of this process, on its
//! own UDP port, joined through the normal connect path. With `dedicated`
//! 0 it is retail's listen server; with 1 or 2 the client shuts down and
//! the process stays up as the dedicated server, as retail's does
//! (docs/research/cod11-front-end.md, section 18).

use std::path::Path;
use std::sync::mpsc::{Sender, channel};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result};
use vcod_server::host::{self, HostConfig};

use crate::frontend::create::StartServer;

/// The first port tried, retail's `net_port`; the next 31 follow.
pub const FIRST_PORT: u16 = 28960;

/// The client's cvars a listen server reads off the shared console: every
/// `sv_`, `g_` and `scr_` one, and `rconPassword`. `g_gametype`, `sv_cheats`
/// and `fs_game` come from the request instead.
fn shared_cvar(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let prefixed = ["sv_", "g_", "scr_"].iter().any(|p| n.starts_with(p));
    (prefixed || n == "rconpassword") && !matches!(n.as_str(), "g_gametype" | "sv_cheats")
}

/// The server `req` asks for, on `port`, with the cvars `cvars` holds.
/// `fs_game` is the player's mod, which a listen server shares with the
/// client.
pub fn config<'a>(
    req: &StartServer,
    port: u16,
    (game_dir, mod_dir, fs_game): (&Path, &str, Option<&str>),
    cvars: impl Iterator<Item = (&'a str, &'a str)>,
) -> HostConfig {
    let mut cfg = HostConfig {
        map: req.map.clone(),
        game_dir: game_dir.to_path_buf(),
        mod_dir: mod_dir.to_string(),
        port,
        hostname: "CoDHost".into(),
        max_clients: 20,
        gametype: req.gametype.clone(),
        gametype_script: None,
        test_entities: 0,
        bots: 0,
        bots_shoot: false,
        sets: vec![("dedicated".into(), req.dedicated.to_string())],
        ban_file: None,
        trace: false,
        listen: req.dedicated == 0,
    };
    if let Some(game) = fs_game.filter(|g| !g.is_empty()) {
        cfg.sets.push(("fs_game".into(), game.into()));
    }
    if req.cheats {
        cfg.sets.push(("sv_cheats".into(), "1".into()));
    }
    for (name, value) in cvars.filter(|(n, v)| shared_cvar(n) && !v.is_empty()) {
        if name.eq_ignore_ascii_case("sv_hostname") {
            cfg.hostname = value.into();
        } else if name.eq_ignore_ascii_case("sv_maxclients") {
            // The server takes a count; junk keeps the default.
            if let Ok(n) = value.trim().parse::<usize>()
                && n > 0
            {
                cfg.max_clients = n;
            }
        } else {
            cfg.sets.push((name.into(), value.into()));
        }
    }
    cfg
}

/// A server running on a thread of this process. Dropping it runs `quit`
/// and waits for the thread, as `SV_Shutdown` does.
pub struct ListenServer {
    console: Sender<String>,
    thread: Option<JoinHandle<Result<()>>>,
    pub port: u16,
}

impl ListenServer {
    /// Binds the first free port from `cfg.port` and starts the server on
    /// it. The map and scripts load on the thread; a failure there ends it
    /// ([`ListenServer::failure`]).
    pub fn start(mut cfg: HostConfig) -> Result<ListenServer> {
        let first = cfg.port;
        let (sock, port) = (first..first.saturating_add(32))
            .find_map(|p| host::bind(p).ok().map(|s| (s, p)))
            .with_context(|| format!("no free UDP port from {first}"))?;
        cfg.port = port;
        let (console, lines) = channel();
        let thread = std::thread::Builder::new()
            .name("server".into())
            .spawn(move || host::run(cfg, sock, lines))
            .context("starting the server thread")?;
        Ok(ListenServer {
            console,
            thread: Some(thread),
            port,
        })
    }

    /// A line for the server's console (`map`, `map_restart`, `kick`).
    pub fn command(&self, line: &str) {
        let _ = self.console.send(line.to_string());
    }

    pub fn running(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }

    /// Why the server ended on its own, once it has.
    pub fn failure(&mut self) -> Option<String> {
        if self.running() {
            return None;
        }
        match self.thread.take()?.join() {
            Ok(Ok(())) => Some("the server quit".into()),
            Ok(Err(e)) => Some(format!("{e:#}")),
            Err(_) => Some("the server panicked".into()),
        }
    }

    /// A dedicated server after the window has closed: runs until it quits
    /// (an rcon `quit`) or a quit signal arrives, which it passes on as
    /// `quit` so the masters get their flatline.
    pub fn run_dedicated(mut self) -> Result<()> {
        let mut asked = false;
        while self.running() {
            if !asked && crate::quit::requested() {
                asked = true;
                self.command("quit");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        match self.thread.take().map(JoinHandle::join) {
            Some(Ok(r)) => r,
            Some(Err(_)) => anyhow::bail!("the server panicked"),
            None => Ok(()),
        }
    }
}

impl Drop for ListenServer {
    fn drop(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        self.command("quit");
        match thread.join() {
            Ok(Err(e)) => log::error!("server: {e:#}"),
            Err(_) => log::error!("server: panicked"),
            Ok(Ok(())) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use vcod_common::net::NetClient;

    fn req(dedicated: i32) -> StartServer {
        StartServer {
            map: "mp_harbor".into(),
            gametype: "tdm".into(),
            dedicated,
            cheats: false,
        }
    }

    #[test]
    fn the_shared_console_maps_onto_the_server_config() {
        let cvars = [
            ("sv_hostname", "My Host"),
            ("sv_maxclients", "12"),
            ("sv_pure", "0"),
            ("g_password", ""),
            ("g_gametype", "dm"),
            ("scr_tdm_timelimit", "5"),
            ("rconPassword", "pw"),
            ("name", "player"),
            ("sv_cheats", "1"),
        ];
        let cfg = config(
            &req(0),
            28961,
            (Path::new("/cod"), "main", Some("mymod")),
            cvars.into_iter(),
        );
        assert_eq!((cfg.hostname.as_str(), cfg.max_clients), ("My Host", 12));
        assert_eq!((cfg.gametype.as_str(), cfg.port), ("tdm", 28961));
        assert!(cfg.listen);
        let sets: Vec<_> = cfg.sets.iter().map(|(n, v)| format!("{n}={v}")).collect();
        assert_eq!(
            sets,
            [
                "dedicated=0",
                "fs_game=mymod",
                "sv_pure=0",
                "scr_tdm_timelimit=5",
                "rconPassword=pw"
            ]
        );
        let junk = config(
            &req(2),
            1,
            (Path::new("d"), "main", None),
            [("sv_maxclients", "x")].into_iter(),
        );
        assert_eq!(junk.max_clients, 20);
        assert!(!junk.listen);
    }

    /// The local client joins its own listen server over loopback: slot 0
    /// past two private slots, and no `g_password` asked of it.
    #[test]
    fn the_local_client_joins_its_listen_server_as_client_0() {
        if vcod_common::testing::game_fs().is_none() {
            return;
        }
        let cvars = [
            ("g_password", "secret"),
            ("sv_privateClients", "2"),
            ("sv_maxclients", "4"),
        ];
        let dir = vcod_common::testing::game_dir();
        let mut cfg = config(&req(0), 30655, (&dir, "main", None), cvars.into_iter());
        cfg.gametype = "dm".into();
        let server = ListenServer::start(cfg).unwrap();
        let mut net = NetClient::connect(&format!("127.0.0.1:{}", server.port)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        while net.gamestate().is_none() && Instant::now() < deadline {
            net.pump();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(net.gamestate().map(|g| g.client_num), Some(0));
        net.disconnect();
        drop(server);
    }
}
