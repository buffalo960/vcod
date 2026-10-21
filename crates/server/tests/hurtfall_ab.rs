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
//! Three more, `-hurtrate`, `-hurtslow` and `-hurtonce`, each on 1.1d,
//! 1.5b (`-1.5`) and UO (`-uo`), hold the hurt's cadence (combat doc 5.9):
//! a `dmg` the script wrote, the slow spawnflag and the once flag on a
//! trigger the script spawned, and the sound's temp entity, which the
//! probe's own `spawn` numbers show.
//!
//! `-hurtuse` and `-hurtwrite` hold `hurt_use` and the read-only fields,
//! `-hurtfar` a spawned hurt touching through its model-0 world copy, and
//! the client-less `-useby` and `-hurtlife` runs of
//! `client-probes/probe_useby.gsc` the other entities' `use` and a map
//! hurt's `life` (combat doc 5.9).
//!
//! `HURTFALL_REPORT=1` prints both sides line by line.
//!
//! Needs `COD_DIR` at the capture's install; without it the test returns
//! early.

mod common;

use common::{ADDR, ClientEnd, FRAME_MS, Join, Queues, holding, step_at};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::protocol::PROTOCOL_V6;
use vcod_common::net::{NetClient, NetEvent};
use vcod_common::pk3::Pk3Fs;

const MAP: &str = "mp_carentan";
const NAME: &str = "probe_hurtfall";
const PROBE: &str =
    include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_hurtfall.gsc");

/// How long after its connect the client's first snapshot lands on ours.
const FIRST_SNAP_AFTER_JOIN_MS: i64 = 400;

/// Ours' script log, its runtime errors and the client's first snapshot
/// time.
struct Ours {
    log: Vec<String>,
    aborts: Vec<String>,
    first_snap: Option<i32>,
}

