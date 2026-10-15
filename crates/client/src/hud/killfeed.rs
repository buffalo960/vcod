//! Obituaries: `EV_OBITUARY` decoded into a kill line for the game message
//! window (`messages::KillLine`), as `CG_Obituary` hands one to the engine.
//! Field mapping, icon rule and placement: docs/research/cod11-hud-protocol.md,
//! sections 1-2 and 1.1.

use vcod_common::net::events::GameEvent;

/// `attackerEntityNum` for a world kill (doc section 1).
const ENTITYNUM_WORLD: i32 = vcod_common::net::protocol::ENTITYNUM_WORLD as i32;

/// `Obituary::mod_` on a weapon death; real ids are 0..=24.
const NO_MOD: i32 = -1;

// Means-of-death ids (doc section 2); only the ones used here are named.
#[cfg_attr(not(test), allow(dead_code))] // only exercised by this module's own tests
const MOD_RIFLE_BULLET: i32 = 2;
const MOD_MELEE: i32 = 7;
const MOD_HEAD_SHOT: i32 = 8;
const MOD_WATER: i32 = 16;
const MOD_SLIME: i32 = 17;
const MOD_CRUSH: i32 = 19;
const MOD_FALLING: i32 = 21;
const MOD_SUICIDE: i32 = 22;

/// A decoded `EV_OBITUARY`. `attacker` is `None` for a world kill or suicide.
/// Exactly one of `weapon`/`mod_` is meaningful, discriminated by `mod_ == NO_MOD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Obituary {
    pub attacker: Option<u32>,
    pub victim: u32,
    /// 1-based index into CS 7; 0 on a MOD death.
    pub weapon: i32,
    /// Means-of-death id (doc section 2); `NO_MOD` on a weapon death.
    pub mod_: i32,
}

/// Doc section 1 for the victim/attacker fields, section 2 for `eventParm`
/// bit 7 selecting MOD versus weapon index.
pub fn decode_obituary(ev: &GameEvent) -> Obituary {
    let victim = ev.other_entity_num;
    let attacker_raw = ev.attacker_entity_num;
    // A negative attacker is `GameEvent`'s no-entity sentinel; guard it
    // before the `as u32` cast.
    let attacker =
        (attacker_raw >= 0 && attacker_raw != ENTITYNUM_WORLD && attacker_raw as u32 != victim)
            .then_some(attacker_raw as u32);
    let (weapon, mod_) = if ev.parm & 0x80 != 0 {
        (0, ev.parm & 0x7f)
    } else {
        (ev.parm, NO_MOD)
    };
    Obituary {
        attacker,
        victim,
        weapon,
        mod_,
    }
}

/// The seven MODs the server tags with bit 7; anything else falls to
/// `killIconDied` like the client's switch default. Drown and slime ship no
/// art (doc section 2).
fn mod_icon(mod_: i32) -> &'static str {
    match mod_ {
        MOD_MELEE => "gfx/hud/death_melee",
        MOD_HEAD_SHOT => "gfx/hud/death_headshot",
        MOD_WATER => "gfx/hud/death_drown",
        MOD_SLIME => "gfx/hud/death_slime",
        MOD_CRUSH => "gfx/hud/death_crush",
        MOD_FALLING => "gfx/hud/death_falling",
        MOD_SUICIDE => "gfx/hud/death_suicide",
        _ => "gfx/hud/death_died",
    }
}

/// The weapon's `killIcon` when non-empty, else the MOD icon. `kill_icon` is
/// resolved by the caller, which has filesystem access.
pub fn icon_for(ob: &Obituary, kill_icon: Option<&str>) -> String {
    if let Some(icon) = kill_icon.filter(|s| !s.is_empty()) {
        return icon.to_string();
    }
    mod_icon(ob.mod_).to_string()
}

/// Stock `g_TeamColor_Axis`/`g_TeamColor_Allies` from
/// `pak0.pk3:safemode_mp.cfg`; the cvars are server-settable.
const AXIS_COLOR: [f32; 4] = [1.0, 0.5, 0.5, 1.0];
const ALLIES_COLOR: [f32; 4] = [0.5, 0.5, 1.0, 1.0];

