//! The UDP frame loop around a [`Server`]: `vcod-server`'s `main` and the
//! client's listen server both run [`run`], on a socket they bind first.

use anyhow::{Context, Result, bail};
use std::net::UdpSocket;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};
use vcod_common::pk3::Pk3Fs;

use crate::{Server, ServerConfig};

/// What a server is started with: `vcod-server`'s command line.
pub struct HostConfig {
    pub map: String,
    /// The install; the paks are under `mod_dir` in it.
    pub game_dir: PathBuf,
    pub mod_dir: String,
    pub port: u16,
    pub hostname: String,
    pub max_clients: usize,
    pub gametype: String,
    /// A gametype script from disk, `(stem, text)`, overlaid at
    /// `maps/mp/gametypes/<stem>` and run as gametype `<stem>`.
    pub gametype_script: Option<(String, String)>,
    pub test_entities: usize,
    pub bots: usize,
    pub bots_shoot: bool,
    /// Retail's `+set name value`s, in order, set before the scripts load.
    pub sets: Vec<(String, String)>,
    pub ban_file: Option<PathBuf>,
    pub trace: bool,
    /// A listen server: a loopback peer is the local client
    /// ([`Server::set_listen`]).
    pub listen: bool,
}

/// `sv_fps 20`.
const FRAME: Duration = Duration::from_millis(50);

/// How far behind the schedule one tick still catches up before the loop
/// resynchronises instead: a dedicated `Com_Frame` hands `SV_Frame` at
/// most 5000 ms (cod_lnxded 0x806d0c5).
const MAX_CATCH_UP: Duration = Duration::from_secs(5);

/// Without a bound a flood keeps the socket readable, `tick` never runs and
/// the outbox is never flushed.
const MAX_PACKETS_PER_FRAME: usize = 256;

/// Packets the reader thread holds for the tick; past this it blocks and the
/// socket's own buffer drops the excess.
const PACKET_QUEUE: usize = 4 * MAX_PACKETS_PER_FRAME;

/// Binds the server's UDP port on every interface.
pub fn bind(port: u16) -> Result<UdpSocket> {
    UdpSocket::bind(("0.0.0.0", port)).with_context(|| format!("binding udp/{port}"))
}

