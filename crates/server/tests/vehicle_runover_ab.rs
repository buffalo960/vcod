//! A United Offensive jeep running over a player, ours against retail's
//! (`fixtures/vehicles/mp_kharkov-jeep-runover-{stand,prone}-uo.txt`,
//! `client-probes/probe_vehicle_ram` with `probe_victim`, recipes in their
//! headers): mp_kharkov's second Horch, driven straight ahead by
//! `--probe-vehicle ram`, meets client 0 150 units ahead of it.
//! `VEH_PlayerCollision`'s run-over damage, knock and `vehicle_hit_person`
//! (docs/research/coduo-vehicles.md 13), and a prone body under the hull
//! taking `ClientThink_real`'s 10 points a cmd (11.6).
//!
//! `RUNOVER_REPORT=1` prints both sides' lines, `RUNOVER_DUMP=<path>`
//! writes ours. Needs `COD_DIR` at a UO
//! install; without one it returns early.

mod common;

use common::{ClientEnd, Join, Queues, step_pair};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::event_ids::EV_SOUND_ALIAS;
use vcod_common::net::msg::{BUTTON_USE, UserCmd};
use vcod_common::net::protocol::PROTOCOL_V22;
use vcod_common::net::{NetClient, NetEvent, NetState};

const MAP: &str = "mp_kharkov";
/// `vehicle_probe.rs` `RAM`, from the driver's placement.
const SETTLE_MS: i64 = 2000;
const TAP_MS: i64 = 150;
const SEATED_MS: i64 = 2000;
const AHEAD_MS: i64 = 6000;

