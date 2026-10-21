//! Seeded bot-vs-bot matches: the one measuring stick for bot changes
//! (docs/research/bot-combat.md section 1). Each map and seed runs one
//! match with the bots' fire on until the map ends (`exitLevel`) or
//! `TICKS` run out, and reads the script's `logPrint` lines: kills,
//! deaths, damage, the time from a bot's first hit on a victim to the
//! kill, objective actions (`A;`: plants, defuses, pickups, captures),
//! wins (`W;`: a round's in `sd` and `re`, the match's elsewhere) and the
//! team scores. Each bot-tick adds its life, its wounds (health first
//! under 50 in a fight this life) and those it died of within 5 s, the
//! retreats it starts (hurt, dry), and the ticks it holds a cover spot and
//! leans, the grenades it throws and the clip it starts each fight on
//! (grenade kills come off the `K;` lines). It also counts stalled bot-ticks as `ship_stalls` does (12 s
//! within 150 units).
//! Run: COD_DIR=<install> cargo run --release -p vcod-server --example bot_match [seed...]
//! `MAPS=` a comma list (`MAP=` alias; mp_carentan), `GAMETYPE=` (tdm),
//! `BOTS=` (6), `TICKS=` (12000, ten minutes), `JOBS=` (4, matches run in
//! parallel), `SET=name=value,...` cvars, `UO=1` a United Offensive
//! server. `AXIS_SKILL=` / `ALLIES_SKILL=` add the bots per side at those
//! `vc_bot_skill` levels, half each (dm: `SKILLS=a,b,...`, one bot per
//! entry); without them `BOTS` join by balance at `vc_bot_skill`.
//! One-sided feature arms: `COVER=`, `RETREAT=` and `TEAMPLAY=` (cover and
//! lean, `bots/cover.rs`; the retreat, `bots/retreat.rs`; team play,
//! `bots/team.rs`) each take `all` (the default), `none`, `axis`, `allies`
//! or `alt` (axis on odd seeds, allies on even, so the map's own
//! asymmetry cancels); `dm` has no sides, so only `all` and `none` there.
//! With an arm one-sided the totals add a row per arm, named by the
//! one-sided features on for it. `VERBOSE=1` prints every kill. Seeds
//! default to 1..=8.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::rc::Rc;
use std::time::{Duration, Instant};

const TICK_MS: u64 = 50;
const WINDOW: usize = 240;
const RADIUS: f32 = 150.0;

/// Which side a feature is on for (`COVER=`, `RETREAT=`, `TEAMPLAY=`).
#[derive(Clone, Copy, PartialEq)]
enum Arm {
    All,
    None,
    Axis,
    Allies,
    Alt,
}

impl Arm {
    fn parse(k: &str) -> Arm {
        match std::env::var(k).as_deref().unwrap_or("all") {
            "all" => Arm::All,
            "none" => Arm::None,
            "axis" => Arm::Axis,
            "allies" => Arm::Allies,
            "alt" => Arm::Alt,
            v => panic!("{k}={v}: all, none, axis, allies or alt"),
        }
    }

    /// Whether the feature is on for `team` ("axis", "allies", "none") in
    /// `seed`'s match.
    fn on(self, team: &str, seed: u64) -> bool {
        match self {
            Arm::All => true,
            Arm::None => false,
            Arm::Axis => team == "axis",
            Arm::Allies => team == "allies",
            Arm::Alt => team == if seed % 2 == 1 { "axis" } else { "allies" },
        }
    }

    fn one_sided(self) -> bool {
        matches!(self, Arm::Axis | Arm::Allies | Arm::Alt)
    }
}

#[derive(Clone)]
struct Opts {
    maps: Vec<String>,
    gametype: String,
    bots: usize,
    ticks: usize,
    uo: bool,
    set: Vec<(String, String)>,
    /// Console lines run once the level is up (the per-side `addbot`s).
    console: Vec<String>,
    verbose: bool,
    /// cover, retreat, team play.
    arms: [Arm; 3],
}

const FEATURES: [&str; 3] = ["cover", "retreat", "teamplay"];

