//! The client's predictor against retail's mover under each patch's rules.
//!
//! The fixtures are `--save-slope --probe-moves` runs of one movement script
//! (`moves_script` in `crates/client/src/probe.rs`: jumps inside and past
//! 1.5's jump penalty, crouch, prone, the prone press while running,
//! strafes, hip and sighted shots) against the retail 1.1d server and the
//! retail 1.5b server, tagged `moves-1.1` and `moves-1.5`. Every snapshot
//! carries its whole decoded playerstate, so each one is rebuilt through
//! `predict::from_wire` under the capture's protocol, the cmds up to the
//! next snapshot's `commandTime` are run through `predict::run_cmd`, and the
//! prediction is compared field by field with the next snapshot rebuilt the
//! same way. The 1.5 capture is also replayed under 1.1's rules, which it
//! must fail: that is what shows the gate can tell the two apart.
//!
//! `MOVES_REPORT=1` prints every miss, `MOVES_TRACE=<commandTime>` the
//! prediction cmd by cmd up to that snapshot.
//!
//! Needs `COD_DIR`; without the paks it returns early.

use std::collections::{BTreeMap, HashMap};
use vcod_common::collision::CollisionWorld;
use vcod_common::movetrace::MoveWorld;
use vcod_common::net::msg::{self, NULL_USERCMD, UserCmd};
use vcod_common::net::protocol::{PROTOCOL_V1, PROTOCOL_V6, Protocol};
use vcod_common::pmove::predict::{self, Predicted};
use vcod_common::pmove::{MoveRules, Stance};
use vcod_common::weapon::WeaponDef;

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/playerstate");

/// Retail's configstring 7 on mp_carentan dm; 1.5b sends the same list
/// (docs/research/cod15-protocol-diff.md, section 7).
fn retail_cs7() -> &'static str {
    include_str!("fixtures/configstrings/mp_carentan-dm.txt")
        .lines()
        .find_map(|l| l.strip_prefix("7 "))
        .expect("configstring 7 in the fixture")
}

fn load_world(fs: &vcod_common::pk3::Pk3Fs, map: &str) -> (CollisionWorld, String) {
    let bsp_path = fs.resolve_map(map).expect("map in the mounted paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).expect("read the bsp")).expect("parse");
    let world = CollisionWorld::build(&bsp, &vcod_common::props::collision_tris(fs, &bsp.entities));
    (world, bsp.entities.clone())
}

enum Line {
    Cmd(UserCmd),
    Snap(Box<msg::PlayerState>),
}

fn parse(p: &Protocol, text: &str) -> Vec<Line> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some((kind, rest)) = line.split_once(' ') else {
            continue;
        };
        let kv: BTreeMap<&str, &str> = rest
            .split_whitespace()
            .filter_map(|kv| kv.split_once('='))
            .collect();
        let int = |k: &str| -> i32 { kv[k].parse().unwrap_or_else(|_| panic!("{k} in {line}")) };
        let pairs = |k: &str| -> Vec<(&str, i32)> {
            kv.get(k)
                .into_iter()
                .flat_map(|v| v.split(','))
                .filter(|s| !s.is_empty())
                .map(|s| {
                    let (a, b) = s.rsplit_once(':').expect("name:value");
                    (a, b.parse().expect("an int"))
                })
                .collect()
        };
        match kind {
            "!cmd" => {
                let a: Vec<i32> = kv["angles"]
                    .split(',')
                    .map(|x| x.parse().unwrap())
                    .collect();
                out.push(Line::Cmd(UserCmd {
                    server_time: int("st"),
                    buttons: int("buttons") as u8,
                    wbuttons: int("wbuttons") as u8,
                    weapon: int("weapon") as u8,
                    angles: [a[0], a[1], a[2]],
                    forward: int("forward") as i8,
                    right: int("right") as i8,
                    up: int("up") as i8,
                    ..NULL_USERCMD
                }));
            }
            "!snap" => {
                let mut w = msg::PlayerState::null(p);
                for (name, v) in pairs("wire") {
                    let i = msg::PlayerState::field_index(p, name).expect(name);
                    w.fields[i] = v;
                }
                for (k, v) in pairs("ammo") {
                    w.arrays.ammo[k.parse::<usize>().unwrap()] = v as i16;
                }
                for (k, v) in pairs("clip") {
                    w.arrays.ammoclip[k.parse::<usize>().unwrap()] = v as i16;
                }
                out.push(Line::Snap(Box::new(w)));
            }
            _ => {}
        }
    }
    out
}

