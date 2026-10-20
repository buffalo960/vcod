//! A `trigger_hurt` kill in mid-air and the corpse's fall, against the
//! retail capture.
//!
//! Three captures in `triggers/` (`mp_carentan-dm-hurtfall.txt`, `-hurtlow`,
//! `-hurtland`, recipes in their headers), each the server's `games_mp.log`
//! under `client-probes/probe_hurtfall.gsc`, which lifts mp_carentan's kill
//! slab to a height of the run's and drops the player through it, then the
//! `--probe-ride` client's `CMDS` lines. Ours runs the same probe with one
//! allied client replaying those cmds (`ProbeHold::framed`), and every
//! `PROBE` line must match: the frame the slab kills on, the callback's
//! undefined point and direction, and the corpse's fall, landing and slide
//! (`docs/research/cod11-combat.md` 5.8).
//!
//! `HURTFALL_REPORT=1` prints both sides line by line.
//!
//! Needs `COD_DIR` at a 1.1 install; without it the test returns early.

mod common;

use common::{ADDR, ClientEnd, FRAME_MS, Join, Queues, holding, step_at};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::protocol::PROTOCOL_V1;
use vcod_common::net::{NetClient, NetEvent};

const MAP: &str = "mp_carentan";
const NAME: &str = "probe_hurtfall";
const PROBE: &str =
    include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_hurtfall.gsc");

/// How long after its connect the client's first snapshot lands on ours.
const FIRST_SNAP_AFTER_JOIN_MS: i64 = 400;

/// Ours' script log and the client's first snapshot time.
struct Ours {
    log: Vec<String>,
    first_snap: Option<i32>,
}

/// Runs the probe on ours under `cvars` (the capture's `+set`s), the
/// client's join started `late_ms` earlier than [`FIRST_SNAP_AFTER_JOIN_MS`]
/// puts it, to `PROBE done`.
fn run_once(text: &str, cvars: &[(&str, &str)], late_ms: i64) -> Option<Ours> {
    let fs = vcod_common::testing::game_fs_11()?;
    let bsp_path = fs.resolve_map(MAP).expect("map in the mounted paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(MAP, NAME), now);
    sv.overlay_script(&format!("maps/mp/gametypes/{NAME}"), PROBE);
    for (k, v) in cvars {
        sv.set_cvar(k, v);
    }
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    let mut hold = common::ProbeHold {
        base: Some([0.0; 3]),
        framed: true,
        times: common::cmds_times(text, ""),
        ran: common::cmds_ran(text, ""),
        ..Default::default()
    };
    let first = hold.ran.first().expect("the capture's CMDS lines").0;
    while (sv.game_time_ms() as i64) < first as i64 - FIRST_SNAP_AFTER_JOIN_MS - late_ms {
        now += Duration::from_millis(FRAME_MS as u64);
        sv.tick(now);
        sv.take_outgoing();
    }
    let q = Rc::new(RefCell::new(Queues::default()));
    let mut cl = NetClient::start_with_qport(ClientEnd(q.clone()), now, 0x2001);
    let mut join = Join::new("allies", "m1carbine_mp");
    let mut first_snap = None;
    for _ in 0..3000 {
        now += Duration::from_millis(FRAME_MS as u64);
        // Playing or dead (`pm_type` 6): the corpse falls on the cmds too.
        let live = cl
            .snapshots()
            .newest()
            .is_some_and(|s| matches!(s.ps.field_i32(&PROTOCOL_V1, "pm_type"), 0 | 1 | 6));
        if live {
            let cmds = hold.cmds(&cl, 0);
            cl.send_cmds(&cmds);
        } else if let Some(c) = cl.send_frame(&holding(&cl)) {
            hold.last = c.server_time;
        }
        for e in step_at(&mut sv, ADDR, &q, &mut cl, now) {
            match e {
                NetEvent::ServerCommand(tokens) => join.on_server_command(&tokens, &mut cl, now),
                NetEvent::Dropped(r) => panic!("dropped: {r}"),
                _ => {}
            }
        }
        if let Some(s) = cl.snapshots().newest() {
            first_snap.get_or_insert(s.server_time);
        }
        if sv.script_log().iter().any(|l| l.contains("PROBE done")) {
            break;
        }
    }
    assert_eq!(sv.script_aborts(), Vec::<String>::new());
    Some(Ours {
        log: sv.script_log().to_vec(),
        first_snap,
    })
}

/// The `PROBE` lines of a log, from the tag on.
fn probe_lines<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<String> {
    lines
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.find("PROBE ").map(|i| l[i..].trim_end().to_string()))
        .collect()
}

/// Replays `fixture` on ours under `cvars` and holds every `PROBE` line to
/// retail's.
fn replay(fixture: &str, cvars: &[(&str, &str)]) {
    let text = std::fs::read_to_string(fixture).unwrap();
    let Some(mut ours) = run_once(&text, cvars, 0) else {
        eprintln!("COD_DIR unset or not a 1.1 install: skipping");
        return;
    };
    // How long ours takes to join differs from retail: rerun once so the
    // client's first snapshot falls on retail's.
    let first = common::cmds_ran(&text, "")[0].0;
    if let Some(t) = ours.first_snap.filter(|&t| t != first) {
        ours = run_once(&text, cvars, (t - first) as i64).unwrap();
    }
    let retail = probe_lines(text.lines());
    let ours = probe_lines(ours.log.iter().map(String::as_str));
    if std::env::var("HURTFALL_REPORT").is_ok() {
        for (a, b) in retail.iter().zip(&ours) {
            println!("retail {a}\nours   {b}");
        }
    }
    assert_eq!(ours.len(), retail.len(), "line count");
    for (r, o) in retail.iter().zip(&ours) {
        assert_eq!(o, r);
    }
}

/// The slab kills 270 over the street.
#[test]
fn a_trigger_hurt_corpse_falls_like_retail() {
    replay("tests/fixtures/triggers/mp_carentan-dm-hurtfall.txt", &[]);
}

/// The slab kills about 70 over the street: a knockback timer on the kill
/// (the `(0,0,0)` point and direction ours once passed) would slide the
/// corpse 17 further on its landing.
#[test]
fn a_trigger_hurt_kill_just_over_the_ground_lands_like_retail() {
    replay(
        "tests/fixtures/triggers/mp_carentan-dm-hurtlow.txt",
        &[("probe_late_lift", "203")],
    );
}

/// The slab's top is 3 over the street: the kill comes before the landing,
/// whose fall damage then lands on the corpse. A knockback timer on the
/// kill slides this corpse 5 too.
#[test]
fn a_trigger_hurt_kill_before_a_hard_landing_matches_retail() {
    replay(
        "tests/fixtures/triggers/mp_carentan-dm-hurtland.txt",
        &[("probe_lift", "120")],
    );
}
