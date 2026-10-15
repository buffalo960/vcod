//! `SV_MasterHeartbeat` (cod_lnxded 0x808ba0c): the `heartbeat COD-1` a
//! `dedicated 2` server sends the master list, and the `flatline` it sends on
//! the way down; `SV_MasterGameCompleteStatus` (0x808bbc4), the console's
//! `gameCompleteStatus`. Numbers and addresses are in
//! docs/research/cod11-server-handshake.md, "Master heartbeat" and "Console
//! commands over rcon".

use std::net::{SocketAddr, ToSocketAddrs};
use vcod_common::net::connectionless::build_oob;

/// `HEARTBEAT_MSEC` (`add eax,0x2bf20` at 0x808ba3d).
pub const HEARTBEAT_MS: i32 = 180_000;
/// `PORT_MASTER` (`push 0x501e` at 0x808baff). Every master gets it: retail's
/// `strstr(":", name)` has its arguments swapped, so a port in the cvar is
/// parsed and then overwritten.
pub const MASTER_PORT: u16 = 20510;
/// `sv_master1`'s default; `sv_master2..5` default empty.
pub const DEFAULT_MASTER: &str = "codmaster.activision.com";
pub const HEARTBEAT_GAME: &str = "COD-1";
pub const FLATLINE: &str = "flatline";
pub const MAX_MASTERS: usize = 5;

/// `sv_master1..5`, their resolved addresses and `svs.nextHeartbeatTime`.
pub struct Masters {
    names: [String; MAX_MASTERS],
    /// `cvar->modified`: resolve again before the next send.
    dirty: [bool; MAX_MASTERS],
    addrs: [Option<SocketAddr>; MAX_MASTERS],
    /// `SV_MasterGameCompleteStatus`'s own address table (0x8160b20, the
    /// heartbeat's is 0x8160aa0). Whichever of the two sees `dirty` first
    /// resolves into its table alone.
    status_addrs: [Option<SocketAddr>; MAX_MASTERS],
    next_ms: i32,
}

impl Default for Masters {
    fn default() -> Self {
        Masters {
            names: [
                DEFAULT_MASTER.to_string(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ],
            dirty: [true; MAX_MASTERS],
            addrs: [None; MAX_MASTERS],
            status_addrs: [None; MAX_MASTERS],
            next_ms: 0,
        }
    }
}

impl Masters {
    /// `sv_master<n>` for `n` in 1..=5; anything else is not one of them.
    pub fn index(cvar: &str) -> Option<usize> {
        let n = cvar
            .to_ascii_lowercase()
            .strip_prefix("sv_master")?
            .parse::<usize>()
            .ok()?;
        (1..=MAX_MASTERS).contains(&n).then(|| n - 1)
    }

    pub fn set(&mut self, i: usize, name: &str) {
        self.names[i] = name.to_string();
        self.dirty[i] = true;
    }

    /// `SV_Heartbeat_f` (0x8084bd0): the next frame sends.
    pub fn force(&mut self) {
        self.next_ms = -9_999_999;
    }

    /// One `SV_MasterHeartbeat(text)`: nothing unless `dedicated` is 2 and the
    /// timer is due. A name that fails to resolve is cleared, as retail's
    /// `Cvar_Set(name, "")` does, so it is never tried again.
    pub fn heartbeat(
        &mut self,
        dedicated: i32,
        now_ms: i32,
        text: &str,
        resolve: &mut dyn FnMut(&str) -> Option<SocketAddr>,
    ) -> Vec<(SocketAddr, Vec<u8>)> {
        if dedicated != 2 || now_ms < self.next_ms {
            return Vec::new();
        }
        self.next_ms = now_ms.saturating_add(HEARTBEAT_MS);
        let mut out = Vec::new();
        for i in 0..MAX_MASTERS {
            if self.names[i].is_empty() {
                continue;
            }
            if self.dirty[i] {
                self.dirty[i] = false;
                log::info!("Resolving {}", self.names[i]);
                match resolve(&self.names[i]) {
                    Some(mut a) => {
                        a.set_port(MASTER_PORT);
                        log::info!("{} resolved to {a}", self.names[i]);
                        self.addrs[i] = Some(a);
                    }
                    None => {
                        log::info!("Couldn't resolve address: {}", self.names[i]);
                        self.names[i].clear();
                        self.addrs[i] = None;
                        continue;
                    }
                }
            }
            if let Some(a) = self.addrs[i] {
                log::info!("Sending heartbeat to {}", self.names[i]);
                out.push((a, build_oob(&format!("heartbeat {text}\n"))));
            }
        }
        out
    }

    /// `SV_MasterGameCompleteStatus` (0x808bbc4): nothing unless
    /// `dedicated` is 2. Returns what it prints and the masters to send the
    /// `gameCompleteStatus` packet to: a master the heartbeat resolved first
    /// has no address in this table, so it is named and sent nothing.
    pub fn game_complete_status(
        &mut self,
        dedicated: i32,
        resolve: &mut dyn FnMut(&str) -> Option<SocketAddr>,
    ) -> (String, Vec<SocketAddr>) {
        let (mut text, mut out) = (String::new(), Vec::new());
        if dedicated != 2 {
            return (text, out);
        }
        for i in 0..MAX_MASTERS {
            if self.names[i].is_empty() {
                continue;
            }
            if self.dirty[i] {
                self.dirty[i] = false;
                text.push_str(&format!("Resolving {}\n", self.names[i]));
                match resolve(&self.names[i]) {
                    Some(mut a) => {
                        a.set_port(MASTER_PORT);
                        text.push_str(&format!("{} resolved to {a}\n", self.names[i]));
                        self.status_addrs[i] = Some(a);
                    }
                    None => {
                        text.push_str(&format!("Couldn't resolve address: {}\n", self.names[i]));
                        self.names[i].clear();
                        self.status_addrs[i] = None;
                        continue;
                    }
                }
            }
            text.push_str(&format!(
                "Sending gameCompleteStatus to {}\n",
                self.names[i]
            ));
            out.extend(self.status_addrs[i]);
        }
        (text, out)
    }

