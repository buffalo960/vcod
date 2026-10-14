//! Spectator audio. Engine facts are in docs/research/cod11-sound-system.md.

pub mod alias;
pub mod bank;
pub mod cues;
mod handle;
pub mod occlusion;
pub mod spatial;
mod stream;
pub mod voices;

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use glam::Vec3;
use kira::effect::reverb::{ReverbBuilder, ReverbHandle};
use kira::effect::volume_control::{VolumeControlBuilder, VolumeControlHandle};
use kira::sound::PlaybackState;
use kira::sound::streaming::StreamingSoundData;
use kira::track::MainTrackBuilder;
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Mix, StartTime, Tween};

use crate::audio::alias::{AliasRow, AliasTable, Pick};
use crate::audio::bank::SoundBank;
use crate::audio::cues::{Cue, CueCtx, Source};
use crate::audio::handle::{Handle, start_sound};
use crate::audio::spatial::{Listener, amplitude_db};
use crate::audio::voices::{Admitted, ENDLESS_MS, NewVoice, VoiceId, VoiceTable};
use crate::fx::registry::{EV_AMMO_PICKUP, EV_ITEM_PICKUP};
use crate::fx::sim::{FxSound, Rng};
use vcod_common::collision::CollisionWorld;
use vcod_common::net::events::GameEvent;
use vcod_common::pk3::Pk3Fs;
use vcod_common::weapon::WeaponSounds;

/// Kira's per-track sound cap. Retail's pools (research doc, section 2) are
/// enforced per class in `VoiceTable`, which evicts a victim instead; this
/// only backstops kira itself.
const SOUND_CAPACITY: usize = 256;

/// Applied to every alias's random volume; `FUN_0044d5c0` @ `CoDMP.exe
/// 0x44d5c0` (research doc, section 3).
const VOLUME_SCALE: f32 = 0.8;

/// `ENTITYNUM_NONE`, the 0x3ff the cgame starts its shellshock sounds on
/// (`0x3002f2d0`).
const SHOCK_ENTITY: u32 = 1023;

/// The shellshock room. Retail hands the room type to Miles and every
/// voice's reverb levels as dry 1, wet the level (syscall 0xd9, CoDMP.exe
/// `0x44f3c0`, `0x44f430`); vcod's stand-in is one dark reverb on the main
/// track for any room but `generic`, its equal-power mix and a gain set so
/// the dry path stays at 1 (docs/research/cod11-shellshock.md, "Sound").
const ROOM_FEEDBACK: f64 = 0.85;
const ROOM_DAMPING: f64 = 0.9;

pub struct AudioOpts {
    pub enabled: bool,
    /// Master volume, linear 0..1.
    pub volume: f32,
    /// `mss_khz`'s mixer rate in Hz (`bank::mixer_rate`).
    pub rate: u32,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct AudioStats {
    pub voices: usize,
    pub plays: u64,
    /// Cues whose alias is not in the table.
    pub misses: u64,
    /// 3D cues whose emitter was already past `dist_max` at start.
    pub culled: u64,
    /// Read or decode failures; a static file counts once (the bank caches
    /// the failure), a stream once per attempt. Only the tests read it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub decode_failures: u64,
    /// Sounds refused for lack of capacity: every pool slot was held by
    /// voices the newcomer may not steal, or kira itself refused.
    pub drops: u64,
    /// Voices ended early to make room (`FUN_0044c350`, research doc,
    /// section 2).
    pub steals: u64,
    pub step_ms: f32,
}

pub struct AudioSystem {
    /// The connection's `CS_SOUNDS` (`Protocol::cs`), 1.1's until a
    /// gamestate names another.
    cs_sounds: usize,
    /// `None` when disabled or no device opened; cues still resolve, nothing
    /// starts.
    manager: Option<AudioManager<DefaultBackend>>,
    aliases_all: AliasTable,
    /// `aliases_all` filtered by loadspec for the current map; empty before
    /// the first gamestate.
    aliases: AliasTable,
    /// `aliases_all` filtered for the name `menu`: CoDMP.exe's alias set 0,
    /// which `CL_InitUI` loads (0x418f10) and the UI's sounds play from
    /// (docs/research/cod11-front-end.md section 20).
    menu_aliases: AliasTable,
    /// The map `aliases` was filtered for.
    map: Option<String>,
    bank: SoundBank,
    table: VoiceTable,
    handles: HashMap<VoiceId, Handle>,
    /// Line-of-sight gain per spatial voice (`occlusion.rs`); empty without
    /// a collision world.
    occlusion: occlusion::Occlusion,
    listener: Listener,
    /// Entity whose playerState we ride. Its body is never drawn, so `step`
    /// pins its voices to the camera. `u32::MAX` before the first
    /// `set_listener`.
    ps_entity: u32,
    /// Lowercased alias name -> last picked row index, for the no-repeat
    /// rule (research doc, section 1e).
    last_pick: HashMap<String, usize>,
    /// Per 1-based CS7 index, filled lazily; `None` = load failed.
    pub(crate) weapon_sounds: HashMap<i32, Option<WeaponSounds>>,
    rng: Rng,
    master_volume: f32,
    stats: AudioStats,
    /// Streamed read or decode failures; the bank counts the static ones.
    stream_failures: u64,
    missed: HashSet<String>,
    /// Configstring 3 ambient and its voice; the name lets a same-alias
    /// resend be a no-op.
    ambient: Option<(String, VoiceId)>,
    /// Live `es.loopSound` voices, keyed by entity.
    loop_voices: HashMap<u32, LoopVoice>,
    /// What `snd_restart` needs to reopen a stream where it was, per
    /// streamed voice outside the menu set.
    resumable: HashMap<VoiceId, Resumable>,
    /// `musicPlay`'s voice (`o`).
    music: Option<Music>,
    /// `soundFade`'s master fade (`q`).
    fade: SoundFade,
    /// When `step` last advanced the fades; they run on wall time.
    last_step: Option<Instant>,
    /// Aliases already warned about on the `loopSound` path, which retries
    /// every snapshot.
    warned_loops: HashSet<String>,
    /// The shellshock room's stand-in ([`ROOM_FEEDBACK`]) and its current
    /// wet level.
    reverb: Option<(ReverbHandle, VolumeControlHandle)>,
    room_wet: f32,
    /// `shellshock_loop`'s voice and its picked volume, and the end cue's.
    shock_loop: Option<(VoiceId, f32)>,
    shock_end: Option<VoiceId>,
}

/// `musicPlay`'s track. `fade` is `musicStop`'s per-ms change of the voice's
/// volume while it fades out, retail's music slot ramp (CoDMP.exe 0x44e7a0,
/// research doc, section 10, "`musicPlay` and `musicStop`").
#[derive(Clone, Copy, Debug)]
struct Music {
    id: VoiceId,
    fade: Option<f32>,
}

/// The master fade `soundFade` drives (CoDMP.exe 0x8e0790..0x8e0798):
/// `level` scales every voice, moving `rate` per ms toward `target`
/// (research doc, section 10, "`soundFade`").
#[derive(Clone, Copy, Debug, PartialEq)]
struct SoundFade {
    level: f32,
    target: f32,
    rate: f32,
}

impl SoundFade {
    /// `S_Init`'s values (0x450029), which `snd_restart` also lands on.
    const FULL: SoundFade = SoundFade {
        level: 1.0,
        target: 1.0,
        rate: 0.0,
    };
}

/// A playing stream's start parameters, replayed by `restart`.
#[derive(Clone, Debug)]
struct Resumable {
    row: AliasRow,
    source: Source,
    volume: f32,
    pitch: f64,
    /// Where it started, in seconds; the position when no handle reports one.
    from_s: f64,
}

/// One live `es.loopSound`. `looping` is the alias row's flag. Only a
/// looping voice is restarted when it loses its voice; a non-looping row (a
/// data error, retail would machine-gun it) plays once.
#[derive(Clone, Copy, Debug)]
struct LoopVoice {
    idx: i32,
    id: VoiceId,
    looping: bool,
}

impl AudioSystem {
    pub fn new(fs: &Pk3Fs, opts: AudioOpts) -> AudioSystem {
        let mut reverb = None;
        let manager = if opts.enabled {
            let mut main = MainTrackBuilder::new().sound_capacity(SOUND_CAPACITY);
            reverb = Some((
                main.add_effect(
                    ReverbBuilder::new()
                        .feedback(ROOM_FEEDBACK)
                        .damping(ROOM_DAMPING)
                        .mix(Mix::DRY),
                ),
                main.add_effect(VolumeControlBuilder::new(Decibels::IDENTITY)),
            ));
            let settings = AudioManagerSettings {
                main_track_builder: main,
                ..Default::default()
            };
            match AudioManager::<DefaultBackend>::new(settings) {
                Ok(m) => Some(m),
                Err(e) => {
                    log::warn!("audio: cannot open an output device ({e}); running silent");
                    reverb = None;
                    None
                }
            }
        } else {
            None
        };
        let aliases_all = AliasTable::load(fs);
        AudioSystem {
            manager,
            cs_sounds: cues::CS_SOUNDS,
            menu_aliases: aliases_all.for_map("menu"),
            aliases_all,
            aliases: AliasTable::default(),
            map: None,
            bank: SoundBank::at_rate(opts.rate),
            table: VoiceTable::new(),
            handles: HashMap::new(),
            occlusion: occlusion::Occlusion::default(),
            listener: Listener::default(),
            ps_entity: u32::MAX,
            last_pick: HashMap::new(),
            weapon_sounds: HashMap::new(),
            rng: Rng::new(0x5eed_a11a5),
            master_volume: opts.volume.clamp(0.0, 1.0),
            stats: AudioStats::default(),
            stream_failures: 0,
            missed: HashSet::new(),
            ambient: None,
            loop_voices: HashMap::new(),
            resumable: HashMap::new(),
            music: None,
            fade: SoundFade::FULL,
            last_step: None,
            warned_loops: HashSet::new(),
            reverb,
            room_wet: 0.0,
            shock_loop: None,
            shock_end: None,
        }
    }

