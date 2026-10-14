//! The configstring table a protocol 22 server builds on a United Offensive
//! install, diffed slot by slot against the retail UO 1.51b server's
//! gamestate in `crates/common/tests/fixtures/net/gamestate-uo.bin`
//! (mp_cassino dm, `sv_hostname CoDHost`, `sv_maxclients 8`, `sv_pure 0`;
//! docs/research/coduo-diff.md 7 and "Server"), and against the
//! `--save-configstrings --capture-tag uo` tables in
//! `tests/fixtures/configstrings/<map>-<gametype>-uo.txt` for UO's own
//! gametypes (coduo-diff.md 21).
//!
//! Needs `COD_DIR` at a UO install (`uo/` beside `main/`); without one it
//! returns early.

use std::collections::BTreeMap;
use vcod_common::net::protocol::PROTOCOL_V22;

/// Slot 1's referenced pak pair: retail lists the paks a file was read from,
/// vcod every pak that fits (`configstrings::systeminfo`). And
/// `sv_serverid`: retail UO's first level reads 17, 16 plus one restart
/// bump vcod does not reproduce (coduo-diff.md 20).
const SKIPPED_KEYS: &[&str] = &["sv_referencedPakNames", "sv_referencedPaks", "sv_serverid"];

/// Slots that differ for a reason the doc names (coduo-diff.md 20): the
/// level start time, which UO's clock starts at the server's uptime (3's
/// `t` and 14), and configstring 8's registered-item bits, which hold the
/// vehicle weapons.
const GAPS: &[usize] = &[3, 8, 14];

/// What the vehicles vcod does not spawn add to the model, sound alias and
/// effect blocks: their models, shells and wrecks, the engine, turret and
/// MG aliases `G_ParseScrVehicleInfo` registers at every load, and the
/// burn and collision effects `_tankeffects_gmi` loads per tank.
const VEHICLE: &[&str] = &[
    "vehicle",
    "tank",
    "tiger",
    "jeep",
    "panzerIV",
    "sherman",
    "t34_",
    "truck",
    "ptboat",
    "c47_",
    "condor_",
    "bmwbike",
    "halftrack",
    "kubel_",
    "peugeot",
    "weap_mg34_",
    "weap_mg42_",
    "flak88",
];

/// What a spawned vehicle adds to the shader and hint string blocks.
const VEHICLE_UI: &[&str] = &["GMI_VEHICLE_", "fire_ready_shell"];

/// The block's names in slot order.
fn block(t: &BTreeMap<usize, String>, (lo, hi): (usize, usize)) -> Vec<String> {
    t.range(lo..=hi).map(|(_, s)| s.clone()).collect()
}

fn retail() -> BTreeMap<usize, String> {
    use vcod_common::net::{gamestate, huffman::Huffman, msg::MsgReader};
    let data = vcod_common::testing::fixture("net/gamestate-uo.bin");
    let huff = Huffman::new();
    let mut r = MsgReader::new(&data[4..], &huff);
    let gs = gamestate::parse(&mut r, &PROTOCOL_V22).expect("the UO gamestate");
    gs.configstrings
        .into_iter()
        .enumerate()
        .filter(|(_, s)| !s.is_empty())
        .collect()
}

/// A `--save-configstrings` table, by slot.
fn retail_txt(map: &str, gametype: &str) -> BTreeMap<usize, String> {
    let path = format!("tests/fixtures/configstrings/{map}-{gametype}-uo.txt");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| {
            let (i, v) = l.split_once(' ').unwrap_or((l, ""));
            (i.parse().unwrap(), v.to_string())
        })
        .collect()
}

