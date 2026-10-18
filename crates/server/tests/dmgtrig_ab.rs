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

/// `client-probes/probe_dmgshot.gsc` on retail UO 1.51b, mp_uo_stanjel,
/// 2026-10-18: one axis `--probe-uo-weapon direct` client with
/// `--probe-weapon kar98k_mp`, held 160 off the first bush and aimed at it.
/// A round and a swing notify `"damage"` with two arguments (no mod) and
/// queue a `"trigger"`, whose waiter runs first; a miss does nothing; the
/// grenade's blast takes `G_Damage`'s six-argument arm and reaches a second
/// bush. Two retail runs read the round and swing lines identically and
/// the grenade's 100/16 and 98/18: where a throw lands rides the client's
/// cmd timing, so [`landing_free`] drops those two numbers and the landing.
const RETAIL_SHOT: &[&str] = &[
    "PROBE phase shot",
    "PROBE trigger 86 player -",
    "PROBE damage 86 120 player -",
    "PROBE phase shot_again",
    "PROBE trigger 86 player 2100",
    "PROBE damage 86 120 player -",
    "PROBE phase miss",
    "PROBE phase melee",
    "PROBE trigger 86 player 5850",
    "PROBE damage 86 150 player -",
    "PROBE phase grenade",
    "PROBE weapon stielhandgranate_mp",
    "PROBE trigger 86 player 8450",
    "PROBE trigger 92 player -",
    "PROBE damage 86 98 player MOD_GRENADE_SPLASH",
    "PROBE damage 92 18 player MOD_GRENADE_SPLASH",
    "PROBE nade (-2021.00, 3339.00, 3.00)",
    "PROBE done",
];

/// A line with what the grenade's landing decides taken out: the blast's
/// charge on a `MOD_GRENADE_SPLASH` line, and the `nade` origin.
fn landing_free(l: &str) -> String {
    if l.starts_with("PROBE nade") {
        return "PROBE nade".to_string();
    }
    if l.ends_with("MOD_GRENADE_SPLASH") {
        let t: Vec<&str> = l.split(' ').collect();
        return format!("{} {} {} _ {} {}", t[0], t[1], t[2], t[4], t[5]);
    }
    l.to_string()
}

/// The buttons of a `f "PROBE_IN ..."` command.
fn direct_buttons(tokens: &[String]) -> Option<u8> {
    if tokens.first().map(String::as_str) != Some("f") {
        return None;
    }
    let mut t = tokens.get(1)?.strip_prefix("PROBE_IN ")?.split_whitespace();
    t.next()?.parse().ok()
}

#[test]
fn a_players_rounds_swing_and_grenade_on_trigger_damage_match_retail_uo() {
    use common::{Join, Queues};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;
    use vcod_common::net::msg::UserCmd;
    use vcod_common::net::{NetClient, NetEvent};
    let Some(fs) = vcod_common::testing::uo_fs() else {
        return;
    };
    let fs = Rc::new(fs);
    let bsp_path = fs.resolve_map("mp_uo_stanjel").unwrap();
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = std::time::Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg("mp_uo_stanjel", "probe_dmgshot"), now);
    sv.set_protocol(&vcod_common::net::protocol::PROTOCOL_V22);
    let src =
        std::fs::read_to_string("../gsc/tests/fixtures/semantics/client-probes/probe_dmgshot.gsc")
            .expect("the probe");
    sv.overlay_script("maps/mp/gametypes/probe_dmgshot", &src);
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(fs).expect("load the scripts");
    common::run_alone(&mut sv, &mut now, 3000);
    let q = Rc::new(RefCell::new(Queues::default()));
    let mut cl = NetClient::start_with_qport(common::ClientEnd(q.clone()), now, 0x2001);
    let mut join = Join::new("axis", "kar98k_mp");
    let mut hold = common::ProbeHold {
        aim: true,
        ..Default::default()
    };
    let (mut buttons, mut selected) = (0u8, None::<u8>);
    let frame = Duration::from_millis(common::FRAME_MS as u64);
    for _ in 0..2000 {
        if sv.script_log().iter().any(|l| l.contains("PROBE done")) {
            break;
        }
        now += frame;
        let live = cl
            .snapshots()
            .newest()
            .is_some_and(|s| s.ps.field_i32(cl.protocol(), "pm_type") == 0);
        if live {
            let mut cmds = hold.cmds(&cl, buttons);
            if let Some(w) = selected {
                for c in &mut cmds {
                    c.weapon = w;
                }
            }
            cl.send_cmds(&cmds);
        } else if let Some(c) = cl.send_frame(&UserCmd::default()) {
            hold.last = c.server_time;
        }
        let pending: Vec<Vec<u8>> = q.borrow_mut().to_server.drain(..).collect();
        for p in pending {
            sv.handle_packet(common::ADDR, &p, now);
        }
        sv.tick(now);
        for (_, p) in sv.take_outgoing() {
            q.borrow_mut().to_client.push_back(p);
        }
        for e in cl.pump_at(now) {
            if let NetEvent::ServerCommand(tokens) = e {
                if tokens.first().map(String::as_str) == Some("a") {
                    selected = tokens.get(1).and_then(|w| w.parse().ok());
                }
                if let Some(b) = direct_buttons(&tokens) {
                    buttons = b;
                }
                join.on_server_command(&tokens, &mut cl, now);
            }
        }
    }
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
    let free = |v: &[&str]| v.iter().map(|l| landing_free(l)).collect::<Vec<_>>();
    assert_eq!(free(&ours), free(RETAIL_SHOT));
}