    pub fn enabled(&self) -> bool {
        self.manager.is_some()
    }

    /// `mss_volume`, linear 0..1; live voices pick it up on their next step.
    pub fn set_master_volume(&mut self, v: f32) {
        self.master_volume = v.clamp(0.0, 1.0);
    }

    /// `snd_restart`'s Miles shutdown and start-up at `rate`: every voice
    /// stops, the sounds decode again for the new mixer rate and the
    /// ambient starts over; entity loops restart with the next snapshot.
    /// Every other stream (dialog, quick chat, `musicPlay`, an in-game
    /// menu's) reopens at its position and current volume, and a `musicStop`
    /// fade goes on; `soundFade`'s master fade is back at full. With no game
    /// up nothing is saved: the main menu stops every sound and replays its
    /// music (research doc, section 10, "Mixer rate and `snd_restart`").
    pub fn restart(&mut self, fs: &Pk3Fs, rate: u32) {
        let ambient = self.ambient.as_ref().map(|(alias, _)| alias.clone());
        let ambient_id = self.ambient.as_ref().map(|(_, id)| *id);
        let loops: HashSet<VoiceId> = self.loop_voices.values().map(|v| v.id).collect();
        let music = self.music;
        let mut saved: Vec<(VoiceId, Resumable)> = self
            .table
            .ids()
            .into_iter()
            .filter(|id| {
                self.in_game()
                    && Some(*id) != ambient_id
                    && !loops.contains(id)
                    && self.shock_loop.is_none_or(|(l, _)| l != *id)
            })
            .filter_map(|id| {
                let mut r = self.resumable.get(&id)?.clone();
                match self.handles.get(&id) {
                    Some(h) if h.state() == PlaybackState::Stopped => return None,
                    Some(h) => r.from_s = h.position(),
                    None => {}
                }
                r.volume = self.table.volume(id)?;
                Some((id, r))
            })
            .collect();
        // Retail's save and restore walk the slots in order.
        saved.sort_by_key(|(id, _)| *id);
        self.table.clear();
        self.stop_all();
        self.fade = SoundFade::FULL;
        self.bank = SoundBank::at_rate(rate);
        self.set_ambient(fs, ambient.as_deref());
        for (old, r) in saved {
            let new = self.start_row(fs, r, 0.0);
            if let Some(m) = music.filter(|m| m.id == old) {
                self.music = new.map(|id| Music { id, ..m });
            }
        }
    }

    /// A game is up: the map set is a level's, not the main menu's.
    fn in_game(&self) -> bool {
        self.map.as_deref().is_some_and(|m| m != "menu")
    }

    /// New map. Refilters the aliases by loadspec and drops everything keyed
    /// to the old table.
    /// Where the connection's sound aliases sit, which United Offensive
    /// moved (docs/research/coduo-diff.md, section 7).
    pub fn set_protocol(&mut self, p: &vcod_common::net::protocol::Protocol) {
        self.cs_sounds = p.cs.sounds();
    }

    pub fn on_gamestate(&mut self, map: &str) {
        self.aliases = self.aliases_all.for_map(map);
        self.map = Some(map.to_string());
        self.table.clear();
        self.stop_all();
        self.last_pick.clear();
        self.weapon_sounds.clear();
        log::info!("audio: {} aliases apply on {map}", self.aliases.len());
    }

    /// Stops every voice, the ambient and the entity loops included:
    /// `CG_MapRestart`'s syscall 0xd3 (`S_StopAllSounds`). The caller
    /// restarts the ambient from CS 3; a loop restarts with the next
    /// snapshot that carries it.
    pub fn stop_all(&mut self) {
        // A dropped kira handle keeps playing.
        for (_, mut h) in self.handles.drain() {
            h.stop();
        }
        self.ambient = None;
        self.loop_voices.clear();
        self.resumable.clear();
        self.music = None;
        self.shock_loop = None;
        self.shock_end = None;
        self.set_room_wet(0.0);
        self.table.set_channel_volumes([1.0; 10]);
        self.stats.voices = 0;
    }

    /// The room's wet level over a dry path held at 1: kira's mix crossfades
    /// `sqrt(1 - m)` dry against `sqrt(m)` wet, so `m = w^2 / (1 + w^2)`
    /// under a gain of `sqrt(1 + w^2)`.
    fn set_room_wet(&mut self, wet: f32) {
        if wet == self.room_wet {
            return;
        }
        self.room_wet = wet;
        if let Some((reverb, gain)) = &mut self.reverb {
            let w2 = wet * wet;
            let tween = Tween {
                duration: Duration::from_millis(30),
                ..Default::default()
            };
            reverb.set_mix(Mix(w2 / (1.0 + w2)), tween);
            gain.set_volume(Decibels(10.0 * (1.0 + w2).log10()), tween);
        }
    }

    /// A frame of the shellshock's sound half (`play::shock::ShockSound`):
    /// the channel mix, the room, `shellshock_loop` blended toward its
    /// silent twin while the blend lasts, and the end or abort cue.
    pub fn set_shellshock(&mut self, fs: &Pk3Fs, s: &crate::play::shock::ShockSound) {
        self.table.set_channel_volumes(s.volumes);
        let wet = if s.room.eq_ignore_ascii_case("generic") {
            0.0
        } else {
            s.wet
        };
        self.set_room_wet(wet);
        let shock_cue = |alias: &str, pos: Vec3| Cue {
            alias: alias.to_string(),
            source: Source::Entity {
                num: SHOCK_ENTITY,
                pos,
            },
            delay_s: 0.0,
        };
        match s.loop_blend {
            Some(blend) => {
                let live = self
                    .shock_loop
                    .filter(|(id, _)| self.table.looping(*id).is_some());
                let started = live.or_else(|| {
                    let id = self.play(fs, shock_cue("shellshock_loop", self.listener.pos))?;
                    Some((id, self.table.volume(id)?))
                });
                // The silent twin's volume is 0: the blend scales the row's.
                if let Some((id, base)) = started {
                    self.table.set_volume(id, base * (1.0 - blend));
                }
                self.shock_loop = started;
            }
            None => {
                if let Some((id, _)) = self.shock_loop.take() {
                    self.stop_voice(id);
                }
            }
        }
        match s.cue {
            // `null.wav` on the end's channel and entity replaces it.
            Some(("shellshock_end_abort", _)) => {
                if let Some(id) = self.shock_end.take() {
                    self.stop_voice(id);
                }
            }
            Some((alias, ms)) => {
                if let Some(id) = self.shock_end.take() {
                    self.stop_voice(id);
                }
                let cue = shock_cue(alias, self.listener.pos);
                self.shock_end = self.play_from(fs, cue, ms.max(0) as f32 / 1000.0);
            }
            None => {}
        }
    }

