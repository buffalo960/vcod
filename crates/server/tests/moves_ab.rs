//! The client's predictor against retail's mover under each patch's rules.
//!
//! A third half replays United Offensive's sprint and bipod capture
//! (`the_predictor_replays_uo_sprint_and_bipod`, below) under UO's rules.
//!
//! The fixtures are `--save-slope --probe-moves` runs of one movement script
//! (`moves_script` in `crates/client/src/probe.rs`: jumps inside and past
//! 1.5's jump penalty, crouch, prone, the prone press while running,
//! strafes, hip and sighted shots) against the retail 1.1d server and the
//! retail 1.5b server, tagged `moves-1.1` and `moves-1.5`, and two
//! `--probe-turns` runs of `turns_script` (prone turns where the legs meet the
//! kerb, a turn past the yaw cap, lean, sighted and crouched gaits), tagged
//! `turns-1.1` and `turns-1.5`. Every snapshot
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
use vcod_common::net::event_ids::event_v22_to_v1;
use vcod_common::net::msg::{self, NULL_USERCMD, UserCmd};
use vcod_common::net::protocol::{PROTOCOL_V1, PROTOCOL_V6, PROTOCOL_V22, Protocol};
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
    /// The playerstate, and in a UO seat its vehicle's angles (`vang`).
    Snap(Box<msg::PlayerState>, Option<[f32; 3]>),
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
                let vang = kv.get("vang").map(|v| {
                    let a: Vec<f32> = v.split(',').map(|x| x.parse().unwrap()).collect();
                    [a[0], a[1], a[2]]
                });
                out.push(Line::Snap(Box::new(w), vang));
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
    // In a UO seat the vehicle carries the body after pmove
    // (`G_PlayerVehiclePositionAndBlend`), and nothing writes the velocity.
    if a.seat.is_none() && b.seat.is_none() {
        let d = a.origin - b.origin;
        f("origin.xy", d.truncate().length(), 0.0, 0.05);
        f("origin.z", d.z, 0.0, 0.05);
        let dv = a.velocity - b.velocity;
        f("velocity", dv.length(), 0.0, 1.5);
    }
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
    if a.rules.v15() {
        f("viewHeightTarget", a.view_target, b.view_target, 0.0);
    }
    if a.rules == MoveRules::Uo {
        f("fatigueScale", a.fatigue, b.fatigue, 1e-4);
    }
    let uo = a.rules == MoveRules::Uo;
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
    e("ads", a.ads_active.into(), b.ads_active.into());
    e("sprinting", a.sprinting.into(), b.sprinting.into());
    e("winded", a.winded.into(), b.winded.into());
    e(
        "lastSprintTime",
        a.last_sprint_time.into(),
        b.last_sprint_time.into(),
    );
    e(
        "weaponTime",
        a.weapon_time_ms.into(),
        b.weapon_time_ms.into(),
    );
    e("bobCycle", a.bob_cycle.into(), b.bob_cycle.into());
    // UO's cook (the fuse, the trigger's pin, the spoon and throw events)
    // and its winded breaths.
    if uo {
        e(
            "grenadeTimeLeft",
            a.grenade_time_left_ms.into(),
            b.grenade_time_left_ms.into(),
        );
        e(
            "weaponDelay",
            a.weapon_delay_ms.into(),
            b.weapon_delay_ms.into(),
        );
        e("eventSequence", pred.ring.seq.into(), truth.ring.seq.into());
        e(
            "iFatigueSoundTime",
            a.fatigue_sound_time.into(),
            b.fatigue_sound_time.into(),
        );
    }
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
    let rebuild = |(w, vang): (&msg::PlayerState, Option<[f32; 3]>)| {
        let ct = w.field_i32(p, "commandTime");
        let mut pred = predict::from_wire(p, w, by_time.get(&ct));
        pred.ps.rules = rules;
        if let (Some(seat), Some(a)) = (&mut pred.ps.seat, vang) {
            seat.angles = a;
        }
        pred
    };
    let snaps: Vec<(&msg::PlayerState, Option<[f32; 3]>)> = lines
        .iter()
        .filter_map(|l| match l {
            Line::Snap(s, vang) => Some((&**s, *vang)),
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
        let from_seq = from.ring.seq;
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
                    "  st {} origin {} vel {} ground {} pm_time {} bob {} yaw {} pdir {} da {:?} spread {}",
                    c.server_time,
                    pred.ps.origin,
                    pred.ps.velocity,
                    pred.ps.on_ground,
                    pred.ps.knockback_ms,
                    pred.ps.bob_cycle,
                    pred.ps.yaw.to_degrees(),
                    pred.ps.prone_direction,
                    pred.delta_angles,
                    pred.ps.aim_spread_scale
                );
            }
        }
        if pred.command_time != to.command_time {
            // The cmd that ended the frame was not captured.
            continue;
        }
        report.compared += 1;
        let mut bad = compare(&pred, &to);
        // UO's new events by id, in 1.1's numbering: the spoon, the throw,
        // the breaths and the sprint's own footsteps.
        if rules == MoveRules::Uo && pred.ring.seq == to.ring.seq {
            let gained = (to.ring.seq - from_seq) & 0xff;
            for k in (to.ring.seq - gained.min(4))..to.ring.seq {
                let slot = (k & 3) as usize;
                let theirs = event_v22_to_v1(to.ring.events[slot]);
                if pred.ring.events[slot] != theirs {
                    bad.push((
                        "events",
                        pred.ring.events[slot].to_string(),
                        theirs.to_string(),
                    ));
                }
            }
        }
        for (name, ours, theirs) in bad {
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
    Capture {
        file: TURNS_1_1,
        protocol: &PROTOCOL_V1,
        rules: MoveRules::V1_1,
    },
    Capture {
        file: TURNS_1_5,
        protocol: &PROTOCOL_V6,
        rules: MoveRules::V1_5,
    },
];