/// One compared field: its name and its value on each side, printed.
type Field = (&'static str, String, String);

/// The fields prediction draws from, predicted against retail's next
/// snapshot. Floats compare within `eps`; the rest exactly.
fn compare(pred: &Predicted, truth: &Predicted) -> Vec<Field> {
    let mut bad = Vec::new();
    let mut f = |name: &'static str, a: f32, b: f32, eps: f32| {
        if (a - b).abs() > eps {
            bad.push((name, format!("{a}"), format!("{b}")));
        }
    };
    let (a, b) = (&pred.ps, &truth.ps);
    let d = a.origin - b.origin;
    f("origin.xy", d.truncate().length(), 0.0, 0.05);
    f("origin.z", d.z, 0.0, 0.05);
    let dv = a.velocity - b.velocity;
    f("velocity", dv.length(), 0.0, 1.5);
    f("viewHeightCurrent", a.view_height(), b.view_height(), 0.01);
    f("fJumpPeak", a.jump_origin_z, b.jump_origin_z, 0.01);
    f(
        "aimSpreadScale",
        a.aim_spread_scale,
        b.aim_spread_scale,
        0.5,
    );
    f(
        "fWeaponPosFrac",
        a.weapon_pos_frac,
        b.weapon_pos_frac,
        0.001,
    );
    f("pm_time", a.knockback_ms, b.knockback_ms, 0.5);
    f("proneDirection", a.prone_direction, b.prone_direction, 0.01);
    // Only 1.5's rules step the target; 1.1's set it with the flags.
    if a.rules == MoveRules::V1_5 {
        f("viewHeightTarget", a.view_target, b.view_target, 0.0);
    }
    let mut e = |name: &'static str, a: i64, b: i64| {
        if a != b {
            bad.push((name, a.to_string(), b.to_string()));
        }
    };
    e(
        "groundEntityNum",
        a.ground_entity_num().into(),
        b.ground_entity_num().into(),
    );
    let stance = |s: Stance| s as i64;
    e("stance", stance(a.stance), stance(b.stance));
    e("ducked", a.ducked.into(), b.ducked.into());
    e("prone_dive", a.prone_dive.into(), b.prone_dive.into());
    e("jump_latched", a.jump_latched.into(), b.jump_latched.into());
    e("jump_penalty", a.jump_penalty.into(), b.jump_penalty.into());
    e(
        "knockback_flags",
        a.knockback_flags.into(),
        b.knockback_flags.into(),
    );
    e(
        "jumpTime",
        (pred.command_time as f32 - a.since_jump_ms) as i64,
        (truth.command_time as f32 - b.since_jump_ms) as i64,
    );
    e(
        "viewHeightLerpTime",
        pred.view_lerp_start.into(),
        truth.view_lerp_start.into(),
    );
    e(
        "viewHeightLerpTarget",
        a.view_lerp_target as i64,
        b.view_lerp_target as i64,
    );
    e("weaponstate", a.weaponstate.into(), b.weaponstate.into());
    e(
        "weaponTime",
        a.weapon_time_ms.into(),
        b.weapon_time_ms.into(),
    );
    e("bobCycle", a.bob_cycle.into(), b.bob_cycle.into());
    e("movementDir", a.movement_dir.into(), b.movement_dir.into());
    for (i, name) in ["delta_angles[0]", "delta_angles[1]"]
        .into_iter()
        .enumerate()
    {
        e(
            name,
            pred.delta_angles[i].into(),
            truth.delta_angles[i].into(),
        );
    }
    bad
}

/// Per field, the snapshots predicted wrong, and how many were compared.
struct Report {
    compared: usize,
    misses: BTreeMap<&'static str, Vec<String>>,
}

impl Report {
    fn count(&self, field: &str) -> usize {
        self.misses.get(field).map_or(0, Vec::len)
    }
}

/// Each snapshot rebuilt and predicted to the next one under `rules`.
fn replay(
    p: &Protocol,
    rules: MoveRules,
    lines: &[Line],
    world: &CollisionWorld,
    weapons: &[Option<WeaponDef>],
) -> Report {
    let cmds: Vec<UserCmd> = lines
        .iter()
        .filter_map(|l| match l {
            Line::Cmd(c) => Some(*c),
            _ => None,
        })
        .collect();
    let by_time: HashMap<i32, UserCmd> = cmds.iter().map(|c| (c.server_time, *c)).collect();
    let rebuild = |w: &msg::PlayerState| {
        let ct = w.field_i32(p, "commandTime");
        let mut pred = predict::from_wire(p, w, by_time.get(&ct));
        pred.ps.rules = rules;
        pred
    };
    let snaps: Vec<&msg::PlayerState> = lines
        .iter()
        .filter_map(|l| match l {
            Line::Snap(s) => Some(&**s),
            _ => None,
        })
        .collect();
    let mw = MoveWorld::bare(world);
    let mut report = Report {
        compared: 0,
        misses: BTreeMap::new(),
    };
    for pair in snaps.windows(2) {
        let (from, to) = (rebuild(pair[0]), rebuild(pair[1]));
        if to.command_time <= from.command_time {
            continue;
        }
        let mut pred = from;
        let trace = std::env::var("MOVES_TRACE")
            .ok()
            .and_then(|v| v.parse::<i32>().ok())
            == Some(to.command_time);
        for c in cmds
            .iter()
            .filter(|c| c.server_time > from.command_time && c.server_time <= to.command_time)
        {
            predict::run_cmd(&mut pred, c, &mw, weapons);
            if trace {
                println!(
                    "  st {} origin {} vel {} ground {} pm_time {}",
                    c.server_time,
                    pred.ps.origin,
                    pred.ps.velocity,
                    pred.ps.on_ground,
                    pred.ps.knockback_ms
                );
            }
        }
        if pred.command_time != to.command_time {
            // The cmd that ended the frame was not captured.
            continue;
        }
        report.compared += 1;
        for (name, ours, theirs) in compare(&pred, &to) {
            report.misses.entry(name).or_default().push(format!(
                "ct {}: ours {ours} retail {theirs}",
                to.command_time
            ));
        }
    }
    report
}

