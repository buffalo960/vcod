//! A walking player's bones against the retail capture.
//!
//! `movers/mp_carentan-dm-legs.txt` is the server's `games_mp.log` under
//! `client-probes/probe_legs.gsc`: from level time 25000 on, the player's
//! origin and four script_origins linked to its feet, right hand and head,
//! one `PROBE f`/`h` pair per server frame. `-legs-client.txt` is the
//! `--save-legs` client's half: every usercmd it sent and every snapshot's
//! `serverTime` and `commandTime`. Ours runs the same gametype with one
//! client that holds still until the placement, then sends retail's cmds
//! verbatim, each ahead of the frame whose snapshot first acknowledged it,
//! so both servers walk the same body on the same level times. The run,
//! stop, backpedal, strafes, diagonals and crouch walk exercise the legs
//! clip's rate and its first phase (`docs/research/cod11-movers.md` 15, "A
//! moving body's clip").
//!
//! `LEGS_REPORT=1` prints every compared frame.
//!
//! Needs `COD_DIR`; without the paks the test returns early.

mod common;

use common::{ADDR, ClientEnd, FRAME_MS, Join, Queues, holding, step_at};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::msg::{NULL_USERCMD, UserCmd};
use vcod_common::net::{NetClient, NetEvent};

const MAP: &str = "mp_carentan";
const SERVER: &str = "tests/fixtures/movers/mp_carentan-dm-legs.txt";
const CLIENT: &str = "tests/fixtures/movers/mp_carentan-dm-legs-client.txt";

/// The player's origin, to `logPrint`'s hundredth.
const ORIGIN_TOL: f32 = 0.02;

/// A bone's origin and its turn, in degrees, off retail's on a body whose
/// legs clip has played alone for a while: the run, backpedal, strafes,
/// diagonals and crouch walk, rate and phase both. Ours reads 0.39 and 0.80
/// at worst; the run's worst sit on the loop's seam.
const STEADY_TOL: f32 = 0.45;
const STEADY_TURN: f32 = 1.0;

/// The same within [`BLEND_MS`] of a legs change, while the old clip blends
/// out: ours reads 0.73 and 2.06 at worst.
const BLEND_TOL: f32 = 0.8;
const BLEND_TURN: f32 = 2.5;

/// The longest blend the setter gives (cod11-movers.md 15).
const BLEND_MS: i32 = 250;

/// Known divergences: `(from, to, why)` over the rows' level times, each
/// pointing at the section that explains it.
const GAPS: &[(i32, i32, &str)] = &[
    // The idle the spawn started runs from ours' spawn, which is not
    // retail's: the join differs. Through the first run's blend.
    (25000, 26700, "spawn idle phase"),
    // runbk, idle and runbk again on three end frames in a row (37350..
    // 37450): two setter calls inside one blend, and the returning node's
    // weight is not what the straight-line ramp gives (movers doc 15).
    (37500, 37600, "back-to-back legs changes"),
    // Retail picks the crouched idle at 43350 with the body still backing
    // at 84 u/s; ours keeps runcrbk for three frames (movers doc 15).
    (43400, 43750, "crouched stop's selection"),
];

fn report() -> bool {
    std::env::var("LEGS_REPORT").is_ok_and(|v| v == "1")
}

/// `(x, y, z)` as retail's `logPrint` renders a vector.
fn vectors(line: &str) -> Vec<[f32; 3]> {
    line.split('(')
        .skip(1)
        .filter_map(|s| {
            let inner = s.split(')').next()?;
            let v: Vec<f32> = inner
                .split(',')
                .filter_map(|t| t.trim().parse().ok())
                .collect();
            (v.len() == 3).then(|| [v[0], v[1], v[2]])
        })
        .collect()
}

/// One frame of the bone log: the player's origin, then each bone's origin
/// and angles (l foot, r foot, hand, head).
#[derive(Clone, Debug, Default)]
struct Frame {
    origin: [f32; 3],
    bones: Vec<([f32; 3], [f32; 3])>,
}

fn frames<'a>(lines: impl Iterator<Item = &'a str>) -> BTreeMap<i32, Frame> {
    let mut out: BTreeMap<i32, Frame> = BTreeMap::new();
    for line in lines.filter(|l| !l.starts_with('#')) {
        let Some(rest) = line.find("PROBE ").map(|i| &line[i + 6..]) else {
            continue;
        };
        let t: Vec<&str> = rest.split_whitespace().collect();
        let (Some(kind), Some(time)) = (t.first(), t.get(1).and_then(|s| s.parse().ok())) else {
            continue;
        };
        let v = vectors(rest);
        let f = out.entry(time).or_default();
        match *kind {
            "f" => {
                f.origin = v[0];
                f.bones.splice(0..0, [(v[1], v[2]), (v[3], v[4])]);
            }
            "h" => f.bones.extend([(v[0], v[1]), (v[2], v[3])]),
            _ => {}
        }
    }
    out
}