/// Our PROBE lines, and the `EV_SOUND_ALIAS` temp entities the victim's
/// snapshots carried, each once as `VEH <time> temp <n> ev 172 parm <p>
/// other 0 pos <x> <y> <z>` in the probe client's format. The victim is
/// allied, the driver on `driver_team`; `cvars` go on top of the probe's.
fn ours(victim: &str, driver_team: &str, cvars: &[(&str, &str)]) -> Option<Vec<String>> {
    let fs = vcod_common::testing::uo_fs()?;
    let p = &PROTOCOL_V22;
    let bsp_path = fs.resolve_map(MAP).expect("the map in the UO paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(MAP, "probe_vehicle_ram"), now);
    sv.set_protocol(p);
    sv.overlay_script(
        "maps/mp/gametypes/probe_vehicle_ram",
        include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_vehicle_ram.gsc"),
    );
    for (k, v) in [
        ("probe_vehicletype", "horch_mp"),
        ("probe_index", "1"),
        ("probe_ahead", "680"),
        ("probe_victim", victim),
        ("probe_victim_ahead", "150"),
    ]
    .iter()
    .chain(cvars)
    {
        sv.set_cvar(k, v);
    }
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    common::run_alone(&mut sv, &mut now, common::PROBE_JOIN_MS);
    let (qa, qb) = (
        Rc::new(RefCell::new(Queues::default())),
        Rc::new(RefCell::new(Queues::default())),
    );
    // The victim connects first, so he is client 0.
    let mut ca = NetClient::start_with_qport(ClientEnd(qa.clone()), now, 0x2001);
    let mut cb = NetClient::start_with_qport(ClientEnd(qb.clone()), now, 0x2002);
    let mut ja = Join::new("allies", "mosin_nagant_mp");
    let weapon = if driver_team == "axis" {
        "kar98k_mp"
    } else {
        "mosin_nagant_mp"
    };
    let mut jb = Join::new(driver_team, weapon);
    let frame = Duration::from_millis(common::FRAME_MS as u64);
    // The victim, `--probe-uo-weapon direct`, holds the view his placement
    // gave him, three cmds a frame; the driver, `--probe-vehicle ram`,
    // holds his from the use tap on. Each takes it off the placement's
    // line: the join's cmds, sent until then, keep the world view at 0.
    let mut victim_hold = common::ProbeHold {
        framed: true,
        ..Default::default()
    };
    let mut hold = common::ProbeHold::default();
    let prone = victim == "prone";
    let mut placed_at: Option<i64> = None;
    let mut sounds = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for i in 0..3000i64 {
        now += frame;
        let since = placed_at.map(|at| (i - at) * common::FRAME_MS);
        let ahead = SETTLE_MS + TAP_MS + SEATED_MS;
        if since.is_some_and(|ms| ms >= ahead + AHEAD_MS) {
            break;
        }
        let tap = since.is_some_and(|ms| (SETTLE_MS..SETTLE_MS + TAP_MS).contains(&ms));
        hold.forward = if since.is_some_and(|ms| (ahead..ahead + AHEAD_MS).contains(&ms)) {
            127
        } else {
            0
        };
        if victim_hold.base.is_none() {
            victim_hold.base = placed_yaw(sv.script_log(), "PROBE victim ").map(|y| [0.0, y, 0.0]);
        }
        if hold.base.is_none() {
            hold.base = placed_yaw(sv.script_log(), "PROBE place ").map(|y| [10.0, y, 0.0]);
        }
        for (cl, h, buttons) in [
            (&mut ca, &mut victim_hold, 0),
            (&mut cb, &mut hold, if tap { BUTTON_USE } else { 0 }),
        ] {
            if h.base.is_none() || cl.state() != NetState::Active {
                if let Some(c) = cl.send_frame(&UserCmd::default()) {
                    h.last = c.server_time;
                }
                continue;
            }
            let mut cmds = h.cmds(cl, buttons);
            if prone && h.framed {
                for c in &mut cmds {
                    c.wbuttons = 0x40;
                    c.up = -127;
                }
            }
            cl.send_cmds(&cmds);
        }
        let (ea, eb) = step_pair(&mut sv, (&qa, &mut ca), (&qb, &mut cb), now);
        for (events, join, cl) in [(ea, &mut ja, &mut ca), (eb, &mut jb, &mut cb)] {
            for e in events {
                if let NetEvent::ServerCommand(tokens) = e {
                    join.on_server_command(&tokens, cl, now);
                }
            }
        }
        if let Some(s) = ca.snapshots().newest() {
            for (num, e) in &s.entities {
                let i = |n: &str| e.field_i32(p, n);
                if p.event_of(i("eType")) != Some(EV_SOUND_ALIAS) {
                    continue;
                }
                let o = e.origin(p);
                let line = format!(
                    "temp {num} ev {EV_SOUND_ALIAS} parm {} other {} pos {:.1} {:.1} {:.1}",
                    i("eventParm"),
                    i("otherEntityNum"),
                    o[0],
                    o[1],
                    o[2]
                );
                if seen.insert(line.clone()) {
                    sounds.push(format!("VEH {} {line}", s.server_time));
                }
            }
        }
        if placed_at.is_none()
            && sv
                .script_log()
                .iter()
                .any(|l| l.starts_with("PROBE place "))
        {
            placed_at = Some(i);
        }
    }
    let mut out: Vec<String> = sv
        .script_log()
        .iter()
        .filter(|l| l.starts_with("PROBE "))
        .map(|l| l.trim_end().to_string())
        .collect();
    out.extend(sounds);
    if let Some(path) = std::env::var_os("RUNOVER_DUMP") {
        std::fs::write(path, out.join("\n")).expect("write RUNOVER_DUMP");
    }
    Some(out)
}

/// The yaw of the first line opening `head` (`PROBE place <t> <n> <spot>
/// <yaw>`).
fn placed_yaw(log: &[String], head: &str) -> Option<f32> {
    log.iter()
        .find(|l| l.starts_with(head))
        .and_then(|l| l.split_whitespace().last()?.parse().ok())
}

fn retail(path: &str) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| l.starts_with("PROBE ") || l.starts_with("VEH "))
        .map(str::to_string)
        .collect()
}

/// A line's time: the third word of a PROBE line, the second of a VEH one.
fn time(l: &str) -> Option<i32> {
    let w: Vec<&str> = l.split(' ').collect();
    match w[0] {
        "VEH" => w.get(1)?.parse().ok(),
        _ => w.get(2)?.parse().ok(),
    }
}

