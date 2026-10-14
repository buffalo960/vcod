//! The builtins United Offensive's stock MP scripts call that 1.1's do not:
//! UO's own additions and the 1.1 ones no 1.1 gametype reached
//! (docs/research/coduo-diff.md 13 and 20). Addresses are
//! `game.mp.uo.i386.so`'s. The vehicle methods are not here: vcod-server
//! spawns no vehicle (coduo-diff.md 20).

use super::client::client_receiver;
use super::entity::entity_receiver;
use crate::game::host::{GameHost, SimOp};
use vcod_gsc::{Cx, ErrorKind, Target, Value};

pub type Builtin = fn(&mut GameHost, &mut Cx, Option<Target>, &[Value]) -> Result<Value, ErrorKind>;

pub const NAMES: &[(&str, Builtin)] = &[
    ("getfullclipammo", get_full_clip_ammo),
    ("isvalidplayer", is_valid_player),
    ("distancesquared", distance_squared),
    ("iprintlnbold", iprint_line_bold),
    ("getweaponslotammo", get_weapon_slot_ammo),
    ("getweaponslotclipammo", get_weapon_slot_clip_ammo),
    ("attackbuttonpressed", attack_button_pressed),
    ("isinvehicle", is_in_vehicle),
    ("viewkick", view_kick),
    ("settakedamage", set_take_damage),
    ("setbounds", set_bounds),
    ("setcontents", set_contents),
];

pub fn lookup(folded: &str) -> Option<Builtin> {
    NAMES.iter().find(|(n, _)| *n == folded).map(|(_, f)| *f)
}

/// `getFullClipAmmo(weapon)` (0x6782e): the weapon file's `clipSize`.
pub fn get_full_clip_ammo(
    host: &mut GameHost,
    cx: &mut Cx,
    _recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let Some(Value::String(name)) = args.first() else {
        return Err(ErrorKind::BadType("getFullClipAmmo takes a weapon name"));
    };
    let size = host
        .defs
        .weapon_index(cx.resolve(*name))
        .and_then(|i| host.weapons.get(i))
        .map_or(0, |d| d.clip_size as i32);
    Ok(Value::Int(size))
}

/// `isValidPlayer(ent)` (0x67176): 1 for an entity in use with a client
/// whose connection state reads 2, anything else 0. vcod's client entity
/// lives from `ClientConnect`, so a client still loading reads 1 here
/// (INFERRED: the state at `client+0x4520` is `CON_CONNECTED`).
pub fn is_valid_player(
    host: &mut GameHost,
    _cx: &mut Cx,
    _recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let valid = match args.first() {
        Some(Value::Entity(id)) => host.ents.get(*id).is_some_and(|e| e.client.is_some()),
        _ => false,
    };
    Ok(Value::Int(i32::from(valid)))
}

/// `distanceSquared(a, b)` (0x6b83d), `VectorDistanceSquared` as a float.
pub fn distance_squared(
    _host: &mut GameHost,
    _cx: &mut Cx,
    _recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let (Some(Value::Vector(a)), Some(Value::Vector(b))) = (args.first(), args.get(1)) else {
        return Err(ErrorKind::BadType("distanceSquared takes two vectors"));
    };
    let d: f32 = (0..3).map(|i| (a[i] - b[i]) * (a[i] - b[i])).sum();
    Ok(Value::Float(d))
}

/// `iPrintLnBold(...)`, the function (0x66e86) to everyone and the player
/// method (0x489fd) to one: `Scr_MakeGameMessage` with `g`, the bold
/// centre print, where `iPrintLn` sends `f`.
pub fn iprint_line_bold(
    host: &mut GameHost,
    cx: &mut Cx,
    recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let text = super::message::construct(host, cx, args);
    let cmd = format!("g \"{}\"", text.replace('"', "'"));
    match recv {
        Some(_) => {
            let slot = client_receiver(host, recv)?;
            host.client_commands.push((slot, cmd));
        }
        None => {
            for slot in host.client_slots() {
                host.client_commands.push((slot, cmd.clone()));
            }
        }
    }
    Ok(Value::Undefined)
}

/// The weapon in a named slot, 0 for none (`getWeaponSlotAmmo`'s and
/// `getWeaponSlotClipAmmo`'s shared head).
fn slot_weapon(
    host: &GameHost,
    cx: &Cx,
    recv: Option<Target>,
    args: &[Value],
) -> Result<(usize, usize), ErrorKind> {
    let client = client_receiver(host, recv)?;
    let Some(Value::String(name)) = args.first() else {
        return Err(ErrorKind::BadType("that builtin takes a weapon slot name"));
    };
    let slot = crate::weapons::SLOT_NAMES
        .iter()
        .position(|s| *s == cx.resolve(*name))
        .filter(|i| *i > 0)
        .ok_or_else(|| {
            ErrorKind::Custom(format!(
                "Unknown weaponslot name {}. Valid weaponslots are \"primary\", \"primaryb\", \"pistol\", \"grenade\", and \"smokegrenade\"",
                cx.resolve(*name)
            ))
        })?;
    Ok((client, host.client_weapons[client].slots[slot] as usize))
}

