//! A prone body's bones, ours against retail's. `client-probes/probe_pronebones`
//! puts one prone client on a spot (`probe_spot`) twice, a second apart,
//! and once he has lain still logs a script_origin linked to each of eight
//! bones every frame for 2 s. The fixtures are retail 1.1d's, 1.5b's and
//! United Offensive 1.51b's: mp_carentan's ~19 degree mound, a flat spot on
//! mp_carentan and probe_scriptmodel's prone target on mp_brecourt's field.
//! Each bone is compared off the body's origin, which a fall onto the grade
//! leaves a fraction of a unit apart run to run
//! (docs/research/player-model-anim-system.md, "The prone body on a grade").
//!
//! On 1.5 and UO the body keeps the bend its last airborne cmd wrote
//! (cod15-movement.md 4, "The airborne prone body"), so ours' cmds run on
//! the probe client's 16, 16, 18 cadence and the playerstate's
//! `fTorsoHeight`, `fTorsoPitch` and `fWaistPitch` are held to the
//! fixture's last `FALL` line.
//!
//! With `probe_spawn_yaw` the first placement is a `spawn` at another yaw:
//! 1.5's and UO's spawn think lies the held prone down along it
//! (cod15-movement.md 4, "The spawn's lie-down").
//!
//! `PB_REPORT=1` prints both logs, `PB_TRACE=1` our snapshots. The tests need `COD_DIR` at the install
//! each names and return early without.

mod common;

use common::{Join, Queues, step};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::msg::{UserCmd, WBUTTON_PRONE};
use vcod_common::net::protocol::PROTOCOL_V6;
use vcod_common::net::{NetClient, NetEvent};

const SCRIPT: &str =
    include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_pronebones.gsc");

/// How far a bone's offset off the origin may sit from retail's: the
/// logged origins are rounded to the hundredth.
const POSE_NEAR: f32 = 0.05;
/// How far the settled origin may sit from retail's: the drop onto the
/// spot lands within the ground trace's quarter unit, and a grade slides
/// it under a unit across, both by the cmd timing.
const ORIGIN_NEAR: f32 = 1.0;
/// How far the wire's prone body may sit from retail's: units, degrees.
const BODY_NEAR: f32 = 0.01;