    /// A download reopened the search path: reload the alias csvs, drop every
    /// decoded sound, and redo the current map's `on_gamestate` against the
    /// new table.
    pub fn reopen(&mut self, fs: &Pk3Fs) {
        self.aliases_all = AliasTable::load(fs);
        self.menu_aliases = self.aliases_all.for_map("menu");
        self.bank = SoundBank::at_rate(self.bank.rate);
        self.missed.clear();
        self.warned_loops.clear();
        match self.map.clone() {
            Some(map) => self.on_gamestate(&map),
            None => self.weapon_sounds.clear(),
        }
    }

    pub fn set_listener(
        &mut self,
        pos: Vec3,
        forward: Vec3,
        right: Vec3,
        up: Vec3,
        ps_entity: u32,
    ) {
        self.listener = Listener {
            pos,
            forward,
            right,
            up,
        };
        self.ps_entity = ps_entity;
    }

    /// Loads the weapon file behind CS7 index `index` once; `None` cached on
    /// failure.
    pub(crate) fn ensure_weapon(&mut self, fs: &Pk3Fs, configstrings: &[String], index: i32) {
        if index <= 0 || self.weapon_sounds.contains_key(&index) {
            return;
        }
        let names = crate::entities::split_weapon_list(
            configstrings.get(7).map(String::as_str).unwrap_or(""),
        );
        let sounds = crate::entities::weapon_name_for_index(&names, index).and_then(|name| {
            match vcod_common::weapon::load(fs, name) {
                Ok(def) => Some(def.sounds),
                Err(e) => {
                    log::warn!("audio: weapon {name}: {e:#}");
                    None
                }
            }
        });
        self.weapon_sounds.insert(index, sounds);
    }

    pub fn on_game_event(
        &mut self,
        fs: &Pk3Fs,
        ev: &GameEvent,
        configstrings: &[String],
        muzzles: &HashMap<u32, (Vec3, Vec3)>,
    ) {
        self.ensure_weapon(fs, configstrings, ev.weapon);
        // `parm` is a CS7 weapon index on the pickup events alone.
        if matches!(ev.event, EV_ITEM_PICKUP | EV_AMMO_PICKUP) {
            self.ensure_weapon(fs, configstrings, ev.parm);
        }
        let cues = {
            let ctx = CueCtx {
                configstrings,
                sounds: self.cs_sounds,
                weapon_sounds: &self.weapon_sounds,
                muzzles,
                listener_pos: self.listener.pos,
                ps_entity: self.ps_entity,
            };
            cues::resolve(ev, &ctx)
        };
        for c in cues {
            self.play(fs, c);
        }
    }

    /// `None` for an unknown alias (a miss) and for a `null.wav` row (not a
    /// miss).
    fn pick_row(&mut self, cue: &Cue, menu: bool) -> Option<AliasRow> {
        let roll = self.rng.next_f32();
        let key = cue.alias.to_ascii_lowercase();
        let last = self.last_pick.get(&key).copied();
        let set = if menu {
            &self.menu_aliases
        } else {
            &self.aliases
        };
        let (pick, index) = set.pick(&cue.alias, roll, last);
        let row = match pick {
            Pick::Row(r) => Some(r.clone()),
            Pick::Silent => None,
            Pick::Unknown => {
                self.stats.misses += 1;
                if self.missed.insert(key) {
                    log::debug!("audio: unknown sound alias {:?}", cue.alias);
                }
                return None;
            }
        };
        if let Some(i) = index {
            self.last_pick.insert(key, i);
        }
        row
    }

    /// Starts `cue`. A disabled system still resolves and allocates the voice
    /// (counters and loop bookkeeping are table state); `step` reaps the
    /// handleless voice next frame.
    pub fn play(&mut self, fs: &Pk3Fs, cue: Cue) -> Option<VoiceId> {
        self.play_inner(fs, cue, true, false)
    }

    /// A `Sound` emitter inside an effect is a world-point cue.
    pub fn play_fx(&mut self, fs: &Pk3Fs, sounds: Vec<FxSound>) {
        for s in sounds {
            self.play(
                fs,
                Cue {
                    alias: s.alias,
                    source: Source::Point(s.pos),
                    delay_s: s.delay_s,
                },
            );
        }
    }

    /// A 2D cue started `from_s` seconds in: the shellshock end cue, which
    /// starts as far in as the frame came late (syscall 0xc2's last
    /// argument).
    fn play_from(&mut self, fs: &Pk3Fs, cue: Cue, from_s: f32) -> Option<VoiceId> {
        let row = self.pick_row(&cue, false)?;
        let volume = self.rng.range(row.vol.0, row.vol.1) * VOLUME_SCALE;
        let pitch = self.rng.range(row.pitch.0, row.pitch.1) as f64;
        self.start_row(
            fs,
            Resumable {
                row,
                source: cue.source,
                volume,
                pitch,
                from_s: f64::from(from_s),
            },
            0.0,
        )
    }

    /// `count_cull` is false on the loop-sound path, which retries every
    /// snapshot and would otherwise tick `culled` per snapshot.
    /// `menu` picks from the menu set.
    fn play_inner(
        &mut self,
        fs: &Pk3Fs,
        cue: Cue,
        count_cull: bool,
        menu: bool,
    ) -> Option<VoiceId> {
        let emitter = match cue.source {
            Source::Point(p) => p,
            Source::Entity { pos, .. } => pos,
        };
        // A NaN origin passes every distance test, and a NaN pan poisons
        // kira's mix for good.
        if !emitter.is_finite() {
            log::debug!("audio: {:?} at a non-finite position {emitter}", cue.alias);
            if count_cull {
                self.stats.culled += 1;
            }
            return None;
        }
        let row = self.pick_row(&cue, menu)?;
        // Over a game the cgame's end-of-frame reaper (syscall 0xd2,
        // 0x44e600) ends a looping voice nobody refreshes, so a menu's
        // looping music stops the frame it starts.
        if menu && row.looping && self.in_game() {
            return None;
        }
        // Sampled before the device check so the rng path is device
        // independent.
        let volume = self.rng.range(row.vol.0, row.vol.1) * VOLUME_SCALE;
        let pitch = self.rng.range(row.pitch.0, row.pitch.1) as f64;
        // Start-time cull, `FUN_0044d670` @ `CoDMP.exe 0x44d670` (research
        // doc, section 3). There is no later cull.
        if row.channel.is_spatial() && emitter.distance(self.listener.pos) > row.dist.1 {
            if count_cull {
                self.stats.culled += 1;
            }
            return None;
        }
        self.start_row(
            fs,
            Resumable {
                row,
                source: cue.source,
                volume,
                pitch,
                from_s: 0.0,
            },
            cue.delay_s,
        )
    }

