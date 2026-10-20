//! Bots and United Offensive vehicles: per seed, one game with
//! `vc_bot_vehicles 0` and one with 1, each printing the kills, those off a
//! vehicle's guns, the boardings, the bot-ticks spent seated, and how far
//! the driven vehicles went. The measurement behind
//! docs/research/bot-objectives.md section 9.
//! Run: COD_DIR=<UO install> cargo run --release -p vcod-server --example bot_vehicles [seed...]
//! `MAP=` picks the map (mp_cassino), `GAMETYPE=` the gametype (tdm),
//! `BOTS=` how many (6), `TICKS=` the length (3600, 3 min).

use std::rc::Rc;
use std::time::{Duration, Instant};

#[derive(Default)]
struct Tally {
    kills: usize,
    gun_kills: usize,
    boardings: u32,
    seated: usize,
    driven: f32,
}

fn game(
    fs: &Rc<vcod_common::pk3::Pk3Fs>,
    bsp: &vcod_common::bsp::Bsp,
    seed: u64,
    on: bool,
) -> Tally {
    let map = std::env::var("MAP").unwrap_or_else(|_| "mp_cassino".into());
    let gametype = std::env::var("GAMETYPE").unwrap_or_else(|_| "tdm".into());
    let bots: usize = std::env::var("BOTS").map_or(6, |t| t.parse().expect("BOTS"));
    let ticks: usize = std::env::var("TICKS").map_or(3600, |t| t.parse().expect("TICKS"));
    let mut now = Instant::now();
    let cfg = vcod_server::ServerConfig {
        map,
        hostname: "vehicles".into(),
        max_clients: 8,
        gametype,
        test_entities: 0,
        trace: false,
        bots,
        bots_shoot: true,
    };
    let mut sv = vcod_server::Server::new(cfg, now);
    sv.set_protocol(&vcod_common::net::protocol::PROTOCOL_V22);
    sv.test_seed_rng(seed);
    sv.set_cvar("vc_bot_vehicles", if on { "1" } else { "0" });
    sv.load_world(vcod_server::world::World::from_bsp(bsp, Some(fs)));
    sv.load_scripts(fs.clone()).expect("load the scripts");
    let mut t = Tally::default();
    let mut last: std::collections::HashMap<u32, [f32; 3]> = Default::default();
    for _ in 0..ticks {
        now += Duration::from_millis(50);
        sv.tick(now);
        let mut driven = Vec::new();
        for slot in sv.bot_slots() {
            if let Some((v, seat)) = sv.bot_body(slot).and_then(|b| b.seat) {
                t.seated += 1;
                if seat == 1 {
                    driven.push(v);
                }
            }
        }
        for v in sv.test_vehicles() {
            if let Some(p) = last.insert(v.num, v.origin)
                && driven.contains(&v.num)
            {
                t.driven += (v.origin[0] - p[0]).hypot(v.origin[1] - p[1]);
            }
        }
    }
    for slot in sv.bot_slots() {
        t.boardings += sv.bot_body(slot).map_or(0, |b| b.boardings);
    }
    for k in sv
        .script_log()
        .iter()
        .filter_map(|l| l.trim_end().split_once("K;").map(|(_, k)| k))
    {
        let k: Vec<&str> = k.split(';').collect();
        t.kills += 1;
        let w = k.get(8).copied().unwrap_or("");
        if w.contains("turret") || w.contains("gunner") {
            t.gun_kills += 1;
        }
    }
    t
}

fn main() {
    let Some(fs) = vcod_common::testing::uo_fs() else {
        eprintln!("no UO install (set COD_DIR)");
        return;
    };
    let fs = Rc::new(fs);
    let map = std::env::var("MAP").unwrap_or_else(|_| "mp_cassino".into());
    let bsp = vcod_common::bsp::parse(&fs.read(&fs.resolve_map(&map).unwrap()).unwrap()).unwrap();
    let mut seeds: Vec<u64> = std::env::args()
        .skip(1)
        .map(|s| s.parse().unwrap())
        .collect();
    if seeds.is_empty() {
        seeds = vec![1, 3, 5, 7];
    }
    println!("seed vehicles kills gun_kills boardings seated_ticks driven");
    for seed in seeds {
        for on in [false, true] {
            let t = game(&fs, &bsp, seed, on);
            println!(
                "{seed} {} {} {} {} {} {:.0}",
                u8::from(on),
                t.kills,
                t.gun_kills,
                t.boardings,
                t.seated,
                t.driven
            );
        }
    }
}