/// Our run of the probe on `map` with one client holding prone at world
/// `yaw`, until the script's `done` line: the log, and the last snapshot's
/// prone body and `proneDirection`. `spawn_yaw` is `probe_spawn_yaw`.
fn ours(
    fs: vcod_common::pk3::Pk3Fs,
    map: &str,
    spot: &str,
    yaw: f32,
    spawn_yaw: Option<f32>,
) -> (Vec<String>, [f32; 3], f32) {
    let proto = vcod_server::server::install_protocol(&fs);
    let bsp_path = fs.resolve_map(map).expect("map in the paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(map, "probe_pronebones"), now);
    sv.set_protocol(proto);
    sv.overlay_script("maps/mp/gametypes/probe_pronebones", SCRIPT);
    sv.set_cvar("probe_spot", spot);
    if let Some(y) = spawn_yaw {
        sv.set_cvar("probe_spawn_yaw", &y.to_string());
    }
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    common::run_alone(&mut sv, &mut now, common::PROBE_JOIN_MS);
    let q = Rc::new(RefCell::new(Queues::default()));
    let mut cl = NetClient::start_with_qport(common::ClientEnd(q.clone()), now, 0x2001);
    let mut join = Join::new("axis", "kar98k_mp");
    let mut hold = common::ProbeHold {
        base: Some([0.0, yaw, 0.0]),
        framed: true,
        ..Default::default()
    };
    let mut body = [0.0; 3];
    let mut dir = 0.0;
    let frame = Duration::from_millis(common::FRAME_MS as u64);
    let live = |cl: &NetClient<common::ClientEnd>| {
        cl.snapshots()
            .newest()
            .is_some_and(|s| s.ps.field_i32(cl.protocol(), "pm_type") == 0)
    };
    for _ in 0..1000 {
        now += frame;
        if live(&cl) {
            let mut cmds = hold.cmds(&cl, 0);
            for c in &mut cmds {
                c.wbuttons |= WBUTTON_PRONE;
                c.up = -127;
            }
            cl.send_cmds(&cmds);
        } else {
            // Held from the join on, as the probe client holds it: 1.5's
            // spawn think runs this cmd.
            let held = UserCmd {
                wbuttons: WBUTTON_PRONE,
                up: -127,
                angles: [0, (yaw * 65536.0 / 360.0) as i32 & 0xffff, 0],
                ..Default::default()
            };
            if let Some(c) = cl.send_frame(&held) {
                hold.last = c.server_time;
            }
        }
        for e in step(&mut sv, &q, &mut cl, now) {
            if let NetEvent::ServerCommand(tokens) = e {
                join.on_server_command(&tokens, &mut cl, now);
            }
        }
        if let Some(s) = cl.snapshots().newest() {
            body = ["fTorsoHeight", "fTorsoPitch", "fWaistPitch"]
                .map(|n| f32::from_bits(s.ps.field_i32(cl.protocol(), n) as u32));
            dir = f32::from_bits(s.ps.field_i32(cl.protocol(), "proneDirection") as u32);
            if std::env::var_os("PB_TRACE").is_some() {
                let i = |n: &str| s.ps.field_i32(cl.protocol(), n);
                eprintln!(
                    "SNAP t={} ct={} pm_time={} lerp={} target={} pm_flags={:#x} view={} dir={dir} body={body:?} z={} legs={} torso={}",
                    s.server_time,
                    i("commandTime"),
                    i("pm_time"),
                    i("viewHeightLerpTime"),
                    i("viewHeightTarget"),
                    i("pm_flags"),
                    s.ps.viewangles(cl.protocol())[1],
                    f32::from_bits(i("origin[2]") as u32),
                    i("legsAnim"),
                    i("torsoAnim"),
                );
            }
        }
        if sv.script_log().iter().any(|l| l.starts_with("PROBE done")) {
            break;
        }
    }
    (sv.script_log().to_vec(), body, dir)
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

/// The settled origin, and each frame's bones as (tag, offset off that
/// frame's origin), in order.
#[allow(clippy::type_complexity)]
fn pose(lines: &[String]) -> ([f32; 3], Vec<Vec<(String, [f32; 3])>>) {
    let mut origin = None;
    let mut frames: Vec<Vec<(String, [f32; 3])>> = Vec::new();
    for l in lines.iter().map(|l| l.trim()) {
        if let Some(rest) = l.strip_prefix("PROBE body ") {
            let (_, v) = rest.split_once(" (").unwrap();
            origin = Some(vec3(v));
            frames.push(Vec::new());
        } else if let Some(rest) = l.strip_prefix("PROBE bone ") {
            // `<time> <client> <tag with spaces> <origin>`
            let (head, v) = rest.split_once(" (").unwrap();
            let tag = head.splitn(3, ' ').nth(2).unwrap().to_string();
            let (o, b) = (origin.unwrap(), vec3(v));
            frames
                .last_mut()
                .unwrap()
                .push((tag, [0, 1, 2].map(|i| b[i] - o[i])));
        }
    }
    (origin.expect("no body line"), frames)
}

/// The prone body of the fixture's last `FALL` line, flat without one.
fn retail_body(fixture: &str) -> [f32; 3] {
    fixture
        .lines()
        .rev()
        .filter(|l| l.starts_with("FALL "))
        .find_map(|l| l.split_once(" prone=")?.1.split(' ').next())
        .map_or([0.0; 3], |v| {
            let v: Vec<f32> = v.split(',').map(|x| x.parse().unwrap()).collect();
            [v[0], v[1], v[2]]
        })
}

/// The `proneDirection` of the fixture's last `FALL` line that carries one.
fn retail_dir(fixture: &str) -> Option<f32> {
    fixture
        .lines()
        .rev()
        .filter(|l| l.starts_with("FALL "))
        .find_map(|l| l.split_once(" dir=")?.1.split(' ').next()?.parse().ok())
}

fn check(fs: vcod_common::pk3::Pk3Fs, map: &str, spot: &str, yaw: f32, fixture: &str) {
    check_spawn(fs, map, spot, yaw, None, fixture, true);
}

/// `bones` false leaves the bones out, for a run whose clip timing ours
/// still misses (named at the caller).
fn check_spawn(
    fs: vcod_common::pk3::Pk3Fs,
    map: &str,
    spot: &str,
    yaw: f32,
    spawn_yaw: Option<f32>,
    fixture: &str,
    bones: bool,
) {
    let retail: Vec<String> = fixture
        .lines()
        .filter(|l| l.starts_with("PROBE"))
        .map(str::to_string)
        .collect();
    let (ours, body, dir) = ours(fs, map, spot, yaw, spawn_yaw);
    if let Some(rd) = retail_dir(fixture) {
        assert!(
            ((rd - dir + 180.0).rem_euclid(360.0) - 180.0).abs() <= BODY_NEAR,
            "{spot}: retail's proneDirection {rd}, ours {dir}"
        );
    }
    if std::env::var_os("PB_REPORT").is_some() {
        ours.iter().for_each(|l| eprintln!("ours {l}"));
        retail.iter().for_each(|l| eprintln!("retail {l}"));
    }
    let rb = retail_body(fixture);
    assert!(
        (0..3).all(|i| (rb[i] - body[i]).abs() <= BODY_NEAR),
        "{spot}: retail's prone body {rb:?}, ours {body:?}"
    );
    let ((ro, rf), (oo, of)) = (pose(&retail), pose(&ours));
    assert_eq!(of.len(), rf.len(), "{ours:?}");
    assert!(
        (0..3).all(|i| (ro[i] - oo[i]).abs() <= ORIGIN_NEAR),
        "{spot}: retail settled at {ro:?}, ours at {oo:?}"
    );
    for (f, (r, o)) in rf.iter().zip(&of).enumerate().filter(|_| bones) {
        assert_eq!(r.len(), o.len());
        for ((tag, a), (otag, b)) in r.iter().zip(o) {
            assert_eq!(tag, otag);
            assert!(
                (0..3).all(|i| (a[i] - b[i]).abs() <= POSE_NEAR),
                "{spot} frame {f} {tag}: retail {a:?} off the origin, ours {b:?}"
            );
        }
    }
}

/// mp_carentan's ~19 degree mound, facing downhill: the slope terms bend
/// the body over the grade.
#[test]
fn a_prone_body_on_the_mp_carentan_mound_poses_like_retail() {
    let Some(fs) = vcod_common::testing::game_fs_11() else {
        return;
    };
    check(
        fs,
        "mp_carentan",
        "mound",
        270.0,
        include_str!("fixtures/pronebones/mp_carentan-mound.txt"),
    );
}

#[test]
fn a_prone_body_on_flat_ground_poses_like_retail() {
    let Some(fs) = vcod_common::testing::game_fs_11() else {
        return;
    };
    check(
        fs,
        "mp_carentan",
        "flat",
        225.0,
        include_str!("fixtures/pronebones/mp_carentan-flat.txt"),
    );
}

/// probe_scriptmodel's prone target on mp_brecourt's field.
#[test]
fn a_prone_body_on_the_mp_brecourt_field_poses_like_retail() {
    let Some(fs) = vcod_common::testing::game_fs_11() else {
        return;
    };
    check(
        fs,
        "mp_brecourt",
        "brecourt",
        270.0,
        include_str!("fixtures/pronebones/mp_brecourt-brecourt.txt"),
    );
}

/// The 1.5b install's paks, `None` elsewhere (a UO install's `main/` is
/// 1.5's, so these run there too).
fn fs_15() -> Option<vcod_common::pk3::Pk3Fs> {
    let fs = vcod_common::testing::game_fs()?;
    (vcod_server::server::install_protocol(&fs).version == PROTOCOL_V6.version).then_some(fs)
}

#[test]
fn a_prone_body_on_the_mp_carentan_mound_poses_like_15() {
    let Some(fs) = fs_15() else {
        return;
    };
    check(
        fs,
        "mp_carentan",
        "mound",
        270.0,
        include_str!("fixtures/pronebones/mp_carentan-mound-1.5.txt"),
    );
}

#[test]
fn a_prone_body_on_flat_ground_poses_like_15() {
    let Some(fs) = fs_15() else {
        return;
    };
    check(
        fs,
        "mp_carentan",
        "flat",
        225.0,
        include_str!("fixtures/pronebones/mp_carentan-flat-1.5.txt"),
    );
}

#[test]
fn a_prone_body_on_the_mp_brecourt_field_poses_like_15() {
    let Some(fs) = fs_15() else {
        return;
    };
    check(
        fs,
        "mp_brecourt",
        "brecourt",
        270.0,
        include_str!("fixtures/pronebones/mp_brecourt-brecourt-1.5.txt"),
    );
}

#[test]
fn a_prone_body_on_the_mp_carentan_mound_poses_like_uo() {
    let Some(fs) = vcod_common::testing::uo_fs() else {
        return;
    };
    check(
        fs,
        "mp_carentan",
        "mound",
        270.0,
        include_str!("fixtures/pronebones/mp_carentan-mound-uo.txt"),
    );
}

#[test]
fn a_prone_body_on_flat_ground_poses_like_uo() {
    let Some(fs) = vcod_common::testing::uo_fs() else {
        return;
    };
    check(
        fs,
        "mp_carentan",
        "flat",
        225.0,
        include_str!("fixtures/pronebones/mp_carentan-flat-uo.txt"),
    );
}

#[test]
fn a_prone_body_on_the_mp_brecourt_field_poses_like_uo() {
    let Some(fs) = vcod_common::testing::uo_fs() else {
        return;
    };
    check(
        fs,
        "mp_brecourt",
        "brecourt",
        270.0,
        include_str!("fixtures/pronebones/mp_brecourt-brecourt-uo.txt"),
    );
}

/// The spawn's own think lies a client holding prone down along the spawn's
/// yaw on 1.5 and UO, which run his last cmd there; 1.1's null cmd leaves
/// him standing until his next cmd, by when the view is the spot's
/// (cod15-movement.md 4, "The spawn's lie-down").
#[test]
fn a_spawn_lies_down_on_its_own_yaw_like_retail() {
    let Some(fs) = vcod_common::testing::game_fs_11() else {
        return;
    };
    check_spawn(
        fs,
        "mp_brecourt",
        "brecourt",
        270.0,
        Some(315.0),
        include_str!("fixtures/pronebones/mp_brecourt-spawn315.txt"),
        true,
    );
}

#[test]
fn a_spawn_lies_down_on_its_own_yaw_like_15() {
    let Some(fs) = fs_15() else {
        return;
    };
    check_spawn(
        fs,
        "mp_brecourt",
        "brecourt",
        270.0,
        Some(315.0),
        include_str!("fixtures/pronebones/mp_brecourt-spawn315-1.5.txt"),
        true,
    );
}

#[test]
fn a_spawn_lies_down_on_its_own_yaw_like_uo() {
    let Some(fs) = vcod_common::testing::uo_fs() else {
        return;
    };
    check_spawn(
        fs,
        "mp_brecourt",
        "brecourt",
        270.0,
        Some(315.0),
        include_str!("fixtures/pronebones/mp_brecourt-spawn315-uo.txt"),
        true,
    );
}
