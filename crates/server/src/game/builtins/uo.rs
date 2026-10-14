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
    ("playlocalannouncersound", play_local_announcer_sound),
    ("rewindfx", rewind_fx),
    ("getstance", get_stance),
    ("detach", detach),
    ("print3d", print_3d),
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
    client_receiver(host, recv)?;
    Ok(Value::Int(0))
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

/// `self playLocalAnnouncerSound(alias)` (0x494d8): `S <idx>` to that
/// player alone, the alias indexed as `playLocalSound`'s `s` is; dom's
/// capture calls.
pub fn play_local_announcer_sound(
    host: &mut GameHost,
    cx: &mut Cx,
    recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let slot = client_receiver(host, recv)?;
    let idx = super::sound::alloc_alias(host, cx, args)? - super::sound::sounds_base(host);
    host.client_commands.push((slot, format!("S {idx}")));
    Ok(Value::Undefined)
}

/// `rewindFx(ent, seconds)` (0x6c620): `F <ms>` to the entity's client, the
/// time an int times 1000; `_killcam_gmi` rewinds the viewer's effects by
/// its `archiveTime`.
pub fn rewind_fx(
    host: &mut GameHost,
    _cx: &mut Cx,
    _recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let [Value::Entity(id), t] = args else {
        return Err(ErrorKind::Custom("USAGE: rewindfx <entity> <time>".into()));
    };
    let secs = match t {
        Value::Int(i) => *i,
        Value::Float(f) => *f as i32,
        _ => return Err(ErrorKind::BadType("rewindfx takes a time")),
    };
    let slot = client_receiver(host, Some(Target::Entity(*id)))?;
    host.client_commands
        .push((slot, format!("F {}", secs.wrapping_mul(1000))));
    Ok(Value::Undefined)
}

/// `self getStance()` (0x68590): `"prone"` under `PMF_PRONE`, else
/// `"crouch"` under `PMF_DUCKED`, else `"stand"`, off the playerstate's
/// `pm_flags` (vcod's mirrored stance); a non-player is the error
/// `GetStance is only defined for players.`
pub fn get_stance(
    host: &mut GameHost,
    cx: &mut Cx,
    recv: Option<Target>,
    _args: &[Value],
) -> Result<Value, ErrorKind> {
    use vcod_common::pmove::Stance;
    let slot = client_receiver(host, recv)
        .map_err(|_| ErrorKind::Custom("GetStance is only defined for players.".into()))?;
    let stance = match host.client_stance[slot] {
        Stance::Prone => "prone",
        Stance::Crouch => "crouch",
        Stance::Stand => "stand",
    };
    Ok(Value::String(cx.intern_exact(stance)))
}

/// `self detach(model[, tag])` (0x67cb0, `G_EntDetach` 0x7818a): drops the
/// first attachment on `tag` (default `""`) whose model matches without
/// case, shifting the rest down with their ignore-collision bits; no match
/// is the error `failed to detach model '%s' from tag '%s'`. ctf's carried
/// flag comes off this way.
pub fn detach(
    host: &mut GameHost,
    cx: &mut Cx,
    recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let id = entity_receiver(recv)?;
    let Some(Value::String(model)) = args.first() else {
        return Err(ErrorKind::BadType("detach takes a model"));
    };
    let model = cx.resolve(*model).to_string();
    let tag = match args.get(1) {
        Some(Value::String(t)) => cx.resolve(*t).to_string(),
        Some(_) => return Err(ErrorKind::BadType("detach takes a tag name")),
        None => String::new(),
    };
    let e = host
        .ents
        .get(id)
        .ok_or(ErrorKind::BadType("no such entity"))?;
    let hit = e.attachments.iter().position(|(m, t)| {
        cx.resolve(*t).eq_ignore_ascii_case(&tag) && cx.resolve(*m).eq_ignore_ascii_case(&model)
    });
    let Some(i) = hit else {
        return Err(ErrorKind::Custom(format!(
            "failed to detach model '{model}' from tag '{tag}'"
        )));
    };
    let e = host.ents.get_mut(id).expect("checked above");
    e.attachments.remove(i);
    let low = e.attach_ignore_collision & ((1u8 << i) - 1);
    e.attach_ignore_collision = low | (e.attach_ignore_collision >> (i + 1) << i);
    Ok(Value::Undefined)
}

/// `print3d(origin, text[, colour[, alpha[, scale]]])` (0x66eb4): a debug
/// string for a local client's renderer (`trap_AddDebugString`), which a
/// dedicated server never draws.
pub fn print_3d(
    _host: &mut GameHost,
    _cx: &mut Cx,
    _recv: Option<Target>,
    _args: &[Value],
) -> Result<Value, ErrorKind> {
    Ok(Value::Undefined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testing::fixture;

    /// `detach` drops the matching pair and shifts the rest, ignore bits
    /// with them; a pair that is not there is an error.
    #[test]
    fn detach_drops_one_attachment_and_keeps_the_rest_in_order() {
        let (mut vm, mut host) = fixture();
        vm.with_cx(|cx| {
            let e = host.ents.spawn(cx).unwrap();
            let s = |cx: &mut Cx, t: &str| cx.intern_exact(t);
            let pairs = [("a", "tag_a"), ("flag", "tag_flag"), ("c", "tag_c")];
            let atoms: Vec<_> = pairs.iter().map(|(m, t)| (s(cx, m), s(cx, t))).collect();
            let ent = host.ents.get_mut(e).unwrap();
            ent.attachments = atoms.clone();
            ent.attach_ignore_collision = 0b100;
            let t = Some(Target::Entity(e));
            let args = [Value::String(s(cx, "FLAG")), Value::String(s(cx, "tag_flag"))];
            detach(&mut host, cx, t, &args).unwrap();
            let ent = host.ents.get(e).unwrap();
            assert_eq!(ent.attachments, vec![atoms[0], atoms[2]]);
            assert_eq!(ent.attach_ignore_collision, 0b10);
            assert!(detach(&mut host, cx, t, &args).is_err());
        });
    }

    #[test]
    fn getstance_reads_the_mirrored_stance() {
        let (mut vm, mut host) = fixture();
        vm.with_cx(|cx| {
            let e = host.ents.spawn_client(cx, 2, None).unwrap();
            host.client_stance[2] = vcod_common::pmove::Stance::Prone;
            let v = get_stance(&mut host, cx, Some(Target::Entity(e)), &[]).unwrap();
            assert!(matches!(v, Value::String(a) if cx.resolve(a) == "prone"));
            let other = host.ents.spawn(cx).unwrap();
            assert!(get_stance(&mut host, cx, Some(Target::Entity(other)), &[]).is_err());
        });
    }
}
