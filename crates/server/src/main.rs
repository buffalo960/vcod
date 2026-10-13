//! `vcod-server`, a CoD 1.1 dedicated server. Answers browsers, accepts retail
//! clients, runs the stock gametype scripts on the gsc VM and sends
//! delta-compressed snapshots.

use anyhow::{Context, Result, bail};
use clap::Parser;
use std::net::UdpSocket;
use std::time::{Duration, Instant};
use vcod_common::net::protocol::{PROTOCOL_V1, PROTOCOL_V6, Protocol};
use vcod_common::pk3::Pk3Fs;
use vcod_server::{Server, ServerConfig};

#[derive(Parser)]
#[command(about = "Call of Duty (2003) 1.1 dedicated server, in progress")]
struct Args {
    /// Map name, e.g. mp_carentan
    map: String,
    /// Game install; defaults to $COD_DIR, else the executable's directory
    #[arg(long, default_value_os_t = vcod_common::game_dir::default_game_dir())]
    game_dir: std::path::PathBuf,
    /// Pk3 subdirectory, main for CoD1 or uo for United Offensive
    #[arg(long, default_value = "main")]
    mod_dir: String,
    /// UDP port
    #[arg(long, default_value_t = 28960)]
    port: u16,
    /// sv_hostname
    #[arg(long, default_value = "vcod")]
    hostname: String,
    /// sv_maxclients
    #[arg(long, default_value_t = 8)]
    max_clients: usize,
    /// g_gametype: the script under maps/mp/gametypes to run
    #[arg(long, default_value = "dm")]
    gametype: String,
    /// A gametype script from disk instead of the paks: overlaid at
    /// maps/mp/gametypes/<stem> and run as gametype <stem>. For the client
    /// probes in crates/gsc/tests/fixtures/semantics/client-probes/.
    #[arg(long)]
    gametype_script: Option<std::path::PathBuf>,
    /// Scripted entities that exercise the packet-entity wire path. 0 is off.
    #[arg(long, default_value_t = 0)]
    test_entities: usize,
    /// Debug bots in play. Each takes a real client slot; 0 is off.
    #[arg(long, default_value_t = 0)]
    bots: usize,
    /// Whether the bots fight. Without it they only roam.
    #[arg(long)]
    bots_shoot: bool,
    /// A cvar to set before the scripts load, retail's `+set name value`;
    /// repeatable, e.g. `--set scr_friendlyfire=1`.
    #[arg(long = "set", value_name = "NAME=VALUE")]
    set: Vec<String>,
    /// File that keeps `banUser`/`banClient` bans across runs, one IPv4
    /// address per line. Without it a ban lasts until the process exits.
    #[arg(long)]
    ban_file: Option<std::path::PathBuf>,

    /// Log one line per snapshot per client: send interval, the serverTime and
    /// commandTime a client predicts from, the usercmds consumed, and whether
    /// the frame went out as a delta. Also one `tick:` line per second: the
    /// slowest tick and how far the loop ran behind its schedule.
    #[arg(long)]
    trace: bool,
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

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    if args.test_entities > vcod_server::world::MAX_TEST_ENTITIES {
        bail!(
            "--test-entities {} exceeds {}, the most that fits below ENTITYNUM_WORLD",
            args.test_entities,
            vcod_server::world::MAX_TEST_ENTITIES
        );
    }
    if args.bots > args.max_clients {
        bail!(
            "--bots {} exceeds --max-clients {}; each bot takes a real slot",
            args.bots,
            args.max_clients
        );
    }
    let dir = args.game_dir.join(&args.mod_dir);
    // `--set fs_game=<dir>` layers that directory over the base, as
    // retail's `+set fs_game` does; the last one wins, as `--set` does.
    let fs_game = args
        .set
        .iter()
        .filter_map(|p| p.split_once('='))
        .filter(|(n, _)| n.eq_ignore_ascii_case("fs_game"))
        .map(|(_, v)| v)
        .next_back()
        .filter(|v| !v.is_empty());
    let game = fs_game.map(|g| args.game_dir.join(g));
    let fs = std::rc::Rc::new(
        Pk3Fs::open_layered(&dir, game.as_deref())
            .with_context(|| format!("opening game data in {}", dir.display()))?,
    );
    let Some(bsp_path) = fs.resolve_map(&args.map) else {
        bail!("map {} not found in {}", args.map, dir.display());
    };
    // A corrupt map fails here rather than at first use.
    let bsp_bytes = fs.read(&bsp_path).context("reading the bsp")?;
    let bsp = vcod_common::bsp::parse(&bsp_bytes).context("parsing the bsp")?;

