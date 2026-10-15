//! A United Offensive vehicle driven into another, ours against retail's.
//! `client-probes/probe_vehicle_ram` seats client 0 in mp_kharkov's second
//! Horch, which `--probe-vehicle ram` drives straight ahead for 6 s into
//! the Panzer IV parked 680 units in front; the probe logs both vehicles'
//! poses every frame (`mp_kharkov-jeep-ram-uo.txt`, retail UO 1.51b). The
//! Horch glances off the tank's collmap and shoves it a few units
//! (0x827ab's `script_vehicle` arm, docs/research/coduo-vehicles.md 12).
//!
//! `RAM_REPORT=1` prints both sides' poses. Needs `COD_DIR` at a UO
//! install; without one it returns early.

mod common;

use common::{Join, Queues, step};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::NetEvent;
use vcod_common::net::msg::{BUTTON_USE, UserCmd};
use vcod_common::net::protocol::PROTOCOL_V22;

const MAP: &str = "mp_kharkov";
/// `vehicle_probe.rs` `RAM`, from the placement.
const SETTLE_MS: i64 = 2000;
const TAP_MS: i64 = 150;
const SEATED_MS: i64 = 2000;
const AHEAD_MS: i64 = 6000;
const STOP_MS: i64 = 3000;

fn ours() -> Option<Vec<String>> {
    let fs = vcod_common::testing::uo_fs()?;
    let bsp_path = fs.resolve_map(MAP).expect("map in the UO paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(MAP, "probe_vehicle_ram"), now);
    sv.set_protocol(&PROTOCOL_V22);
    sv.overlay_script(
        "maps/mp/gametypes/probe_vehicle_ram",
        include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_vehicle_ram.gsc"),
    );
    for (k, v) in [
        ("probe_vehicletype", "horch_mp"),
        ("probe_index", "1"),
        ("probe_ahead", "680"),
    ] {
        sv.set_cvar(k, v);
    }
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
    for i in 0..3000i64 {
        now += frame;
        let since = placed_at.map(|at| (i - at) * common::FRAME_MS);
        let ahead = SETTLE_MS + TAP_MS + SEATED_MS;
        if since.is_some_and(|ms| ms >= ahead + AHEAD_MS + STOP_MS) {
            break;
        }
        let tap = since.is_some_and(|ms| (SETTLE_MS..SETTLE_MS + TAP_MS).contains(&ms));
        if tap {
            hold.follow = false;
        }
        hold.forward = if since.is_some_and(|ms| (ahead..ahead + AHEAD_MS).contains(&ms)) {
            127
        } else {
            0
        };
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
        if placed_at.is_none() && sv.script_log().iter().any(|l| l.contains("PROBE place")) {
            placed_at = Some(i);
        }
    }
    Some(sv.script_log().to_vec())
}

fn vec3(s: &str) -> [f32; 3] {
    let v: Vec<f32> = s
        .trim_matches(|c| c == '(' || c == ')')
        .split(',')
        .map(|x| x.trim().parse().unwrap())
        .collect();
    [v[0], v[1], v[2]]
}

/// One vehicle's `(time past the placement, origin, angles)` lines.
fn poses(log: &[String], which: &str) -> Vec<(i32, [f32; 3], [f32; 3])> {
    let placed: i32 = log
        .iter()
        .find_map(|l| l.trim().strip_prefix("PROBE place "))
        .and_then(|r| r.split(' ').next()?.parse().ok())
        .expect("the driver was placed");
    log.iter()
        .filter_map(|l| {
            l.trim()
                .strip_prefix(&format!("PROBE {which} "))
                .map(str::to_string)
        })
        .map(|l| {
            let (t, rest) = l.split_once(" (").unwrap();
            let (o, rest) = rest.split_once(") (").unwrap();
            let a = rest.split(')').next().unwrap();
            (t.parse::<i32>().unwrap() - placed, vec3(o), vec3(a))
        })
        .collect()
}

#[test]
fn a_jeep_driven_into_a_tank_shoves_it_as_on_uo() {
    let Some(log) = ours() else {
        return;
    };
    let fixture = include_str!("fixtures/vehicles/mp_kharkov-jeep-ram-uo.txt");
    let retail: Vec<String> = fixture.lines().map(str::to_string).collect();
    let (ra, oa) = (poses(&retail, "a"), poses(&log, "a"));
    let (rb, ob) = (poses(&retail, "b"), poses(&log, "b"));
    if std::env::var_os("RAM_REPORT").is_some() {
        for (r, o) in ra.iter().zip(&oa).step_by(5) {
            eprintln!("a retail {:?} ours {:?}", r, o);
        }
        for (r, o) in rb.iter().zip(&ob).step_by(5) {
            eprintln!("b retail {:?} ours {:?}", r, o);
        }
    }
    let end = |v: &[(i32, [f32; 3], [f32; 3])]| v.last().copied().expect("poses");
    // The tank parked, then shoved: how far it went.
    let moved = |v: &[(i32, [f32; 3], [f32; 3])]| {
        let (a, b) = (v[0].1, end(v).1);
        ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
    };
    let (rm, om) = (moved(&rb), moved(&ob));
    assert!(rm > 1.0, "retail's tank moved {rm}");
    assert!(
        om > 0.5 * rm && om < 2.0 * rm,
        "the tank's shove: retail {rm}, ours {om}"
    );
}
