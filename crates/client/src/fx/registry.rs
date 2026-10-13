//! Event to visual-effect resolution: the MP `EV_*` ids joined with
//! `fx/iw_impacts.csv`. docs/research/cod11-events-and-fx.md sections 1 and 5a.

use crate::fx::sim::SpawnAt;
use glam::Vec3;
use std::collections::HashMap;
use std::sync::{LazyLock, PoisonError, RwLock};
use vcod_common::net::events::{GameEvent, byte_to_dir};
use vcod_common::net::protocol::{CsLayout, CsRange};
use vcod_common::pk3::Pk3Fs;
use vcod_common::weapon::WeaponDef;

pub use vcod_common::net::event_ids::*;

/// `surfType` index order (doc section 4); also the csv's surface column.
const SURFACE_NAMES: [&str; 23] = [
    "default", "bark", "brick", "carpet", "cloth", "concrete", "dirt", "flesh", "foliage", "glass",
    "grass", "gravel", "ice", "metal", "mud", "paper", "plaster", "rock", "sand", "snow", "water",
    "wood", "asphalt",
];

/// The csv's nine impact-type literals (docs/research/efx-grammar.md, 6a).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum ImpactKind {
    BulletSmallNormal,
    BulletSmallReflect,
    BulletLargeNormal,
    BulletLargeReflect,
    GrenadeBounce,
    GrenadeExplode,
    RocketExplode,
    MolotovExplodeNormal,
    MolotovExplodeReflect,
}

impl ImpactKind {
    fn parse(s: &str) -> Option<ImpactKind> {
        Some(match s {
            "bullet_small_normal" => ImpactKind::BulletSmallNormal,
            "bullet_small_reflect" => ImpactKind::BulletSmallReflect,
            "bullet_large_normal" => ImpactKind::BulletLargeNormal,
            "bullet_large_reflect" => ImpactKind::BulletLargeReflect,
            "grenade_bounce" => ImpactKind::GrenadeBounce,
            "grenade_explode" => ImpactKind::GrenadeExplode,
            "rocket_explode" => ImpactKind::RocketExplode,
            "molotov_explode_normal" => ImpactKind::MolotovExplodeNormal,
            "molotov_explode_reflect" => ImpactKind::MolotovExplodeReflect,
            _ => return None,
        })
    }
}

/// `(impact type, surfType)` to `.efx` path. Absent means no effect.
#[derive(Default)]
struct ImpactTable {
    map: HashMap<(ImpactKind, u8), String>,
}

impl ImpactTable {
    fn get(&self, kind: ImpactKind, surf: u8) -> Option<&str> {
        self.map.get(&(kind, surf)).map(String::as_str)
    }
}

/// Format: docs/research/efx-grammar.md section 6a. Duplicate rows agree in
/// stock data, so last write wins. Unknown literals are skipped so a mod's
/// extra rows do not break the rest.
fn parse_impacts_csv(text: &str) -> ImpactTable {
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("\"#") {
            continue;
        }
        let mut cols = line.splitn(3, ',');
        let (Some(kind_s), Some(surf_s)) = (cols.next(), cols.next()) else {
            continue;
        };
        let path = cols.next().unwrap_or("").trim();
        let Some(kind) = ImpactKind::parse(kind_s.trim()) else {
            continue;
        };
        let Some(surf) = SURFACE_NAMES.iter().position(|s| *s == surf_s.trim()) else {
            continue;
        };
        if !path.is_empty() {
            map.insert((kind, surf as u8), path.to_string());
        }
    }
    ImpactTable { map }
}

const IMPACTS_CSV_PATH: &str = "fx/iw_impacts.csv";

fn load(fs: &Pk3Fs) -> ImpactTable {
    match fs.read(IMPACTS_CSV_PATH) {
        Some(bytes) => parse_impacts_csv(&String::from_utf8_lossy(&bytes)),
        None => {
            log::warn!("fx {IMPACTS_CSV_PATH}: not found; bullet impacts will have no visual");
            ImpactTable::default()
        }
    }
}

static TABLE: LazyLock<RwLock<ImpactTable>> = LazyLock::new(RwLock::default);

/// Loads the table off `fs`: at startup, and again whenever a download
/// reopens the search path. `resolve()` reads an empty table before the first.
pub fn init(fs: &Pk3Fs) {
    *TABLE.write().unwrap_or_else(PoisonError::into_inner) = load(fs);
}

/// `Known` is a recognized event with nothing to draw. `Unknown` is an id this
/// build does not recognize; the F3 overlay counts those.
#[derive(Debug)]
pub enum Resolved {
    Spawn {
        path: String,
        at: SpawnAt,
        /// `EV_ROCKET_EXPLODE_NOMARKS`: the effect places no impact mark
        /// (cgame 0x3001e59f sets the flag `CG_ImpactMark` tests at 0x3002357f).
        nomarks: bool,
        /// The `ctx.muzzles` key of the tag it plays on, which its
        /// `relative` elements ride (`CG_MuzzleFlash`, syscall 0xe5).
        bolt: Option<u32>,
    },
    /// `EV_PLAY_FX_ON_TAG`: `path` on `entity`'s `tag`, which the caller
    /// poses ([`crate::fx::sim::FxSystem::spawn_on_tag`]). `entity` is
    /// `u32::MAX` on the playerState ring, as `GameEvent` has it; `slot`
    /// is the event's parm, the bolt's key.
    OnTag {
        path: String,
        entity: u32,
        tag: String,
        slot: i32,
    },
    /// Retail draws tracers as a hardcoded quad, not an `.efx`
    /// (docs/research/efx-grammar.md R8); see [`crate::fx::sim::FxSystem::spawn_tracer`].
    Tracer {
        muzzle: Vec3,
        impact: Vec3,
        /// `surfType == 7`, which retail draws as a one-frame segment.
        flesh: bool,
    },
    Known,
    Unknown,
}

pub struct ResolveCtx<'a> {
    /// Entity number to (world muzzle position, forward) this frame. Missing
    /// means the shooter's model has not loaded yet.
    pub muzzles: &'a HashMap<u32, (Vec3, Vec3)>,
    /// 1-based CS7 weapon index to that weapon file's `worldFlashEffect`.
    /// Missing falls back to [`flash_effect_path`].
    pub weapon_flash: &'a HashMap<i32, String>,
    /// Configstring 7's weapons while a viewmodel is drawn, for the
    /// `viewFlashEffect` of playerState-ring fire events
    /// (`entity_num == u32::MAX`); `None` while none is drawn.
    pub view_flash: Option<&'a [Option<WeaponDef>]>,
    /// The shooter whose bullet hits draw no tracer; see [`view_body`].
    pub view_body: Option<u32>,
    /// `cg_blood`: off swaps a flesh hit's effect for
    /// [`FLESH_NOBLOOD`] (`CG_BulletHitWall`, cgame 0x30039679).
    pub blood: bool,
    /// Entity number to (world `tag_brass`, its forward) this frame;
    /// `u32::MAX` is the viewmodel's, as in `muzzles`.
    pub brass: &'a HashMap<u32, (Vec3, Vec3)>,
    /// Configstring 7's weapons, by the events' 1-based `weapon`.
    pub weapons: &'a [Option<WeaponDef>],
    /// `cg_brass`: off, `CG_EjectBrass` returns at once (cgame 0x30038a30).
    pub eject_brass: bool,
    /// The gamestate's table, for the effect paths `EV_PLAY_FX` names.
    pub configstrings: &'a [String],
    /// Where its blocks sit (`Protocol::cs`).
    pub cs: &'a CsLayout,
}

