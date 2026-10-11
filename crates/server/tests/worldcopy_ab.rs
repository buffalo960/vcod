//! A script-spawned brush entity, a copy of the world's clip where it
//! stands, on the wire and as a pusher, against the retail capture.
//!
//! Two fixtures from one retail run (the first one's header carries the
//! recipe): `movers/mp_carentan-dm-worldcopy.txt` is the server's
//! `games_mp.log` under `client-probes/probe_worldcopy.gsc`, one `PROBE f`
//! line per server frame of a phase (the player, the health pack and the
//! copy); `-worldcopy-wire.txt` is the `--probe-ride` client's half. Ours
//! runs the same probe with one allied client standing still.
//! `docs/research/cod11-movers.md` 20 reads both. Each pack landing's
//! random lift (items doc 14.2) is retail's, held to the lift's range, so
//! the pack is compared to the same tolerance as the rest.
//!
//! Three more captures, each header with its recipe, movers doc 21:
//! `-worldcopy2.txt` turns a copy under the player and blasts three
//! `func_static`s, `-worldcopy3.txt` reruns its turns with the player
//! printed to the ten-thousandth, and `-sinecrush.txt` crushes the player
//! under a `func_bobbing`. `mp_harbor-dm-turnedslab.txt` stands the player
//! beside a turned submodel as it lifts.
//!
//! `WORLDCOPY_REPORT=1` prints every compared row, `WORLDCOPY_PS=1` the
//! client's playerstate per snapshot.
//!
//! Needs `COD_DIR`; without the paks the test returns early.

mod common;

use common::{ADDR, ClientEnd, FRAME_MS, Join, Queues, holding, step_at};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::protocol::PROTOCOL_V1;
use vcod_common::net::{NetClient, NetEvent};

const MAP: &str = "mp_carentan";
const SERVER: &str = "tests/fixtures/movers/mp_carentan-dm-worldcopy.txt";
const WIRE: &str = "tests/fixtures/movers/mp_carentan-dm-worldcopy-wire.txt";
const PROBE: &str =
    include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_worldcopy.gsc");
const SERVER2: &str = "tests/fixtures/movers/mp_carentan-dm-worldcopy2.txt";
const PROBE2: &str =
    include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_worldcopy2.gsc");
const SERVER3: &str = "tests/fixtures/movers/mp_carentan-dm-worldcopy3.txt";
const PROBE3: &str =
    include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_worldcopy3.gsc");
const CRUSH: &str = "tests/fixtures/movers/mp_carentan-dm-sinecrush.txt";
const SLAB: &str = "tests/fixtures/movers/mp_harbor-dm-turnedslab.txt";
const PROBE_SLAB: &str =
    include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_turnedslab.gsc");
const PROBE_CRUSH: &str =
    include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_sinecrush.gsc");

/// Float formatting.
const TOL: f32 = 0.05;

