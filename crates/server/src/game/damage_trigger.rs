//! `trigger_damage`: `SP_trigger_damage`'s keys, `Activate_trigger_damage`
//! and `Respond_trigger_damage`, and the two walks that reach one without
//! `G_Damage`, `G_CheckHitTriggerDamage` (a round's or a swing's segment, a
//! UO shell's impact) and `G_GrenadeTouchTriggerDamage`
//! (docs/research/cod11-combat.md 14.8).

use crate::game::entity::ThinkFn;
use crate::game::host::GameHost;
use crate::game::trigger::{DamageSpec, TriggerKind};
use glam::Vec3;
use std::collections::HashMap;
use vcod_gsc::{Cx, EntId, Host, Value};

/// The `health` `SP_trigger_damage` stores and every activation restores
/// (1.1d 0x65337, 0x65262).
pub const FULL_HEALTH: i32 = 32000;

/// The contents both walks hand `trap_EntitiesInBox` (1.1d 0x6543e).
const WALK_CONTENTS: i32 = 0x40_0000;

/// The keys `SP_trigger_damage` reads. `random` at or over a non-negative
/// `wait` becomes `wait - 100` (`.rodata` 0x79ab8, UO 0xa5d88), and the
/// engine prints `trigger_damage has random >= wait`. UO alone reads
/// `teamname` (0x76f59).
pub fn spec_of(block: &HashMap<String, String>, uo: bool) -> DamageSpec {
    let float = |k: &str, d: f32| {
        block
            .get(k)
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(d)
    };
    let int = |k: &str| {
        block
            .get(k)
            .and_then(|v| v.trim().parse::<i32>().ok())
            .unwrap_or(0)
    };
    let wait = float("wait", 0.5);
    let mut random = float("random", 0.0);
    if wait <= random && wait >= 0.0 {
        random = wait - 100.0;
        log::info!("trigger_damage has random >= wait");
    }
    let team = match block.get("teamname").map(|t| t.to_ascii_lowercase()) {
        Some(t) if uo && t == "axis" => 1,
        Some(t) if uo && t == "allies" => 2,
        _ => 0,
    };
    DamageSpec {
        wait,
        random,
        accumulate: int("accumulate"),
        threshold: int("threshold"),
        spawnflags: int("spawnflags"),
        team,
        grenade_touch: false,
    }
}

/// `SP_trigger_damage` for a script's `spawn("trigger_damage", origin
/// [, spawnflags])`: no spawn keys, so every default, and no `model` key,
/// so `trap_SetBrushModel` takes inline model 0 and the trigger links as a
/// copy of the whole world where it stands, its box the world's moved to
/// its origin (docs/research/cod11-combat.md 14.8, movers doc 19).
pub fn spawn_scripted(host: &mut GameHost, cx: &mut Cx, id: EntId, spawnflags: i32) {
    let block = HashMap::from([("spawnflags".to_string(), spawnflags.to_string())]);
    let spec = spec_of(&block, host.is_uo());
    let (mins, maxs) = host.model_bounds.first().copied().unwrap_or_default();
    let shape = crate::game::trigger::TriggerShape {
        mins,
        maxs,
        model: Some(0),
    };
    host.triggers.register_damage(id, shape, spec);
    restore(host, cx, id);
    crate::game::trigger::link(host, cx, id, spawnflags);
}

/// `Respond_trigger_damage` (1.1d 0x65024, 1.5b 0x6a7f5, UO 0x76aca):
/// whether a trigger with these spawnflags takes this means of death.
/// 1.1 and 1.5 share a body; UO's means of death are renumbered and it has
/// no 0x40 arm. `mod_` -1 (the `use` arm's) clears every flag.
pub fn responds(spawnflags: i32, mod_: i32, uo: bool) -> bool {
    let f = |bit: i32| spawnflags & bit != 0;
    let refused: &[(i32, &[i32])] = if uo {
        &[
            (0x1, &[1]),
            (0x2, &[2]),
            (0x4, &[3, 4, 5, 6, 13, 14]),
            (0x8, &[3, 4, 5, 6, 9, 10, 11, 12, 13, 14, 23]),
            (0x10, &[4, 6, 10, 12, 14]),
            (0x20, &[7]),
            (0x100, &[0, 15, 16, 19, 20, 21, 22]),
        ]
    } else {
        &[
            (0x1, &[1]),
            (0x2, &[2]),
            (0x4, &[3, 4, 5, 6]),
            (0x8, &[3, 4, 5, 6, 9, 10, 13, 14, 15, 24]),
            (0x10, &[4, 6, 10, 14]),
            (0x20, &[7]),
            (0x40, &[18]),
            (0x100, &[0, 11, 12, 16, 17, 19, 20, 21, 22, 23]),
        ]
    };
    !refused
        .iter()
        .any(|(bit, mods)| f(*bit) && mods.contains(&mod_))
}

