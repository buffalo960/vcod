//! `Scr_Vehicle_Think` (game.mp.uo 0x8c7cc) around the motion step: the
//! dead riders' exits, the guns' timers, heat and fire notifies, and what
//! the riders read back (docs/research/coduo-vehicles.md 12 and 13).

use super::info::KIND_FOUR_WHEEL;
use super::{Gun, MAX_SEATS, physics};
use crate::game::host::GameHost;
use vcod_gsc::{Cx, EntId};

/// A gun's overheat check ahead of its cooling: `Some(true)` as it
/// overheats, `Some(false)` as it recovers.
fn heat_check(heat: f32, overheating: &mut bool) -> Option<bool> {
    if heat >= OVERHEAT && !*overheating {
        *overheating = true;
        Some(true)
    } else if *overheating && heat <= RECOVER {
        *overheating = false;
        Some(false)
    } else {
        None
    }
}

/// One think's step, a retail immediate whatever the frame.
pub const STEP_MS: i32 = 50;
/// Overheat at 1.0, cool back under 0.5 (0xa6d14, 0xa7590).
const OVERHEAT: f32 = 1.0;
const RECOVER: f32 = 0.5;
/// The horn's alt heat cools this much a think on a four-wheel with no alt
/// gun.
const HORN_COOL: f32 = 0.01;

/// What a seated player hands its vehicle's think: its last cmd and view.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RiderInput {
    pub forward: i8,
    pub right: i8,
    pub up: i8,
    pub buttons: u8,
    pub view: [f32; 3],
    /// `ps.fWeaponPosFrac != 0`.
    pub sighted: bool,
    pub alive: bool,
}

/// `BUTTON_ATTACK` and the alt gun's 0x20 (`BUTTON_MELEE`).
const ATTACK: u8 = vcod_common::net::msg::BUTTON_ATTACK;
const ALT: u8 = vcod_common::net::msg::BUTTON_MELEE;

/// `G_VehicleClientThink` and the undriven vehicles' own thinks for one
/// server frame: a driven vehicle steps every frame, any other on its
/// 50 ms think. `inputs` is each client slot's seated input.
pub fn run(host: &mut GameHost, cx: &mut Cx, inputs: &[Option<RiderInput>]) {
    for id in std::mem::take(&mut host.vehicles.deaths) {
        die(host, cx, id);
    }
    let now = host.level_time_ms;
    for id in host.vehicles.in_slot_order() {
        let Some(v) = host.vehicles.live.get(&id) else {
            continue;
        };
        if !v.driven && now < v.next_think {
            continue;
        }
        think(host, cx, id, inputs);
        if let Some(v) = host.vehicles.live.get_mut(&id) {
            v.next_think = now + STEP_MS;
        }
    }
}