/// The victim's lines (`v`, `dmg`) and the hit sounds (alias 8,
/// `vehicle_hit_person`) from 100 ms before the first hit to `upto` ms past
/// it, each time made relative to the jeep's first move: the drive's start
/// rides the probe client's own clock from its spawn.
fn around_hit(lines: &[String], upto: i32) -> Vec<String> {
    let placed = lines
        .iter()
        .filter(|l| l.starts_with("PROBE a "))
        .find(|l| l.rsplit(' ').next() != Some("0"))
        .and_then(|l| time(l))
        .expect("the jeep moved");
    let hit = lines
        .iter()
        .find(|l| l.starts_with("PROBE dmg "))
        .and_then(|l| time(l))
        .expect("the victim was hit");
    let mut out: Vec<String> = lines
        .iter()
        .filter(|l| {
            l.starts_with("PROBE v ")
                || l.starts_with("PROBE dmg ")
                || (l.starts_with("VEH ") && l.contains(" parm 8 "))
        })
        .filter_map(|l| {
            let t = time(l)?;
            (hit - 100..=hit + upto).contains(&t).then(|| {
                let mut w: Vec<String> = l.split(' ').map(str::to_string).collect();
                let at = if w[0] == "VEH" { 1 } else { 2 };
                w[at] = format!("+{}", t - placed);
                // A temp entity's number rides the free list.
                if w[0] == "VEH" {
                    w[3] = "-".into();
                }
                w.join(" ")
            })
        })
        .collect();
    out.sort_by_key(|l| (time_rel(l), l.starts_with("VEH")));
    out
}

/// The jeep's ram sounds (alias 7, `vehicle_impact`, as its Horch strikes
/// the Panzer) up to `upto` ms past the jeep's first move, each time made
/// relative to that move and the temp entity's number dropped.
fn impacts(lines: &[String], upto: i32) -> Vec<String> {
    let placed = lines
        .iter()
        .filter(|l| l.starts_with("PROBE a "))
        .find(|l| l.rsplit(' ').next() != Some("0"))
        .and_then(|l| time(l))
        .expect("the jeep moved");
    lines
        .iter()
        .filter(|l| l.starts_with("VEH ") && l.contains(" parm 7 "))
        .filter_map(|l| {
            let t = time(l)? - placed;
            let w: Vec<&str> = l.split(' ').skip(4).collect();
            (t <= upto).then(|| format!("+{t} {}", w.join(" ")))
        })
        .collect()
}

/// The ram sounds on ours against retail's, one for one: same times from
/// the jeep's first move, same alias and same snapped origin.
fn compare_impacts(retail: &[String], ours: &[String]) {
    let upto = ours
        .iter()
        .filter_map(|l| time(l))
        .max()
        .expect("our lines")
        - ours
            .iter()
            .filter(|l| l.starts_with("PROBE a "))
            .find(|l| l.rsplit(' ').next() != Some("0"))
            .and_then(|l| time(l))
            .expect("our jeep moved");
    let (r, o) = (impacts(retail, upto), impacts(ours, upto));
    assert!(!r.is_empty(), "retail's jeep struck nothing by +{upto}");
    assert_eq!(r, o, "the ram sounds");
}

fn time_rel(l: &str) -> i32 {
    l.split(' ')
        .find_map(|w| w.strip_prefix('+')?.parse().ok())
        .unwrap_or(0)
}

/// Lines past `after` ms from the first hit let differ from retail's by
/// at most `within` a number, for `why`; at least one must.
struct Drift {
    after: i32,
    within: f32,
    why: &'static str,
}

/// The victim knocked through the air between the two hits, and his corpse
/// after: the gsc reads his origin after however many of the probe
/// client's cmds a frame ran, which retail's own clock spaces unevenly
/// (+550 to +650 steps of 14, 3.8 and 17.6 units on retail, 10.6 each on
/// ours, 16, 16 and 18 ms apart), so a line sits up to a cmd's travel
/// (3.8 at 212 units/s) off; the fall's height drifts 0.9 by the second
/// hit, and its sound's snapped origin a unit.
const FLIGHT: Drift = Drift {
    after: 150,
    within: 4.0,
    why: "the probe client's cmd clock in the flight",
};