/// The `.efx` an effect id names: configstring `780 + id`, ids 1..63
/// (`CG_PlayFx`, cgame 0x3001db20, which prints an error and plays nothing
/// for any other id), United Offensive's 80-slot block past 917
/// (`CsLayout`). `None` for an id out of range or an empty slot.
pub fn effect_path<'a>(configstrings: &'a [String], cs: &CsLayout, id: i32) -> Option<&'a str> {
    let path = configstrings.get(cs.slot(CsRange::Effect, id)?)?;
    (!path.is_empty()).then_some(path.as_str())
}

/// `CG_PlayFxOnTag`'s read of configstring `844 + slot` (cgame
/// 0x3001db80): the effect id in the two leading digits, the tag after
/// them. Retail turns the two characters into an id without checking them
/// (0x3001dbb5..0x3001dbbf); `None` here for a short or non-digit string.
pub fn fx_tag<'a>(
    configstrings: &'a [String],
    layout: &CsLayout,
    slot: i32,
) -> Option<(i32, &'a str)> {
    let cs = configstrings.get(layout.fx_tags() + usize::try_from(slot).ok()?)?;
    let digits = cs.get(..2)?;
    let id = digits
        .bytes()
        .all(|b| b.is_ascii_digit())
        .then(|| digits.parse().ok())??;
    Some((id, &cs[2..]))
}

/// `CG_EjectBrass` (cgame 0x30038a30, docs/research/cod11-events-and-fx.md
/// 5d): the weapon's `lastShotEjectEffect` for `EV_FIRE_WEAPON_LASTSHOT`
/// when it has one, else its `shellEjectEffect`, on the shooter's
/// `tag_brass`. Nothing under `cg_brass 0`, for weapon 0, a weapon with
/// neither effect, or a shooter whose tag is not posed this frame.
fn eject_brass(ev: &GameEvent, ctx: &ResolveCtx) -> Option<Resolved> {
    if !ctx.eject_brass {
        return None;
    }
    let def = ctx
        .weapons
        .get(usize::try_from(ev.weapon).ok().filter(|&w| w != 0)?)?
        .as_ref()?;
    let path = def
        .last_shot_eject_effect
        .as_ref()
        .filter(|_| ev.event == EV_FIRE_WEAPON_LASTSHOT)
        .or(def.shell_eject_effect.as_ref())?;
    let &(pos, dir) = ctx.brass.get(&ev.entity_num)?;
    Some(Resolved::Spawn {
        path: path.clone(),
        at: SpawnAt::Directed { pos, dir },
        nomarks: false,
        bolt: None,
    })
}

/// `CG_FireWeapon` ejects a case on every shot of a weapon without
/// `boltAction` (cgame 0x30038ce6); a bolt-action's comes on
/// `EV_EJECT_BRASS` instead.
fn fire_ejects_brass(ev: &GameEvent, ctx: &ResolveCtx) -> bool {
    usize::try_from(ev.weapon)
        .ok()
        .and_then(|w| ctx.weapons.get(w)?.as_ref())
        .is_some_and(|def| !def.bolt_action)
}

/// The flesh hit `cg_blood 0` draws (docs/research/cod11-events-and-fx.md,
/// the surface type table).
pub const FLESH_NOBLOOD: &str = "fx/impacts/flesh_hit_noblood.efx";

/// `ps.pm_flags` bits that attach the view to a player body: 0x40000 our own
/// live one, 0x10000 a followed one (docs/research/cod11-events-and-fx.md 7).
const PMF_VIEW_BODY: i32 = 0x50000;

/// The body the view rides, whose shots `CG_Tracer` draws no tracer for
/// (docs/research/cod11-events-and-fx.md 6): `ps.clientNum` while `pm_flags`
/// carries [`PMF_VIEW_BODY`], else none.
pub fn view_body(pm_flags: i32, ps_client_num: i32) -> Option<u32> {
    if pm_flags & PMF_VIEW_BODY == 0 {
        return None;
    }
    u32::try_from(ps_client_num).ok()
}

/// Class-based guess when `ctx.weapon_flash` has no entry. Every MG42/PTRS41
/// weapon file in pak0 uses `mg42hv.efx`; most others `standardflashworld.efx`.
fn flash_effect_path(event: i32) -> &'static str {
    match event {
        EV_FIRE_WEAPON_MG42 | EV_FIRE_QUADBARREL_1 | EV_FIRE_QUADBARREL_2 => {
            "fx/muzzleflashes/mg42hv.efx"
        }
        _ => "fx/muzzleflashes/standardflashworld.efx",
    }
}

/// An empty spawn list is still a recognized event.
fn known_or(spawns: Vec<Resolved>) -> Vec<Resolved> {
    if spawns.is_empty() {
        vec![Resolved::Known]
    } else {
        spawns
    }
}

/// ids 1-138. No visual; vcod draws no footstep dust.
fn is_footstep_group(event: i32) -> bool {
    const BASES: [i32; 6] = [
        EV_FOOTSTEP_RUN_BASE,
        EV_FOOTSTEP_WALK_BASE,
        EV_FOOTSTEP_PRONE_BASE,
        EV_JUMP_BASE,
        EV_LANDING_BASE,
        EV_LANDING_PAIN_BASE,
    ];
    BASES
        .iter()
        .any(|&base| (base..base + SURFACE_GROUP_SIZE).contains(&event))
}

/// The muzzle flash of a fire event, on the shooter itself: `entity_num`,
/// not `other_entity_num`. No muzzle entry yet drops the flash rather than
/// misplacing it.
fn fire_flash(ev: &GameEvent, ctx: &ResolveCtx) -> Vec<Resolved> {
    match ctx.muzzles.get(&ev.entity_num) {
        Some(&(pos, dir)) => {
            let path = match ctx.view_flash.filter(|_| ev.entity_num == u32::MAX) {
                // First person draws the view flash or nothing
                // (`CG_MuzzleFlash`, doc section 5b).
                Some(weapons) => usize::try_from(ev.weapon)
                    .ok()
                    .and_then(|i| weapons.get(i)?.as_ref()?.view_flash_effect.clone()),
                None => Some(
                    ctx.weapon_flash
                        .get(&ev.weapon)
                        .cloned()
                        .unwrap_or_else(|| flash_effect_path(ev.event).to_string()),
                ),
            };
            match path {
                Some(path) => vec![Resolved::Spawn {
                    path,
                    at: SpawnAt::Directed { pos, dir },
                    nomarks: false,
                    bolt: Some(ev.entity_num),
                }],
                None => Vec::new(),
            }
        }
        None => Vec::new(),
    }
}