fn think(host: &mut GameHost, cx: &mut Cx, id: EntId, inputs: &[Option<RiderInput>]) {
    let now = host.level_time_ms;
    let input = |slot: Option<usize>| slot.and_then(|s| inputs.get(s).copied().flatten());
    // The dead riders get out where they sit.
    let riders: Vec<usize> = host.vehicles.live[&id]
        .seats
        .iter()
        .flatten()
        .copied()
        .collect();
    for r in riders {
        if host
            .client_vitals
            .get(r)
            .is_none_or(|v| v.health < 1 || v.dead)
        {
            super::seat::unlink_player(host, cx, r, false, [0.0; 3], ([0.0; 3], [0.0; 3]));
        }
    }
    let info = {
        let v = &host.vehicles.live[&id];
        host.vehicles.info_of(v).clone()
    };
    let health = crate::game::builtins::vehicle::health_of(host, cx, id);
    let capturing = script_int(host, cx, id, "capturing") != 0;
    let world = host.world.clone();
    let tex_scroll = crate::game::builtins::cvar::atof(host.cvars.get("g_vehicleTexScrollScale"));
    let horns = host.cvars.get("g_vehicleHorns") != "0";
    let linked = host.ents.get(id).is_some_and(|e| !e.unlinked);
    let bounds_maxs = host
        .vehicles
        .live
        .get(&id)
        .and_then(|v| v.collmap)
        .and_then(|n| host.model_bounds.get(n as usize))
        .map_or(glam::Vec3::from(info.maxs), |b| glam::Vec3::from(b.1));
    // The bodies the probes can strike: every live player but the riders.
    let riders_now = host.vehicles.live[&id].seats;
    let bodies: Vec<(u32, glam::Vec3, glam::Vec3, glam::Vec3)> = host
        .client_bodies
        .iter()
        .flatten()
        .filter(|b| !riders_now.contains(&Some(b.slot)))
        .filter(|b| host.vehicles.seat_of(b.slot).is_none())
        .map(|b| (b.slot as u32, b.origin, b.mins, b.maxs))
        .collect();
    let v = host.vehicles.live.get_mut(&id).unwrap();
    v.posed_wire = v.wire;
    let driver = input(v.seats[1]);
    let gunner = input(v.seats[2]);
    let step_in = physics::StepInput {
        now,
        driver: driver.map(|d| physics::DriveInput {
            forward: d.forward,
            right: d.right,
            up: d.up,
            buttons: i32::from(d.buttons),
            view: d.view,
            ads: d.sighted,
            popped_out: false,
        }),
        gunner_view: gunner.map(|g| g.view),
        driven: v.driven,
        health,
        capturing,
        closespeed: 0.0,
        harc: 0.0,
        linked,
        bounds_maxs,
        max_clients: crate::server::MAX_CLIENTS as u32,
        neighbour_moving: false,
        tex_scroll_scale: tex_scroll,
        horns,
        alt_overheated: v.guns.overheating[0],
    };
    let mut out = physics::StepOutput::default();
    if let Some(w) = world.as_deref() {
        let vw = BodiesWorld {
            inner: physics::CollisionVehicleWorld {
                world: &w.collision,
                skip: v
                    .collmap
                    .map(|n| vcod_common::collision::Pusher::Model(n as usize)),
            },
            bodies: &bodies,
        };
        if !v.initialized {
            v.phys.init(&info, &vw, v.driven);
        }
        for j in std::mem::take(&mut v.jolts) {
            let dir = glam::Vec3::from(v.pose.origin) - glam::Vec3::from(j.from);
            v.phys.jolt(
                &info,
                dir.normalize_or_zero(),
                j.force,
                j.speed_frac,
                j.decel,
                now,
            );
        }
        out = v.phys.motion(&info, &vw, &step_in);
    }
    v.initialized = true;
    if let Some(flag) = out.driver_flag_200000 {
        v.hull_view = flag;
    }
    // The alt gun's heat (think step 9): the check, then the cooling.
    let mut events = Vec::new();
    if heat_check(v.guns.heat[0], &mut v.guns.overheating[0]) == Some(true)
        && info.kind != KIND_FOUR_WHEEL
    {
        events.push(EV_OVERHEATING);
    }
    let alt_cool = cooldown(host_weapons(&host.weapons, v.weapons[1]));
    let v = host.vehicles.live.get_mut(&id).unwrap();
    match alt_cool {
        Some(c) => {
            v.guns.heat[0] = if v.guns.heat[0] > 0.0 {
                v.guns.heat[0] - c
            } else {
                0.0
            }
        }
        None if info.kind == KIND_FOUR_WHEEL => {
            if out.horn {
                v.guns.heat[0] += HORN_HEAT;
            }
            v.guns.heat[0] = if v.guns.heat[0] > 0.0 {
                v.guns.heat[0] - HORN_COOL
            } else {
                0.0
            }
        }
        None => {}
    }
    let mut notify: Vec<&'static str> = Vec::new();
    // The driver's reader (0x86c57).
    if let Some(d) = driver
        && v.weapons[0] != 0
        && !capturing
    {
        v.wire.weapon = v.weapons[0] as i32;
        if v.guns.timers[0] < 1 {
            if d.buttons & ATTACK != 0 {
                notify.push("turret_fire");
            }
        } else {
            v.guns.timers[0] -= STEP_MS;
        }
        v.guns.timers[1] -= STEP_MS;
        if v.guns.timers[1] < 1 && d.buttons & ALT != 0 {
            notify.push("turret_alt_fire");
        }
    }
    // The gunner's (0x871a6): heat, the overheated bit, then the trigger.
    match heat_check(v.guns.heat[1], &mut v.guns.overheating[1]) {
        Some(true) => {
            events.push(EV_OVERHEATING);
            // Retail writes the seat mask here (0x87268).
            v.wire.anim_movetype = 1;
        }
        Some(false) => v.wire.anim_movetype = 0,
        None => {}
    }
    let gunner_overheated = v.seats[2].is_some() && v.guns.overheating[1];
    if gunner_overheated {
        notify.push("overheating");
    }
    if let Some(g) = gunner
        && g.alive
    {
        v.guns.timers[2] -= STEP_MS;
        if v.guns.timers[2] < 1 && g.buttons & ATTACK != 0 {
            notify.push("turret_gunner_fire");
        }
    }
    let gunner_slot = v.seats[2];
    let gun_cool = cooldown(host_weapons(
        &host.weapons,
        host.vehicles.live[&id].weapons[2],
    ));
    let v = host.vehicles.live.get_mut(&id).unwrap();
    if let Some(c) = gun_cool {
        v.guns.heat[1] = if v.guns.heat[1] > 0.0 {
            v.guns.heat[1] - c
        } else {
            0.0
        };
    }
    // The aims, the sway, the wheel and track clocks.
    v.phys.finish(&info, &step_in, &mut out);
    let before = v.pose;
    sync(v);
    let stop_alias = sounds(v, &info, health, capturing);
    let moved = out.moved || v.pose != before;
    let pose = v.pose;
    if let Some(s) = gunner_slot {
        host.vehicles.overheated_gunners.retain(|&g| g != s);
        if gunner_overheated {
            host.vehicles.overheated_gunners.push(s);
        }
    }
    if moved {
        super::set_field(host, cx, id, "origin", vcod_gsc::Value::Vector(pose.origin));
        super::set_field(host, cx, id, "angles", vcod_gsc::Value::Vector(pose.angles));
        super::pose_collmap(host, id);
        super::link(host, cx, id);
    }
    let speed = host.vehicles.live[&id].phys.speed;
    super::set_field(host, cx, id, "speed", vcod_gsc::Value::Float(speed));
    if let Some(e) = host.ents.get_mut(id) {
        for ev in events {
            e.events.add(ev, 0);
        }
        for alias in stop_alias {
            e.events.add(crate::game::script::EV_SOUND_ALIAS, alias);
        }
    }
    for n in notify {
        host.engine_notifies.push((id, n, Vec::new()));
    }
    for c in &out.collisions {
        if let Some((normal, at)) = c.notify {
            host.engine_notifies.push((
                id,
                "vehicle_collision",
                vec![
                    vcod_gsc::Value::Vector(at.to_array()),
                    vcod_gsc::Value::Vector(normal.to_array()),
                ],
            ));
        }
    }
    for t in out.touched {
        if let Some(e) = host.ents.handle(t) {
            host.engine_notifies
                .push((e, "trigger", vec![vcod_gsc::Value::Entity(id)]));
        }
    }
    host.vehicles
        .strikes
        .extend(out.struck_clients.iter().map(|s| (id, *s)));
    // The driver's reader's aim point (`rec+0x28c`), which the next
    // `fireTurret` bends the round toward.
    let aim = driver
        .filter(|_| !capturing && host.vehicles.live[&id].weapons[0] != 0)
        .and_then(|d| super::fire::aim_point(host, cx, id, d.view, d.sighted));
    if let Some(v) = host.vehicles.live.get_mut(&id) {
        v.aim_point = aim;
    }
}

