//! `Scr_Vehicle_Think` (game.mp.uo 0x8c7cc) around the motion step: the
//! dead riders' exits, the guns' timers, heat and fire notifies, and what
//! the riders read back (docs/research/coduo-vehicles.md 12 and 13).

use super::info::KIND_FOUR_WHEEL;
use super::{Gun, MAX_SEATS};
use crate::game::host::GameHost;
use vcod_gsc::{Cx, EntId};

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
    let v = host.vehicles.live.get_mut(&id).unwrap();
    if !v.initialized {
        v.initialized = true;
    }
    v.wire.time2 = (1000.0 * info.suspension_travel) as i32;
    let driver = input(v.seats[1]);
    let gunner = input(v.seats[2]);
    // The alt gun's heat (think step 9): the check, then the cooling.
    let mut events = Vec::new();
    if v.guns.heat[0] >= OVERHEAT && !v.guns.overheating[0] {
        v.guns.overheating[0] = true;
        if info.kind != KIND_FOUR_WHEEL {
            events.push(EV_OVERHEATING);
        }
    } else if v.guns.overheating[0] && v.guns.heat[0] <= RECOVER {
        v.guns.overheating[0] = false;
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
    let capturing = false;
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
    if v.guns.heat[1] >= OVERHEAT && !v.guns.overheating[1] {
        v.guns.overheating[1] = true;
        events.push(EV_OVERHEATING);
        // Retail writes the seat mask here (0x87268).
        v.wire.anim_movetype = 1;
    } else if v.guns.overheating[1] && v.guns.heat[1] <= RECOVER {
        v.guns.overheating[1] = false;
        v.wire.anim_movetype = 0;
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
    for h in &mut v.guns.loop_hold {
        *h = (*h - STEP_MS).max(0);
    }
    if let Some(s) = gunner_slot {
        host.vehicles.overheated_gunners.retain(|&g| g != s);
        if gunner_overheated {
            host.vehicles.overheated_gunners.push(s);
        }
    }
    if let Some(e) = host.ents.get_mut(id) {
        for ev in events {
            e.events.add(ev, 0);
        }
    }
    for n in notify {
        host.engine_notifies.push((id, n, Vec::new()));
    }
}

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
