//! Protocol descriptor: the version-specific wire facts. Protocol 1 is patch
//! 1.1, protocol 6 patch 1.5 (docs/research/cod15-protocol-diff.md), protocol
//! 22 United Offensive 1.51 (docs/research/coduo-diff.md); a connection holds
//! one and everything that decodes reads it from there.

/// One netfield. `bits == 0` is a float, [`ANGLE_BITS`] a 16-bit angle;
/// another negative `bits` is a plain unsigned width
/// (docs/protocol-1.1.md, "Delta field encoding"). `offset` is the engine
/// struct offset, used only as a stable identity.
#[derive(Clone, Copy)]
pub struct NetField {
    pub name: &'static str,
    pub offset: u16,
    pub bits: i32,
}

/// Protocol 22's angle field: a zero flag, then a short of
/// `deg * 65536 / 360` truncated toward zero (coduo_lnxded 0x8080815). Decoded, it holds
/// the f32 bit pattern of the degrees, as a float field does.
pub const ANGLE_BITS: i32 = -100;

/// The playerState's HUD element block, which United Offensive widened
/// (coduo-diff.md, section 5).
pub struct HudWire {
    /// Width of each array's element count.
    pub count_bits: i32,
    pub max_elems: usize,
    /// Fields per element on the wire; [`crate::net::msg::HudElem`] holds
    /// the widest protocol's.
    pub fields: usize,
}

/// `G_CheckForCursorHints`' item values: health's, and an unowned
/// weapon's base (`base + giTag`); an owned weapon or an ammo row adds
/// `max_weapons` more (cod11-items.md 2.3, coduo-diff.md 12).
pub struct CursorHints {
    pub health: i32,
    pub weapon: i32,
}

pub struct Protocol {
    pub version: u32,
    /// The server's `shortversion`, which a connect refused for the wrong
    /// protocol names (`EXE_SERVER_IS_DIFFERENT_VER\x15<this>`).
    pub short_version: &'static str,
    /// The master server the browser asks and a heartbeat goes to (the
    /// client's and the server's binaries name it).
    pub master: &'static str,
    pub max_configstrings: usize,
    pub entity_fields: &'static [NetField],
    /// Protocol 22's second entity table: an entity whose `eType` is
    /// [`crate::net::flags::ET_VEHICLE_V22`] travels in it, behind a select
    /// bit after its number (coduo-diff.md, section 3). Empty where the
    /// protocol has no select bit.
    pub vehicle_fields: &'static [NetField],
    /// Each `vehicle_fields` entry's index into `entity_fields`; a decoded
    /// entity keeps `entity_fields` order whichever table carried it.
    pub vehicle_field_slots: &'static [u8],
    pub player_fields: &'static [NetField],
    pub client_fields: &'static [NetField],
    /// Rewrites a decoded `pm_flags` into 1.1's bit layout, the one the rest
    /// of vcod reads ([`crate::net::flags`]).
    pub pm_flags_to_v1: fn(i32) -> i32,
    /// The writer's inverse: vcod's `pm_flags` in this protocol's layout.
    pub pm_flags_from_v1: fn(i32) -> i32,
    /// What a `map_restart` that drops `pers[]` sends a client (1.1 `n`, 1.5
    /// and UO `B`); one that keeps it sends `n` on all three.
    pub map_restart_command: &'static str,
    /// The same for an entity's or the playerState's `eFlags`.
    pub eflags_to_v1: fn(i32) -> i32,
    /// A server's inverses of `eflags_to_v1` and `event_to_v1`: vcod's
    /// (1.1's) value in this protocol's layout. The snapshot writer leaves
    /// both alone; the server maps what it builds.
    pub eflags_from_v1: fn(i32) -> i32,
    pub event_from_v1: fn(i32) -> i32,
    /// `bg_itemlist`'s weapon rows: a weapon is item 1..=`max_weapons`, its
    /// configstring 7 index, and the compiled-in ammo and health rows
    /// follow (1.1 64, UO 128; coduo-diff.md 12).
    pub max_weapons: usize,
    pub cursor_hints: CursorHints,
    pub hud: HudWire,
    /// The usercmd's full-branch `weapon` width.
    pub usercmd_weapon_bits: i32,
    /// Where the configstring blocks sit.
    pub cs: CsLayout,
    /// `ET_EVENTS`: an `eType` at or above it is a temp event entity
    /// carrying event `eType - et_events`.
    pub et_events: i32,
    /// Maps one of this protocol's `EV_*` ids to vcod's (1.1's,
    /// [`crate::net::event_ids`]); an id with no 1.1 counterpart comes back
    /// as [`crate::net::event_ids::EV_FOREIGN`] `| id`.
    pub event_to_v1: fn(i32) -> i32,
    /// The same for a means of death (`EV_OBITUARY`'s parm with bit 7 set);
    /// one with no 1.1 counterpart comes back -1.
    pub mod_to_v1: fn(i32) -> i32,
    /// `FS_idPak`'s pak numbers, as its format prints them: `main/pak<n>`,
    /// `mp_pak<n>`, `sp_pak<n>` and `localized_*_pak<n>` are the stock paks
    /// `FS_ComparePaks` never fetches ([`crate::net::download::is_stock_pak`]).
    pub id_pak_numbers: &'static [&'static str],
    /// The directories and name prefixes of those stock paks.
    pub id_pak_dirs: &'static [&'static str],
    pub id_pak_prefixes: &'static [&'static str],
    /// 1.5's `FS_ComparePaks` also skips a pak whose name holds `_svr_`.
    pub skips_svr_paks: bool,
    /// The client game and UI modules whose paks open a pure `cp`
    /// (`CL_SendPureChecksums`, `SV_VerifyPaks_f`).
    pub pure_dlls: [&'static str; 2],
    /// The client's text renderer takes `^8` and `^9` as colour codes, in
    /// `g_TeamColor_Allies` and `g_TeamColor_Axis` (CoDMP.exe 1.5
    /// 0x4ba8e0, CoDUOMP.exe 0x4c02f0); 1.1's prints them
    /// (cod11-hud-protocol.md 7).
    pub team_colour_codes: bool,
}

