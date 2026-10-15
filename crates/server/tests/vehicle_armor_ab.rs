//! A gun on foot against a United Offensive vehicle's riders, ours against
//! retail's. `client-probes/probe_vehicle_armor` seats client 0 beside a
//! vehicle and turns client 1's sighted, held trigger onto it every frame;
//! its damage callback only logs (`PROBE hit ... seat <vehpos>`), so the
//! rider lives through every seat. The fixtures are retail UO 1.51b's
//! `PROBE` lines (recipe in docs/research/coduo-vehicles.md 13); the replay
//! runs the same script on ours with the same timeline and compares, seat
//! by seat, whether any round reached the damage callback.
//!
//! `ARMOR_REPORT=1` prints both logs. Needs `COD_DIR` at a UO install;
//! without one it returns early.

mod common;

use common::{Join, Queues, step_pair};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::NetClient;
use vcod_common::net::NetEvent;
use vcod_common::net::msg::{BUTTON_ADS, BUTTON_ATTACK, BUTTON_USE, UserCmd};
use vcod_common::net::protocol::PROTOCOL_V22;

/// `vehicle_probe.rs` `SIT`, from the target's placement: on foot, the use
/// tap, each seat for 12 s, out.
const SETTLE_MS: i64 = 10_000;
const SEAT_MS: i64 = 12_000;
const TAP_MS: i64 = 150;

/// Hits by (phase, seat): `phase` counts the rider's seat changes, so a
/// seat met twice (a tank's driver after the cycle wraps) is two groups.
fn hits(log: &[String]) -> BTreeMap<(i32, i32), Vec<String>> {
    let mut out: BTreeMap<(i32, i32), Vec<String>> = BTreeMap::new();
    for l in log {
        let Some(rest) = l.trim().strip_prefix("PROBE hit ") else {
            continue;
        };
        let t: Vec<&str> = rest.split(' ').collect();
        let field = |name: &str| {
            t.iter()
                .position(|x| *x == name)
                .and_then(|i| t.get(i + 1))
                .copied()
                .unwrap_or("")
        };
        let key = (
            field("phase").parse().unwrap(),
            field("seat").parse().unwrap(),
        );
        out.entry(key)
            .or_default()
            .push(format!("{} {}", field("mod"), field("dmg")));
    }
    out
}

/// The groups a hit can fall in, in order: on foot before the use tap,
/// each seat as `activated` named it, on foot after the exit.
fn phases(log: &[String]) -> Vec<(i32, i32)> {
    let mut out = vec![(0, 0)];
    for l in log {
        if let Some(rest) = l.trim().strip_prefix("PROBE seat ") {
            let t: Vec<&str> = rest.split(' ').collect();
            out.push((t[3].parse().unwrap(), t[1].parse().unwrap()));
        }
    }
    let last = out.last().unwrap().0;
    out.push((last, 0));
    out
}

/// A cmd that holds the view the newest snapshot has (the gsc turns the
/// shooter's), stamped past the last one sent.
fn holding(cl: &NetClient<common::ClientEnd>, buttons: u8, last: &mut i32) -> UserCmd {
    let p = &PROTOCOL_V22;
    let Some(s) = cl.snapshots().newest() else {
        return UserCmd::default();
    };
    let v = s.ps.viewangles(p);
    let da =
        ["delta_angles[0]", "delta_angles[1]", "delta_angles[2]"].map(|n| s.ps.field_i32(p, n));
    let st = s.server_time.max(*last + 1);
    *last = st;
    UserCmd {
        server_time: st,
        angles: std::array::from_fn(|i| ((v[i] * 65536.0 / 360.0).round() as i32 - da[i]) & 0xffff),
        buttons,
        weapon: s.ps.field_i32(p, "weapon") as u8,
        ..UserCmd::default()
    }
}