/// A known divergence: phase, column (0 the player, 1 the pack, 2 the
/// copy), the first and last frame it covers, and its tolerance.
type Gap = (&'static str, usize, usize, usize, f32);

/// The known divergences, each with the movers doc section that explains
/// it. Every one is a player in the air for a frame or two, which a
/// usercmd's length decides: the retail client's cmds ran 50 to 65 ms
/// apart, ours run exactly one a frame.
const GAPS: &[Gap] = &[
    // The first push leaves the player in the air over the copy's floor
    // (cod11-movers.md 20).
    ("up", 0, 28, 28, 0.15),
];

/// [`GAPS`] for the turned copy's capture (cod11-movers.md 21), replayed at
/// the probe client's three cmds a frame. After each the player is put back
/// at retail's rest ([`RESYNC2`]). Each is a fall whose slide down the
/// copy's slope rides on where the cmd boundaries fell, which the capture
/// does not record.
const GAPS2: &[Gap] = &[
    // The first lift: retail slid 0.86 down the copy's slope, ours 0.54.
    ("up", 0, 11, 30, 0.35),
    // A jitter off a blocked turn: the fall that follows.
    ("yaw45", 0, 14, 19, 1.1),
    // Retail's push is blocked at pitch frame 15 and jitters (-4, 4); ours
    // is blocked a frame later. The resync to retail's hundredths leaves
    // ours' player up to 0.005 off retail's, more than the 0.002 the turned
    // centre leaves at that frame; `GAPS3`' capture holds it.
    ("pitch", 0, 15, 15, 4.1),
    // The jitter's fall and slide.
    ("pitch", 0, 16, 25, 0.55),
    // A jitter off the lowered copy: the fall that follows.
    ("down", 0, 12, 13, 0.25),
];

/// The samples after which ours' player is put where retail's is at the
/// next: a rest after each of [`GAPS2`]' falls.
const RESYNC2: &[(&str, usize)] = &[("up", 30), ("yaw45", 19), ("pitch", 25)];

/// [`GAPS2`] for `probe_worldcopy3`'s rerun of the turns, whose `PROBE g`
/// lines hold the player to the ten-thousandth. Its first lift's slide
/// matches to 0.007.
const GAPS3: &[Gap] = &[
    // A jitter off a blocked turn: the fall that follows.
    ("yaw45", 0, 14, 19, 1.4),
    // The jitter off the pitch's blocked frame: retail's sample 15 is 0.3
    // into the fall, ours has landed; both slide down the copy's slope.
    ("pitch", 0, 15, 25, 0.35),
];

/// The `PROBE g` print's ten-thousandth plus the carry's float error.
const PRECISE_TOL: f32 = 0.0003;

/// [`RESYNC2`] for `probe_worldcopy3`, the last before the pitch to the
/// ten-thousandth.
const RESYNC3: &[(&str, usize)] = &[("up", 30), ("yaw45", 19), ("yaw0", 48), ("pitch", 25)];

/// When ours' client starts joining in the crush capture: it is playing
/// about 700 ms later, after the sinking copy's floor is under the real one
/// (at 2000) and before the probe places it (at 2400). Earlier, it spawns
/// inside the copy's floor and is crushed where it spawned.
const CRUSH_JOIN_MS: u32 = 1400;

/// How far the crushed player's corpse may sit from retail's while it
/// falls, the first [`FALL_ROWS`] rows after the death.
const CORPSE_TOL: f32 = 0.6;
const FALL_ROWS: usize = 2;

/// The most `G_BounceItem` lifts a landing item off its floor:
/// `0.5 + 0.5 * rand() * -2^-31` (items doc 14.2).
const MAX_LIFT: f32 = 0.5;

/// The pack per sample time, the second vector of each `PROBE f` line.
fn pack_track(text: &str) -> Vec<(i32, [f32; 3])> {
    text.lines()
        .filter_map(|l| {
            let rest = &l[l.find("PROBE f ")? + 8..];
            let time = rest.split_whitespace().nth(1)?.parse().ok()?;
            Some((time, vectors(rest).get(1).copied().flatten()?))
        })
        .collect()
}

fn report() -> bool {
    std::env::var("WORLDCOPY_REPORT").is_ok_and(|v| v == "1")
}

/// Every `(x, y, z)` on a line, and `None` for each `(none)`.
fn vectors(line: &str) -> Vec<Option<[f32; 3]>> {
    line.split('(')
        .skip(1)
        .map(|s| {
            let inner = s.split(')').next()?;
            let v: Vec<f32> = inner
                .split(',')
                .filter_map(|t| t.trim().parse().ok())
                .collect();
            (v.len() == 3).then(|| [v[0], v[1], v[2]])
        })
        .collect()
}

/// Each phase's frames: player, pack and copy, the copy `None` when the
/// phase has none.
type Phases = BTreeMap<String, Vec<Vec<Option<[f32; 3]>>>>;

fn phases<'a>(lines: impl Iterator<Item = &'a str>) -> Phases {
    let mut out = Phases::new();
    for line in lines.filter(|l| !l.starts_with('#')) {
        let Some(rest) = line.find("PROBE f ").map(|i| &line[i + 8..]) else {
            continue;
        };
        let phase = rest.split_whitespace().next().unwrap().to_string();
        out.entry(phase).or_default().push(vectors(rest));
    }
    out
}