const HUD_V1: HudWire = HudWire {
    count_bits: 5,
    max_elems: 31,
    fields: 28,
};

pub const PROTOCOL_V1: Protocol = Protocol {
    version: 1, // reported by cod_lnxded 1.1d getstatus, 2026-08-23
    short_version: "1.1",
    master: "codmaster.activision.com",
    max_configstrings: 2048, // codextended/src/shared.h:141
    entity_fields: crate::net::fields_v1::ENTITY_FIELDS,
    vehicle_fields: &[],
    vehicle_field_slots: &[],
    player_fields: crate::net::fields_v1::PLAYER_FIELDS,
    client_fields: crate::net::fields_v1::CLIENT_FIELDS,
    pm_flags_to_v1: |f| f,
    pm_flags_from_v1: crate::net::flags::pm_flags_v1_wire,
    map_restart_command: "n",
    eflags_to_v1: |f| f,
    eflags_from_v1: |f| f,
    event_from_v1: |e| e,
    max_weapons: 64, // bg_itemlist, game.mp.i386.so .data 0x7b9d8
    // game.mp.i386.so 0x4f787, 0x4f7b9 (1.5b: 0x4e51c, 0x4e56c).
    cursor_hints: CursorHints {
        health: 7,
        weapon: 9,
    },
    hud: HUD_V1,
    usercmd_weapon_bits: 6,
    cs: CS_LAYOUT_V1,
    et_events: 12,
    event_to_v1: |e| e,
    mod_to_v1: |m| m,
    // `%s/pak%d` for 0..8, CoDMP.exe 0x43b6b0.
    id_pak_numbers: &["0", "1", "2", "3", "4", "5", "6", "7", "8"],
    id_pak_dirs: &["main"],
    id_pak_prefixes: &["pak", "mp_pak", "sp_pak"],
    skips_svr_paks: false,
    pure_dlls: ["cgame_mp_x86.dll", "ui_mp_x86.dll"],
    team_colour_codes: false,
};

/// Patch 1.5: 1.1's wire with a wider, renumbered `pm_flags`
/// (docs/research/cod15-protocol-diff.md).
pub const PROTOCOL_V6: Protocol = Protocol {
    version: 6, // cod_lnxded 1.5b getinfo and getstatus, 2026-10-13
    short_version: "1.5",
    master: "codmaster.activision.com",
    max_configstrings: 2048,
    entity_fields: crate::net::fields_v1::ENTITY_FIELDS,
    vehicle_fields: &[],
    vehicle_field_slots: &[],
    player_fields: crate::net::fields_v6::PLAYER_FIELDS,
    client_fields: crate::net::fields_v1::CLIENT_FIELDS,
    pm_flags_to_v1: crate::net::flags::pm_flags_v6_to_v1,
    pm_flags_from_v1: crate::net::flags::pm_flags_v1_to_v6,
    // cod_lnxded 1.5b map_restart, live (cod15-protocol-diff.md 8).
    map_restart_command: "B",
    eflags_to_v1: |f| f,
    eflags_from_v1: |f| f,
    event_from_v1: |e| e,
    max_weapons: 64,
    cursor_hints: CursorHints {
        health: 7,
        weapon: 9,
    },
    hud: HUD_V1,
    usercmd_weapon_bits: 6,
    cs: CsLayout {
        match_timeout: Some(1628),
        vote: 15,
        ..CS_LAYOUT_V1
    },
    et_events: 12,
    event_to_v1: |e| e,
    mod_to_v1: |m| m,
    // `%s/pak%x` for 0..0xf, 1.5 CoDMP.exe 0x43e310; `_svr_` at 0x43e490.
    id_pak_numbers: HEX_PAK_NUMBERS,
    id_pak_dirs: &["main"],
    id_pak_prefixes: &["pak", "mp_pak", "sp_pak"],
    skips_svr_paks: true,
    pure_dlls: ["cgame_mp_x86.dll", "ui_mp_x86.dll"],
    team_colour_codes: true,
};

