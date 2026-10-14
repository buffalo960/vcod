//! A United Offensive vehicle on ours against retail's, cmd for cmd. The
//! fixtures are `--probe-vehicle seats|drive --save-slope` captures of
//! retail UO 1.51b (`client-probes/probe_vehicle`, the header names the
//! recipe): every usercmd the probe sent from its first seated snapshot on,
//! and every snapshot's playerstate and, on the drive captures, the
//! vehicle's whole entity state (`veh=`). The replay boards the same
//! vehicle on ours through the same probe gametype and the use key, then
//! sends the capture's cmds verbatim, rebased onto our clock, and diffs
//! each snapshot (docs/research/coduo-vehicles.md, "The server").
//!
//! `VEHICLE_REPORT=1` prints every snapshot's differences. Needs `COD_DIR`
//! at a UO install; without one it returns early.

mod common;

use common::{ClientEnd, Join, Queues, step};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::NetClient;
use vcod_common::net::NetEvent;
use vcod_common::net::msg::{BUTTON_USE, EntityState, PlayerState, UserCmd};
use vcod_common::net::protocol::PROTOCOL_V22;

/// One `!cmd` or `!snap` line.
enum Line {
    Cmd(UserCmd),
    Snap(Snap),
}

#[derive(Clone, Debug)]
struct Snap {
    t: i32,
    /// Every non-zero playerstate netfield as decoded.
    wire: BTreeMap<String, i32>,
    /// The seat's vehicle's every non-zero entity field (drive captures).
    veh: Option<BTreeMap<String, i32>>,
}

fn kv(s: &str) -> BTreeMap<String, i32> {
    s.split(',')
        .filter_map(|p| p.split_once(':'))
        .map(|(k, v)| (k.to_string(), v.parse().unwrap()))
        .collect()
}

fn parse(text: &str) -> Vec<Line> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some((kind, rest)) = line.split_once(' ') else {
            continue;
        };
        let field = |name: &str| {
            rest.split(' ')
                .find_map(|t| t.strip_prefix(&format!("{name}=")))
                .unwrap_or("")
        };
        match kind {
            "!cmd" => {
                let n = |name: &str| field(name).parse::<i32>().unwrap();
                let a: Vec<i32> = field("angles")
                    .split(',')
                    .map(|x| x.parse().unwrap())
                    .collect();
                out.push(Line::Cmd(UserCmd {
                    server_time: n("st"),
                    buttons: n("buttons") as u8,
                    wbuttons: n("wbuttons") as u8,
                    weapon: n("weapon") as u8,
                    up: n("up") as i8,
                    forward: n("forward") as i8,
                    right: n("right") as i8,
                    angles: [a[0], a[1], a[2]],
                    ..UserCmd::default()
                }));
            }
            "!snap" => {
                let veh = field("veh");
                out.push(Line::Snap(Snap {
                    t: field("t").parse().unwrap(),
                    wire: kv(field("wire")),
                    veh: (!veh.is_empty()).then(|| kv(veh)),
                }));
            }
            _ => {}
        }
    }
    out
}

/// Ours, one sample per retail snapshot.
struct Sample {
    ps: PlayerState,
    veh: Option<EntityState>,
}

/// A cmd that holds the view where the newest snapshot has it, stamped at
/// that snapshot's time so the capture's first cmd is never stale behind it.
fn holding(cl: &NetClient<ClientEnd>, buttons: u8) -> UserCmd {
    let p = &PROTOCOL_V22;
    let Some(s) = cl.snapshots().newest() else {
        return UserCmd::default();
    };
    let v = s.ps.viewangles(p);
    let da =
        ["delta_angles[0]", "delta_angles[1]", "delta_angles[2]"].map(|n| s.ps.field_i32(p, n));
    UserCmd {
        server_time: s.server_time,
        angles: std::array::from_fn(|i| ((v[i] * 65536.0 / 360.0).round() as i32 - da[i]) & 0xffff),
        buttons,
        weapon: s.ps.field_i32(p, "weapon") as u8,
        ..UserCmd::default()
    }
}

