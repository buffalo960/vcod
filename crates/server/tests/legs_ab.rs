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

/// The cgame's copy of the clock (`cgame_mp_x86.dll` 0x30003be0) on
/// retail's own snapshots of the walk: a body drawn every `draw_ms`, each
/// snapshot arriving [`LEAD_MS`] before the drawn clock reaches its
/// `serverTime`, starts its clips and blends on the drawn clock and measures
/// its rate whenever the newest snapshot moves. Returns how many drawn
/// frames were compared and how far, at worst, a foot sat off where the end
/// frame's clock puts it `LEAD_MS` later; `None` without the paks.
fn drawn_feet(draw_ms: i32) -> Option<(usize, f32)> {
    use vcod_common::animclock::{ChannelClock, Seen, Times};
    use vcod_common::playerpose::{PoseInputs, pose_player};
    let fs = vcod_common::testing::game_fs()?;
    let anims = vcod_common::animtree::PlayerAnims::load(&fs).unwrap();
    let body = vcod_common::xmodel::load(&fs, "playerbody_american_airborne").unwrap();
    let skel = vcod_common::skeleton::Skeleton::build(&[&body]);
    let feet = ["bip01 l foot", "bip01 r foot"].map(|n| skel.bone_index(n).expect(n));
    let mut clips: BTreeMap<String, Option<Rc<vcod_common::xanim::XAnim>>> = BTreeMap::new();
    let mut feet_of = |legs: &ChannelClock, torso: &ChannelClock, now: i32| {
        let inputs = PoseInputs {
            anims: &anims,
            legs: 0,
            torso: 0,
            legs_layers: legs.layers(now),
            torso_layers: torso.layers(now),
            group_pitch: 0.0,
            turret_leaves: None,
            controllers: None,
        };
        let pose = pose_player(&skel, &inputs, |n| {
            clips
                .entry(n.to_string())
                .or_insert_with(|| vcod_common::xanim::load(&fs, n).ok().map(Rc::new))
                .clone()
        });
        feet.map(|b| pose.bone_world(&skel, b).0)
    };
    // `(serverTime, trBase, legsAnim, torsoAnim)` per snapshot.
    let snaps: Vec<(i32, glam::Vec3, i32, i32)> = std::fs::read_to_string(CLIENT)
        .unwrap()
        .lines()
        .filter(|l| l.starts_with("!snap"))
        .map(|line| {
            let kv: BTreeMap<&str, &str> = line
                .split_whitespace()
                .skip(1)
                .filter_map(|w| w.split_once('='))
                .collect();
            let o: Vec<f32> = kv["origin"]
                .split(',')
                .map(|x| x.parse().unwrap())
                .collect();
            let n = |k: &str| kv[k].parse::<i32>().unwrap();
            let tr_base = glam::Vec3::new(o[0], o[1], o[2]).trunc();
            (n("t"), tr_base, n("legs"), n("torso"))
        })
        .collect();
    let seen = |tr_base| Seen {
        client_num: 0,
        tr_base,
        eflags: 0,
    };
    // The end frame's clocks, kept at every snapshot.
    let mut end = (ChannelClock::default(), ChannelClock::default());
    let mut ends = BTreeMap::new();
    for &(t, tr, l, to) in &snaps {
        end.0
            .update(&anims, true, l, &seen(tr), Times::end_frame(t));
        end.1
            .update(&anims, false, to, &seen(tr), Times::end_frame(t));
        ends.insert(t, end);
    }
    let mut drawn = (ChannelClock::default(), ChannelClock::default());
    let (first, last) = (snaps[0].0, snaps.last().unwrap().0);
    // Past the idle the join started and the first run's blend, as the
    // bone gate skips them.
    let compare_from = GAPS[0].1;
    let (mut worst, mut compared) = (0.0f32, 0);
    let mut r = first - LEAD_MS;
    while r + LEAD_MS <= last {
        let &(t, tr, l, to) = snaps.iter().rev().find(|s| s.0 - LEAD_MS <= r)?;
        let at = Times {
            clock_ms: r,
            snapshot_ms: t,
        };
        drawn.0.update(&anims, true, l, &seen(tr), at);
        drawn.1.update(&anims, false, to, &seen(tr), at);
        let later = r + LEAD_MS;
        // Between two end frames the end frame's clock runs on from the
        // last one.
        let (_, e) = ends.range(..=later).next_back().unwrap();
        if later >= compare_from && GAPS.iter().all(|g| !(g.0..=g.1).contains(&later)) {
            let a = feet_of(&drawn.0, &drawn.1, r);
            let b = feet_of(&e.0, &e.1, later);
            let d = a[0].distance(b[0]).max(a[1].distance(b[1]));
            if report() && d > 0.1 {
                println!("drawn {r}: feet {d:.2} off the end frame's at {later}");
            }
            worst = worst.max(d);
            compared += 1;
        }
        r += draw_ms;
    }
    println!("drawn every {draw_ms} ms: {compared} frames, feet at worst {worst:.2} off");
    Some((compared, worst))
}

/// How long before the drawn clock reaches a snapshot's `serverTime` the
/// snapshot arrives: one snapshot interval, as `cg.nextSnap` runs.
const LEAD_MS: i32 = 50;

/// Drawn on a cadence that meets every arrival, the cgame's clock is the end
/// frame's run a snapshot early: the setter, the rate and the blend all land
/// on the same clip times (cod11-movers.md 15, "The drawn body's clip").
#[test]
fn a_drawn_body_runs_the_end_frame_clips_a_snapshot_early() {
    let Some((compared, worst)) = drawn_feet(10) else {
        eprintln!("COD_DIR unset or has no main/: skipping");
        return;
    };
    assert!(compared > 1500, "only {compared} drawn frames compared");
    assert!(worst < 0.01, "feet {worst:.2} off the end frame's");
}

/// On a cadence that misses the arrivals each change lands on the first
/// drawn frame past it, up to 7 ms late. A steady clip then reads under 1.9
/// units off, a strafe-to-strafe blend 2.9 at worst while it runs, and
/// nothing piles up over the walk.
#[test]
fn a_drawn_body_off_the_arrivals_stays_within_a_frame_of_clip() {
    let Some((compared, worst)) = drawn_feet(7) else {
        eprintln!("COD_DIR unset or has no main/: skipping");
        return;
    };
    assert!(compared > 2000, "only {compared} drawn frames compared");
    assert!(worst < 3.0, "feet {worst:.2} off the end frame's");
}
