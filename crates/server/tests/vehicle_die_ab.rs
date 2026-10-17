//! A United Offensive vehicle's death with riders in it, on ours against
//! retail's (`fixtures/vehicles/*-die-*-uo.txt`, `client-probes/probe_vehdie`,
//! recipes in their headers): `Scr_Vehicle_Die`'s 5-point probe hit on each
//! rider through the damage callback, who is put out where he sits and who
//! looks for a dismount spot, and the stock death scripts after it
//! (docs/research/coduo-vehicles.md 11.2). Ours runs the same probe with
//! two clients joined as axis, boarding by the use key.
//!
//! `DIE_REPORT=1` prints both sides' lines. Needs `COD_DIR` at a UO
//! install; without one it returns early.

mod common;

use common::{ClientEnd, Queues, step_pair};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::NetClient;
use vcod_common::net::NetEvent;
use vcod_common::net::msg::{BUTTON_USE, UserCmd};
use vcod_common::net::protocol::PROTOCOL_V22;

/// A cmd holding the view the probe's placement gave client `n`: 10
/// degrees down, facing the hull, as retail's probe clients hold it from
/// their use tap on, seated or not.
fn facing(cl: &NetClient<ClientEnd>, yaw: f32, buttons: u8) -> Option<UserCmd> {
    let p = &PROTOCOL_V22;
    let s = cl.snapshots().newest()?;
    let v = [10.0, yaw, 0.0];
    let da =
        ["delta_angles[0]", "delta_angles[1]", "delta_angles[2]"].map(|n| s.ps.field_i32(p, n));
    Some(UserCmd {
        server_time: s.server_time,
        angles: std::array::from_fn(|i| ((v[i] * 65536.0 / 360.0).round() as i32 - da[i]) & 0xffff),
        buttons,
        weapon: s.ps.field_i32(p, "weapon") as u8,
        ..UserCmd::default()
    })
}

/// The yaw the probe's `PROBE place <t> <n> <spot> <yaw>` line gave client
/// `n`.
fn placed_yaw(log: &[String], n: usize) -> Option<f32> {
    log.iter()
        .filter(|l| l.starts_with("PROBE place "))
        .find(|l| l.split(' ').nth(3) == Some(&n.to_string()))
        .and_then(|l| l.split_whitespace().last()?.parse().ok())
}

fn seated(cl: &NetClient<ClientEnd>) -> bool {
    cl.snapshots().newest().is_some_and(|s| {
        s.ps.field_i32(&PROTOCOL_V22, "eFlags") & vcod_common::net::flags::EF_IN_VEHICLE_V22 != 0
    })
}

