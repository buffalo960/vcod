//! Seeded bot-vs-bot fights (shoot on) per map: the kills and deaths of
//! each team off the script log's `K;` lines, and the mean life of each
//! team's bots, with the retreat (`bots/retreat.rs`) on for the bots
//! `RETREAT=` names: `all` (the default), `none`, `allies` or `axis`.
//! One side on and the other off is the measure: in a symmetric match
//! every kill is someone's death. Behind docs/research/bot-combat.md
//! section 4.
//! Run: COD_DIR=<install> cargo run --release -p vcod-server --example bot_fights [seed...]
//! `MAPS=` a comma list (mp_carentan,mp_harbor,mp_depot,mp_brecourt),
//! `GAMETYPE=` (tdm), `BOTS=` (8), `TICKS=` (6000, 5 min), `UO=1` a United
//! Offensive server.

use std::rc::Rc;
use std::time::{Duration, Instant};

#[derive(Default)]
struct Side {
    kills: usize,
    deaths: usize,
    /// Suicides and deaths with no enemy attacker.
    selfs: usize,
    life_ticks: usize,
    lives: usize,
    /// Retreats started, hurt and dry.
    hurt: usize,
    dry: usize,
    covered: usize,
    /// Wounds (health first under 50 in a fight this life) and those the
    /// bot died of within 5 s.
    wounds: usize,
    wound_deaths: usize,
}

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
    let maps = std::env::var("MAPS")
        .unwrap_or_else(|_| "mp_carentan,mp_harbor,mp_depot,mp_brecourt".into());
    let ticks: usize = std::env::var("TICKS").map_or(6000, |t| t.parse().expect("TICKS"));
    let gametype = std::env::var("GAMETYPE").unwrap_or_else(|_| "tdm".into());
    let bots: usize = std::env::var("BOTS").map_or(8, |t| t.parse().expect("BOTS"));
    let retreat = std::env::var("RETREAT").unwrap_or_else(|_| "all".into());
    let mut seeds: Vec<u64> = std::env::args()
        .skip(1)
        .map(|s| s.parse().unwrap())
        .collect();
    if seeds.is_empty() {
        seeds = (1..12).step_by(2).collect();
    }
    // [allies, axis]
    let mut total: [Side; 2] = Default::default();
    for map in maps.split(',') {
        let bsp =
            vcod_common::bsp::parse(&fs.read(&fs.resolve_map(map).unwrap()).unwrap()).unwrap();
        let mut per_map: [Side; 2] = Default::default();
        for &seed in &seeds {
            let mut now = Instant::now();
            let cfg = vcod_server::ServerConfig {
                map: map.to_string(),
                hostname: "fights".into(),
                max_clients: bots.max(8),
                gametype: gametype.clone(),
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
            let mut side: [Side; 2] = Default::default();
            let mut alive = [0usize; 64];
            let mut was = [0u8; 64];
            // Ticks since this life's wound, if any.
            let mut wound: [Option<usize>; 64] = [None; 64];
            for _ in 0..ticks {
                now += Duration::from_millis(50);
                sv.tick(now);
                for slot in sv.bot_slots() {
                    // 1 axis, 2 allies (`script::TEAM_*`).
                    let team = sv.bot_team(slot);
                    let k = match team {
                        2 => 0,
                        1 => 1,
                        _ => continue,
                    };
                    let on = match retreat.as_str() {
                        "all" => true,
                        "none" => false,
                        "allies" => k == 0,
                        "axis" => k == 1,
                        r => panic!("RETREAT={r}"),
                    };
                    sv.test_bot_retreat(slot, on);
                    let body = sv.bot_body(slot);
                    let r = body.as_ref().map_or(0, |b| b.retreating);
                    match (was[slot], r) {
                        (0, 1) => side[k].hurt += 1,
                        (0, 2) => side[k].dry += 1,
                        (1 | 2, 3) => side[k].covered += 1,
                        _ => {}
                    }
                    was[slot] = r;
                    let playing = body.as_ref().is_some_and(|b| b.playing);
                    if let Some(b) = body.as_ref().filter(|b| b.playing) {
                        if wound[slot].is_none() && b.engaged && b.health < 50 {
                            wound[slot] = Some(0);
                            side[k].wounds += 1;
                        } else if let Some(t) = wound[slot].as_mut() {
                            *t += 1;
                        }
                    } else if body.as_ref().is_some_and(|b| b.dead) {
                        if wound[slot].is_some_and(|t| t < 100) {
                            side[k].wound_deaths += 1;
                        }
                        wound[slot] = None;
                    }
                    if playing {
                        alive[slot] += 1;
                    } else if alive[slot] > 0 {
                        side[k].life_ticks += alive[slot];
                        side[k].lives += 1;
                        alive[slot] = 0;
                    }
                }
            }
            // `K;` victim num, team, name, attacker num, team, name, weapon,
            // damage, means of death, hit location; 1.5 and UO put a guid
            // ahead of each num.
            for l in sv.script_log() {
                let Some((_, k)) = l.trim_end().split_once("K;") else {
                    continue;
                };
                let f: Vec<&str> = k.split(';').collect();
                let team = |t: &str| match t {
                    "allies" => Some(0),
                    "axis" => Some(1),
                    _ => None,
                };
                let g = usize::from(f.len() >= 12);
                let Some(v) = team(f[1 + g]) else {
                    continue;
                };
                side[v].deaths += 1;
                match team(f[4 + 2 * g]) {
                    Some(a) if a != v => side[a].kills += 1,
                    _ => side[v].selfs += 1,
                }
            }
            let line = |s: &Side| {
                format!(
                    "k {:3} d {:3} self {:2} life {:5.1}s retreats {}/{}",
                    s.kills,
                    s.deaths,
                    s.selfs,
                    s.life_ticks as f32 / 20.0 / s.lives.max(1) as f32,
                    s.hurt,
                    s.dry
                )
            };
            println!(
                "{map} seed {seed}: allies {} | axis {}",
                line(&side[0]),
                line(&side[1])
            );
            for (t, s) in per_map.iter_mut().zip(&side) {
                add(t, s);
            }
        }
        report(map, &per_map);
        for (t, s) in total.iter_mut().zip(&per_map) {
            add(t, s);
        }
    }
    report("total", &total);
}

fn add(t: &mut Side, s: &Side) {
    t.kills += s.kills;
    t.deaths += s.deaths;
    t.selfs += s.selfs;
    t.life_ticks += s.life_ticks;
    t.lives += s.lives;
    t.hurt += s.hurt;
    t.dry += s.dry;
    t.covered += s.covered;
    t.wounds += s.wounds;
    t.wound_deaths += s.wound_deaths;
}

fn report(what: &str, s: &[Side; 2]) {
    for (name, s) in ["allies", "axis"].iter().zip(s) {
        println!(
            "{what} {name}: kills {} deaths {} self {} k/d {:.2} mean life {:.1}s retreats hurt {} dry {} covered {} wounds {} dead in 5 s {:.0}%",
            s.kills,
            s.deaths,
            s.selfs,
            s.kills as f32 / s.deaths.max(1) as f32,
            s.life_ticks as f32 / 20.0 / s.lives.max(1) as f32,
            s.hurt,
            s.dry,
            s.covered,
            s.wounds,
            100.0 * s.wound_deaths as f32 / s.wounds.max(1) as f32
        );
    }
}
