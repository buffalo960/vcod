//! A script model in a locational trace's way, ours against retail's.
//! `client-probes/probe_scriptmodel` spawns an `xmodel/barrel_black1`
//! script model and logs a `bulletTrace` through it shown, hidden,
//! `notSolid()`ed and deleted (1.1d and UO alike); on UO it also stands one
//! between a `--probe-uo-weapon aim` shooter and an empty Willys, shown for
//! 3 s, hidden for 3 s, then deleted, and logs the jeep's `damage`
//! notifies. Retail stops the trace and every round on the model until it
//! is deleted, hidden or not (docs/research/cod11-combat.md 2.7).
//!
//! `SM_REPORT=1` prints both logs. The trace test needs `COD_DIR` at a 1.1
//! install, the rounds test at a UO one; each returns early without.

mod common;

use common::{Join, Queues, step};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::msg::{BUTTON_ADS, BUTTON_ATTACK, UserCmd};
use vcod_common::net::protocol::PROTOCOL_V22;
use vcod_common::net::{NetClient, NetEvent};

/// The shooter's first rounds leave before its sight is up, at the hip's
/// spread, and ours' RNG sends one or two past the barrel's sides where
/// retail's sent none; the shown phase is counted from this far past the
/// placement.
const SIGHT_MS: i32 = 500;

const SCRIPT: &str =
    include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_scriptmodel.gsc");