    /// Admits and starts a picked row after the cull, `r.from_s` seconds in.
    /// Every streamed voice gets a `resumable` entry, kept on a silent
    /// system too.
    fn start_row(&mut self, fs: &Pk3Fs, r: Resumable, delay_s: f32) -> Option<VoiceId> {
        let Resumable {
            ref row,
            source,
            volume,
            pitch,
            from_s,
        } = r;
        let spatial = row.channel.is_spatial();
        let master = self.master();
        // Static files are decoded before admission so a failed load never
        // steals a pool slot; their length feeds the steal tiebreak.
        // Streams have no length before playback (research doc approximation,
        // section 10).
        let static_data = (!row.streamed)
            .then(|| self.bank.get(fs, &row.file))
            .flatten();
        if !row.streamed && static_data.is_none() {
            // The bank already counted and warned about the failure.
            return None;
        }
        let duration_ms = static_data
            .as_ref()
            .map(|d| u32::try_from(d.duration().as_millis()).unwrap_or(ENDLESS_MS))
            .unwrap_or(ENDLESS_MS);
        let (id, replaced, stolen) = match self.table.add(
            NewVoice {
                source,
                channel: row.channel,
                volume,
                dist: row.dist,
                master_slave: row.master_slave,
                spatial,
                looping: row.looping,
                streamed: row.streamed,
                duration_ms,
                protected: matches!(
                    source,
                    Source::Entity {
                        num: cues::AMBIENT_ENTITY | cues::MUSIC_ENTITY,
                        ..
                    }
                ),
            },
            |id| {
                self.handles
                    .get(&id)
                    .is_some_and(|h| h.state() == PlaybackState::Stopped)
            },
        ) {
            Err(_) => {
                // Every slot was held by voices this sound may not steal;
                // retail's allocator comes up empty in the same case.
                self.stats.drops += 1;
                return None;
            }
            Ok(Admitted::Started {
                id,
                replaced,
                stolen,
            }) => (id, replaced, stolen),
        };
        self.stats.steals += stolen.len() as u64;
        // Replacement and stealing already removed the victims from the
        // table; only their kira handles remain.
        for r in replaced.iter().chain(stolen.iter()) {
            if let Some(mut h) = self.handles.remove(r) {
                h.stop();
            }
        }
        // Initial gain/pan so the first audio chunk is already placed. The
        // occlusion map is empty here; a voice born behind a wall settles
        // over the next few frames like any other change.
        let (db, pan) = self
            .table
            .update(&self.listener, &HashMap::new(), &HashMap::new(), master)
            .into_iter()
            .find(|u| u.id == id)
            .map(|u| (amplitude_db(u.gain), u.pan))
            // Unreachable; silence if it ever is.
            .unwrap_or((-60.0, 0.0));
        // `from_secs_f32` panics on a negative or NaN delay, and effect files
        // can carry one.
        let start = match Duration::try_from_secs_f32(delay_s) {
            Ok(d) if !d.is_zero() => StartTime::Delayed(d),
            _ => StartTime::Immediate,
        };

        // No device. The voice stays in the table (ambient and loop
        // bookkeeping are table state) and `step` reaps it next frame.
        // `manager` is borrowed mutably across the field accesses below.
        if row.streamed {
            self.resumable.insert(id, r.clone());
        }
        let Some(manager) = self.manager.as_mut() else {
            return Some(id);
        };
        let started = if row.streamed {
            // `read_stream` already warned on a missing file.
            match SoundBank::read_stream(fs, &row.file).map(|b| stream::open(b, self.bank.rate)) {
                Some(Ok(decoder)) => Some(start_sound!(
                    manager,
                    StreamingSoundData::from_decoder(decoder),
                    Stream,
                    db,
                    pan,
                    pitch,
                    start,
                    from_s,
                    row.looping
                )),
                Some(Err(e)) => {
                    log::warn!("audio: stream {}: decode failed: {e}", row.file);
                    self.stream_failures += 1;
                    None
                }
                None => {
                    self.stream_failures += 1;
                    None
                }
            }
        } else {
            // The bank already counted and warned about a failure.
            static_data.map(|data| {
                start_sound!(
                    manager,
                    data,
                    Static,
                    db,
                    pan,
                    pitch,
                    start,
                    from_s,
                    row.looping
                )
            })
        };

        match started {
            Some(Ok(h)) => {
                self.handles.insert(id, h);
                self.stats.plays += 1;
                Some(id)
            }
            Some(Err(e)) => {
                log::debug!("audio: play {}: {e}", row.name);
                self.stats.drops += 1;
                self.table.remove(id);
                self.resumable.remove(&id);
                None
            }
            None => {
                self.table.remove(id);
                self.resumable.remove(&id);
                None
            }
        }
    }

    fn stop_voice(&mut self, id: VoiceId) {
        self.table.remove(id);
        self.resumable.remove(&id);
        if let Some(mut h) = self.handles.remove(&id) {
            h.stop();
        }
    }

    /// Configstring 3's `n` key, `ambientPlay`'s wire form (research doc,
    /// section 9). The row is `local`, `streamed`, `looping`, so `play` makes
    /// it a 2D loop. A round restart resends CS 3 with the same alias and a
    /// new `t`; an unchanged alias with a live voice is a no-op, a lost voice
    /// is restarted. It sits on [`cues::AMBIENT_ENTITY`] so no cue can replace
    /// it (research doc, "vcod divergences from retail"). The `t` crossfade
    /// deadline is ignored; vcod cuts over.
    pub fn set_ambient(&mut self, fs: &Pk3Fs, alias: Option<&str>) {
        let alias = alias.unwrap_or("").trim();
        if self
            .ambient
            .as_ref()
            .is_some_and(|(cur, id)| cur == alias && self.table.looping(*id).is_some())
        {
            return;
        }
        if let Some((_, id)) = self.ambient.take() {
            self.stop_voice(id);
        }
        if alias.is_empty() {
            return;
        }
        let cue = Cue {
            alias: alias.to_string(),
            // The row is 2D; the position is never read.
            source: Source::Entity {
                num: cues::AMBIENT_ENTITY,
                pos: self.listener.pos,
            },
            delay_s: 0.0,
        };
        if let Some(id) = self.play(fs, cue) {
            log::info!("audio: ambient {alias}");
            self.ambient = Some((alias.to_string(), id));
        }
    }

    /// This snapshot's `es.loopSound` per entity as `(CS_SOUNDS index,
    /// origin)` (research doc, section 9). Called per snapshot; `step` keeps
    /// the voices moving. An entity absent from `loops` (or at index 0) has
    /// its loop stopped; live, loops end by the entity leaving the snapshot,
    /// never by `loopSound -> 0`.
    pub fn set_loop_sounds(
        &mut self,
        fs: &Pk3Fs,
        configstrings: &[String],
        loops: &HashMap<u32, (i32, Vec3)>,
    ) {
        let gone: Vec<u32> = self
            .loop_voices
            .iter()
            .filter(|(ent, v)| !matches!(loops.get(ent), Some(&(idx, _)) if idx == v.idx))
            .map(|(ent, _)| *ent)
            .collect();
        for ent in gone {
            if let Some(v) = self.loop_voices.remove(&ent) {
                log::debug!("audio: loop off entity {ent}");
                self.stop_voice(v.id);
            }
        }

        for (&ent, &(idx, pos)) in loops {
            if idx <= 0 {
                continue;
            }
            if let Some(&LoopVoice { id, looping, .. }) = self.loop_voices.get(&ent) {
                // Still live, or a non-looping row already played once.
                if !looping || self.table.looping(id).is_some() {
                    continue;
                }
                self.loop_voices.remove(&ent);
            }
            // Resolved per snapshot; the `CS_SOUNDS` block fills lazily
            // (research doc, section 9).
            let alias = match configstrings.get(self.cs_sounds + idx as usize) {
                Some(a) if !a.is_empty() => a.clone(),
                _ => continue,
            };
            // Dropped here rather than counted as a miss every snapshot.
            if self.aliases.get(&alias).is_none() {
                if self.warned_loops.insert(alias.to_ascii_lowercase()) {
                    log::warn!("audio: loopSound alias {alias:?} is not in the alias table");
                }
                continue;
            }
            let cue = Cue {
                alias: alias.clone(),
                source: Source::Entity { num: ent, pos },
                delay_s: 0.0,
            };
            // Silent pick, cull or decode failure; the next snapshot retries.
            let Some(id) = self.play_inner(fs, cue, false, false) else {
                continue;
            };
            log::debug!("audio: loop {alias:?} on entity {ent} at {pos}");
            let looping = self.table.looping(id).unwrap_or(false);
            if !looping && self.warned_loops.insert(alias.to_ascii_lowercase()) {
                log::warn!(
                    "audio: loopSound alias {alias:?} is not a looping row; playing it once"
                );
            }
            self.loop_voices.insert(ent, LoopVoice { idx, id, looping });
        }
    }

    /// `alias` on the viewer, the way `CG_LocalSound` and the chat and game
    /// message beeps play.
    /// Whether the menu set has `alias` (UI syscall 0x26).
    pub fn has_menu_alias(&self, alias: &str) -> bool {
        self.menu_aliases.get(alias).is_some()
    }

    /// A menu's sound: `alias` from the menu set on the listener, as UI
    /// syscall 0x27 (CoDMP.exe 0x44d9c0) plays it.
    pub fn play_menu(&mut self, fs: &Pk3Fs, alias: &str) -> Option<VoiceId> {
        let cue = Cue {
            alias: alias.to_string(),
            source: Source::Entity {
                num: self.ps_entity,
                pos: self.listener.pos,
            },
            delay_s: 0.0,
        };
        self.play_inner(fs, cue, true, true)
    }

