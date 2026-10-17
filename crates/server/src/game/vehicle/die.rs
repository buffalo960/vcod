//! `Scr_Vehicle_Die` (game.mp.uo 0x8dccb), the die callback `G_Damage`
//! runs once a vehicle's health is gone: each rider, seat 1 to 6, takes a
//! 5-point `MOD_EXPLOSIVE` hit from the vehicle's attacker through his
//! damage callback with his health padded by 10, and one the hit did not
//! hurt looks for a dismount spot while the rest are put out where they sit
//! (docs/research/coduo-vehicles.md 11.2). A rider's callback runs to its
//! first `wait` before the next rider's hit, so the death is a walk:
//! [`step`] finishes the rider whose callback just returned and hands back
//! the next callback to run.

use super::{EntId, MAX_SEATS};
use crate::game::host::GameHost;
use vcod_gsc::{Cx, Value};

/// The `spawn_then` token of a rider's probe callback
/// (`GameHost::spawn_returned`).
pub const DIE_TOKEN: u32 = u32::MAX - 1;

/// The probe hit's damage and the health it is padded by.
const PROBE_DAMAGE: i32 = 5;
const PROBE_PAD: i32 = 10;

/// `G_SetOrigin`'s lift on each rider after his unlink (f32 32.0 at 0xa7624):
/// `r.currentOrigin` only, which his next end frame overwrites from the
/// playerstate.
const LIFT: f32 = 32.0;

/// A vehicle whose health ran out: `G_Damage`'s attacker (the world when
/// none) and its normalised direction, the die callback's arguments.
#[derive(Clone, Copy, Debug)]
pub struct Death {
    pub id: EntId,
    pub attacker: EntId,
    pub dir: [f32; 3],
}

/// One death's walk over the seats.
#[derive(Clone, Copy, Debug)]
pub struct Walk {
    death: Death,
    /// The next seat to look at.
    seat: usize,
    /// The rider whose callback is running, and his health before the pad.
    probing: Option<(usize, i32)>,
}

/// A rider's damage callback to run: its receiver and its nine arguments.
pub type Callback = (EntId, Vec<Value>);

/// Starts the walk of `death`; [`step`] takes it from there.
pub fn begin(host: &mut GameHost, death: Death) {
    host.vehicles.dying.push(Walk {
        death,
        seat: 1,
        probing: None,
    });
}

/// The innermost walk's next turn: the rider whose callback returned is
/// put out, then the next rider is padded and his callback handed back. At
/// the walk's end the jolt, and `None`.
pub fn step(host: &mut GameHost, cx: &mut Cx) -> Option<Callback> {
    loop {
        let walk = host.vehicles.dying.last_mut()?;
        let death = walk.death;
        if let Some((slot, health)) = walk.probing.take() {
            finish_rider(host, cx, death.id, slot, health);
            continue;
        }
        let rider = host
            .vehicles
            .live
            .get(&death.id)
            .and_then(|v| (walk.seat..=MAX_SEATS).find_map(|s| v.seats[s].map(|r| (s, r))));
        let Some((seat, slot)) = rider else {
            host.vehicles.dying.pop();
            jolt(host, death);
            return None;
        };
        walk.seat = seat + 1;
        let Some(victim) = host.ents.handle(slot as u32) else {
            continue;
        };
        let health = host.client_vitals[slot].health;
        walk.probing = Some((slot, health));
        host.client_vitals[slot].health = health + PROBE_PAD;
        if let Some(args) = probe_args(host, cx, death.attacker, victim, slot) {
            return Some((victim, args));
        }
    }
}

