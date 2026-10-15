//! A tank's death on ours against retail's capture of it
//! (`crates/common/tests/fixtures/net/*-uo-wreck.bin`, an axis player under
//! `client-probes/probe_vehwreck` on mp_cassino; recipe in
//! docs/research/coduo-vehicles.md 15.2). Ours runs the same probe with a
//! client joined as axis and records the events its snapshots carry.
//!
//! `WRECK_REPORT=1` prints both sides' events. Needs `COD_DIR` at a UO
//! install; without one it returns early.

mod common;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::NetEvent;
use vcod_common::net::event_ids::{EV_EARTHQUAKE, EV_SOUND_ALIAS};
use vcod_common::net::events::{EventTracker, GameEvent};
use vcod_common::net::msg::UserCmd;
use vcod_common::net::protocol::PROTOCOL_V22;

const MAP: &str = "mp_cassino";
const PROBE: &str = "probe_vehwreck";

/// Each event with the server time of the snapshot that brought it.
fn retail() -> Vec<(i32, GameEvent)> {
    let p = &PROTOCOL_V22;
    let mut tracker = EventTracker::new();
    let mut out = Vec::new();
    for f in vcod_common::testing::fixture_frames(
        p,
        "net/gamestate-uo-wreck.bin",
        "net/snapshots-uo-wreck.bin",
    ) {
        let s = &f.snapshot;
        out.extend(tracker.drain(s, p).into_iter().map(|e| (s.server_time, e)));
    }
    out
}

fn ours(secs: i64) -> Vec<(i32, GameEvent)> {
    let fs = vcod_common::testing::uo_fs().unwrap();
    let p = &PROTOCOL_V22;
    let src = format!("../gsc/tests/fixtures/semantics/client-probes/{PROBE}.gsc");
    let probe = std::fs::read_to_string(src).expect("read the probe");
    let bsp_path = fs.resolve_map(MAP).expect("the map in the UO paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(MAP, PROBE), now);
    sv.set_protocol(p);
    sv.overlay_script(&format!("maps/mp/gametypes/{PROBE}"), &probe);
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    common::run_alone(&mut sv, &mut now, common::PROBE_JOIN_MS);
    let q = Rc::new(RefCell::new(common::Queues::default()));
    let mut cl = common::connect(&mut sv, &q, &mut now);
    let mut join = common::Join::new("axis", "kar98k_mp");
    let mut tracker = EventTracker::new();
    let mut out = Vec::new();
    for _ in 0..secs * 1000 / common::FRAME_MS {
        now += Duration::from_millis(common::FRAME_MS as u64);
        cl.send_frame(&UserCmd::default());
        for e in common::step(&mut sv, &q, &mut cl, now) {
            if let NetEvent::ServerCommand(tokens) = e {
                join.on_server_command(&tokens, &mut cl, now);
            }
        }
        if let Some(s) = cl.snapshots().newest() {
            out.extend(tracker.drain(s, p).into_iter().map(|e| (s.server_time, e)));
        }
    }
    if std::env::var_os("WRECK_REPORT").is_some() {
        for l in sv.script_log() {
            print!("ours log {l}");
        }
    }
    out
}

fn report(side: &str, run: &[(i32, GameEvent)]) {
    if std::env::var_os("WRECK_REPORT").is_none() {
        return;
    }
    for (t, e) in run {
        println!(
            "{side} t {t} ev {} on {} parm {} pos {:?}",
            e.event, e.entity_num, e.parm, e.pos
        );
    }
}

/// The kill's earthquake time and position, then each `EV_SOUND_ALIAS` a
/// temp entity carried: its time past the kill, parm and offset from the
/// quake.
fn wreck_sounds(run: &[(i32, GameEvent)]) -> (i32, Vec<(i32, i32, [f32; 3])>) {
    let (t0, quake) = run
        .iter()
        .find(|(_, e)| e.event == EV_EARTHQUAKE)
        .expect("the kill's earthquake");
    let sounds = run
        .iter()
        .filter(|(_, e)| e.event == EV_SOUND_ALIAS && e.entity_num != u32::MAX)
        .filter(|(t, _)| *t < t0 + 10_400)
        .map(|(t, e)| (t - t0, e.parm, [0, 1, 2].map(|i| e.pos[i] - quake.pos[i])))
        .collect();
    (*t0, sounds)
}

/// 100 ms after `spawnDuplicate`, the duplicate (unlinked, but a brush
/// model) runs its first step's collision probes against the burning tank
/// it stands in and plays `impactSnd` 2500 units above the hit
/// (coduo-vehicles.md 15.2): the one sound alias a temp entity carries.
#[test]
fn the_wrecks_duplicate_plays_its_impact_sound() {
    if vcod_common::testing::uo_fs().is_none() {
        return;
    }
    let r = retail();
    let o = ours(48);
    report("retail", &r);
    report("ours", &o);
    let (_, rs) = wreck_sounds(&r);
    let (_, os) = wreck_sounds(&o);
    assert_eq!(rs.len(), 1, "{rs:?}");
    assert_eq!(os.len(), rs.len(), "retail {rs:?}, ours {os:?}");
    let (rt, rp, roff) = rs[0];
    let (ot, op, ooff) = os[0];
    assert_eq!((ot, op), (rt, rp), "retail {rs:?}, ours {os:?}");
    for i in 0..3 {
        assert!(
            (ooff[i] - roff[i]).abs() < 2.0,
            "retail {rs:?}, ours {os:?}"
        );
    }
}
