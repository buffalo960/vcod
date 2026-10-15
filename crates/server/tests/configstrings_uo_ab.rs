//! The configstring table a protocol 22 server builds on a United Offensive
//! install, diffed slot by slot against the retail UO 1.51b server's
//! gamestate in `crates/common/tests/fixtures/net/gamestate-uo.bin`
//! (mp_cassino dm, `sv_hostname CoDHost`, `sv_maxclients 8`, `sv_pure 0`;
//! docs/research/coduo-diff.md 7 and "Server"), and against the
//! `--save-configstrings --capture-tag uo` tables in
//! `tests/fixtures/configstrings/<map>-<gametype>-uo.txt` for UO's own
//! gametypes (coduo-diff.md 22).
//!
//! Needs `COD_DIR` at a UO install (`uo/` beside `main/`); without one it
//! returns early. The `-uo-pure` tables are a `sv_pure 1` server on that
//! install with the third-party `nsf_mp_j5outpost.pk3` added to `main/`
//! (coduo-diff.md 24); without that pak their gate returns early.

use std::collections::BTreeMap;
use vcod_common::net::protocol::PROTOCOL_V22;

/// Slot 1's `sv_serverid`: retail UO's first level reads 17, 16 plus one
/// restart bump vcod does not reproduce (coduo-diff.md 20), and a
/// second map load moves it again.
const SKIPPED_KEYS: &[&str] = &["sv_serverid"];

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

/// A `--save-configstrings` table, by slot.
fn retail_txt(map: &str, gametype: &str) -> BTreeMap<usize, String> {
    table(&format!("{map}-{gametype}-uo"))
}

/// `tests/fixtures/configstrings/<name>.txt`, by slot.
fn table(name: &str) -> BTreeMap<usize, String> {
    let path = format!("tests/fixtures/configstrings/{name}.txt");
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
    let sv = server(fs, map, gametype, frames);
    if let Ok(p) = std::env::var("CS_UO_DUMP") {
        let t: String = (0..2048)
            .filter(|i| !sv.configstring(*i).is_empty())
            .map(|i| format!("{i} {}\n", sv.configstring(i)))
            .collect();
        std::fs::write(format!("{p}-{map}-{gametype}.txt"), t).unwrap();
    }
    (0..2048)
        .filter(|i| !sv.configstring(*i).is_empty())
        .map(|i| (i, sv.configstring(i).to_string()))
        .collect()
}

fn server(
    fs: vcod_common::pk3::Pk3Fs,
    map: &str,
    gametype: &str,
    frames: u32,
) -> vcod_server::server::Server {
    server_with(fs, map, gametype, frames, &[])
}

fn server_with(
    fs: vcod_common::pk3::Pk3Fs,
    map: &str,
    gametype: &str,
    frames: u32,
    sets: &[(&str, &str)],
) -> vcod_server::server::Server {
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
    for (k, v) in sets {
        sv.set_cvar(k, v);
    }
    let bsp_path = fs.resolve_map(map).expect("map in the mounted paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(fs).expect("load the scripts");
    for i in 1..=frames {
        sv.tick(start + std::time::Duration::from_millis(50 * u64::from(i)));
    }
    assert_eq!(sv.script_aborts(), Vec::<String>::new(), "{map} {gametype}");
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
    check(retail(), ours(fs, "mp_cassino", "dm", 0), "mp_cassino dm");
}

/// UO's own gametypes, each on the first stock map whose arena lists it.
/// `bas` runs on mp_foy, tanks and all.
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

/// A pure server's systeminfo, all four pak lists in it and 1318 and 1351
/// bytes long: UO keeps `BIG_INFO_STRING` where 1.1 cuts at 1023, and
/// references only the paks the level read a file from, plus any outside
/// `main` and `uo` (cod11-server-handshake.md, "The four cvars").
#[test]
fn a_pure_server_s_systeminfo_matches_uo() {
    for map in ["mp_cassino", "mp_j5outpost"] {
        let Some(fs) = vcod_common::testing::uo_fs() else {
            return;
        };
        if fs.resolve_map("mp_j5outpost").is_none() {
            eprintln!("no nsf_mp_j5outpost.pk3 in this install: skipped");
            return;
        }
        let sv = server_with(fs, map, "dm", 0, &[("sv_pure", "1")]);
        let retail = table(&format!("{map}-dm-uo-pure"));
        for i in [0, 1] {
            let (a, b) = (
                without(&retail[&i], SKIPPED_KEYS),
                without(sv.configstring(i), SKIPPED_KEYS),
            );
            assert_eq!(a, b, "{map} cs[{i}]: {}", info_diff(&a, &b));
        }
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
    let mut sv = server(fs, "mp_cassino", "dm", 0);
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