const HEX_PAK_NUMBERS: &[&str] = &[
    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "a", "b", "c", "d", "e", "f",
];

/// United Offensive 1.51: 1.1's transport with grown and partly re-encoded
/// netfield tables, a second entity table for vehicles, 63 HUD elements,
/// rebased configstrings and renumbered events (docs/research/coduo-diff.md,
/// sections 3 to 8).
pub const PROTOCOL_V22: Protocol = Protocol {
    version: 22, // coduo_lnxded 1.51b getinfo and getstatus, 2026-10-13
    short_version: "1.51",
    master: "coduomaster.activision.com",
    max_configstrings: 2048, // CoDUOMP.exe 0x4014c6
    entity_fields: crate::net::fields_v22::ENTITY_FIELDS,
    vehicle_fields: crate::net::fields_v22::VEHICLE_FIELDS,
    vehicle_field_slots: &crate::net::fields_v22::VEHICLE_FIELD_SLOTS,
    player_fields: crate::net::fields_v22::PLAYER_FIELDS,
    client_fields: crate::net::fields_v1::CLIENT_FIELDS,
    pm_flags_to_v1: crate::net::flags::pm_flags_v22_to_v1,
    pm_flags_from_v1: crate::net::flags::pm_flags_v1_to_v22,
    // UO's cgame takes 1.5's command set (coduo-diff.md 8).
    map_restart_command: "B",
    eflags_to_v1: crate::net::flags::eflags_v22_to_v1,
    eflags_from_v1: crate::net::flags::eflags_v1_to_v22,
    event_from_v1: crate::net::event_ids::event_v1_to_v22,
    max_weapons: 128, // bg_itemlist, game.mp.uo.i386.so 0xa9880
    // game.mp.uo.i386.so 0x57add, 0x57b2f: `hintLMGMount` took 7.
    cursor_hints: CursorHints {
        health: 8,
        weapon: 10,
    },
    // coduo_lnxded 0x8082550: a 6-bit count, 30 fields (SCOORD, TCOORD).
    hud: HudWire {
        count_bits: 6,
        max_elems: 63,
        fields: 30,
    },
    usercmd_weapon_bits: 7, // coduo_lnxded 0x80814ce
    cs: CS_LAYOUT_V22,
    et_events: 16, // `G_TempEntity` stores `event + 0x10`
    event_to_v1: crate::net::event_ids::event_v22_to_v1,
    mod_to_v1: mod_v22_to_v1,
    // `%s/pak%x`, `%s/pakuo%x`, `%s/pakuo0%x`, `%s/mp_pak%x` (coduo_lnxded strings).
    id_pak_numbers: HEX_PAK_NUMBERS,
    id_pak_dirs: &["main", "uo"],
    id_pak_prefixes: &["pak", "pakuo", "pakuo0", "mp_pak"],
    skips_svr_paks: true,
    // Spelled obfuscated in both binaries: coduo_lnxded `SV_VerifyPaks_f`
    // 0x808cf23 and CoDUOMP.exe 0x42db37 (coduo-diff.md 23).
    pure_dlls: ["uo_cgame_mp_x86.dll", "uo_ui_mp_x86.dll"],
    team_colour_codes: true,
};

/// United Offensive's means of death as 1.1's: `modNames`
/// (game.mp.uo.i386.so 0xacf20) drops `KICKED`, `GRABBER`, `AIRSTRIKE`,
/// `SLIME` and `LAVA` and adds the artillery, tank and jeep crush,
/// collision, flame and binoculars kills, which have no 1.1 id
/// (docs/research/coduo-diff.md, section 12). The obituary switch in
/// `uo_cgame_mp_x86.dll` (0x3002231e) reads the same ids.
fn mod_v22_to_v1(m: i32) -> i32 {
    match m {
        0..=10 => m,
        11 | 12 => m + 2, // dynamite and its splash
        15 => 16,         // water
        16 => 19,         // crush
        19..=23 => m + 1, // telefrag, falling, suicide, trigger_hurt, explosive
        _ => -1,
    }
}

