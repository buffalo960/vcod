//! A dying body's bones and its corpse's event ring, ours against retail
//! 1.1d's, 1.5b's and United Offensive 1.51b's. `client-probes/probe_dyingpose` stands one client on mp_carentan's
//! flat prone spot facing 90, links a script_origin to each of eight bones,
//! logs them every frame for 0.5 s, then `suicide()`s him into the dead
//! session and logs them 2.5 s more. The killed callback clones the corpse
//! and plays an alias on it (docs/research/cod11-combat.md 5.2 and 16.4,
//! "The dying body").
//!
//! `DP_REPORT=1` prints our log, `DP_TRACE=1` our snapshots, `DP_SEED=<n>`
//! runs another server seed (another death clip), `DP_RETAIL=1` prints the
//! retail capture's death frames. The 1.1 gates need `COD_DIR` at a 1.1
//! install, the 1.5 ones at a 1.5 or UO install (its `main/`), the UO ones
//! at a UO install.

mod common;

use common::{Join, Queues, step};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::NetEvent;
use vcod_common::net::msg::EntityState;
use vcod_common::net::protocol::{PROTOCOL_V1, PROTOCOL_V6, Protocol};

const SCRIPT: &str =
    include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_dyingpose.gsc");
const FIXTURE: &str = include_str!("fixtures/dyingpose/mp_carentan-dm.txt");
const FIXTURE_15: &str = include_str!("fixtures/dyingpose/mp_carentan-dm-1.5.txt");
const FIXTURE_UO: &str = include_str!("fixtures/dyingpose/mp_carentan-dm-uo.txt");

/// How far a bone's offset off the origin may sit from retail's: the bone
/// and the origin are each logged rounded to the hundredth, so their
/// difference carries up to 0.01, plus f32 noise at map coordinates.
const POSE_NEAR: f32 = 0.015;

/// The server seed whose death draw is retail's, `legsAnim` 20: the pick is
/// `rand()` on retail and ours draws its own sequence.
const SEED: u64 = 2;
/// 1.5b's and UO's, drawing their captures' death clips.
const SEED_15: u64 = 6;
const SEED_UO: u64 = 68;