/// The client half: every cmd, and each snapshot's `(serverTime,
/// commandTime, script step, legsAnim)`.
struct Walk {
    cmds: Vec<(UserCmd, [i32; 3])>,
    snaps: Vec<(i32, i32, String, i32, i32)>,
}

fn walk(text: &str) -> Walk {
    let mut cmds = Vec::new();
    let mut snaps = Vec::new();
    for line in text.lines() {
        let kv: BTreeMap<&str, &str> = line
            .split_whitespace()
            .skip(1)
            .filter_map(|w| w.split_once('='))
            .collect();
        let n = |k: &str| kv[k].parse::<i32>().unwrap();
        let v3 = |k: &str| -> [i32; 3] {
            let v: Vec<i32> = kv[k].split(',').map(|x| x.parse().unwrap()).collect();
            [v[0], v[1], v[2]]
        };
        if line.starts_with("!cmd") {
            let cmd = UserCmd {
                server_time: n("st"),
                buttons: n("buttons") as u8,
                wbuttons: n("wbuttons") as u8,
                weapon: n("weapon") as u8,
                up: n("up") as i8,
                forward: n("forward") as i8,
                right: n("right") as i8,
                ..NULL_USERCMD
            };
            cmds.push((cmd, v3("world")));
        } else if line.starts_with("!snap") {
            snaps.push((
                n("t"),
                n("ct"),
                kv["step"].to_string(),
                n("legs"),
                n("movementDir"),
            ));
        }
    }
    Walk { cmds, snaps }
}

/// Ours' bone log over the same walk, and its `legsAnim` by serverTime.
struct Ours {
    log: Vec<String>,
    legs: BTreeMap<i32, (i32, i32)>,
}

fn ours() -> Option<&'static Ours> {
    static RUN: std::sync::OnceLock<Option<Ours>> = std::sync::OnceLock::new();
    RUN.get_or_init(run_ours).as_ref()
}