/// The copy's spawn origin, which `probe_worldcopy3`'s `PROBE g` lines
/// measure the player from.
const COPY_SPAWN: [f32; 3] = [-576.0, 2688.0, -75.875];

/// Each phase's player per frame off the `PROBE g` lines, to the
/// ten-thousandth: `[x, y, z]` in the world.
fn precise<'a>(lines: impl Iterator<Item = &'a str>) -> BTreeMap<String, Vec<[f32; 3]>> {
    let mut out: BTreeMap<String, Vec<[f32; 3]>> = BTreeMap::new();
    for line in lines.filter(|l| !l.starts_with('#')) {
        let Some(rest) = line.find("PROBE g ").map(|i| &line[i + 8..]) else {
            continue;
        };
        let t: Vec<&str> = rest.split_whitespace().collect();
        let v: Vec<f32> = t[2..5].iter().map(|v| v.parse().unwrap()).collect();
        out.entry(t[0].to_string())
            .or_default()
            .push(std::array::from_fn(|i| v[i] + COPY_SPAWN[i]));
    }
    out
}

/// A `RIDE_NEW` line's `eType`, `solid`, `index`, `eFlags` and origin,
/// keyed by what the probe named the entity.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sent {
    etype: i32,
    solid: i32,
    index: i32,
    eflags: i32,
    origin: [f32; 3],
}

fn parse_new(line: &str) -> (u32, Sent) {
    let field = |k: &str| {
        line.split_whitespace()
            .find_map(|t| t.strip_prefix(k))
            .unwrap()
    };
    let int = |k: &str| {
        let v = field(k);
        match v.strip_prefix("0x") {
            Some(h) => i64::from_str_radix(h, 16).unwrap() as i32,
            None => v.parse().unwrap(),
        }
    };
    let o: Vec<f32> = field("origin=")
        .split(',')
        .map(|v| v.parse().unwrap())
        .collect();
    (
        int("num=") as u32,
        Sent {
            etype: int("eType="),
            solid: int("solid="),
            index: int("index="),
            eflags: int("eFlags="),
            origin: [o[0], o[1], o[2]],
        },
    )
}

/// The probe's `PROBE <tag> <num>` lines: which entity is the copy and
/// which the `func_static`.
fn tagged(lines: &[String], tag: &str) -> Option<u32> {
    let key = format!("PROBE {tag} ");
    lines.iter().find_map(|l| {
        l.split(&key)
            .nth(1)?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    })
}

struct Ours {
    log: Vec<String>,
    /// Every entity that entered a snapshot after the first, as first sent.
    new: BTreeMap<u32, Sent>,
    /// Every pack landing: retail's level time, ours' rest with no lift,
    /// retail's lift above it.
    lifts: Vec<(i32, f32, f32)>,
    /// Whether the level dropped.
    killed: bool,
}

fn ours() -> Option<&'static Ours> {
    static RUN: std::sync::OnceLock<Option<Ours>> = std::sync::OnceLock::new();
    RUN.get_or_init(|| run_ours(MAP, "probe_worldcopy", PROBE, SERVER, &[], 0, false))
        .as_ref()
}

fn ours2() -> Option<&'static Ours> {
    static RUN: std::sync::OnceLock<Option<Ours>> = std::sync::OnceLock::new();
    RUN.get_or_init(|| run_ours(MAP, "probe_worldcopy2", PROBE2, SERVER2, RESYNC2, 0, true))
        .as_ref()
}

fn ours3() -> Option<&'static Ours> {
    static RUN: std::sync::OnceLock<Option<Ours>> = std::sync::OnceLock::new();
    RUN.get_or_init(|| run_ours(MAP, "probe_worldcopy3", PROBE3, SERVER3, RESYNC3, 0, true))
        .as_ref()
}