/// `self getWeaponSlotAmmo(slot)` (0x4838a): the reserve of the slot's
/// weapon, or its clip for a clip-only one; 0 for an empty slot.
pub fn get_weapon_slot_ammo(
    host: &mut GameHost,
    cx: &mut Cx,
    recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let (client, weapon) = slot_weapon(host, cx, recv, args)?;
    let Some(def) = host.weapons.get(weapon) else {
        return Ok(Value::Int(0));
    };
    let a = &host.client_ammo[client];
    let rounds = if def.clip_only {
        a.clip[def.clip_index]
    } else {
        a.ammo[def.ammo_index]
    };
    Ok(Value::Int(i32::from(rounds)))
}

/// `self getWeaponSlotClipAmmo(slot)` (0x486f3): the slot's weapon's clip.
pub fn get_weapon_slot_clip_ammo(
    host: &mut GameHost,
    cx: &mut Cx,
    recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let (client, weapon) = slot_weapon(host, cx, recv, args)?;
    let Some(def) = host.weapons.get(weapon) else {
        return Ok(Value::Int(0));
    };
    Ok(Value::Int(i32::from(
        host.client_ammo[client].clip[def.clip_index],
    )))
}

/// `self attackButtonPressed()`: the attack bit of the client's last cmd,
/// as `useButtonPressed` reads the use bit.
pub fn attack_button_pressed(
    host: &mut GameHost,
    _cx: &mut Cx,
    recv: Option<Target>,
    _args: &[Value],
) -> Result<Value, ErrorKind> {
    let slot = client_receiver(host, recv)?;
    let held = host.client_buttons[slot] & vcod_common::net::msg::BUTTON_ATTACK != 0;
    Ok(Value::Int(i32::from(held)))
}

/// `self isInVehicle()`: always 0, since vcod-server spawns no vehicle.
pub fn is_in_vehicle(
    host: &mut GameHost,
    _cx: &mut Cx,
    recv: Option<Target>,
    _args: &[Value],
) -> Result<Value, ErrorKind> {
    let slot = client_receiver(host, recv)?;
    Ok(Value::Int(i32::from(host.vehicles.seat_of(slot).is_some())))
}

/// `self viewKick(force, origin)` (0x6db55): what a hit leaves for
/// `P_DamageFeedback`, without the hit. The client's damage this frame is
/// `(force * maxHealth + 50) / 100` and its direction the player's origin
/// less `origin`, so the end frame turns it into `damageCount` `force`, the
/// view kick and the pain event, as a bullet's would (`_shellshock_gmi`).
pub fn view_kick(
    host: &mut GameHost,
    _cx: &mut Cx,
    recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let slot = client_receiver(host, recv)?;
    let (Some(force), Some(Value::Vector(from))) = (args.first(), args.get(1)) else {
        return Err(ErrorKind::Custom(
            "USAGE: <player> viewkick <force 0-127> <source position>\n".into(),
        ));
    };
    let force = match force {
        Value::Int(i) => *i,
        Value::Float(f) => *f as i32,
        _ => return Err(ErrorKind::BadType("viewkick takes a force")),
    };
    if force < 0 {
        return Err(ErrorKind::Custom(format!("viewkick: damage {force} < 0\n")));
    }
    host.client_sim_ops
        .push((slot, SimOp::ViewKick { force, from: *from }));
    Ok(Value::Undefined)
}

/// `setTakeDamage(bool)` (0x691d2): `ent->takedamage`. Nothing in vcod
/// damages a script model yet, so the call is checked and has no effect;
/// `_treefall_gmi` then waits on a `damage` notify that never comes.
pub fn set_take_damage(
    _host: &mut GameHost,
    _cx: &mut Cx,
    recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    entity_receiver(recv)?;
    if args.len() != 1 {
        return Err(ErrorKind::Custom("SetTakeDamage true/false\n".into()));
    }
    Ok(Value::Undefined)
}

/// `setBounds(mins, maxs)` (0x69716): `r.mins` and `r.maxs`, relinked. vcod
/// keeps no per-entity bounds for a script model's link, so the call is
/// checked and has no effect.
pub fn set_bounds(
    _host: &mut GameHost,
    _cx: &mut Cx,
    recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    entity_receiver(recv)?;
    match (args.first(), args.get(1)) {
        (Some(Value::Vector(_)), Some(Value::Vector(_))) => Ok(Value::Undefined),
        _ => Err(ErrorKind::BadType("setBounds takes two vectors")),
    }
}

/// `setContents(int)` (0x696b9): `r.contents` set and the entity relinked,
/// returning the old contents. vcod relinks through the link shape, which
/// an entity with none (a model never linked) lacks; the new contents then
/// stay unread, and the old read 0.
pub fn set_contents(
    host: &mut GameHost,
    _cx: &mut Cx,
    recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let id = entity_receiver(recv)?;
    let Some(Value::Int(contents)) = args.first() else {
        return Err(ErrorKind::BadType("setContents takes an int"));
    };
    let old = host
        .ents
        .get(id)
        .and_then(|e| e.link.as_ref())
        .map_or(0, |l| l.contents);
    host.set_link_contents(id, *contents);
    Ok(Value::Int(old))
}
