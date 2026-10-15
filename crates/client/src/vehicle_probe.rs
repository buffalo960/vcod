//! `--probe-vehicle driver|gunner`: the client half of the United Offensive
//! vehicle capture. `client-probes/probe_vehicle` puts client 0 beside the
//! first tank's right flank and client 1 beside its left, facing the hull;
//! the driver taps use 2 s after its spawn and the gunner 4 s after, so
//! they take seats 1 and 2, then each runs a fixed script of moves, view
//! sweeps and trigger pulls. Prints a `VEH` line per snapshot for every
//! vehicle and every seated player. Writes no fixture of its own: pair it
//! with `--save-snapshots --capture-tag <tag>` on one of the two.
//!
//! `--probe-uo-weapon arty|smoke|mg` runs the same step machine on foot:
//! it takes the server's `a` select (`client-probes/probe_uofx` hands the
//! weapon over and switches to it a second after the spawn), then sights
//! and fires the artillery binoculars, throws the smoke grenade, or
//! deploys the bipod and holds the trigger; `hold` sights and holds the
//! trigger wherever the gsc aims it.

use std::time::Instant;
use vcod_common::net::msg::{BUTTON_ADS, BUTTON_ATTACK, BUTTON_MELEE, BUTTON_USE, UserCmd};
use vcod_common::net::protocol::Protocol;
use vcod_common::net::snapshot::Snapshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum VehicleRole {
    Driver,
    Gunner,
    /// The gunner holding its MG past the overheat (`fireHeat` 0.04 a
    /// round at 10 rounds/s: 2.5 s).
    Overheat,
    /// Alone in every seat in turn ([`SEATS`]): the pitch clamp, the sight,
    /// the hull turned to the view, the seat cycle and the exit. Pair it
    /// with `--save-slope --capture-tag <tag>` for the seated replay.
    Seats,
    /// Alone in the driver's seat ([`DRIVE`]): straight, a forward right
    /// turn, a reverse left turn, the hull turned to the view, the main gun
    /// and the coaxial MG, then out. Pair it with `--save-slope
    /// --capture-tag <tag>`: each snapshot line carries the vehicle's
    /// entity state as `veh=`, for the server's drive replay.
    Drive,
    /// Sits still in every seat in turn ([`SIT`]) while a shooter fires at
    /// it, then gets out: `client-probes/probe_vehicle_armor`.
    Sit,
    /// Drives straight ahead for 6 s ([`RAM`]) into whatever stands
    /// there, then stops and gets out: `client-probes/probe_vehicle_ram`.
    Ram,
}

/// `--probe-uo-weapon`: the scripts on foot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum WeaponScript {
    /// `binoculars_artillery_mp`: sight, fire, hold the sight.
    Arty,
    /// `smokegrenade_mp`: pull back, throw.
    Smoke,
    /// A bipod MG (`mg34_mp`): sight to deploy, hold the trigger, release,
    /// fire from the hip.
    Mg,
    /// Sight and hold the trigger for a minute, aimed by the gsc
    /// (`client-probes/probe_vehicle_armor` keeps the clip full).
    Hold,
    /// [`Self::Hold`] with every cmd at the newest snapshot's view, so a
    /// gsc `setPlayerAngles` every frame steers every round; `Hold` keeps
    /// its first view and lets the rebase fight the gsc
    /// (docs/research/coduo-vehicles.md 13).
    Aim,
    /// [`Self::Smoke`]'s pullback and throw at the newest snapshot's view,
    /// twice, 7 s apart: the grenade part of
    /// `client-probes/probe_scriptmodel`, any patch.
    Throw,
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
    up: i8,
    /// A client command sent once as the step starts.
    say: Option<&'static str>,
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
        up: 0,
        say: None,
    }
}

/// A step holding `up` (in a seat: `+moveup`, the hull turned to the view).
const fn up(label: &'static str, ms: u128, view: [f32; 2]) -> Step {
    Step {
        up: 127,
        ..step(label, ms, 0, 0, 0, view)
    }
}

/// A step that opens with a client command.
const fn say(label: &'static str, ms: u128, cmd: &'static str, view: [f32; 2]) -> Step {
    Step {
        say: Some(cmd),
        ..step(label, ms, 0, 0, 0, view)
    }
}

