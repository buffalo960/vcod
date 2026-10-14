//! `--probe-vehicle driver|gunner`: the client half of the United Offensive
//! vehicle capture. `client-probes/probe_vehicle` puts client 0 beside the
//! first tank's right flank and client 1 beside its left, facing the hull;
//! the driver taps use 2 s after its spawn and the gunner 4 s after, so
//! they take seats 1 and 2, then each runs a fixed script of moves, view
//! sweeps and trigger pulls. Prints a `VEH` line per snapshot for every
//! vehicle and every seated player. Writes no fixture of its own: pair it
//! with `--save-snapshots --capture-tag <tag>` on one of the two.

use std::time::Instant;
use vcod_common::net::msg::{BUTTON_ATTACK, BUTTON_MELEE, BUTTON_USE, UserCmd};
use vcod_common::net::protocol::Protocol;
use vcod_common::net::snapshot::Snapshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum VehicleRole {
    Driver,
    Gunner,
}

/// One step: how long it holds, the moves, the buttons, and the view as
/// offsets from the view at the use tap, ramped linearly from the previous
/// step's.
struct Step {
    label: &'static str,
    ms: u128,
    forward: i8,
    right: i8,
    buttons: u8,
    /// Pitch and yaw offsets in degrees, reached at the step's end.
    view: [f32; 2],
}

const fn step(
    label: &'static str,
    ms: u128,
    forward: i8,
    right: i8,
    buttons: u8,
    view: [f32; 2],
) -> Step {
    Step {
        label,
        ms,
        forward,
        right,
        buttons,
        view,
    }
}

/// Steps that end early once a vehicle in the snapshot has these seats
/// taken (`animMovetype` bit n for seat n): the gunner waits for the
/// driver before its use tap, the driver for both before it drives.
const WAIT_DRIVER: &str = "wait_driver";
const CREW: &str = "crew";

fn gate(label: &str) -> Option<i32> {
    match label {
        WAIT_DRIVER => Some(2),
        CREW => Some(6),
        _ => None,
    }
}

/// From the spawn: the driver climbs in, waits for the gunner, drives straight, turns right while the turret
/// swings, stops and fires the main gun, reverses turning left, holds the
/// coaxial MG (button 0x20), then fires again.
const DRIVER: &[Step] = &[
    step("settle", 2000, 0, 0, 0, [0.0, 0.0]),
    step("use", 150, 0, 0, BUTTON_USE, [0.0, 0.0]),
    step(CREW, 20_000, 0, 0, 0, [0.0, 0.0]),
    step("ahead", 3000, 127, 0, 0, [0.0, 0.0]),
    step("turn_right", 3000, 127, 127, 0, [-5.0, -60.0]),
    step("stop", 1000, 0, 0, 0, [-5.0, -60.0]),
    step("fire", 150, 0, 0, BUTTON_ATTACK, [-5.0, -60.0]),
    step("after_fire", 850, 0, 0, 0, [-5.0, -60.0]),
    step("reverse_left", 2000, -127, -127, 0, [5.0, 20.0]),
    step("coax", 1000, 0, 0, BUTTON_MELEE, [5.0, 20.0]),
    step("stop2", 1000, 0, 0, 0, [0.0, 0.0]),
    step("fire2", 150, 0, 0, BUTTON_ATTACK, [0.0, 0.0]),
    step("hold", 60_000, 0, 0, 0, [0.0, 0.0]),
];

/// From the spawn: the gunner climbs in beside the waiting driver, sweeps its gun
/// left and right and fires two bursts.
const GUNNER: &[Step] = &[
    step("settle", 1500, 0, 0, 0, [0.0, 0.0]),
    step(WAIT_DRIVER, 20_000, 0, 0, 0, [0.0, 0.0]),
    step("use", 150, 0, 0, BUTTON_USE, [0.0, 0.0]),
    step(CREW, 5000, 0, 0, 0, [0.0, 0.0]),
    step("seated", 2000, 0, 0, 0, [0.0, 0.0]),
    step("sweep_left", 2000, 0, 0, 0, [-20.0, 90.0]),
    step("burst", 1000, 0, 0, BUTTON_ATTACK, [-20.0, 90.0]),
    step("sweep_right", 3000, 0, 0, 0, [10.0, -60.0]),
    step("burst2", 1000, 0, 0, BUTTON_ATTACK, [10.0, -60.0]),
    step("back", 2000, 0, 0, 0, [0.0, 0.0]),
    step("hold", 60_000, 0, 0, 0, [0.0, 0.0]),
];

pub struct VehicleProbe {
    role: VehicleRole,
    spawned: Option<Instant>,
    /// The view at the use tap, pitch and yaw in degrees.
    base: Option<[f32; 2]>,
    label: &'static str,
    last_snap: Option<u32>,
    /// Script time skipped by [`CREW`] steps that ended early.
    skipped: u128,
}

impl VehicleProbe {
    pub fn new(role: VehicleRole) -> VehicleProbe {
        VehicleProbe {
            role,
            spawned: None,
            base: None,
            label: "",
            last_snap: None,
            skipped: 0,
        }
    }