    /// `SV_MasterShutdown` (0x808d268): one `flatline`, sent at once.
    pub fn shutdown(
        &mut self,
        dedicated: i32,
        now_ms: i32,
        resolve: &mut dyn FnMut(&str) -> Option<SocketAddr>,
    ) -> Vec<(SocketAddr, Vec<u8>)> {
        self.next_ms = -9999;
        self.heartbeat(dedicated, now_ms, FLATLINE, resolve)
    }
}

/// `NET_StringToAdr` for a master name: the first IPv4 address it resolves
/// to. The port is replaced by [`MASTER_PORT`] either way.
pub fn resolve(name: &str) -> Option<SocketAddr> {
    let host = name.rsplit_once(':').map_or(name, |(h, _)| h);
    (host, MASTER_PORT)
        .to_socket_addrs()
        .ok()?
        .find(SocketAddr::is_ipv4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(_: &str) -> Option<SocketAddr> {
        Some("127.0.0.1:29463".parse().unwrap())
    }

    /// The heartbeat and `gameCompleteStatus` keep separate address tables
    /// behind one `modified` flag: whichever resolves a master first is the
    /// only one that can reach it.
    #[test]
    fn game_complete_status_reaches_only_masters_it_resolved_itself() {
        let mut m = Masters::default();
        m.set(0, "127.0.0.1");
        assert_eq!(m.heartbeat(2, 0, HEARTBEAT_GAME, &mut local).len(), 1);
        m.set(1, "127.0.0.1");
        let (text, to) = m.game_complete_status(2, &mut local);
        assert_eq!(
            text,
            "Sending gameCompleteStatus to 127.0.0.1\nResolving 127.0.0.1\n\
             127.0.0.1 resolved to 127.0.0.1:20510\n\
             Sending gameCompleteStatus to 127.0.0.1\n"
        );
        assert_eq!(to, vec!["127.0.0.1:20510".parse().unwrap()]);
        m.force();
        assert_eq!(m.heartbeat(2, 0, HEARTBEAT_GAME, &mut local).len(), 1);
        assert_eq!(
            m.game_complete_status(1, &mut local),
            (String::new(), vec![])
        );
    }

    #[test]
    fn heartbeats_only_on_dedicated_2_every_three_minutes() {
        let mut m = Masters::default();
        assert!(m.heartbeat(1, 50, HEARTBEAT_GAME, &mut local).is_empty());
        let out = m.heartbeat(2, 50, HEARTBEAT_GAME, &mut local);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "127.0.0.1:20510".parse().unwrap());
        assert_eq!(out[0].1, b"\xff\xff\xff\xffheartbeat COD-1\n");
        assert!(
            m.heartbeat(2, 50 + HEARTBEAT_MS - 1, HEARTBEAT_GAME, &mut local)
                .is_empty()
        );
        assert_eq!(
            m.heartbeat(2, 50 + HEARTBEAT_MS, HEARTBEAT_GAME, &mut local)
                .len(),
            1
        );
        m.force();
        assert_eq!(
            m.heartbeat(2, 50 + HEARTBEAT_MS + 50, HEARTBEAT_GAME, &mut local)
                .len(),
            1
        );
    }

    #[test]
    fn flatline_ignores_the_timer_but_not_dedicated() {
        let mut m = Masters::default();
        assert_eq!(m.heartbeat(2, 50, HEARTBEAT_GAME, &mut local).len(), 1);
        let out = m.shutdown(2, 100, &mut local);
        assert_eq!(out[0].1, b"\xff\xff\xff\xffheartbeat flatline\n");
        assert!(Masters::default().shutdown(1, 0, &mut local).is_empty());
    }

    #[test]
    fn an_unresolvable_master_is_cleared_and_not_retried() {
        let mut m = Masters::default();
        let mut calls = 0;
        let mut fail = |_: &str| {
            calls += 1;
            None
        };
        assert!(m.heartbeat(2, 0, HEARTBEAT_GAME, &mut fail).is_empty());
        m.force();
        assert!(m.heartbeat(2, 50, HEARTBEAT_GAME, &mut fail).is_empty());
        assert_eq!(calls, 1);
    }

    #[test]
    fn every_master_slot_is_sent_and_only_a_set_one_resolves_again() {
        let mut m = Masters::default();
        m.set(Masters::index("SV_MASTER3").unwrap(), "127.0.0.2:1234");
        let mut calls = Vec::new();
        let mut r = |n: &str| {
            calls.push(n.to_string());
            local(n)
        };
        assert_eq!(m.heartbeat(2, 0, HEARTBEAT_GAME, &mut r).len(), 2);
        m.force();
        assert_eq!(m.heartbeat(2, 50, HEARTBEAT_GAME, &mut r).len(), 2);
        assert_eq!(calls, [DEFAULT_MASTER, "127.0.0.2:1234"]);
        assert_eq!(Masters::index("sv_master6"), None);
        assert_eq!(Masters::index("sv_masterx"), None);
    }
}
