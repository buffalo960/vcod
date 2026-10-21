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
//! (grenade kills come off the `K;` lines), and its aim: rounds fired (a
//! bullet gun's clip drops), bullet hits off the `D;`/`K;` lines, and the
//! time from acquiring a target with none tracked to the first round at
//! it. It also counts the bot-ticks 12 s within 150 units, as
//! `ship_stalls` does, split into `held` (holding still on purpose,
//! `BotBody::holds`) and `stalled` (the rest), how close
//! a team keeps: each playing bot-tick's distance to its nearest teammate
//! and whether two or more stand within 400 units, the hits bots answer
//! with fire at the attacker within 1 s and the deaths to a killer the
//! bot never targeted. It counts peeks from cover, those that repeat the
//! spot and pose of the one before, hunts that reach a lost enemy's spot
//! (or the vantage over it), and the deaths within 2 s of each.
//! Run: COD_DIR=<install> cargo run --release -p vcod-server --example bot_match [seed...]
//! `MAPS=` a comma list (`MAP=` alias; mp_carentan), `GAMETYPE=` (tdm),
//! `BOTS=` (6), `TICKS=` (12000, ten minutes), `JOBS=` (4, matches run in
//! parallel), `SET=name=value,...` cvars, `UO=1` a United Offensive
//! server. `AXIS_SKILL=` / `ALLIES_SKILL=` add the bots per side at those
//! `vc_bot_skill` levels, half each (dm: `SKILLS=a,b,...`, one bot per
//! entry); without them `BOTS` join by balance at `vc_bot_skill`.
//! One-sided feature arms: `COVER=`, `RETREAT=`, `TEAMPLAY=`, `AWARE=`,
//! `FOOTWORK=` and `WARY=` (cover and lean, `bots/cover.rs`; the
//! retreat, `bots/retreat.rs`; team play, `bots/team.rs`;
//! `Bot::aware`; moving while fighting, `Bot::footwork`; varied peeks
//! and hunts, `Bot::wary`) each take `all` (the default), `none`, `axis`, `allies`
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
/// A bot-tick is packed with this many teammates or more within
/// `PACK_RANGE` of it.
const PACK: usize = 2;
const PACK_RANGE: f32 = 400.0;

/// Which side a feature is on for (`COVER=`, `RETREAT=`, `TEAMPLAY=`,
/// `AWARE=`, `FOOTWORK=`, `WARY=`).
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
    /// cover, retreat, team play, awareness, footwork, wariness.
    arms: [Arm; 6],
}

const FEATURES: [&str; 6] = ["cover", "retreat", "teamplay", "aware", "footwork", "wary"];
const ARMS: [&str; 6] = ["COVER", "RETREAT", "TEAMPLAY", "AWARE", "FOOTWORK", "WARY"];

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
    /// Hits a bot took from an enemy (one per attacker until answered or
    /// 1 s on), those whose attacker it targeted within 1 s, and those it
    /// answered within 1 s with its trigger down and that attacker its
    /// target.
    hits_taken: u32,
    turned: u32,
    answered: u32,
    /// A bot's deaths to an enemy, and those whose killer it never
    /// targeted this life.
    killed: u32,
    blindsided: u32,
    /// Bot-ticks playing with a teammate playing, the sum of their
    /// distances to the nearest one, and those with [`PACK`] or more
    /// teammates inside [`PACK_RANGE`].
    mate_ticks: u64,
    mate_dist: f64,
    packed: u64,
    aim: Aim,
    kit: Kit,
    peeks: Peeks,
    /// Walking bot-ticks (`BotBody::walking` this tick and the last), the
    /// sum of their view turns in degrees, and those that turned more than
    /// [`SNAP_DEG`].
    walk_ticks: u64,
    walk_turn: f64,
    snaps: u64,
}

/// A walking view turned more than this in one tick has snapped.
const SNAP_DEG: f32 = 30.0;

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

/// The aim columns (bot-combat.md section 1): rounds fired, bullet hits,
/// targets acquired with none tracked, and the ms from each to its first
/// round at that target.
#[derive(Default, Clone)]
struct Aim {
    shots: u64,
    hits: u64,
    acquired: u32,
    first_shot_ms: Vec<u64>,
}

impl Aim {
    fn add(&mut self, o: &Aim) {
        self.shots += o.shots;
        self.hits += o.hits;
        self.acquired += o.acquired;
        self.first_shot_ms.extend(&o.first_shot_ms);
    }

