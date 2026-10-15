//! Builds the bot navigation graph on stock maps and prints its size, its
//! build time and how many spawn points reach each other through it.
//! Run: COD_DIR=<install> cargo run --release -p vcod-server --example nav_build [map...]
//! `UO=1` reads United Offensive's `uo/` over `main/` and proves the walks
//! under UO's movement, as a UO server builds the graph; `RULES=1.1`,
//! `1.5` or `uo` picks the movement on its own. `NEAR=x,y,r` lists the
//! nodes within `r` across of `(x, y)`, with their edge counts, and
//! `ROUTE=x,y,z,x,y,z` the route between the nodes nearest two points.
//! `UNLINK=n,...` builds with those submodels out, as a script's
//! `delete` leaves them.

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
        // As a server's build: without the brushes `bas.gsc` lifts away.
        for &m in &world.rubble {
            world.collision.set_model_linked(m, false);
        }
        for m in std::env::var("UNLINK")
            .unwrap_or_default()
            .split(',')
            .filter_map(|m| m.parse().ok())
        {
            world.collision.set_model_linked(m, false);
        }
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
        if let Ok(route) = std::env::var("ROUTE") {
            let v: Vec<f32> = route
                .split(',')
                .map(|x| x.parse().expect("ROUTE"))
                .collect();
            let (a, b) = (g.nearest([v[0], v[1], v[2]]), g.nearest([v[3], v[4], v[5]]));
            let path = a.zip(b).and_then(|(a, b)| g.path(a, b));
            println!(
                "  route {a:?} -> {b:?}: {:?}",
                path.map(|p| p
                    .iter()
                    .map(|n| g.nodes[*n as usize].round())
                    .collect::<Vec<_>>())
            );
        }
        if let Ok(near) = std::env::var("NEAR") {
            let v: Vec<f32> = near.split(',').map(|x| x.parse().expect("NEAR")).collect();
            for (i, n) in g.nodes.iter().enumerate() {
                if (n.x - v[0]).hypot(n.y - v[1]) < v[2] {
                    println!("  node {i} {:?} edges {}", n.round(), g.edges[i].len());
                }
            }
        }
    }
}