/// The protocol an install hosts: 22 when United Offensive's `uo/` paks are
/// mounted, 6 when its `main/pak0.pk3` is 1.5's, whose checksum differs
/// from 1.1's (docs/research/cod15-protocol-diff.md 10), 1 otherwise.
pub fn install_protocol(fs: &crate::pk3::Pk3Fs) -> &'static Protocol {
    install_protocol_of(fs.paks())
}

/// [`install_protocol`] over a search path's paks before they are opened.
pub fn install_protocol_of<'a>(
    paks: impl IntoIterator<Item = &'a crate::pk3::PakInfo>,
) -> &'static Protocol {
    /// `FS_LoadZipFile`'s checksum of the 1.5 install's `main/pak0.pk3`, as
    /// the 1.5b server's `sv_referencedPaks` lists it.
    const PAK0_V6_CHECKSUM: i32 = 1_048_127_331;
    let mut v6 = false;
    for p in paks {
        // A `uo/` with United Offensive's own paks beside `main/` (`--mod-dir
        // uo`, `pk3::base_dirs`) is UO's.
        if p.game == "uo" && p.name.starts_with("pakuo") {
            return &PROTOCOL_V22;
        }
        v6 |= p.game == "main" && p.name == "pak0" && p.checksum == PAK0_V6_CHECKSUM;
    }
    if v6 { &PROTOCOL_V6 } else { &PROTOCOL_V1 }
}

impl Protocol {
    /// Every protocol vcod speaks, the one it tries first leading.
    pub const ALL: [&'static Protocol; 3] = [&PROTOCOL_V1, &PROTOCOL_V6, &PROTOCOL_V22];

    /// The event an `eType` carries, `None` for an ordinary entity.
    pub fn event_of(&self, etype: i32) -> Option<i32> {
        (etype >= self.et_events).then(|| (self.event_to_v1)(etype - self.et_events))
    }

    /// The protocol an `infoResponse`'s `protocol` key names.
    pub fn by_version(version: u32) -> Option<&'static Protocol> {
        Self::ALL.into_iter().find(|p| p.version == version)
    }

    /// The protocol a server's `shortversion` belongs to.
    pub fn by_short_version(v: &str) -> Option<&'static Protocol> {
        Self::ALL.into_iter().find(|p| p.short_version == v)
    }

    /// Whether a client on one's install can play the other's server. 1.1
    /// and 1.5 share `pak1`-`pak4` and cross over; United Offensive's
    /// `uo/` paks override `main/`'s weapons and menus, so it plays only
    /// itself.
    pub fn same_game(&self, other: &Protocol) -> bool {
        (self.version == PROTOCOL_V22.version) == (other.version == PROTOCOL_V22.version)
    }
}

/// Model paths sit at `CS_MODELS_V1 + index`, index 1..=255 (`G_ModelIndex`,
/// game.mp.i386.so 0x66ed8).
pub const CS_MODELS_V1: usize = 268;

/// Tag names sit at `CS_TAGS_V1 + index`, index 1..=31 (`G_TagIndex`,
/// game.mp.i386.so 0x66fda).
pub const CS_TAGS_V1: usize = 108;

/// Global fog parameters, configstring 12. Written by the game dll's
/// `setCullFog` / `setExpFog` builtins; field order per RTCW-MP
/// cg_servercmds.c `CG_ParseFog` and two live captures
/// (docs/research/cod11-server-handshake.md, "Map-dependent, index < 140").
pub const CS_FOG_V1: usize = 12;

/// `<near> <far> <density> <r> <g> <b> <fadeTime ms>`. `density > 1`
/// selects linear farclip fog across near..far; otherwise GL_EXP at that
/// density (RTCW-MP tr_main.c R_SetFog). Linear fog leaves the sky alone,
/// exp fog fogs it (`drawsky` in the same function).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogParams {
    pub near: f32,
    pub far: f32,
    pub density: f32,
    pub color: [f32; 3],
    pub fade_ms: u32,
}

impl FogParams {
    /// RTCW-MP's client adds .1 to this slot before R_SetFog so its >1 test
    /// fires; CoD's wire carries the un-offset value ("1" in both live
    /// captures), and `setExpFog` refuses a density outside (0, 1).
    pub fn is_linear(&self) -> bool {
        self.density >= 1.0
    }

