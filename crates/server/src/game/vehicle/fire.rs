//! A vehicle's guns firing (docs/research/coduo-vehicles.md 13):
//! `CMD_VEH_FireTurret` (game.mp.uo 0x7ee02), `CMD_VEH_FireAltTurret`
//! (0x7f57e) and `G_VEH_FireGunner` (0x7fc7a). The host aims the round off
//! the posed vehicle; the server traces it against the bodies and hands the
//! result back to [`landed`].

use super::{Gun, seat};
use crate::game::host::GameHost;
use glam::{Quat, Vec3};
use vcod_common::net::event_ids::{EV_FIRE_WEAPON, EV_FIRE_WEAPONB, EV_UO_FIRE_WEAPONC};
use vcod_gsc::{Cx, EntId};

/// The driver's aim point reaches this far along the view (0x86c57).
const AIM_REACH: f32 = 10240.0;
/// A turret round bends toward the driver's aim point by at most this much
/// pitch and yaw off the barrel (0x7ee02).
const NUDGE_PITCH: f32 = 5.0;
const NUDGE_YAW: f32 = 2.0;
/// The MG loops' hold after each round (`rec+0x2e4`, `0x2f0`).
const LOOP_HOLD_MS: i32 = 200;
/// Event parm bit 7: the server drew neither impact nor tracer, the client
/// traces the round itself.
pub const CLIENT_TRACED: i32 = 0x80;

/// One round to trace.
#[derive(Clone, Copy, Debug)]
pub struct Shot {
    pub vehicle: EntId,
    pub gun: Gun,
    /// The seated player the round is charged to.
    pub attacker: usize,
    pub weapon: usize,
    pub muzzle: Vec3,
    /// The direction, spread already in.
    pub dir: Vec3,
    pub damage: i32,
    pub rifle_bullet: bool,
    /// A projectile weapon's shell rather than a bullet.
    pub projectile: bool,
}

fn angles_of(v: Vec3) -> [f32; 3] {
    let yaw = v.y.atan2(v.x).to_degrees();
    let pitch = (-v.z).atan2(v.x.hypot(v.y)).to_degrees();
    [pitch, yaw, 0.0]
}

fn sub180(a: f32) -> f32 {
    let mut a = a % 360.0;
    if a > 180.0 {
        a -= 360.0;
    }
    if a < -180.0 {
        a += 360.0;
    }
    a
}

/// The fire call's aim and round, `None` where retail refuses: no rider in
/// the seat, an overheated MG, a gun the vehicle lacks, a missing tag.
/// `views` is each client slot's view angles and sight.
pub fn prepare(
    host: &mut GameHost,
    cx: &mut Cx,
    id: EntId,
    gun: Gun,
    views: &[Option<([f32; 3], bool)>],
) -> Option<Shot> {
    let v = host.vehicles.live.get(&id)?;
    let (seat_no, weapon) = match gun {
        Gun::Main => (1, v.weapons[0]),
        Gun::Alt => (1, v.weapons[1]),
        Gun::Gunner => (2, v.weapons[2]),
    };
    let attacker = v.seats[seat_no]?;
    if weapon == 0
        || (gun == Gun::Alt && v.guns.overheating[0])
        || (gun == Gun::Gunner && v.guns.overheating[1])
    {
        return None;
    }
    let def = host.weapons.get(weapon)?.clone();
    let posed = seat::posed(host, cx, id)?;
    let flash = match gun {
        Gun::Main => "tag_flash",
        Gun::Alt => "tag_altfire",
        Gun::Gunner => "tag_secondary_flash",
    };
    let (muzzle, rot): (Vec3, Quat) = posed.tag(flash)?;
    let mut dir = rot * Vec3::X;
    if gun != Gun::Gunner
        && let Some((view, sighted)) = views.get(attacker).copied().flatten()
    {
        // The driver's aim point: from the chase camera (the gunsight when
        // sighted) along the view, stopped at the first thing in the way.
        let eye_tag = if sighted {
            "tag_aimdownbarrel"
        } else {
            "tag_chasecam"
        };
        if let Some((eye, _)) = posed.tag(eye_tag) {
            let fwd = Vec3::from(crate::game::spawn::angle_forward(view));
            let mut aim = eye + fwd * AIM_REACH;
            if let Some(w) = host.world.as_ref() {
                let tr = w.collision.shot_trace(eye, aim);
                aim = tr.endpos;
            }
            let want = angles_of((aim - muzzle).normalize_or_zero());
            let have = angles_of(dir);
            let dp = sub180(have[0] - want[0]).clamp(-NUDGE_PITCH, NUDGE_PITCH);
            let dy = sub180(have[1] - want[1]).clamp(-NUDGE_YAW, NUDGE_YAW);
            dir = Vec3::from(crate::game::spawn::angle_forward([
                have[0] - dp,
                have[1] - dy,
                0.0,
            ]));
        }
    }
    let projectile = def.weapon_type == "projectile";
    if !projectile {
        // `minSpread + rand * maxSpread` degrees (sic).
        let (lo, hi) = (def.min_spread.unwrap_or(0.0), def.max_spread.unwrap_or(0.0));
        let spread = lo + host.rand_unit() * hi;
        let r = spread.to_radians().tan();
        let angle = host.rand_unit() * std::f32::consts::TAU;
        let radius = host.rand_unit();
        let right = rot * Vec3::NEG_Y;
        let up = rot * Vec3::Z;
        dir = (dir + right * (radius * angle.cos() * r) + up * (radius * angle.sin() * r))
            .normalize_or_zero();
    }
    Some(Shot {
        vehicle: id,
        gun,
        attacker,
        weapon,
        muzzle,
        dir,
        damage: def.damage,
        rifle_bullet: def.sounds.rifle_bullet,
        projectile,
    })
}

/// What the fire call leaves on the vehicle once its round is out: the gun
/// in `s.weapon`, the fire event on its own ring (163 main, 164 alt, 165
/// gunner; a bullet's parm the weapon with bit 7 unless it struck a
/// player, a shell's 0), the gun's timer, an MG's heat and loop hold.
pub fn landed(host: &mut GameHost, shot: &Shot, struck_player: bool) {
    let Some(v) = host.vehicles.live.get_mut(&shot.vehicle) else {
        return;
    };
    let def = host.weapons.get(shot.weapon);
    let fire_ms = def.map_or(0, |d| vcod_common::weapon::stored_ms(d.fire_time));
    let heat = def.map_or(0.0, |d| d.fire_heat);
    v.wire.weapon = shot.weapon as i32;
    let parm = if shot.projectile {
        0
    } else {
        shot.weapon as i32 | if struck_player { 0 } else { CLIENT_TRACED }
    };
    let event = match shot.gun {
        Gun::Main => {
            v.guns.timers[0] = fire_ms;
            EV_FIRE_WEAPON
        }
        Gun::Alt => {
            v.guns.timers[1] = fire_ms;
            v.guns.heat[0] += heat;
            v.guns.loop_hold[0] = LOOP_HOLD_MS;
            EV_FIRE_WEAPONB
        }
        Gun::Gunner => {
            v.guns.timers[2] = fire_ms;
            v.guns.heat[1] += heat;
            v.guns.loop_hold[1] = LOOP_HOLD_MS;
            EV_UO_FIRE_WEAPONC
        }
    };
    if let Some(e) = host.ents.get_mut(shot.vehicle) {
        e.events.add(event, parm);
    }
}
