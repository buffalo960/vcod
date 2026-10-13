//! The configstring table a protocol 6 server builds on a 1.5 install,
//! diffed slot by slot against the 1.5b server's gamestate in
//! `crates/common/tests/fixtures/net/gamestate-1.5.bin` (mp_carentan dm,
//! `sv_hostname CoDHost`, `sv_maxclients 8`, `sv_pure 0`, stock `scr_*`;
//! docs/research/cod15-protocol-diff.md 7 and "Server side").
//!
//! Needs `COD_DIR` at a 1.5 install; on a 1.1 one (the suite's usual) or
//! without paks it returns early.

use std::collections::BTreeMap;
use vcod_common::net::protocol::PROTOCOL_V6;

/// Slot 1's referenced pak pair: retail lists the paks a file was read from,
/// vcod every pak that fits (`configstrings::systeminfo`).
const REFERENCED: &[&str] = &["sv_referencedPakNames", "sv_referencedPaks"];

fn retail() -> BTreeMap<usize, String> {
    use vcod_common::net::{gamestate, huffman::Huffman, msg::MsgReader};
    let data = vcod_common::testing::fixture("net/gamestate-1.5.bin");
    let huff = Huffman::new();
    let mut r = MsgReader::new(&data[4..], &huff);
    let gs = gamestate::parse(&mut r, &PROTOCOL_V6).expect("the 1.5b gamestate");
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
        map: "mp_carentan".into(),
        hostname: "CoDHost".into(),
        max_clients: 8,
        gametype: "dm".into(),
        test_entities: 0,
        trace: false,
        bots: 0,
        bots_shoot: false,
    };
    let mut sv = vcod_server::server::Server::new(cfg, start);
    sv.set_protocol(&PROTOCOL_V6);
    let bsp_path = fs
        .resolve_map("mp_carentan")
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

#[test]
fn the_configstring_table_matches_15b_on_mp_carentan() {
    let Some(fs) = vcod_common::testing::game_fs() else {
        return;
    };
    if vcod_server::server::install_protocol(&fs).version != PROTOCOL_V6.version {
        return;
    }
    let mut retail = retail();
    let mut ours = ours(fs);
    for t in [&mut retail, &mut ours] {
        if let Some(s) = t.get_mut(&1) {
            *s = without(s, REFERENCED);
        }
    }
    let mut slots: Vec<usize> = retail.keys().chain(ours.keys()).copied().collect();
    slots.sort_unstable();
    slots.dedup();
    let diffs: Vec<String> = slots
        .into_iter()
        .filter(|i| retail.get(i) != ours.get(i))
        .map(|i| {
            format!(
                "cs[{i}] retail {:?} ours {:?}",
                retail.get(&i),
                ours.get(&i)
            )
        })
        .collect();
    assert!(
        diffs.is_empty(),
        "{} configstring(s) differ from 1.5b\n{}",
        diffs.len(),
        diffs.join("\n")
    );
}