/// Team 1 is Axis, 2 is Allies; 0 and 3 render white (doc section 1).
pub fn team_color(team: i32) -> [f32; 4] {
    match team {
        1 => AXIS_COLOR,
        2 => ALLIES_COLOR,
        _ => [1.0, 1.0, 1.0, 1.0],
    }
}

/// A name as the kill line draws it: `CG_Obituary` cuts it to 31 bytes, and
/// the engine's line writer (CoDMP.exe 0x409590) drops every `^0`..`^7`, so
/// the whole name takes its team colour.
pub fn line_name(name: &str) -> String {
    let b = &name.as_bytes()[..name.len().min(31)];
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'^' && b.get(i + 1).is_some_and(|c| (b'0'..=b'7').contains(c)) {
            i += 2;
            continue;
        }
        if b[i] != b'\n' && b[i] != b'\r' {
            out.push(b[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(parm: i32, victim: u32, attacker: i32) -> GameEvent {
        GameEvent {
            event: crate::fx::registry::EV_OBITUARY,
            parm,
            entity_num: 99,
            client_num: -1,
            weapon: 0,
            surf_type: 0,
            pos: [0.0; 3],
            dir: [0.0; 3],
            scale: 0,
            other_entity_num: victim,
            attacker_entity_num: attacker,
            e_type: 0,
            eflags: 0,
        }
    }

    #[test]
    fn icon_prefers_weapon_kill_icon_for_weapon_deaths() {
        let ob = Obituary {
            attacker: Some(1),
            victim: 2,
            weapon: 3,
            mod_: MOD_RIFLE_BULLET,
        };
        assert_eq!(
            icon_for(&ob, Some("gfx/hud/hud@death_kar98.tga")),
            "gfx/hud/hud@death_kar98.tga"
        );
        let ob = Obituary {
            attacker: None,
            victim: 2,
            weapon: 0,
            mod_: MOD_FALLING,
        };
        assert_eq!(icon_for(&ob, None), "gfx/hud/death_falling");
    }

    #[test]
    fn real_weapon_files_carry_kill_icons() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let w = vcod_common::weapon::load(&fs, "kar98k_mp").unwrap();
        assert_eq!(w.kill_icon.as_deref(), Some("gfx/hud/hud@death_kar98.tga"));
    }

    #[test]
    fn decode_reads_victim_attacker_and_weapon_index() {
        let ob = decode_obituary(&ev(5, 3, 7));
        assert_eq!(
            ob,
            Obituary {
                attacker: Some(7),
                victim: 3,
                weapon: 5,
                mod_: NO_MOD,
            }
        );
    }

    #[test]
    fn decode_reads_mod_when_bit_seven_is_set() {
        let parm = 0x80 | MOD_FALLING;
        let ob = decode_obituary(&ev(parm, 3, 7));
        assert_eq!(
            ob,
            Obituary {
                attacker: Some(7),
                victim: 3,
                weapon: 0,
                mod_: MOD_FALLING,
            }
        );
    }

    #[test]
    fn decode_blanks_attacker_for_world_kills_and_suicides() {
        // world (ENTITYNUM_WORLD)
        assert_eq!(decode_obituary(&ev(5, 3, 1022)).attacker, None);
        // suicide
        assert_eq!(decode_obituary(&ev(5, 3, 3)).attacker, None);
        assert_eq!(decode_obituary(&ev(5, 3, 7)).attacker, Some(7));
    }

    #[test]
    fn a_line_name_is_cut_and_loses_its_colour_codes() {
        assert_eq!(line_name("^1Foo^7bar^^9"), "Foobar^^9");
        assert_eq!(line_name(&"x".repeat(40)).len(), 31);
    }

    #[test]
    fn decode_guards_a_negative_attacker_entity_num_before_the_cast() {
        // GameEvent's no-entity sentinel must not wrap through `as u32`.
        assert_eq!(decode_obituary(&ev(5, 3, -1)).attacker, None);
    }
}