    pub fn play_local(&mut self, fs: &Pk3Fs, alias: &str) -> Option<VoiceId> {
        let cue = Cue {
            alias: alias.to_string(),
            source: Source::Entity {
                num: self.ps_entity,
                pos: self.listener.pos,
            },
            delay_s: 0.0,
        };
        self.play(fs, cue)
    }

    /// A server command the net layer did not consume; returns whether it was
    /// a sound command. `s <idx>` is `playLocalSound` (research doc, section
    /// 9). The alias is resolved at receive time since the `CS_SOUNDS` slot
    /// is often created by the `d` update just before it.
    pub fn on_server_command(
        &mut self,
        fs: &Pk3Fs,
        tokens: &[String],
        configstrings: &[String],
    ) -> bool {
        let Some(cmd) = tokens.first() else {
            return false;
        };
        match cmd.as_str() {
            // United Offensive's `playLocalAnnouncerSound` sends `S`, read
            // the same way (coduo-diff.md, section 8).
            "s" | "S" => {
                let idx = tokens
                    .get(1)
                    .and_then(|t| t.parse::<usize>().ok())
                    .filter(|i| (1..=255).contains(i));
                let Some(idx) = idx else {
                    log::debug!("audio: malformed playLocalSound command {tokens:?}");
                    return true;
                };
                match configstrings.get(self.cs_sounds + idx) {
                    Some(a) if !a.is_empty() => {
                        log::debug!("audio: playLocalSound {idx} = {a:?}");
                        self.play_local(fs, a);
                    }
                    _ => log::debug!("audio: playLocalSound {idx}: no alias in that configstring"),
                }
                true
            }
            "o" => {
                if let Some(alias) = tokens.get(1) {
                    self.music_play(fs, alias);
                }
                true
            }
            "p" => {
                let fade_ms = tokens.get(1).and_then(|t| t.parse::<i64>().ok());
                self.music_stop(fade_ms.unwrap_or(0));
                true
            }
            "q" => {
                // `atof` and `atol`: an unparsable word reads as 0.
                let target = tokens.get(1).and_then(|t| t.parse::<f32>().ok());
                let ms = tokens.get(2).and_then(|t| t.parse::<i32>().ok());
                self.sound_fade(target.unwrap_or(0.0), ms.unwrap_or(0));
                true
            }
            // Quick chat (`j`/`k`/`l`) is handled by `quick_chat::QuickChat`
            // before commands reach here.
            _ => false,
        }
    }

    /// `musicPlay`'s `o <alias>` (cgame 0x3002e0d0 case `o`, syscall 0xd4 =
    /// CoDMP.exe 0x44dc40): starts the track only while the music slot is
    /// idle, so a second `o` during a track is dropped (research doc,
    /// section 10, "`musicPlay` and `musicStop`").
    fn music_play(&mut self, fs: &Pk3Fs, alias: &str) {
        if self
            .music
            .is_some_and(|m| self.table.looping(m.id).is_some())
        {
            return;
        }
        self.music = None;
        if let Some(row) = self.aliases.get(alias).and_then(|rows| rows.first()) {
            if !row.streamed {
                log::warn!(
                    "audio: alias {} sound {} played as an ambient / music track is not streamed",
                    row.name,
                    row.file
                );
            }
            if row.channel.is_spatial() {
                log::warn!(
                    "audio: alias {} sound {} played as an ambient / music track uses a 3D channel",
                    row.name,
                    row.file
                );
            }
        }
        let cue = Cue {
            alias: alias.to_string(),
            source: Source::Entity {
                num: cues::MUSIC_ENTITY,
                pos: self.listener.pos,
            },
            delay_s: 0.0,
        };
        self.music = self.play(fs, cue).map(|id| Music { id, fade: None });
    }

    /// `musicStop`'s `p <fade ms>` (syscall 0xd5 = CoDMP.exe 0x44db90): 0
    /// closes the track at once, more ramps its volume to 0 over that many
    /// ms, and a second `p` with a fade leaves a fade in progress alone. A
    /// negative fade, which the server refuses to send, stops at once.
    fn music_stop(&mut self, fade_ms: i64) {
        let Some(m) = self.music else {
            return;
        };
        if fade_ms <= 0 {
            self.stop_voice(m.id);
            self.music = None;
            return;
        }
        if m.fade.is_some() {
            return;
        }
        let volume = self.table.volume(m.id).unwrap_or(0.0);
        self.music = Some(Music {
            fade: Some(-volume / fade_ms as f32),
            ..m
        });
    }

    /// `soundFade`'s `q <volume> <ms>` (cgame 0x30031fc0, syscall 0xd7 =
    /// CoDMP.exe 0x44e0f0): the master fade heads for `target` over `ms`.
    /// A time of 0 lands on the next step, and a target of 0 then stops
    /// every sound at once (research doc, section 10, "`soundFade`").
    fn sound_fade(&mut self, target: f32, ms: i32) {
        let delta = target - self.fade.level;
        self.fade.target = target;
        if ms != 0 {
            self.fade.rate = delta / ms as f32;
            return;
        }
        self.fade.rate = delta;
        if target == 0.0 {
            self.stop_everything();
        }
    }

    /// `S_StopAllSounds(0)` (0x44fa40): every voice ends, the ambient and
    /// the music with it.
    fn stop_everything(&mut self) {
        self.table.clear();
        self.stop_all();
    }

    /// Linear master gain: `mss_volume` times the `soundFade` level, as
    /// retail's 0x8e078c is `s_volume` times 0x8e0790 (0x44ece8).
    fn master(&self) -> f32 {
        self.master_volume * self.fade.level
    }

    /// Advances the master fade and a `musicStop` fade by `ms` of wall
    /// time, as `S_Update` does before it sets any voice's volume
    /// (0x44e890 for the master, 0x44e7a0 for the music slot). The master
    /// landing on 0 stops every sound; the music reaching 0 closes it.
    fn advance_fades(&mut self, ms: f32) {
        let f = &mut self.fade;
        if f.rate != 0.0 {
            f.level += f.rate * ms;
            let past = if f.rate >= 0.0 {
                f.level > f.target
            } else {
                f.level < f.target
            };
            if past {
                f.level = f.target;
                f.rate = 0.0;
            }
            if f.level == 0.0 && f.rate == 0.0 {
                self.stop_everything();
            }
        }
        if let Some(Music {
            id,
            fade: Some(rate),
        }) = self.music
        {
            let volume = self.table.volume(id).unwrap_or(0.0) + rate * ms;
            if volume < 0.0 {
                self.stop_voice(id);
                self.music = None;
            } else {
                self.table.set_volume(id, volume);
            }
        }
    }

    /// Once per frame, after the event drain. `world` is the collision world
    /// in game mode and `None` where there is none (fly mode), which disables
    /// occlusion.
    pub fn step(&mut self, entity_pos: &HashMap<u32, Vec3>, world: Option<&CollisionWorld>) {
        let t0 = Instant::now();
        let ms = self
            .last_step
            .map_or(0.0, |t| t0.duration_since(t).as_secs_f32() * 1000.0);
        self.last_step = Some(t0);
        self.advance_fades(ms);
        let master = self.master();
        let handles = &mut self.handles;
        self.table.retain(|id| match handles.get(&id) {
            Some(h) => h.state() != PlaybackState::Stopped,
            None => false,
        });
        let live: HashSet<VoiceId> = self.table.ids().into_iter().collect();
        // A dropped kira handle keeps playing.
        handles.retain(|id, h| {
            let keep = live.contains(id);
            if !keep {
                h.stop();
            }
            keep
        });
        self.resumable.retain(|id, _| live.contains(id));

        if !self.table.is_empty() {
            // The ridden entity has no drawn body, so `entity_pos` never
            // carries it.
            let mut positions = entity_pos.clone();
            positions.insert(self.ps_entity, self.listener.pos);
            // A 3D music row stays on the listener, as a 3D ambient would.
            positions.insert(cues::MUSIC_ENTITY, self.listener.pos);
            let occluded = match world {
                Some(w) => {
                    self.occlusion
                        .compute(&self.table.spatial_positions(), self.listener.pos, w)
                }
                None => {
                    self.occlusion.reset();
                    HashMap::new()
                }
            };
            for u in self
                .table
                .update(&self.listener, &positions, &occluded, master)
            {
                if let Some(h) = handles.get_mut(&u.id) {
                    h.set_volume(amplitude_db(u.gain));
                    h.set_panning(u.pan);
                }
            }
        }
        self.stats.voices = self.table.len();
        self.stats.step_ms = t0.elapsed().as_secs_f32() * 1000.0;
    }