/// Our `PROBE` lines: `probe_vehdie` on `map` with `cvars`, client 0
/// boarding 2 s after its placement and client 1 (when `riders` is 2) 2 s
/// after client 0 sits, run on until 3 s past the kill.
fn ours(map: &str, cvars: &[(&str, &str)], riders: usize) -> Option<Vec<String>> {
    let fs = vcod_common::testing::uo_fs()?;
    let p = &PROTOCOL_V22;
    let bsp_path = fs.resolve_map(map).expect("the map in the UO paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(map, "probe_vehdie"), now);
    sv.set_protocol(p);
    sv.overlay_script(
        "maps/mp/gametypes/probe_vehdie",
        include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_vehdie.gsc"),
    );
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
    let (mut ca, mut cb, mut ja, mut jb) = common::join_pair_logged(
        &mut sv,
        &qa,
        &qb,
        &mut now,
        ("axis", "kar98k_mp"),
        ("axis", "kar98k_mp"),
    );
    let frame = Duration::from_millis(common::FRAME_MS as u64);
    let (mut since_a, mut since_b) = (None::<i64>, None::<i64>);
    let mut killed = None::<i64>;
    let mut last = [0i32; 2];
    for i in 0..3000i64 {
        now += frame;
        if since_a.is_none() && placed_yaw(sv.script_log(), 0).is_some() {
            since_a = Some(i);
        }
        if since_b.is_none() && seated(&ca) {
            since_b = Some(i);
        }
        let tap = |since: Option<i64>| since.is_some_and(|at| (40..43).contains(&(i - at)));
        let taps = [tap(since_a), riders == 2 && tap(since_b)];
        if std::env::var_os("DIE_DEBUG").is_some()
            && (taps[0] || taps[1] || since_a.is_some_and(|a| i - a < 4))
        {
            for cl in [&ca, &cb] {
                if let Some(s) = cl.snapshots().newest() {
                    eprintln!(
                        "tap i {i} t {} ct {} origin {:?} view {:?} eflags {:x} pmt {}",
                        s.server_time,
                        s.ps.field_i32(p, "commandTime"),
                        s.ps.origin(p),
                        s.ps.viewangles(p),
                        s.ps.field_i32(p, "eFlags"),
                        s.ps.field_i32(p, "pm_type")
                    );
                }
            }
        }
        for (k, cl) in [&mut ca, &mut cb].into_iter().enumerate() {
            let buttons = if taps[k] { BUTTON_USE } else { 0 };
            let cmd = placed_yaw(sv.script_log(), k).and_then(|yaw| facing(cl, yaw, buttons));
            match cmd {
                Some(mut c) => {
                    c.server_time = c.server_time.max(last[k] + 1);
                    last[k] = c.server_time;
                    cl.send_cmds(&[c]);
                }
                None => {
                    cl.send_frame(&UserCmd::default());
                }
            }
        }
        let (ea, eb) = step_pair(&mut sv, (&qa, &mut ca), (&qb, &mut cb), now);
        for (events, join, cl) in [(ea, &mut ja, &mut ca), (eb, &mut jb, &mut cb)] {
            for e in events {
                if let NetEvent::ServerCommand(tokens) = e {
                    join.on_server_command(&tokens, cl, now);
                }
            }
        }
        if killed.is_none() && sv.script_log().iter().any(|l| l.starts_with("PROBE kill ")) {
            killed = Some(i);
        }
        if std::env::var_os("DIE_DEBUG").is_some() && killed.is_some_and(|k| i - k < 12) {
            for cl in [&ca, &cb] {
                if let Some(s) = cl.snapshots().newest() {
                    eprintln!(
                        "t {} ct {} pmt {} origin {:?} vel {:?}",
                        s.server_time,
                        s.ps.field_i32(p, "commandTime"),
                        s.ps.field_i32(p, "pm_type"),
                        s.ps.origin(p),
                        ["velocity[0]", "velocity[1]", "velocity[2]"]
                            .map(|n| f32::from_bits(s.ps.field_i32(p, n) as u32)),
                    );
                }
            }
        }
        if killed.is_some_and(|k| i - k > 60) {
            break;
        }
    }
    assert!(
        killed.is_some(),
        "the vehicle was never killed:\n{}",
        sv.script_log().join("")
    );
    Some(
        sv.script_log()
            .iter()
            .filter(|l| l.starts_with("PROBE "))
            .map(|l| l.trim_end().to_string())
            .collect(),
    )
}

fn retail(path: &str) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| l.starts_with("PROBE "))
        .map(str::to_string)
        .collect()
}

/// The lines from the kill on, within a second of it, each time made
/// relative to the kill's.
fn from_kill(lines: &[String]) -> Vec<String> {
    let time = |l: &str| l.split(' ').nth(2).and_then(|t| t.parse::<i32>().ok());
    let Some(kill) = lines
        .iter()
        .find(|l| l.starts_with("PROBE kill "))
        .and_then(|l| time(l))
    else {
        return Vec::new();
    };
    lines
        .iter()
        .skip_while(|l| !l.starts_with("PROBE kill "))
        .filter_map(|l| {
            let t = time(l)?;
            (t - kill <= 1000).then(|| {
                let mut w: Vec<String> = l.split(' ').map(str::to_string).collect();
                w[2] = format!("+{}", t - kill);
                w.join(" ")
            })
        })
        .collect()
}

/// The client a line is about: a hit's victim, a rider line's rider.
fn subject(l: &str) -> Option<&str> {
    let w: Vec<&str> = l.split(' ').collect();
    match w.get(1) {
        Some(&"dmg") | Some(&"deact") => w.get(3).copied(),
        Some(&"rider") => w.get(4).copied(),
        _ => None,
    }
}

/// How far a number about the gunner may sit off retail's. His body rides
/// the gun, which turns from the seat's view tag to his view; the kill
/// comes 50 ms after his seat, mid-turn, and retail's three runs put him
/// 3.3 units apart at it.
const GUNNER_TOL: f32 = 4.0;