    /// Empty or malformed configstrings mean "no fog state", not zero fog.
    pub fn parse(s: &str) -> Option<FogParams> {
        let mut it = s.split_whitespace();
        let mut num = || it.next().and_then(|t| t.parse::<f32>().ok());
        let near = num()?;
        let far = num()?;
        let density = num()?;
        let r = num()?;
        let g = num()?;
        let b = num()?;
        let fade_ms = num()? as u32;
        Some(FogParams {
            near,
            far,
            density,
            color: [r, g, b],
            fade_ms,
        })
    }
}

pub const GENTITYNUM_BITS: u32 = 10; // codextended/src/shared.h:395
pub const MAX_GENTITIES: usize = 1 << GENTITYNUM_BITS;
pub const ENTITYNUM_NONE: u32 = (MAX_GENTITIES - 1) as u32;
/// Reserved slot for the world entity (`worldspawn`); never a dynamic entity number.
pub const ENTITYNUM_WORLD: u32 = (MAX_GENTITIES - 2) as u32;
/// The client index in the snapshot's clientState stream (cod_lnxded 0x808e1fd).
pub const CLIENTNUM_BITS: u32 = 6;
/// Retail's `MAX_CLIENTS`: client slots index the 6-bit clientState field
/// (`ps.clientNum` gets 8), so more than 64 would collide silently.
pub const MAX_CLIENTS: usize = 1 << CLIENTNUM_BITS;

/// Serverinfo, `Cvar_InfoString(CVAR_SERVERINFO)`; `g_gametype` is a key.
pub const CS_SERVERINFO: usize = 0;
/// Systeminfo, `Cvar_InfoString_Big(CVAR_SYSTEMINFO)`, `sv_serverid` included.
pub const CS_SYSTEMINFO: usize = 1;
/// `setTeamScore("axis", ...)` writes here, "allies" to the next slot
/// (docs/research/cod11-hud-protocol.md 5, "Which team score is which").
pub const CS_AXIS_SCORE: usize = 5;
pub const CS_ALLIES_SCORE: usize = 6;
/// `northyaw`, the compass's north in world yaw degrees, copied verbatim by
/// `SP_worldspawn` (0x61cec).
pub const CS_NORTHYAW: usize = 11;
/// `level.startTime`, server-clock ms.
pub const CS_LEVEL_START_TIME: usize = 13;

/// The mirrored server cvars: names here, values in the next block at the
/// same offset; the client `Cvar_Set`s the pairs.
pub const CS_CVAR_NAMES: std::ops::RangeInclusive<usize> = 140..=203;
pub const CS_CVAR_VALUES: std::ops::RangeInclusive<usize> = 204..=267;

/// `CS_SOUNDS`: a sound-alias index (`EV_SOUND_ALIAS`'s parm, `loopSound`,
/// `s <idx>`) counts from here, so the alias range's first slot travels as 1
/// and 0 means no alias (docs/protocol-1.1.md, `s <idx>`).
pub const CS_SOUNDS: usize = CsRange::SoundAlias.bounds().0 - 1;
/// A hudelem's `text`/`label` index counts from here, 1-based like
/// `CS_SOUNDS`.
pub const CS_LOCALIZED: usize = CsRange::Localized.bounds().0 - 1;
/// A hudelem's `shaderIndex` and the objective icons count from here, 1-based.
pub const CS_SHADERS: usize = CsRange::Shader.bounds().0 - 1;
/// An effect id (`loadFx`'s return, `EV_PLAY_FX`'s parm, an `ET_FX`
/// entity's `scale`) counts from here, 1..63 (`CG_PlayFx`, cgame
/// 0x3001db20, docs/research/cod11-events-and-fx.md 5c).
pub const CS_EFFECTS: usize = CsRange::Effect.bounds().0 - 1;
/// `EV_PLAY_FX_ON_TAG`'s parm counts from here: configstring `844 + parm`
/// holds `"%02d%s"` of the effect id and the tag (cgame 0x3001db80).
pub const CS_FX_TAGS: usize = CsRange::FxTag.bounds().0 - 1;
/// The first of the 32 hint-string configstrings `G_GetHintStringIndex`
/// (0x5a238) fills; `serverCursorHintString` indexes from here.
pub const CS_HINT_STRINGS: usize = 1212;
/// 1.5's match timeout: the ms left (`%i`, empty once over) and the message
/// under it, written by `Cmd_MatchTimeout_f`, `Cmd_MatchTimein_f` and
/// `CheckMatchTimeout` (game.mp.i386.so 1.5b 0x4fea8). No 1.1 module writes
/// either (docs/research/cod15-protocol-diff.md, "Match timeout").
pub const CS_MATCH_TIMEOUT_V6: usize = 1628;
pub const CS_MATCH_TIMEOUT_TEXT_V6: usize = 1629;

