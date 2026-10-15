//! Retail captures replayed through the client's per-frame event path,
//! headless: the prediction (`Predictor::predict`), the event collection and
//! its dedup (`events::frame_events`), the kick (`events::view_kicks`), the
//! effects (`fx::registry::resolve`) and the sounds (`audio::cues::resolve`),
//! each called as the frame in `main.rs` calls it. Every shot has to play
//! once: one fire event, one flash, one fire sound, one kick.
//!
//! The own-body replays read a `--save-slope` capture's cmds and snapshots
//! in arrival order, one frame per cmd, and compare what played with the
//! events each snapshot brought (`ev=`). They need the paks: the UO autofire
//! capture the UO install's (`COD_DIR` at it), else 1.1's mp_carentan and
//! mp44, whose fire and kick keys are UO's (coduo-movement.md 15); the 1.1
//! moves capture a 1.1 install. The follow replays decode the committed
//! snapshot captures and need nothing.

use std::collections::{BTreeMap, HashMap};

use glam::Vec3;
use vcod_common::collision::CollisionWorld;
use vcod_common::net::events::{EventTracker, GameEvent};
use vcod_common::net::msg::{self, NULL_USERCMD, UserCmd};
use vcod_common::net::protocol::{PROTOCOL_V1, PROTOCOL_V22, Protocol};
use vcod_common::net::snapshot::Snapshot;
use vcod_common::pk3::Pk3Fs;
use vcod_common::pmove::movers::SnapshotMovers;
use vcod_common::pmove::predict;
use vcod_common::weapon::{WeaponDef, WeaponSounds};

use super::cmds::CmdRing;
use super::events::{self, PredictedEvents};
use super::predict::{Predictor, solid_bodies};
use crate::audio::cues::{self, CueCtx};
use crate::fx::registry::{self, ResolveCtx, Resolved};

const SLOPE_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../server/tests/fixtures/playerstate"
);

/// What a replay played.
#[derive(Default, Debug)]
struct Tally {
    /// Events played, by 1.1 id.
    played: BTreeMap<i32, usize>,
    /// Fire events played, and their flashes, fire sounds and kicks.
    fires: usize,
    flashes: usize,
    fire_sounds: usize,
    kicks: usize,
    /// Fire events that flashed more than once or sounded more than once a
    /// barrel.
    doubled: usize,
    /// Events no effect handler knows, by id.
    unknown: BTreeMap<i32, usize>,
}

/// One frame's state, as `LivePhase` holds it.
struct Client<'a> {
    p: &'static Protocol,
    client_num: i32,
    weapons: &'a [Option<WeaponDef>],
    sounds: HashMap<i32, Option<WeaponSounds>>,
    configstrings: &'a [String],
    tracked: PredictedEvents,
    tracker: EventTracker,
    predictor: Predictor,
    tally: Tally,
}

impl<'a> Client<'a> {
    fn new(
        p: &'static Protocol,
        client_num: i32,
        weapons: &'a [Option<WeaponDef>],
        configstrings: &'a [String],
    ) -> Self {
        let sounds = weapons
            .iter()
            .enumerate()
            .map(|(i, w)| (i as i32, w.as_ref().map(|d| d.sounds.clone())))
            .collect();
        Client {
            p,
            client_num,
            weapons,
            sounds,
            configstrings,
            tracked: PredictedEvents::default(),
            tracker: EventTracker::new(),
            predictor: Predictor::default(),
            tally: Tally::default(),
        }
    }