fn print(name: &str, r: &Report) {
    println!("{name}: {} snapshots compared", r.compared);
    for (field, misses) in &r.misses {
        println!("  {field}: {} wrong, first {}", misses.len(), misses[0]);
        if std::env::var_os("MOVES_REPORT").is_some() {
            for m in misses {
                println!("    {m}");
            }
        }
    }
}

struct Capture {
    file: &'static str,
    protocol: &'static Protocol,
    rules: MoveRules,
}

const CAPTURES: &[Capture] = &[
    Capture {
        file: MOVES_1_1,
        protocol: &PROTOCOL_V1,
        rules: MoveRules::V1_1,
    },
    Capture {
        file: MOVES_1_5,
        protocol: &PROTOCOL_V6,
        rules: MoveRules::V1_5,
    },
];

const MOVES_1_1: &str = "mp_carentan-dm-slope-8ms-moves-1.1.txt";
const MOVES_1_5: &str = "mp_carentan-dm-slope-8ms-moves-1.5.txt";

/// Fields a capture's replay may miss, with how many snapshots and why.
/// - `bobCycle`: vcod's footstep cadence drifts a tick off retail's under
///   both patches and restarts on a different frame after a stop; it is
///   1.1's own gap, unmodelled in `pmove::footsteps` (`effective_stance`
///   and the ADS walk do not reach the step rate there).
/// - `aimSpreadScale`: the prone press while running, where retail's
///   `PM_Weapon` stores 255 for a prone body with move keys (1.1 0x39254,
///   1.5 0x30643) and vcod does not; again both patches.
/// - `velocity` on 1.5: one frame past a jump's apex where retail keeps the
///   fall and vcod takes the move as the velocity, the frame's travel within
///   an ulp of half its speed (`clamp_velocity_to_move`).
const GAPS: &[(&str, &str, usize)] = &[
    (MOVES_1_1, "bobCycle", 146),
    (MOVES_1_1, "aimSpreadScale", 1),
    (MOVES_1_5, "bobCycle", 207),
    (MOVES_1_5, "aimSpreadScale", 1),
    (MOVES_1_5, "velocity", 1),
];

fn setup() -> Option<(CollisionWorld, Vec<Option<WeaponDef>>)> {
    let fs = vcod_common::testing::game_fs()?;
    let weapons = vcod_common::weapon_table::from_configstring(&fs, retail_cs7());
    let (world, entities) = load_world(&fs, "mp_carentan");
    world.unlink_script_brushes(&entities, "dm");
    Some((world, weapons))
}

#[test]
fn the_predictor_replays_each_patch_under_its_own_rules() {
    let Some((world, weapons)) = setup() else {
        return;
    };
    let mut failures = Vec::new();
    for c in CAPTURES {
        let text = std::fs::read_to_string(format!("{FIXTURE_DIR}/{}", c.file)).unwrap();
        let r = replay(
            c.protocol,
            c.rules,
            &parse(c.protocol, &text),
            &world,
            &weapons,
        );
        print(c.file, &r);
        assert!(
            r.compared > 500,
            "{}: only {} snapshots",
            c.file,
            r.compared
        );
        for (field, misses) in &r.misses {
            let allowed = GAPS
                .iter()
                .find(|(f, name, _)| *f == c.file && name == field)
                .map_or(0, |g| g.2);
            if misses.len() > allowed {
                failures.push(format!("{}: {field} {} wrong", c.file, misses.len()));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The 1.5 capture under 1.1's rules: the jump penalty, the missing dive
/// and the stepped eye put the body and the eye off on enough snapshots
/// that the gate above would fail.
#[test]
fn the_1_5_capture_fails_under_1_1_rules() {
    let Some((world, weapons)) = setup() else {
        return;
    };
    let c = &CAPTURES[1];
    let text = std::fs::read_to_string(format!("{FIXTURE_DIR}/{}", c.file)).unwrap();
    let r = replay(
        c.protocol,
        MoveRules::V1_1,
        &parse(c.protocol, &text),
        &world,
        &weapons,
    );
    print("1.5 under 1.1 rules", &r);
    let off: usize = [
        "origin.xy",
        "origin.z",
        "velocity",
        "viewHeightCurrent",
        "pm_time",
    ]
    .iter()
    .map(|f| r.count(f))
    .sum();
    assert!(off > 50, "{off}");
}