/// A configstring block the game module allocates into at runtime, each
/// mirroring one engine indexer in `game.mp.i386.so`. The bounds are the
/// ones those indexers walk; the addresses are in
/// docs/research/clientstate-wire-format.md.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CsRange {
    StatusIcon,
    HeadIcon,
    Tag,
    Model,
    SoundAlias,
    Effect,
    /// `playFxOnTag`'s `"%02d%s"` effect id and tag pairs
    /// (`G_FindConfigstringIndex(.., 0x34c, 0x100, ..)`, game.mp 0x5b386).
    FxTag,
    /// `G_ShellShockIndex` (game.mp 0x67540): `0x44c + i`, `i` in 1..16.
    ShellShock,
    Menu,
    Localized,
    Shader,
}

impl CsRange {
    pub const ALL: [CsRange; 11] = [
        CsRange::StatusIcon,
        CsRange::HeadIcon,
        CsRange::Tag,
        CsRange::Model,
        CsRange::SoundAlias,
        CsRange::Effect,
        CsRange::FxTag,
        CsRange::ShellShock,
        CsRange::Menu,
        CsRange::Localized,
        CsRange::Shader,
    ];

    /// Inclusive, in 1.1's layout (the one vcod's server writes).
    pub const fn bounds(self) -> (usize, usize) {
        CS_LAYOUT_V1.bounds(self)
    }
}

/// One protocol's configstring layout: the runtime blocks by [`CsRange`]
/// and the fixed slots that moved with them. Slots 0..21 (serverinfo up to
/// the level start time) are the same in every protocol.
#[derive(Clone, Copy, Debug)]
pub struct CsLayout {
    /// Inclusive bounds, indexed by `CsRange as usize`.
    pub ranges: [(usize, usize); 11],
    /// The mirrored cvars' names, inclusive; the values follow at
    /// `cvar_values`, the same offset into each block.
    pub cvar_names: (usize, usize),
    pub cvar_values: usize,
    /// The first of the 32 hint strings.
    pub hint_strings: usize,
    /// The match timeout's ms left; its text is the slot after.
    pub match_timeout: Option<usize>,
    /// The vote's time; its string, yes and no counts are the three slots
    /// after (docs/research/cod11-votes.md).
    pub vote: usize,
    /// `setCullFog`'s and `setExpFog`'s slot.
    pub fog: usize,
    /// `level.startTime`.
    pub level_start: usize,
    /// `g_motd`, which `SP_worldspawn` writes after the worldspawn's
    /// `message` (4).
    pub motd: usize,
    /// The `\winner\` info string `G_InitGame` resets.
    pub winner: usize,
}

impl CsLayout {
    pub const fn bounds(&self, r: CsRange) -> (usize, usize) {
        self.ranges[r as usize]
    }

    /// The slot one below a 1-based block: `base(r) + index` for index >= 1.
    pub const fn base(&self, r: CsRange) -> usize {
        self.bounds(r).0 - 1
    }

    pub const fn models(&self) -> usize {
        self.base(CsRange::Model)
    }

    pub const fn tags(&self) -> usize {
        self.base(CsRange::Tag)
    }

    pub const fn sounds(&self) -> usize {
        self.base(CsRange::SoundAlias)
    }

    pub const fn effects(&self) -> usize {
        self.base(CsRange::Effect)
    }

    pub const fn fx_tags(&self) -> usize {
        self.base(CsRange::FxTag)
    }

    pub const fn localized(&self) -> usize {
        self.base(CsRange::Localized)
    }

    pub const fn shaders(&self) -> usize {
        self.base(CsRange::Shader)
    }

    /// The configstring of a 1-based block index, `None` past the block.
    pub fn slot(&self, r: CsRange, index: i32) -> Option<usize> {
        let (first, last) = self.bounds(r);
        let i = self.base(r).checked_add(usize::try_from(index).ok()?)?;
        (first..=last).contains(&i).then_some(i)
    }
}