/// Our run of the probe on `map`; with `shooter`, client 0 joins and holds
/// the trigger from its placement for `ms` past it.
fn ours(fs: vcod_common::pk3::Pk3Fs, map: &str, uo: bool, shooter: bool, ms: i64) -> Vec<String> {
    let bsp_path = fs.resolve_map(map).expect("map in the paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(map, "probe_scriptmodel"), now);
    if uo {
        sv.set_protocol(&PROTOCOL_V22);
    }
    sv.overlay_script("maps/mp/gametypes/probe_scriptmodel", SCRIPT);
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    if !shooter {
        common::run_alone(&mut sv, &mut now, 3000);
        return sv.script_log().to_vec();
    }
    common::run_alone(&mut sv, &mut now, common::PROBE_JOIN_MS);
    let q = Rc::new(RefCell::new(Queues::default()));
    let mut cl = NetClient::start_with_qport(common::ClientEnd(q.clone()), now, 0x2001);
    let mut join = Join::new("allies", "thompson_mp");
    let frame = Duration::from_millis(common::FRAME_MS as u64);
    let mut hold = common::ProbeHold {
        aim: true,
        ..Default::default()
    };
    let mut placed_at: Option<i64> = None;
    for i in 0..3000i64 {
        now += frame;
        if placed_at.is_some_and(|at| (i - at) * common::FRAME_MS > ms) {
            break;
        }
        if placed_at.is_some_and(|at| i > at) && cl.state() == vcod_common::net::NetState::Active {
            cl.send_cmds(&hold.cmds(&cl, BUTTON_ATTACK | BUTTON_ADS));
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
    sv.script_log().to_vec()
}

fn probe_lines(text: impl IntoIterator<Item = String>) -> Vec<String> {
    text.into_iter()
        .map(|l| l.trim().to_string())
        .filter(|l| l.starts_with("PROBE"))
        .collect()
}

fn report(retail: &[String], ours: &[String]) {
    if std::env::var_os("SM_REPORT").is_some() {
        for l in ours {
            eprintln!("ours   {l}");
        }
        for l in retail {
            eprintln!("retail {l}");
        }
    }
}

/// Each `mtrace` line's phase, whether it stopped on the model, its
/// fraction and surface type.
fn traces(lines: &[String]) -> Vec<(String, bool, f32, String)> {
    let model = lines
        .iter()
        .find_map(|l| l.strip_prefix("PROBE model "))
        .and_then(|r| r.split(' ').next())
        .expect("the model was spawned")
        .to_string();
    lines
        .iter()
        .filter_map(|l| l.strip_prefix("PROBE mtrace "))
        .map(|l| {
            let t: Vec<&str> = l.split(' ').collect();
            (
                t[1].to_string(),
                t[2] == model,
                t[3].parse().unwrap(),
                t[4].to_string(),
            )
        })
        .collect()
}

fn compare_traces(fixture: &str, log: Vec<String>) {
    let retail = probe_lines(fixture.lines().map(str::to_string));
    let ours = probe_lines(log);
    report(&retail, &ours);
    let (r, o) = (traces(&retail), traces(&ours));
    assert_eq!(r.len(), 4, "{retail:?}");
    assert_eq!(o.len(), r.len(), "{ours:?}");
    for (r, o) in r.iter().zip(&o) {
        assert_eq!(
            (&o.0, o.1, &o.3),
            (&r.0, r.1, &r.3),
            "retail {r:?}, ours {o:?}"
        );
        assert!((o.2 - r.2).abs() < 0.005, "retail {r:?}, ours {o:?}");
    }
}

/// mp_carentan on 1.1d: the barrel stops the trace shown, hidden and
/// `notSolid`; deleted, the trace runs on to the world.
#[test]
fn a_script_model_stops_bullettrace_until_deleted() {
    let Some(fs) = vcod_common::testing::game_fs_11() else {
        return;
    };
    let log = ours(fs, "mp_carentan", false, false, 0);
    compare_traces(
        include_str!("fixtures/scriptmodel/mp_carentan-scriptmodel.txt"),
        log,
    );
}

/// The jeep's damage notifies in each phase of the barrel: shown, hidden,
/// deleted. The shown phase starts [`SIGHT_MS`] past the placement.
fn phase_counts(lines: &[String]) -> [usize; 3] {
    let time = |l: &str| -> i32 { l.split(' ').nth(2).unwrap().parse().unwrap() };
    let at = |p: &str| {
        lines
            .iter()
            .find(|l| l.starts_with("PROBE rphase") && l.split(' ').nth(3) == Some(p))
            .map(|l| time(l))
            .unwrap_or_else(|| panic!("no {p} phase in {lines:?}"))
    };
    let (shown, hidden, deleted) = (at("shown"), at("hidden"), at("deleted"));
    let mut out = [0; 3];
    for l in lines.iter().filter(|l| l.starts_with("PROBE vdamage")) {
        let t = time(l);
        if t < shown + SIGHT_MS {
            continue;
        }
        out[(t >= hidden) as usize + (t >= deleted) as usize] += 1;
    }
    out
}

/// mp_peaks on UO: the barrel stops every round shown and hidden; deleted,
/// the jeep takes them, 22 each.
#[test]
fn a_script_model_stops_rounds_until_deleted_on_uo() {
    let Some(fs) = vcod_common::testing::uo_fs() else {
        return;
    };
    let fixture = include_str!("fixtures/scriptmodel/mp_peaks-scriptmodel-uo.txt");
    let log = ours(fs, "mp_peaks", true, true, 9000);
    let retail = probe_lines(fixture.lines().map(str::to_string));
    let ours = probe_lines(log);
    report(&retail, &ours);
    let (rt, ot) = (traces(&retail), traces(&ours));
    assert_eq!(rt.len(), ot.len(), "{ours:?}");
    for (r, o) in rt.iter().zip(&ot) {
        assert_eq!(
            (&o.0, o.1, &o.3),
            (&r.0, r.1, &r.3),
            "retail {r:?}, ours {o:?}"
        );
        assert!((o.2 - r.2).abs() < 0.005, "retail {r:?}, ours {o:?}");
    }
    // Retail's deleted phase ran to the server's end; ours stops 3 s past
    // the delete, so only its first 3 s count.
    let deleted = |lines: &[String]| {
        lines
            .iter()
            .find(|l| l.starts_with("PROBE rphase") && l.ends_with("deleted"))
            .and_then(|l| l.split(' ').nth(2)?.parse::<i32>().ok())
            .expect("the barrel was deleted")
    };
    let rd = deleted(&retail);
    let retail_window: Vec<String> = retail
        .iter()
        .filter(|l| {
            !l.starts_with("PROBE vdamage")
                || l.split(' ')
                    .nth(2)
                    .and_then(|t| t.parse::<i32>().ok())
                    .is_some_and(|t| t < rd + 3000)
        })
        .cloned()
        .collect();
    let (r, o) = (phase_counts(&retail_window), phase_counts(&ours));
    assert_eq!((r[0], r[1]), (0, 0), "retail's barrel let rounds by: {r:?}");
    assert_eq!((o[0], o[1]), (0, 0), "ours' barrel let rounds by: {o:?}");
    assert!(
        r[2] > 10 && o[2].abs_diff(r[2]) * 5 <= r[2],
        "rounds past the deleted barrel in 3 s: retail {}, ours {}",
        r[2],
        o[2]
    );
    assert!(
        ours.iter()
            .filter(|l| l.starts_with("PROBE vdamage"))
            .all(|l| l.split(' ').nth(3) == Some("22")),
        "{ours:?}"
    );
}