/// Loads the map and the scripts, then runs frames on `sock` until a
/// `quit`. `console` feeds console lines (`map`, `quit`) into the frame
/// loop; a closed channel reads as `quit`. An error is what retail's
/// `Com_Error` ends the process for: no map, a failed script load.
pub fn run(cfg: HostConfig, sock: UdpSocket, console: Receiver<String>) -> Result<()> {
    if cfg.test_entities > crate::world::MAX_TEST_ENTITIES {
        bail!(
            "--test-entities {} exceeds {}, the most that fits below ENTITYNUM_WORLD",
            cfg.test_entities,
            crate::world::MAX_TEST_ENTITIES
        );
    }
    if cfg.bots > cfg.max_clients {
        bail!(
            "--bots {} exceeds --max-clients {}; each bot takes a real slot",
            cfg.bots,
            cfg.max_clients
        );
    }
    let dir = cfg.game_dir.join(&cfg.mod_dir);
    // `fs_game` layers that directory over the base, as retail's `+set
    // fs_game` does; the last one wins.
    let fs_game = cfg
        .sets
        .iter()
        .filter(|(n, _)| n.eq_ignore_ascii_case("fs_game"))
        .map(|(_, v)| v.as_str())
        .next_back()
        .filter(|v| !v.is_empty());
    let game = fs_game.map(|g| cfg.game_dir.join(g));
    let fs = std::rc::Rc::new(
        Pk3Fs::open_layered(&dir, game.as_deref())
            .with_context(|| format!("opening game data in {}", dir.display()))?,
    );
    let Some(bsp_path) = fs.resolve_map(&cfg.map) else {
        bail!("map {} not found in {}", cfg.map, dir.display());
    };
    // A corrupt map fails here rather than at first use.
    let bsp_bytes = fs.read(&bsp_path).context("reading the bsp")?;
    let bsp = vcod_common::bsp::parse(&bsp_bytes).context("parsing the bsp")?;
    let gametype = cfg
        .gametype_script
        .as_ref()
        .map_or(cfg.gametype.clone(), |(s, _)| s.clone());

    // A thread reads the socket as packets arrive so each carries its arrival
    // time into the tick (`Server::handle_packet_at`); the tick still handles
    // them in arrival order at its start.
    let (packet_tx, packets) = std::sync::mpsc::sync_channel(PACKET_QUEUE);
    {
        let sock = sock.try_clone().context("cloning the socket")?;
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 65536];
            loop {
                match sock.recv_from(&mut buf) {
                    Ok((n, from)) => {
                        if packet_tx
                            .send((Instant::now(), from, buf[..n].to_vec()))
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(e) => log::debug!("recv: {e}"),
                }
            }
        });
    }
    log::info!("vcod-server: {} on udp/{}", cfg.map, cfg.port);
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let trace = cfg.trace;
    let mut server = Server::with_seed(
        ServerConfig {
            map: cfg.map,
            hostname: cfg.hostname,
            max_clients: cfg.max_clients,
            gametype,
            test_entities: cfg.test_entities,
            bots: cfg.bots,
            bots_shoot: cfg.bots_shoot,
            trace: cfg.trace,
        },
        Instant::now(),
        seed,
    );
    server.set_listen(cfg.listen);
    server.build_nav_in_background(true);
    server.load_world(crate::world::World::from_bsp(&bsp, Some(&fs)));
    if let Some((stem, text)) = &cfg.gametype_script {
        server.overlay_script(&format!("maps/mp/gametypes/{stem}"), text);
    }
    // The engine cvars a retail server fills in from its command line and
    // host, for `cvarlist` and a query to read; `sets` still outrank them.
    let base = cfg.game_dir.display().to_string();
    for (name, value) in [
        ("net_port", cfg.port.to_string()),
        ("fs_basepath", base.clone()),
        ("fs_homepath", base),
    ] {
        server.set_cvar(name, &value);
    }
    if let Some(path) = cfg.ban_file {
        let bans = crate::bans::Bans::load(path.clone())
            .with_context(|| format!("reading {}", path.display()))?;
        server.set_bans(bans);
    }
    for (name, value) in &cfg.sets {
        server.set_cvar(name, value);
    }
    // A failed script load is fatal, not a warning. The configstring table is
    // mostly script output now, so a server that kept serving would hand
    // clients a gamestate with no team menu, no status icons and no shaders,
    // which reads as a protocol bug rather than a script failure.
    server
        .load_scripts(fs.clone())
        .context("loading the map and gametype scripts")?;
    let mut quit_queued = false;
    // A fixed schedule, not a sleep after each tick: `SV_Frame` runs one
    // game frame per `sv_fps` slice of wall time and catches up when a
    // frame overran, so `svs.time` tracks the wall clock. Sleeping the
    // remainder of each tick lets every overshoot accumulate, which read as
    // a serverTime 5-10% slow against a probe's wall clock under load. The
    // frames owed run in one tick with one round of snapshots, as
    // `SV_Frame`'s loop does (map-cycle doc, 4.7).
    let mut next = Instant::now();
    let mut stats = TickStats::default();
    loop {
        let now = Instant::now();
        let late = now.saturating_duration_since(next);
        for _ in 0..MAX_PACKETS_PER_FRAME {
            let Ok((arrived, from, pkt)) = packets.try_recv() else {
                break;
            };
            server.handle_packet_at(from, &pkt, now, arrived);
        }
        loop {
            match console.try_recv() {
                Ok(line) => server.push_console(&line),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !quit_queued {
                        quit_queued = true;
                        server.push_console("quit");
                    }
                    break;
                }
            }
        }
        // The console first, then the count: what it ran (a `map_restart`)
        // is owed by this tick's frames, as `Com_Frame` charges the time
        // `Cbuf_Execute` took to the next `SV_Frame` (0x806d003, 0x806d0e4).
        server.drain_console(now);
        let behind = Instant::now().saturating_duration_since(next);
        let frames = if behind > MAX_CATCH_UP {
            // A stall past the catch-up bound (a debugger, a suspend) is
            // dropped rather than replayed.
            next = Instant::now();
            1
        } else {
            1 + (behind.as_millis() / FRAME.as_millis()) as u32
        };
        server.tick_frames(now, frames);
        if trace {
            stats.add(now.elapsed(), late);
        }
        // A level load that failed with the level already torn down: there is
        // no script to end the level and no table for a client to pull.
        if let Some(e) = server.take_fatal() {
            return Err(e);
        }
        for (to, pkt) in server.take_outgoing() {
            if let Err(e) = sock.send_to(&pkt, to) {
                log::debug!("send to {to}: {e}");
            }
        }
        server.frame_sent(Instant::now());
        if server.quit_requested() {
            return Ok(());
        }
        next += FRAME * frames;
        if trace && stats.ticks == 20 {
            stats.log_and_reset();
        }
        let after = Instant::now();
        if next > after {
            std::thread::sleep(next - after);
        }
    }
}

/// `--trace`'s per-second tick summary: the slowest `Server::tick` and the
/// worst lag of a tick's start behind its slot on the fixed schedule.
#[derive(Default)]
struct TickStats {
    ticks: u32,
    sum: Duration,
    max: Duration,
    late: Duration,
}

impl TickStats {
    fn add(&mut self, took: Duration, late: Duration) {
        self.ticks += 1;
        self.sum += took;
        self.max = self.max.max(took);
        self.late = self.late.max(late);
    }

    fn log_and_reset(&mut self) {
        log::info!(
            "tick: mean {:.2} ms max {:.2} ms late {:.2} ms",
            self.sum.as_secs_f64() * 1000.0 / f64::from(self.ticks.max(1)),
            self.max.as_secs_f64() * 1000.0,
            self.late.as_secs_f64() * 1000.0
        );
        *self = TickStats::default();
    }
}