/// `Scr_Vehicle_Die` (0x8dccb): every rider out. One the death cannot
/// hurt (a tank's driver) looks for a dismount spot; the rest get out where
/// they sit. Retail decides by a 5-point `MOD_EXPLOSIVE` probe hit through
/// the damage callback; vcod asks the invulnerability rule directly. The
/// vehicle itself stays, for the scripts to burn and delete.
fn die(host: &mut GameHost, cx: &mut Cx, id: EntId) {
    let Some(v) = host.vehicles.live.get(&id) else {
        return;
    };
    let kind = host.vehicles.info_of(v).kind;
    let riders: Vec<(usize, usize)> = (1..=MAX_SEATS)
        .filter_map(|s| v.seats[s].map(|r| (s, r)))
        .collect();
    for (seat, r) in riders {
        let shielded = kind == super::info::KIND_TANK && seat == 1;
        let from = host
            .ents
            .handle(r as u32)
            .map_or([0.0; 3], |e| crate::game::item::origin_of(host, cx, e));
        super::seat::unlink_player(host, cx, r, shielded, from, super::seat::PLAYER_BOX);
        if host.vehicles.seat_of(r).is_some() {
            super::seat::unlink_player(host, cx, r, false, from, super::seat::PLAYER_BOX);
        }
    }
}

