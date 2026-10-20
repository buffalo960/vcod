//! Shooting bots on a 1.1 objective gametype (`sd` or `re`), one seeded
//! match per seed: counts the `logPrint` lines the stock scripts write for
//! plants, defuses, pickups, captures, round wins and kills. The tally
//! behind docs/research/bot-objectives.md section 9.
//! Run: COD_DIR=<install> cargo run --release -p vcod-server --example team_tally [seed...]
//! `MAPS=` a comma list (mp_carentan), `GAMETYPE=` (sd), `BOTS=` (8),
//! `TICKS=` (6000), `SET=name=value,...` cvars.

use std::rc::Rc;
use std::time::{Duration, Instant};

#[derive(Default, Debug, Clone, Copy)]
struct Tally {
    plant: u32,
    defuse: u32,
    pickup: u32,
    capture: u32,
    allies: u32,
    axis: u32,
    kills: u32,
}

impl Tally {
    fn add(&mut self, o: &Tally) {
        self.plant += o.plant;
        self.defuse += o.defuse;
        self.pickup += o.pickup;
        self.capture += o.capture;
        self.allies += o.allies;
        self.axis += o.axis;
        self.kills += o.kills;
    }

    fn line(&mut self, l: &str) {
        if l.contains("A;") {
            self.plant += l.contains(";bomb_plant") as u32;
            self.defuse += l.contains(";bomb_defuse") as u32;
            self.pickup += l.contains(";re_pickup") as u32;
            self.capture += l.contains(";re_capture") as u32;
        }
        self.allies += l.contains("W;allies") as u32;
        self.axis += l.contains("W;axis") as u32;
        self.kills += l.contains("K;") as u32;
    }
}

fn main() {
    let Some(fs) = vcod_common::testing::game_fs() else {
        eprintln!("no game fs (set COD_DIR)");
        return;
    };
    let fs = Rc::new(fs);
    let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.into());
    let gametype = env("GAMETYPE", "sd");
    let bots: usize = env("BOTS", "8").parse().expect("BOTS");
    let ticks: usize = env("TICKS", "6000").parse().expect("TICKS");
    let mut seeds: Vec<u64> = std::env::args()
        .skip(1)
        .map(|s| s.parse().unwrap())
        .collect();
    if seeds.is_empty() {
        seeds = vec![1, 3, 5, 7];
    }
    let mut all = Tally::default();
    for map in env("MAPS", "mp_carentan").split(',') {
        let bsp =
            vcod_common::bsp::parse(&fs.read(&fs.resolve_map(map).unwrap()).unwrap()).unwrap();
        let mut sum = Tally::default();
        for &seed in &seeds {
            let mut now = Instant::now();
            let cfg = vcod_server::ServerConfig {
                map: map.into(),
                hostname: "tally".into(),
                max_clients: bots.max(8),
                gametype: gametype.clone(),
                test_entities: 0,
                trace: false,
                bots,
                bots_shoot: true,
            };
            let mut sv = vcod_server::Server::new(cfg, now);
            sv.test_seed_rng(seed);
            for kv in env("SET", "").split(',').filter(|s| !s.is_empty()) {
                let (k, v) = kv.split_once('=').expect("SET name=value");
                sv.set_cvar(k, v);
            }
            sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
            sv.load_scripts(fs.clone()).expect("load the scripts");
            let mut t = Tally::default();
            let (mut seen, mut last) = (0usize, String::new());
            for _ in 0..ticks {
                now += Duration::from_millis(50);
                sv.tick(now);
                let log = sv.script_log();
                // A restart starts a fresh log.
                if log.len() < seen || (seen > 0 && log[seen - 1] != last) {
                    seen = 0;
                }
                for l in &log[seen..] {
                    t.line(l);
                }
                seen = log.len();
                last = log.last().cloned().unwrap_or_default();
            }
            println!("{map} {gametype} seed {seed}: {t:?}");
            sum.add(&t);
        }
        println!("{map} {gametype} total: {sum:?}");
        all.add(&sum);
    }
    println!("all {gametype}: {all:?}");
}