/// A prone corpse creeping off the hull's line: retail's +1150 repeats
/// +1100 (a frame no cmd reached), the creep a cmd at a time.
const CREEP: Drift = Drift {
    after: 200,
    within: 0.1,
    why: "the probe client's cmd clock in the corpse's creep",
};

/// Where a prone victim settles: his lie-down slides him from where he
/// landed, and the slide rides when the probe client got the gsc's
/// `PROBE_IN`; retail's two prone runs lie (0.50, 0.99) apart, and ours
/// on the other's spot to the hundredth; the corpse creeps a little
/// further apart.
const LIE_DOWN: Drift = Drift {
    after: -1000,
    within: 1.1,
    why: "where the prone victim's lie-down left him",
};

/// Retail's lines against ours one for one: whole words (times, damage,
/// health, alias indices) equal, decimals within 0.01 up to `drift.after`
/// ms past the first hit and within `drift.within` from there.
fn compare(retail: &[String], ours: &[String], upto: i32, drift: Drift) {
    let (r, o) = (around_hit(retail, upto), around_hit(ours, upto));
    if std::env::var_os("RUNOVER_REPORT").is_some() {
        for l in &r {
            eprintln!("retail {l}");
        }
        for l in &o {
            eprintln!("ours   {l}");
        }
    }
    let words = |l: &str| -> Vec<String> {
        l.split([' ', '(', ')', ','])
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect()
    };
    let hit = r
        .iter()
        .find(|l| l.starts_with("PROBE dmg "))
        .map(|l| time_rel(l))
        .expect("retail's first hit");
    let same = |a: &str, b: &str, tol: f32| {
        let (wa, wb) = (words(a), words(b));
        wa.len() == wb.len()
            && wa.iter().zip(&wb).all(|(x, y)| {
                match (x.contains('.'), x.parse::<f32>(), y.parse::<f32>()) {
                    (true, Ok(x), Ok(y)) => (x - y).abs() <= tol,
                    _ => x == y,
                }
            })
    };
    let mut drifted = false;
    for (a, b) in r.iter().zip(&o) {
        if same(a, b, 0.01) {
            continue;
        }
        assert!(
            time_rel(a) > hit + drift.after && same(a, b, drift.within),
            "past {} ({})\nretail {a}\nours   {b}",
            drift.within,
            drift.why
        );
        drifted = true;
    }
    assert_eq!(r.len(), o.len(), "line counts\nretail {r:#?}\nours {o:#?}");
    assert!(
        drifted,
        "every line matches; drop the drift ({})",
        drift.why
    );
}

#[test]
fn a_jeep_runs_over_a_standing_player_twice() {
    let r = retail("tests/fixtures/vehicles/mp_kharkov-jeep-runover-stand-uo.txt");
    let Some(o) = ours("stand", "axis", &[]) else {
        return;
    };
    compare(&r, &o, 1000, FLIGHT);
    compare_impacts(&r, &o);
}

#[test]
fn a_prone_player_under_a_jeep_takes_ten_a_cmd() {
    let r = retail("tests/fixtures/vehicles/mp_kharkov-jeep-runover-prone-uo.txt");
    let Some(o) = ours("prone", "axis", &[]) else {
        return;
    };
    compare(&r, &o, 1000, CREEP);
    compare_impacts(&r, &o);
}

/// A teammate's jeep under friendly fire 0: each cmd's crush is refused,
/// so the prone victim deals himself the second 10 (`MOD_SUICIDE`, his own
/// weapon), and once dead keeps taking both until his end frame.
#[test]
fn a_teammate_s_jeep_makes_a_prone_player_hurt_himself() {
    let r = retail("tests/fixtures/vehicles/mp_kharkov-jeep-runover-prone-team-uo.txt");
    let Some(o) = ours("prone", "allies", &[("scr_friendlyfire", "0")]) else {
        return;
    };
    compare(&r, &o, 1000, LIE_DOWN);
    compare_impacts(&r, &o);
}