    pub fn stats(&self) -> AudioStats {
        AudioStats {
            decode_failures: self.bank.failures + self.stream_failures,
            ..self.stats
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_system_is_inert_but_counts_resolution() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = AudioSystem::new(
            &fs,
            AudioOpts {
                enabled: false,
                volume: 1.0,
                rate: 0,
            },
        );
        assert!(!a.enabled());
        a.on_gamestate("mp_carentan");
        a.play(
            &fs,
            Cue {
                alias: "weap_thompson_fire".into(),
                source: Source::Point(Vec3::ZERO),
                delay_s: 0.0,
            },
        );
        a.play(
            &fs,
            Cue {
                alias: "no_such_alias".into(),
                source: Source::Point(Vec3::ZERO),
                delay_s: 0.0,
            },
        );
        // Out of range, so culled rather than missed.
        a.play(
            &fs,
            Cue {
                alias: "step_run_dirt".into(),
                source: Source::Point(Vec3::splat(100_000.0)),
                delay_s: 0.0,
            },
        );
        a.step(&HashMap::new(), None);
        let s = a.stats();
        assert_eq!((s.plays, s.misses, s.voices, s.culled), (0, 1, 0, 1));
    }

    #[test]
    fn on_gamestate_filters_the_full_table() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = AudioSystem::new(
            &fs,
            AudioOpts {
                enabled: false,
                volume: 1.0,
                rate: 0,
            },
        );
        assert!(a.aliases.is_empty());
        a.on_gamestate("mp_carentan");
        assert!(a.aliases.len() > 1000, "aliases: {}", a.aliases.len());
    }

    #[test]
    fn menu_sounds_play_from_the_menu_set_over_a_game() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let opts = AudioOpts {
            enabled: false,
            volume: 1.0,
            rate: 0,
        };
        let mut a = AudioSystem::new(&fs, opts);
        a.on_gamestate("mp_carentan");
        // The `menu` loadspec rows are not the map's.
        assert!(a.aliases.get("mouse_over").is_none());
        assert!(a.has_menu_alias("mouse_over"));
        assert!(a.play_menu(&fs, "mouse_over").is_some());
        assert!(a.play_menu(&fs, "mouse_click").is_some());
        assert_eq!(a.stats().misses, 0);
        // The looping menu music is reaped over a game, not in the front end.
        assert!(a.play_menu(&fs, "music_mainmenu").is_none());
        a.on_gamestate("menu");
        assert!(a.play_menu(&fs, "music_mainmenu").is_some());
    }