    fn line(&self) -> String {
        let mut t = self.first_shot_ms.clone();
        t.sort_unstable();
        let mean = t.iter().sum::<u64>() / t.len().max(1) as u64;
        format!(
            "shots {:6} hits {:5} accuracy {:4.1}% sight to shot mean {:4} median {:4} ms, {:4.1}% of {} acquired fired on",
            self.shots,
            self.hits,
            100.0 * self.hits as f32 / self.shots.max(1) as f32,
            mean,
            t.get(t.len() / 2).copied().unwrap_or(0),
            100.0 * t.len() as f32 / self.acquired.max(1) as f32,
            self.acquired,
        )
    }
}

/// A means of death a bullet deals: what a hit counts.
fn bullet(mod_: &str) -> bool {
    matches!(
        mod_,
        "MOD_RIFLE_BULLET" | "MOD_PISTOL_BULLET" | "MOD_HEAD_SHOT"
    )
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
        self.aim.add(&o.aim);
        self.hits_taken += o.hits_taken;
        self.answered += o.answered;
        self.turned += o.turned;
        self.killed += o.killed;
        self.blindsided += o.blindsided;
        self.mate_ticks += o.mate_ticks;
        self.mate_dist += o.mate_dist;
        self.packed += o.packed;
        self.kit.add(&o.kit);
        self.peeks.add(&o.peeks);
        self.walk_ticks += o.walk_ticks;
        self.walk_turn += o.walk_turn;
        self.snaps += o.snaps;
    }

    /// The walking view's line (bot-combat.md section 1).
    fn walk(&self) -> String {
        format!(
            "walking ticks {:7} turn mean {:5.2} deg snaps over {SNAP_DEG:.0} deg {:6} ({:5.2}%)",
            self.walk_ticks,
            self.walk_turn / self.walk_ticks.max(1) as f64,
            self.snaps,
            100.0 * self.snaps as f64 / self.walk_ticks.max(1) as f64,
        )
    }

    /// The per-bot-tick line under [`Tally::line`].
    fn survive(&self) -> String {
        format!(
            "life {:5.1} s wounds {:4} dead in 5 s {:3.0}% retreats hurt {:4} dry {:4} cover hold {:6} lean {:6} mate {:4.0} packed {:4.1}% turned {:3.0}% answered {:3.0}% of {:5} blindsided {:3.0}% of {:4}",
            self.life_ticks as f32 * TICK_MS as f32 / 1000.0 / self.lives.max(1) as f32,
            self.wounds,
            100.0 * self.wound_deaths as f32 / self.wounds.max(1) as f32,
            self.hurt,
            self.dry,
            self.hold,
            self.lean,
            self.mate_dist / self.mate_ticks.max(1) as f64,
            100.0 * self.packed as f64 / self.mate_ticks.max(1) as f64,
            100.0 * self.turned as f32 / self.hits_taken.max(1) as f32,
            100.0 * self.answered as f32 / self.hits_taken.max(1) as f32,
            self.hits_taken,
            100.0 * self.blindsided as f32 / self.killed.max(1) as f32,
            self.killed,
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
    /// 12 s stall ticks with no hold, and those holding on purpose.
    stalled: u32,
    held: u32,
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
        arms: ARMS.map(Arm::parse),
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
        "maps {} gametype {} bots {} ticks {} uo {} set {:?} console {:?} cover/retreat/teamplay/aware/footwork/wary {:?}",
        opts.maps.join(","),
        opts.gametype,
        opts.bots,
        opts.ticks,
        opts.uo,
        opts.set,
        opts.console,
        ARMS.map(|k| std::env::var(k).unwrap_or_else(|_| "all".into())),
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
            "{} seed {}: ticks {} ended {} wins {:?} score axis {} allies {} stalled {} held {} {} actions {:?}",
            m.map,
            m.seed,
            m.ticks,
            m.ended,
            m.wins,
            m.scores[0],
            m.scores[1],
            m.stalled,
            m.held,
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
    let (mut stalled, mut held, mut ended, mut scores) = (0, 0, 0, [0i64; 2]);
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
        held += m.held;
        scores[0] += i64::from(m.scores[0]);
        scores[1] += i64::from(m.scores[1]);
    }
    println!("{what} over {} matches:", results.len());
    let rows = |kind: &str, map: &BTreeMap<String, Tally>| {
        for (k, t) in map {
            println!("  {kind} {k:<13} {}", t.line());
            println!("  {kind} {k:<13} {}", t.survive());
            println!("  {kind} {k:<13} {}", t.aim.line());
            println!("  {kind} {k:<13} {}", t.kit.line());
            println!("  {kind} {k:<13} {}", t.peeks.line());
            println!("  {kind} {k:<13} {}", t.walk());
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
        "  score axis {} allies {}  actions {actions:?}  stalled {stalled} held {held}",
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
    let mut react = React::default();
    for tick in 0..o.ticks {
        now += Duration::from_millis(TICK_MS);
        // A default arm never calls its hook, so an all-on run is the
        // stock game.
        for slot in sv.bot_slots() {
            let team = team_name(&mut sv, slot, m.dm);
            let [cover, retreat, team_play, aware, footwork, wary] = o.arms;
            if cover != Arm::All {
                sv.test_bot_cover(slot, cover.on(team, seed));
            }
            if retreat != Arm::All {
                sv.test_bot_retreat(slot, retreat.on(team, seed));
            }
            if team_play != Arm::All {
                sv.test_bot_team_play(slot, team_play.on(team, seed));
            }
            if aware != Arm::All {
                sv.test_bot_aware(slot, aware.on(team, seed));
            }
            if footwork != Arm::All {
                sv.test_bot_footwork(slot, footwork.on(team, seed));
            }
            if wary != Arm::All {
                sv.test_bot_wary(slot, wary.on(team, seed));
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
        react.watch(&mut sv, m.dm, tick, |num, team, f| {
            let t = tally(&mut m, o, num, team, &skill);
            f(t.0);
            f(t.1);
            f(t.2);
        });
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
                        let hit = u64::from(bullet(f[9 + 2 * g]));
                        att.0.damage += dmg;
                        att.1.damage += dmg;
                        att.2.damage += dmg;
                        att.0.aim.hits += hit;
                        att.1.aim.hits += hit;
                        att.2.aim.hits += hit;
                        first_hit
                            .entry((anum.to_string(), vnum.to_string()))
                            .or_insert(tick);
                        if f[9 + 2 * g] == "MOD_GRENADE_SPLASH" {
                            att.0.kit.grenade_hits += 1;
                            att.1.kit.grenade_hits += 1;
                            att.2.kit.grenade_hits += 1;
                        }
                        let vteam = f[2 + g];
                        if m.dm || ateam.is_empty() || ateam != vteam {
                            react.hit(vnum, anum, tick, |t| {
                                let v = tally(&mut m, o, vnum, vteam, &skill);
                                v.0.hits_taken += t;
                                v.1.hits_taken += t;
                                v.2.hits_taken += t;
                            });
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
                    let hit = u64::from(bullet(f[9 + 2 * g]));
                    a.0.aim.hits += hit;
                    a.1.aim.hits += hit;
                    a.2.aim.hits += hit;
                    if !ateam.is_empty() && ateam == vteam && o.gametype != "dm" {
                        a.0.team_kills += 1;
                        a.1.team_kills += 1;
                        a.2.team_kills += 1;
                    } else {
                        let blind = react.killed(vnum, anum);
                        if let Some(blind) = blind {
                            let v = tally(&mut m, o, vnum, vteam, &skill);
                            for t in [v.0, v.1, v.2] {
                                t.killed += 1;
                                t.blindsided += u32::from(blind);
                            }
                        }
                        let a = tally(&mut m, o, anum, ateam, &skill);
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
            let tactics = sv.bot_tactics(slot);
            let life = bots.entry(slot).or_default();
            let t = tally(&mut m, o, &num, team, &skill);
            life.step(body.as_ref(), cover, tactics, t);
        }
        // Each playing bot's distance to its nearest playing teammate, and
        // how many stand inside PACK_RANGE.
        let playing: Vec<(usize, [f32; 3])> = sv
            .bot_slots()
            .into_iter()
            .filter_map(|s| Some((s, sv.bot_body(s).filter(|b| b.playing && !b.dead)?.origin)))
            .collect();
        let playing: Vec<(usize, &'static str, [f32; 3])> = playing
            .into_iter()
            .map(|(s, p)| (s, team_name(&mut sv, s, m.dm), p))
            .filter(|(_, t, _)| *t != "none")
            .collect();
        for &(slot, team, at) in &playing {
            let d: Vec<f32> = playing
                .iter()
                .filter(|(s, t, _)| *s != slot && *t == team)
                .map(|(_, _, p)| (0..3).map(|i| (p[i] - at[i]).powi(2)).sum::<f32>().sqrt())
                .collect();
            let Some(near) = d.iter().copied().reduce(f32::min) else {
                continue;
            };
            let close = d.iter().filter(|&&x| x < PACK_RANGE).count();
            let t = tally(&mut m, o, &slot.to_string(), team, &skill);
            for t in [t.0, t.1, t.2] {
                t.mate_ticks += 1;
                t.mate_dist += f64::from(near);
                t.packed += u64::from(close >= PACK);
            }
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
                if b.holds() {
                    m.held += 1;
                } else {
                    m.stalled += 1;
                }
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
    /// The held bullet gun and its clip last tick.
    gun: Option<(u8, i16)>,
    /// The target acquired with none tracked, and the ticks since, until
    /// its first round.
    acquired: Option<(usize, Option<u64>)>,
    /// [`Kit::step`]'s last throw count and tracking.
    throws: u32,
    engaged: bool,
    peeks: PeekLife,
    /// The view's yaw last tick when the bot walked then.
    walk_yaw: Option<f32>,
}

impl Life {
    /// Adds one tick of `body` (and its cover hold and lean) to `t`.
    fn step(
        &mut self,
        body: Option<&vcod_server::bots::BotBody>,
        cover: Option<(bool, f32)>,
        tactics: Option<[u32; 3]>,
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
        let fired = match (body.filter(|b| b.playing && b.gun), self.gun) {
            (Some(b), Some((w, clip))) if b.weapon == w && b.clip < clip => (clip - b.clip) as u64,
            _ => 0,
        };
        d.aim.shots += fired;
        self.gun = body
            .filter(|b| b.playing && b.gun)
            .map(|b| (b.weapon, b.clip));
        let target = body.filter(|b| b.playing).and_then(|b| b.target);
        // Ticks since the fresh target came, `None` once it was fired on.
        self.acquired = match (target, self.acquired) {
            (Some(t), Some((a, pending))) if t == a => Some((
                a,
                pending.and_then(|n| {
                    if fired > 0 {
                        d.aim.first_shot_ms.push(n * TICK_MS);
                        None
                    } else {
                        Some(n + 1)
                    }
                }),
            )),
            // A switch from one target to another is not a fresh sight.
            (Some(t), Some(_)) => Some((t, None)),
            (Some(t), None) => {
                d.aim.acquired += 1;
                if fired > 0 {
                    d.aim.first_shot_ms.push(0);
                    Some((t, None))
                } else {
                    Some((t, Some(1)))
                }
            }
            (None, _) => None,
        };
        d.kit = Kit::step(self, body);
        d.peeks = Peeks::step(&mut self.peeks, tactics, dead);
        let walking = body.filter(|b| b.playing && b.walking);
        if let (Some(b), Some(y)) = (walking, self.walk_yaw) {
            let turn = (b.yaw - y + 180.0).rem_euclid(360.0) - 180.0;
            d.walk_ticks += 1;
            d.walk_turn += f64::from(turn.abs());
            d.snaps += u64::from(turn.abs() > SNAP_DEG);
        }
        self.walk_yaw = walking.map(|b| b.yaw);
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

/// Ticks a hit's answer is waited for: 1 s.
const ANSWER_TICKS: usize = (1000 / TICK_MS) as usize;

/// How bots answer hits and whom they had in their sights when killed
/// (`turned`, `answered`, `blindsided`; bot-combat.md section 1).
#[derive(Default)]
struct React {
    /// (victim, attacker) -> tick of the hit still waiting on an answer,
    /// and whether the victim has targeted the attacker since.
    pending: HashMap<(usize, usize), (usize, bool)>,
    /// Per bot: the slots it targeted this life, and whether it played
    /// last tick.
    targeted: HashMap<usize, (std::collections::HashSet<usize>, bool)>,
}

impl React {
    /// One tick's bodies, read before its log lines: on a tick after the
    /// hit's, the attacker as target turns, the trigger down besides
    /// answers. `add` credits a slot's tallies.
    fn watch(
        &mut self,
        sv: &mut vcod_server::Server,
        dm: bool,
        tick: usize,
        mut add: impl FnMut(&str, &str, &mut dyn FnMut(&mut Tally)),
    ) {
        for slot in sv.bot_slots() {
            let Some(b) = sv.bot_body(slot) else {
                continue;
            };
            let (seen, was) = self.targeted.entry(slot).or_default();
            if b.playing && !*was {
                seen.clear();
            }
            *was = b.playing;
            if !b.playing {
                continue;
            }
            seen.extend(b.target);
            let team = team_name(sv, slot, dm);
            let num = slot.to_string();
            self.pending.retain(|(v, a), (t, turned)| {
                if *v != slot || *t >= tick || b.target != Some(*a) {
                    return true;
                }
                if !*turned {
                    *turned = true;
                    add(&num, team, &mut |t| t.turned += 1);
                }
                if b.firing {
                    add(&num, team, &mut |t| t.answered += 1);
                }
                !b.firing
            });
        }
        self.pending.retain(|_, (t, _)| tick - *t < ANSWER_TICKS);
    }

    /// A hit on `victim` by `attacker` at `tick`; `count(1)` when it opens
    /// a window (the victim is a bot with none open on that attacker).
    fn hit(&mut self, victim: &str, attacker: &str, tick: usize, count: impl FnOnce(u32)) {
        let (Ok(v), Ok(a)) = (victim.parse(), attacker.parse()) else {
            return;
        };
        if !self.targeted.contains_key(&v) || self.pending.contains_key(&(v, a)) {
            return;
        }
        self.pending.insert((v, a), (tick, false));
        count(1);
    }

    /// A bot `victim` killed by enemy `killer`: whether it never targeted
    /// him this life; `None` when the victim is no bot.
    fn killed(&mut self, victim: &str, killer: &str) -> Option<bool> {
        let (Ok(v), Ok(k)) = (victim.parse::<usize>(), killer.parse::<usize>()) else {
            return None;
        };
        self.pending.retain(|(pv, _), _| *pv != v);
        let (seen, _) = self.targeted.get(&v)?;
        Some(!seen.contains(&k))
    }
}

/// Ticks a death counts against the peek or hunt arrival before it: 2 s.
const EXPOSED_TICKS: u64 = 2000 / TICK_MS;

/// Peeks from cover and hunt arrivals (`Bot::tactics`) and the deaths
/// within [`EXPOSED_TICKS`] of each: of any peek, of a re-peek (the spot
/// and pose of the peek before), of a hunt reaching the lost enemy's spot
/// or its vantage (bot-combat.md section 1).
#[derive(Default, Clone)]
struct Peeks {
    peeks: u32,
    peek_deaths: u32,
    repeeks: u32,
    repeek_deaths: u32,
    hunts: u32,
    hunt_deaths: u32,
}

/// One bot's last counts and the ticks since its last peek, re-peek and
/// hunt arrival this life.
#[derive(Default)]
struct PeekLife {
    last: [u32; 3],
    since: [Option<u64>; 3],
    dead: bool,
}

impl Peeks {
    fn add(&mut self, o: &Peeks) {
        self.peeks += o.peeks;
        self.peek_deaths += o.peek_deaths;
        self.repeeks += o.repeeks;
        self.repeek_deaths += o.repeek_deaths;
        self.hunts += o.hunts;
        self.hunt_deaths += o.hunt_deaths;
    }

    fn line(&self) -> String {
        let pct = |d: u32, n: u32| 100.0 * d as f32 / n.max(1) as f32;
        format!(
            "peeks {:6} dead in 2 s {:4.1}% re-peeks {:6} dead in 2 s {:4.1}% hunts {:5} dead in 2 s {:4.1}%",
            self.peeks,
            pct(self.peek_deaths, self.peeks),
            self.repeeks,
            pct(self.repeek_deaths, self.repeeks),
            self.hunts,
            pct(self.hunt_deaths, self.hunts),
        )
    }

    /// One bot-tick: the counts' rise since the last, and on the tick the
    /// body dies, a death against each still inside its window.
    fn step(life: &mut PeekLife, tactics: Option<[u32; 3]>, dead: bool) -> Peeks {
        let mut p = Peeks::default();
        let now = tactics.unwrap_or(life.last);
        let rose: [u32; 3] = std::array::from_fn(|i| now[i].saturating_sub(life.last[i]));
        [p.peeks, p.repeeks, p.hunts] = rose;
        for (since, rose) in life.since.iter_mut().zip(rose) {
            *since = if rose > 0 {
                Some(0)
            } else {
                since.map(|t| t + 1)
            };
        }
        life.last = now;
        if dead && !life.dead {
            let inside = |s: Option<u64>| u32::from(s.is_some_and(|t| t < EXPOSED_TICKS));
            p.peek_deaths = inside(life.since[0]);
            p.repeek_deaths = inside(life.since[1]);
            p.hunt_deaths = inside(life.since[2]);
            life.since = [None; 3];
        }
        life.dead = dead;
        p
    }
}
