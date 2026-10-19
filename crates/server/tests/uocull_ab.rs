//! What a United Offensive server sends past the PVS cull, against retail's
//! run of `client-probes/probe_uocull` on mp_berlin
//! (`tests/fixtures/vehicles/uocull-berlin.txt`): a viewer out of the
//! parked Panzer IV's PVS. The Panzer and its sound blends stay out until
//! a `playSound` on it brings it in for 300 ms (`r.broadcastTime`); a
//! `script_origin` sounded beside it never comes in; the flak guns' silent
//! blends ride with their guns; the map's looping fire sounds ride from
//! everywhere (`playLoopSound`'s broadcast time -1); a 500-unit cull fog
//! drops everything past it and exponential fog brings it back
//! (docs/research/coduo-diff.md 9, cod11-sound-system.md 9).
//!
//! Per step the gate compares the run of distinct entity lists the viewer
//! was sent, temp entities (the map's ambient sound events) left out.
//! `UOCULL_REPORT=1` prints both sides. Needs `COD_DIR` at a UO install;
//! without one it returns early.
//!
//! The `snapcull` gates replay `client-probes/probe_snapcull` on
//! mp_carentan against retail UO's and 1.1d's runs
//! (`tests/fixtures/entities/snapcull-carentan-*.txt`): an eye in a solid
//! leaf is sent nothing, and the compass reads each engine's
//! `SV_inSnapshot` of a teammate under the map and of a sounded one
//! (docs/protocol-1.1.md, "Which entities a client is sent";
//! cod11-hud-protocol.md 9). The 1.1 one runs at a 1.1 install.

mod common;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::msg::NULL_USERCMD;
use vcod_common::net::protocol::{PROTOCOL_V22, Protocol};

const MAP: &str = "mp_berlin";
const PROBE: &str = "probe_uocull";
const FIXTURE: &str = "tests/fixtures/vehicles/uocull-berlin.txt";

/// An entity list as `number:eType`, the wire's type.
type Ents = BTreeSet<String>;
/// A side's run: its `PROBE step` lines and every change of its list.
type Run = (Vec<(String, i32)>, Vec<(i32, Ents)>);

/// Each step's run of distinct lists: the list in force when the step's
/// line was logged, then every different one before the next step.
fn by_step(steps: &[(String, i32)], snaps: &[(i32, Ents)]) -> BTreeMap<String, Vec<Ents>> {
    let mut out = BTreeMap::new();
    for (i, (name, t)) in steps.iter().enumerate() {
        let end = steps.get(i + 1).map_or(i32::MAX, |s| s.1);
        let mut run: Vec<Ents> = Vec::new();
        let at = snaps.iter().rev().find(|(st, _)| st <= t).map(|(_, e)| e);
        for e in at.into_iter().chain(
            snaps
                .iter()
                .filter(|(st, _)| st > t && *st < end)
                .map(|(_, e)| e),
        ) {
            if run.last() != Some(e) {
                run.push(e.clone());
            }
        }
        out.insert(name.clone(), run);
    }
    out
}

fn step_line(l: &str) -> Option<(String, i32)> {
    let rest = l.trim().strip_prefix("PROBE step ")?;
    let (name, t) = rest.split_once(' ')?;
    Some((name.to_string(), t.trim().parse().ok()?))
}

fn retail() -> Run {
    let text = std::fs::read_to_string(FIXTURE).expect("read the fixture");
    let steps = text.lines().filter_map(step_line).collect();
    let snaps = text
        .lines()
        .filter_map(|l| {
            let rest = l.strip_prefix("EDGE ents serverTime=")?;
            let (t, list) = rest.split_once(" [")?;
            let ents = list
                .trim_end_matches(']')
                .split_whitespace()
                .filter(|e| !e.ends_with(":211"))
                .map(str::to_string)
                .collect();
            Some((t.parse().ok()?, ents))
        })
        .collect();
    (steps, snaps)
}

