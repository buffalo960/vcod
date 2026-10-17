//! `addTestClient` on a whole server: `probe_testclient` run on mp_pavlov
//! against its retail section, the spawned test client's origin included,
//! which only a server with the map's collision and its own frame loop
//! produces (docs/research/cod11-gsc-object-model.md 25).
//!
//! Needs `COD_DIR`; without the paks it returns early.

mod common;

use std::rc::Rc;
use std::time::Instant;

#[test]
fn probe_testclient_matches_retail_on_a_server() {
    let Some(fs) = vcod_common::testing::game_fs_11() else {
        eprintln!("COD_DIR unset or not a 1.1 install: skipping");
        return;
    };
    let mut now = Instant::now();
    let bsp_path = fs
        .resolve_map("mp_pavlov")
        .expect("map in the mounted paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut sv = vcod_server::Server::new(common::cfg("mp_pavlov", "probe_testclient"), now);
    sv.overlay_script(
        "maps/mp/gametypes/probe_testclient",
        include_str!("../../gsc/tests/fixtures/semantics/probe_testclient.gsc"),
    );
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    common::run_alone(&mut sv, &mut now, 8000);
    let ours: Vec<&str> = sv
        .script_log()
        .iter()
        .filter(|l| l.starts_with("PROBE "))
        .map(String::as_str)
        .collect();
    let text = std::fs::read_to_string("../gsc/tests/fixtures/semantics/retail-captures.txt")
        .expect("read retail-captures.txt");
    let retail: Vec<&str> = text
        .split("# probe_testclient\n")
        .nth(1)
        .expect("a probe_testclient section")
        .lines()
        .take_while(|l| !l.is_empty())
        .collect();
    assert_eq!(ours, retail);
}