fn ours_slab() -> Option<&'static Ours> {
    static RUN: std::sync::OnceLock<Option<Ours>> = std::sync::OnceLock::new();
    RUN.get_or_init(|| {
        run_ours(
            "mp_harbor",
            "probe_turnedslab",
            PROBE_SLAB,
            SLAB,
            &[],
            0,
            true,
        )
    })
    .as_ref()
}

fn ours_crush() -> Option<&'static Ours> {
    static RUN: std::sync::OnceLock<Option<Ours>> = std::sync::OnceLock::new();
    RUN.get_or_init(|| {
        run_ours(
            MAP,
            "probe_sinecrush",
            PROBE_CRUSH,
            CRUSH,
            &[],
            CRUSH_JOIN_MS,
            true,
        )
    })
    .as_ref()
}

/// Runs `probe` on `map` on ours with one allied client standing still, to `PROBE
/// done` or the level's drop. `fixture` is the retail capture whose pack
/// lifts it borrows, when it has a pack; once ours has logged each
/// `resync` sample, the player is put where retail's is at the next.
/// `probe_rate` sends the probe client's three cmds a frame instead of one.
fn run_ours(
    map: &str,
    name: &str,
    probe: &str,
    fixture: &str,
    resync: &[(&str, usize)],
    join_ms: u32,
    probe_rate: bool,
) -> Option<Ours> {
    let fs = vcod_common::testing::game_fs()?;
    let bsp_path = fs.resolve_map(map).expect("map in the mounted paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(map, name), now);
    sv.overlay_script(&format!("maps/mp/gametypes/{name}"), probe);
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    // Each landing's lift is a fresh `rand()` draw, which ours cannot
    // replay. Before the samples start, a landing where the pack rests takes
    // the lift up to retail's first rest. After, ours lands every item with
    // none and the pack is raised to retail's rest after the frame it landed
    // in: the retail sample a frame later (a thread reads the last frame's
    // entity pass), the clocks lined up on the first `rest` sample. Each lift
    // is kept for the test to hold to the lift's range.
    let retail_text = std::fs::read_to_string(fixture).unwrap();
    let retail_phases = phases(retail_text.lines());
    let retail_precise = precise(retail_text.lines());
    let mut resynced = 0;
    let track = pack_track(&retail_text);
    let rest = track.first().map_or([0.0; 3], |s| s.1);
    let sampling = Rc::new(std::cell::Cell::new(false));
    let landed = Rc::new(RefCell::new(Vec::new()));
    let lifts = Rc::new(RefCell::new(Vec::new()));
    let (seen, early, started) = (landed.clone(), lifts.clone(), sampling.clone());
    let has_pack = !track.is_empty();
    sv.test_item_lifts(Box::new(move |time, floor| {
        if !has_pack {
            return 0.25;
        }
        let at_rest = (floor[0] - rest[0]).abs() < 0.5 && (floor[1] - rest[1]).abs() < 0.5;
        if started.get() || !at_rest {
            seen.borrow_mut().push((time, floor));
            return 0.0;
        }
        early
            .borrow_mut()
            .push((time, floor[2], rest[2] - floor[2]));
        rest[2] - floor[2]
    }));
    let mut offset = None;
    let mut pack = None;
    // The server runs alone until `join_ms`: the client the crush capture
    // had took that long to be playing.
    for _ in 0..join_ms / FRAME_MS as u32 {
        now += Duration::from_millis(FRAME_MS as u64);
        sv.tick(now);
        sv.take_outgoing();
    }
    let q = Rc::new(RefCell::new(Queues::default()));
    let mut cl = NetClient::start_with_qport(ClientEnd(q.clone()), now, 0x2001);
    // mp_harbor's allies are Russian.
    let weapon = if map == "mp_harbor" {
        "mosin_nagant_mp"
    } else {
        "m1carbine_mp"
    };
    let mut join = Join::new("allies", weapon);
    let mut present: Option<std::collections::BTreeSet<u32>> = None;
    let mut new = BTreeMap::new();
    for _ in 0..3000 {
        now += Duration::from_millis(FRAME_MS as u64);
        if probe_rate {
            send_at_probe_rate(&mut cl);
        } else {
            cl.send_frame(&holding(&cl));
        }
        for e in step_at(&mut sv, ADDR, &q, &mut cl, now) {
            match e {
                NetEvent::ServerCommand(tokens) => join.on_server_command(&tokens, &mut cl, now),
                NetEvent::Dropped(r) => assert!(sv.is_killed(), "dropped: {r}"),
                _ => {}
            }
        }
        if let Some(s) = cl.snapshots().newest() {
            let p = &PROTOCOL_V1;
            if let Some(present) = &present {
                for (&n, e) in s.entities.iter().filter(|(n, _)| !present.contains(n)) {
                    new.entry(n).or_insert(Sent {
                        etype: e.field_i32(p, "eType"),
                        solid: e.field_i32(p, "solid"),
                        index: e.field_i32(p, "index"),
                        eflags: e.field_i32(p, "eFlags"),
                        origin: e.origin(p),
                    });
                }
            }
            present = Some(s.entities.keys().copied().collect());
            if std::env::var("WORLDCOPY_PS").is_ok() {
                println!(
                    "PS t={} origin={:?} vel={:.0},{:.0},{:.0} ground={}",
                    s.server_time,
                    s.ps.origin(p),
                    s.ps.field_f32(p, "velocity[0]"),
                    s.ps.field_f32(p, "velocity[1]"),
                    s.ps.field_f32(p, "velocity[2]"),
                    s.ps.field_i32(p, "groundEntityNum"),
                );
            }
        }
        if offset.is_none()
            && has_pack
            && let Some(t) = pack_track(&sv.script_log().join("\n")).first()
        {
            offset = Some(track[0].0 - t.0);
            sampling.set(true);
            pack = sv.script_log().iter().find_map(|l| {
                let rest = l.split("PROBE player ").nth(1)?;
                rest.split(" pack ").nth(1)?.trim().parse().ok()
            });
        }
        if let (Some(off), Some(pack)) = (offset, pack) {
            for (time, floor) in landed.take() {
                let Some(o) = sv.test_raise_entity(pack, 0.0) else {
                    continue;
                };
                if (o[0] - floor[0]).abs() > 0.5 || (o[1] - floor[1]).abs() > 0.5 {
                    continue;
                }
                let at = time + off + FRAME_MS as i32;
                let Some(&(_, r)) = track.iter().find(|s| s.0 >= at) else {
                    continue;
                };
                sv.test_raise_entity(pack, r[2] - o[2]);
                lifts.borrow_mut().push((time + off, o[2], r[2] - o[2]));
            }
        }
        if let Some(&(phase, k)) = resync.get(resynced) {
            let ours = phases(sv.script_log().iter().map(String::as_str));
            if ours.get(phase).is_some_and(|f| f.len() > k) {
                // The frame's push has run, so the next sample is where
                // retail's player stood: to the hundredth, or to the
                // ten-thousandth off a fixture's `PROBE g` lines.
                let at = match retail_precise.get(phase) {
                    Some(p) => p[k + 1],
                    None => retail_phases[phase][k + 1][0].expect("a player sample"),
                };
                sv.test_set_client_origin(0, at);
                resynced += 1;
            }
        }
        if sv.is_killed() || sv.script_log().iter().any(|l| l.contains("PROBE done")) {
            break;
        }
    }
    assert_eq!(sv.script_aborts(), Vec::<String>::new());
    Some(Ours {
        log: sv.script_log().to_vec(),
        new,
        lifts: lifts.take(),
        killed: sv.is_killed(),
    })
}

/// One frame of the probe client's cmds, one every 16 or 17 ms, each with
/// `send_frame`'s stamp and `delta_angles` rebase. A corpse falls a cmd's
/// gravity on every cmd and keeps the sink of the first that ends above the
/// floor, which a 50 ms cmd never does; a fall onto a slope slides down it
/// only from a cmd that starts within the ground trace's reach, which a
/// 50 ms cmd rarely does (cod11-movers.md 21).
fn send_at_probe_rate(cl: &mut NetClient<ClientEnd>) {
    let now = cl.server_clock_ms();
    let input = holding(cl);
    let da = cl
        .snapshots()
        .newest()
        .map(|s| vcod_common::net::DELTA_ANGLE_FIELDS.map(|n| s.ps.field_i32(&PROTOCOL_V1, n)));
    let cmds = [now - 34, now - 17, now].map(|t| {
        let mut c = input;
        c.server_time = t;
        if let Some(da) = da {
            for (a, d) in c.angles.iter_mut().zip(da) {
                *a = (*a - d) & 0xffff;
            }
        }
        c
    });
    cl.send_cmds(&cmds);
}

/// A dead player's z off a `PROBE f` line.
fn corpse_z(line: &str) -> Option<f32> {
    if !line.contains(" dead ") {
        return None;
    }
    vectors(line).first().copied().flatten().map(|p| p[2])
}

fn diff(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0, f32::max)
}

