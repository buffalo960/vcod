//! A script model in a locational trace's way, ours against retail's.
//! `client-probes/probe_scriptmodel` spawns an `xmodel/barrel_black1`
//! script model and logs a `bulletTrace` through it shown, hidden,
//! `notSolid()`ed and deleted (1.1d and UO alike); on UO it also stands one
//! between a `--probe-uo-weapon aim` shooter and an empty Willys, shown for
//! 3 s, hidden for 3 s, then deleted, and logs the jeep's `damage`
//! notifies. Retail stops the trace and every round on the model until it
//! is deleted, hidden or not (docs/research/cod11-combat.md 2.7).
//!
//! On 1.1d the pair parts (`probe_part`) put a player behind a barrel: a
//! Thompson held on him (`rounds`), a frag thrown at the barrel and then
//! with it gone at him (`grenade`), and a frag resting with a barrel
//! between it and both players (`blast`). The `prone` part throws a frag
//! onto a prone player's back and, once it rests and he is moved off, a
//! second where he lay: on mp_carentan the first rests on him with his
//! number as its ground, on mp_brecourt the second rolls down terrain.
//!
//! `SM_REPORT=1` prints both logs. The 1.1d tests need `COD_DIR` at a 1.1
//! install, the UO rounds test at a UO one; each returns early without.

mod common;

use common::{Join, Queues, step};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::msg::{BUTTON_ADS, BUTTON_ATTACK, UserCmd};
use vcod_common::net::protocol::PROTOCOL_V22;
use vcod_common::net::{NetClient, NetEvent};

/// The shooter's first rounds leave before its sight is up, at the hip's
/// spread, and ours' RNG sends one or two past the barrel's sides where
/// retail's sent none; the shown phase is counted from this far past the
/// placement.
const SIGHT_MS: i32 = 500;

const SCRIPT: &str =
    include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_scriptmodel.gsc");