/// Resolve one drained [`GameEvent`] against the cached table and this
/// frame's muzzles.
pub fn resolve(ev: &GameEvent, ctx: &ResolveCtx) -> Vec<Resolved> {
    let table = TABLE.read().unwrap_or_else(PoisonError::into_inner);
    resolve_with(ev, &table, ctx)
}

fn resolve_with(ev: &GameEvent, table: &ImpactTable, ctx: &ResolveCtx) -> Vec<Resolved> {
    match ev.event {
        EV_NONE => vec![Resolved::Known],

        // eventParm is the byte-dir surface normal; `ev.dir` (origin2) is zero
        // on these events (doc section 2). Retail draws the tracer from here,
        // in `CG_BulletHitWall` and `CG_BulletHitFlesh`, not from the fire
        // event: `EV_BULLET_TRACER` has no MP case (doc section 6).
        // `other_entity_num` is the shooter. The client-hit pair is the
        // victim's copy of a flesh hit and carries no effect of its own.
        EV_BULLET_HIT_SMALL
        | EV_BULLET_HIT_LARGE
        | EV_BULLET_HIT_CLIENT_SMALL
        | EV_BULLET_HIT_CLIENT_LARGE => {
            let mut out = Vec::new();
            let kind = match ev.event {
                EV_BULLET_HIT_SMALL => Some(ImpactKind::BulletSmallNormal),
                EV_BULLET_HIT_LARGE => Some(ImpactKind::BulletLargeNormal),
                _ => None,
            };
            let surf = ev.surf_type.clamp(0, SURFACE_NAMES.len() as i32 - 1) as u8;
            let impact = Vec3::from(ev.pos);
            let flesh = SURFACE_NAMES.get(surf as usize) == Some(&"flesh");
            let path = kind.and_then(|kind| match flesh && !ctx.blood {
                true => Some(FLESH_NOBLOOD),
                false => table.get(kind, surf),
            });
            if let Some(path) = path {
                out.push(Resolved::Spawn {
                    path: path.to_string(),
                    at: SpawnAt::Surface {
                        pos: impact,
                        normal: Vec3::from(byte_to_dir(ev.parm)),
                    },
                    nomarks: false,
                    bolt: None,
                });
            }
            // Every surface type, flesh included (doc section 6). Not
            // `fx/tagged/tracers.efx`, which is the AA gun's tracer. Chance
            // roll, distance gate and geometry live in `FxSystem::spawn_tracer`.
            let muzzle = ctx
                .muzzles
                .get(&ev.other_entity_num)
                .filter(|_| ctx.view_body != Some(ev.other_entity_num));
            if let Some(&(muzzle_pos, _)) = muzzle {
                out.push(Resolved::Tracer {
                    muzzle: muzzle_pos,
                    impact,
                    flesh,
                });
            }
            known_or(out)
        }

        // A flash, and a spent case unless the weapon works a bolt.
        EV_FIRE_WEAPON
        | EV_FIRE_WEAPONB
        | EV_FIRE_WEAPON_LASTSHOT
        | EV_FIRE_WEAPON_MG42
        | EV_FIRE_QUADBARREL_1
        | EV_FIRE_QUADBARREL_2 => {
            let mut out = fire_flash(ev, ctx);
            if fire_ejects_brass(ev, ctx) {
                out.extend(eject_brass(ev, ctx));
            }
            known_or(out)
        }

        EV_EJECT_BRASS => known_or(eject_brass(ev, ctx).into_iter().collect()),

        // eventParm is the surface normal here too (doc section 2). `_NOMARKS`
        // only suppresses the decal, so both rocket ids read the same csv row.
        // `grenade_bounce` is blank in stock data and resolves to `Known`.
        // The weapon's own `projExplosionEffect` is not spawned; `weapon.rs`
        // does not parse that key.
        EV_GRENADE_BOUNCE | EV_GRENADE_EXPLODE | EV_ROCKET_EXPLODE | EV_ROCKET_EXPLODE_NOMARKS => {
            let kind = match ev.event {
                EV_GRENADE_BOUNCE => ImpactKind::GrenadeBounce,
                EV_GRENADE_EXPLODE => ImpactKind::GrenadeExplode,
                _ => ImpactKind::RocketExplode,
            };
            let surf = ev.surf_type.clamp(0, SURFACE_NAMES.len() as i32 - 1) as u8;
            let mut out = Vec::new();
            if let Some(path) = table.get(kind, surf) {
                out.push(Resolved::Spawn {
                    path: path.to_string(),
                    at: SpawnAt::Surface {
                        pos: Vec3::from(ev.pos),
                        normal: Vec3::from(byte_to_dir(ev.parm)),
                    },
                    nomarks: ev.event == EV_ROCKET_EXPLODE_NOMARKS,
                    bolt: None,
                });
            }
            known_or(out)
        }

        // `playFx` (docs/research/cod11-events-and-fx.md 2): the effect at the
        // temp entity, on syscall 0xe3's up axis or, for `EV_PLAY_FX_DIR`,
        // on the forward `es.scale` carries as a byte dir (0x3001e6ce).
        EV_PLAY_FX | EV_PLAY_FX_DIR => {
            let Some(path) = effect_path(ctx.configstrings, ctx.cs, ev.parm) else {
                return vec![Resolved::Known];
            };
            let pos = Vec3::from(ev.pos);
            let at = match ev.event {
                EV_PLAY_FX_DIR => SpawnAt::Directed {
                    pos,
                    dir: Vec3::from(byte_to_dir(ev.scale)),
                },
                _ => SpawnAt::Point { pos },
            };
            vec![Resolved::Spawn {
                path: path.to_string(),
                at,
                nomarks: false,
                bolt: None,
            }]
        }

        // `playFxOnTag` (docs/research/cod11-events-and-fx.md 2): the tag
        // pair at configstring `844 + parm`, played where the caller poses
        // the tag on the event's own entity.
        EV_PLAY_FX_ON_TAG => {
            let Some((path, tag)) = fx_tag(ctx.configstrings, ctx.cs, ev.parm)
                .and_then(|(id, tag)| Some((effect_path(ctx.configstrings, ctx.cs, id)?, tag)))
            else {
                return vec![Resolved::Known];
            };
            vec![Resolved::OnTag {
                path: path.to_string(),
                entity: ev.entity_num,
                tag: tag.to_string(),
                slot: ev.parm,
            }]
        }

        // Recognized, no visual.
        EV_FOLIAGE_SOUND
        | EV_STANCE_FORCE_STAND
        | EV_STANCE_FORCE_CROUCH
        | EV_STANCE_FORCE_PRONE
        | EV_STEP_VIEW
        | EV_WATER_TOUCH
        | EV_WATER_LEAVE
        | EV_ITEM_PICKUP
        | EV_ITEM_PICKUP_QUIET
        | EV_AMMO_PICKUP
        | EV_NOAMMO
        | EV_EMPTYCLIP
        | EV_RELOAD
        | EV_RELOAD_FROM_EMPTY
        | EV_RELOAD_START
        | EV_RELOAD_END
        | EV_RAISE_WEAPON
        | EV_PUTAWAY_WEAPON
        | EV_WEAPON_ALT
        | EV_PULLBACK_WEAPON
        | EV_RECHAMBER_WEAPON
        | EV_MELEE_SWIPE
        | EV_FIRE_MELEE
        | EV_MELEE_HIT
        | EV_MELEE_MISS
        | EV_BULLET_TRACER
        | EV_SOUND_ALIAS
        | EV_MOLOTOV_EXPLODE
        | EV_MOLOTOV_EXPLODE_NOMARKS
        | EV_CUSTOM_EXPLODE
        | EV_CUSTOM_EXPLODE_NOMARKS
        | EV_RAILTRAIL
        | EV_BULLET
        | EV_PAIN
        | EV_CROUCH_PAIN
        | EV_DEATH
        | EV_DEBUG_LINE
        | EV_FLAMEBARREL_BOUNCE
        | EV_EARTHQUAKE
        | EV_DROPWEAPON
        | EV_ITEM_RESPAWN
        | EV_ITEM_POP
        | EV_PLAYER_TELEPORT_IN
        | EV_PLAYER_TELEPORT_OUT
        | EV_OBITUARY => vec![Resolved::Known],

        e if is_footstep_group(e) => vec![Resolved::Known],

        _ => vec![Resolved::Unknown],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;
    use vcod_common::net::events::BYTE_DIRS;
    use vcod_common::net::protocol::{CS_EFFECTS, CS_FX_TAGS};

    static EMPTY_BRASS: LazyLock<HashMap<u32, (Vec3, Vec3)>> = LazyLock::new(HashMap::new);

    fn ev(event: i32, parm: i32, surf_type: i32, pos: [f32; 3]) -> GameEvent {
        GameEvent {
            event,
            parm,
            entity_num: 40,
            client_num: -1,
            weapon: 2,
            surf_type,
            pos,
            // origin2 is zero on bullet hits; the normal comes from `parm`.
            dir: [0.0, 0.0, 0.0],
            scale: 0,
            other_entity_num: 40,
            attacker_entity_num: -1,
        }
    }

    fn empty_ctx() -> ResolveCtx<'static> {
        static EMPTY_MUZZLES: OnceLock<HashMap<u32, (Vec3, Vec3)>> = OnceLock::new();
        static EMPTY_FLASH: OnceLock<HashMap<i32, String>> = OnceLock::new();
        ResolveCtx {
            brass: &EMPTY_BRASS,
            weapons: &[],
            eject_brass: true,
            configstrings: &[],
            cs: &vcod_common::net::protocol::CS_LAYOUT_V1,
            blood: true,
            muzzles: EMPTY_MUZZLES.get_or_init(HashMap::new),
            weapon_flash: EMPTY_FLASH.get_or_init(HashMap::new),
            view_flash: None,
            view_body: None,
        }
    }

    #[test]
    fn bullet_hit_resolves_to_surface_impact() {
        let mut table = ImpactTable::default();
        table.map.insert(
            (ImpactKind::BulletSmallNormal, 1),
            "fx/impacts/test.efx".to_string(),
        );
        let e = ev(EV_BULLET_HIT_SMALL, 0, 1, [10.0, 20.0, 30.0]);
        let rs = resolve_with(&e, &table, &empty_ctx());
        assert_eq!(rs.len(), 1, "{rs:?}"); // no shooter muzzle known: no tracer
        match &rs[0] {
            Resolved::Spawn {
                path,
                at: SpawnAt::Surface { pos, normal },
                ..
            } => {
                assert_eq!(path, "fx/impacts/test.efx");
                assert_eq!(*pos, Vec3::new(10.0, 20.0, 30.0));
                assert_eq!(*normal, Vec3::from(BYTE_DIRS[0]));
            }
            other => panic!("{other:?}"),
        }
    }

    /// `playFx`'s two events read the effect off configstring `780 + parm`;
    /// the plain one plays on the up axis, the `_DIR` one along the byte dir
    /// in `es.scale`. An id outside 1..63 or an empty slot plays nothing.
    #[test]
    fn play_fx_resolves_the_effect_configstring() {
        let mut cs = vec![String::new(); 2048];
        cs[CS_EFFECTS + 3] = "fx/explosions/mp_bomb.efx".to_string();
        let ctx = ResolveCtx {
            configstrings: &cs,
            cs: &vcod_common::net::protocol::CS_LAYOUT_V1,
            ..empty_ctx()
        };
        let table = ImpactTable::default();
        let plain = ev(EV_PLAY_FX, 3, 0, [1.0, 2.0, 3.0]);
        match &resolve_with(&plain, &table, &ctx)[..] {
            [
                Resolved::Spawn {
                    path,
                    at: SpawnAt::Point { pos },
                    bolt: None,
                    ..
                },
            ] => {
                assert_eq!(path, "fx/explosions/mp_bomb.efx");
                assert_eq!(*pos, Vec3::new(1.0, 2.0, 3.0));
            }
            other => panic!("{other:?}"),
        }
        let mut dir = ev(EV_PLAY_FX_DIR, 3, 0, [0.0; 3]);
        dir.scale = 5;
        match &resolve_with(&dir, &table, &ctx)[..] {
            [
                Resolved::Spawn {
                    at: SpawnAt::Directed { dir, .. },
                    ..
                },
            ] => {
                assert_eq!(*dir, Vec3::from(BYTE_DIRS[5]));
            }
            other => panic!("{other:?}"),
        }
        for parm in [0, 4, 64] {
            let e = ev(EV_PLAY_FX, parm, 0, [0.0; 3]);
            assert!(matches!(
                &resolve_with(&e, &table, &ctx)[..],
                [Resolved::Known]
            ));
        }
    }

    /// `EV_PLAY_FX_ON_TAG` reads its pair at `844 + parm`: the effect id
    /// in two digits, then the tag, on the event's own entity. A pair
    /// naming an empty effect slot or a malformed string plays nothing.
    #[test]
    fn play_fx_on_tag_reads_the_tag_pair() {
        let mut cs = vec![String::new(); 2048];
        cs[CS_EFFECTS + 3] = "fx/explosions/mp_bomb.efx".to_string();
        cs[CS_FX_TAGS + 1] = "03tag_helmet".to_string();
        cs[CS_FX_TAGS + 2] = "07tag_helmet".to_string();
        cs[CS_FX_TAGS + 3] = "x".to_string();
        let ctx = ResolveCtx {
            configstrings: &cs,
            cs: &vcod_common::net::protocol::CS_LAYOUT_V1,
            ..empty_ctx()
        };
        let table = ImpactTable::default();
        let e = ev(EV_PLAY_FX_ON_TAG, 1, 0, [0.0; 3]);
        match &resolve_with(&e, &table, &ctx)[..] {
            [
                Resolved::OnTag {
                    path,
                    entity,
                    tag,
                    slot,
                },
            ] => {
                assert_eq!(path, "fx/explosions/mp_bomb.efx");
                assert_eq!((*entity, tag.as_str(), *slot), (40, "tag_helmet", 1));
            }
            other => panic!("{other:?}"),
        }
        for parm in [2, 3, 4] {
            let e = ev(EV_PLAY_FX_ON_TAG, parm, 0, [0.0; 3]);
            assert!(matches!(
                &resolve_with(&e, &table, &ctx)[..],
                [Resolved::Known]
            ));
        }
    }

    #[test]
    fn bullet_hit_large_uses_the_large_column() {
        let mut table = ImpactTable::default();
        table.map.insert(
            (ImpactKind::BulletLargeNormal, 13),
            "fx/impacts/metalhit_large.efx".to_string(),
        );
        let e = ev(EV_BULLET_HIT_LARGE, 5, 13, [0.0, 0.0, 0.0]);
        match &resolve_with(&e, &table, &empty_ctx())[0] {
            Resolved::Spawn { path, .. } => assert_eq!(path, "fx/impacts/metalhit_large.efx"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn blank_csv_cell_resolves_to_known_not_spawn() {
        let e = ev(EV_BULLET_HIT_SMALL, 0, 4, [0.0, 0.0, 0.0]);
        let rs = resolve_with(&e, &ImpactTable::default(), &empty_ctx());
        assert!(matches!(rs.as_slice(), [Resolved::Known]), "{rs:?}");
    }

    #[test]
    fn cg_blood_off_swaps_the_flesh_hit() {
        let mut table = ImpactTable::default();
        table.map.insert(
            (ImpactKind::BulletLargeNormal, 7),
            "fx/impacts/flesh_hit.efx".to_string(),
        );
        let e = ev(EV_BULLET_HIT_LARGE, 0, 7, [0.0, 0.0, 0.0]);
        let path = |blood| match &resolve_with(
            &e,
            &table,
            &ResolveCtx {
                brass: &EMPTY_BRASS,
                weapons: &[],
                eject_brass: true,
                configstrings: &[],
                cs: &vcod_common::net::protocol::CS_LAYOUT_V1,
                blood,
                ..empty_ctx()
            },
        )[0]
        {
            Resolved::Spawn { path, .. } => path.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(path(true), "fx/impacts/flesh_hit.efx");
        assert_eq!(path(false), FLESH_NOBLOOD);
    }

    #[test]
    fn out_of_range_surf_type_clamps_instead_of_panicking() {
        let mut table = ImpactTable::default();
        table.map.insert(
            (ImpactKind::BulletSmallNormal, 22),
            "fx/impacts/small_concrete.efx".to_string(),
        );
        let e = ev(EV_BULLET_HIT_SMALL, 0, 255, [0.0, 0.0, 0.0]);
        match &resolve_with(&e, &table, &empty_ctx())[0] {
            Resolved::Spawn { path, .. } => assert_eq!(path, "fx/impacts/small_concrete.efx"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn bullet_hit_spawns_a_tracer_from_the_shooters_muzzle() {
        let mut muzzles = HashMap::new();
        muzzles.insert(7u32, (Vec3::new(0.0, 0.0, 0.0), Vec3::X));
        let weapon_flash = HashMap::new();
        let ctx = ResolveCtx {
            brass: &EMPTY_BRASS,
            weapons: &[],
            eject_brass: true,
            configstrings: &[],
            cs: &vcod_common::net::protocol::CS_LAYOUT_V1,
            blood: true,
            muzzles: &muzzles,
            weapon_flash: &weapon_flash,
            view_flash: None,
            view_body: None,
        };
        let mut e = ev(EV_BULLET_HIT_SMALL, 0, 5, [10.0, 0.0, 0.0]); // concrete, blank cell
        e.other_entity_num = 7;
        let rs = resolve_with(&e, &ImpactTable::default(), &ctx);
        assert_eq!(rs.len(), 1, "{rs:?}"); // impact itself was a blank cell
        match &rs[0] {
            Resolved::Tracer {
                muzzle,
                impact,
                flesh,
            } => {
                assert_eq!(*muzzle, Vec3::ZERO);
                assert_eq!(*impact, Vec3::new(10.0, 0.0, 0.0));
                assert!(!flesh);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn flesh_hits_get_a_tracer() {
        let mut muzzles = HashMap::new();
        muzzles.insert(7u32, (Vec3::ZERO, Vec3::X));
        let weapon_flash = HashMap::new();
        let ctx = ResolveCtx {
            brass: &EMPTY_BRASS,
            weapons: &[],
            eject_brass: true,
            configstrings: &[],
            cs: &vcod_common::net::protocol::CS_LAYOUT_V1,
            blood: true,
            muzzles: &muzzles,
            weapon_flash: &weapon_flash,
            view_flash: None,
            view_body: None,
        };
        let mut e = ev(EV_BULLET_HIT_SMALL, 0, 7, [10.0, 0.0, 0.0]);
        e.other_entity_num = 7;
        let rs = resolve_with(&e, &ImpactTable::default(), &ctx);
        assert!(
            rs.iter()
                .any(|r| matches!(r, Resolved::Tracer { flesh: true, .. })),
            "{rs:?}"
        );
    }

    /// `CG_Tracer` draws nothing for the body the view rides, ours or a
    /// followed one, and still resolves the impact.
    #[test]
    fn the_viewed_bodys_hits_draw_no_tracer() {
        let mut muzzles = HashMap::new();
        muzzles.insert(3u32, (Vec3::ZERO, Vec3::X));
        muzzles.insert(7u32, (Vec3::ZERO, Vec3::X));
        let weapon_flash = HashMap::new();
        let mut table = ImpactTable::default();
        table.map.insert(
            (ImpactKind::BulletSmallNormal, 5),
            "fx/impacts/small_concrete.efx".to_string(),
        );
        let resolve_for = |pm_flags, ps_client, shooter| {
            let ctx = ResolveCtx {
                brass: &EMPTY_BRASS,
                weapons: &[],
                eject_brass: true,
                configstrings: &[],
                cs: &vcod_common::net::protocol::CS_LAYOUT_V1,
                blood: true,
                muzzles: &muzzles,
                weapon_flash: &weapon_flash,
                view_flash: None,
                view_body: view_body(pm_flags, ps_client),
            };
            let mut e = ev(EV_BULLET_HIT_SMALL, 0, 5, [10.0, 0.0, 0.0]);
            e.other_entity_num = shooter;
            resolve_with(&e, &table, &ctx)
        };
        let tracer = |rs: &[Resolved]| rs.iter().any(|r| matches!(r, Resolved::Tracer { .. }));
        for (pm_flags, what) in [(0x40000, "playing"), (0x30000, "following")] {
            let rs = resolve_for(pm_flags, 3, 3);
            assert!(!tracer(&rs), "{what}: {rs:?}");
            assert!(matches!(rs.as_slice(), [Resolved::Spawn { .. }]), "{rs:?}");
            assert!(
                tracer(&resolve_for(pm_flags, 3, 7)),
                "{what}: another shooter"
            );
        }
        // A free spectator's `ps.clientNum` is its own, and the view rides no body.
        assert!(tracer(&resolve_for(0x20000, 3, 3)));
    }

    /// `CG_BulletHitFlesh` hands `CG_Tracer` the same shooter, impact and
    /// `surfType` the wall path does and spawns no effect.
    #[test]
    fn client_hit_draws_the_flesh_tracer_and_no_effect() {
        let mut muzzles = HashMap::new();
        muzzles.insert(7u32, (Vec3::new(0.0, 0.0, 60.0), Vec3::X));
        let weapon_flash = HashMap::new();
        let ctx = ResolveCtx {
            brass: &EMPTY_BRASS,
            weapons: &[],
            eject_brass: true,
            configstrings: &[],
            cs: &vcod_common::net::protocol::CS_LAYOUT_V1,
            blood: true,
            muzzles: &muzzles,
            weapon_flash: &weapon_flash,
            view_flash: None,
            view_body: None,
        };
        let mut table = ImpactTable::default();
        for kind in [ImpactKind::BulletSmallNormal, ImpactKind::BulletLargeNormal] {
            table
                .map
                .insert((kind, 7), "fx/impacts/flesh_hit.efx".to_string());
        }
        for id in [EV_BULLET_HIT_CLIENT_SMALL, EV_BULLET_HIT_CLIENT_LARGE] {
            let mut e = ev(id, 0, 7, [300.0, 0.0, 50.0]);
            e.other_entity_num = 7;
            match resolve_with(&e, &table, &ctx).as_slice() {
                [
                    Resolved::Tracer {
                        muzzle,
                        impact,
                        flesh: true,
                    },
                ] => {
                    assert_eq!(*muzzle, Vec3::new(0.0, 0.0, 60.0));
                    assert_eq!(*impact, Vec3::new(300.0, 0.0, 50.0));
                }
                other => panic!("event {id}: {other:?}"),
            }
            // A shooter with no muzzle this frame: `CG_GetMuzzlePoint` fails.
            e.other_entity_num = 8;
            let rs = resolve_with(&e, &table, &ctx);
            assert!(matches!(rs.as_slice(), [Resolved::Known]), "{rs:?}");
        }
    }

    #[test]
    fn footstep_and_landing_groups_are_known_not_unknown() {
        for id in [
            EV_FOOTSTEP_RUN_BASE,
            EV_FOOTSTEP_RUN_BASE + 22,
            EV_JUMP_BASE,
            EV_LANDING_PAIN_BASE + 22,
        ] {
            let e = ev(id, 0, 0, [0.0, 0.0, 0.0]);
            let rs = resolve_with(&e, &ImpactTable::default(), &empty_ctx());
            assert!(
                matches!(rs.as_slice(), [Resolved::Known]),
                "event {id}: {rs:?}"
            );
        }
    }

    #[test]
    fn named_non_visual_events_are_known_not_unknown() {
        // Against an empty table the explode ids miss the csv and collapse
        // to Known too.
        for id in [
            EV_GRENADE_BOUNCE,
            EV_GRENADE_EXPLODE,
            EV_ROCKET_EXPLODE,
            EV_OBITUARY,
        ] {
            let e = ev(id, 0, 0, [0.0, 0.0, 0.0]);
            let rs = resolve_with(&e, &ImpactTable::default(), &empty_ctx());
            assert!(
                matches!(rs.as_slice(), [Resolved::Known]),
                "event {id}: {rs:?}"
            );
        }
    }

    #[test]
    fn fire_event_with_no_muzzle_entry_resolves_known() {
        for id in [EV_FIRE_WEAPON, EV_FIRE_WEAPON_MG42] {
            let e = ev(id, 0, 0, [0.0, 0.0, 0.0]);
            let rs = resolve_with(&e, &ImpactTable::default(), &empty_ctx());
            assert!(
                matches!(rs.as_slice(), [Resolved::Known]),
                "event {id}: {rs:?}"
            );
        }
    }

    #[test]
    fn unrecognized_event_id_is_unknown() {
        let e = ev(9999, 0, 0, [0.0, 0.0, 0.0]);
        let rs = resolve_with(&e, &ImpactTable::default(), &empty_ctx());
        assert!(matches!(rs.as_slice(), [Resolved::Unknown]), "{rs:?}");
    }

    #[test]
    fn csv_comment_and_blank_lines_are_skipped() {
        let text = concat!(
            "# This file maps impact types to effects,,\n",
            "\"# quoted comment, with a comma\",,\n",
            "\n",
            "bullet_small_normal,brick,fx/impacts/small_brick.efx\n",
        );
        let table = parse_impacts_csv(text);
        assert_eq!(
            table.get(ImpactKind::BulletSmallNormal, 2),
            Some("fx/impacts/small_brick.efx")
        );
    }

    #[test]
    fn duplicate_rows_and_missing_effect_column_are_handled() {
        let text = concat!(
            "bullet_small_normal,bark,fx/impacts/default_hit.efx\n",
            "bullet_small_normal,bark,fx/impacts/default_hit.efx\n",
            "bullet_small_reflect,bark\n", // column omitted entirely
            "grenade_bounce,gravel,\n",    // trailing comma, nothing after
        );
        let table = parse_impacts_csv(text);
        assert_eq!(
            table.get(ImpactKind::BulletSmallNormal, 1),
            Some("fx/impacts/default_hit.efx")
        );
        assert_eq!(table.get(ImpactKind::BulletSmallReflect, 1), None);
        assert_eq!(table.get(ImpactKind::GrenadeBounce, 11), None);
    }

    #[test]
    fn fire_event_spawns_a_flash_at_the_muzzle() {
        let mut muzzles = HashMap::new();
        muzzles.insert(7u32, (Vec3::new(1.0, 2.0, 3.0), Vec3::X));
        let weapon_flash = HashMap::new(); // no per-weapon path: class fallback
        let ev = GameEvent {
            event: EV_FIRE_WEAPON,
            parm: 0,
            entity_num: 7,
            client_num: 3,
            weapon: 2,
            surf_type: 0,
            pos: [0.0; 3],
            dir: [0.0; 3],
            scale: 0,
            other_entity_num: 0,
            attacker_entity_num: -1,
        };
        let rs = resolve(
            &ev,
            &ResolveCtx {
                brass: &EMPTY_BRASS,
                weapons: &[],
                eject_brass: true,
                configstrings: &[],
                cs: &vcod_common::net::protocol::CS_LAYOUT_V1,
                blood: true,
                muzzles: &muzzles,
                weapon_flash: &weapon_flash,
                view_flash: None,
                view_body: None,
            },
        );
        assert_eq!(rs.len(), 1, "{rs:?}");
        match &rs[0] {
            Resolved::Spawn {
                path,
                at: SpawnAt::Directed { pos, dir },
                ..
            } => {
                assert_eq!(path, "fx/muzzleflashes/standardflashworld.efx");
                assert_eq!(*pos, Vec3::new(1.0, 2.0, 3.0));
                assert_eq!(*dir, Vec3::X);
            }
            other => panic!("{other:?}"),
        }
    }

    /// playerState-ring fire events carry `entity_num == u32::MAX`; `main.rs`
    /// inserts a synthetic muzzle under that key.
    #[test]
    fn view_marker_fire_event_resolves_to_a_directed_flash_at_the_synthetic_muzzle() {
        let mut muzzles = HashMap::new();
        muzzles.insert(u32::MAX, (Vec3::new(5.0, 6.0, 7.0), Vec3::Y));
        let weapon_flash = HashMap::new();
        let ev = GameEvent {
            event: EV_FIRE_WEAPON,
            parm: 0,
            entity_num: u32::MAX,
            client_num: 3,
            weapon: 2,
            surf_type: 0,
            pos: [0.0; 3],
            dir: [0.0; 3],
            scale: 0,
            other_entity_num: u32::MAX,
            attacker_entity_num: -1,
        };
        let rs = resolve(
            &ev,
            &ResolveCtx {
                brass: &EMPTY_BRASS,
                weapons: &[],
                eject_brass: true,
                configstrings: &[],
                cs: &vcod_common::net::protocol::CS_LAYOUT_V1,
                blood: true,
                muzzles: &muzzles,
                weapon_flash: &weapon_flash,
                view_flash: None,
                view_body: None,
            },
        );
        assert_eq!(rs.len(), 1, "{rs:?}");
        match &rs[0] {
            Resolved::Spawn {
                at: SpawnAt::Directed { pos, dir },
                ..
            } => {
                assert_eq!(*pos, Vec3::new(5.0, 6.0, 7.0));
                assert_eq!(*dir, Vec3::Y);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn fire_event_prefers_the_weapons_own_flash_over_the_class_fallback() {
        let mut muzzles = HashMap::new();
        muzzles.insert(7u32, (Vec3::ZERO, Vec3::X));
        let mut weapon_flash = HashMap::new();
        weapon_flash.insert(5, "fx/muzzleflashes/thompson.efx".to_string()); // weapon index 5 = thompson_mp
        let ctx = ResolveCtx {
            brass: &EMPTY_BRASS,
            weapons: &[],
            eject_brass: true,
            configstrings: &[],
            cs: &vcod_common::net::protocol::CS_LAYOUT_V1,
            blood: true,
            muzzles: &muzzles,
            weapon_flash: &weapon_flash,
            view_flash: None,
            view_body: None,
        };

        let mut thompson = ev(EV_FIRE_WEAPON, 0, 0, [0.0; 3]);
        thompson.entity_num = 7;
        thompson.weapon = 5;
        match &resolve_with(&thompson, &ImpactTable::default(), &ctx)[0] {
            Resolved::Spawn { path, .. } => assert_eq!(path, "fx/muzzleflashes/thompson.efx"),
            other => panic!("{other:?}"),
        }

        let mut kar98k = ev(EV_FIRE_WEAPON, 0, 0, [0.0; 3]);
        kar98k.entity_num = 7;
        kar98k.weapon = 2;
        match &resolve_with(&kar98k, &ImpactTable::default(), &ctx)[0] {
            Resolved::Spawn { path, .. } => {
                assert_eq!(path, "fx/muzzleflashes/standardflashworld.efx")
            }
            other => panic!("{other:?}"),
        }
    }

    /// First person flashes the fired weapon's view effect, or nothing when
    /// its file has none (`CG_MuzzleFlash`, docs/research/cod11-events-and-fx.md
    /// 5b); anyone else's shot, the world one.
    #[test]
    fn view_fire_takes_the_view_flash() {
        let mut muzzles = HashMap::new();
        muzzles.insert(u32::MAX, (Vec3::ZERO, Vec3::X));
        muzzles.insert(7u32, (Vec3::ZERO, Vec3::X));
        let mut weapon_flash = HashMap::new();
        weapon_flash.insert(5, "fx/muzzleflashes/thompson.efx".to_string());
        let mut weapons = vec![None; 6];
        weapons[5] = Some(WeaponDef {
            view_flash_effect: Some("fx/muzzleflashes/thompsonview.efx".to_string()),
            ..WeaponDef::default()
        });
        weapons[2] = Some(WeaponDef::default());
        let ctx = ResolveCtx {
            brass: &EMPTY_BRASS,
            weapons: &[],
            eject_brass: true,
            configstrings: &[],
            cs: &vcod_common::net::protocol::CS_LAYOUT_V1,
            blood: true,
            muzzles: &muzzles,
            weapon_flash: &weapon_flash,
            view_flash: Some(&weapons),
            view_body: None,
        };
        let path = |entity_num, weapon| {
            let mut e = ev(EV_FIRE_WEAPON, 0, 0, [0.0; 3]);
            e.entity_num = entity_num;
            e.weapon = weapon;
            match resolve_with(&e, &ImpactTable::default(), &ctx).remove(0) {
                Resolved::Spawn { path, .. } => Some(path),
                Resolved::Known => None,
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(
            path(u32::MAX, 5).as_deref(),
            Some("fx/muzzleflashes/thompsonview.efx")
        );
        assert_eq!(path(u32::MAX, 2), None, "no viewFlashEffect, no flash");
        assert_eq!(path(u32::MAX, 9), None, "not in configstring 7");
        assert_eq!(path(7, 5).as_deref(), Some("fx/muzzleflashes/thompson.efx"));
    }

    /// Impact first, then tracer: the order `CG_BulletHitWall` runs them.
    #[test]
    fn bullet_hit_yields_both_impact_and_tracer_when_both_are_known() {
        let mut table = ImpactTable::default();
        table.map.insert(
            (ImpactKind::BulletSmallNormal, 1),
            "fx/impacts/test.efx".to_string(),
        );
        let mut muzzles = HashMap::new();
        muzzles.insert(7u32, (Vec3::new(0.0, 0.0, 0.0), Vec3::X));
        let weapon_flash = HashMap::new();
        let ctx = ResolveCtx {
            brass: &EMPTY_BRASS,
            weapons: &[],
            eject_brass: true,
            configstrings: &[],
            cs: &vcod_common::net::protocol::CS_LAYOUT_V1,
            blood: true,
            muzzles: &muzzles,
            weapon_flash: &weapon_flash,
            view_flash: None,
            view_body: None,
        };
        let mut e = ev(EV_BULLET_HIT_SMALL, 0, 1, [10.0, 20.0, 30.0]);
        e.other_entity_num = 7;

        let rs = resolve_with(&e, &table, &ctx);
        assert_eq!(rs.len(), 2, "{rs:?}");
        match &rs[0] {
            Resolved::Spawn {
                path,
                at: SpawnAt::Surface { .. },
                ..
            } => assert_eq!(path, "fx/impacts/test.efx"),
            other => panic!("expected the impact spawn first: {other:?}"),
        }
        match &rs[1] {
            Resolved::Tracer { .. } => {}
            other => panic!("expected the tracer second: {other:?}"),
        }
    }

    #[test]
    fn grenade_explode_resolves_to_a_surface_effect() {
        let mut table = ImpactTable::default();
        table.map.insert(
            (ImpactKind::GrenadeExplode, 0),
            "fx/explosions/grenade2.efx".to_string(),
        );
        let e = ev(EV_GRENADE_EXPLODE, 0, 0, [5.0, 5.0, 0.0]);
        let rs = resolve_with(&e, &table, &empty_ctx());
        assert!(matches!(&rs[..], [Resolved::Spawn { .. }]), "{rs:?}");
        match &rs[0] {
            Resolved::Spawn {
                path,
                at: SpawnAt::Surface { pos, normal },
                ..
            } => {
                assert_eq!(path, "fx/explosions/grenade2.efx");
                assert_eq!(*pos, Vec3::new(5.0, 5.0, 0.0));
                assert_eq!(*normal, Vec3::from(BYTE_DIRS[0]));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rocket_explode_and_nomarks_both_resolve_the_same_row_and_only_nomarks_drops_marks() {
        let mut table = ImpactTable::default();
        table.map.insert(
            (ImpactKind::RocketExplode, 6), // dirt
            "fx/impacts/newimps/v_blast1dirt.efx".to_string(),
        );
        for id in [EV_ROCKET_EXPLODE, EV_ROCKET_EXPLODE_NOMARKS] {
            let e = ev(id, 0, 6, [0.0, 0.0, 0.0]);
            let rs = resolve_with(&e, &table, &empty_ctx());
            match &rs[..] {
                [Resolved::Spawn { path, nomarks, .. }] => {
                    assert_eq!(path, "fx/impacts/newimps/v_blast1dirt.efx", "event {id}");
                    assert_eq!(*nomarks, id == EV_ROCKET_EXPLODE_NOMARKS);
                }
                other => panic!("event {id}: {other:?}"),
            }
        }
    }

    #[test]
    fn grenade_bounce_with_no_csv_row_resolves_known() {
        let e = ev(EV_GRENADE_BOUNCE, 0, 0, [0.0, 0.0, 0.0]);
        let rs = resolve_with(&e, &ImpactTable::default(), &empty_ctx());
        assert!(matches!(rs.as_slice(), [Resolved::Known]), "{rs:?}");
    }

    /// Skips when the game is not installed (`testing::game_fs`).
    #[test]
    fn real_iw_impacts_csv_matches_the_research_doc() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let table = load(&fs);
        assert_eq!(
            table.get(ImpactKind::BulletSmallNormal, 2), // brick
            Some("fx/impacts/small_brick.efx")
        );
        assert_eq!(
            table.get(ImpactKind::BulletLargeNormal, 20), // water
            Some("fx/impacts/waterhit_large.efx")
        );
        assert_eq!(
            table.get(ImpactKind::GrenadeExplode, 19), // snow
            Some("fx/impacts/snow_mortarlite.efx")
        );
        // blank for every surface in stock data
        assert_eq!(table.get(ImpactKind::BulletSmallReflect, 2), None);
        // the duplicate bark rows agree
        assert_eq!(
            table.get(ImpactKind::BulletSmallNormal, 1), // bark
            Some("fx/impacts/default_hit.efx")
        );
    }

    /// `CG_EjectBrass` (cgame 0x30038a30): a case on each shot of a gun
    /// without `boltAction`, on `EV_EJECT_BRASS` for one with it, the last
    /// shot's own effect when the file sets one, nothing under `cg_brass 0`.
    #[test]
    fn brass_follows_the_weapon_file_and_cg_brass() {
        let mut brass = HashMap::new();
        brass.insert(7u32, (Vec3::new(1.0, 2.0, 3.0), Vec3::Y));
        let mut weapons = vec![None; 4];
        weapons[1] = Some(WeaponDef {
            shell_eject_effect: Some("fx/shellejects/pistol.efx".to_string()),
            last_shot_eject_effect: Some("fx/shellejects/last.efx".to_string()),
            ..WeaponDef::default()
        });
        weapons[2] = Some(WeaponDef {
            shell_eject_effect: Some("fx/shellejects/rifle.efx".to_string()),
            bolt_action: true,
            ..WeaponDef::default()
        });
        weapons[3] = Some(WeaponDef::default());
        let paths = |event, weapon, on| {
            let ctx = ResolveCtx {
                brass: &brass,
                weapons: &weapons,
                eject_brass: on,
                ..empty_ctx()
            };
            let mut e = ev(event, 0, 0, [0.0; 3]);
            e.entity_num = 7;
            e.weapon = weapon;
            resolve_with(&e, &ImpactTable::default(), &ctx)
                .into_iter()
                .filter_map(|r| match r {
                    Resolved::Spawn { path, at, .. } if path.starts_with("fx/shellejects/") => {
                        match at {
                            SpawnAt::Directed { pos, dir } => {
                                assert_eq!((pos, dir), (Vec3::new(1.0, 2.0, 3.0), Vec3::Y));
                            }
                            other => panic!("{other:?}"),
                        }
                        Some(path)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            paths(EV_FIRE_WEAPON, 1, true),
            ["fx/shellejects/pistol.efx"]
        );
        assert_eq!(
            paths(EV_FIRE_WEAPON_LASTSHOT, 1, true),
            ["fx/shellejects/last.efx"]
        );
        assert_eq!(
            paths(EV_EJECT_BRASS, 1, true),
            ["fx/shellejects/pistol.efx"]
        );
        assert!(
            paths(EV_FIRE_WEAPON, 2, true).is_empty(),
            "a bolt waits for 163"
        );
        assert_eq!(paths(EV_EJECT_BRASS, 2, true), ["fx/shellejects/rifle.efx"]);
        assert!(
            paths(EV_FIRE_WEAPON, 3, true).is_empty(),
            "no shellEjectEffect"
        );
        assert!(paths(EV_EJECT_BRASS, 0, true).is_empty(), "weapon 0");
        assert!(paths(EV_FIRE_WEAPON, 1, false).is_empty(), "cg_brass 0");
        assert!(paths(EV_EJECT_BRASS, 2, false).is_empty(), "cg_brass 0");
    }
}