const MOVES_1_1: &str = "mp_carentan-dm-slope-8ms-moves-1.1.txt";
const MOVES_1_5: &str = "mp_carentan-dm-slope-8ms-moves-1.5.txt";
const TURNS_1_1: &str = "mp_carentan-dm-slope-8ms-turns-1.1.txt";
const TURNS_1_5: &str = "mp_carentan-dm-slope-8ms-turns-1.5.txt";

/// Fields a capture's replay may miss, with how many snapshots and why.
/// - `velocity` on 1.5: one frame past a jump's apex where retail keeps the
///   fall and vcod takes the move as the velocity, the frame's travel within
///   an ulp of half its speed (`clamp_velocity_to_move`).
/// - The 1.1 turns capture at ct 37293-37444: the prone body, crawled back
///   onto the kerb's edge, slides off it airborne; retail gains 3 units a
///   second more along the slide than ours over three snapshots, and the
///   step phase follows a tick off. 1.1's own slide off an edge, not the
///   turn; the 1.5 run of the same script never left the ground there.
const GAPS: &[(&str, &str, usize)] = &[
    (MOVES_1_5, "velocity", 1),
    (TURNS_1_1, "velocity", 3),
    (TURNS_1_1, "origin.xy", 2),
    (TURNS_1_1, "bobCycle", 1),
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

    // The turns capture: 1.1's swing takes the legs where 1.5's search
    // refuses them, and 1.1 has no view pull.
    let c = &CAPTURES[3];
    let text = std::fs::read_to_string(format!("{FIXTURE_DIR}/{}", c.file)).unwrap();
    let r = replay(
        c.protocol,
        MoveRules::V1_1,
        &parse(c.protocol, &text),
        &world,
        &weapons,
    );
    print("1.5 turns under 1.1 rules", &r);
    let off = r.count("proneDirection") + r.count("delta_angles[1]");
    assert!(off > 20, "{off}");
}

const SPRINT_UO: &str = "mp_carentan-dm-slope-8ms-sprint-uo.txt";