/// A dismounted rider's fall a frame on: the probe client's cmds cover
/// part of the frame on retail and all of it on ours, so the fall and the
/// knockback run apart by up to 2.2 units.
const CMD_CLOCK: &str = "the probe client's cmd clock";

/// A rider killed where he sat: retail's corpse slides off the hull, ours
/// stays where the seat left it, its velocity gone on the first cmd. Open
/// (coduo-vehicles.md 11.2).
const STUCK_CORPSE: &str = "a corpse left in a seat's place does not slide";

/// Retail's lines against ours from the kill on, one for one, numbers
/// exact but the gunner's ([`GUNNER_TOL`]). A line holding one of
/// `drift`'s headers (`frame 0`: the frame line of rider 0) is let differ
/// for its reason, and must still differ; everything else must match.
fn compare(retail: &[String], ours: &[String], drift: &[(&str, &str)]) {
    let (r, o) = (from_kill(retail), from_kill(ours));
    if std::env::var_os("DIE_REPORT").is_some() {
        for l in &r {
            eprintln!("retail {l}");
        }
        for l in &o {
            eprintln!("ours   {l}");
        }
    }
    assert_eq!(r.len(), o.len(), "line counts\nretail {r:#?}\nours {o:#?}");
    let mut drifted = Vec::new();
    for (a, b) in r.iter().zip(&o) {
        let tol = if subject(a) == Some("1") {
            GUNNER_TOL
        } else {
            0.0
        };
        let words = |l: &str| -> Vec<String> {
            l.split([' ', '(', ')', ','])
                .filter(|w| !w.is_empty())
                .map(str::to_string)
                .collect()
        };
        let (wa, wb) = (words(a), words(b));
        let same = wa.len() == wb.len()
            && wa.iter().zip(&wb).all(|(x, y)| {
                // `inf`, the inflictor's tag, parses as a float too.
                let num = |w: &str| w.parse::<f32>().ok().filter(|v| v.is_finite());
                match (num(x), num(y)) {
                    (Some(x), Some(y)) => (x - y).abs() <= tol,
                    _ => x == y,
                }
            });
        if same {
            continue;
        }
        let d = drift.iter().find(|(h, _)| a.contains(h));
        assert!(d.is_some(), "retail {a}\nours   {b}");
        drifted.push(d.unwrap().0);
    }
    for (h, why) in drift {
        assert!(
            drifted.contains(h),
            "{h} now matches retail; drop it from the drift ({why})"
        );
    }
}

#[test]
fn the_tank_killed_by_itself_puts_both_riders_out_where_they_sit() {
    let r = retail("tests/fixtures/vehicles/mp_cassino-tank-die-self-uo.txt");
    let Some(o) = ours("mp_cassino", &[("probe_vehicletype", "panzeriv_mp")], 2) else {
        return;
    };
    compare(
        &r,
        &o,
        &[
            ("frame 0", STUCK_CORPSE),
            ("later 0", STUCK_CORPSE),
            ("frame 1", STUCK_CORPSE),
            ("later 1", STUCK_CORPSE),
        ],
    );
}

#[test]
fn the_tank_killed_by_its_gunner_dismounts_the_driver() {
    let r = retail("tests/fixtures/vehicles/mp_cassino-tank-die-gunner-uo.txt");
    let cvars = [
        ("probe_vehicletype", "panzeriv_mp"),
        ("probe_die_by", "gunner"),
    ];
    let Some(o) = ours("mp_cassino", &cvars, 2) else {
        return;
    };
    compare(
        &r,
        &o,
        &[
            ("frame 0", CMD_CLOCK),
            ("later 0", CMD_CLOCK),
            ("later 1", STUCK_CORPSE),
        ],
    );
}

#[test]
fn the_jeep_killed_by_a_teammate_dismounts_its_driver() {
    let r = retail("tests/fixtures/vehicles/mp_peaks-jeep-die-teammate-uo.txt");
    let cvars = [
        ("probe_vehicletype", "willyjeep_mp"),
        ("probe_die_by", "gunner"),
        ("probe_side", "75"),
        ("scr_friendlyfire", "0"),
        ("probe_riders", "1"),
    ];
    let Some(o) = ours("mp_peaks", &cvars, 1) else {
        return;
    };
    compare(&r, &o, &[("frame 0", CMD_CLOCK)]);
}