impl Opts {
    /// The one-sided features on for `team`, the arm its tallies land in.
    fn arm(&self, team: &str, seed: u64) -> String {
        let on: Vec<&str> = FEATURES
            .iter()
            .zip(&self.arms)
            .filter(|(_, a)| a.one_sided() && a.on(team, seed))
            .map(|(f, _)| *f)
            .collect();
        if on.is_empty() {
            "control".into()
        } else {
            on.join("+")
        }
    }
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
    /// Bot-ticks played and the lives they made up.
    life_ticks: u64,
    lives: u32,
    /// Wounds, and those died of within 5 s.
    wounds: u32,
    wound_deaths: u32,
    /// Retreats started hurt and dry.
    hurt: u32,
    dry: u32,
    /// Bot-ticks holding a cover spot, and leaning.
    hold: u64,
    lean: u64,
    kit: Kit,
}

/// Grenades and the clip a fight starts on: throws, the hits and kills
/// their blasts made on others (`MOD_GRENADE_SPLASH`), engagements started
/// with a bullet weapon in hand and the clip fractions they started on,
/// summed.
#[derive(Default, Clone)]
struct Kit {
    throws: u32,
    grenade_hits: u32,
    grenade_kills: u32,
    engages: u32,
    clip_sum: f64,
}

impl Kit {
    fn add(&mut self, o: &Kit) {
        self.throws += o.throws;
        self.grenade_hits += o.grenade_hits;
        self.grenade_kills += o.grenade_kills;
        self.engages += o.engages;
        self.clip_sum += o.clip_sum;
    }

    fn line(&self) -> String {
        format!(
            "grenades thrown {:4} hits {:3} kills {:3} engagements {:5} clip at start {:3.0}%",
            self.throws,
            self.grenade_hits,
            self.grenade_kills,
            self.engages,
            100.0 * self.clip_sum / f64::from(self.engages.max(1)),
        )
    }

    /// One bot-tick: the throws since `life`'s last count, and an
    /// engagement on the tick tracking starts.
    fn step(life: &mut Life, body: Option<&vcod_server::bots::BotBody>) -> Kit {
        let mut k = Kit::default();
        let throws = body.map_or(life.throws, |b| b.throws);
        k.throws = throws.saturating_sub(life.throws);
        life.throws = throws;
        let engaged = body.is_some_and(|b| b.playing && b.engaged);
        if engaged
            && !life.engaged
            && let Some(b) = body.filter(|b| b.clip_size > 0 && b.clip >= 0)
        {
            k.engages = 1;
            k.clip_sum = f64::from(b.clip) / f64::from(b.clip_size);
        }
        life.engaged = engaged;
        k
    }
}

impl Tally {
    fn add(&mut self, o: &Tally) {
        self.kills += o.kills;
        self.deaths += o.deaths;
        self.suicides += o.suicides;
        self.team_kills += o.team_kills;
        self.damage += o.damage;
        self.ttk_ms.extend(&o.ttk_ms);
        self.life_ticks += o.life_ticks;
        self.lives += o.lives;
        self.wounds += o.wounds;
        self.wound_deaths += o.wound_deaths;
        self.hurt += o.hurt;
        self.dry += o.dry;
        self.hold += o.hold;
        self.lean += o.lean;
        self.kit.add(&o.kit);
    }

