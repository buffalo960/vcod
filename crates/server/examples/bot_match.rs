//! Seeded bot-vs-bot matches: the measuring stick for bot changes
//! (docs/research/bot-combat.md). Each seed runs one match with the bots'
//! fire on until the map ends (`exitLevel`) or `TICKS` run out, and
//! reads the script's `logPrint` lines: kills, deaths, damage, the time
//! from a bot's first hit on a victim to the kill, objective actions
//! (`A;`), wins (`W;`: a round's in `sd` and `re`, the match's elsewhere)
//! and the team scores. It also counts stalled bot-ticks as
//! `ship_stalls` does (12 s within 150 units).
//! Run: COD_DIR=<install> cargo run --release -p vcod-server --example bot_match [seed...]
//! `MAP=` (mp_carentan), `GAMETYPE=` (tdm), `BOTS=` (6), `TICKS=` (12000,
//! ten minutes), `JOBS=` (4, seeds run in parallel), `SET=name=value,...`
//! cvars, `UO=1` a United Offensive server. `AXIS_SKILL=` / `ALLIES_SKILL=`
//! add the bots per side at those `vc_bot_skill` levels, half each (dm:
//! `SKILLS=a,b,...`, one bot per entry); without them `BOTS` join by
//! balance at `vc_bot_skill`. `VERBOSE=1` prints every kill. Seeds default
//! to 1..=8.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::rc::Rc;
use std::time::{Duration, Instant};

const TICK_MS: u64 = 50;
const WINDOW: usize = 240;
const RADIUS: f32 = 150.0;

#[derive(Clone)]
struct Opts {
    map: String,
    gametype: String,
    bots: usize,
    ticks: usize,
    uo: bool,
    set: Vec<(String, String)>,
    /// Console lines run once the level is up (the per-side `addbot`s).
    console: Vec<String>,
    verbose: bool,
}

/// One side of the tally: a team, or a skill level.
#[derive(Default, Clone)]
struct Tally {
    kills: u32,
    deaths: u32,
    suicides: u32,
    team_kills: u32,
    damage: u64,
    ttk_ms: Vec<u64>,
}

impl Tally {
    fn add(&mut self, o: &Tally) {
        self.kills += o.kills;
        self.deaths += o.deaths;
        self.suicides += o.suicides;
        self.team_kills += o.team_kills;
        self.damage += o.damage;
        self.ttk_ms.extend(&o.ttk_ms);
    }

    fn line(&self) -> String {
        let mut t = self.ttk_ms.clone();
        t.sort_unstable();
        let mean = if t.is_empty() {
            0
        } else {
            t.iter().sum::<u64>() / t.len() as u64
        };
        let median = t.get(t.len() / 2).copied().unwrap_or(0);
        format!(
            "kills {:4} deaths {:4} k/d {:5.2} suicides {:3} teamkills {:3} damage {:6} ttk mean {:5} median {:5} ms",
            self.kills,
            self.deaths,
            self.kills as f32 / self.deaths.max(1) as f32,
            self.suicides,
            self.team_kills,
            self.damage,
            mean,
            median,
        )
    }
}

#[derive(Default, Clone)]
struct Match {
    seed: u64,
    ticks: usize,
    /// Whether the map ended inside `TICKS`.
    ended: bool,
    /// `W;` lines by winning team (or, in `dm`, player name).
    wins: BTreeMap<String, u32>,
    /// [axis, allies] when the match ended or ran out.
    scores: [i32; 2],
    by_team: BTreeMap<String, Tally>,
    by_skill: BTreeMap<String, Tally>,
    actions: BTreeMap<String, u32>,
    stalled: u32,
    log: Vec<String>,
}