/// Our run of the probe until its `done` line: the log and the corpse as
/// the client's first snapshot holding it carried it.
fn ours(fs: vcod_common::pk3::Pk3Fs, seed: u64) -> (Vec<String>, Option<EntityState>) {
    let map = "mp_carentan";
    let proto = vcod_server::server::install_protocol(&fs);
    let bsp_path = fs.resolve_map(map).expect("map in the paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::with_seed(common::cfg(map, "probe_dyingpose"), now, seed);
    sv.set_protocol(proto);
    sv.overlay_script("maps/mp/gametypes/probe_dyingpose", SCRIPT);
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    common::run_alone(&mut sv, &mut now, common::PROBE_JOIN_MS);
    let q = Rc::new(RefCell::new(Queues::default()));
    let mut cl =
        vcod_common::net::NetClient::start_with_qport(common::ClientEnd(q.clone()), now, 0x2001);
    let mut join = Join::new("axis", "kar98k_mp");
    // `--probe-yaw 90`: every cmd at world yaw 90.
    let mut hold = common::ProbeHold {
        base: Some([0.0, 90.0, 0.0]),
        ..Default::default()
    };
    let mut corpse = None;
    let frame = Duration::from_millis(common::FRAME_MS as u64);
    for _ in 0..1000 {
        now += frame;
        let cmds = hold.cmds(&cl, 0);
        if cmds.is_empty() {
            cl.send_frame(&vcod_common::net::msg::NULL_USERCMD);
        } else {
            cl.send_cmds(&cmds);
        }
        for e in step(&mut sv, &q, &mut cl, now) {
            if let NetEvent::ServerCommand(tokens) = e {
                join.on_server_command(&tokens, &mut cl, now);
            }
        }
        if std::env::var_os("DP_TRACE").is_some()
            && let Some(s) = cl.snapshots().newest()
        {
            let i = |n: &str| s.ps.field_i32(cl.protocol(), n);
            eprintln!(
                "SNAP t={} pm_type={} legs={} torso={} view={:?} eflags={:#x}",
                s.server_time,
                i("pm_type"),
                i("legsAnim"),
                i("torsoAnim"),
                s.ps.viewangles(cl.protocol()),
                i("eFlags"),
            );
        }
        if corpse.is_none() {
            corpse = cl
                .snapshots()
                .newest()
                .and_then(|s| s.entities.get(&64).cloned());
        }
        if sv.script_log().iter().any(|l| l.starts_with("PROBE done")) {
            break;
        }
    }
    (sv.script_log().to_vec(), corpse)
}

fn vec3(s: &str) -> [f32; 3] {
    let v: Vec<f32> = s
        .trim()
        .trim_matches(|c| c == '(' || c == ')')
        .split(", ")
        .map(|x| x.parse().unwrap())
        .collect();
    [v[0], v[1], v[2]]
}

/// Each frame's session and bones as (tag, offset off the origin the frame
/// before logged), in order: a link reads the body as the last frame left
/// it, its origin included, which the death's relink moves.
#[allow(clippy::type_complexity)]
fn pose(lines: &[String]) -> Vec<(String, Vec<(String, [f32; 3])>)> {
    let (mut origin, mut last) = ([0.0; 3], None);
    let mut frames: Vec<(String, Vec<(String, [f32; 3])>)> = Vec::new();
    for l in lines.iter().map(|l| l.trim()) {
        if let Some(rest) = l.strip_prefix("PROBE body ") {
            // `<time> <client> <origin> <angles> <session>`
            let (_, v) = rest.split_once(" (").unwrap();
            let (o, rest) = v.split_once(") (").unwrap();
            origin = last.unwrap_or(vec3(o));
            last = Some(vec3(o));
            let session = rest.rsplit(' ').next().unwrap().to_string();
            frames.push((session, Vec::new()));
        } else if let Some(rest) = l.strip_prefix("PROBE bone ") {
            // `<time> <client> <tag with spaces> <origin>`
            let (head, v) = rest.split_once(" (").unwrap();
            let tag = head.splitn(3, ' ').nth(2).unwrap().to_string();
            let b = vec3(v);
            frames
                .last_mut()
                .unwrap()
                .1
                .push((tag, [0, 1, 2].map(|i| b[i] - origin[i])));
        }
    }
    frames
}

fn retail(fixture: &str) -> Vec<String> {
    fixture
        .lines()
        .filter(|l| l.starts_with("PROBE"))
        .map(str::to_string)
        .collect()
}

/// The 1.5b install's paks, `None` elsewhere (a UO install's `main/` is
/// 1.5's, so these run there too).
fn fs_15() -> Option<vcod_common::pk3::Pk3Fs> {
    let fs = vcod_common::testing::game_fs()?;
    (vcod_server::server::install_protocol(&fs).version == PROTOCOL_V6.version).then_some(fs)
}

/// Every frame's bones within [`POSE_NEAR`] of retail's `fixture`.
fn check_pose(fs: vcod_common::pk3::Pk3Fs, fixture: &str, seed: u64) {
    let retail = retail(fixture);
    let seed = std::env::var("DP_SEED").map_or(seed, |v| v.parse().unwrap());
    let (ours, _) = ours(fs, seed);
    if std::env::var_os("DP_REPORT").is_some() {
        ours.iter().for_each(|l| eprintln!("ours {l}"));
    }
    let (rf, of) = (pose(&retail), pose(&ours));
    assert_eq!(of.len(), rf.len(), "{ours:?}");
    let mut misses = Vec::new();
    for (f, ((rs, r), (os, o))) in rf.iter().zip(&of).enumerate() {
        assert_eq!(rs, os, "frame {f}");
        for ((tag, a), (otag, b)) in r.iter().zip(o) {
            assert_eq!(tag, otag);
            if !(0..3).all(|i| (a[i] - b[i]).abs() <= POSE_NEAR) {
                misses.push(format!("frame {f} ({rs}) {tag}: retail {a:?}, ours {b:?}"));
            }
        }
    }
    assert!(misses.is_empty(), "{}", misses.join("\n"));
}

/// Retail's corpse as the capture pair first sent entity 64.
fn retail_corpse(p: &Protocol, tag: &str) -> EntityState {
    let frames = vcod_common::testing::fixture_frames(
        p,
        &format!("net/gamestate-{tag}.bin"),
        &format!("net/snapshots-{tag}.bin"),
    );
    if std::env::var_os("DP_RETAIL").is_some() {
        let mut last = None;
        for f in &frames {
            let s = &f.snapshot;
            let i = |n: &str| s.ps.field_i32(p, n);
            let now = (i("pm_type"), i("legsAnim"), i("torsoAnim"), i("eFlags"));
            if last != Some(now) {
                eprintln!(
                    "RETAIL t={} pm_type={} legs={} torso={} eflags={:#x}",
                    s.server_time, now.0, now.1, now.2, now.3
                );
                last = Some(now);
            }
        }
    }
    frames
        .into_iter()
        .find_map(|f| f.snapshot.entities.get(&64).cloned())
        .expect("retail's corpse")
}

/// Our corpse's ring, flags and yaw against retail's first one.
fn check_corpse(fs: vcod_common::pk3::Pk3Fs, p: &Protocol, tag: &str, seed: u64) {
    let retail = retail_corpse(p, tag);
    let (log, corpse) = ours(fs, seed);
    assert!(
        log.iter()
            .any(|l| l.starts_with("PROBE killed") && l.ends_with(" body 64")),
        "the killed line names the corpse: {log:?}"
    );
    let corpse = corpse.expect("the corpse rode a snapshot");
    let mut fields = vec!["eventSequence", "eFlags", "apos.trBase[1]"];
    let ring: Vec<String> = (0..4)
        .flat_map(|i| [format!("events[{i}]"), format!("eventParms[{i}]")])
        .collect();
    fields.extend(ring.iter().map(String::as_str));
    for name in fields {
        if std::env::var_os("DP_REPORT").is_some() {
            eprintln!(
                "{name}: retail {} ours {}",
                retail.field_i32(p, name),
                corpse.field_i32(p, name)
            );
        }
        assert_eq!(
            corpse.field_i32(p, name),
            retail.field_i32(p, name),
            "{name}: ours {corpse:?}, retail {retail:?}"
        );
    }
}

/// The dead session's bones: retail's controller hook still runs on a body
/// with `EF_DEAD` and sets `tag_origin`'s yaw to the legs' world yaw
/// (combat doc 16.4, "The dying body").
#[test]
fn a_dying_body_poses_like_retail() {
    let Some(fs) = vcod_common::testing::game_fs_11() else {
        return;
    };
    check_pose(fs, FIXTURE, SEED);
}

/// 1.5b's: the same hook and an ease with no dead test (16.4).
#[test]
fn a_dying_body_poses_like_15() {
    let Some(fs) = fs_15() else {
        return;
    };
    check_pose(fs, FIXTURE_15, SEED_15);
}

/// UO's: the same, its ease (0x1f107) read for a dead test too (16.4).
#[test]
fn a_dying_body_poses_like_uo() {
    let Some(fs) = vcod_common::testing::uo_fs() else {
        return;
    };
    check_pose(fs, FIXTURE_UO, SEED_UO);
}

/// `cloneplayer` hands the corpse back as an entity, and an alias played on
/// it rides the corpse's own ring: retail's first snapshot of entity 64
/// (`net/snapshots-dyingpose.bin`) sent `eventSequence` 1, event 172
/// (`EV_SOUND_ALIAS`), parm 4, `eFlags` 0x818 and the body's yaw 90.
#[test]
fn a_sound_played_on_a_corpse_rides_its_ring() {
    let Some(fs) = vcod_common::testing::game_fs_11() else {
        return;
    };
    check_corpse(fs, &PROTOCOL_V1, "dyingpose", SEED);
    assert_eq!(
        retail_corpse(&PROTOCOL_V1, "dyingpose").field_i32(&PROTOCOL_V1, "events[0]"),
        172
    );
}

#[test]
fn a_sound_played_on_a_corpse_rides_its_ring_15() {
    let Some(fs) = fs_15() else {
        return;
    };
    check_corpse(fs, &PROTOCOL_V6, "dyingpose-1.5", SEED_15);
}

#[test]
fn a_sound_played_on_a_corpse_rides_its_ring_uo() {
    let Some(fs) = vcod_common::testing::uo_fs() else {
        return;
    };
    let p = vcod_server::server::install_protocol(&fs);
    check_corpse(fs, p, "dyingpose-uo", SEED_UO);
}