/// A step on foot: no moves.
const fn foot(label: &'static str, ms: u128, buttons: u8, view: [f32; 2]) -> Step {
    step(label, ms, 0, 0, buttons, view)
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

/// The gunner's script with one long burst.
const OVERHEAT: &[Step] = &[
    step("settle", 1500, 0, 0, 0, [0.0, 0.0]),
    step(WAIT_DRIVER, 20_000, 0, 0, 0, [0.0, 0.0]),
    step("use", 150, 0, 0, BUTTON_USE, [0.0, 0.0]),
    step(CREW, 5000, 0, 0, 0, [0.0, 0.0]),
    step("seated", 2000, 0, 0, 0, [0.0, 0.0]),
    step("sweep_left", 2000, 0, 0, 0, [-10.0, 60.0]),
    step("long_burst", 6000, 0, 0, BUTTON_ATTACK, [-10.0, 60.0]),
    step("cool", 8000, 0, 0, 0, [-10.0, 60.0]),
    step("burst2", 1000, 0, 0, BUTTON_ATTACK, [-10.0, 60.0]),
    step("hold", 60_000, 0, 0, 0, [0.0, 0.0]),
];

/// One player through every seat of the vehicle beside it
/// (docs/research/coduo-vehicles.md, the capture). Seat 1: the pitch past
/// the seat's 12000 clamp both ways, the sight, a drive, the hull turned to
/// the view with `up`, a shot; `nextvehslot` to the gunner: a sweep past
/// the gun's spans and a burst; `nextvehslot` again (a jeep's passenger
/// seat, or back to the driver): a sweep past the passenger's arcs and a
/// shot; then use, the exit. Cycles sit 2.5 s apart (retail spaces them
/// 1750 ms).
const SEATS: &[Step] = &[
    step("settle", 2000, 0, 0, 0, [0.0, 0.0]),
    step("use", 150, 0, 0, BUTTON_USE, [0.0, 0.0]),
    step("seated", 2000, 0, 0, 0, [0.0, 0.0]),
    step("pitch_down", 1500, 0, 0, 0, [80.0, 0.0]),
    step("pitch_up", 2500, 0, 0, 0, [-80.0, 0.0]),
    step("level", 1000, 0, 0, 0, [0.0, 0.0]),
    step("ads", 1500, 0, 0, BUTTON_ADS, [0.0, 0.0]),
    step("unads", 1000, 0, 0, 0, [0.0, 0.0]),
    step("ahead", 2000, 127, 0, 0, [0.0, 0.0]),
    step("stop", 1000, 0, 0, 0, [0.0, 0.0]),
    up("hull_to_view", 3000, [0.0, 60.0]),
    step("fire", 150, 0, 0, BUTTON_ATTACK, [0.0, 60.0]),
    step("after_fire", 1000, 0, 0, 0, [0.0, 60.0]),
    say("to_gunner", 2500, "nextvehslot", [0.0, 60.0]),
    step("gun_sweep", 2500, 0, 0, 0, [-70.0, 200.0]),
    step("gun_burst", 1000, 0, 0, BUTTON_ATTACK, [-70.0, 200.0]),
    step("gun_back", 1500, 0, 0, 0, [20.0, 60.0]),
    say("next_seat", 2500, "nextvehslot", [20.0, 60.0]),
    step("arc_yaw", 2000, 0, 0, 0, [0.0, 150.0]),
    step("arc_pitch", 1500, 0, 0, 0, [75.0, 150.0]),
    step("seat_fire", 150, 0, 0, BUTTON_ATTACK, [75.0, 150.0]),
    step("arc_back", 1500, 0, 0, 0, [0.0, 0.0]),
    step("exit", 150, 0, 0, BUTTON_USE, [0.0, 0.0]),
    step("on_foot", 3000, 0, 0, 0, [0.0, 0.0]),
    step("hold", 60_000, 0, 0, 0, [0.0, 0.0]),
];

/// One driver alone: the drive replay's inputs (docs/research/coduo-vehicles.md,
/// the server). Each motion settles before the next starts.
const DRIVE: &[Step] = &[
    step("settle", 2000, 0, 0, 0, [0.0, 0.0]),
    step("use", 150, 0, 0, BUTTON_USE, [0.0, 0.0]),
    step("seated", 2000, 0, 0, 0, [0.0, 0.0]),
    step("ahead", 3000, 127, 0, 0, [0.0, 0.0]),
    step("turn_right", 3000, 127, 127, 0, [-5.0, -60.0]),
    step("coast", 2000, 0, 0, 0, [-5.0, -60.0]),
    step("reverse_left", 2500, -127, -127, 0, [5.0, 20.0]),
    step("stop", 2000, 0, 0, 0, [5.0, 20.0]),
    up("hull_to_view", 3000, [0.0, 90.0]),
    step("fire", 150, 0, 0, BUTTON_ATTACK, [0.0, 90.0]),
    step("after_fire", 1000, 0, 0, 0, [0.0, 90.0]),
    step("coax", 1500, 0, 0, BUTTON_MELEE, [10.0, 60.0]),
    step("rest", 1500, 0, 0, 0, [0.0, 0.0]),
    step("exit", 150, 0, 0, BUTTON_USE, [0.0, 0.0]),
    step("on_foot", 3000, 0, 0, 0, [0.0, 0.0]),
    step("hold", 60_000, 0, 0, 0, [0.0, 0.0]),
];

/// Still in each seat for 12 s: on foot beside the hull, use (seat 1),
/// `nextvehslot` (seat 2), `nextvehslot` again (a jeep's passenger seat, or
/// back to a tank's driver), then out and on foot again.
const SIT: &[Step] = &[
    step("settle", 10_000, 0, 0, 0, [0.0, 0.0]),
    step("use", 150, 0, 0, BUTTON_USE, [0.0, 0.0]),
    step("seat_a", 12_000, 0, 0, 0, [0.0, 0.0]),
    say("seat_b", 12_000, "nextvehslot", [0.0, 0.0]),
    say("seat_c", 12_000, "nextvehslot", [0.0, 0.0]),
    step("exit", 150, 0, 0, BUTTON_USE, [0.0, 0.0]),
    step("on_foot", 12_000, 0, 0, 0, [0.0, 0.0]),
    step("hold", 60_000, 0, 0, 0, [0.0, 0.0]),
];

/// Into the vehicle, straight ahead for 6 s, a stop, out.
const RAM: &[Step] = &[
    step("settle", 2000, 0, 0, 0, [0.0, 0.0]),
    step("use", 150, 0, 0, BUTTON_USE, [0.0, 0.0]),
    step("seated", 2000, 0, 0, 0, [0.0, 0.0]),
    step("ahead", 6000, 127, 0, 0, [0.0, 0.0]),
    step("stop", 3000, 0, 0, 0, [0.0, 0.0]),
    step("exit", 150, 0, 0, BUTTON_USE, [0.0, 0.0]),
    step("on_foot", 3000, 0, 0, 0, [0.0, 0.0]),
    step("hold", 60_000, 0, 0, 0, [0.0, 0.0]),
];

/// From the spawn (the select lands a second in, the raise half a second
/// later): sight down 12 degrees for the binoculars' 0.8 s transition, fire,
/// hold the sight while the barrage comes in.
const ARTY: &[Step] = &[
    foot("settle", 3000, 0, [12.0, 0.0]),
    foot("sight", 1500, BUTTON_ADS, [12.0, 0.0]),
    foot("fire", 200, BUTTON_ADS | BUTTON_ATTACK, [12.0, 0.0]),
    foot("sighted", 3000, BUTTON_ADS, [12.0, 0.0]),
    foot("hold", 60_000, 0, [12.0, 0.0]),
];

/// Pull back for a second, throw 10 degrees up, watch the cloud.
const SMOKE: &[Step] = &[
    foot("settle", 3000, 0, [-10.0, 0.0]),
    foot("pullback", 1000, BUTTON_ATTACK, [-10.0, 0.0]),
    foot("thrown", 2000, 0, [-10.0, 0.0]),
    foot("hold", 60_000, 0, [-10.0, 0.0]),
];

/// Two pullbacks of a second, each let go, 7 s apart.
const THROW: &[Step] = &[
    foot("settle", 3000, 0, [0.0, 0.0]),
    foot("pullback", 1000, BUTTON_ATTACK, [0.0, 0.0]),
    foot("thrown", 6000, 0, [0.0, 0.0]),
    foot("pullback2", 1000, BUTTON_ATTACK, [0.0, 0.0]),
    foot("hold", 60_000, 0, [0.0, 0.0]),
];

/// Sighted, the trigger held for the whole run.
const HOLD: &[Step] = &[
    foot("settle", 1000, 0, [0.0, 0.0]),
    foot("fire", 120_000, BUTTON_ADS | BUTTON_ATTACK, [0.0, 0.0]),
];

/// Sight (the deploy: an lmg sighted in the open goes prone,
/// docs/research/coduo-diff.md 11), three seconds on the trigger, let go
/// of it, of the sight (the breakdown), and fire once from the hip.
const MG: &[Step] = &[
    foot("settle", 3000, 0, [0.0, 0.0]),
    foot("deploy", 3000, BUTTON_ADS, [0.0, 0.0]),
    foot("burst", 3000, BUTTON_ADS | BUTTON_ATTACK, [0.0, 0.0]),
    foot("deployed", 2000, BUTTON_ADS, [0.0, 0.0]),
    foot("breakdown", 3000, 0, [0.0, 0.0]),
    foot("hip", 1000, BUTTON_ATTACK, [0.0, 0.0]),
    foot("hold", 60_000, 0, [0.0, 0.0]),
];

pub struct VehicleProbe {
    script: &'static [Step],
    /// On foot: the weapon of the server's last `a`, sent from then on.
    on_foot: bool,
    selected: Option<u8>,
    spawned: Option<Instant>,
    /// The view at the use tap, pitch and yaw in degrees.
    base: Option<[f32; 2]>,
    label: &'static str,
    last_snap: Option<u32>,
    /// Script time skipped by [`CREW`] steps that ended early.
    skipped: u128,
    /// Every cmd takes the newest snapshot's view ([`WeaponScript::Aim`]).
    follow: bool,
    /// The client command of the step just entered, until taken.
    say: Option<&'static str>,
}

impl VehicleProbe {
    pub fn new(role: VehicleRole) -> VehicleProbe {
        let script = match role {
            VehicleRole::Driver => DRIVER,
            VehicleRole::Gunner => GUNNER,
            VehicleRole::Overheat => OVERHEAT,
            VehicleRole::Seats => SEATS,
            VehicleRole::Drive => DRIVE,
            VehicleRole::Sit => SIT,
            VehicleRole::Ram => RAM,
        };
        VehicleProbe::with(script, false)
    }

    pub fn on_foot(script: WeaponScript) -> VehicleProbe {
        let follow = matches!(script, WeaponScript::Aim | WeaponScript::Throw);
        let script = match script {
            WeaponScript::Arty => ARTY,
            WeaponScript::Smoke => SMOKE,
            WeaponScript::Mg => MG,
            WeaponScript::Hold | WeaponScript::Aim => HOLD,
            WeaponScript::Throw => THROW,
        };
        VehicleProbe {
            follow,
            ..VehicleProbe::with(script, true)
        }
    }

    fn with(script: &'static [Step], on_foot: bool) -> VehicleProbe {
        VehicleProbe {
            script,
            on_foot,
            selected: None,
            spawned: None,
            base: None,
            label: "",
            last_snap: None,
            skipped: 0,
            say: None,
            follow: false,
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
        let script = self.script;
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
            self.say = cur.say;
        }
        // The view the gsc set, held until the tap, then the script's
        // offsets from it.
        if self.follow {
            self.base = None;
        }
        if (cur.buttons & BUTTON_USE != 0 || self.on_foot) && self.base.is_none() {
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
        cmd.up = cur.up;
        cmd.buttons = cur.buttons;
        cmd
    }

    /// The client command the script's current step opened with, once.
    pub fn take_say(&mut self) -> Option<&'static str> {
        self.say.take()
    }

    /// A server command: on foot, an `a <weapon>` select is taken.
    pub fn on_command(&mut self, cmd: &str) {
        if let Some(w) = cmd.strip_prefix("a ").and_then(|w| w.trim().parse().ok())
            && self.on_foot
        {
            self.selected = Some(w);
        }
    }

    /// The weapon byte to send instead of `ps.weapon`'s echo.
    pub fn weapon(&self) -> Option<u8> {
        self.selected
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
             locked {} pm_flags {:#x} eflags {:#x} weapon {} pm_type {} da {} {} frac {:.3} \
             hint {} {} hp {} wstate {}",
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
            ps("pm_type"),
            ps("delta_angles[0]") as u16 as i16,
            ps("delta_angles[1]") as u16 as i16,
            s.ps.field_f32(p, "fWeaponPosFrac"),
            ps("serverCursorHint"),
            ps("serverCursorHintString"),
            s.ps.health(),
            ps("weaponstate"),
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