/// `Activate_trigger_damage(self, attacker, damage, mod)`: refused while a
/// wait is armed (unless `enableLinkTo` set its think), under `threshold`,
/// for a means of death the spawnflags refuse, or while `accumulate` is
/// more than the damage taken so far; UO also refuses a client attacker of
/// the other team. Otherwise `G_Trigger` queues its `"trigger"` with the
/// attacker (not for the `use` arm's mod -1), the wait is armed
/// (`multi_wait` after `(crandom * random + wait) * 1000` ms, or with no
/// positive wait `G_FreeEntity` 100 ms on), and `health` goes back to full.
/// Returns whether it fired.
pub fn activate(
    host: &mut GameHost,
    cx: &mut Cx,
    id: EntId,
    attacker: EntId,
    damage: i32,
    mod_: i32,
) -> bool {
    let Some(spec) = host
        .triggers
        .get(id)
        .filter(|t| t.kind == TriggerKind::Damage)
        .and_then(|t| t.spec)
    else {
        return false;
    };
    let uo = host.is_uo();
    if uo && !teams_agree(host, cx, spec.team, attacker) {
        return false;
    }
    let linked = host.links.is_enabled(id);
    let armed = host.ents.get(id).is_some_and(|e| e.nextthink != 0);
    if armed && !linked {
        return false;
    }
    if spec.threshold > 0 && damage < spec.threshold {
        return false;
    }
    if !responds(spec.spawnflags, mod_, uo) {
        return false;
    }
    let health_field = cx.intern_folded("health");
    let health = match host.get_field(cx, id, health_field) {
        Value::Int(h) => h,
        _ => 0,
    };
    if spec.accumulate != 0 && FULL_HEALTH - health < spec.accumulate {
        return false;
    }
    if mod_ != -1 {
        host.trigger_fires.push((id, attacker));
    }
    if !linked {
        let now = host.level_time_ms;
        if spec.wait > 0.0 {
            let r = host.rand_int();
            let crandom = 2.0 * (r as f32 / 2_147_483_648.0) - 1.0;
            let ms = ((crandom * spec.random + spec.wait) * 1000.0) as i32;
            host.ents.schedule(id, ThinkFn::MultiWait, now + ms);
        } else {
            host.ents
                .schedule(id, ThinkFn::Free, now + crate::game::trigger::SPENT_FREE_MS);
        }
    }
    restore(host, cx, id);
    true
}

/// `health` back to [`FULL_HEALTH`].
fn restore(host: &mut GameHost, cx: &mut Cx, id: EntId) {
    let field = cx.intern_folded("health");
    if let Err(e) = host.set_field(cx, id, field, Value::Int(FULL_HEALTH)) {
        log::warn!("trigger_damage {}: {e:?}", id.0);
    }
}

/// UO's team gate: the trigger's own team stands in for an attacker with no
/// client, so only a client of the other team is refused.
fn teams_agree(host: &mut GameHost, cx: &mut Cx, team: i32, attacker: EntId) -> bool {
    if team == 0 || host.ents.get(attacker).is_none_or(|e| e.client.is_none()) {
        return true;
    }
    let field = cx.intern_folded("sessionteam");
    let theirs = match host.get_field(cx, attacker, field) {
        Value::String(s) => match cx.resolve(s) {
            "axis" => 1,
            "allies" => 2,
            "spectator" => 3,
            _ => 0,
        },
        _ => 0,
    };
    theirs == 0 || theirs == team
}

/// `Pain_trigger_damage` / `Die_trigger_damage` after `G_Damage` took
/// `damage` off `health`: activate, then back to full when nothing
/// accumulates. `false` when `id` is no `trigger_damage`.
pub fn damaged(
    host: &mut GameHost,
    cx: &mut Cx,
    id: EntId,
    attacker: EntId,
    damage: i32,
    mod_name: &str,
) -> bool {
    let Some(spec) = host
        .triggers
        .get(id)
        .filter(|t| t.kind == TriggerKind::Damage)
        .and_then(|t| t.spec)
    else {
        return false;
    };
    let mod_ = host.defs.mod_index(mod_name).unwrap_or(0);
    activate(host, cx, id, attacker, damage, mod_);
    if spec.accumulate == 0 {
        restore(host, cx, id);
    }
    true
}