#[test]
fn a_moving_world_copy_pushes_like_retail() {
    let Some(Ours { log, lifts, .. }) = ours() else {
        eprintln!("COD_DIR unset or has no main/: skipping");
        return;
    };
    assert!(
        log.iter().any(|l| l.contains("PROBE done")),
        "the probe never finished on ours"
    );
    // Retail's two decimals put a lift up to 0.005 out of range.
    for &(time, floor, lift) in lifts {
        if report() {
            println!("landing {time} floor {floor} lift {lift}");
        }
        assert!(
            (-0.01..=MAX_LIFT + 0.01).contains(&lift),
            "the pack landed at {time} at {floor}, {lift} under retail's rest"
        );
    }
    assert!(!lifts.is_empty(), "the pack never landed on ours");
    let retail = phases(std::fs::read_to_string(SERVER).unwrap().lines());
    let ours = phases(log.iter().map(String::as_str));
    let bad = compare_phases(&retail, &ours, GAPS);
    assert!(
        bad.is_empty(),
        "{} rows differ:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// Every phase's frames column by column, each to [`TOL`] or its `gaps`
/// row; the rows that differ.
fn compare_phases(retail: &Phases, ours: &Phases, gaps: &[Gap]) -> Vec<String> {
    let tol = |phase: &str, col: usize, k: usize| {
        gaps.iter()
            .find(|g| g.0 == phase && g.1 == col && (g.2..=g.3).contains(&k))
            .map_or(TOL, |g| g.4)
    };
    let mut bad = Vec::new();
    for (name, r) in retail {
        let Some(o) = ours.get(name) else {
            bad.push(format!("ours never ran {name}"));
            continue;
        };
        if r.len() != o.len() {
            bad.push(format!("{name}: {} samples, ours {}", r.len(), o.len()));
        }
        for (k, (rf, of)) in r.iter().zip(o).enumerate() {
            if report() {
                println!("{name} {k} retail {rf:?}\n{name} {k} ours   {of:?}");
            }
            for col in 0..rf.len().max(of.len()) {
                let (a, b) = (
                    rf.get(col).copied().flatten(),
                    of.get(col).copied().flatten(),
                );
                let row = match (a, b) {
                    (Some(a), Some(b)) if diff(a, b) > tol(name, col, k) => {
                        Some(format!("{a:?} ours {b:?}"))
                    }
                    (Some(_), Some(_)) | (None, None) => None,
                    (a, b) => Some(format!("{a:?} ours {b:?}")),
                };
                if let Some(row) = row {
                    bad.push(format!("{name} frame {k} column {col}: retail {row}"));
                }
            }
        }
    }
    bad
}

/// The `PROBE` lines other than the per-frame samples, with the `<time>`
/// field of the tags that carry one dropped, since ours' clock starts
/// elsewhere.
fn events<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<String> {
    lines
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.find("PROBE ").map(|i| l[i + 6..].trim_end()))
        .filter(|l| !l.starts_with("f ") && !l.starts_with("done") && !l.starts_with("player"))
        .map(|l| {
            let t: Vec<&str> = l.split_whitespace().collect();
            match t[0] {
                "ptr" => [&t[..2], &t[3..]].concat().join(" "),
                "notify" => [&t[..3], &t[4..]].concat().join(" "),
                _ => t.join(" "),
            }
        })
        .collect()
}

#[test]
fn a_turned_world_copy_clips_and_pushes_like_retail() {
    let Some(Ours { log, lifts, .. }) = ours2() else {
        eprintln!("COD_DIR unset or has no main/: skipping");
        return;
    };
    assert!(
        log.iter().any(|l| l.contains("PROBE done")),
        "the probe never finished on ours"
    );
    for &(time, floor, lift) in lifts {
        if report() {
            println!("landing {time} floor {floor} lift {lift}");
        }
        assert!(
            (-0.01..=MAX_LIFT + 0.01).contains(&lift),
            "the pack landed at {time} at {floor}, {lift} under retail's rest"
        );
    }
    let text = std::fs::read_to_string(SERVER2).unwrap();
    let retail = phases(text.lines());
    let ours = phases(log.iter().map(String::as_str));
    let mut bad = compare_phases(&retail, &ours, GAPS2);
    let (r, o) = (events(text.lines()), events(log.iter().map(String::as_str)));
    for i in 0..r.len().max(o.len()) {
        let (a, b) = (r.get(i), o.get(i));
        if report() {
            println!("retail {a:?}\nours   {b:?}");
        }
        if a != b {
            bad.push(format!("event {i}: retail {a:?} ours {b:?}"));
        }
    }
    assert!(
        bad.is_empty(),
        "{} rows differ:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// The first lift and the pitch of `probe_worldcopy3`: the lift's fall
/// slides down the copy's slope as far as retail's, and from retail's rest
/// to the ten-thousandth the turned centre (`CM_TransformedBoxTrace`) blocks the
/// carried spot on the frame retail's does and the jitter lands where
/// retail's did (cod11-movers.md 21).
#[test]
fn a_pitched_copy_blocks_its_rider_like_retail() {
    let Some(Ours { log, .. }) = ours3() else {
        eprintln!("COD_DIR unset or has no main/: skipping");
        return;
    };
    let text = std::fs::read_to_string(SERVER3).unwrap();
    let retail = phases(text.lines());
    let ours = phases(log.iter().map(String::as_str));
    let (rp, op) = (
        precise(text.lines()),
        precise(log.iter().map(String::as_str)),
    );
    if report() {
        for (phase, r) in &rp {
            for (k, (a, b)) in r.iter().zip(&op[phase]).enumerate() {
                println!("g {phase} {k} retail {a:?}\ng {phase} {k} ours   {b:?}");
            }
        }
    }
    let mut bad = compare_phases(&retail, &ours, GAPS3);
    // The first lift's fall lands and slides down the copy's slope to the
    // ten-thousandth in its first frame.
    for (k, (a, b)) in rp["up"].iter().zip(&op["up"]).enumerate().take(12) {
        if diff(*a, *b) > PRECISE_TOL {
            bad.push(format!("up {k} precise: retail {a:?} ours {b:?}"));
        }
    }
    // Carried to the ten-thousandth up to the blocked frame.
    for (k, (a, b)) in rp["pitch"].iter().zip(&op["pitch"]).enumerate().take(15) {
        if diff(*a, *b) > PRECISE_TOL {
            bad.push(format!("pitch {k} precise: retail {a:?} ours {b:?}"));
        }
    }
    assert!(
        bad.is_empty(),
        "{} rows differ:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// [`GAPS`] for the turned slab: the player's height in each stand. The
/// probe sets it a unit up and it lands within the ground trace's quarter
/// unit, 0.125 to 0.36 over the floor on retail, where the client's cmd
/// lengths leave it.
const SLAB_GAPS: &[Gap] = &[
    ("18", 0, 0, 19, 0.25),
    ("20", 0, 0, 19, 0.25),
    ("22", 0, 0, 19, 0.25),
    ("25", 0, 0, 19, 0.25),
];

/// A turned submodel tests a body beside it with a box axial in the
/// model's frame (`CM_TransformedBoxTrace`, cod11-movers.md 22): lifted 2
/// with the player 18 to 25 from its 45-degree face, retail's slab moves
/// and the player is never pushed, where a box axial in the world's would
/// reach the face to 23.2.
#[test]
fn a_turned_submodel_tests_its_neighbour_like_retail() {
    let Some(Ours { log, .. }) = ours_slab() else {
        eprintln!("COD_DIR unset or has no main/: skipping");
        return;
    };
    assert!(
        log.iter().any(|l| l.contains("PROBE done")),
        "the probe never finished on ours: {log:#?}"
    );
    let retail = phases(std::fs::read_to_string(SLAB).unwrap().lines());
    let ours = phases(log.iter().map(String::as_str));
    let bad = compare_phases(&retail, &ours, SLAB_GAPS);
    assert!(
        bad.is_empty(),
        "{} rows differ:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

#[test]
fn a_sine_mover_crushes_what_it_cannot_push_like_retail() {
    let Some(Ours { log, killed, .. }) = ours_crush() else {
        eprintln!("COD_DIR unset or has no main/: skipping");
        return;
    };
    let lines = |it: &mut dyn Iterator<Item = &str>| -> Vec<String> {
        it.filter(|l| !l.starts_with('#'))
            .filter_map(|l| l.find("PROBE").map(|i| l[i..].trim_end().to_string()))
            .collect()
    };
    let text = std::fs::read_to_string(CRUSH).unwrap();
    let retail: Vec<String> = lines(&mut text.lines())
        .into_iter()
        .filter(|l| !l.starts_with("PROBE_ERROR"))
        .collect();
    let ours = lines(&mut log.iter().map(String::as_str));
    if report() {
        for (a, b) in retail.iter().zip(&ours) {
            println!("retail {a}\nours   {b}");
        }
    }
    assert_eq!(ours.len(), retail.len(), "line count");
    // The corpse's fall, the first two rows after the death, lasts the
    // client's cmds: retail's fell 32 ms of them by 2450 (cod11-movers.md
    // 21). From its rest on, every line is exact, the sink under a live
    // player's rest included.
    let mut fall = FALL_ROWS;
    for (r, o) in retail.iter().zip(&ours) {
        let (rz, oz) = (corpse_z(r), corpse_z(o));
        if let (Some(a), Some(b)) = (rz, oz)
            && fall > 0
        {
            fall -= 1;
            let strip = |l: &str, z: f32| l.replacen(&format!("{z:.2})"), ")", 1);
            assert_eq!(strip(o, b), strip(r, a));
            assert!((a - b).abs() <= CORPSE_TOL, "retail {r}\nours   {o}");
            continue;
        }
        assert_eq!(o, r);
    }
    assert!(killed, "the bobbing drops the level one period in");
}

#[test]
fn a_spawned_brush_entity_goes_on_the_wire_like_retail() {
    let Some(Ours { log, new, .. }) = ours() else {
        eprintln!("COD_DIR unset or has no main/: skipping");
        return;
    };
    let text = std::fs::read_to_string(SERVER).unwrap();
    let retail_log: Vec<String> = text.lines().map(str::to_string).collect();
    let retail_new: BTreeMap<u32, Sent> = std::fs::read_to_string(WIRE)
        .unwrap()
        .lines()
        .filter(|l| l.starts_with("RIDE_NEW "))
        .map(parse_new)
        .collect();
    for tag in ["copy", "static"] {
        let r = tagged(&retail_log, tag).and_then(|n| retail_new.get(&n));
        let o = tagged(log, tag).and_then(|n| new.get(&n));
        if report() {
            println!("{tag} retail {r:?} ours {o:?}");
        }
        let (Some(r), Some(o)) = (r, o) else {
            panic!("{tag}: retail {r:?} ours {o:?}");
        };
        assert_eq!(
            (r.etype, r.solid, r.index, r.eflags),
            (o.etype, o.solid, o.index, o.eflags),
            "{tag}"
        );
        assert!(diff(r.origin, o.origin) < TOL, "{tag}: {r:?} ours {o:?}");
    }
}