    /// One frame on `newest`, predicted through `ring` when a world is given.
    fn frame(
        &mut self,
        newest: &Snapshot,
        prediction: Option<(&CmdRing, &CollisionWorld)>,
        now_ms: f64,
    ) {
        let p = self.p;
        let ps_client = newest.ps.field_i32(p, "clientNum");
        let pm_type = newest.ps.field_i32(p, "pm_type");
        let own = ps_client == self.client_num;
        let predicted = match prediction.filter(|_| own) {
            Some((ring, world)) => {
                let bodies =
                    solid_bodies(p, &newest.entities, self.client_num as u32, &HashMap::new());
                let movers = (
                    SnapshotMovers::from_entities(p, &newest.entities),
                    newest.server_time,
                );
                self.predictor.predict(
                    p,
                    &newest.ps,
                    ring,
                    world,
                    &bodies,
                    &movers,
                    self.weapons,
                    now_ms,
                )
            }
            None => None,
        };
        let evs = events::frame_events(
            &mut self.tracked,
            &mut self.tracker,
            newest,
            p,
            own,
            own && predict::predictable(pm_type),
            predicted.as_ref().map(|v| &v.pred),
            ps_client,
        );
        // `view_ps` is drawn, and so kicked, only while predictable.
        if predict::predictable(pm_type) {
            self.tally.kicks += events::view_kicks(&evs, ps_client);
        }
        // Every body has a muzzle: the model pass poses them all.
        let mut muzzles: HashMap<u32, (Vec3, Vec3)> = newest
            .entities
            .keys()
            .map(|&n| (n, (Vec3::ZERO, Vec3::X)))
            .collect();
        muzzles.insert(u32::MAX, (Vec3::ZERO, Vec3::X));
        let no_flash = HashMap::new();
        let ctx = ResolveCtx {
            muzzles: &muzzles,
            weapon_flash: &no_flash,
            view_flash: Some(self.weapons),
            view_body: registry::view_body(newest.ps.field_i32(p, "pm_flags"), ps_client),
            blood: true,
            brass: &HashMap::new(),
            weapons: self.weapons,
            eject_brass: true,
            configstrings: self.configstrings,
            cs: &p.cs,
        };
        let cue_ctx = CueCtx {
            configstrings: self.configstrings,
            sounds: p.cs.sounds(),
            weapon_sounds: &self.sounds,
            muzzles: &muzzles,
            listener_pos: Vec3::ZERO,
            ps_entity: cues::ps_entity(ps_client),
            uo: p.version == PROTOCOL_V22.version,
        };
        for ev in &evs {
            dispatch(&mut self.tally, ev, &ctx, &cue_ctx);
        }
    }
}

/// One event through the effects and the sounds, as the frame's event loop
/// runs it.
fn dispatch(t: &mut Tally, ev: &GameEvent, ctx: &ResolveCtx, cue_ctx: &CueCtx) {
    *t.played.entry(ev.event).or_default() += 1;
    let resolved = registry::resolve(ev, ctx);
    if resolved.iter().any(|r| matches!(r, Resolved::Unknown)) {
        *t.unknown.entry(ev.event).or_default() += 1;
    }
    if !cues::fires_weapon(ev.event) {
        return;
    }
    // A fire event resolves to its flash, its spent case and its sound alone.
    let flashes = resolved
        .iter()
        .filter(|r| match r {
            Resolved::Spawn { bolt, .. } => bolt.is_some(),
            Resolved::OnTag { .. } => true,
            _ => false,
        })
        .count();
    let sounds = cues::resolve(ev, cue_ctx).len();
    let calls = super::recoil::fire_calls(ev.event).max(1);
    t.fires += 1;
    t.flashes += flashes;
    t.fire_sounds += sounds;
    t.doubled += usize::from(flashes > 1 || sounds > calls);
}

/// A `--save-slope` capture line.
enum Line {
    Cmd(UserCmd),
    /// The snapshot, and the `(event, parm)` pairs it brought in 1.1's ids.
    Snap(Box<Snapshot>, Vec<(i32, i32)>),
}

fn parse(p: &Protocol, text: &str) -> Vec<Line> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some((kind, rest)) = line.split_once(' ') else {
            continue;
        };
        let kv: BTreeMap<&str, &str> = rest
            .split_whitespace()
            .filter_map(|kv| kv.split_once('='))
            .collect();
        let int = |k: &str| -> i32 { kv[k].parse().unwrap_or_else(|_| panic!("{k} in {line}")) };
        let pairs = |k: &str| -> Vec<(&str, i32)> {
            kv.get(k)
                .into_iter()
                .flat_map(|v| v.split(','))
                .filter(|s| !s.is_empty())
                .map(|s| {
                    let (a, b) = s.rsplit_once(':').expect("name:value");
                    (a, b.parse().expect("an int"))
                })
                .collect()
        };
        match kind {
            "!cmd" => {
                let a: Vec<i32> = kv["angles"]
                    .split(',')
                    .map(|x| x.parse().unwrap())
                    .collect();
                out.push(Line::Cmd(UserCmd {
                    server_time: int("st"),
                    buttons: int("buttons") as u8,
                    wbuttons: int("wbuttons") as u8,
                    weapon: int("weapon") as u8,
                    angles: [a[0], a[1], a[2]],
                    forward: int("forward") as i8,
                    right: int("right") as i8,
                    up: int("up") as i8,
                    ..NULL_USERCMD
                }));
            }
            "!snap" => {
                let mut ps = msg::PlayerState::null(p);
                for (name, v) in pairs("wire") {
                    ps.fields[msg::PlayerState::field_index(p, name).expect(name)] = v;
                }
                for (k, v) in pairs("ammo") {
                    ps.arrays.ammo[k.parse::<usize>().unwrap()] = v as i16;
                }
                for (k, v) in pairs("clip") {
                    ps.arrays.ammoclip[k.parse::<usize>().unwrap()] = v as i16;
                }
                let ev = pairs("ev")
                    .into_iter()
                    .map(|(e, parm)| (e.parse().expect("an event id"), parm))
                    .collect();
                let snap = Snapshot {
                    server_time: int("t"),
                    message_num: int("msg") as u32,
                    delta_num: -1,
                    snap_flags: 0,
                    ps,
                    entities: BTreeMap::new(),
                    clients: BTreeMap::new(),
                    valid: true,
                };
                out.push(Line::Snap(Box::new(snap), ev));
            }
            _ => {}
        }
    }
    out
}

