//! Cover against no cover: a seeded `tdm` of fighting bots in which one team
//! fights from cover (`bots::cover`) and the other does not, the sides
//! swapped every other seed so the map's own asymmetry cancels. Prints per
//! seed and in total each side's deaths (the other side's kills, frags on
//! one's own side aside), the bot-ticks holding a cover spot and leaning.
//! Behind the tables in docs/research/bot-combat.md.
//! Run: COD_DIR=<install> cargo run --release -p vcod-server --example cover_ab [seed...]
//! `MAP=` (mp_carentan), `BOTS=` (8), `TICKS=` (6000, five minutes), `UO=1`
//! for a United Offensive server, `OFF=1` turns cover off on both sides
//! (the baseline: the two columns then differ only by the map and seed).
use std::rc::Rc;
use std::time::{Duration, Instant};

fn main() {
    let uo = std::env::var("UO").is_ok_and(|v| v == "1");
    let fs = if uo {
        vcod_common::testing::uo_fs()
    } else {
        vcod_common::testing::game_fs()
    };
    let Some(fs) = fs else {
        eprintln!("no game fs (set COD_DIR)");
        return;
    };
    let fs = Rc::new(fs);
    let env = |k: &str, d: usize| std::env::var(k).map_or(d, |v| v.parse().expect(k));
    let map = std::env::var("MAP").unwrap_or_else(|_| "mp_carentan".into());
    let ticks = env("TICKS", 6000);
    let bots = env("BOTS", 8);
    let off = std::env::var("OFF").is_ok_and(|v| v == "1");
    let mut seeds: Vec<u64> = std::env::args()
        .skip(1)
        .map(|s| s.parse().unwrap())
        .collect();
    if seeds.is_empty() {
        seeds = (1..=8).collect();
    }
    let bsp = vcod_common::bsp::parse(&fs.read(&fs.resolve_map(&map).unwrap()).unwrap()).unwrap();
    // [cover side, other side]: deaths, holding ticks, leaning ticks.
    let mut total = [[0usize; 3]; 2];
    for seed in seeds {
        let mut now = Instant::now();
        let cfg = vcod_server::ServerConfig {
            map: map.clone(),
            hostname: "cover".into(),
            max_clients: bots.max(8),
            gametype: "tdm".into(),
            test_entities: 0,
            trace: false,
            bots,
            bots_shoot: true,
        };
        let mut sv = vcod_server::Server::new(cfg, now);
        if uo {
            sv.set_protocol(&vcod_common::net::protocol::PROTOCOL_V22);
        }
        sv.test_seed_rng(seed);
        sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
        sv.load_scripts(fs.clone()).expect("load the scripts");
        let cover_team = if seed % 2 == 0 {
            vcod_server::game::script::TEAM_AXIS
        } else {
            vcod_server::game::script::TEAM_ALLIES
        };
        let mut side = [[0usize; 3]; 2];
        let mut dead = [false; 64];
        for _ in 0..ticks {
            now += Duration::from_millis(50);
            for slot in sv.bot_slots() {
                let team = sv.bot_team(slot);
                sv.test_bot_cover(slot, !off && team == cover_team);
            }
            sv.tick(now);
            for slot in sv.bot_slots() {
                let team = sv.bot_team(slot);
                if team != cover_team && team != 3 - cover_team {
                    continue;
                }
                let s = usize::from(team != cover_team);
                let d = sv.bot_body(slot).is_some_and(|b| b.dead);
                if d && !dead[slot] {
                    side[s][0] += 1;
                }
                dead[slot] = d;
                if let Some((hold, lean)) = sv.bot_cover_state(slot) {
                    side[s][1] += usize::from(hold);
                    side[s][2] += usize::from(lean != 0.0);
                }
            }
        }
        println!(
            "seed {seed}: cover side deaths {} (hold {} lean {}), other side deaths {} (hold {} lean {})",
            side[0][0], side[0][1], side[0][2], side[1][0], side[1][1], side[1][2]
        );
        for s in 0..2 {
            for k in 0..3 {
                total[s][k] += side[s][k];
            }
        }
    }
    println!(
        "total: cover side deaths {} (hold {} lean {}), other side deaths {} (hold {} lean {})",
        total[0][0], total[0][1], total[0][2], total[1][0], total[1][1], total[1][2]
    );
}