/// Our run of the probe on `map`, the target boarding a `vehicletype`; the
/// script log.
fn ours(map: &str, vehicletype: &str, cvars: &[(&str, &str)]) -> Option<Vec<String>> {
    let fs = vcod_common::testing::uo_fs()?;
    let bsp_path = fs.resolve_map(map).expect("map in the UO paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(map, "probe_vehicle_armor"), now);
    sv.set_protocol(&PROTOCOL_V22);
    sv.overlay_script(
        "maps/mp/gametypes/probe_vehicle_armor",
        include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_vehicle_armor.gsc"),
    );
    sv.set_cvar("probe_vehicletype", vehicletype);
    for (k, v) in cvars {
        sv.set_cvar(k, v);
    }
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    common::run_alone(&mut sv, &mut now, common::PROBE_JOIN_MS);
    let (qa, qb) = (
        Rc::new(RefCell::new(Queues::default())),
        Rc::new(RefCell::new(Queues::default())),
    );
    let mut ta = NetClient::start_with_qport(common::ClientEnd(qa.clone()), now, 0x2001);
    let mut sh = NetClient::start_with_qport(common::ClientEnd(qb.clone()), now, 0x2002);
    let (mut ja, mut jb) = (
        Join::new("axis", "kar98k_mp"),
        Join::new("allies", "thompson_mp"),
    );
    let frame = Duration::from_millis(common::FRAME_MS as u64);
    let mut last_a = 0;
    let mut hold = common::ProbeHold::default();
    let mut shooter_placed = false;
    let mut placed_at: Option<i64> = None;
    let mut said = 0;
    for i in 0..4000i64 {
        now += frame;
        let since = placed_at.map(|at| (i - at) * common::FRAME_MS);
        let mut a_buttons = 0;
        if let Some(ms) = since {
            // The use taps: the entry, and after both seat changes the exit.
            let exit = SETTLE_MS + TAP_MS + 3 * SEAT_MS;
            if (SETTLE_MS..SETTLE_MS + TAP_MS).contains(&ms) || (exit..exit + TAP_MS).contains(&ms)
            {
                a_buttons = BUTTON_USE;
            }
            let change = SETTLE_MS + TAP_MS + (said + 1) * SEAT_MS;
            if said < 2 && ms >= change {
                ta.send_reliable("nextvehslot");
                said += 1;
            }
            if ms >= SETTLE_MS + 2 * TAP_MS + 4 * SEAT_MS {
                break;
            }
        }
        if ta.snapshots().newest().is_some() && ta.state() == vcod_common::net::NetState::Active {
            let cmd = holding(&ta, a_buttons, &mut last_a);
            ta.send_cmds(&[cmd]);
        } else if let Some(c) = ta.send_frame(&UserCmd::default()) {
            last_a = c.server_time;
        }
        // The shooter is `--probe-uo-weapon hold` from its placement on.
        if shooter_placed && sh.state() == vcod_common::net::NetState::Active {
            sh.send_cmds(&hold.cmds(&sh, BUTTON_ATTACK | BUTTON_ADS));
        } else if let Some(c) = sh.send_frame(&UserCmd::default()) {
            hold.last = c.server_time;
        }
        let (ea, eb) = step_pair(&mut sv, (&qa, &mut ta), (&qb, &mut sh), now);
        for (events, join, cl) in [(ea, &mut ja, &mut ta), (eb, &mut jb, &mut sh)] {
            for e in events {
                if let NetEvent::ServerCommand(tokens) = e {
                    join.on_server_command(&tokens, cl, now);
                }
            }
        }
        if placed_at.is_none()
            && sv
                .script_log()
                .iter()
                .any(|l| l.contains("PROBE place") && l.trim().split(' ').nth(3) == Some("0"))
        {
            placed_at = Some(i);
        }
        shooter_placed = shooter_placed
            || sv
                .script_log()
                .iter()
                .any(|l| l.contains("PROBE place") && l.trim().split(' ').nth(3) == Some("1"));
    }
    Some(sv.script_log().to_vec())
}

fn compare(fixture: &str, ours: &[String]) {
    let retail: Vec<String> = fixture.lines().map(str::to_string).collect();
    if std::env::var("ARMOR_REPORT").is_ok() {
        for l in ours.iter().filter(|l| l.contains("PROBE")) {
            eprintln!("ours   {}", l.trim());
        }
        for l in &retail {
            eprintln!("retail {l}");
        }
    }
    let (rp, op) = (phases(&retail), phases(ours));
    assert_eq!(op, rp, "the seats the rider went through");
    let (rh, oh) = (hits(&retail), hits(ours));
    for key in rp {
        let (r, o) = (rh.get(&key), oh.get(&key));
        assert_eq!(
            o.is_some(),
            r.is_some(),
            "phase {} seat {}: retail {} hit(s) {:?}, ours {} {:?}",
            key.0,
            key.1,
            r.map_or(0, Vec::len),
            r.map(|v| &v[..v.len().min(4)]),
            o.map_or(0, Vec::len),
            o.map(|v| &v[..v.len().min(4)]),
        );
    }
}

/// mp_cassino's first Panzer IV: on foot the rounds land, the driver's seat
/// takes none, the gunner's on top does.
#[test]
fn a_tank_driver_takes_no_rounds_from_a_gun_on_foot() {
    let Some(log) = ours("mp_cassino", "panzeriv_mp", &[]) else {
        return;
    };
    compare(
        include_str!("fixtures/vehicles/mp_cassino-tank-armor-uo.txt"),
        &log,
    );
}

/// mp_peaks' first Willys: every seat is in the open, so rounds reach the
/// driver, the gunner and the passenger alike.
#[test]
fn every_jeep_seat_takes_rounds_from_a_gun_on_foot() {
    let Some(log) = ours("mp_peaks", "willyjeep_mp", &[("probe_side", "75")]) else {
        return;
    };
    compare(
        include_str!("fixtures/vehicles/mp_peaks-jeep-armor-uo.txt"),
        &log,
    );
}
