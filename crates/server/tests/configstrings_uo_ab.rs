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
/// `t` and 14).
const GAPS: &[usize] = &[3, 14];

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
    let sv = server(fs);
    (0..2048)
        .filter(|i| !sv.configstring(*i).is_empty())
        .map(|i| (i, sv.configstring(i).to_string()))
        .collect()
}

fn server(fs: vcod_common::pk3::Pk3Fs) -> vcod_server::server::Server {
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
    sv
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

/// The two tanks dm keeps on mp_cassino, as the retail gamestate's
/// baselines have them: the entity numbers the spawn order gives, the pool
/// slots, the collmaps' brush models, the settled height and every other
/// field (docs/research/coduo-vehicles.md 10).
#[test]
fn the_vehicle_baselines_match_uo_on_mp_cassino() {
    let Some(fs) = vcod_common::testing::uo_fs() else {
        return;
    };
    use vcod_common::net::{gamestate, huffman::Huffman, msg::MsgReader};
    let data = vcod_common::testing::fixture("net/gamestate-uo.bin");
    let huff = Huffman::new();
    let mut r = MsgReader::new(&data[4..], &huff);
    let p = &PROTOCOL_V22;
    let gs = gamestate::parse(&mut r, p).expect("the UO gamestate");
    let mut sv = server(fs);
    let mut ours = sv.all_entities();
    for e in ours.values_mut() {
        vcod_server::game::wire::entity_to_protocol(p, e);
    }
    let mut tanks: Vec<u32> = gs
        .baselines
        .iter()
        .filter(|(_, e)| e.field_i32(p, "eType") == vcod_common::net::flags::ET_VEHICLE_V22)
        .map(|(n, _)| *n)
        .collect();
    tanks.sort_unstable();
    assert_eq!(tanks, [176, 181]);
    for n in tanks {
        let (r, o) = (&gs.baselines[&n], ours.get(&n).expect("our tank"));
        for (f, (a, b)) in p.entity_fields.iter().zip(r.fields.iter().zip(&o.fields)) {
            // The vehicle table sends angles as 16 bits, which read back
            // within -180..180: ours holds the BSP's 270 where the wire
            // gives -90.
            if f.name.starts_with("apos.trBase") {
                let d = (f32::from_bits(*a as u32) - f32::from_bits(*b as u32)).rem_euclid(360.0);
                assert!(d.min(360.0 - d) < 1e-3, "entity {n} {}", f.name);
                continue;
            }
            // An int field travels in the vehicle table's width (`time2`
            // 6 bits).
            let bits = p
                .vehicle_fields
                .iter()
                .find(|v| v.name == f.name)
                .map_or(f.bits, |v| v.bits);
            let mask = match bits {
                1..=31 => ((1i64 << bits) - 1) as i32,
                _ => !0,
            };
            assert_eq!(a & mask, b & mask, "entity {n} {}", f.name);
        }
    }
}