    /// The per-bot-tick line under [`Tally::line`].
    fn survive(&self) -> String {
        format!(
            "life {:5.1} s wounds {:4} dead in 5 s {:3.0}% retreats hurt {:4} dry {:4} cover hold {:6} lean {:6}",
            self.life_ticks as f32 * TICK_MS as f32 / 1000.0 / self.lives.max(1) as f32,
            self.wounds,
            100.0 * self.wound_deaths as f32 / self.wounds.max(1) as f32,
            self.hurt,
            self.dry,
            self.hold,
            self.lean,
        )
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
    map: String,
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
    by_arm: BTreeMap<String, Tally>,
    /// Wins and actions by arm, keyed as `wins` and `actions` are.
    arm_wins: BTreeMap<String, u32>,
    arm_actions: BTreeMap<String, u32>,
    actions: BTreeMap<String, u32>,
    stalled: u32,
    log: Vec<String>,
    dm: bool,
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
        maps: env("MAPS", &env("MAP", "mp_carentan"))
            .split(',')
            .map(str::to_string)
            .collect(),
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
        arms: ["COVER", "RETREAT", "TEAMPLAY"].map(Arm::parse),
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
        "maps {} gametype {} bots {} ticks {} uo {} set {:?} console {:?} cover/retreat/teamplay {:?}",
        opts.maps.join(","),
        opts.gametype,
        opts.bots,
        opts.ticks,
        opts.uo,
        opts.set,
        opts.console,
        ["COVER", "RETREAT", "TEAMPLAY"].map(|k| std::env::var(k).unwrap_or_else(|_| "all".into())),
    );
    let started = Instant::now();
    let jobs_list: Vec<(String, u64)> = opts
        .maps
        .iter()
        .rev()
        .flat_map(|m| seeds.iter().rev().map(move |s| (m.clone(), *s)))
        .collect();
    let queue = std::sync::Mutex::new(jobs_list);
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..jobs.max(1) {
            s.spawn(|| {
                loop {
                    let Some((map, seed)) = queue.lock().unwrap().pop() else {
                        break;
                    };
                    let Some(m) = run(&opts, &map, seed) else {
                        return;
                    };
                    eprintln!(
                        "{map} seed {seed} done ({:.0} s)",
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
    results.sort_by(|a, b| (&a.map, a.seed).cmp(&(&b.map, b.seed)));
    for m in &results {
        for l in &m.log {
            println!("  {} seed {} {l}", m.map, m.seed);
        }
        let all = m.by_team.values().fold(Tally::default(), |mut a, t| {
            a.add(t);
            a
        });
        println!(
            "{} seed {}: ticks {} ended {} wins {:?} score axis {} allies {} stalled {} {} actions {:?}",
            m.map,
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
    }
    if opts.maps.len() > 1 {
        for map in &opts.maps {
            let of: Vec<&Match> = results.iter().filter(|m| &m.map == map).collect();
            summarize(map, &of);
        }
    }
    summarize("total", &results.iter().collect::<Vec<_>>());
}

/// Totals over `results`: per team, skill and arm, wins, scores, actions.
fn summarize(what: &str, results: &[&Match]) {
    let mut by_team: BTreeMap<String, Tally> = BTreeMap::new();
    let mut by_skill: BTreeMap<String, Tally> = BTreeMap::new();
    let mut by_arm: BTreeMap<String, Tally> = BTreeMap::new();
    let mut actions: BTreeMap<String, u32> = BTreeMap::new();
    let mut wins: BTreeMap<String, u32> = BTreeMap::new();
    let mut arm_wins: BTreeMap<String, u32> = BTreeMap::new();
    let mut arm_actions: BTreeMap<String, u32> = BTreeMap::new();
    let (mut stalled, mut ended, mut scores) = (0, 0, [0i64; 2]);
    for m in results {
        for (k, t) in &m.by_team {
            by_team.entry(k.clone()).or_default().add(t);
        }
        for (k, t) in &m.by_skill {
            by_skill.entry(k.clone()).or_default().add(t);
        }
        for (k, t) in &m.by_arm {
            by_arm.entry(k.clone()).or_default().add(t);
        }
        for (k, n) in &m.actions {
            *actions.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &m.wins {
            *wins.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &m.arm_wins {
            *arm_wins.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &m.arm_actions {
            *arm_actions.entry(k.clone()).or_default() += n;
        }
        ended += u32::from(m.ended);
        stalled += m.stalled;
        scores[0] += i64::from(m.scores[0]);
        scores[1] += i64::from(m.scores[1]);
    }
    println!("{what} over {} matches:", results.len());
    let rows = |kind: &str, map: &BTreeMap<String, Tally>| {
        for (k, t) in map {
            println!("  {kind} {k:<13} {}", t.line());
            println!("  {kind} {k:<13} {}", t.survive());
            println!("  {kind} {k:<13} {}", t.kit.line());
        }
    };
    rows("team ", &by_team);
    rows("skill", &by_skill);
    if by_arm.len() > 1 {
        rows("arm  ", &by_arm);
        println!("  arm wins {arm_wins:?}  arm actions {arm_actions:?}");
    }
    println!("  ended {ended}  wins {wins:?}");
    println!(
        "  score axis {} allies {}  actions {actions:?}  stalled {stalled}",
        scores[0], scores[1]
    );
}

/// One seeded match; `None` without game data.
fn run(o: &Opts, map: &str, seed: u64) -> Option<Match> {
    let fs = if o.uo {
        vcod_common::testing::uo_fs()
    } else {
        vcod_common::testing::game_fs()
    }?;
    let fs = Rc::new(fs);
    let bsp = vcod_common::bsp::parse(&fs.read(&fs.resolve_map(map).unwrap()).unwrap()).unwrap();
    let mut now = Instant::now();
    let manual = o.console.is_empty();
    let cfg = vcod_server::ServerConfig {
        map: map.to_string(),
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
        map: map.to_string(),
        seed,
        dm: o.gametype == "dm",
        ..Default::default()
    };
    let mut seen = 0;
    let mut level = sv.server_id();
    // (attacker, victim) -> tick of the attacker's first hit this life.
    let mut first_hit: HashMap<(String, String), usize> = HashMap::new();
    let mut hist: HashMap<usize, VecDeque<[f32; 3]>> = HashMap::new();
    let mut bots: HashMap<usize, Life> = HashMap::new();
    for tick in 0..o.ticks {
        now += Duration::from_millis(TICK_MS);
        // A default arm never calls its hook, so an all-on run is the
        // stock game.
        for slot in sv.bot_slots() {
            let team = team_name(&mut sv, slot, m.dm);
            let [cover, retreat, team_play] = o.arms;
            if cover != Arm::All {
                sv.test_bot_cover(slot, cover.on(team, seed));
            }
            if retreat != Arm::All {
                sv.test_bot_retreat(slot, retreat.on(team, seed));
            }
            if team_play != Arm::All {
                sv.test_bot_team_play(slot, team_play.on(team, seed));
            }
        }
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
                    if anum == "-1" {
                        continue;
                    }
                    let att = tally(&mut m, o, anum, ateam, &skill);
                    let dmg: u64 = f[8 + 2 * g].parse().unwrap_or(0);
                    if anum != vnum {
                        att.0.damage += dmg;
                        att.1.damage += dmg;
                        att.2.damage += dmg;
                        first_hit
                            .entry((anum.to_string(), vnum.to_string()))
                            .or_insert(tick);
                        if f[9 + 2 * g] == "MOD_GRENADE_SPLASH" {
                            att.0.kit.grenade_hits += 1;
                            att.1.kit.grenade_hits += 1;
                            att.2.kit.grenade_hits += 1;
                        }
                    }
                }
                "K" if f.len() >= 11 => {
                    let g = usize::from(f.len() >= 13);
                    let (vnum, vteam) = (f[1 + g], f[2 + g]);
                    let (anum, ateam) = (f[4 + 2 * g], f[5 + 2 * g]);
                    if o.verbose {
                        m.log.push(format!("t{tick} kill {}", f[1..].join(";")));
                    }
                    let v = tally(&mut m, o, vnum, vteam, &skill);
                    v.0.deaths += 1;
                    v.1.deaths += 1;
                    v.2.deaths += 1;
                    // The lethal hit logs `K` alone: a one-hit kill takes 0.
                    let ttk = first_hit
                        .remove(&(anum.to_string(), vnum.to_string()))
                        .map_or(0, |t| (tick - t) as u64 * TICK_MS);
                    let dmg: u64 = f[8 + 2 * g].parse().unwrap_or(0);
                    first_hit.retain(|(_, victim), _| victim != vnum);
                    if anum == vnum || anum == "-1" {
                        let v = tally(&mut m, o, vnum, vteam, &skill);
                        v.0.suicides += 1;
                        v.1.suicides += 1;
                        v.2.suicides += 1;
                        continue;
                    }
                    let a = tally(&mut m, o, anum, ateam, &skill);
                    if !ateam.is_empty() && ateam == vteam && o.gametype != "dm" {
                        a.0.team_kills += 1;
                        a.1.team_kills += 1;
                        a.2.team_kills += 1;
                    } else {
                        a.0.kills += 1;
                        a.1.kills += 1;
                        a.2.kills += 1;
                        a.0.damage += dmg;
                        a.1.damage += dmg;
                        a.2.damage += dmg;
                        a.0.ttk_ms.push(ttk);
                        a.1.ttk_ms.push(ttk);
                        a.2.ttk_ms.push(ttk);
                        if f[9 + 2 * g] == "MOD_GRENADE_SPLASH" {
                            a.0.kit.grenade_hits += 1;
                            a.1.kit.grenade_hits += 1;
                            a.2.kit.grenade_hits += 1;
                            a.0.kit.grenade_kills += 1;
                            a.1.kit.grenade_kills += 1;
                            a.2.kit.grenade_kills += 1;
                        }
                    }
                }
                // [guid,] num, team, name, action
                "A" if f.len() >= 5 => {
                    let n = f.len();
                    *m.actions
                        .entry(format!("{}:{}", f[n - 3], f[n - 1]))
                        .or_default() += 1;
                    let arm = o.arm(f[n - 3], seed);
                    *m.arm_actions
                        .entry(format!("{arm}:{}", f[n - 1]))
                        .or_default() += 1;
                }
                "W" => {
                    let who = match f.get(1) {
                        Some(t) if !t.is_empty() => t.to_string(),
                        _ => f.last().unwrap_or(&"").to_string(),
                    };
                    *m.arm_wins.entry(o.arm(&who, seed)).or_default() += 1;
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
            let team = team_name(&mut sv, slot, m.dm);
            // Still picking a team.
            if team == "none" && !m.dm {
                continue;
            }
            let num = slot.to_string();
            let body = sv.bot_body(slot);
            let cover = sv.bot_cover_state(slot);
            let life = bots.entry(slot).or_default();
            let t = tally(&mut m, o, &num, team, &skill);
            life.step(body.as_ref(), cover, t);
        }
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

/// The tallies a client's line lands in: its team's, its skill's and its
/// arm's.
fn tally<'a>(
    m: &'a mut Match,
    o: &Opts,
    num: &str,
    team: &str,
    skill: &HashMap<String, String>,
) -> (&'a mut Tally, &'a mut Tally, &'a mut Tally) {
    // `dm`'s `D;` lines carry the menu's team, its `K;` lines none.
    let team = if team.is_empty() || m.dm {
        "none"
    } else {
        team
    };
    let skill = skill.get(num).cloned().unwrap_or_else(|| "-".into());
    let arm = o.arm(team, m.seed);
    (
        m.by_team.entry(team.to_string()).or_default(),
        m.by_skill.entry(skill).or_default(),
        m.by_arm.entry(arm).or_default(),
    )
}

/// A bot's team as the log names it ("none" in `dm` or off a team).
fn team_name(sv: &mut vcod_server::Server, slot: usize, dm: bool) -> &'static str {
    match sv.bot_team(slot) {
        _ if dm => "none",
        1 => "axis",
        2 => "allies",
        _ => "none",
    }
}

/// One bot's running life, wound and retreat state between ticks.
#[derive(Default)]
struct Life {
    alive: u64,
    /// Ticks since this life's wound, if any.
    wound: Option<u64>,
    retreating: u8,
    /// [`Kit::step`]'s last throw count and tracking.
    throws: u32,
    engaged: bool,
}

impl Life {
    /// Adds one tick of `body` (and its cover hold and lean) to `t`.
    fn step(
        &mut self,
        body: Option<&vcod_server::bots::BotBody>,
        cover: Option<(bool, f32)>,
        t: (&mut Tally, &mut Tally, &mut Tally),
    ) {
        let (mut d, playing, dead) = (
            Tally::default(),
            body.is_some_and(|b| b.playing),
            body.is_some_and(|b| b.dead),
        );
        let r = body.map_or(0, |b| b.retreating);
        match (self.retreating, r) {
            (0, 1) => d.hurt += 1,
            (0, 2) => d.dry += 1,
            _ => {}
        }
        self.retreating = r;
        d.kit = Kit::step(self, body);
        if let Some(b) = body.filter(|b| b.playing) {
            if self.wound.is_none() && b.engaged && b.health < 50 {
                self.wound = Some(0);
                d.wounds += 1;
            } else if let Some(w) = self.wound.as_mut() {
                *w += 1;
            }
        } else if dead {
            if self.wound.is_some_and(|w| w < 5000 / TICK_MS) {
                d.wound_deaths += 1;
            }
            self.wound = None;
        }
        if playing {
            self.alive += 1;
            if let Some((hold, lean)) = cover {
                d.hold += u64::from(hold);
                d.lean += u64::from(lean != 0.0);
            }
        } else if self.alive > 0 {
            d.life_ticks += self.alive;
            d.lives += 1;
            self.alive = 0;
        }
        t.0.add(&d);
        t.1.add(&d);
        t.2.add(&d);
    }
}

/// The `vc_bot_skill` level a bot plays at, as text.
fn skill_of(sv: &vcod_server::Server, slot: usize) -> String {
    sv.bot_skill(slot).map_or("-".into(), |l| l.to_string())
}