    let overlay = match &args.gametype_script {
        Some(path) => {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .context("--gametype-script needs a file name")?
                .to_string();
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            Some((stem, text))
        }
        None => None,
    };
    let gametype = overlay
        .as_ref()
        .map_or(args.gametype.clone(), |(s, _)| s.clone());

    let sock = UdpSocket::bind(("0.0.0.0", args.port))
        .with_context(|| format!("binding udp/{}", args.port))?;
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
    log::info!("vcod-server: {} on udp/{}", args.map, args.port);
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let protocol = protocol(&args.set, &fs)?;
    log::info!(
        "speaking protocol {} (patch {})",
        protocol.version,
        protocol.short_version
    );
    let trace = args.trace;
    let mut server = Server::with_seed(
        ServerConfig {
            map: args.map,
            hostname: args.hostname,
            max_clients: args.max_clients,
            gametype,
            test_entities: args.test_entities,
            bots: args.bots,
            bots_shoot: args.bots_shoot,
            trace: args.trace,
        },
        Instant::now(),
        seed,
    );
    server.set_protocol(protocol);
    server.build_nav_in_background(true);
    server.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    if let Some((stem, text)) = &overlay {
        server.overlay_script(&format!("maps/mp/gametypes/{stem}"), text);
    }
    // The engine cvars a retail server fills in from its command line and
    // host, for `cvarlist` and a query to read; `--set` still outranks them.
    let base = args.game_dir.display().to_string();
    for (name, value) in [
        ("net_port", args.port.to_string()),
        ("fs_basepath", base.clone()),
        ("fs_homepath", base),
    ] {
        server.set_cvar(name, &value);
    }
    if let Some(path) = args.ban_file {
        let bans = vcod_server::bans::Bans::load(path.clone())
            .with_context(|| format!("reading {}", path.display()))?;
        server.set_bans(bans);
    }
    for pair in &args.set {
        let Some((name, value)) = pair.split_once('=') else {
            bail!("--set takes NAME=VALUE, got {pair:?}");
        };
        server.set_cvar(name, value);
    }
    // A failed script load is fatal, not a warning. The configstring table is
    // mostly script output now, so a server that kept serving would hand
    // clients a gamestate with no team menu, no status icons and no shaders,
    // which reads as a protocol bug rather than a script failure.
    if let Err(e) = server.load_scripts(fs.clone()) {
        log::error!("loading the map and gametype scripts: {e:#}");
        std::process::exit(1);
    }
    // Ctrl-C runs `quit`, so the masters get their flatline; a second one
    // exits at once.
    let interrupted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let flag = interrupted.clone();
        ctrlc::set_handler(move || {
            if flag.swap(true, std::sync::atomic::Ordering::Relaxed) {
                std::process::exit(130);
            }
        })
        .context("installing the Ctrl-C handler")?;
    }
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
        if interrupted.load(std::sync::atomic::Ordering::Relaxed) && !quit_queued {
            quit_queued = true;
            server.push_console("quit");
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
        // no script to end the level and no table for a client to pull, which
        // is what retail's `Com_Error` ends the process for.
        if let Some(e) = server.take_fatal() {
            log::error!("{e:#}");
            std::process::exit(1);
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

/// The protocol to host: `--set protocol=<1|6>`, else the install's: 6 when
/// its `pak0` is 1.5's, whose checksum differs from 1.1's
/// (docs/research/cod15-protocol-diff.md 10), 1 otherwise. One per process,
/// as each retail binary speaks one.
fn protocol(sets: &[String], fs: &Pk3Fs) -> Result<&'static Protocol> {
    /// `FS_LoadZipFile`'s checksum of the 1.5 install's `main/pak0.pk3`,
    /// as the 1.5b server's `sv_referencedPaks` lists it.
    const PAK0_V6_CHECKSUM: i32 = 1_048_127_331;
    let asked = sets
        .iter()
        .filter_map(|p| p.split_once('='))
        .filter(|(n, _)| n.eq_ignore_ascii_case("protocol"))
        .map(|(_, v)| v)
        .next_back();
    if let Some(v) = asked {
        return v
            .trim()
            .parse()
            .ok()
            .and_then(Protocol::by_version)
            .with_context(|| format!("--set protocol={v}: vcod speaks 1 (1.1) and 6 (1.5)"));
    }
    let v6 = fs
        .paks()
        .any(|p| p.game == "main" && p.name == "pak0" && p.checksum == PAK0_V6_CHECKSUM);
    Ok(if v6 { &PROTOCOL_V6 } else { &PROTOCOL_V1 })
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