/// 0x87480, the engine and gun sounds: the idle pair (B1) and the engine
/// pair (B2) crossfaded by the step's blend while a driver has the engine
/// running, the turret's spin alias in `loopSound` while it turns (its
/// stop alias once as it stops), each MG's loop (B3, B4) held 200 ms past
/// a round, its cooldown alias once as the hold runs out. Returns the
/// aliases to play on the vehicle.
fn sounds(
    v: &mut super::Vehicle,
    info: &super::info::VehicleInfo,
    health: i32,
    capturing: bool,
) -> Vec<i32> {
    let dead = health < 1;
    let mut play = Vec::new();
    v.wire.loop_sound = 0;
    let driver = v.seats[1].is_some();
    let si = info.sound_index;
    let blend = &mut v.sound_blends;
    if v.blends[0].is_some() {
        blend[0] = (!(dead || !v.engine_on || !driver || capturing)).then_some((
            si[0],
            si[1],
            v.phys.engine_blend[0],
        ));
    }
    if v.blends[1].is_some() {
        let quiet = dead
            || !v.engine_on
            || (v.blends[0].is_some() && v.phys.engine_blend[1] == 0.0)
            || (!driver && v.phys.s.local_velocity.x.abs() < 0.1);
        blend[1] = (!quiet).then_some((si[2], si[3], v.phys.engine_blend[1]));
    }
    if !dead {
        if v.phys.turret_spin == 2 && si[4] != 0 {
            v.wire.loop_sound = si[4];
        } else if v.phys.turret_spin == 1 && si[5] != 0 {
            play.push(si[5]);
        }
    }
    for (k, named) in [
        !info.turret_alt_weapon.is_empty(),
        !info.gunner_weapon.is_empty(),
    ]
    .into_iter()
    .enumerate()
    {
        if v.blends[2 + k].is_none() || !named {
            continue;
        }
        blend[2 + k] = None;
        if !dead && v.guns.loop_hold[k] > 0 {
            let alias = v.gun_sounds[k * 2];
            blend[2 + k] = (alias != 0).then_some((alias, alias, 0.0));
            v.guns.loop_hold[k] -= STEP_MS;
            if v.guns.loop_hold[k] < 1 && v.gun_sounds[k * 2 + 1] != 0 {
                play.push(v.gun_sounds[k * 2 + 1]);
            }
        }
    }
    play
}

/// The record's copy of what the step left: the pose, the wire fields and
/// the speeds the seats and the scripts read.
fn sync(v: &mut super::Vehicle) {
    let p = &v.phys;
    v.pose.origin = p.origin().to_array();
    v.pose.angles = p.angles().to_array();
    v.wire.angles2 = p.angles2;
    v.wire.origin2 = [
        p.sway_packed[0] as f32,
        p.gunner_yaw,
        p.sway_packed[1] as f32,
    ];
    v.wire.leanf = p.leanf;
    v.wire.time = p.time;
    v.wire.time2 = p.time2;
    v.speed = p.speed;
    v.forward_speed = p.s.local_velocity.x;
    v.velocity = p.s.velocity.to_array();
}

/// A script field read as an int, 0 when unset.
fn script_int(host: &mut GameHost, cx: &mut Cx, id: EntId, name: &str) -> i32 {
    use vcod_gsc::Host;
    let atom = cx.intern_folded(name);
    match host.get_field(cx, id, atom) {
        vcod_gsc::Value::Int(i) => i,
        vcod_gsc::Value::Float(f) => f as i32,
        _ => 0,
    }
}

/// The horn's heat, a think it sounds.
const HORN_HEAT: f32 = 0.04;

use vcod_common::net::event_ids::EV_UO_OVERHEATING as EV_OVERHEATING;

fn host_weapons(
    table: &crate::weapons::WeaponTable,
    index: usize,
) -> Option<&vcod_common::weapon::WeaponDef> {
    (index != 0).then(|| table.get(index)).flatten()
}

/// `cooldownRate` of a gun that has one.
fn cooldown(def: Option<&vcod_common::weapon::WeaponDef>) -> Option<f32> {
    def.map(|d| d.cooldown_rate)
}

/// Each seat's rider, for the gunner's `pm_flags` and the like.
pub fn riders(host: &GameHost, id: EntId) -> [Option<usize>; MAX_SEATS + 1] {
    host.vehicles
        .live
        .get(&id)
        .map_or([None; MAX_SEATS + 1], |v| v.seats)
}