fn main() {
    let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.into());
    let uo = env("UO", "0") == "1";
    let gametype = env("GAMETYPE", "tdm");
    let mut bots: usize = env("BOTS", "6").parse().expect("BOTS");
    let mut console = Vec::new();
    let side_skill = (std::env::var("AXIS_SKILL"), std::env::var("ALLIES_SKILL"));
    if let Ok(list) = std::env::var("SKILLS") {
        let list: Vec<&str> = list.split(',').collect();
        for s in &list {
            console.push(format!("addbot 1 skill {s}"));
        }
        bots = list.len();
    } else if side_skill.0.is_ok() || side_skill.1.is_ok() {
        let axis = side_skill.0.unwrap_or_else(|_| "1".into());
        let allies = side_skill.1.unwrap_or_else(|_| "1".into());
        console.push(format!("addbot axis {} skill {axis}", bots - bots / 2));
        console.push(format!("addbot allies {} skill {allies}", bots / 2));
    }
    let opts = Opts {
        map: env("MAP", "mp_carentan"),
        gametype,
        bots,
        ticks: env("TICKS", "12000").parse().expect("TICKS"),
        uo,
        set: env("SET", "")
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|kv| {
                let (k, v) = kv.split_once('=').expect("SET name=value");
                (k.to_string(), v.to_string())
            })
            .collect(),
        console,
        verbose: env("VERBOSE", "0") == "1",
    };
    let jobs: usize = env("JOBS", "4").parse().expect("JOBS");
    let mut seeds: Vec<u64> = std::env::args()
        .skip(1)
        .map(|s| s.parse().expect("seed"))
        .collect();
    if seeds.is_empty() {
        seeds = (1..=8).collect();
    }
    println!(
        "map {} gametype {} bots {} ticks {} uo {} set {:?} console {:?}",
        opts.map, opts.gametype, opts.bots, opts.ticks, opts.uo, opts.set, opts.console
    );
    let started = Instant::now();
    let queue = std::sync::Mutex::new(seeds.clone());
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..jobs.max(1) {
            s.spawn(|| {
                loop {
                    let Some(seed) = queue.lock().unwrap().pop() else {
                        break;
                    };
                    let Some(m) = run(&opts, seed) else {
                        return;
                    };
                    eprintln!(
                        "seed {seed} done ({:.0} s)",
                        started.elapsed().as_secs_f32()
                    );
                    results.lock().unwrap().push(m);
                }
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    if results.is_empty() {
        eprintln!("no game fs (set COD_DIR)");
        return;
    }
    results.sort_by_key(|m| m.seed);
    let mut by_team: BTreeMap<String, Tally> = BTreeMap::new();
    let mut by_skill: BTreeMap<String, Tally> = BTreeMap::new();
    let mut actions: BTreeMap<String, u32> = BTreeMap::new();
    let mut wins: BTreeMap<String, u32> = BTreeMap::new();
    let (mut stalled, mut ended, mut scores) = (0, 0, [0i64; 2]);
    for m in &results {
        for l in &m.log {
            println!("  seed {} {l}", m.seed);
        }
        let all = m.by_team.values().fold(Tally::default(), |mut a, t| {
            a.add(t);
            a
        });
        println!(
            "seed {}: ticks {} ended {} wins {:?} score axis {} allies {} stalled {} {} actions {:?}",
            m.seed,
            m.ticks,
            m.ended,
            m.wins,
            m.scores[0],
            m.scores[1],
            m.stalled,
            all.line(),
            m.actions,
        );
        for (k, t) in &m.by_team {
            by_team.entry(k.clone()).or_default().add(t);
        }
        for (k, t) in &m.by_skill {
            by_skill.entry(k.clone()).or_default().add(t);
        }
        for (k, n) in &m.actions {
            *actions.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &m.wins {
            *wins.entry(k.clone()).or_default() += n;
        }
        ended += u32::from(m.ended);
        stalled += m.stalled;
        scores[0] += i64::from(m.scores[0]);
        scores[1] += i64::from(m.scores[1]);
    }
    println!("total over {} matches:", results.len());
    for (k, t) in &by_team {
        println!("  team {k:<8} {}", t.line());
    }
    for (k, t) in &by_skill {
        println!("  skill {k:<7} {}", t.line());
    }
    println!("  ended {ended}  wins {wins:?}");
    println!(
        "  score axis {} allies {}  actions {actions:?}  stalled {stalled}",
        scores[0], scores[1]
    );
}

/// One seeded match; `None` without game data.
fn run(o: &Opts, seed: u64) -> Option<Match> {
    let fs = if o.uo {
        vcod_common::testing::uo_fs()
    } else {
        vcod_common::testing::game_fs()
    }?;
    let fs = Rc::new(fs);
    let bsp = vcod_common::bsp::parse(&fs.read(&fs.resolve_map(&o.map).unwrap()).unwrap()).unwrap();
    let mut now = Instant::now();
    let manual = o.console.is_empty();
    let cfg = vcod_server::ServerConfig {
        map: o.map.clone(),
        hostname: "match".into(),
        max_clients: (o.bots + 2).max(8),
        gametype: o.gametype.clone(),
        test_entities: 0,
        trace: false,
        bots: if manual { o.bots } else { 0 },
        bots_shoot: true,
    };
    let mut sv = vcod_server::Server::new(cfg, now);
    if o.uo {
        sv.set_protocol(&vcod_common::net::protocol::PROTOCOL_V22);
    }
    // `test_seed_rng` sets the low bit: 2 and 3 would replay one match.
    sv.test_seed_rng(seed * 2 + 1);
    for (k, v) in &o.set {
        sv.set_cvar(k, v);
    }
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(fs.clone()).expect("load the scripts");
    for line in &o.console {
        sv.push_console(line);
    }

    let mut m = Match {
        seed,
        ..Default::default()
    };
    let mut seen = 0;
    let mut level = sv.server_id();
    // (attacker, victim) -> tick of the attacker's first hit this life.
    let mut first_hit: HashMap<(String, String), usize> = HashMap::new();
    let mut hist: HashMap<usize, VecDeque<[f32; 3]>> = HashMap::new();
    for tick in 0..o.ticks {
        now += Duration::from_millis(TICK_MS);
        sv.tick(now);
        m.ticks = tick + 1;
        if sv.server_id() != level {
            // A restart (sd's rounds) keeps the log; every body respawns.
            level = sv.server_id();
            first_hit.clear();
        }
        seen = seen.min(sv.script_log().len());
        let skill: HashMap<String, String> = sv
            .bot_slots()
            .into_iter()
            .map(|s| (s.to_string(), skill_of(&sv, s)))
            .collect();
        let lines: Vec<String> = sv.script_log().get(seen..).unwrap_or_default().to_vec();
        seen += lines.len();
        for l in &lines {
            let f: Vec<&str> = l.trim_end().split(';').collect();
            match f[0] {
                // victim num, team, name, attacker num, team, name, weapon,
                // damage, means of death, hit location; UO adds a guid
                // before each num.
                "D" if f.len() >= 11 => {
                    let g = usize::from(f.len() >= 13);
                    let (vnum, anum, ateam) = (f[1 + g], f[4 + 2 * g], f[5 + 2 * g]);
                    let att = tally(&mut m, anum, ateam, &skill);
                    let dmg: u64 = f[8 + 2 * g].parse().unwrap_or(0);
                    if anum != vnum {
                        att.0.damage += dmg;
                        att.1.damage += dmg;
                        first_hit
                            .entry((anum.to_string(), vnum.to_string()))
                            .or_insert(tick);
                    }
                }
                "K" if f.len() >= 11 => {
                    let g = usize::from(f.len() >= 13);
                    let (vnum, vteam) = (f[1 + g], f[2 + g]);
                    let (anum, ateam) = (f[4 + 2 * g], f[5 + 2 * g]);
                    if o.verbose {
                        m.log.push(format!("t{tick} kill {}", f[1..].join(";")));
                    }
                    let v = tally(&mut m, vnum, vteam, &skill);
                    v.0.deaths += 1;
                    v.1.deaths += 1;
                    // The lethal hit logs `K` alone: a one-hit kill takes 0.
                    let ttk = first_hit
                        .remove(&(anum.to_string(), vnum.to_string()))
                        .map_or(0, |t| (tick - t) as u64 * TICK_MS);
                    let dmg: u64 = f[8 + 2 * g].parse().unwrap_or(0);
                    first_hit.retain(|(_, victim), _| victim != vnum);
                    let a = tally(&mut m, anum, ateam, &skill);
                    if anum == vnum || anum == "-1" {
                        a.0.suicides += 1;
                        a.1.suicides += 1;
                    } else if !ateam.is_empty() && ateam == vteam && o.gametype != "dm" {
                        a.0.team_kills += 1;
                        a.1.team_kills += 1;
                    } else {
                        a.0.kills += 1;
                        a.1.kills += 1;
                        a.0.damage += dmg;
                        a.1.damage += dmg;
                        a.0.ttk_ms.push(ttk);
                        a.1.ttk_ms.push(ttk);
                    }
                }
                // [guid,] num, team, name, action
                "A" if f.len() >= 5 => {
                    let n = f.len();
                    *m.actions
                        .entry(format!("{}:{}", f[n - 3], f[n - 1]))
                        .or_default() += 1;
                }
                "W" => {
                    let who = match f.get(1) {
                        Some(t) if !t.is_empty() => t.to_string(),
                        _ => f.last().unwrap_or(&"").to_string(),
                    };
                    *m.wins.entry(who).or_default() += 1;
                }
                // `exitLevel`'s own line; it has zeroed the team scores.
                "ExitLevel: executed" => m.ended = true,
                _ => {}
            }
        }
        if m.ended {
            break;
        }
        m.scores = sv.team_scores();
        for slot in sv.bot_slots() {
            let Some(b) = sv.bot_body(slot).filter(|b| b.playing && !b.deployed) else {
                hist.remove(&slot);
                continue;
            };
            let h = hist.entry(slot).or_default();
            h.push_back(b.origin);
            if h.len() > WINDOW {
                h.pop_front();
            }
            let first = h[0];
            if h.len() == WINDOW
                && h.iter().all(|p| {
                    (0..3).map(|i| (p[i] - first[i]).powi(2)).sum::<f32>() < RADIUS * RADIUS
                })
            {
                m.stalled += 1;
            }
        }
    }
    Some(m)
}

/// The tallies a client's line lands in: its team's and its skill's.
fn tally<'a>(
    m: &'a mut Match,
    num: &str,
    team: &str,
    skill: &HashMap<String, String>,
) -> (&'a mut Tally, &'a mut Tally) {
    let team = if team.is_empty() { "none" } else { team };
    let skill = skill.get(num).cloned().unwrap_or_else(|| "-".into());
    (
        m.by_team.entry(team.to_string()).or_default(),
        m.by_skill.entry(skill).or_default(),
    )
}

/// The `vc_bot_skill` level a bot plays at, as text.
fn skill_of(sv: &vcod_server::Server, slot: usize) -> String {
    sv.bot_skill(slot).map_or("-".into(), |l| l.to_string())
}