/// `G_DamageClient` (0x4fd4b) for the probe hit, inflictor and attacker
/// both the vehicle's attacker, no direction, the rider's origin as the
/// point: `None` where it never reaches the callback (no `takedamage`, a
/// tank's driver shielded from anything but his own tank).
fn probe_args(
    host: &mut GameHost,
    cx: &mut Cx,
    attacker: EntId,
    victim: EntId,
    slot: usize,
) -> Option<Vec<Value>> {
    if !host.client_vitals[slot].takedamage || host.client_vitals[slot].dead {
        return None;
    }
    let damage = host
        .vehicles
        .occupant_damage(slot, Some(attacker), PROBE_DAMAGE, 0)?;
    let weapon = super::inflictor_weapon(host, Some(attacker));
    let weapon = host.defs.weapon_name(weapon).unwrap_or("none").to_string();
    let point = crate::game::item::origin_of(host, cx, victim);
    Some(vec![
        Value::Entity(attacker),
        Value::Entity(attacker),
        Value::Int(damage),
        Value::Int(0),
        Value::String(cx.intern_exact("MOD_EXPLOSIVE")),
        Value::String(cx.intern_exact(&weapon)),
        Value::Vector(point),
        Value::Vector([0.0; 3]),
        Value::String(cx.intern_exact("none")),
    ])
}

/// The rider's health back as it was; a hit that left the padded health
/// whole marks him shielded, and `VEH_UnlinkPlayer` searches a dismount
/// spot for him (a blocked search leaves him seated), the rest get out in
/// place. Then `G_SetOrigin` lifts his `r.currentOrigin` [`LIFT`] units.
fn finish_rider(host: &mut GameHost, cx: &mut Cx, id: EntId, slot: usize, health: i32) {
    let shielded = host.client_vitals[slot].health == health + PROBE_PAD;
    host.client_vitals[slot].health = health;
    let Some(player) = host.ents.handle(slot as u32) else {
        return;
    };
    if host.vehicles.seat_of(slot).is_none_or(|(v, _)| v != id) {
        return;
    }
    let from = crate::game::item::origin_of(host, cx, player);
    let raised = host.engine_notifies.len();
    let out = super::seat::unlink_player(host, cx, slot, shielded, from, super::seat::PLAYER_BOX);
    // `VEH_UnlinkPlayer`'s `deactivated` pair goes out inside the callback,
    // not with the entity pass's notifies.
    let fresh = host.engine_notifies.split_off(raised);
    host.vehicles.die_notifies.extend(fresh);
    // `SetClientOrigin` at the spot a dismount found (one unit up), else
    // where he sat.
    let at = match host.vehicles.seat_ops.last() {
        Some(super::seat::SeatOp::Exit {
            slot: s,
            origin: Some(spot),
            ..
        }) if out && *s == slot => [spot[0], spot[1], spot[2] + 1.0],
        _ => from,
    };
    // `doDamage` and the playerstate keep the unlifted origin.
    host.vehicles.lifted.insert(slot, at);
    let field = cx.intern_folded("origin");
    let _ = host.write_field(
        cx,
        player,
        field,
        Value::Vector([at[0], at[1], at[2] + LIFT]),
    );
}

/// The die callback's tail: an attacker holding a grenade or a launcher
/// jolts the hull along the hit's direction (0x8d492, scale 1).
fn jolt(host: &mut GameHost, death: Death) {
    let held = super::inflictor_weapon(host, Some(death.attacker));
    let heavy = host
        .weapons
        .get(held)
        .is_some_and(|d| matches!(d.weapon_type.as_str(), "grenade" | "projectile"));
    if !heavy {
        return;
    }
    let now = host.level_time_ms;
    let Some(v) = host.vehicles.live.get(&death.id) else {
        return;
    };
    let info = host.vehicles.info_of(v).clone();
    if let Some(v) = host.vehicles.live.get_mut(&death.id) {
        v.phys
            .jolt(&info, glam::Vec3::from(death.dir), 1.0, 0.0, 0.0, now);
    }
}

/// Every death queued outside a builtin, walked with each callback run
/// through `run`, which starts the thread and runs it to its first `wait`.
pub fn drain(host: &mut GameHost, cx: &mut Cx) -> Option<Callback> {
    if host.vehicles.dying.is_empty() {
        let death = host.vehicles.deaths.first().copied()?;
        host.vehicles.deaths.remove(0);
        begin(host, death);
    }
    step(host, cx)
}