    #[test]
    fn repeated_plays_thread_the_no_repeat_index() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = AudioSystem::new(
            &fs,
            AudioOpts {
                enabled: false,
                volume: 1.0,
                rate: 0,
            },
        );
        a.on_gamestate("mp_carentan");
        // At least 3 variants, so the no-repeat rule applies.
        assert!(a.aliases.get("step_run_dirt").unwrap().len() >= 3);
        let mut picks = Vec::new();
        for _ in 0..3 {
            a.play(
                &fs,
                Cue {
                    alias: "step_run_dirt".into(),
                    source: Source::Point(Vec3::ZERO),
                    delay_s: 0.0,
                },
            );
            picks.push(a.last_pick["step_run_dirt"]);
        }
        assert_ne!(picks[0], picks[1], "picks: {picks:?}");
        assert_ne!(picks[1], picks[2], "picks: {picks:?}");
        let s = a.stats();
        assert_eq!((s.plays, s.misses), (0, 0));
    }

    #[test]
    fn game_event_resolves_a_weapon_fire_cue() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = AudioSystem::new(
            &fs,
            AudioOpts {
                enabled: false,
                volume: 1.0,
                rate: 0,
            },
        );
        a.on_gamestate("mp_carentan");
        a.set_listener(Vec3::ZERO, Vec3::X, -Vec3::Y, Vec3::Z, 5);
        let mut cs = vec![String::new(); 8];
        cs[7] = "kar98k_mp thompson_mp".to_string();
        let ev = GameEvent {
            event: crate::fx::registry::EV_FIRE_WEAPON,
            // Not a pickup, so `parm` must not be loaded as a weapon index.
            parm: 1,
            entity_num: 5,
            client_num: 5,
            weapon: 2,
            surf_type: 0,
            pos: [0.0; 3],
            dir: [0.0; 3],
            scale: 0,
            other_entity_num: u32::MAX,
            attacker_entity_num: -1,
        };
        a.on_game_event(&fs, &ev, &cs, &HashMap::new());
        assert_eq!(
            a.weapon_sounds[&2].as_ref().unwrap().fire.as_deref(),
            Some("weap_thompson_fire")
        );
        assert!(a.last_pick.contains_key("weap_thompson_fire"));
        assert!(!a.weapon_sounds.contains_key(&1));
        let s = a.stats();
        assert_eq!(
            (s.plays, s.misses, s.decode_failures, s.culled),
            (0, 0, 0, 0)
        );
    }

    #[test]
    fn loop_sounds_start_stop_with_entities() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = AudioSystem::new(
            &fs,
            AudioOpts {
                enabled: false,
                volume: 1.0,
                rate: 0,
            },
        );
        a.on_gamestate("mp_carentan");
        let mut cs = vec![String::new(); 800];
        cs[cues::CS_SOUNDS + 3] = "bomb_tick".to_string();
        let mut loops = HashMap::new();
        loops.insert(12u32, (3, Vec3::new(1.0, 2.0, 3.0)));
        a.set_loop_sounds(&fs, &cs, &loops);
        assert_eq!(a.loop_voices.len(), 1);
        let id = a.loop_voices[&12].id;
        a.set_loop_sounds(&fs, &cs, &loops); // same state, no second voice
        assert_eq!(a.loop_voices.len(), 1);
        assert_eq!(a.loop_voices[&12].id, id);
        assert_eq!(a.table.len(), 1);

        // A looping voice that lost its table entry is restarted next
        // snapshot.
        a.step(&HashMap::new(), None);
        assert_eq!(a.table.len(), 0);
        a.set_loop_sounds(&fs, &cs, &loops);
        assert_eq!(a.loop_voices.len(), 1);
        assert_ne!(a.loop_voices[&12].id, id);
        assert_eq!(a.table.len(), 1);

        // The entity leaves the snapshot; live stops arrive this way
        // (research doc, section 9).
        a.set_loop_sounds(&fs, &cs, &HashMap::new());
        assert!(a.loop_voices.is_empty());
        assert_eq!(a.table.len(), 0);
    }

    #[test]
    fn non_looping_loop_alias_plays_once_and_is_not_restarted() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = AudioSystem::new(
            &fs,
            AudioOpts {
                enabled: false,
                volume: 1.0,
                rate: 0,
            },
        );
        a.on_gamestate("mp_carentan");
        let mut cs = vec![String::new(); 800];
        // `MP_bomb_plant` is `all_mp`, one-shot, channel `auto`.
        cs[cues::CS_SOUNDS + 4] = "MP_bomb_plant".to_string();
        let mut loops = HashMap::new();
        loops.insert(13u32, (4, Vec3::ZERO));
        a.set_loop_sounds(&fs, &cs, &loops);
        let id = a.loop_voices[&13].id;
        assert!(!a.loop_voices[&13].looping);
        a.step(&HashMap::new(), None);
        assert_eq!(a.table.len(), 0);
        a.set_loop_sounds(&fs, &cs, &loops);
        assert_eq!(a.loop_voices[&13].id, id); // not restarted
        assert_eq!(a.table.len(), 0);
        a.set_loop_sounds(&fs, &cs, &HashMap::new());
        assert!(a.loop_voices.is_empty());
    }

    #[test]
    fn loop_sound_index_change_restarts_and_zero_stops() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = AudioSystem::new(
            &fs,
            AudioOpts {
                enabled: false,
                volume: 1.0,
                rate: 0,
            },
        );
        a.on_gamestate("mp_carentan");
        let mut cs = vec![String::new(); 800];
        cs[cues::CS_SOUNDS + 3] = "bomb_tick".to_string();
        cs[cues::CS_SOUNDS + 4] = "ambient_mp_carentan".to_string();
        let mut loops = HashMap::new();
        loops.insert(12u32, (3, Vec3::ZERO));
        a.set_loop_sounds(&fs, &cs, &loops);
        let first = a.loop_voices[&12].id;
        loops.insert(12u32, (4, Vec3::ZERO));
        a.set_loop_sounds(&fs, &cs, &loops);
        assert_eq!(a.loop_voices.len(), 1);
        assert_ne!(a.loop_voices[&12].id, first);
        assert_eq!(a.table.len(), 1); // the old voice was stopped, not left
        loops.insert(12u32, (0, Vec3::ZERO));
        a.set_loop_sounds(&fs, &cs, &loops);
        assert!(a.loop_voices.is_empty());
        assert_eq!(a.table.len(), 0);
    }

    /// `snd_restart` reopens the ambient from the top on a new voice and
    /// takes later decodes and streams down to the new mixer rate (research
    /// doc, section 10, "Mixer rate and `snd_restart`").
    #[test]
    fn snd_restart_restarts_the_ambient_at_the_new_rate() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = AudioSystem::new(
            &fs,
            AudioOpts {
                enabled: false,
                volume: 1.0,
                rate: 44100,
            },
        );
        a.on_gamestate("mp_carentan");
        a.set_ambient(&fs, Some("ambient_mp_carentan"));
        let id = a.ambient.as_ref().unwrap().1;
        a.restart(&fs, 11025);
        let (alias, new_id) = a.ambient.clone().unwrap();
        assert_eq!(alias, "ambient_mp_carentan");
        assert_ne!(new_id, id);
        assert_eq!(a.bank.rate, 11025);
    }

    fn silent(fs: &Pk3Fs, rate: u32) -> AudioSystem {
        let mut a = AudioSystem::new(
            fs,
            AudioOpts {
                enabled: false,
                volume: 1.0,
                rate,
            },
        );
        a.on_gamestate("mp_carentan");
        a
    }

    fn cmd(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    /// Retail's `snd_restart` saves every stream but the ambient slots and
    /// reopens each at its position (research doc, section 10).
    #[test]
    fn snd_restart_resumes_dialog_and_restarts_the_ambient_from_the_top() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = silent(&fs, 44100);
        a.set_ambient(&fs, Some("ambient_mp_carentan"));
        let ambient = a.ambient.as_ref().unwrap().1;
        let line = a.play_local(&fs, "MP_announcer_allies_win").unwrap();
        // A silent system has no handle to ask; stand in for 1.25 s played.
        a.resumable.get_mut(&line).unwrap().from_s = 1.25;
        a.resumable.get_mut(&ambient).unwrap().from_s = 9.0;
        a.restart(&fs, 11025);
        assert_eq!(a.bank.rate, 11025);
        let new_ambient = a.ambient.as_ref().unwrap().1;
        assert_eq!(a.resumable[&new_ambient].from_s, 0.0);
        let resumed: Vec<&Resumable> = a
            .resumable
            .iter()
            .filter(|(id, _)| **id != new_ambient)
            .map(|(_, r)| r)
            .collect();
        assert_eq!(resumed.len(), 1);
        assert!(
            resumed[0]
                .row
                .name
                .eq_ignore_ascii_case("MP_announcer_allies_win")
        );
        assert_eq!(resumed[0].from_s, 1.25);
        assert_eq!(a.table.len(), 2);
    }

    /// A loaded (non-streamed) sound is not saved; it ends at the restart.
    #[test]
    fn snd_restart_drops_loaded_sounds() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = silent(&fs, 22050);
        a.play_local(&fs, "weap_thompson_fire");
        assert_eq!(a.table.len(), 1);
        assert!(a.resumable.is_empty());
        a.restart(&fs, 22050);
        assert_eq!(a.table.len(), 0);
    }

    /// `o` plays only into an idle music slot; `p 0` frees it
    /// (CoDMP.exe 0x44dc40, 0x44db90). The track survives `snd_restart`.
    #[test]
    fn music_play_waits_for_an_idle_slot_and_survives_snd_restart() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = silent(&fs, 22050);
        let cs = Vec::new();
        assert!(a.on_server_command(&fs, &cmd(&["o", "MP_announcer_allies_win"]), &cs));
        let first = a.music.unwrap().id;
        assert!(a.on_server_command(&fs, &cmd(&["o", "MP_announcer_axis_win"]), &cs));
        assert_eq!(a.music.unwrap().id, first);
        assert_eq!(a.table.len(), 1);
        a.resumable.get_mut(&first).unwrap().from_s = 0.5;
        a.restart(&fs, 44100);
        let Music { id: resumed, fade } = a.music.unwrap();
        assert!(fade.is_none());
        assert_ne!(resumed, first);
        assert_eq!(a.resumable[&resumed].from_s, 0.5);
        assert!(a.on_server_command(&fs, &cmd(&["p", "0"]), &cs));
        assert!(a.music.is_none());
        assert_eq!(a.table.len(), 0);
        a.on_server_command(&fs, &cmd(&["o", "MP_announcer_axis_win"]), &cs);
        let id = a.music.unwrap().id;
        assert!(
            a.resumable[&id]
                .row
                .name
                .eq_ignore_ascii_case("MP_announcer_axis_win")
        );
    }

    /// `p N` ramps the track's volume to 0 over N ms and closes it there; a
    /// second `p N` leaves the ramp alone (CoDMP.exe 0x44db90, 0x44e7a0).
    #[test]
    fn music_stop_ramps_the_track_out_and_closes_it() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = silent(&fs, 22050);
        let cs = Vec::new();
        a.on_server_command(&fs, &cmd(&["o", "MP_announcer_allies_win"]), &cs);
        let id = a.music.unwrap().id;
        let start = a.table.volume(id).unwrap();
        a.on_server_command(&fs, &cmd(&["p", "1000"]), &cs);
        a.advance_fades(250.0);
        a.on_server_command(&fs, &cmd(&["p", "100"]), &cs);
        a.advance_fades(250.0);
        let half = a.table.volume(id).unwrap();
        assert!((half - start * 0.5).abs() < 1e-5, "{half} vs {start}");
        a.advance_fades(600.0);
        assert!(a.music.is_none());
        assert_eq!(a.table.len(), 0);
    }

    /// `q <volume> <ms>` moves the master fade linearly and scales every
    /// voice; landing on 0 stops every sound, the ambient and the music
    /// included, and a later `q 1 0` brings the level back on the next step
    /// (CoDMP.exe 0x44e0f0, 0x44e890).
    #[test]
    fn sound_fade_ramps_the_master_and_stops_everything_at_zero() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = silent(&fs, 22050);
        let cs = Vec::new();
        a.set_master_volume(0.8);
        a.set_ambient(&fs, Some("ambient_mp_carentan"));
        a.on_server_command(&fs, &cmd(&["o", "MP_announcer_allies_win"]), &cs);
        assert!(a.on_server_command(&fs, &cmd(&["q", "0.000000", "1000"]), &cs));
        a.advance_fades(400.0);
        assert!((a.master() - 0.8 * 0.6).abs() < 1e-5);
        assert_eq!(a.table.len(), 2);
        a.advance_fades(700.0);
        assert_eq!(a.fade.level, 0.0);
        assert_eq!(a.table.len(), 0);
        assert!(a.ambient.is_none() && a.music.is_none());
        a.on_server_command(&fs, &cmd(&["q", "1.000000", "0"]), &cs);
        assert_eq!(a.fade.level, 0.0);
        a.advance_fades(16.0);
        assert_eq!(a.fade, SoundFade::FULL);
    }

    /// `q 0 0` stops every sound in the command itself.
    #[test]
    fn sound_fade_to_zero_in_no_time_stops_at_once() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = silent(&fs, 22050);
        a.play_local(&fs, "MP_announcer_allies_win");
        a.on_server_command(&fs, &cmd(&["q", "0.000000", "0"]), &[]);
        assert_eq!(a.table.len(), 0);
    }

    /// `snd_restart` runs `S_Init`, which sets the master fade back to full,
    /// but saves the music slot's ramp with the track's current volume, so a
    /// `musicStop` fade goes on where it was (CoDMP.exe 0x450029, 0x4510f0,
    /// 0x451330).
    #[test]
    fn snd_restart_resets_the_master_fade_and_keeps_a_music_fade() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = silent(&fs, 22050);
        let cs = Vec::new();
        a.on_server_command(&fs, &cmd(&["o", "MP_announcer_allies_win"]), &cs);
        let start = a.table.volume(a.music.unwrap().id).unwrap();
        a.on_server_command(&fs, &cmd(&["p", "1000"]), &cs);
        a.on_server_command(&fs, &cmd(&["q", "0.500000", "1000"]), &cs);
        a.advance_fades(250.0);
        let rate = a.music.unwrap().fade;
        a.restart(&fs, 44100);
        assert_eq!(a.fade, SoundFade::FULL);
        let m = a.music.unwrap();
        assert_eq!(m.fade, rate);
        let resumed = a.table.volume(m.id).unwrap();
        assert!((resumed - start * 0.75).abs() < 1e-5);
        assert_eq!(a.resumable[&m.id].volume, resumed);
        a.advance_fades(800.0);
        assert!(a.music.is_none());
    }

    /// With no game up the main menu replays its own music, so a restart
    /// saves nothing (research doc, section 10).
    #[test]
    fn snd_restart_saves_nothing_at_the_main_menu() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = silent(&fs, 22050);
        a.on_gamestate("menu");
        assert!(a.play_menu(&fs, "music_mainmenu").is_some());
        assert_eq!(a.resumable.len(), 1);
        a.restart(&fs, 44100);
        assert_eq!(a.table.len(), 0);
    }

    /// A round restart resends CS 3 with the same alias and a new `t`
    /// (research doc, section 9).
    #[test]
    fn ambient_restarts_only_on_a_changed_alias() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = AudioSystem::new(
            &fs,
            AudioOpts {
                enabled: false,
                volume: 1.0,
                rate: 0,
            },
        );
        a.on_gamestate("mp_carentan");
        a.set_ambient(&fs, Some("ambient_mp_carentan"));
        let id = a.ambient.as_ref().unwrap().1;
        a.set_ambient(&fs, Some("ambient_mp_carentan"));
        assert_eq!(a.ambient.as_ref().unwrap().1, id);
        assert_eq!(a.table.len(), 1);
        a.set_ambient(&fs, Some("ambient_mp_harbor"));
        assert_ne!(a.ambient.as_ref().unwrap().1, id);
        assert_eq!(a.table.len(), 1);
        a.set_ambient(&fs, None);
        assert!(a.ambient.is_none());
        assert_eq!(a.table.len(), 0);
    }

    #[test]
    fn server_command_s_plays_the_announcer_alias() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = AudioSystem::new(
            &fs,
            AudioOpts {
                enabled: false,
                volume: 1.0,
                rate: 0,
            },
        );
        a.on_gamestate("mp_carentan");
        let mut cs = vec![String::new(); 800];
        cs[cues::CS_SOUNDS + 5] = "MP_announcer_allies_win".to_string();
        let tok = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();

        assert!(a.on_server_command(&fs, &tok(&["s", "5"]), &cs));
        assert_eq!(a.table.len(), 1);
        assert_eq!(a.stats().misses, 0);

        // Unparsable, empty slot, out of range; recognised, silent.
        assert!(a.on_server_command(&fs, &tok(&["s", "x"]), &cs));
        assert!(a.on_server_command(&fs, &tok(&["s", "6"]), &cs));
        assert!(a.on_server_command(&fs, &tok(&["s", "0"]), &cs));
        assert_eq!(a.table.len(), 1);

        // Quick chat (`j`/`k`/`l`) is owned by `quick_chat::QuickChat`
        // upstream; the audio layer no longer recognises it.
        assert!(!a.on_server_command(&fs, &tok(&["j", "0", "0", "0", "yes_sir"]), &cs));
        assert_eq!(a.table.len(), 1);

        // The HUD's scoreboard command, not ours.
        assert!(!a.on_server_command(&fs, &tok(&["b", "1"]), &cs));
        assert!(!a.on_server_command(&fs, &[], &cs));
    }

    #[test]
    fn ambient_keeps_a_reserved_identity_and_restarts_when_lost() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = AudioSystem::new(
            &fs,
            AudioOpts {
                enabled: false,
                volume: 1.0,
                rate: 0,
            },
        );
        a.on_gamestate("mp_carentan");
        a.set_listener(Vec3::ZERO, Vec3::X, -Vec3::Y, Vec3::Z, 5);
        a.set_ambient(&fs, Some("ambient_mp_carentan"));
        let id = a.ambient.as_ref().unwrap().1;

        // `player_out_of_ammo` has a `local` row; on a shared
        // `(ps_entity, Local)` identity it would replace the ambient.
        for _ in 0..8 {
            a.play(
                &fs,
                Cue {
                    alias: "player_out_of_ammo".into(),
                    source: Source::Entity {
                        num: 5,
                        pos: Vec3::ZERO,
                    },
                    delay_s: 0.0,
                },
            );
        }
        assert!(a.table.looping(id).is_some(), "the ambient was replaced");

        // No handles on a silent system, so `step` reaps every voice.
        a.step(&HashMap::new(), None);
        assert!(a.table.looping(id).is_none());
        a.set_ambient(&fs, Some("ambient_mp_carentan"));
        assert_ne!(a.ambient.as_ref().unwrap().1, id);
        assert_eq!(a.table.len(), 1);
    }

    /// An unknown alias and an out-of-range emitter are permanent; the
    /// per-snapshot retry must not count them.
    #[test]
    fn loop_retries_do_not_inflate_the_counters() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = AudioSystem::new(
            &fs,
            AudioOpts {
                enabled: false,
                volume: 1.0,
                rate: 0,
            },
        );
        a.on_gamestate("mp_carentan");
        let far = Vec3::splat(50_000.0);
        let row = &a.aliases.get("bomb_tick").unwrap()[0];
        assert!(row.channel.is_spatial() && row.dist.1 < far.length());

        let mut cs = vec![String::new(); 800];
        cs[cues::CS_SOUNDS + 3] = "bomb_tick".to_string();
        cs[cues::CS_SOUNDS + 4] = "no_such_loop_alias".to_string();
        let mut loops = HashMap::new();
        loops.insert(12u32, (3, far));
        loops.insert(13u32, (4, Vec3::ZERO));
        for _ in 0..3 {
            a.set_loop_sounds(&fs, &cs, &loops);
        }
        let s = a.stats();
        assert_eq!((s.culled, s.misses), (0, 0));
        assert!(a.loop_voices.is_empty());
        assert_eq!(a.table.len(), 0);
    }

    #[test]
    fn non_finite_cue_positions_are_rejected() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = AudioSystem::new(
            &fs,
            AudioOpts {
                enabled: false,
                volume: 1.0,
                rate: 0,
            },
        );
        a.on_gamestate("mp_carentan");
        a.play(
            &fs,
            Cue {
                alias: "step_run_dirt".into(),
                source: Source::Point(Vec3::NAN),
                delay_s: 0.0,
            },
        );
        a.play(
            &fs,
            Cue {
                alias: "step_run_dirt".into(),
                source: Source::Entity {
                    num: 7,
                    pos: Vec3::new(0.0, f32::INFINITY, 0.0),
                },
                delay_s: 0.0,
            },
        );
        let s = a.stats();
        assert_eq!((s.culled, s.misses, s.plays), (2, 0, 0));
        assert_eq!(a.table.len(), 0);
        // Rejected ahead of the pick, so the no-repeat state is untouched.
        assert!(!a.last_pick.contains_key("step_run_dirt"));
    }

    #[test]
    fn weapon_sounds_cache_fills_from_cs7() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut a = AudioSystem::new(
            &fs,
            AudioOpts {
                enabled: false,
                volume: 1.0,
                rate: 0,
            },
        );
        let mut cs = vec![String::new(); 8];
        // CS7 is 1-based with no leading `none`, so index 1 is the first
        // token.
        cs[7] = "kar98k_mp thompson_mp".to_string();
        a.ensure_weapon(&fs, &cs, 2);
        assert_eq!(
            a.weapon_sounds[&2].as_ref().unwrap().fire.as_deref(),
            Some("weap_thompson_fire")
        );
        a.ensure_weapon(&fs, &cs, 9);
        assert!(a.weapon_sounds[&9].is_none());
    }
}
