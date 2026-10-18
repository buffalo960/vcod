//! `trigger_damage` under script blasts, against retail UO 1.51b
//! (`client-probes/probe_dmgtrig.gsc` on mp_uo_stanjel, `COD_VERSION=uo
//! tools/run_probe.sh`, 2026-10-18; docs/research/cod11-combat.md 14.8).
//! Every blast activates its bush, but a `"trigger"` a script thread
//! queues is dropped with `G_RunFrame`'s queue, so none is ever notified:
//! the activations show in the wait they arm (the second and third blasts
//! take damage and fire nothing) and in `health`, back at 32000 after
//! each. A breakpoint on retail's `G_Trigger` counted one call per
//! activation the gate expects.
//!
//! Needs `COD_DIR` at a UO install; without one it returns early.

mod common;

/// What retail's `games_mp.log` held.
const RETAIL: &[&str] = &[
    "PROBE count 17",
    "PROBE ent 86 (-1976.00, 3240.00, 24.00) 32000",
    "PROBE ent 87 (-1352.00, 3656.00, -312.00) 32000",
    "PROBE ent 88 (-816.00, 4520.00, -448.00) 32000",
    "PROBE ent 89 (1240.00, 3856.00, -384.00) 32000",
    "PROBE ent 90 (1952.00, 3336.00, -312.00) 32000",
    "PROBE ent 91 (-1944.00, 2136.00, 560.00) 32000",
    "PROBE ent 92 (-1120.00, 3184.00, -112.00) 32000",
    "PROBE ent 93 (1640.00, 3472.00, -96.00) 32000",
    "PROBE ent 94 (368.00, 2448.00, 16.00) 32000",
    "PROBE ent 414 (-2000.00, 2104.00, 272.00) 32000",
    "PROBE ent 416 (528.00, 4904.00, -464.00) 32000",
    "PROBE ent 417 (-200.00, 4888.00, -464.00) 32000",
    "PROBE ent 418 (-696.00, 4888.00, -464.00) 32000",
    "PROBE ent 420 (-48.00, 2888.00, -144.00) 32000",
    "PROBE ent 422 (0.00, 0.00, 0.00) 32000",
    "PROBE ent 425 (1504.00, 2536.00, -56.00) 32000",
    "PROBE ent 493 (-2072.00, 2920.00, 72.00) 32000",
    "PROBE phase 1550 single",
    "PROBE health 1550 86 32000",
    "PROBE damage 1550 86 100 world MOD_EXPLOSIVE",
    "PROBE phase 1650 busy",
    "PROBE health 1650 86 32000",
    "PROBE damage 1650 86 100 world MOD_EXPLOSIVE",
    "PROBE phase 1900 still_busy",
    "PROBE damage 1900 86 100 world MOD_EXPLOSIVE",
    "PROBE phase 2000 rearmed",
    "PROBE damage 2000 86 100 world MOD_EXPLOSIVE",
    "PROBE phase 3000 edge",
    "PROBE health 3000 86 32000",
    "PROBE damage 3000 86 77 world MOD_EXPLOSIVE",
    "PROBE phase 4000 out_of_range",
    "PROBE phase 5000 kill",
    "PROBE health 5000 86 32000",
    "PROBE death 5000 86 world",
    "PROBE damage 5000 86 40000 world MOD_EXPLOSIVE",
    "PROBE phase 6000 same_frame",
    "PROBE health 6000 87 32000",
    "PROBE damage 6000 87 100 world MOD_EXPLOSIVE",
    "PROBE phase 7000 attacker",
    "PROBE damage 7000 87 100 312 MOD_EXPLOSIVE",
    "PROBE phase 8000 gone_attacker",
    "PROBE damage 8000 86 100 313 MOD_EXPLOSIVE",
    "PROBE phase 9000 wait_loop",
    "PROBE damage 9000 87 100 world MOD_EXPLOSIVE",
    "PROBE damage 9050 87 100 world MOD_EXPLOSIVE",
    "PROBE damage 9100 87 100 world MOD_EXPLOSIVE",
    "PROBE damage 9150 87 100 world MOD_EXPLOSIVE",
    "PROBE damage 9200 87 100 world MOD_EXPLOSIVE",
    "PROBE damage 9250 87 100 world MOD_EXPLOSIVE",
    "PROBE damage 9300 87 100 world MOD_EXPLOSIVE",
    "PROBE damage 9350 87 100 world MOD_EXPLOSIVE",
    "PROBE damage 9400 87 100 world MOD_EXPLOSIVE",
    "PROBE damage 9450 87 100 world MOD_EXPLOSIVE",
    "PROBE damage 9500 87 100 world MOD_EXPLOSIVE",
    "PROBE damage 9550 87 100 world MOD_EXPLOSIVE",
    "PROBE end",
];

#[test]
fn script_blasts_on_trigger_damage_match_retail_uo() {
    let Some(fs) = vcod_common::testing::uo_fs() else {
        return;
    };
    let fs = std::rc::Rc::new(fs);
    let bsp_path = fs
        .resolve_map("mp_uo_stanjel")
        .expect("mp_uo_stanjel in the UO paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = std::time::Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg("mp_uo_stanjel", "probe_dmgtrig"), now);
    sv.set_protocol(&vcod_common::net::protocol::PROTOCOL_V22);
    let src =
        std::fs::read_to_string("../gsc/tests/fixtures/semantics/client-probes/probe_dmgtrig.gsc")
            .expect("the probe");
    sv.overlay_script("maps/mp/gametypes/probe_dmgtrig", &src);
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(fs).expect("load the scripts");
    common::run_alone(&mut sv, &mut now, 11000);
    let ours: Vec<&str> = sv
        .script_log()
        .iter()
        .map(|l| l.trim())
        .filter(|l| l.starts_with("PROBE"))
        .collect();
    if std::env::var_os("DMGTRIG_REPORT").is_some() {
        for l in &ours {
            eprintln!("{l}");
        }
    }
    assert_eq!(ours, RETAIL);
}