/// United Offensive's sprint and bipod (`--probe-sprint`, `sprint_script`
/// in `crates/client/src/probe.rs`, docs/research/coduo-movement.md): a
/// sprint to exhaustion, its regeneration, sprint with every other gait,
/// the bipod deployed prone, swung past its yaw cap, moved and fired, and
/// broken down. The weapon defs are UO's, so this runs only with `COD_DIR`
/// at a United Offensive install (its `uo/` over `main/`); mp_carentan is
/// the stock pak4 map both patches load. The same capture under 1.5's rules
/// must fail, which is what shows the sprint is measured.
#[test]
fn the_predictor_replays_uo_sprint_and_bipod() {
    let text = std::fs::read_to_string(format!("{FIXTURE_DIR}/{SPRINT_UO}")).unwrap();
    let Some((world, weapons)) = uo_setup(&text, "mp_carentan") else {
        return;
    };
    let lines = parse(&PROTOCOL_V22, &text);
    let r = replay(&PROTOCOL_V22, MoveRules::Uo, &lines, &world, &weapons);
    print(SPRINT_UO, &r);
    assert!(r.compared > 500, "only {} snapshots", r.compared);
    let failures = over(&r, UO_GAPS);
    assert!(failures.is_empty(), "{failures:#?}");

    let r = replay(&PROTOCOL_V22, MoveRules::V1_5, &lines, &world, &weapons);
    print("UO under 1.5 rules", &r);
    let off = r.count("origin.xy") + r.count("velocity") + r.count("sprinting");
    assert!(off > 50, "{off}");
}

/// Fields the UO capture's replay may miss, with how many snapshots and why.
/// - `aimSpreadScale` on the first pair: the capture holds no cmd at the
///   first snapshot's `commandTime`, so the turn term measures the first
///   cmd's yaw against none and opens the cone.
const UO_GAPS: &[(&str, usize)] = &[("aimSpreadScale", 1)];

const TANK_SEATS_UO: &str = "mp_cassino-tdm-slope-8ms-tank-seats-uo.txt";
const JEEP_SEATS_UO: &str = "mp_peaks-tdm-slope-8ms-jeep-seats-uo.txt";

/// United Offensive's seats against retail 1.51b: one probe through a
/// Panzer IV's driver and gunner seats and a Willys jeep's three
/// (`--probe-vehicle seats`, vehicle_probe.rs `SEATS`), each snapshot
/// replayed under UO's rules with its vehicle's angles. It covers the
/// 12000 pitch clamp, the driver's snapped sight, the gunner's ramped one,
/// the jeep passenger's arcs and its own weapon, and the walk after the
/// exit (docs/research/coduo-vehicles.md, 3 and 8). Needs `COD_DIR` at a UO
/// install, as the sprint gate does.
#[test]
fn the_predictor_replays_uo_seats() {
    let uo = vcod_common::testing::game_dir().join("uo");
    if !uo.is_dir() {
        return;
    }
    let fs = vcod_common::pk3::Pk3Fs::open(&uo).expect("the UO paks");
    for (file, map) in [(TANK_SEATS_UO, "mp_cassino"), (JEEP_SEATS_UO, "mp_peaks")] {
        let text = std::fs::read_to_string(format!("{FIXTURE_DIR}/{file}")).unwrap();
        let cs7 = text
            .lines()
            .find_map(|l| l.strip_prefix("# cs7 "))
            .expect("the capture names its weapon list");
        let weapons = vcod_common::weapon_table::from_configstring(&fs, cs7);
        let (world, entities) = load_world(&fs, map);
        world.unlink_script_brushes(&entities, "tdm");
        let lines = parse(&PROTOCOL_V22, &text);
        let seated = lines
            .iter()
            .filter(|l| matches!(l, Line::Snap(_, Some(_))))
            .count();
        assert!(seated > 400, "{file}: only {seated} seated snapshots");
        let r = replay(&PROTOCOL_V22, MoveRules::Uo, &lines, &world, &weapons);
        print(file, &r);
        let failures: Vec<String> = r
            .misses
            .iter()
            .filter(|(field, misses)| {
                let allowed = SEAT_GAPS
                    .iter()
                    .find(|(f, name, _)| *f == file && name == *field)
                    .map_or(0, |g| g.2);
                misses.len() > allowed
            })
            .map(|(field, misses)| format!("{file}: {field} {} wrong", misses.len()))
            .collect();
        assert!(failures.is_empty(), "{failures:#?}");

        // The same capture with every seat taken off the playerstate must
        // miss the clamps and the sight: that is what shows the seat is
        // measured.
        let on_foot: Vec<Line> = lines
            .into_iter()
            .map(|l| match l {
                Line::Snap(mut w, _) => {
                    let i = msg::PlayerState::field_index(&PROTOCOL_V22, "eFlags").unwrap();
                    w.fields[i] &= !vcod_common::net::flags::EF_IN_VEHICLE_V22;
                    Line::Snap(w, None)
                }
                cmd => cmd,
            })
            .collect();
        let r = replay(&PROTOCOL_V22, MoveRules::Uo, &on_foot, &world, &weapons);
        print("seats taken off", &r);
        let off =
            r.count("delta_angles[0]") + r.count("delta_angles[1]") + r.count("fWeaponPosFrac");
        assert!(off > 20, "{file}: {off}");
    }
}

