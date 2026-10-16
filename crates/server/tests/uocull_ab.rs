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