    /// The cmd for `now`; `spawned` once the playerstate is a live player.
    pub fn cmd(
        &mut self,
        now: Instant,
        spawned: bool,
        newest: Option<&Snapshot>,
        p: &Protocol,
    ) -> UserCmd {
        let mut cmd = UserCmd::default();
        let view = newest.map(|s| s.ps.viewangles(p));
        if spawned && self.spawned.is_none() {
            self.spawned = Some(now);
        }
        let Some(t0) = self.spawned else {
            return cmd;
        };
        let script = match self.role {
            VehicleRole::Driver => DRIVER,
            VehicleRole::Gunner => GUNNER,
        };
        let seated = |mask: i32| {
            newest.is_some_and(|s| {
                s.entities.values().any(|e| {
                    e.field_i32(p, "eType") == vcod_common::net::flags::ET_VEHICLE_V22
                        && e.field_i32(p, "animMovetype") & mask == mask
                })
            })
        };
        let mut ms = now.saturating_duration_since(t0).as_millis() + self.skipped;
        let mut from = [0.0f32; 2];
        let mut cur = &script[script.len() - 1];
        let mut frac = 1.0;
        for s in script {
            if ms < s.ms {
                if gate(s.label).is_some_and(seated) {
                    self.skipped += s.ms - ms;
                    ms = 0;
                    from = s.view;
                    continue;
                }
                cur = s;
                frac = ms as f32 / s.ms as f32;
                break;
            }
            ms -= s.ms;
            from = s.view;
        }
        if cur.label != self.label {
            println!("VEH step {} {}", self.label_ms(now), cur.label);
            self.label = cur.label;
        }
        // The view the gsc set, held until the tap, then the script's
        // offsets from it.
        if cur.buttons & BUTTON_USE != 0 && self.base.is_none() {
            self.base = view.map(|v| [v[0], v[1]]);
        }
        let base = match (self.base, view) {
            (Some(b), _) => b,
            (None, Some(v)) => [v[0], v[1]],
            (None, None) => [0.0, 0.0],
        };
        let off = [0, 1].map(|i| from[i] + (cur.view[i] - from[i]) * frac);
        cmd.angles = [
            deg_to_short(base[0] + off[0]),
            deg_to_short(base[1] + off[1]),
            0,
        ];
        cmd.forward = cur.forward;
        cmd.right = cur.right;
        cmd.buttons = cur.buttons;
        cmd
    }

    fn label_ms(&self, now: Instant) -> u128 {
        self.spawned
            .map_or(0, |t| now.saturating_duration_since(t).as_millis())
    }

    /// A `VEH` line per vehicle and per player, once per snapshot.
    pub fn report(&mut self, s: &Snapshot, p: &Protocol) {
        if self.last_snap == Some(s.message_num) {
            return;
        }
        self.last_snap = Some(s.message_num);
        let ps = |n: &str| s.ps.field_i32(p, n);
        let (o, v) = (s.ps.origin(p), s.ps.viewangles(p));
        println!(
            "VEH {} ps pos {:.1} {:.1} {:.1} view {:.2} {:.2} vehPos {} vehType {} vehMotion {} \
             locked {} pm_flags {:#x} eflags {:#x} weapon {}",
            s.server_time,
            o[0],
            o[1],
            o[2],
            v[0],
            v[1],
            ps("vehPos"),
            ps("vehType"),
            ps("vehMotion"),
            ps("viewlocked_entNum"),
            ps("pm_flags"),
            ps("eFlags"),
            ps("weapon"),
        );
        let vehicle = vcod_common::net::flags::ET_VEHICLE_V22;
        let seated = vcod_common::net::flags::EF_IN_VEHICLE_V22;
        for (num, e) in &s.entities {
            let i = |n: &str| e.field_i32(p, n);
            let f = |n: &str| e.field_f32(p, n);
            let o = e.origin(p);
            if i("eType") == vehicle {
                println!(
                    "VEH {} ent {num} pos {:.1} {:.1} {:.1} ang {:.2} {:.2} {:.2} time {} time2 {} \
                     origin2 {} {:.2} {} angles2 {:.2} {:.2} {:.2} leanf {:.2} seats {} driver {} \
                     weapon {} surf {} eflags {:#x} ev {}",
                    s.server_time,
                    o[0],
                    o[1],
                    o[2],
                    f("apos.trBase[0]"),
                    f("apos.trBase[1]"),
                    f("apos.trBase[2]"),
                    i("time"),
                    i("time2"),
                    i("origin2[0]"),
                    f("origin2[1]"),
                    i("origin2[2]"),
                    f("angles2[0]"),
                    f("angles2[1]"),
                    f("angles2[2]"),
                    f("leanf"),
                    i("animMovetype"),
                    i("otherEntityNum"),
                    i("weapon"),
                    i("surfType"),
                    i("eFlags"),
                    i("eventSequence"),
                );
            } else if i("eFlags") & seated != 0 || i("eType") == vcod_common::net::flags::ET_PLAYER
            {
                println!(
                    "VEH {} rider {num} pos {:.1} {:.1} {:.1} ang {:.2} {:.2} {:.2} veh {} surf {:#x} \
                     legs {} torso {} eflags {:#x} type {}",
                    s.server_time,
                    o[0],
                    o[1],
                    o[2],
                    f("apos.trBase[0]"),
                    f("apos.trBase[1]"),
                    f("apos.trBase[2]"),
                    i("otherEntityNum"),
                    i("surfType"),
                    i("legsAnim"),
                    i("torsoAnim"),
                    i("eFlags"),
                    i("pos.trType"),
                );
            }
        }
    }
}

/// ANGLE2SHORT: the wire's 16 bits over a full turn.
fn deg_to_short(deg: f32) -> i32 {
    (deg * 65536.0 / 360.0).round() as i32
}