/// Fields the seat captures' replays may miss, per capture, with how many
/// snapshots and why.
/// - `delta_angles` at each `nextvehslot`: the new seat's
///   `SetClientViewAngle` (game 0x8998d), the server's, not pmove's.
/// - `origin.z` and `velocity` on the exit's snapshot: `VEH_UnlinkPlayer`
///   sets the body down at the dismount spot.
/// - `aimSpreadScale` on the first pair, which has no cmd at its
///   `commandTime` (as `UO_GAPS`), and on the tank two later ones
///   (unexplained).
/// - On the tank, `aimSpreadScale` and `eventSequence` on the main gun's
///   snapshot: the driver's damage feedback (`P_DamageFeedback`: count 44
///   onto the cone, `EV_PAIN` 100), the server's.
/// - On the tank, `events` on the landing after the exit: retail lands on
///   a rock clip brush, ours on the caulk brush that meets it there.
/// - `weaponTime` and `weaponDelay` on the jeep passenger's rechamber:
///   retail's start one 8 ms cmd later (unexplained).
///
/// See docs/research/coduo-vehicles.md 7.
const SEAT_GAPS: &[(&str, &str, usize)] = &[
    (TANK_SEATS_UO, "delta_angles[0]", 2),
    (TANK_SEATS_UO, "delta_angles[1]", 2),
    (TANK_SEATS_UO, "origin.z", 1),
    (TANK_SEATS_UO, "velocity", 1),
    (TANK_SEATS_UO, "aimSpreadScale", 4),
    (TANK_SEATS_UO, "eventSequence", 1),
    (TANK_SEATS_UO, "events", 1),
    (JEEP_SEATS_UO, "delta_angles[0]", 2),
    (JEEP_SEATS_UO, "delta_angles[1]", 2),
    (JEEP_SEATS_UO, "aimSpreadScale", 1),
    (JEEP_SEATS_UO, "weaponTime", 1),
    (JEEP_SEATS_UO, "weaponDelay", 1),
];

/// What the server makes of a driver's cmd bytes, off the tank capture
/// (docs/research/coduo-vehicles.md 2): `forwardmove` drives the hull
/// without turning it, and `upmove` > 0 turns the hull to the view. These
/// are the bytes the client's stock keys put on a seated cmd
/// (`play::input`'s `a_seat_drives_with_the_stock_keys`).
#[test]
fn the_tank_capture_drives_and_turns_on_the_seat_cmds() {
    let text = std::fs::read_to_string(format!("{FIXTURE_DIR}/{TANK_SEATS_UO}")).unwrap();
    let lines = parse(&PROTOCOL_V22, &text);
    let p = &PROTOCOL_V22;
    // Per snapshot in the driver's seat: the last cmd before it, the body's
    // origin, the view's yaw and the hull's.
    let mut last = None;
    let mut rows = Vec::new();
    for l in &lines {
        match l {
            Line::Cmd(c) => last = Some(*c),
            Line::Snap(w, Some(vang)) if w.field_i32(p, "vehPos") == 1 => {
                if let Some(c) = last {
                    rows.push((
                        c,
                        glam::Vec3::from(w.origin(p)),
                        w.viewangles(p)[1],
                        vang[1],
                    ));
                }
            }
            _ => {}
        }
    }
    let ahead: Vec<_> = rows.iter().filter(|r| r.0.forward == 127).collect();
    let (first, end) = (ahead.first().unwrap(), ahead.last().unwrap());
    assert!(first.1.distance(end.1) > 200.0, "{} {}", first.1, end.1);
    assert!(
        ahead.iter().all(|r| r.3 == first.3),
        "forward alone turns nothing"
    );
    let turning: Vec<_> = rows.iter().filter(|r| r.0.up == 127).collect();
    let (first, end) = (turning.first().unwrap(), turning.last().unwrap());
    let off = |a: f32, b: f32| (a - b + 540.0).rem_euclid(360.0) - 180.0;
    assert!(off(end.3, first.3).abs() > 100.0, "{} {}", first.3, end.3);
    assert!(
        off(end.3, end.2).abs() < 3.0,
        "hull {} view {}",
        end.3,
        end.2
    );
}

