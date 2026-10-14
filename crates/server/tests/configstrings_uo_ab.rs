//! The configstring table a protocol 22 server builds on a United Offensive
//! install, diffed slot by slot against the retail UO 1.51b server's
//! gamestate in `crates/common/tests/fixtures/net/gamestate-uo.bin`
//! (mp_cassino dm, `sv_hostname CoDHost`, `sv_maxclients 8`, `sv_pure 0`;
//! docs/research/coduo-diff.md 7 and "Server").
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
];

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

fn ours(fs: vcod_common::pk3::Pk3Fs) -> BTreeMap<usize, String> {
    let fs = std::rc::Rc::new(fs);
    let start = std::time::Instant::now();
    let cfg = vcod_server::ServerConfig {
        map: "mp_cassino".into(),
        hostname: "CoDHost".into(),
        max_clients: 8,
        gametype: "dm".into(),
        test_entities: 0,
        trace: false,
        bots: 0,
        bots_shoot: false,
    };
    let mut sv = vcod_server::server::Server::new(cfg, start);
    sv.set_protocol(&PROTOCOL_V22);
    let bsp_path = fs
        .resolve_map("mp_cassino")
        .expect("map in the mounted paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(fs).expect("load the scripts");
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
    let mut retail = retail();
    let mut ours = ours(fs);
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
            "{range:?}: ours has {left:?} out of retail's order"
        );
        let not_vehicle: Vec<&String> = extras
            .iter()
            .filter(|n| !VEHICLE.iter().any(|v| n.contains(v)))
            .collect();
        assert!(
            not_vehicle.is_empty(),
            "{range:?}: ours misses {not_vehicle:?}"
        );
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
        "{} configstring(s) differ from UO 1.51b\n{}",
        diffs.len(),
        diffs.join("\n")
    );
}
