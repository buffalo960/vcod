//! A United Offensive tank's shell on another vehicle, ours against
//! retail's. `client-probes/probe_vehicle_shell` seats client 0 in
//! mp_kharkov's first Panzer IV, turns its view onto the Elefant 276 units
//! off and fires the gun with `fireTurret` every 4 s from 3 s into the
//! seat. The fixtures are retail UO 1.51b's: the probe's log
//! (`mp_kharkov-tank-shell-uo.txt`, the Elefant's `damage` notifies) and
//! the driver's snapshots (`*-uo-shellveh.bin`, the explodes). The shell's
//! direct hit is the Elefant's only notify: `G_MissileImpact`'s `G_Damage`
//! with the driver as the attacker and the tank as the inflictor, and the
//! splash skips what it struck (docs/research/coduo-vehicles.md 13).
//!
//! `SHELL_REPORT=1` prints both sides. Needs `COD_DIR` at a UO install;
//! without one it returns early.

mod common;

use common::{Join, Queues, step};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::NetEvent;
use vcod_common::net::event_ids::EV_ROCKET_EXPLODE_NOMARKS;
use vcod_common::net::events::{EventTracker, GameEvent};
use vcod_common::net::msg::{BUTTON_USE, UserCmd};
use vcod_common::net::protocol::PROTOCOL_V22;

const MAP: &str = "mp_kharkov";
/// `vehicle_probe.rs` `SIT`, from the placement: on foot, the use tap,
/// the driver's seat for 12 s.
const SETTLE_MS: i64 = 10_000;
const TAP_MS: i64 = 150;
const SEAT_MS: i64 = 12_000;