/// The UO captures' weapon list, defs and world: `COD_DIR` at a United
/// Offensive install (its `uo/` over `main/`), or `None`.
fn uo_setup(text: &str, map: &str) -> Option<(CollisionWorld, Vec<Option<WeaponDef>>)> {
    let uo = vcod_common::testing::game_dir().join("uo");
    if !uo.is_dir() {
        return None;
    }
    let fs = vcod_common::pk3::Pk3Fs::open(&uo).expect("the UO paks");
    let cs7 = text
        .lines()
        .find_map(|l| l.strip_prefix("# cs7 "))
        .expect("the capture names its weapon list");
    let weapons = vcod_common::weapon_table::from_configstring(&fs, cs7);
    let (world, entities) = load_world(&fs, map);
    world.unlink_script_brushes(&entities, "dm");
    Some((world, weapons))
}

/// The fields of `r` past what `gaps` allows them.
fn over(r: &Report, gaps: &[(&str, usize)]) -> Vec<String> {
    r.misses
        .iter()
        .filter(|(field, misses)| {
            let allowed = gaps
                .iter()
                .find(|(name, _)| name == *field)
                .map_or(0, |g| g.1);
            misses.len() > allowed
        })
        .map(|(field, misses)| format!("{field} {} wrong", misses.len()))
        .collect()
}

const UOFOOT: [&str; 3] = [
    "mp_foy-dm-slope-8ms-uofoot-stand.txt",
    "mp_foy-dm-slope-8ms-uofoot-crouch.txt",
    "mp_foy-dm-slope-8ms-uofoot-mount.txt",
];

/// United Offensive's mounts and grenade cook (`--probe-uofoot`,
/// `uofoot_script` in `crates/client/src/probe.rs`, `client-probes/
/// probe_uofoot`, docs/research/coduo-movement.md 11 and 12) on mp_foy: the
/// mg30cal deployed standing on an `hmgstanding` brush, crouched on an
/// `hmgcrouching` one and crouched at a script `trigger_mount` the client
/// knows only through `serverCursorHint` 7, swung past its cap, fired and
/// broken down, then the frag thrown plain, cooked twice and cooked out.
/// Needs `COD_DIR` at a UO install, as the sprint gate does.
#[test]
fn the_predictor_replays_uo_mounts_and_cook() {
    let mut failures = Vec::new();
    for file in UOFOOT {
        let text = std::fs::read_to_string(format!("{FIXTURE_DIR}/{file}")).unwrap();
        let Some((world, weapons)) = uo_setup(&text, "mp_foy") else {
            return;
        };
        let lines = parse(&PROTOCOL_V22, &text);
        let r = replay(&PROTOCOL_V22, MoveRules::Uo, &lines, &world, &weapons);
        print(file, &r);
        assert!(r.compared > 500, "{file}: only {} snapshots", r.compared);
        let gaps: Vec<(&str, usize)> = UOFOOT_GAPS
            .iter()
            .filter(|(f, _, _)| file.contains(f))
            .map(|&(_, name, n)| (name, n))
            .collect();
        failures.extend(over(&r, &gaps).into_iter().map(|f| format!("{file}: {f}")));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Fields the mount and cook captures' replays may miss, per capture.
/// - `aimSpreadScale` on each first pair, as in [`UO_GAPS`].
/// - The trigger_mount's two deploys (ct 23991, 39483): the server's own
///   `pointcontents` sees the trigger's 0x400000 and deploys crouched, so
///   it never asks `G_CheckPointInsideTriggerMount` and sends hint 7 with
///   `serverCursorHintVal` 0, which the predictor (and, by the same code,
///   the retail cgame) reads as a standing mount. The next snapshot puts
///   the stance right (coduo-movement.md, 11).
const UOFOOT_GAPS: &[(&str, &str, usize)] = &[
    ("uofoot", "aimSpreadScale", 1),
    ("mount", "ducked", 2),
    ("mount", "stance", 2),
    ("mount", "viewHeightCurrent", 2),
    ("mount", "viewHeightLerpTarget", 2),
    ("mount", "viewHeightLerpTime", 2),
    ("mount", "viewHeightTarget", 2),
];