/// `G_CheckHitTriggerDamage(attacker, start, end, damage, mod)` (1.1d
/// 0x653f8, 1.5b 0x6ad7f, UO 0x77130) and, with `grenade`,
/// `G_GrenadeTouchTriggerDamage` (1.1d 0x655a0), which keeps only the
/// triggers `enableGrenadeTouchDamage` opened: every `trigger_damage` the
/// area tree lists under 0x400000 in the segment's box and the segment
/// crosses (`trap_SightTraceToEntity`) is notified `"damage"` with the
/// damage and the attacker on the spot, activated, and put back to full
/// when nothing accumulates. Nothing comes off `health`.
#[allow(clippy::too_many_arguments)]
pub fn sweep(
    host: &mut GameHost,
    cx: &mut Cx,
    attacker: EntId,
    start: Vec3,
    end: Vec3,
    damage: i32,
    mod_: i32,
    grenade: bool,
) {
    let (lo, hi) = (start.min(end), start.max(end));
    let ids: Vec<EntId> = host
        .area
        .entities_in_box(lo.into(), hi.into(), WALK_CONTENTS)
        .into_iter()
        .filter_map(|n| host.ents.handle(n))
        .collect();
    let origin = cx.intern_folded("origin");
    for id in ids {
        let Some(t) = host.triggers.get(id).copied() else {
            continue;
        };
        let Some(spec) = t.spec.filter(|_| t.kind == TriggerKind::Damage) else {
            continue;
        };
        if grenade && !spec.grenade_touch {
            continue;
        }
        let at = match host.get_field(cx, id, origin) {
            Value::Vector(v) => Vec3::from(v),
            _ => Vec3::ZERO,
        };
        let hulls = t
            .shape
            .model
            .filter(|m| *m != 0)
            .and_then(|m| host.model_brushes.get(m as usize));
        let crossed = match (t.shape.model, hulls) {
            // A world copy: the segment against the whole world's clip
            // moved to the trigger, under `trap_SightTraceToEntity`'s -1.
            (Some(0), _) => host.world.as_ref().is_some_and(|w| {
                let tr = w.collision.pusher_box_trace(
                    vcod_common::collision::Pusher::Copy(id.0),
                    start,
                    end,
                    Vec3::ZERO,
                    Vec3::ZERO,
                    u32::MAX,
                );
                tr.fraction < 1.0 || tr.startsolid
            }),
            (_, Some(hulls)) if !hulls.is_empty() => {
                crate::game::trigger::segment_enters_hulls(start, end, at, hulls).is_some()
            }
            _ => {
                let (lo, hi) = crate::game::trigger::abs_bounds(at.into(), &t);
                crate::game::combat::ray_box(start, end, lo.into(), hi.into()).is_some()
            }
        };
        if !crossed {
            continue;
        }
        host.engine_notifies.push((
            id,
            "damage",
            vec![Value::Int(damage), Value::Entity(attacker)],
        ));
        activate(host, cx, id, attacker, damage, mod_);
        if spec.accumulate == 0 {
            restore(host, cx, id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::trigger::TriggerShape;

    /// The spawnflags' refusals on 1.1 against the means-of-death table
    /// (cod11-combat.md 4.1): mp_foy's belltower (115) on UO takes a
    /// script blast and no round, swing or splash.
    #[test]
    fn the_spawnflags_refuse_their_means_of_death() {
        assert!(responds(0, 1, false) && responds(0, 24, false));
        assert!(!responds(1, 1, false) && responds(1, 2, false));
        assert!(!responds(0x8, 24, false) && responds(0x8, 2, false));
        assert!(!responds(0x40, 18, false) && responds(0x40, 18, true));
        assert!(!responds(0x100, 23, false) && responds(0x100, 24, false));
        let belltower = 115;
        for m in [1, 2, 4, 6, 7, 10, 12, 14] {
            assert!(!responds(belltower, m, true), "mod {m}");
        }
        for m in [3, 5, 13, 23, 25] {
            assert!(responds(belltower, m, true), "mod {m}");
        }
        assert!(responds(0x1ff, -1, true) && responds(0x1ff, -1, false));
    }

    fn place(host: &mut GameHost, cx: &mut Cx, spec: DamageSpec) -> EntId {
        let origin = cx.intern_folded("origin");
        let health = cx.intern_folded("health");
        let id = host.ents.spawn(cx).unwrap();
        host.set_field(cx, id, origin, Value::Vector([100.0, 0.0, 0.0]))
            .unwrap();
        host.set_field(cx, id, health, Value::Int(FULL_HEALTH))
            .unwrap();
        host.triggers
            .register_damage(id, TriggerShape::boxed([-16.0; 3], [16.0; 3]), spec);
        host.link_trigger(cx, id);
        id
    }

    fn spec() -> DamageSpec {
        DamageSpec {
            wait: 0.5,
            ..DamageSpec::default()
        }
    }

    /// A round through the box notifies `"damage"` and queues one
    /// `"trigger"`; a second inside the wait notifies and queues nothing;
    /// once `multi_wait` has run the next fires again. One beside the box
    /// does nothing.
    #[test]
    fn a_round_fires_once_a_wait() {
        let (mut vm, mut host) = crate::game::testing::fixture();
        vm.with_cx(|cx| {
            let id = place(&mut host, cx, spec());
            let who = host.ents.spawn(cx).unwrap();
            let shoot = |host: &mut GameHost, cx: &mut Cx, y: f32| {
                sweep(
                    host,
                    cx,
                    who,
                    Vec3::new(0.0, y, 0.0),
                    Vec3::new(300.0, y, 0.0),
                    40,
                    2,
                    false,
                )
            };
            host.level_time_ms = 1000;
            shoot(&mut host, cx, 0.0);
            assert_eq!(host.trigger_fires, vec![(id, who)]);
            assert_eq!(host.engine_notifies.len(), 1);
            assert_eq!(host.ents.get(id).unwrap().nextthink, 1500);
            shoot(&mut host, cx, 0.0);
            assert_eq!(host.trigger_fires.len(), 1);
            assert_eq!(host.engine_notifies.len(), 2);
            shoot(&mut host, cx, 40.0);
            assert_eq!(host.engine_notifies.len(), 2);
            host.run_entity_thinks(cx, 1500);
            shoot(&mut host, cx, 0.0);
            assert_eq!(host.trigger_fires.len(), 2);
        });
    }

    /// UO's shell walk is the contact alone: a point inside the box
    /// notifies, one beside it does not.
    #[test]
    fn a_shells_walk_tests_its_contact_point() {
        let (mut vm, mut host) = crate::game::testing::fixture();
        vm.with_cx(|cx| {
            place(&mut host, cx, spec());
            let who = host.ents.spawn(cx).unwrap();
            let beside = Vec3::new(130.0, 0.0, 0.0);
            sweep(&mut host, cx, who, beside, beside, 1000, 5, false);
            assert!(host.engine_notifies.is_empty());
            let inside = Vec3::new(110.0, 4.0, -15.0);
            sweep(&mut host, cx, who, inside, inside, 1000, 5, false);
            assert_eq!(host.engine_notifies.len(), 1);
            assert_eq!(host.trigger_fires.len(), 1);
        });
    }

    /// `threshold` refuses a smaller hit, and `accumulate` waits for the
    /// damage `G_Damage` took off `health` to add up; a round takes none.
    #[test]
    fn threshold_and_accumulate_gate_an_activation() {
        let (mut vm, mut host) = crate::game::testing::fixture();
        vm.with_cx(|cx| {
            let who = host.ents.spawn(cx).unwrap();
            let t = place(
                &mut host,
                cx,
                DamageSpec {
                    threshold: 50,
                    ..spec()
                },
            );
            assert!(!activate(&mut host, cx, t, who, 49, 2));
            assert!(activate(&mut host, cx, t, who, 50, 2));
            let a = place(
                &mut host,
                cx,
                DamageSpec {
                    accumulate: 100,
                    ..spec()
                },
            );
            let health = cx.intern_folded("health");
            host.set_field(cx, a, health, Value::Int(FULL_HEALTH - 60))
                .unwrap();
            damaged(&mut host, cx, a, who, 60, "MOD_EXPLOSIVE");
            assert_eq!(host.trigger_fires.len(), 1, "60 of 100 taken");
            assert_eq!(host.get_field(cx, a, health), Value::Int(FULL_HEALTH - 60));
            host.set_field(cx, a, health, Value::Int(FULL_HEALTH - 120))
                .unwrap();
            damaged(&mut host, cx, a, who, 60, "MOD_EXPLOSIVE");
            assert_eq!(host.get_field(cx, a, health), Value::Int(FULL_HEALTH));
            assert_eq!(host.trigger_fires.len(), 2);
        });
    }

    /// No positive `wait`: the first activation schedules the free.
    #[test]
    fn no_wait_spends_the_trigger() {
        let (mut vm, mut host) = crate::game::testing::fixture();
        vm.with_cx(|cx| {
            let who = host.ents.spawn(cx).unwrap();
            let t = place(
                &mut host,
                cx,
                DamageSpec {
                    wait: 0.0,
                    ..spec()
                },
            );
            host.level_time_ms = 2000;
            assert!(activate(&mut host, cx, t, who, 10, 0));
            assert!(!activate(&mut host, cx, t, who, 10, 0));
            host.run_entity_thinks(cx, 2100);
            assert!(host.ents.get(t).is_none());
        });
    }
}