/// Ours after the load and `frames` frames: retail's table was read at a
/// client's gamestate, seconds in.
fn ours(
    fs: vcod_common::pk3::Pk3Fs,
    map: &str,
    gametype: &str,
    frames: u32,
) -> BTreeMap<usize, String> {
    let fs = std::rc::Rc::new(fs);
    let start = std::time::Instant::now();
    let cfg = vcod_server::ServerConfig {
        map: map.into(),
        hostname: "CoDHost".into(),
        max_clients: 8,
        gametype: gametype.into(),
        test_entities: 0,
        trace: false,
        bots: 0,
        bots_shoot: false,
    };
    let mut sv = vcod_server::server::Server::new(cfg, start);
    sv.set_protocol(&PROTOCOL_V22);
    let bsp_path = fs.resolve_map(map).expect("map in the mounted paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(fs).expect("load the scripts");
    for i in 1..=frames {
        sv.tick(start + std::time::Duration::from_millis(50 * u64::from(i)));
    }
    if let Ok(p) = std::env::var("CS_UO_DUMP") {
        let t: String = (0..2048)
            .filter(|i| !sv.configstring(*i).is_empty())
            .map(|i| format!("{i} {}\n", sv.configstring(i)))
            .collect();
        std::fs::write(format!("{p}-{map}-{gametype}.txt"), t).unwrap();
    }
    assert_eq!(sv.script_aborts(), Vec::<String>::new(), "{map} {gametype}");
    (0..2048)
        .filter(|i| !sv.configstring(*i).is_empty())
        .map(|i| (i, sv.configstring(i).to_string()))
        .collect()
}

/// An info string without the keys `skip` names.
fn without(info: &str, skip: &[&str]) -> String {
    let parts: Vec<&str> = info.split('\\').skip(1).collect();
    parts
        .chunks(2)
        .filter(|kv| !skip.contains(&kv[0]))
        .map(|kv| format!("\\{}\\{}", kv[0], kv.get(1).unwrap_or(&"")))
        .collect()
}

/// The keys two info strings disagree on.
fn info_diff(a: &str, b: &str) -> String {
    let pairs = |s: &str| -> BTreeMap<String, String> {
        let parts: Vec<&str> = s.split('\\').skip(1).collect();
        parts
            .chunks(2)
            .map(|kv| (kv[0].to_string(), kv.get(1).unwrap_or(&"").to_string()))
            .collect()
    };
    let (a, b) = (pairs(a), pairs(b));
    let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
    keys.sort_unstable();
    keys.dedup();
    keys.into_iter()
        .filter(|k| a.get(*k) != b.get(*k))
        .map(|k| format!("{k}: retail {:?} ours {:?}", a.get(k), b.get(k)))
        .collect::<Vec<_>>()
        .join(", ")
}

#[test]
fn the_configstring_table_matches_uo_on_mp_cassino() {
    let Some(fs) = vcod_common::testing::uo_fs() else {
        return;
    };
    check(retail(), ours(fs, "mp_cassino", "dm", 0), "mp_cassino dm");
}

/// UO's own gametypes, each on the first stock map whose arena lists it.
/// `bas` runs on mp_foy, whose tanks vcod does not spawn either.
#[test]
fn the_configstring_tables_match_uo_s_gametypes() {
    for (map, gametype) in [
        ("mp_cassino", "ctf"),
        ("mp_cassino", "dom"),
        ("mp_cassino", "hq"),
        ("mp_foy", "bas"),
    ] {
        let Some(fs) = vcod_common::testing::uo_fs() else {
            return;
        };
        check(
            retail_txt(map, gametype),
            ours(fs, map, gametype, 20),
            &format!("{map} {gametype}"),
        );
    }
}

fn check(mut retail: BTreeMap<usize, String>, mut ours: BTreeMap<usize, String>, what: &str) {
    for t in [&mut retail, &mut ours] {
        if let Some(s) = t.get_mut(&1) {
            *s = without(s, SKIPPED_KEYS);
        }
        for i in GAPS {
            t.remove(i);
        }
    }
    // The model, alias and effect blocks: ours is retail's in retail's order with
    // only the vehicles' own entries missing.
    use vcod_common::net::protocol::CsRange;
    for range in [CsRange::Model, CsRange::SoundAlias, CsRange::Effect] {
        let bounds = PROTOCOL_V22.cs.bounds(range);
        let ours_block = block(&ours, bounds);
        let mut next = ours_block.iter().peekable();
        let mut extras = Vec::new();
        for name in block(&retail, bounds) {
            if next.peek() == Some(&&name) {
                next.next();
            } else {
                extras.push(name);
            }
        }
        let left: Vec<&String> = next.collect();
        assert!(
            left.is_empty(),
            "{what} {range:?}: ours has {left:?} out of retail's order"
        );
        let not_vehicle: Vec<&String> = extras
            .iter()
            .filter(|n| !VEHICLE.iter().any(|v| n.contains(v)))
            .collect();
        assert!(
            not_vehicle.is_empty(),
            "{what} {range:?}: ours misses {not_vehicle:?}"
        );
        for t in [&mut retail, &mut ours] {
            t.retain(|i, _| !(bounds.0..=bounds.1).contains(i));
        }
    }
    // The shaders and the hint strings: retail's spawned vehicles register
    // theirs first (mp_foy's five `GMI_VEHICLE_*` hints, the tank HUD's
    // `fire_ready_shell`), so the two match once those are set aside.
    let hints = PROTOCOL_V22.cs.hint_strings;
    for bounds in [PROTOCOL_V22.cs.bounds(CsRange::Shader), (hints, hints + 31)] {
        let rest = |t: &BTreeMap<usize, String>| -> Vec<String> {
            block(t, bounds)
                .into_iter()
                .filter(|n| !VEHICLE_UI.iter().any(|v| n.contains(v)))
                .collect()
        };
        assert_eq!(rest(&retail), rest(&ours), "{what} {bounds:?}");
        for t in [&mut retail, &mut ours] {
            t.retain(|i, _| !(bounds.0..=bounds.1).contains(i));
        }
    }
    let mut slots: Vec<usize> = retail.keys().chain(ours.keys()).copied().collect();
    slots.sort_unstable();
    slots.dedup();
    let diffs: Vec<String> = slots
        .into_iter()
        .filter(|i| retail.get(i) != ours.get(i))
        .map(|i| match (retail.get(&i), ours.get(&i)) {
            (Some(a), Some(b)) if a.starts_with('\\') => format!("cs[{i}] {}", info_diff(a, b)),
            (a, b) => format!("cs[{i}] retail {a:?} ours {b:?}"),
        })
        .collect();
    assert!(
        diffs.is_empty(),
        "{what}: {} configstring(s) differ from UO 1.51b\n{}",
        diffs.len(),
        diffs.join("\n")
    );
}