/// The capture's map world as the client loads it.
fn load_world(fs: &Pk3Fs, map: &str) -> CollisionWorld {
    let bsp_path = fs.resolve_map(map).expect("map in the mounted paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).expect("read the bsp")).expect("parse");
    let world = CollisionWorld::build(&bsp, &vcod_common::props::collision_tris(fs, &bsp.entities));
    world.unlink_script_brushes(&bsp.entities, "dm");
    world
}

/// A `--save-slope` capture replayed as our own body, one frame per cmd:
/// what played, and the events the snapshots brought by id.
fn replay_own(
    p: &'static Protocol,
    file: &str,
    fs: &Pk3Fs,
    map: &str,
) -> (Tally, BTreeMap<i32, usize>) {
    let text = std::fs::read_to_string(format!("{SLOPE_DIR}/{file}")).unwrap();
    // The 1.1 captures name none: retail's mp_carentan dm list.
    let cs7 = text
        .lines()
        .find_map(|l| l.strip_prefix("# cs7 "))
        .or_else(|| {
            include_str!("../../../server/tests/fixtures/configstrings/mp_carentan-dm.txt")
                .lines()
                .find_map(|l| l.strip_prefix("7 "))
        })
        .expect("a weapon list");
    let weapons = vcod_common::weapon_table::from_configstring(fs, cs7);
    let world = load_world(fs, map);
    let mut client = Client::new(p, 0, &weapons, &[]);
    let mut ring = CmdRing::default();
    let mut newest: Option<Snapshot> = None;
    let mut brought = BTreeMap::new();
    for line in parse(p, &text) {
        match line {
            Line::Snap(s, ev) => {
                // The first drain only marks the ring.
                if newest.is_some() {
                    for (e, _) in ev {
                        *brought.entry(e).or_default() += 1;
                    }
                }
                newest = Some(*s);
            }
            Line::Cmd(c) => {
                ring.push(c);
                if let Some(s) = &newest {
                    client.frame(s, Some((&ring, &world)), f64::from(c.server_time));
                }
            }
        }
    }
    (client.tally, brought)
}

/// Every shot of an own-body replay played once, with its flash, its
/// sound and its kick.
fn assert_each_shot_once(name: &str, t: &Tally, brought: &BTreeMap<i32, usize>, shots: usize) {
    println!("{name}: {t:?}");
    let fires: usize = brought
        .iter()
        .filter(|(e, _)| cues::fires_weapon(**e))
        .map(|(_, n)| n)
        .sum();
    assert_eq!(fires, shots, "{name}: the capture's shots");
    assert_eq!(&t.played, brought, "{name}: events played against brought");
    assert_eq!(t.fires, shots, "{name}: fire events");
    assert_eq!(t.flashes, shots, "{name}: flashes");
    assert_eq!(t.fire_sounds, shots, "{name}: fire sounds");
    assert_eq!(t.kicks, shots, "{name}: kicks");
    assert!(
        t.unknown.is_empty(),
        "{name}: unknown events {:?}",
        t.unknown
    );
}

/// A 1.1 install's paks: `main/` with no `uo/` beside it.
fn v1_fs() -> Option<Pk3Fs> {
    vcod_common::testing::uo_fs()
        .is_none()
        .then(vcod_common::testing::game_fs)
        .flatten()
}

/// United Offensive's mp44 held from the hip, held down the sight, then
/// tapped (`--probe-autofire`, moves_ab's `AUTOFIRE_UO`): 29 rounds, each
/// once. Reading the predicted ring in UO's ids played every round twice
/// (coduo-movement.md 15).
#[test]
fn a_uo_autofire_capture_plays_each_shot_once() {
    let Some(fs) = vcod_common::testing::uo_fs().or_else(vcod_common::testing::game_fs) else {
        return;
    };
    let (t, brought) = replay_own(
        &PROTOCOL_V22,
        "mp_carentan-dm-slope-8ms-autofire-uo.txt",
        &fs,
        "mp_carentan",
    );
    assert_each_shot_once("autofire-uo", &t, &brought, 29);
}

/// 1.1's movement script (moves_ab's `moves-1.1`): jumps, landings, stance
/// changes and eight carbine shots, every event once.
#[test]
fn a_1_1_moves_capture_plays_each_event_once() {
    let Some(fs) = v1_fs() else {
        return;
    };
    let (t, brought) = replay_own(
        &PROTOCOL_V1,
        "mp_carentan-dm-slope-8ms-moves-1.1.txt",
        &fs,
        "mp_carentan",
    );
    assert_each_shot_once("moves-1.1", &t, &brought, 8);
}

/// The 1.1 ids of a ring's new slots, `prev..cur` (`eventSequence` counts
/// after the write).
fn new_slots(p: &Protocol, prev: i32, cur: i32, slot: impl Fn(String) -> i32) -> Vec<i32> {
    (prev..prev + ((cur - prev) & 0xff).min(4))
        .map(|i| (p.event_to_v1)(slot(format!("events[{}]", i & 3))))
        .collect()
}

/// A committed snapshot capture decoded and played frame by frame with no
/// prediction: a follower's view as the client draws it, and a seated
/// body's own ring as a prediction miss plays it, off the snapshot. Checks
/// the fire events played against the rings' own count.
fn replay_capture(p: &'static Protocol, tag: &str, weapons: &[Option<WeaponDef>]) -> Tally {
    let gs = vcod_common::testing::fixture_gamestate(p, &format!("net/gamestate{tag}.bin"));
    let mut client = Client::new(p, gs.client_num, weapons, &gs.configstrings);
    let (mut fires, mut kicks) = (0, 0);
    let mut ps_seq = None;
    let mut seqs: HashMap<u32, i32> = HashMap::new();
    for f in vcod_common::testing::fixture_frames(
        p,
        &format!("net/gamestate{tag}.bin"),
        &format!("net/snapshots{tag}.bin"),
    ) {
        let s = &f.snapshot;
        let cur = s.ps.field_i32(p, "eventSequence");
        if let Some(prev) = ps_seq.replace(cur) {
            for e in new_slots(p, prev, cur, |n| s.ps.field_i32(p, &n)) {
                kicks += super::recoil::fire_calls(e);
                fires += usize::from(cues::fires_weapon(e));
            }
        }
        for (&num, e) in &s.entities {
            if p.event_of(e.field_i32(p, "eType")).is_some() {
                continue;
            }
            let cur = e.field_i32(p, "eventSequence");
            if let Some(prev) = seqs.insert(num, cur) {
                fires += new_slots(p, prev, cur, |n| e.field_i32(p, &n))
                    .into_iter()
                    .filter(|&ev| cues::fires_weapon(ev))
                    .count();
            }
        }
        seqs.retain(|n, _| s.entities.contains_key(n));
        client.frame(s, None, f64::from(s.server_time));
    }
    let t = client.tally;
    println!("{tag}: {fires} fires and {kicks} kicks in the rings, {t:?}");
    assert_eq!(t.fires, fires, "{tag}: fire events");
    assert_eq!(t.kicks, kicks, "{tag}: kicks");
    t
}

/// The UO captures' weapons: the UO install's files, else the keys
/// `registry::uo_captures` copied out of them.
fn uo_weapons(tag: &str) -> Vec<Option<WeaponDef>> {
    match vcod_common::testing::uo_fs() {
        Some(fs) => {
            let gs = vcod_common::testing::fixture_gamestate(
                &PROTOCOL_V22,
                &format!("net/gamestate{tag}.bin"),
            );
            vcod_common::weapon_table::from_configstring(&fs, &gs.configstrings[7])
        }
        None => registry::uo_captures::weapons(tag.trim_start_matches('-')),
    }
}

/// Every committed UO capture: each fire event played and kicked once,
/// flashing and sounding at most once, and no event the client cannot place.
#[test]
fn the_uo_captures_play_each_shot_once() {
    for tag in [
        "-uo",
        "-uo-arty",
        "-uo-flame",
        "-uo-jeep",
        "-uo-mg",
        "-uo-overheat",
        "-uo-quake",
        "-uo-satchel",
        "-uo-smoke",
        "-uo-tank",
        "-uo-turret",
    ] {
        let weapons = uo_weapons(tag);
        let t = replay_capture(&PROTOCOL_V22, tag, &weapons);
        assert!(
            t.unknown.is_empty(),
            "{tag}: unknown events {:?}",
            t.unknown
        );
        assert_eq!(t.doubled, 0, "{tag}: fire events flashed or sounded twice");
    }
}
