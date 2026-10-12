//! `vcod-server`, a CoD 1.1 dedicated server. Answers browsers, accepts retail
//! clients, runs the stock gametype scripts on the gsc VM and sends
//! delta-compressed snapshots.

use anyhow::{Context, Result, bail};
use clap::Parser;
use std::sync::atomic::{AtomicBool, Ordering};
use vcod_server::host::{self, HostConfig};

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

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    let gametype_script = match &args.gametype_script {
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
    let mut sets = Vec::new();
    for pair in &args.set {
        let Some((name, value)) = pair.split_once('=') else {
            bail!("--set takes NAME=VALUE, got {pair:?}");
        };
        sets.push((name.to_string(), value.to_string()));
    }
    let sock = host::bind(args.port)?;
    // Ctrl-C runs `quit`, so the masters get their flatline; a second one
    // exits at once.
    let (console, lines) = std::sync::mpsc::channel();
    {
        let interrupted = AtomicBool::new(false);
        ctrlc::set_handler(move || {
            if interrupted.swap(true, Ordering::Relaxed) {
                std::process::exit(130);
            }
            let _ = console.send("quit".to_string());
        })
        .context("installing the Ctrl-C handler")?;
    }
    let cfg = HostConfig {
        map: args.map,
        game_dir: args.game_dir,
        mod_dir: args.mod_dir,
        port: args.port,
        hostname: args.hostname,
        max_clients: args.max_clients,
        gametype: args.gametype,
        gametype_script,
        test_entities: args.test_entities,
        bots: args.bots,
        bots_shoot: args.bots_shoot,
        sets,
        ban_file: args.ban_file,
        trace: args.trace,
        listen: false,
    };
    if let Err(e) = host::run(cfg, sock, lines) {
        log::error!("{e:#}");
        std::process::exit(1);
    }
    Ok(())
}
