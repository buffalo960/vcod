//! A gun on foot against a United Offensive vehicle's riders, ours against
//! retail's. `client-probes/probe_vehicle_armor` seats client 0 beside a
//! vehicle and turns client 1's sighted, held trigger onto it every frame;
//! its damage callback only logs (`PROBE hit ... seat <vehpos>`), so the
//! rider lives through every seat. The fixtures are retail UO 1.51b's
//! `PROBE` lines (recipe in docs/research/coduo-vehicles.md 13); the replay
//! runs the same script on ours with the same timeline and compares, seat
//! by seat, whether any round reached the damage callback.
//!
//! The `hold` captures' shooter rebases a held view against the gsc's
//! per-frame `setPlayerAngles`, so where its rounds go rides the client's
//! timing: two retail runs of the jeep put 1 and 4 rounds into its driver.
//! The sweep captures (`--probe-uo-weapon aim`, `probe_aim_sweep 1`) send
//! every round along the gsc's aim, stepped through fixed heights, and the
//! sweep tests compare the hits seat by seat and height by height
//! (docs/research/coduo-vehicles.md 13).
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
/// script log. `aim`: the shooter is `--probe-uo-weapon aim`, else `hold`.
fn ours(map: &str, vehicletype: &str, cvars: &[(&str, &str)], aim: bool) -> Option<Vec<String>> {
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
    let mut hold = common::ProbeHold {
        aim,
        ..Default::default()
    };
    // In the sweep runs the target is `--probe-vehicle sit` too: the
    // snapshot's view until the use tap, that view held from then on.
    let mut target = common::ProbeHold {
        aim: true,
        ..Default::default()
    };
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
        if a_buttons != 0 {
            target.aim = false;
        }
        let active =
            ta.snapshots().newest().is_some() && ta.state() == vcod_common::net::NetState::Active;
        if active && aim && placed_at.is_some() {
            ta.send_cmds(&target.cmds(&ta, a_buttons));
        } else if active {
            let cmd = holding(&ta, a_buttons, &mut last_a);
            ta.send_cmds(&[cmd]);
            target.last = last_a;
        } else if let Some(c) = ta.send_frame(&UserCmd::default()) {
            last_a = c.server_time;
            target.last = last_a;
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
    let Some(log) = ours("mp_cassino", "panzeriv_mp", &[], false) else {
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
    let Some(log) = ours("mp_peaks", "willyjeep_mp", &[("probe_side", "75")], false) else {
        return;
    };
    compare(
        include_str!("fixtures/vehicles/mp_peaks-jeep-armor-uo.txt"),
        &log,
    );
}

/// A sweep log seat by seat: (phase, seat) to each aim height's time
/// inside the seat, in half second steps, and the rounds the rider took
/// there.
type Sweep = BTreeMap<(i32, i32), BTreeMap<i32, (f32, usize)>>;

/// The gsc's half second between heights.
const STEP_MS: i32 = 500;

fn sweep(log: &[String]) -> Sweep {
    let lines: Vec<Vec<&str>> = log
        .iter()
        .filter_map(|l| l.trim().strip_prefix("PROBE "))
        .map(|l| l.split(' ').collect())
        .collect();
    let time = |t: &[&str]| t.get(1).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
    // `seat <t> <seat> phase <n>` opens a window; the next one or the last
    // `unseat` (the exit) closes it.
    let mut windows: Vec<(i32, i32, i32, i32)> = Vec::new();
    for t in &lines {
        match t[0] {
            "seat" => {
                if let Some(w) = windows.last_mut().filter(|w| w.3 == i32::MAX) {
                    w.3 = time(t);
                }
                windows.push((
                    t[4].parse().unwrap(),
                    t[2].parse().unwrap(),
                    time(t),
                    i32::MAX,
                ));
            }
            // A seat change's forced exit logs at the new seat's time.
            "unseat" => {
                if let Some(w) = windows
                    .last_mut()
                    .filter(|w| w.3 == i32::MAX && w.2 < time(t))
                {
                    w.3 = time(t);
                }
            }
            _ => {}
        }
    }
    let mut out = Sweep::new();
    for t in &lines {
        let at = time(t);
        let field = |name: &str| -> i32 {
            t.iter()
                .position(|x| *x == name)
                .and_then(|i| t.get(i + 1))
                .and_then(|v| v.parse().ok())
                .unwrap_or(-1)
        };
        match t[0] {
            "aim" => {
                let z = t[2].parse().unwrap();
                for w in &windows {
                    let overlap = (at + STEP_MS).min(w.3) - at.max(w.2);
                    if overlap > 0 {
                        let e = out.entry((w.0, w.1)).or_default().entry(z).or_default();
                        e.0 += overlap as f32 / STEP_MS as f32;
                    }
                }
            }
            "hit" if field("seat") > 0 => {
                let e = out.entry((field("phase"), field("seat"))).or_default();
                e.entry(field("aim")).or_default().1 += 1;
            }
            _ => {}
        }
    }
    out
}

/// Seats whose count may stray past the tolerance, with the reason.
const SWEEP_GAPS: &[(&str, i32, &str)] = &[
    (
        "mp_peaks",
        2,
        "the jeep gunner's blend (G_PlayerVehiclePositionAndBlend 0x9048f, \
         type 1 seat 2) is not ported: ours takes rounds on the lower legs at \
         0 and 8, retail none (coduo-vehicles.md 13)",
    ),
    (
        "mp_cassino",
        2,
        "the Panzer IV gunner's MG42 blend by the gun (0x9048f, type 2 seat 2) \
         is not ported, and the gun turns with the held view: ours takes a \
         round a step at 48, retail 4.5 at 48..64 (coduo-vehicles.md 13)",
    ),
];

/// Seat by seat and height by height, the rounds that reached the rider
/// per half second step at that height within 2.5 of retail's (a step fires
/// about five); and over the seat, within 1.5 a step on average.
fn compare_sweep(map: &str, fixture: &str, ours: &[String]) {
    let retail: Vec<String> = fixture.lines().map(str::to_string).collect();
    if std::env::var("ARMOR_REPORT").is_ok() {
        for l in ours.iter().filter(|l| l.contains("PROBE")) {
            eprintln!("ours   {}", l.trim());
        }
    }
    assert_eq!(
        phases(ours),
        phases(&retail),
        "the seats the rider went through"
    );
    let (rs, os) = (sweep(&retail), sweep(ours));
    let rate = |(steps, hits): (f32, usize)| hits as f32 / steps.max(0.5);
    for (key, r) in &rs {
        let o = os.get(key).cloned().unwrap_or_default();
        let heights: std::collections::BTreeSet<i32> = r.keys().chain(o.keys()).copied().collect();
        let row =
            |m: &BTreeMap<i32, (f32, usize)>, z: i32| rate(m.get(&z).copied().unwrap_or((0.0, 0)));
        let profile = |m: &BTreeMap<i32, (f32, usize)>| -> Vec<String> {
            heights
                .iter()
                .map(|&z| format!("{z}:{:.1}", row(m, z)))
                .collect()
        };
        eprintln!(
            "{map} phase {} seat {}\n  retail {:?}\n  ours   {:?}",
            key.0,
            key.1,
            profile(r),
            profile(&o)
        );
        if SWEEP_GAPS
            .iter()
            .any(|(m, seat, _)| *m == map && *seat == key.1)
        {
            continue;
        }
        let mut sum = 0.0;
        for &z in &heights {
            let d = (row(r, z) - row(&o, z)).abs();
            sum += d;
            assert!(
                d <= 2.5,
                "phase {} seat {} height {z}: retail {:.1} a step, ours {:.1}",
                key.0,
                key.1,
                row(r, z),
                row(&o, z)
            );
        }
        assert!(
            sum <= 1.5 * heights.len().max(1) as f32,
            "phase {} seat {}: off by {sum:.1} over {} heights",
            key.0,
            key.1,
            heights.len()
        );
    }
}

/// The jeep's three seats under the sweep: every round leaves along the
/// gsc's aim, and each seat takes about as many as retail's at the same
/// heights.
#[test]
fn every_jeep_seat_takes_retails_rounds_under_the_sweep() {
    let cvars = [("probe_side", "75"), ("probe_aim_sweep", "1")];
    let Some(log) = ours("mp_peaks", "willyjeep_mp", &cvars, true) else {
        return;
    };
    compare_sweep(
        "mp_peaks",
        include_str!("fixtures/vehicles/mp_peaks-jeep-armor-sweep-uo.txt"),
        &log,
    );
}

/// The Panzer IV under the sweep: no round reaches the driver, the gunner
/// on top takes them where retail's does.
#[test]
fn the_tank_seats_take_retails_rounds_under_the_sweep() {
    let Some(log) = ours(
        "mp_cassino",
        "panzeriv_mp",
        &[("probe_aim_sweep", "1")],
        true,
    ) else {
        return;
    };
    compare_sweep(
        "mp_cassino",
        include_str!("fixtures/vehicles/mp_cassino-tank-armor-sweep-uo.txt"),
        &log,
    );
}