/// Ours: the log and every explode our snapshots carried.
fn ours() -> Option<(Vec<String>, Vec<(i32, GameEvent)>)> {
    let fs = vcod_common::testing::uo_fs()?;
    let p = &PROTOCOL_V22;
    let bsp_path = fs.resolve_map(MAP).expect("map in the UO paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(MAP, "probe_vehicle_shell"), now);
    sv.set_protocol(&PROTOCOL_V22);
    sv.overlay_script(
        "maps/mp/gametypes/probe_vehicle_shell",
        include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_vehicle_shell.gsc"),
    );
    sv.set_cvar("probe_vehicletype", "panzeriv_mp");
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    common::run_alone(&mut sv, &mut now, common::PROBE_JOIN_MS);
    let q = Rc::new(RefCell::new(Queues::default()));
    let mut cl =
        vcod_common::net::NetClient::start_with_qport(common::ClientEnd(q.clone()), now, 0x2001);
    let mut join = Join::new("axis", "kar98k_mp");
    let frame = Duration::from_millis(common::FRAME_MS as u64);
    let mut hold = common::ProbeHold {
        follow: true,
        ..Default::default()
    };
    let mut placed_at: Option<i64> = None;
    let mut tracker = EventTracker::new();
    let mut explodes = Vec::new();
    let mut seen_time = 0;
    for i in 0..3000i64 {
        now += frame;
        let since = placed_at.map(|at| (i - at) * common::FRAME_MS);
        if since.is_some_and(|ms| ms >= SETTLE_MS + TAP_MS + SEAT_MS) {
            break;
        }
        let tap = since.is_some_and(|ms| (SETTLE_MS..SETTLE_MS + TAP_MS).contains(&ms));
        // The probe holds the view from the use tap on.
        if tap {
            hold.follow = false;
        }
        if placed_at.is_some() && cl.state() == vcod_common::net::NetState::Active {
            let buttons = if tap { BUTTON_USE } else { 0 };
            cl.send_cmds(&hold.cmds(&cl, buttons));
        } else if let Some(c) = cl.send_frame(&UserCmd::default()) {
            hold.last = c.server_time;
        }
        for e in step(&mut sv, &q, &mut cl, now) {
            if let NetEvent::ServerCommand(tokens) = e {
                join.on_server_command(&tokens, &mut cl, now);
            }
        }
        if let Some(s) = cl.snapshots().newest()
            && s.server_time != seen_time
        {
            seen_time = s.server_time;
            for ev in tracker.drain(s, p) {
                if ev.event == EV_ROCKET_EXPLODE_NOMARKS
                    || ev.event == vcod_common::net::event_ids::EV_GRENADE_EXPLODE
                {
                    explodes.push((s.server_time, ev));
                }
            }
        }
        if placed_at.is_none() && sv.script_log().iter().any(|l| l.contains("PROBE place")) {
            placed_at = Some(i);
        }
    }
    Some((sv.script_log().to_vec(), explodes))
}

/// The retail capture's explodes in 1.1's numbering.
fn retail_explodes() -> Vec<(i32, GameEvent)> {
    let p = &PROTOCOL_V22;
    let frames = vcod_common::testing::fixture_frames(
        p,
        "net/gamestate-uo-shellveh.bin",
        "net/snapshots-uo-shellveh.bin",
    );
    let mut tracker = EventTracker::new();
    let mut out = Vec::new();
    for f in &frames {
        for ev in tracker.drain(&f.snapshot, p) {
            if ev.event == EV_ROCKET_EXPLODE_NOMARKS
                || ev.event == vcod_common::net::event_ids::EV_GRENADE_EXPLODE
            {
                out.push((f.snapshot.server_time, ev));
            }
        }
    }
    out
}

/// A `PROBE vdamage` line's amount, means of death, point, direction,
/// attacker and inflictor.
type Notify = (i32, String, [f32; 3], [f32; 3], i32, i32);

fn vec3(s: &str) -> [f32; 3] {
    let v: Vec<f32> = s
        .trim_matches(|c| c == '(' || c == ')')
        .split(',')
        .map(|x| x.trim().parse().unwrap())
        .collect();
    [v[0], v[1], v[2]]
}

fn notifies(log: &[String]) -> Vec<Notify> {
    log.iter()
        .filter_map(|l| l.trim().strip_prefix("PROBE vdamage ").map(str::to_string))
        .map(|l| {
            // `<time> <amount> <mod> (x, y, z) (x, y, z) <attacker> <inflictor> <health>`
            let (head, rest) = l.split_once(" (").unwrap();
            let h: Vec<&str> = head.split(' ').collect();
            let (point, rest) = rest.split_once(") (").unwrap();
            let (dir, tail) = rest.split_once(") ").unwrap();
            let t: Vec<&str> = tail.split(' ').collect();
            (
                h[1].parse().unwrap(),
                h[2].to_string(),
                vec3(point),
                vec3(dir),
                t[0].parse().unwrap(),
                t[1].parse().unwrap(),
            )
        })
        .collect()
}

#[test]
fn a_shell_strikes_a_vehicle_as_on_uo() {
    let Some((log, ours_ex)) = ours() else {
        return;
    };
    let fixture = include_str!("fixtures/vehicles/mp_kharkov-tank-shell-uo.txt");
    let retail: Vec<String> = fixture.lines().map(str::to_string).collect();
    let retail_ex = retail_explodes();
    if std::env::var_os("SHELL_REPORT").is_some() {
        for l in log.iter().filter(|l| l.contains("PROBE")) {
            eprintln!("ours   {}", l.trim());
        }
        for (t, e) in &ours_ex {
            eprintln!(
                "ours   t {t} ev {} parm {} surf {} scale {} weapon {} eFlags {:#x} pos {:?}",
                e.event, e.parm, e.surf_type, e.scale, e.weapon, e.eflags, e.pos
            );
        }
        for (t, e) in &retail_ex {
            eprintln!(
                "retail t {t} ev {} parm {} surf {} scale {} weapon {} eFlags {:#x} pos {:?}",
                e.event, e.parm, e.surf_type, e.scale, e.weapon, e.eflags, e.pos
            );
        }
    }
    let (rn, on) = (notifies(&retail), notifies(&log));
    assert_eq!(
        on.len(),
        rn.len(),
        "the Elefant's notifies: retail {rn:?}, ours {on:?}"
    );
    for (o, r) in on.iter().zip(&rn) {
        assert_eq!(
            (o.0, &o.1, o.4, o.5),
            (r.0, &r.1, r.4, r.5),
            "amount, means, attacker, inflictor"
        );
        for i in 0..3 {
            assert!(
                (o.2[i] - r.2[i]).abs() < 4.0,
                "point: retail {:?}, ours {:?}",
                r.2,
                o.2
            );
            assert!(
                (o.3[i] - r.3[i]).abs() < 0.05,
                "dir: retail {:?}, ours {:?}",
                r.3,
                o.3
            );
        }
    }
    assert_eq!(ours_ex.len(), retail_ex.len(), "the explodes");
    for ((_, o), (_, r)) in ours_ex.iter().zip(&retail_ex) {
        assert_eq!(
            (o.event, o.parm, o.surf_type, o.scale, o.weapon, o.eflags),
            (r.event, r.parm, r.surf_type, r.scale, r.weapon, r.eflags),
            "the explode on the hull"
        );
    }
}