fn run_ours() -> Option<Ours> {
    let fs = vcod_common::testing::game_fs()?;
    let walk = walk(&std::fs::read_to_string(CLIENT).unwrap());
    let bsp_path = fs.resolve_map(MAP).expect("map in the mounted paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(MAP, "probe_legs"), now);
    sv.overlay_script(
        "maps/mp/gametypes/probe_legs",
        include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_legs.gsc"),
    );
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    let q = Rc::new(RefCell::new(Queues::default()));
    let mut cl = NetClient::start_with_qport(ClientEnd(q.clone()), now, 0x2001);
    let mut join = Join::new("allies", "m1carbine_mp");
    let p = &vcod_common::net::protocol::PROTOCOL_V1;
    let placed = walk.snaps.first().expect("a snapshot").0;
    let acked: BTreeMap<i32, i32> = walk.snaps.iter().map(|(t, ct, ..)| (*t, *ct)).collect();
    let mut legs = BTreeMap::new();
    let last = walk.snaps.last().unwrap().0;
    let mut next_cmd = 0;
    let mut last_st = i32::MIN;
    loop {
        let newest = cl.snapshots().newest().map(|s| s.server_time);
        let t = newest.map_or(0, |t| t + FRAME_MS as i32);
        if t > last {
            break;
        }
        if t <= placed {
            cl.send_frame(&holding(&cl));
        } else {
            // Retail's cmds up to what the snapshot of frame `t` had
            // acknowledged, so this frame runs the moves retail's did.
            let ct = acked
                .range(..=t)
                .next_back()
                .map_or(i32::MIN, |(_, ct)| *ct);
            let delta = cl.snapshots().newest().map_or([0; 3], |s| {
                [0, 1, 2].map(|i| s.ps.field_i32(p, &format!("delta_angles[{i}]")))
            });
            let mut batch = Vec::new();
            while let Some((cmd, world)) = walk.cmds.get(next_cmd)
                && cmd.server_time <= ct
            {
                next_cmd += 1;
                if cmd.server_time <= last_st {
                    continue;
                }
                last_st = cmd.server_time;
                let mut c = *cmd;
                c.angles = [0, 1, 2].map(|i| (world[i] - delta[i]) & 0xffff);
                batch.push(c);
            }
            cl.send_cmds(&batch);
        }
        now += Duration::from_millis(FRAME_MS as u64);
        for e in step_at(&mut sv, ADDR, &q, &mut cl, now) {
            match e {
                NetEvent::ServerCommand(tokens) => join.on_server_command(&tokens, &mut cl, now),
                NetEvent::Dropped(r) => panic!("dropped: {r}"),
                _ => {}
            }
        }
        assert!(sv.script_aborts().is_empty(), "{:?}", sv.script_aborts());
        if let Some(s) = cl.snapshots().newest() {
            legs.insert(
                s.server_time,
                (
                    s.ps.field_i32(p, "legsAnim"),
                    s.ps.field_i32(p, "movementDir"),
                ),
            );
        }
    }
    Some(Ours {
        log: sv.script_log().to_vec(),
        legs,
    })
}

/// The largest component difference.
fn off(r: [f32; 3], o: [f32; 3]) -> f32 {
    (0..3).map(|i| (r[i] - o[i]).abs()).fold(0.0, f32::max)
}

/// The rotation between two `(pitch, yaw, roll)` triples, in degrees: a foot
/// sits near pitch 90, where yaw and roll trade off and compare apart.
fn turn(r: [f32; 3], o: [f32; 3]) -> f32 {
    let q = |a: [f32; 3]| {
        let [p, y, r] = a.map(f32::to_radians);
        glam::Quat::from_rotation_z(y)
            * glam::Quat::from_rotation_y(p)
            * glam::Quat::from_rotation_x(r)
    };
    q(r).angle_between(q(o)).to_degrees()
}

#[test]
fn a_walking_body_is_posed_like_retail() {
    let Some(run) = ours() else {
        eprintln!("COD_DIR unset or has no main/: skipping");
        return;
    };
    let retail = frames(std::fs::read_to_string(SERVER).unwrap().lines());
    let ours = frames(run.log.iter().map(String::as_str));
    let walk = walk(&std::fs::read_to_string(CLIENT).unwrap());
    let step: BTreeMap<i32, &str> = walk
        .snaps
        .iter()
        .map(|(t, _, s, ..)| (*t, s.as_str()))
        .collect();
    let mut bad = Vec::new();
    // The anim selection first: the same index and movementDir on the same
    // frame, so a bone row that differs is the pose's doing.
    for (t, _, _, l, dir) in &walk.snaps {
        let o = run.legs.get(t);
        let differs = o.is_some_and(|o| o.0 & 511 != l & 511 || o.1 != *dir);
        if differs || report() && o != Some(&(*l, *dir)) {
            println!("{t} legsAnim, movementDir: retail {l} {dir} ours {o:?}");
        }
        // The crouched stop of `GAPS`, the three frames before its rows.
        if differs && !(43350..=43450).contains(t) {
            bad.push(format!(
                "{t} legsAnim, movementDir: retail {l} {dir} ours {o:?}"
            ));
        }
    }
    // The end frames that changed the legs anim, toggle included.
    let changes: std::collections::BTreeSet<i32> = walk
        .snaps
        .windows(2)
        .filter(|w| w[0].3 != w[1].3)
        .map(|w| w[1].0)
        .collect();
    let mut worst: BTreeMap<String, (f32, f32)> = BTreeMap::new();
    let mut compared = 0;
    for (t, r) in &retail {
        // The placement frame poses nothing yet: the links are made after it.
        let Some(o) = ours.get(t).filter(|_| r.bones.len() == 4) else {
            continue;
        };
        compared += 1;
        let label = step
            .range(..=*t)
            .next_back()
            .map_or("?", |(_, s)| *s)
            .to_string();
        let d_origin = off(r.origin, o.origin);
        if d_origin > ORIGIN_TOL {
            bad.push(format!(
                "{t} {label} origin: retail {:?} ours {:?}",
                r.origin, o.origin
            ));
        }
        let (mut dp, mut da) = (0.0f32, 0.0f32);
        for (rb, ob) in r.bones.iter().zip(&o.bones) {
            dp = dp.max(off(rb.0, ob.0));
            da = da.max(turn(rb.1, ob.1));
        }
        let w = worst.entry(label.clone()).or_default();
        w.0 = w.0.max(dp);
        w.1 = w.1.max(da);
        // The row logs the pose the frame before it took (movers doc 15),
        // and a blend counts from the end frame that set the clip.
        let posed = t - FRAME_MS as i32;
        let blending = changes
            .range(..=posed)
            .next_back()
            .is_some_and(|c| posed - c < BLEND_MS);
        let (tol, turn_tol) = if blending {
            (BLEND_TOL, BLEND_TURN)
        } else {
            (STEADY_TOL, STEADY_TURN)
        };
        let gap = GAPS.iter().find(|g| (g.0..=g.1).contains(t));
        if gap.is_none() && (dp > tol || da > turn_tol) {
            bad.push(format!(
                "{t} {label}{}: bones off {dp:.2}, turned {da:.2}",
                if blending { " (blending)" } else { "" }
            ));
        }
        if report() {
            println!("{t} {label} origin {d_origin:.2} bones {dp:.2} angles {da:.2}");
            for (k, (rb, ob)) in r.bones.iter().zip(&o.bones).enumerate() {
                println!(
                    "   {k} retail {:?} {:?}\n   {k} ours   {:?} {:?}",
                    rb.0, rb.1, ob.0, ob.1
                );
            }
        }
    }
    if report() {
        for (label, (p, a)) in &worst {
            println!("{label}: worst bone origin {p:.2}, turn {a:.2}");
        }
    }
    assert!(compared > 400, "only {compared} frames compared");
    assert!(
        bad.is_empty(),
        "{} rows differ:\n{}",
        bad.len(),
        bad.join("\n")
    );
}