/// Runs the probe on ours under `cvars` (the capture's `+set`s), the
/// client's join started `late_ms` earlier than [`FIRST_SNAP_AFTER_JOIN_MS`]
/// puts it, to `PROBE done`.
fn run_once(fs: Pk3Fs, text: &str, cvars: &[(&str, &str)], late_ms: i64) -> Option<Ours> {
    let p = vcod_server::server::install_protocol(&fs);
    let bsp_path = fs.resolve_map(MAP).expect("map in the mounted paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(MAP, NAME), now);
    sv.set_protocol(p);
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
            .is_some_and(|s| matches!(s.ps.field_i32(p, "pm_type"), 0 | 1 | 6));
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
    Some(Ours {
        log: sv.script_log().to_vec(),
        aborts: sv.script_aborts(),
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

/// A 1.5 install's paks, `None` at any other.
fn fs_15() -> Option<Pk3Fs> {
    let fs = vcod_common::testing::game_fs()?;
    (vcod_server::server::install_protocol(&fs).version == PROTOCOL_V6.version).then_some(fs)
}

/// Replays `fixture` on ours under `cvars` and holds every `PROBE` line to
/// retail's.
fn replay(fixture: &str, cvars: &[(&str, &str)]) {
    replay_on(vcod_common::testing::game_fs_11, fixture, cvars);
}

/// [`replay`] on the paks `fs` mounts, a 1.1, 1.5 or UO install's.
fn replay_on(fs: fn() -> Option<Pk3Fs>, fixture: &str, cvars: &[(&str, &str)]) {
    replay_with(fs, fixture, cvars, 0, Hold::All);
}

/// Which of retail's lines a replay holds ours to.
#[derive(Clone, Copy, PartialEq)]
enum Hold {
    All,
    /// Retail's log is cut short by the process dying on the first error:
    /// ours' must open with it.
    Prefix,
}

/// [`replay_on`] where the probe raises `errors` runtime errors, each read
/// only entity field's, holding the lines `hold` names.
fn replay_with(
    fs: fn() -> Option<Pk3Fs>,
    fixture: &str,
    cvars: &[(&str, &str)],
    errors: usize,
    hold: Hold,
) {
    let text = std::fs::read_to_string(fixture).unwrap();
    let Some(mut ours) = fs().and_then(|f| run_once(f, &text, cvars, 0)) else {
        eprintln!("COD_DIR unset or not the capture's install: skipping");
        return;
    };
    // How long ours takes to join differs from retail: rerun once so the
    // client's first snapshot falls on retail's.
    let first = common::cmds_ran(&text, "")[0].0;
    if let Some(t) = ours.first_snap.filter(|&t| t != first) {
        ours = run_once(fs().unwrap(), &text, cvars, (t - first) as i64).unwrap();
    }
    assert_eq!(ours.aborts.len(), errors, "{:?}", ours.aborts);
    for a in &ours.aborts {
        assert!(a.contains("Tried to set a read only entity field"), "{a}");
    }
    let retail = probe_lines(text.lines());
    let mut ours = probe_lines(ours.log.iter().map(String::as_str));
    if hold == Hold::Prefix {
        ours.truncate(retail.len());
    }
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

const RATE: &[(&str, &str)] = &[
    ("probe_lift", "120"),
    ("probe_dmg", "30"),
    ("probe_drop", "-30"),
];
const SLOW: &[(&str, &str)] = &[
    ("probe_spawn", "16"),
    ("probe_dmg", "25"),
    ("probe_drop", "-30"),
];
const ONCE: &[(&str, &str)] = &[
    ("probe_spawn", "44"),
    ("probe_dmg", "10"),
    ("probe_drop", "-30"),
];

/// The slab with a script-written `dmg` 30 hurts every 100 ms, each touch
/// playing its sound on a temp entity, and the fourth kills.
#[test]
fn a_trigger_hurt_hurts_every_100_ms_with_its_written_dmg() {
    replay("tests/fixtures/triggers/mp_carentan-dm-hurtrate.txt", RATE);
}

/// A script-spawned `trigger_hurt` with the slow flag hurts once a second.
#[test]
fn a_slow_trigger_hurt_hurts_once_a_second() {
    replay("tests/fixtures/triggers/mp_carentan-dm-hurtslow.txt", SLOW);
}

/// Spawnflags 44: one silent touch with no protection, then never again.
#[test]
fn a_once_trigger_hurt_hurts_once_without_a_sound() {
    replay("tests/fixtures/triggers/mp_carentan-dm-hurtonce.txt", ONCE);
}

#[test]
fn a_trigger_hurt_cadence_matches_retail_15() {
    replay_on(
        fs_15,
        "tests/fixtures/triggers/mp_carentan-dm-hurtrate-1.5.txt",
        RATE,
    );
    replay_on(
        fs_15,
        "tests/fixtures/triggers/mp_carentan-dm-hurtslow-1.5.txt",
        SLOW,
    );
    replay_on(
        fs_15,
        "tests/fixtures/triggers/mp_carentan-dm-hurtonce-1.5.txt",
        ONCE,
    );
}

#[test]
fn a_trigger_hurt_cadence_matches_retail_uo() {
    let uo = vcod_common::testing::uo_fs;
    replay_on(
        uo,
        "tests/fixtures/triggers/mp_carentan-dm-hurtrate-uo.txt",
        RATE,
    );
    replay_on(
        uo,
        "tests/fixtures/triggers/mp_carentan-dm-hurtslow-uo.txt",
        SLOW,
    );
    replay_on(
        uo,
        "tests/fixtures/triggers/mp_carentan-dm-hurtonce-uo.txt",
        ONCE,
    );
}

const USE: &[(&str, &str)] = &[
    ("probe_spawn", "17"),
    ("probe_dmg", "25"),
    ("probe_drop", "-30"),
    ("probe_use", "1000"),
];
const WRITE: &[(&str, &str)] = &[
    ("probe_spawn", "17"),
    ("probe_drop", "-30"),
    ("probe_write", "1"),
];

/// A `START_OFF` hurt hurts nothing until a `useBy` toggles its touch on,
/// and a second `useBy` turns it off; each use notifies `"trigger"` with
/// its user on the spot (combat doc 5.9).
#[test]
fn a_start_off_trigger_hurt_toggles_on_its_use() {
    replay("tests/fixtures/triggers/mp_carentan-dm-hurtuse.txt", USE);
}

/// A script write of `spawnflags`, `classname` or `model` is refused, and
/// below `developer 1` the writing thread runs on.
#[test]
fn a_trigger_hurt_refuses_a_write_of_its_read_only_fields() {
    replay_with(
        vcod_common::testing::game_fs_11,
        "tests/fixtures/triggers/mp_carentan-dm-hurtwrite.txt",
        WRITE,
        3,
        Hold::All,
    );
}

#[test]
fn a_trigger_hurt_use_and_write_match_retail_15() {
    replay_on(
        fs_15,
        "tests/fixtures/triggers/mp_carentan-dm-hurtuse-1.5.txt",
        USE,
    );
    replay_with(
        fs_15,
        "tests/fixtures/triggers/mp_carentan-dm-hurtwrite-1.5.txt",
        WRITE,
        3,
        Hold::All,
    );
}

/// UO's server process dies on the first write (SIGABRT, combat doc 5.9);
/// ours runs on, so only the lines before it are held.
#[test]
fn a_trigger_hurt_use_and_write_match_retail_uo() {
    let uo = vcod_common::testing::uo_fs;
    replay_on(
        uo,
        "tests/fixtures/triggers/mp_carentan-dm-hurtuse-uo.txt",
        USE,
    );
    replay_with(
        uo,
        "tests/fixtures/triggers/mp_carentan-dm-hurtwrite-uo.txt",
        WRITE,
        3,
        Hold::Prefix,
    );
}

const FAR: &[(&str, &str)] = &[
    ("probe_spawn", "16"),
    ("probe_dmg", "25"),
    ("probe_drop", "-30"),
    ("probe_spawn_x", "0"),
    ("probe_spawn_y", "0"),
    ("probe_spawn_z", "8"),
];

/// A hurt spawned at (0 0 8), far from the body, hurts it as a slow hurt
/// next to it does: its inline model 0 is the world lifted 8, and the body
/// lands inside that copy's street (combat doc 5.9).
#[test]
fn a_script_spawned_trigger_hurt_touches_through_its_world_copy() {
    replay("tests/fixtures/triggers/mp_carentan-dm-hurtfar.txt", FAR);
    replay_on(
        fs_15,
        "tests/fixtures/triggers/mp_carentan-dm-hurtfar-1.5.txt",
        FAR,
    );
    replay_on(
        vcod_common::testing::uo_fs,
        "tests/fixtures/triggers/mp_carentan-dm-hurtfar-uo.txt",
        FAR,
    );
}

/// `probe_useby` and its `probe_life` run: no client, so ours runs the
/// probe alone on the paks `fs` mounts, its bsp through `patch`, and every
/// `PROBE` line must match the fixture's.
fn replay_alone(
    fs: fn() -> Option<Pk3Fs>,
    fixture: &str,
    cvars: &[(&str, &str)],
    patch: fn(Vec<u8>) -> Vec<u8>,
) {
    const PROBE: &str =
        include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_useby.gsc");
    let Some(mut fs) = fs() else {
        eprintln!("COD_DIR unset or not the capture's install: skipping");
        return;
    };
    let p = vcod_server::server::install_protocol(&fs);
    let bsp_path = fs.resolve_map(MAP).expect("map in the mounted paks");
    let bytes = patch(fs.read(&bsp_path).unwrap());
    fs.overlay(&bsp_path, bytes.clone());
    let bsp = vcod_common::bsp::parse(&bytes).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(MAP, "probe_useby"), now);
    sv.set_protocol(p);
    // run_probe.sh's.
    sv.set_cvar("developer", "1");
    sv.overlay_script("maps/mp/gametypes/probe_useby", PROBE);
    for (k, v) in cvars {
        sv.set_cvar(k, v);
    }
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    common::run_alone(&mut sv, &mut now, 8000);
    assert!(sv.script_aborts().is_empty(), "{:?}", sv.script_aborts());
    let text = std::fs::read_to_string(fixture).unwrap();
    let retail = probe_lines(text.lines());
    let ours = probe_lines(sv.script_log().iter().map(String::as_str));
    if std::env::var("HURTFALL_REPORT").is_ok() {
        for (a, b) in retail.iter().zip(&ours) {
            println!("retail {a}\nours   {b}");
        }
    }
    assert_eq!(ours, retail);
}

fn unpatched(bytes: Vec<u8>) -> Vec<u8> {
    bytes
}

/// `useBy` on a `trigger_once` spends it (`Use_Multi`, freed 150 ms on to
/// the script), on a `trigger_multiple` or a `trigger_damage` notifies
/// alone, and on a `func_static` toggles its link, which a `bulletTrace`
/// sees (combat doc 5.9, "useBy").
#[test]
fn use_by_runs_each_entity_s_use_like_retail() {
    replay_alone(
        vcod_common::testing::game_fs_11,
        "tests/fixtures/triggers/mp_carentan-dm-useby.txt",
        &[],
        unpatched,
    );
    replay_alone(
        fs_15,
        "tests/fixtures/triggers/mp_carentan-dm-useby-1.5.txt",
        &[],
        unpatched,
    );
    replay_alone(
        vcod_common::testing::uo_fs,
        "tests/fixtures/triggers/mp_carentan-dm-useby-uo.txt",
        &[],
        unpatched,
    );
}

/// The `-hurtlife` captures' entity lump edit: mp_carentan's one hurt
/// trades `"dmg" "1000"` for `"life" "2.5"`, the same length.
fn hurtlife_bsp(mut bytes: Vec<u8>) -> Vec<u8> {
    let (old, new) = (b"\"dmg\" \"1000\"", b"\"life\" \"2.5\"");
    let at: Vec<usize> = bytes
        .windows(old.len())
        .enumerate()
        .filter(|(_, w)| w == old)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(at.len(), 1);
    bytes[at[0]..at[0] + old.len()].copy_from_slice(new);
    bytes
}

/// A map hurt with a `life` key is freed by `hurt_think` once its first
/// `useBy` is `life` seconds old, on the first 100 ms think past it.
#[test]
fn a_used_trigger_hurt_with_a_life_is_freed_like_retail() {
    const LIFE: &[(&str, &str)] = &[("probe_life", "1")];
    replay_alone(
        vcod_common::testing::game_fs_11,
        "tests/fixtures/triggers/mp_carentan-dm-hurtlife.txt",
        LIFE,
        hurtlife_bsp,
    );
    replay_alone(
        fs_15,
        "tests/fixtures/triggers/mp_carentan-dm-hurtlife-1.5.txt",
        LIFE,
        hurtlife_bsp,
    );
    replay_alone(
        vcod_common::testing::uo_fs,
        "tests/fixtures/triggers/mp_carentan-dm-hurtlife-uo.txt",
        LIFE,
        hurtlife_bsp,
    );
}