fn seated(cl: &NetClient<ClientEnd>) -> bool {
    cl.snapshots().newest().is_some_and(|s| {
        s.ps.field_i32(&PROTOCOL_V22, "eFlags") & vcod_common::net::flags::EF_IN_VEHICLE_V22 != 0
    })
}

/// Our replay of `lines` on `map`, boarding a `vehicletype` beside it.
fn ours(
    map: &str,
    vehicletype: &str,
    cvars: &[(&str, &str)],
    lines: &[Line],
) -> Option<Vec<Sample>> {
    let fs = vcod_common::testing::uo_fs()?;
    let p = &PROTOCOL_V22;
    let bsp_path = fs.resolve_map(map).expect("map in the UO paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(map, "probe_vehicle"), now);
    sv.set_protocol(p);
    sv.overlay_script(
        "maps/mp/gametypes/probe_vehicle",
        include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_vehicle.gsc"),
    );
    sv.set_cvar("probe_vehicletype", vehicletype);
    for (k, v) in cvars {
        sv.set_cvar(k, v);
    }
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    common::run_alone(&mut sv, &mut now, common::PROBE_JOIN_MS);
    let q = Rc::new(RefCell::new(Queues::default()));
    let mut cl = common::connect(&mut sv, &q, &mut now);
    let mut join = Join::new("axis", "kar98k_mp");
    let frame = Duration::from_millis(common::FRAME_MS as u64);
    let mut placed_at = None;
    // A cmd's clock rides as a delta on the last one's, so it must not go
    // back.
    let mut last_sent = 0;
    // Join, wait for the gsc's placement, settle two seconds, tap use for
    // 150 ms as the probe does, and wait for the seat.
    for i in 0..2000i64 {
        now += frame;
        let noboard = std::env::var("VEHICLE_NOBOARD").is_ok();
        if noboard && placed_at.is_some_and(|at| (i - at) * common::FRAME_MS >= 3000) {
            break;
        }
        let tap = !noboard
            && placed_at.is_some_and(|at| {
                (i - at) * common::FRAME_MS >= 2000 && (i - at) * common::FRAME_MS < 2150
            });
        if cl.snapshots().newest().is_some() {
            let mut cmd = holding(&cl, if tap { BUTTON_USE } else { 0 });
            cmd.server_time = cmd.server_time.max(last_sent + 1);
            last_sent = cmd.server_time;
            cl.send_cmds(&[cmd]);
        } else if let Some(c) = cl.send_frame(&UserCmd::default()) {
            last_sent = c.server_time;
        }
        for e in step(&mut sv, &q, &mut cl, now) {
            if let NetEvent::ServerCommand(tokens) = e {
                join.on_server_command(&tokens, &mut cl, now);
            }
        }
        if placed_at.is_none() && sv.script_log().iter().any(|l| l.contains("PROBE place")) {
            placed_at = Some(i);
        }
        if std::env::var("VEHICLE_CMDS").is_ok()
            && let Some(n) = cl.snapshots().newest()
        {
            eprintln!(
                "board t {} ct {} sent {last_sent}",
                n.server_time,
                n.ps.field_i32(&PROTOCOL_V22, "commandTime")
            );
        }
        if seated(&cl) {
            break;
        }
    }
    assert!(
        seated(&cl) || std::env::var("VEHICLE_NOBOARD").is_ok(),
        "never got into the {vehicletype}"
    );
    let first = lines.iter().find_map(|l| match l {
        Line::Snap(s) => Some(s.t),
        _ => None,
    })?;
    // The capture's first snapshot lands on a frame of ours past every cmd
    // sent so far; the frames between hold the seat with no input.
    let mut ours_first = cl.snapshots().newest()?.server_time;
    while ours_first <= last_sent + 50 {
        now += frame;
        step(&mut sv, &q, &mut cl, now);
        ours_first = cl.snapshots().newest()?.server_time;
    }
    let offset = ours_first - first;
    let mut out = Vec::new();
    let mut pending: Vec<UserCmd> = Vec::new();
    let mut started = false;
    for line in lines {
        match line {
            Line::Cmd(c) => {
                if started {
                    let mut c = *c;
                    c.server_time += offset;
                    pending.push(c);
                }
            }
            Line::Snap(s) => {
                if !started {
                    started = true;
                    out.push(sample(&cl));
                    continue;
                }
                let want = s.t + offset;
                for _ in 0..40 {
                    if cl
                        .snapshots()
                        .newest()
                        .is_some_and(|n| n.server_time >= want)
                    {
                        break;
                    }
                    now += frame;
                    let next = cl.snapshots().newest().map_or(0, |n| n.server_time) + 50;
                    let due: Vec<UserCmd> = pending
                        .iter()
                        .filter(|c| c.server_time <= next)
                        .copied()
                        .collect();
                    pending.retain(|c| c.server_time > next);
                    if std::env::var("VEHICLE_CMDS").is_ok() {
                        eprintln!("send {due:?}");
                    }
                    let due: Vec<UserCmd> = if std::env::var("VEHICLE_ONE").is_ok() {
                        due.last().copied().into_iter().collect()
                    } else {
                        due
                    };
                    cl.send_cmds(&due);
                    for e in step(&mut sv, &q, &mut cl, now) {
                        if let NetEvent::ServerCommand(tokens) = e {
                            join.on_server_command(&tokens, &mut cl, now);
                        }
                    }
                }
                out.push(sample(&cl));
                if std::env::var("VEHICLE_CMDS").is_ok() {
                    let n = cl.snapshots().newest().unwrap();
                    eprintln!(
                        "snap t {} ct {} view {:?}",
                        n.server_time,
                        n.ps.field_i32(&PROTOCOL_V22, "commandTime"),
                        n.ps.viewangles(&PROTOCOL_V22)
                    );
                }
            }
        }
    }
    Some(out)
}

fn sample(cl: &NetClient<ClientEnd>) -> Sample {
    let p = &PROTOCOL_V22;
    let s = cl.snapshots().newest().unwrap();
    let veh_num = s.ps.field_i32(p, "viewlocked_entNum") as u32;
    Sample {
        ps: s.ps.clone(),
        veh: s.entities.get(&veh_num).cloned(),
    }
}

fn f(v: i32) -> f32 {
    f32::from_bits(v as u32)
}

/// Per field, the snapshots that differ: a float to `tol`, an angle
/// modulo 360, an int exactly.
#[derive(Default)]
struct Tally {
    seen: BTreeMap<String, (usize, usize)>,
    report: Vec<String>,
}

impl Tally {
    fn check(&mut self, k: usize, name: &str, retail: Option<i32>, ours: i32, kind: Kind) {
        let r = retail.unwrap_or(0);
        let same = match kind {
            Kind::Int => r == ours,
            Kind::Mask(m) => r & m == ours & m,
            Kind::Float(tol) => (f(r) - f(ours)).abs() <= tol,
            Kind::Angle(tol) => {
                let d = (f(r) - f(ours)).rem_euclid(360.0);
                d.min(360.0 - d) <= tol
            }
        };
        let e = self.seen.entry(name.to_string()).or_default();
        e.0 += 1;
        if !same {
            e.1 += 1;
            let show = |v: i32| match kind {
                Kind::Float(_) | Kind::Angle(_) => format!("{}", f(v)),
                _ => format!("{v}"),
            };
            self.report.push(format!(
                "snap {k} {name}: retail {} ours {}",
                show(r),
                show(ours)
            ));
        }
    }

    fn misses(&self, name: &str) -> usize {
        self.seen.get(name).map_or(0, |e| e.1)
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Int,
    Mask(i32),
    Float(f32),
    Angle(f32),
}

const PS_FIELDS: &[(&str, Kind)] = &[
    ("pm_type", Kind::Int),
    ("eFlags", Kind::Mask(!0x10)),
    ("pm_flags", Kind::Int),
    ("vehPos", Kind::Int),
    ("vehType", Kind::Int),
    ("vehMotion", Kind::Int),
    ("viewlocked_entNum", Kind::Int),
    ("delta_angles[0]", Kind::Int),
    ("delta_angles[1]", Kind::Int),
    ("viewangles[0]", Kind::Angle(0.01)),
    ("viewangles[1]", Kind::Angle(0.01)),
    ("origin[0]", Kind::Float(0.5)),
    ("origin[1]", Kind::Float(0.5)),
    ("origin[2]", Kind::Float(0.5)),
    ("fWeaponPosFrac", Kind::Float(0.001)),
    ("legsAnim", Kind::Mask(511)),
    ("weapon", Kind::Int),
];

const VEH_FIELDS: &[(&str, Kind)] = &[
    ("pos.trBase[0]", Kind::Float(0.5)),
    ("pos.trBase[1]", Kind::Float(0.5)),
    ("pos.trBase[2]", Kind::Float(0.5)),
    ("apos.trBase[0]", Kind::Angle(0.1)),
    ("apos.trBase[1]", Kind::Angle(0.1)),
    ("apos.trBase[2]", Kind::Angle(0.1)),
    ("angles2[0]", Kind::Angle(0.1)),
    ("angles2[1]", Kind::Angle(0.1)),
    ("angles2[2]", Kind::Angle(0.1)),
    ("origin2[0]", Kind::Int),
    ("origin2[1]", Kind::Angle(0.1)),
    ("origin2[2]", Kind::Int),
    ("leanf", Kind::Float(0.1)),
    ("time", Kind::Int),
    ("time2", Kind::Int),
    ("animMovetype", Kind::Int),
    ("otherEntityNum", Kind::Int),
    ("eFlags", Kind::Int),
    ("weapon", Kind::Int),
    ("index", Kind::Int),
    ("solid", Kind::Int),
    ("xmodel", Kind::Int),
    ("surfType", Kind::Int),
    ("attackerEntityNum", Kind::Int),
    ("fTorsoPitch", Kind::Float(0.0)),
    ("fWaistPitch", Kind::Float(0.0)),
    ("loopSound", Kind::Int),
];

fn tally(lines: &[Line], ours: &[Sample]) -> Tally {
    let p = &PROTOCOL_V22;
    let snaps: Vec<&Snap> = lines
        .iter()
        .filter_map(|l| match l {
            Line::Snap(s) => Some(s),
            _ => None,
        })
        .collect();
    let mut t = Tally::default();
    for (k, (r, o)) in snaps.iter().zip(ours).enumerate() {
        for (name, kind) in PS_FIELDS {
            let i = PlayerState::field_index(p, name).unwrap();
            t.check(k, name, r.wire.get(*name).copied(), o.ps.fields[i], *kind);
        }
        if let (Some(rv), Some(ov)) = (&r.veh, &o.veh) {
            for (name, kind) in VEH_FIELDS {
                let i = EntityState::field_index(p, name).unwrap();
                let mut ours = ov.fields[i];
                // The wire keeps `time` to 16 bits and `time2` to 6.
                if *name == "time" {
                    ours &= 0xffff;
                }
                t.check(
                    k,
                    &format!("veh {name}"),
                    rv.get(*name).copied(),
                    ours,
                    *kind,
                );
            }
        }
    }
    if std::env::var("VEHICLE_REPORT").is_ok() {
        for l in &t.report {
            eprintln!("{l}");
        }
    }
    for (name, (n, miss)) in &t.seen {
        eprintln!("{name}: {miss} of {n} differ");
    }
    t
}

const TANK_SEATS: &str = "tests/fixtures/playerstate/mp_cassino-tdm-slope-8ms-tank-seats-uo.txt";

#[test]
fn the_tank_seats_replay_on_ours() {
    let text = std::fs::read_to_string(TANK_SEATS).unwrap();
    let lines = parse(&text);
    let Some(ours) = ours("mp_cassino", "panzeriv_mp", &[], &lines) else {
        return;
    };
    let t = tally(&lines, &ours);
    for name in ["vehPos", "vehType", "viewlocked_entNum", "pm_type"] {
        assert_eq!(t.misses(name), 0, "{name}");
    }
}
