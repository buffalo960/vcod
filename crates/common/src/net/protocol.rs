//! Protocol descriptor: the version-specific wire facts. Protocol 1 is patch
//! 1.1, protocol 6 patch 1.5 (docs/research/cod15-protocol-diff.md); a
//! connection holds one and everything that decodes reads it from there.

/// One netfield. `bits == 0` is a float; a negative `bits` is a plain
/// unsigned width on CoD 1.1 (docs/protocol-1.1.md, "Delta field encoding").
/// `offset` is the engine struct offset, used only as a stable identity.
#[derive(Clone, Copy)]
pub struct NetField {
    pub name: &'static str,
    pub offset: u16,
    pub bits: i32,
}

pub struct Protocol {
    pub version: u32,
    /// The server's `shortversion`, which a connect refused for the wrong
    /// protocol names (`EXE_SERVER_IS_DIFFERENT_VER\x15<this>`).
    pub short_version: &'static str,
    pub max_configstrings: usize,
    pub entity_fields: &'static [NetField],
    pub player_fields: &'static [NetField],
    pub client_fields: &'static [NetField],
    /// Rewrites a decoded `pm_flags` into 1.1's bit layout, the one the rest
    /// of vcod reads ([`crate::net::flags`]).
    pub pm_flags_to_v1: fn(i32) -> i32,
    /// `FS_idPak`'s pak numbers, as its format prints them: `main/pak<n>`,
    /// `mp_pak<n>`, `sp_pak<n>` and `localized_*_pak<n>` are the stock paks
    /// `FS_ComparePaks` never fetches ([`crate::net::download::is_stock_pak`]).
    pub id_pak_numbers: &'static [&'static str],
    /// 1.5's `FS_ComparePaks` also skips a pak whose name holds `_svr_`.
    pub skips_svr_paks: bool,
}

pub const PROTOCOL_V1: Protocol = Protocol {
    version: 1, // reported by cod_lnxded 1.1d getstatus, 2026-08-23
    short_version: "1.1",
    max_configstrings: 2048, // codextended/src/shared.h:141
    entity_fields: crate::net::fields_v1::ENTITY_FIELDS,
    player_fields: crate::net::fields_v1::PLAYER_FIELDS,
    client_fields: crate::net::fields_v1::CLIENT_FIELDS,
    pm_flags_to_v1: |f| f,
    // `%s/pak%d` for 0..8, CoDMP.exe 0x43b6b0.
    id_pak_numbers: &["0", "1", "2", "3", "4", "5", "6", "7", "8"],
    skips_svr_paks: false,
};

/// Patch 1.5: 1.1's wire with a wider, renumbered `pm_flags`
/// (docs/research/cod15-protocol-diff.md).
pub const PROTOCOL_V6: Protocol = Protocol {
    version: 6, // cod_lnxded 1.5b getinfo and getstatus, 2026-10-13
    short_version: "1.5",
    max_configstrings: 2048,
    entity_fields: crate::net::fields_v1::ENTITY_FIELDS,
    player_fields: crate::net::fields_v6::PLAYER_FIELDS,
    client_fields: crate::net::fields_v1::CLIENT_FIELDS,
    pm_flags_to_v1: crate::net::flags::pm_flags_v6_to_v1,
    // `%s/pak%x` for 0..0xf, 1.5 CoDMP.exe 0x43e310; `_svr_` at 0x43e490.
    id_pak_numbers: &[
        "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "a", "b", "c", "d", "e", "f",
    ],
    skips_svr_paks: true,
};

impl Protocol {
    /// Every protocol vcod speaks, the one it tries first leading.
    pub const ALL: [&'static Protocol; 2] = [&PROTOCOL_V1, &PROTOCOL_V6];

    /// The protocol an `infoResponse`'s `protocol` key names.
    pub fn by_version(version: u32) -> Option<&'static Protocol> {
        Self::ALL.into_iter().find(|p| p.version == version)
    }

    /// The protocol a server's `shortversion` belongs to.
    pub fn by_short_version(v: &str) -> Option<&'static Protocol> {
        Self::ALL.into_iter().find(|p| p.short_version == v)
    }
}

// Configstring block layout: docs/research/clientstate-wire-format.md,
// "Configstring map".

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
    Menu,
    Localized,
    Shader,
}

impl CsRange {
    pub const ALL: [CsRange; 10] = [
        CsRange::StatusIcon,
        CsRange::HeadIcon,
        CsRange::Tag,
        CsRange::Model,
        CsRange::SoundAlias,
        CsRange::Effect,
        CsRange::FxTag,
        CsRange::Menu,
        CsRange::Localized,
        CsRange::Shader,
    ];

    /// Inclusive.
    pub const fn bounds(self) -> (usize, usize) {
        match self {
            CsRange::StatusIcon => (21, 28),
            CsRange::HeadIcon => (29, 43),
            CsRange::Tag => (109, 139),
            CsRange::Model => (269, 523),
            CsRange::SoundAlias => (525, 779),
            CsRange::Effect => (781, 843),
            CsRange::FxTag => (845, 1099),
            CsRange::Menu => (1180, 1211),
            CsRange::Localized => (1245, 1499),
            CsRange::Shader => (1501, 1627),
        }
    }
}

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
        assert!(Protocol::by_short_version("1.51").is_none());
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