/// Our run of the probe on `map`; with `shooter`, client 0 joins and holds
/// the trigger from its placement for `ms` past it.
fn ours(fs: vcod_common::pk3::Pk3Fs, map: &str, uo: bool, shooter: bool, ms: i64) -> Vec<String> {
    let bsp_path = fs.resolve_map(map).expect("map in the paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(map, "probe_scriptmodel"), now);
    if uo {
        sv.set_protocol(&PROTOCOL_V22);
    }
    sv.overlay_script("maps/mp/gametypes/probe_scriptmodel", SCRIPT);
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    if !shooter {
        common::run_alone(&mut sv, &mut now, 3000);
        return sv.script_log().to_vec();
    }
    common::run_alone(&mut sv, &mut now, common::PROBE_JOIN_MS);
    let q = Rc::new(RefCell::new(Queues::default()));
    let mut cl = NetClient::start_with_qport(common::ClientEnd(q.clone()), now, 0x2001);
    let mut join = Join::new("allies", "thompson_mp");
    let frame = Duration::from_millis(common::FRAME_MS as u64);
    let mut hold = common::ProbeHold {
        aim: true,
        ..Default::default()
    };
    let mut placed_at: Option<i64> = None;
    for i in 0..3000i64 {
        now += frame;
        if placed_at.is_some_and(|at| (i - at) * common::FRAME_MS > ms) {
            break;
        }
        if placed_at.is_some_and(|at| i > at) && cl.state() == vcod_common::net::NetState::Active {
            cl.send_cmds(&hold.cmds(&cl, BUTTON_ATTACK | BUTTON_ADS));
        } else if let Some(c) = cl.send_frame(&UserCmd::default()) {
            hold.last = c.server_time;
        }
        for e in step(&mut sv, &q, &mut cl, now) {
            if let NetEvent::ServerCommand(tokens) = e {
                join.on_server_command(&tokens, &mut cl, now);
            }
        }
        if placed_at.is_none() && sv.script_log().iter().any(|l| l.contains("PROBE place")) {
            placed_at = Some(i);
        }
    }
    sv.script_log().to_vec()
}

fn probe_lines(text: impl IntoIterator<Item = String>) -> Vec<String> {
    text.into_iter()
        .map(|l| l.trim().to_string())
        .filter(|l| l.starts_with("PROBE"))
        .collect()
}

fn report(retail: &[String], ours: &[String]) {
    if std::env::var_os("SM_REPORT").is_some() {
        for l in ours {
            eprintln!("ours   {l}");
        }
        for l in retail {
            eprintln!("retail {l}");
        }
    }
}

/// Each `mtrace` line's phase, whether it stopped on the model, its
/// fraction and surface type.
fn traces(lines: &[String]) -> Vec<(String, bool, f32, String)> {
    let model = lines
        .iter()
        .find_map(|l| l.strip_prefix("PROBE model "))
        .and_then(|r| r.split(' ').next())
        .expect("the model was spawned")
        .to_string();
    lines
        .iter()
        .filter_map(|l| l.strip_prefix("PROBE mtrace "))
        .map(|l| {
            let t: Vec<&str> = l.split(' ').collect();
            (
                t[1].to_string(),
                t[2] == model,
                t[3].parse().unwrap(),
                t[4].to_string(),
            )
        })
        .collect()
}

fn compare_traces(fixture: &str, log: Vec<String>) {
    let retail = probe_lines(fixture.lines().map(str::to_string));
    let ours = probe_lines(log);
    report(&retail, &ours);
    let (r, o) = (traces(&retail), traces(&ours));
    assert_eq!(r.len(), 4, "{retail:?}");
    assert_eq!(o.len(), r.len(), "{ours:?}");
    for (r, o) in r.iter().zip(&o) {
        assert_eq!(
            (&o.0, o.1, &o.3),
            (&r.0, r.1, &r.3),
            "retail {r:?}, ours {o:?}"
        );
        assert!((o.2 - r.2).abs() < 0.005, "retail {r:?}, ours {o:?}");
    }
}

/// mp_carentan on 1.1d: the barrel stops the trace shown, hidden and
/// `notSolid`; deleted, the trace runs on to the world.
#[test]
fn a_script_model_stops_bullettrace_until_deleted() {
    let Some(fs) = vcod_common::testing::game_fs_11() else {
        return;
    };
    let log = ours(fs, "mp_carentan", false, false, 0);
    compare_traces(
        include_str!("fixtures/scriptmodel/mp_carentan-scriptmodel.txt"),
        log,
    );
}

/// The jeep's damage notifies in each phase of the barrel: shown, hidden,
/// deleted. The shown phase starts [`SIGHT_MS`] past the placement.
fn phase_counts(lines: &[String]) -> [usize; 3] {
    let time = |l: &str| -> i32 { l.split(' ').nth(2).unwrap().parse().unwrap() };
    let at = |p: &str| {
        lines
            .iter()
            .find(|l| l.starts_with("PROBE rphase") && l.split(' ').nth(3) == Some(p))
            .map(|l| time(l))
            .unwrap_or_else(|| panic!("no {p} phase in {lines:?}"))
    };
    let (shown, hidden, deleted) = (at("shown"), at("hidden"), at("deleted"));
    let mut out = [0; 3];
    for l in lines.iter().filter(|l| l.starts_with("PROBE vdamage")) {
        let t = time(l);
        if t < shown + SIGHT_MS {
            continue;
        }
        out[(t >= hidden) as usize + (t >= deleted) as usize] += 1;
    }
    out
}

/// mp_peaks on UO: the barrel stops every round shown and hidden; deleted,
/// the jeep takes them, 22 each.
#[test]
fn a_script_model_stops_rounds_until_deleted_on_uo() {
    let Some(fs) = vcod_common::testing::uo_fs() else {
        return;
    };
    let fixture = include_str!("fixtures/scriptmodel/mp_peaks-scriptmodel-uo.txt");
    let log = ours(fs, "mp_peaks", true, true, 9000);
    let retail = probe_lines(fixture.lines().map(str::to_string));
    let ours = probe_lines(log);
    report(&retail, &ours);
    let (rt, ot) = (traces(&retail), traces(&ours));
    assert_eq!(rt.len(), ot.len(), "{ours:?}");
    for (r, o) in rt.iter().zip(&ot) {
        assert_eq!(
            (&o.0, o.1, &o.3),
            (&r.0, r.1, &r.3),
            "retail {r:?}, ours {o:?}"
        );
        assert!((o.2 - r.2).abs() < 0.005, "retail {r:?}, ours {o:?}");
    }
    // Retail's deleted phase ran to the server's end; ours stops 3 s past
    // the delete, so only its first 3 s count.
    let deleted = |lines: &[String]| {
        lines
            .iter()
            .find(|l| l.starts_with("PROBE rphase") && l.ends_with("deleted"))
            .and_then(|l| l.split(' ').nth(2)?.parse::<i32>().ok())
            .expect("the barrel was deleted")
    };
    let rd = deleted(&retail);
    let retail_window: Vec<String> = retail
        .iter()
        .filter(|l| {
            !l.starts_with("PROBE vdamage")
                || l.split(' ')
                    .nth(2)
                    .and_then(|t| t.parse::<i32>().ok())
                    .is_some_and(|t| t < rd + 3000)
        })
        .cloned()
        .collect();
    let (r, o) = (phase_counts(&retail_window), phase_counts(&ours));
    assert_eq!((r[0], r[1]), (0, 0), "retail's barrel let rounds by: {r:?}");
    assert_eq!((o[0], o[1]), (0, 0), "ours' barrel let rounds by: {o:?}");
    assert!(
        r[2] > 10 && o[2].abs_diff(r[2]) * 5 <= r[2],
        "rounds past the deleted barrel in 3 s: retail {}, ours {}",
        r[2],
        o[2]
    );
    assert!(
        ours.iter()
            .filter(|l| l.starts_with("PROBE vdamage"))
            .all(|l| l.split(' ').nth(3) == Some("22")),
        "{ours:?}"
    );
}

/// Server time before ours' target connects, and how long after it the
/// shooter does: the fixtures' recipe, the target in slot 0 and the
/// barrel placed near retail's 12000. The target's idle clip runs on the
/// level time and his slot, and a frag off his bones reads its pose.
const TARGET_JOIN_MS: i64 = 7000;
const SHOOTER_LATE_MS: i64 = 3000;

/// The pair parts on mp_carentan (1.1d): `probe_part` set to `part`, an
/// axis target and then an allies shooter or thrower joined, run until `ms`
/// past the script's first `rphase` or `rest` line. The shooter holds the
/// trigger at the snapshot's view (`--probe-uo-weapon aim`) or, with
/// `throw`, runs `--probe-uo-weapon throw`'s two pullbacks. The `prone`
/// part's target holds prone (`--probe-fall-prone`). Each snapshot's
/// missiles follow the script's lines as the thrower's `VEH missile` lines.
fn ours_pair(part: &str, throw: bool, ms: i64) -> Vec<String> {
    ours_pair_on("mp_carentan", part, throw, ms, None)
}

/// [`ours_pair`] on `map`; with `prone_yaw` the target holds prone at that
/// world yaw on every cmd from the first, as `--probe-fall-prone` does (the
/// prone heading is the yaw the body lay down at).
fn ours_pair_on(
    map: &str,
    part: &str,
    throw: bool,
    ms: i64,
    prone_yaw: Option<f32>,
) -> Vec<String> {
    let fs = vcod_common::testing::game_fs_11().expect("checked by the caller");
    ours_pair_in(fs, map, part, throw, ms, prone_yaw)
}

/// [`ours_pair_on`] on `fs`'s install, its server speaking the install's
/// protocol.
fn ours_pair_in(
    fs: vcod_common::pk3::Pk3Fs,
    map: &str,
    part: &str,
    throw: bool,
    ms: i64,
    prone_yaw: Option<f32>,
) -> Vec<String> {
    let prone = prone_yaw.is_some();
    let mut missiles = Vec::new();
    let mut last_snap = None;
    let proto = vcod_server::server::install_protocol(&fs);
    let bsp_path = fs.resolve_map(map).expect("map in the paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(map, "probe_scriptmodel"), now);
    sv.set_protocol(proto);
    sv.overlay_script("maps/mp/gametypes/probe_scriptmodel", SCRIPT);
    sv.set_cvar("probe_part", part);
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    common::run_alone(&mut sv, &mut now, TARGET_JOIN_MS);
    let (qa, qb) = (
        Rc::new(RefCell::new(Queues::default())),
        Rc::new(RefCell::new(Queues::default())),
    );
    let mut ca: Option<NetClient<common::ClientEnd>> = None;
    let mut cb = NetClient::start_with_qport(common::ClientEnd(qb.clone()), now, 0x2002);
    let (mut ja, mut jb) = (
        Join::new("allies", "thompson_mp"),
        Join::new("axis", "kar98k_mp"),
    );
    let aim = || common::ProbeHold {
        aim: true,
        ..Default::default()
    };
    let (mut ha, mut hb) = (aim(), aim());
    if let Some(yaw) = prone_yaw {
        hb = common::ProbeHold {
            base: Some([0.0, yaw, 0.0]),
            ..Default::default()
        };
    }
    let mut selected: Option<u8> = None;
    let mut spawned: Option<i64> = None;
    let mut started: Option<i64> = None;
    let frame = Duration::from_millis(common::FRAME_MS as u64);
    let live = |cl: &NetClient<common::ClientEnd>| {
        cl.snapshots()
            .newest()
            .is_some_and(|s| s.ps.field_i32(cl.protocol(), "pm_type") == 0)
    };
    // One cmd round for a probe: the hold once it plays, a bare frame
    // before (the join's menus answer off it).
    let send = |cl: &mut NetClient<common::ClientEnd>,
                hold: &mut common::ProbeHold,
                buttons: u8,
                weapon: Option<u8>| {
        if live(cl) {
            let mut cmds = hold.cmds(cl, buttons);
            if let Some(w) = weapon {
                cmds.iter_mut().for_each(|c| c.weapon = w);
            }
            cl.send_cmds(&cmds);
        } else if let Some(c) = cl.send_frame(&UserCmd::default()) {
            hold.last = c.server_time;
        }
    };
    for i in 0..3000i64 {
        if started.is_some_and(|at| (i - at) * common::FRAME_MS > ms) {
            break;
        }
        now += frame;
        if ca.is_none() && i * common::FRAME_MS >= SHOOTER_LATE_MS {
            ca = Some(NetClient::start_with_qport(
                common::ClientEnd(qa.clone()),
                now,
                0x2001,
            ));
        }
        if spawned.is_none() && ca.as_ref().is_some_and(live) {
            spawned = Some(i);
        }
        let buttons = match (throw, spawned) {
            (_, None) => 0,
            (false, Some(_)) => BUTTON_ATTACK | BUTTON_ADS,
            (true, Some(at)) => {
                // `vehicle_probe::THROW`: settle 3 s, pull back 1 s, 6 s
                // thrown, pull back 1 s.
                let t = (i - at) * common::FRAME_MS;
                if (3000..4000).contains(&t) || (10_000..11_000).contains(&t) {
                    BUTTON_ATTACK
                } else {
                    0
                }
            }
        };
        if let Some(ca) = ca.as_mut() {
            send(ca, &mut ha, buttons, selected);
        }
        if prone && live(&cb) {
            let mut cmds = hb.cmds(&cb, 0);
            for c in &mut cmds {
                c.wbuttons |= vcod_common::net::msg::WBUTTON_PRONE;
                c.up = -127;
            }
            cb.send_cmds(&cmds);
        } else {
            send(&mut cb, &mut hb, 0, None);
        }
        for (addr, q) in [(common::ADDR, &qa), (common::ADDR_B, &qb)] {
            let pending: Vec<Vec<u8>> = q.borrow_mut().to_server.drain(..).collect();
            for p in pending {
                sv.handle_packet(addr, &p, now);
            }
        }
        sv.tick(now);
        for (to, p) in sv.take_outgoing() {
            let q = if to == common::ADDR { &qa } else { &qb };
            q.borrow_mut().to_client.push_back(p);
        }
        if let Some(ca) = ca.as_mut() {
            for e in ca.pump_at(now) {
                if let NetEvent::ServerCommand(tokens) = e {
                    if tokens.first().map(String::as_str) == Some("a") {
                        selected = tokens.get(1).and_then(|w| w.parse().ok());
                    }
                    ja.on_server_command(&tokens, ca, now);
                }
            }
        }
        for e in cb.pump_at(now) {
            if let NetEvent::ServerCommand(tokens) = e {
                jb.on_server_command(&tokens, &mut cb, now);
            }
        }
        if let Some(snap) = ca.as_ref().and_then(|c| c.snapshots().newest())
            && last_snap != Some(snap.message_num)
        {
            last_snap = Some(snap.message_num);
            let p = ca.as_ref().unwrap().protocol();
            for (num, e) in &snap.entities {
                let i = |n: &str| e.field_i32(p, n);
                if i("eType") == 4 {
                    missiles.push(format!(
                        "VEH {} missile {num} type {} bits {:#x} {:#x} {:#x} ground {}",
                        snap.server_time,
                        i("pos.trType"),
                        i("pos.trBase[0]"),
                        i("pos.trBase[1]"),
                        i("pos.trBase[2]"),
                        i("groundEntityNum"),
                    ));
                }
            }
        }
        if started.is_none()
            && sv
                .script_log()
                .iter()
                .any(|l| l.contains("PROBE rphase") || l.contains("PROBE rest"))
        {
            started = Some(i);
        }
    }
    let mut out = sv.script_log().to_vec();
    out.extend(missiles);
    out
}

fn line_time(l: &str) -> i32 {
    l.split(' ').nth(2).unwrap().parse().unwrap()
}

/// The target's hits in each phase of the rounds part's barrel: shown,
/// hidden, deleted, the shown phase from [`SIGHT_MS`] past its start and
/// the deleted one for its first 3 s.
fn target_phases(lines: &[String]) -> [usize; 3] {
    let at = |p: &str| {
        lines
            .iter()
            .find(|l| l.starts_with("PROBE rphase") && l.split(' ').nth(3) == Some(p))
            .map(|l| line_time(l))
            .unwrap_or_else(|| panic!("no {p} phase in {lines:?}"))
    };
    let (shown, hidden, deleted) = (at("shown"), at("hidden"), at("deleted"));
    let mut out = [0; 3];
    for l in lines.iter().filter(|l| l.starts_with("PROBE hit")) {
        let t = line_time(l);
        if t < shown + SIGHT_MS || t >= deleted + 3000 {
            continue;
        }
        out[(t >= hidden) as usize + (t >= deleted) as usize] += 1;
    }
    out
}

/// mp_carentan on 1.1d: a barrel 26 units in front of a player stops every
/// Thompson round shown and hidden, none passes or carries through; deleted,
/// the rounds land.
#[test]
fn a_script_model_stops_rounds_until_deleted() {
    if vcod_common::testing::game_fs_11().is_none() {
        return;
    }
    let fixture = include_str!("fixtures/scriptmodel/mp_carentan-scriptmodel-rounds.txt");
    let retail = probe_lines(fixture.lines().map(str::to_string));
    let ours = probe_lines(ours_pair("rounds", false, 9000));
    report(&retail, &ours);
    let (r, o) = (target_phases(&retail), target_phases(&ours));
    assert_eq!((r[0], r[1]), (0, 0), "retail's barrel let rounds by: {r:?}");
    assert_eq!((o[0], o[1]), (0, 0), "ours' barrel let rounds by: {o:?}");
    assert!(
        r[2] > 10 && o[2].abs_diff(r[2]) * 5 <= r[2],
        "rounds past the deleted barrel in 3 s: retail {}, ours {}",
        r[2],
        o[2]
    );
}

/// Each grenade's path, frame by frame from its first, and every hit as
/// (team, damage) in order.
#[allow(clippy::type_complexity)]
fn throws(lines: &[String]) -> (Vec<Vec<[f32; 3]>>, Vec<(String, i32)>) {
    let mut paths: Vec<(String, Vec<[f32; 3]>)> = Vec::new();
    for l in lines.iter().filter_map(|l| l.strip_prefix("PROBE gren ")) {
        let (num, rest) = l.split_once(' ').unwrap().1.split_once(' ').unwrap();
        let v: Vec<f32> = rest
            .trim_matches(|c| c == '(' || c == ')')
            .split(", ")
            .map(|x| x.parse().unwrap())
            .collect();
        match paths.iter_mut().find(|(n, _)| n == num) {
            Some((_, p)) => p.push([v[0], v[1], v[2]]),
            None => paths.push((num.to_string(), vec![[v[0], v[1], v[2]]])),
        }
    }
    let hits = lines
        .iter()
        .filter(|l| l.starts_with("PROBE hit"))
        .map(|l| {
            let t: Vec<&str> = l.split(' ').collect();
            (t[3].to_string(), t[5].parse().unwrap())
        })
        .collect();
    (paths.into_iter().map(|(_, p)| p).collect(), hits)
}

/// Retail's paths and hits against ours: every frame within 0.1 of
/// retail's (a unit once the explode snaps it), every hit within 2. A frag
/// off the target's bones reads his idle clip's pose: two retail runs part
/// by 0.01 there.
fn compare_throws(fixture: &str, ours: Vec<String>) {
    compare_throws_within(fixture, ours, 0.1);
}

/// [`compare_throws`] with `near` for the frames before the explode.
fn compare_throws_within(fixture: &str, ours: Vec<String>, near: f32) {
    let retail = probe_lines(fixture.lines().map(str::to_string));
    let ours = probe_lines(ours);
    report(&retail, &ours);
    let ((rp, rh), (op, oh)) = (throws(&retail), throws(&ours));
    assert_eq!(rp.len(), 2, "{retail:?}");
    assert_eq!(op.len(), rp.len(), "{ours:?}");
    for (g, (r, o)) in rp.iter().zip(&op).enumerate() {
        assert!(r.len().abs_diff(o.len()) <= 1, "grenade {g}: {r:?} {o:?}");
        for (f, (a, b)) in r.iter().zip(o).enumerate() {
            let snapped = a.iter().all(|v| v.fract() == 0.0);
            let tol = if snapped { 1.0f32.max(near) } else { near };
            assert!(
                (0..3).all(|i| (a[i] - b[i]).abs() <= tol),
                "grenade {g} frame {f}: retail {a:?}, ours {b:?}"
            );
        }
    }
    assert_eq!(oh.len(), rh.len(), "retail {rh:?}, ours {oh:?}");
    for (r, o) in rh.iter().zip(&oh) {
        assert!(
            r.0 == o.0 && (r.1 - o.1).abs() <= 2,
            "retail {rh:?}, ours {oh:?}"
        );
    }
}

/// mp_carentan on 1.1d: a frag thrown at a barrel bounces off its mesh;
/// thrown again with the barrel gone it strikes the target behind, whose
/// posed bones it bounces off at an eighth of its speed
/// (docs/research/cod11-combat.md 12.4).
#[test]
fn a_frag_bounces_off_a_script_model_and_a_players_bones() {
    if vcod_common::testing::game_fs_11().is_none() {
        return;
    }
    compare_throws(
        include_str!("fixtures/scriptmodel/mp_carentan-scriptmodel-grenade.txt"),
        ours_pair("grenade", true, 16_000),
    );
}

/// mp_carentan on 1.1d: a barrel between a resting frag and both players
/// keeps the whole blast off them; the same blast in the open hits both.
#[test]
fn a_script_model_shelters_players_from_a_blast() {
    if vcod_common::testing::game_fs_11().is_none() {
        return;
    }
    compare_throws(
        include_str!("fixtures/scriptmodel/mp_carentan-scriptmodel-blast.txt"),
        ours_pair("blast", true, 12_000),
    );
}

/// One snapshot's missile: trType, `trBase` and groundEntityNum.
type MissileRow = (i32, [f32; 3], i32);

/// Each grenade's `VEH missile` lines per snapshot, in order of first
/// appearance.
fn missile_grounds(lines: &[String]) -> Vec<Vec<MissileRow>> {
    let mut out: Vec<(String, Vec<MissileRow>)> = Vec::new();
    for l in lines
        .iter()
        .filter(|l| l.starts_with("VEH ") && l.contains(" missile "))
    {
        let t: Vec<&str> = l.split(' ').collect();
        let after = |k: &str, n: usize| t[t.iter().position(|x| *x == k).unwrap() + n];
        let bits = |n| {
            let raw = after("bits", n).trim_start_matches("0x");
            f32::from_bits(u32::from_str_radix(raw, 16).unwrap())
        };
        let row = (
            after("type", 1).parse().unwrap(),
            [bits(1), bits(2), bits(3)],
            after("ground", 1).parse().unwrap(),
        );
        let num = t[3].to_string();
        match out.iter_mut().find(|(n, _)| *n == num) {
            Some((_, v)) => v.push(row),
            None => out.push((num, vec![row])),
        }
    }
    out.into_iter().map(|(_, v)| v).collect()
}

/// How far ours' frames may sit from retail's off a prone body: his pose
/// strikes half a unit off retail's.
const PRONE_NEAR: f32 = 1.0;

/// mp_carentan on 1.1d: a frag thrown onto a prone player's back rests
/// there with his entity number as its ground (`G_BounceMissile` writes
/// `tr.entityNum`); the second rests on the floor where he lay
/// (docs/research/cod11-combat.md 12.4).
#[test]
fn a_frag_rests_on_a_prone_players_back() {
    if vcod_common::testing::game_fs_11().is_none() {
        return;
    }
    let fixture = include_str!("fixtures/scriptmodel/mp_carentan-scriptmodel-prone.txt");
    // The site's yaw (45) turned round, as the recipe's `--probe-fall-prone`.
    let ours = ours_pair_on("mp_carentan", "prone", true, 12_000, Some(225.0));
    let retail: Vec<String> = fixture.lines().map(str::to_string).collect();
    compare_throws_within(fixture, ours.clone(), PRONE_NEAR);
    let (r, o) = (missile_grounds(&retail), missile_grounds(&ours));
    if std::env::var_os("SM_REPORT").is_some() {
        eprintln!("retail {r:?}\nours   {o:?}");
    }
    assert_eq!(r.len(), 2);
    assert_eq!(o.len(), r.len(), "{o:?}");
    for (g, (r, o)) in r.iter().zip(&o).enumerate() {
        let rest = |v: &[MissileRow]| *v.last().unwrap();
        let (rr, or) = (rest(r), rest(o));
        assert_eq!(or.0, 0, "grenade {g} never came to rest: {o:?}");
        assert_eq!(
            or.2, rr.2,
            "grenade {g}'s ground: retail {rr:?}, ours {or:?}"
        );
    }
}

/// mp_brecourt on 1.1d: the prone part's second frag bounces down the
/// field's terrain and rolls to rest on it, each contact on retail's
/// snapshot with the world as its ground, every base within
/// [`TERRAIN_NEAR`] of retail's. The first rests on the target's back, his
/// number its ground; his pose on the slope puts it elsewhere on him than
/// retail's, so only its ground is compared (docs/research/cod11-combat.md
/// 12.4, 12.5).
#[test]
fn a_frag_rolls_down_terrain_like_retail() {
    if vcod_common::testing::game_fs_11().is_none() {
        return;
    }
    let fixture = include_str!("fixtures/scriptmodel/mp_brecourt-scriptmodel-prone.txt");
    // The site's yaw (90) turned round.
    let ours = ours_pair_on("mp_brecourt", "prone", true, 12_000, Some(270.0));
    let retail: Vec<String> = fixture.lines().map(str::to_string).collect();
    let (r, o) = (missile_grounds(&retail), missile_grounds(&ours));
    if std::env::var_os("SM_REPORT").is_some() {
        eprintln!("retail {r:?}\nours   {o:?}");
    }
    assert_eq!(r.len(), 2);
    assert_eq!(o.len(), r.len(), "{o:?}");
    assert_eq!(o[0].last().map(|g| g.2), r[0].last().map(|g| g.2));
    // Each distinct snapshot state: a resting frag repeats its last.
    let changes = |v: &[MissileRow]| {
        let mut out: Vec<MissileRow> = Vec::new();
        for &g in v {
            if out.last() != Some(&g) {
                out.push(g);
            }
        }
        out
    };
    let (rc, oc) = (changes(&r[1]), changes(&o[1]));
    assert_eq!(oc.len(), rc.len(), "retail {rc:?}\nours   {oc:?}");
    for (a, b) in rc.iter().zip(&oc) {
        let near = (0..3).all(|i| (a.1[i] - b.1[i]).abs() <= TERRAIN_NEAR);
        assert!(
            a.0 == b.0 && a.2 == b.2 && near,
            "retail {rc:?}\nours   {oc:?}"
        );
    }
}

/// The terrain roll's bases sit 2 or 3 float steps off retail's from the
/// second contact: the point clip is retail's (0x8052894), the bounce and
/// the arc are not ported to the x87's precision.
const TERRAIN_NEAR: f32 = 0.001;