/// Which gun a `turret_*` notify names.
pub fn gun_of(event: &str) -> Option<Gun> {
    match event {
        "turret_fire" => Some(Gun::Main),
        "turret_alt_fire" => Some(Gun::Alt),
        "turret_gunner_fire" => Some(Gun::Gunner),
        _ => None,
    }
}

/// The collision world with the players' bodies in it: a trace whose mask
/// holds `CONTENTS_BODY` (0x2000000) also meets each body's box.
struct BodiesWorld<'a> {
    inner: physics::CollisionVehicleWorld<'a>,
    /// Entity number, origin, mins, maxs.
    bodies: &'a [(u32, glam::Vec3, glam::Vec3, glam::Vec3)],
}

const CONTENTS_BODY: u32 = 0x0200_0000;

impl physics::VehicleWorld for BodiesWorld<'_> {
    fn trace(
        &self,
        start: glam::Vec3,
        end: glam::Vec3,
        mins: glam::Vec3,
        maxs: glam::Vec3,
        mask: u32,
    ) -> physics::VehTrace {
        let mut t = self.inner.trace(start, end, mins, maxs, mask);
        if mask & CONTENTS_BODY == 0 {
            return t;
        }
        for &(n, o, bmins, bmaxs) in self.bodies {
            // The sweep against the body's box grown by the swept box.
            let lo = o + bmins - maxs;
            let hi = o + bmaxs - mins;
            let inside = (0..3).all(|i| start[i] > lo[i] && start[i] < hi[i]);
            if inside {
                if t.fraction > 0.0 || !t.startsolid {
                    t = physics::VehTrace {
                        fraction: 0.0,
                        endpos: start,
                        normal: glam::Vec3::ZERO,
                        surface_flags: 0,
                        startsolid: true,
                        allsolid: start == end,
                        entity: n,
                    };
                }
                continue;
            }
            let Some(f) = crate::game::combat::ray_box(start, end, lo, hi) else {
                continue;
            };
            if f < t.fraction {
                let d = end - start;
                // The face the sweep entered by.
                let p = start + d * f;
                let mut normal = glam::Vec3::ZERO;
                for i in 0..3 {
                    if (p[i] - lo[i]).abs() < 1e-3 {
                        normal[i] = -1.0;
                    } else if (p[i] - hi[i]).abs() < 1e-3 {
                        normal[i] = 1.0;
                    }
                }
                t = physics::VehTrace {
                    fraction: f,
                    endpos: p,
                    normal: normal.normalize_or_zero(),
                    surface_flags: 0,
                    startsolid: false,
                    allsolid: false,
                    entity: n,
                };
            }
        }
        t
    }

    fn entity(&self, num: u32) -> physics::HitEntity {
        match self.bodies.iter().find(|b| b.0 == num) {
            Some(&(_, o, ..)) => physics::HitEntity {
                origin: o,
                client: true,
                ..physics::HitEntity::default()
            },
            None => self.inner.entity(num),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The retail overheat capture (`snapshots-uo-overheat.bin`): a tank
    /// gunner holding `mg42_turret_mp` (`fireHeat` 0.04, `cooldownRate`
    /// 0.01, `fireTime` 0.1) fires 49 rounds 100 ms apart, overheats on the
    /// 49th and recovers 51 thinks later. Each think checks, cools, runs
    /// the trigger; the script's round lands before the next think.
    #[test]
    fn the_gunner_overheats_on_the_49th_round_and_recovers_2550_ms_on() {
        let (heat_per, cool, fire_ms) = (0.04f32, 0.01f32, 100);
        let (mut heat, mut flag, mut timer) = (0.0f32, false, 0);
        let (mut rounds, mut fire_due) = (0, false);
        let (mut over_at, mut back_at) = (None, None);
        for think in 0..400 {
            if fire_due && !flag {
                heat += heat_per;
                timer = fire_ms;
                rounds += 1;
            }
            fire_due = false;
            match heat_check(heat, &mut flag) {
                Some(true) => over_at = Some((think, rounds)),
                Some(false) => {
                    back_at.get_or_insert(think);
                }
                None => {}
            }
            heat = if heat > 0.0 { heat - cool } else { 0.0 };
            timer -= STEP_MS;
            if timer < 1 && over_at.is_none() {
                fire_due = true;
            }
        }
        let (over, n) = over_at.unwrap();
        assert_eq!(n, 49);
        assert_eq!((back_at.unwrap() - over) * STEP_MS, 2550);
    }
}
