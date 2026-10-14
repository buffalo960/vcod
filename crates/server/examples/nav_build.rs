//! Builds the bot navigation graph on stock maps and prints its size, its
//! build time and how many spawn points reach each other through it.
//! Run: COD_DIR=<install> cargo run --release -p vcod-server --example nav_build [map...]
//! `UO=1` reads United Offensive's `uo/` over `main/` and proves the walks
//! under UO's movement, as a UO server builds the graph; `RULES=1.1`,
//! `1.5` or `uo` picks the movement on its own.

use std::time::Instant;
use vcod_common::pmove::MoveRules;
use vcod_server::nav::{NavGraph, spawn_points};

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
    let rules = match std::env::var("RULES").as_deref() {
        Ok("1.1") => MoveRules::V1_1,
        Ok("1.5") => MoveRules::V1_5,
        Ok("uo") => MoveRules::Uo,
        _ if uo => MoveRules::Uo,
        _ => MoveRules::V1_1,
    };
    let mut maps: Vec<String> = std::env::args().skip(1).collect();
    if maps.is_empty() {
        maps = fs
            .find_maps()
            .into_iter()
            .filter(|m| m.starts_with("mp_"))
            .collect();
    }
    for map in maps {
        let Some(entry) = fs.resolve_map(&map) else {
            eprintln!("{map}: not found");
            continue;
        };
        let bsp = vcod_common::bsp::parse(&fs.read(&entry).unwrap()).unwrap();
        let world = vcod_server::world::World::from_bsp(&bsp, Some(&fs));
        let seeds = spawn_points(&bsp.entities);
        let t = Instant::now();
        let g = NavGraph::build(&world.collision, &seeds, &world.hazards, rules);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let on = seeds.iter().filter(|s| g.nearest(**s).is_some()).count();
        let (reach, leave) = g.spawn_census(&world.collision, &seeds);
        println!(
            "{map}: spacing {}, {} nodes, {} edges, {ms:.0} ms, spawns {}/{} on graph, {reach} in one component, {leave} reach it, {} leaps",
            g.spacing,
            g.len(),
            g.edge_count(),
            on,
            seeds.len(),
            g.leaps.len(),
        );
    }
}