/// Configstring block layout: docs/research/clientstate-wire-format.md,
/// "Configstring map".
pub const CS_LAYOUT_V1: CsLayout = CsLayout {
    ranges: [
        (21, 28),     // StatusIcon
        (29, 43),     // HeadIcon
        (109, 139),   // Tag
        (269, 523),   // Model
        (525, 779),   // SoundAlias
        (781, 843),   // Effect
        (845, 1099),  // FxTag
        (1101, 1115), // ShellShock
        (1180, 1211), // Menu
        (1245, 1499), // Localized
        (1501, 1627), // Shader
    ],
    cvar_names: (140, 203),
    cvar_values: 204,
    hint_strings: 1212,
    match_timeout: None,
    vote: 15,
    fog: CS_FOG_V1,
    level_start: CS_LEVEL_START_TIME,
    motd: 14, // SP_worldspawn 0x61dde (1.5b 0x66a3d)
    winner: 20,
};

/// United Offensive's rebased blocks, each read off its indexer in
/// `game.mp.uo.i386.so` (docs/research/coduo-diff.md, section 7).
pub const CS_LAYOUT_V22: CsLayout = CsLayout {
    ranges: [
        (22, 37),     // StatusIcon, i + 0x16
        (38, 52),     // HeadIcon, i + 0x26
        (118, 148),   // Tag, G_TagIndex 0x75
        (406, 660),   // Model, G_ModelIndex 0x195
        (662, 916),   // SoundAlias, G_SoundAliasIndex 0x295
        (918, 996),   // Effect, G_EffectIndex 0x395
        (998, 1252),  // FxTag, playFxOnTag 0x3e5
        (1254, 1268), // ShellShock, G_ShellShockIndex 0x4e5
        (1333, 1364), // Menu, i + 0x535
        (1398, 1652), // Localized, G_LocalizedStringIndex 0x575
        (1654, 1908), // Shader, G_ShaderIndex 0x675
    ],
    cvar_names: (149, 276),
    cvar_values: 277,
    hint_strings: 1365,        // trigger_use, i + 0x555
    match_timeout: Some(1909), // Cmd_MatchTimeout_f 0x775
    // UO put `setWind` at 12 (0x6ca27) and moved 12..20 up one: `G_setfog`
    // writes 13 (0x4a59a), `SP_worldspawn` the start time 14, `G_InitGame`
    // the winner 21 (coduo-diff.md 7).
    vote: 16,
    fog: 13,
    level_start: 14,
    motd: 15, // SP_worldspawn 0x72c38
    winner: 21,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_bases_sit_one_below_their_ranges() {
        assert_eq!((CS_SOUNDS, CS_LOCALIZED, CS_SHADERS), (524, 1244, 1500));
    }

    #[test]
    fn v1_tables_look_sane() {
        let p = &PROTOCOL_V1;
        assert_eq!(p.entity_fields.len(), 59);
        assert_eq!(p.player_fields.len(), 103);
        // Array order is the wire order; pin both ends against a reordering
        // regeneration.
        assert_eq!(p.entity_fields[0].name, "pos.trTime");
        assert_eq!(p.entity_fields[58].name, "dmgFlags");
        assert_eq!(p.player_fields[0].name, "commandTime");
        assert_eq!(p.player_fields[102].name, "gunfx");
        assert_eq!(p.client_fields.len(), 22);
        assert_eq!(p.client_fields[0].name, "team");
        assert_eq!(p.client_fields[21].name, "name[28]");
        for f in p
            .entity_fields
            .iter()
            .chain(p.player_fields)
            .chain(p.client_fields)
        {
            assert!((-32..=32).contains(&f.bits), "{} bits {}", f.name, f.bits);
        }
        // Offsets identify fields, so a duplicate means a mis-parsed table.
        for table in [p.entity_fields, p.player_fields, p.client_fields] {
            let mut offsets: Vec<u16> = table.iter().map(|f| f.offset).collect();
            offsets.sort_unstable();
            let len = offsets.len();
            offsets.dedup();
            assert_eq!(offsets.len(), len, "duplicate offset in field table");
        }
    }

    #[test]
    fn v6_is_v1_with_a_wider_pm_flags() {
        let (v1, v6) = (&PROTOCOL_V1, &PROTOCOL_V6);
        assert!(std::ptr::eq(v1.entity_fields, v6.entity_fields));
        assert!(std::ptr::eq(v1.client_fields, v6.client_fields));
        assert_eq!(v1.player_fields.len(), v6.player_fields.len());
        for (i, (a, b)) in v1.player_fields.iter().zip(v6.player_fields).enumerate() {
            assert_eq!((a.name, a.offset), (b.name, b.offset), "entry {i}");
            if a.name == "pm_flags" {
                assert_eq!((i, a.bits, b.bits), (12, 19, 21));
            } else {
                assert_eq!(a.bits, b.bits, "{}", a.name);
            }
        }
    }

    #[test]
    fn protocols_by_version_and_short_version() {
        assert_eq!(
            Protocol::by_version(1).map(|p| p.short_version),
            Some("1.1")
        );
        assert_eq!(
            Protocol::by_version(6).map(|p| p.short_version),
            Some("1.5")
        );
        assert!(Protocol::by_version(5).is_none());
        assert_eq!(
            Protocol::by_short_version("1.5").map(|p| p.version),
            Some(6)
        );
        assert_eq!(
            Protocol::by_short_version("1.51").map(|p| p.version),
            Some(22)
        );
        assert!(Protocol::by_short_version("1.4").is_none());
    }

    #[test]
    fn v22_tables_look_sane() {
        let p = &PROTOCOL_V22;
        assert_eq!(p.entity_fields.len(), 60);
        assert_eq!(p.vehicle_fields.len(), 60);
        assert_eq!(p.player_fields.len(), 114);
        assert_eq!(p.entity_fields[0].name, "pos.trTime");
        assert_eq!(p.vehicle_fields[0].name, "pos.trBase[2]");
        assert_eq!(p.player_fields[113].name, "weaponrechamber[3]");
        // `pm_flags` keeps its slot, which the reader's map keys on.
        assert_eq!(p.player_fields[12].name, "pm_flags");
        // The vehicle table is a permutation of the main one.
        let mut slots: Vec<u8> = p.vehicle_field_slots.to_vec();
        for (k, &i) in p.vehicle_field_slots.iter().enumerate() {
            let (a, b) = (&p.vehicle_fields[k], &p.entity_fields[usize::from(i)]);
            assert_eq!((a.name, a.offset), (b.name, b.offset));
        }
        slots.sort_unstable();
        assert_eq!(slots, (0..60).collect::<Vec<u8>>());
        // The names consumers look up are there.
        for name in [
            "eType",
            "eFlags",
            "_union.scale",
            "xmodel",
            "weapon",
            "events[3]",
        ] {
            assert!(p.entity_fields.iter().any(|f| f.name == name), "{name}");
        }
        for name in ["fJumpPeak", "viewangles[0]", "delta_angles[1]", "clientNum"] {
            assert!(p.player_fields.iter().any(|f| f.name == name), "{name}");
        }
    }

    #[test]
    fn v22_means_of_death() {
        let map = PROTOCOL_V22.mod_to_v1;
        // Headshot and melee keep their ids; drown, crush, falling and
        // suicide moved; tank crush and the artillery have none on 1.1.
        for (uo, v1) in [
            (7, 7),
            (8, 8),
            (15, 16),
            (16, 19),
            (20, 21),
            (21, 22),
            (17, -1),
            (13, -1),
        ] {
            assert_eq!(map(uo), v1, "UO MOD {uo}");
        }
    }

    #[test]
    fn v22_configstring_blocks() {
        let cs = &PROTOCOL_V22.cs;
        // The live mp_cassino gamestate: a Sherman's `xmodel` 36 is
        // configstring 441, and the first script menu sits at 1333.
        assert_eq!(cs.slot(CsRange::Model, 36), Some(441));
        assert_eq!(cs.bounds(CsRange::Menu).0, 1333);
        assert_eq!(cs.slot(CsRange::Effect, 80), None);
        assert_eq!(PROTOCOL_V1.cs.slot(CsRange::Model, 1), Some(269));
        // Blocks never overlap.
        let mut r = cs.ranges.to_vec();
        r.sort_unstable();
        assert!(r.windows(2).all(|w| w[0].1 < w[1].0), "{r:?}");
    }

    #[test]
    fn fog_params_parse_live_captures() {
        // mp_pavlov.gsc setCullFog(0, 6000, 0.8, 0.8, 0.8, 0), captured live
        let f = FogParams::parse("0 6000 1 0.8 0.8 0.8 0").unwrap();
        assert_eq!(f.near, 0.0);
        assert_eq!(f.far, 6000.0);
        assert!(f.is_linear());
        assert_eq!(f.color, [0.8, 0.8, 0.8]);
        assert_eq!(f.fade_ms, 0);
        // mp_carentan.gsc setCullFog(0, 16500, 0.7, 0.85, 1.0, 0)
        let f = FogParams::parse("0 16500 1 0.7 0.85 1 0").unwrap();
        assert_eq!(f.far, 16500.0);
        assert_eq!(f.color, [0.7, 0.85, 1.0]);
    }

    #[test]
    fn fog_params_reject_garbage() {
        assert_eq!(FogParams::parse(""), None);
        assert_eq!(FogParams::parse("1 2 3"), None);
        assert_eq!(FogParams::parse("a b c d e f g"), None);
    }
}
