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
//! `WORLDCOPY_REPORT=1` prints every compared row.
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

/// Float formatting.
const TOL: f32 = 0.05;

/// Known divergences, `(phase, column, tolerance)`; the columns are 0 the
/// player, 1 the pack, 2 the copy. Each names the movers doc section that
/// explains it.
const GAPS: &[(&str, usize, f32)] = &[
    // The first frame after the first push: the player falls inside its own
    // cmd from the pushed spot, 0.11 lower on retail.
    ("up", 0, 0.15),
];

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
}

fn ours() -> Option<&'static Ours> {
    static RUN: std::sync::OnceLock<Option<Ours>> = std::sync::OnceLock::new();
    RUN.get_or_init(run_ours).as_ref()
}

fn run_ours() -> Option<Ours> {
    let fs = vcod_common::testing::game_fs()?;
    let bsp_path = fs.resolve_map(MAP).expect("map in the mounted paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(MAP, "probe_worldcopy"), now);
    sv.overlay_script(
        "maps/mp/gametypes/probe_worldcopy",
        include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_worldcopy.gsc"),
    );
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    // Each landing's lift is a fresh `rand()` draw, which ours cannot
    // replay. Before the samples start, a landing where the pack rests takes
    // the lift up to retail's first rest. After, ours lands every item with
    // none and the pack is raised to retail's rest after the frame it landed
    // in: the retail sample a frame later (a thread reads the last frame's
    // entity pass), the clocks lined up on the first `rest` sample. Each lift
    // is kept for the test to hold to the lift's range.
    let track = pack_track(&std::fs::read_to_string(SERVER).unwrap());
    let rest = track[0].1;
    let sampling = Rc::new(std::cell::Cell::new(false));
    let landed = Rc::new(RefCell::new(Vec::new()));
    let lifts = Rc::new(RefCell::new(Vec::new()));
    let (seen, early, started) = (landed.clone(), lifts.clone(), sampling.clone());
    sv.test_item_lifts(Box::new(move |time, floor| {
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
    let q = Rc::new(RefCell::new(Queues::default()));
    let mut cl = NetClient::start_with_qport(ClientEnd(q.clone()), now, 0x2001);
    let mut join = Join::new("allies", "m1carbine_mp");
    let mut present: Option<std::collections::BTreeSet<u32>> = None;
    let mut new = BTreeMap::new();
    for _ in 0..3000 {
        now += Duration::from_millis(FRAME_MS as u64);
        cl.send_frame(&holding(&cl));
        for e in step_at(&mut sv, ADDR, &q, &mut cl, now) {
            match e {
                NetEvent::ServerCommand(tokens) => join.on_server_command(&tokens, &mut cl, now),
                NetEvent::Dropped(r) => panic!("dropped: {r}"),
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
        }
        if offset.is_none()
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
        if sv.script_log().iter().any(|l| l.contains("PROBE done")) {
            break;
        }
    }
    assert_eq!(sv.script_aborts(), Vec::<String>::new());
    Some(Ours {
        log: sv.script_log().to_vec(),
        new,
        lifts: lifts.take(),
    })
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
    let tol = |phase: &str, col: usize| {
        GAPS.iter()
            .find(|g| (g.0 == phase || g.0 == "*") && g.1 == col)
            .map_or(TOL, |g| g.2)
    };
    let mut bad = Vec::new();
    for (name, r) in &retail {
        let o = ours
            .get(name)
            .unwrap_or_else(|| panic!("ours never ran {name}"));
        assert_eq!(r.len(), o.len(), "{name}: sample count");
        for (k, (rf, of)) in r.iter().zip(o).enumerate() {
            if report() {
                println!("{name} {k} retail {rf:?}\n{name} {k} ours   {of:?}");
            }
            for col in 0..3 {
                let row = match (rf[col], of[col]) {
                    (Some(a), Some(b)) if diff(a, b) > tol(name, col) => {
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
    assert!(
        bad.is_empty(),
        "{} rows differ:\n{}",
        bad.len(),
        bad.join("\n")
    );
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