fn ours() -> Run {
    let fs = vcod_common::testing::uo_fs().unwrap();
    let p: &'static Protocol = &PROTOCOL_V22;
    let bsp_path = fs.resolve_map(MAP).expect("the map in the UO paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(MAP, PROBE), now);
    sv.set_protocol(p);
    sv.overlay_script(
        &format!("maps/mp/gametypes/{PROBE}"),
        include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_uocull.gsc"),
    );
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    common::run_alone(&mut sv, &mut now, common::PROBE_JOIN_MS);
    let q = Rc::new(RefCell::new(common::Queues::default()));
    let mut cl = common::connect(&mut sv, &q, &mut now);
    let mut snaps: Vec<(i32, Ents)> = Vec::new();
    let mut seen = i32::MIN;
    for _ in 0..(40_000 / common::FRAME_MS) {
        now += Duration::from_millis(common::FRAME_MS as u64);
        cl.send_frame(&NULL_USERCMD);
        common::step(&mut sv, &q, &mut cl, now);
        if let Some(s) = cl.snapshots().newest()
            && s.server_time != seen
        {
            seen = s.server_time;
            let ents: Ents = s
                .entities
                .iter()
                .map(|(n, e)| (n, e.field_i32(p, "eType")))
                .filter(|&(_, t)| p.event_of(t).is_none())
                .map(|(n, t)| format!("{n}:{t}"))
                .collect();
            if snaps.last().is_none_or(|(_, e)| *e != ents) {
                snaps.push((s.server_time, ents));
            }
        }
        if sv
            .script_log()
            .iter()
            .any(|l| l.contains("PROBE step done"))
        {
            break;
        }
    }
    let steps = sv
        .script_log()
        .iter()
        .filter_map(|l| step_line(l))
        .collect();
    (steps, snaps)
}

#[test]
fn the_uo_cull_matches_retail_step_for_step() {
    if vcod_common::testing::uo_fs().is_none() {
        eprintln!("COD_DIR has no uo/: skipping");
        return;
    }
    let (rs, rsn) = retail();
    let (os, osn) = ours();
    let names: Vec<&String> = rs.iter().map(|(n, _)| n).collect();
    assert_eq!(
        names,
        os.iter().map(|(n, _)| n).collect::<Vec<_>>(),
        "steps"
    );
    // The sounded Panzer rides from the sound's frame up to 300 ms past it.
    let ridden = |snaps: &[(i32, Ents)]| {
        let on = snaps.iter().position(|(_, e)| e.contains("109:12"))?;
        Some(snaps.get(on + 1)?.0 - snaps[on].0)
    };
    assert_eq!(ridden(&rsn), Some(350), "retail's window");
    assert_eq!(ridden(&osn), ridden(&rsn), "the sounded Panzer's window");
    let (r, o) = (by_step(&rs, &rsn), by_step(&os, &osn));
    let report = std::env::var_os("UOCULL_REPORT").is_some();
    for name in names {
        let (r, o) = (&r[name], &o[name]);
        if report {
            for (side, run) in [("retail", r), ("ours", o)] {
                for (i, e) in run.iter().enumerate() {
                    println!(
                        "{name} {side} {i}: {}",
                        e.iter().cloned().collect::<Vec<_>>().join(" ")
                    );
                }
            }
        }
        assert_eq!(r.len(), o.len(), "{name}: lists in the step");
        for (i, (a, b)) in r.iter().zip(o).enumerate() {
            assert!(
                a == b,
                "{name} list {i}: retail only {:?}, ours only {:?}",
                a.difference(b).collect::<Vec<_>>(),
                b.difference(a).collect::<Vec<_>>()
            );
        }
    }
}

/// A side's `probe_snapcull` run: the step lines, the viewer's compass
/// word and the subject's entity list, each at every change.
struct SnapRun {
    steps: Vec<(String, i32)>,
    compass: Vec<(i32, i32)>,
    subject: Vec<(i32, Ents)>,
}

impl SnapRun {
    /// Every change from the `quiet` step to `done`, as (ms past `quiet`,
    /// value), the value in force at `quiet` first.
    fn from_quiet<T: Clone + PartialEq>(&self, changes: &[(i32, T)]) -> Vec<(i32, T)> {
        let at = |name: &str| self.steps.iter().find(|(n, _)| n == name).unwrap().1;
        let (quiet, done) = (at("quiet"), at("done"));
        let mut out: Vec<(i32, T)> = Vec::new();
        let first = changes.iter().rev().find(|(t, _)| *t <= quiet);
        for (t, v) in first
            .into_iter()
            .chain(changes.iter().filter(|(t, _)| *t > quiet && *t < done))
        {
            if out.last().is_none_or(|(_, l)| l != v) {
                out.push(((*t - quiet).max(0), v.clone()));
            }
        }
        out
    }
}

fn snapcull_retail(path: &str, p: &Protocol) -> SnapRun {
    let text = std::fs::read_to_string(path).expect("read the fixture");
    let field = |l: &str, key: &str| -> Option<String> {
        l.split_whitespace()
            .find_map(|w| w.strip_prefix(key))
            .map(str::to_string)
    };
    let compass = text
        .lines()
        .filter(|l| l.starts_with("COMPASS "))
        .filter_map(|l| {
            let t = field(l, "t=")?.parse().ok()?;
            let info = field(l, "info=0x")?;
            Some((t, u32::from_str_radix(&info, 16).ok()? as i32))
        })
        .collect();
    let subject = text
        .lines()
        .filter_map(|l| {
            let rest = l.strip_prefix("EDGE ents serverTime=")?;
            let (t, list) = rest.split_once(" [")?;
            let ents = list
                .trim_end_matches(']')
                .split_whitespace()
                .filter(|e| {
                    let t: i32 = e.split_once(':').unwrap().1.parse().unwrap();
                    p.event_of(t).is_none()
                })
                .map(str::to_string)
                .collect();
            Some((t.parse().ok()?, ents))
        })
        .collect();
    SnapRun {
        steps: text.lines().filter_map(step_line).collect(),
        compass,
        subject,
    }
}

fn snapcull_ours(fs: vcod_common::pk3::Pk3Fs, p: &'static Protocol) -> SnapRun {
    use common::{ClientEnd, Queues};
    use vcod_common::net::NetClient;
    const SNAP_MAP: &str = "mp_carentan";
    let bsp_path = fs.resolve_map(SNAP_MAP).expect("the map in the paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(SNAP_MAP, "probe_snapcull"), now);
    sv.set_protocol(p);
    sv.overlay_script(
        "maps/mp/gametypes/probe_snapcull",
        include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_snapcull.gsc"),
    );
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    let (qa, qb) = (
        Rc::new(RefCell::new(Queues::default())),
        Rc::new(RefCell::new(Queues::default())),
    );
    let mut viewer = NetClient::start_with_qport(ClientEnd(qa.clone()), now, 0x2001);
    let mut subject = NetClient::start_with_qport(ClientEnd(qb.clone()), now, 0x2002);
    let (mut compass, mut ents): (Vec<(i32, i32)>, Vec<(i32, Ents)>) = (Vec::new(), Vec::new());
    let (mut seen_v, mut seen_s) = (i32::MIN, i32::MIN);
    for _ in 0..(40_000 / common::FRAME_MS) {
        now += Duration::from_millis(common::FRAME_MS as u64);
        viewer.send_frame(&NULL_USERCMD);
        subject.send_frame(&NULL_USERCMD);
        common::step_pair(&mut sv, (&qa, &mut viewer), (&qb, &mut subject), now);
        if let Some(s) = viewer.snapshots().newest()
            && s.server_time != seen_v
        {
            seen_v = s.server_time;
            let info = s.ps.field_i32(p, "iCompassFriendInfo");
            if compass.last().is_none_or(|(_, i)| *i != info) {
                compass.push((s.server_time, info));
            }
        }
        if let Some(s) = subject.snapshots().newest()
            && s.server_time != seen_s
        {
            seen_s = s.server_time;
            let list: Ents = s
                .entities
                .iter()
                .map(|(n, e)| (n, e.field_i32(p, "eType")))
                .filter(|&(_, t)| p.event_of(t).is_none())
                .map(|(n, t)| format!("{n}:{t}"))
                .collect();
            if ents.last().is_none_or(|(_, e)| *e != list) {
                ents.push((s.server_time, list));
            }
        }
        if sv
            .script_log()
            .iter()
            .any(|l| l.contains("PROBE step done"))
        {
            break;
        }
    }
    SnapRun {
        steps: sv
            .script_log()
            .iter()
            .filter_map(|l| step_line(l))
            .collect(),
        compass,
        subject: ents,
    }
}

/// `probe_snapcull`'s rows, retail's `fixture` against ours.
fn snapcull_matches(fixture: &str, fs: vcod_common::pk3::Pk3Fs, p: &'static Protocol) {
    let r = snapcull_retail(fixture, p);
    let o = snapcull_ours(fs, p);
    let names = |s: &SnapRun| s.steps.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
    assert_eq!(names(&r), names(&o), "steps");
    let fmt = |c: Vec<(i32, i32)>| {
        c.into_iter()
            .map(|(t, i)| (t, format!("{i:#010x}")))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        fmt(o.from_quiet(&o.compass)),
        fmt(r.from_quiet(&r.compass)),
        "the viewer's iCompassFriendInfo past quiet, (ms, word)"
    );
    let (rs, os) = (r.from_quiet(&r.subject), o.from_quiet(&o.subject));
    assert_eq!(
        os.iter().map(|(t, e)| (*t, e.len())).collect::<Vec<_>>(),
        rs.iter().map(|(t, e)| (*t, e.len())).collect::<Vec<_>>(),
        "the subject's lists past quiet, (ms, entities)"
    );
    for ((t, a), (_, b)) in rs.iter().zip(&os) {
        assert!(
            a == b,
            "+{t}: retail only {:?}, ours only {:?}",
            a.difference(b).collect::<Vec<_>>(),
            b.difference(a).collect::<Vec<_>>()
        );
    }
}

/// An eye in a solid leaf is sent nothing; a teammate in no cluster is
/// on the compass (UO's `SV_inSnapshot` reads him as not sent); a sounded
/// one is off it until the snapshot after his broadcast time runs out.
#[test]
fn the_uo_solid_eye_and_compass_match_retail() {
    let Some(fs) = vcod_common::testing::uo_fs() else {
        eprintln!("COD_DIR has no uo/: skipping");
        return;
    };
    snapcull_matches(
        "tests/fixtures/entities/snapcull-carentan-uo.txt",
        fs,
        &PROTOCOL_V22,
    );
}

/// 1.1's side of the same run: the eye in a solid leaf is sent nothing
/// too, but 1.1's `SV_inSnapshot` reads a teammate in no cluster as sent
/// (off the compass), and `playSound`'s `SVF_BROADCAST` keeps him off it.
#[test]
fn the_11_solid_eye_and_compass_match_retail() {
    let Some(fs) = vcod_common::testing::game_fs_11() else {
        eprintln!("COD_DIR is not a 1.1 install: skipping");
        return;
    };
    snapcull_matches(
        "tests/fixtures/entities/snapcull-carentan-11.txt",
        fs,
        &vcod_common::net::protocol::PROTOCOL_V1,
    );
}
