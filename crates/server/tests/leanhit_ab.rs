//! A leaning target's hit pose and a leaning shooter's rounds, ours against
//! retail 1.1d's. `client-probes/probe_leanhit` holds an allies shooter and
//! an axis target 256 units apart and drives both clients' stance, lean and
//! trigger through `PROBE_IN` lines (`--probe-uo-weapon direct`). Each
//! phase it crosses the target with a grid of `bulletTrace`s, whose cells
//! read the posed body (docs/research/cod11-combat.md 3, 16.4), then the
//! sighted springfield fires once from the shooter's eye, leaned in the
//! last three phases (2.1); the damage callback logs where it struck.
//!
//! `LH_REPORT=1` prints both logs. Needs `COD_DIR` at a 1.1 install.

mod common;

use common::{Join, Queues};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::msg::UserCmd;
use vcod_common::net::{NetClient, NetEvent};

const SCRIPT: &str =
    include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_leanhit.gsc");
const FIXTURE: &str = include_str!("fixtures/leanhit/mp_carentan-leanhit.txt");

/// Server time before ours' target connects, and how long after it the
/// shooter does: the fixture's recipe. The target's idle clip runs on the
/// level time and his slot.
const TARGET_JOIN_MS: i64 = 8550;
const SHOOTER_LATE_MS: i64 = 3450;

/// How far a grid cell may sit from retail's, units along the ray.
const CELL_NEAR: f32 = 0.5;
/// How far a round's point may sit from retail's: the scoped idle sway
/// moves 0.45 degrees a second at most, and retail logs the point to the
/// hundredth.
const POINT_NEAR: f32 = 0.5;

/// Cells `(view, height, column)` per phase where ours sits more than
/// [`CELL_NEAR`] off retail's, or hits where retail misses or the other way:
/// a ray grazing an arm, hand or head box edge-on, 0.5 to 3 units along it
/// (one 8.6), on standing and crouched bodies whether they lean or not.
/// Prone and the leaning right phase match everywhere. Not explained: the
/// idle clip's phase is not it (ours moved 50 ms reads 7 cells apart, not
/// these), and the boxes and controllers are those every other cell
/// matches (docs/research/cod11-combat.md 3.6).
#[allow(clippy::type_complexity)]
const CELL_GAPS: &[(&str, &[(char, u32, usize)])] = &[
    (
        "level",
        &[
            ('f', 40, 5),
            ('f', 46, 5),
            ('f', 52, 3),
            ('f', 64, 6),
            ('s', 4, 7),
            ('s', 46, 3),
            ('s', 64, 4),
        ],
    ),
    (
        "t_left",
        &[
            ('f', 4, 7),
            ('f', 40, 6),
            ('f', 58, 8),
            ('s', 4, 4),
            ('s', 4, 5),
            ('s', 46, 3),
            ('s', 46, 5),
            ('s', 58, 6),
            ('s', 64, 4),
            ('s', 64, 5),
        ],
    ),
    (
        "t_crouch",
        &[
            ('f', 22, 6),
            ('f', 22, 8),
            ('f', 28, 4),
            ('f', 40, 4),
            ('f', 40, 6),
            ('s', 22, 6),
            ('s', 22, 7),
        ],
    ),
    ("t_crouch_left", &[('f', 4, 3), ('f', 46, 8)]),
    (
        "t_crouch_right",
        &[
            ('f', 22, 5),
            ('f', 40, 2),
            ('f', 40, 4),
            ('f', 46, 3),
            ('s', 22, 4),
        ],
    ),
    ("t_stand", &[('f', 58, 4), ('f', 70, 4), ('s', 46, 6)]),
    (
        "s_left",
        &[
            ('f', 16, 4),
            ('f', 16, 7),
            ('f', 22, 7),
            ('f', 40, 5),
            ('s', 16, 6),
            ('s', 40, 4),
            ('s', 40, 9),
            ('s', 46, 3),
        ],
    ),
    ("s_right", &[('f', 52, 3), ('f', 70, 4), ('s', 46, 6)]),
    (
        "s_left_t_right",
        &[
            ('f', 10, 7),
            ('f', 40, 5),
            ('f', 46, 1),
            ('f', 46, 6),
            ('s', 28, 9),
            ('s', 34, 9),
            ('s', 40, 9),
        ],
    ),
];

fn gap(phase: &str, view: &str, height: &str, column: usize) -> bool {
    let (v, h) = (view.chars().next().unwrap(), height.parse::<u32>().unwrap());
    CELL_GAPS
        .iter()
        .any(|(p, cells)| *p == phase && cells.contains(&(v, h, column)))
}

/// The buttons, `wbuttons` and `up` of a `f "PROBE_IN ..."` command
/// (`vehicle_probe::direct_input`).
fn direct_input(tokens: &[String]) -> Option<(u8, u8, i8)> {
    if tokens.first().map(String::as_str) != Some("f") {
        return None;
    }
    let mut t = tokens.get(1)?.strip_prefix("PROBE_IN ")?.split_whitespace();
    Some((
        t.next()?.parse().ok()?,
        t.next()?.parse().ok()?,
        t.next()?.parse().ok()?,
    ))
}

/// Our run of the probe on mp_carentan until its `done` line.
fn ours() -> Vec<String> {
    let fs = vcod_common::testing::game_fs_11().expect("checked by the caller");
    let map = "mp_carentan";
    let bsp_path = fs.resolve_map(map).expect("map in the paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(map, "probe_leanhit"), now);
    sv.overlay_script("maps/mp/gametypes/probe_leanhit", SCRIPT);
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
        Join::new("allies", "springfield_mp"),
        Join::new("axis", "kar98k_mp"),
    );
    let aim = || common::ProbeHold {
        aim: true,
        ..Default::default()
    };
    let (mut ha, mut hb) = (aim(), aim());
    // Each client's last `PROBE_IN`, and the shooter's `a` select.
    let (mut ia, mut ib) = ((0u8, 0u8, 0i8), (0u8, 0u8, 0i8));
    let mut selected: Option<u8> = None;
    let mut spawned: Vec<&str> = Vec::new();
    let frame = Duration::from_millis(common::FRAME_MS as u64);
    let live = |cl: &NetClient<common::ClientEnd>| {
        cl.snapshots()
            .newest()
            .is_some_and(|s| s.ps.field_i32(cl.protocol(), "pm_type") == 0)
    };
    let send = |cl: &mut NetClient<common::ClientEnd>,
                hold: &mut common::ProbeHold,
                input: (u8, u8, i8),
                weapon: Option<u8>| {
        if live(cl) {
            let mut cmds = hold.cmds(cl, input.0);
            for c in &mut cmds {
                c.wbuttons = input.1;
                c.up = input.2;
                if let Some(w) = weapon {
                    c.weapon = w;
                }
            }
            cl.send_cmds(&cmds);
        } else if let Some(c) = cl.send_frame(&UserCmd::default()) {
            hold.last = c.server_time;
        }
    };
    for i in 0..4000i64 {
        if sv.script_log().iter().any(|l| l.contains("PROBE done")) {
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
        if std::env::var_os("LH_REPORT").is_some() {
            for (name, cl) in [("shooter", ca.as_ref()), ("target", Some(&cb))] {
                if let Some(cl) = cl
                    && live(cl)
                    && !spawned.contains(&name)
                {
                    spawned.push(name);
                    let s = cl.snapshots().newest().unwrap();
                    let p = cl.protocol();
                    eprintln!(
                        "{name} live at {} origin {:?} view {:?}",
                        s.server_time,
                        s.ps.origin(p),
                        s.ps.viewangles(p)
                    );
                }
            }
        }
        if let Some(ca) = ca.as_mut() {
            send(ca, &mut ha, ia, selected);
        }
        send(&mut cb, &mut hb, ib, None);
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
                    if let Some(input) = direct_input(&tokens) {
                        ia = input;
                    }
                    ja.on_server_command(&tokens, ca, now);
                }
            }
        }
        for e in cb.pump_at(now) {
            if let NetEvent::ServerCommand(tokens) = e {
                if let Some(input) = direct_input(&tokens) {
                    ib = input;
                }
                jb.on_server_command(&tokens, &mut cb, now);
            }
        }
    }
    sv.script_log().to_vec()
}

fn probe_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    lines
        .into_iter()
        .map(str::trim)
        .filter(|l| l.starts_with("PROBE"))
        .map(str::to_string)
        .collect()
}

/// Each phase's grid rows, keyed `(phase, view, height)`.
fn grids(lines: &[String]) -> BTreeMap<(String, String, String), Vec<Option<f32>>> {
    let mut out = BTreeMap::new();
    for l in lines {
        let t: Vec<&str> = l.split(' ').collect();
        if t[1] != "grid" {
            continue;
        }
        let cells = t[6..].iter().map(|c| c.parse().ok()).collect();
        out.insert((t[3].into(), t[4].into(), t[5].into()), cells);
    }
    out
}

/// The round each phase landed: location, damage and point.
fn hits(lines: &[String]) -> BTreeMap<String, (String, String, [f32; 3])> {
    let mut out = BTreeMap::new();
    for l in lines {
        let Some(rest) = l.strip_prefix("PROBE hit ") else {
            continue;
        };
        let t: Vec<&str> = rest.splitn(8, ' ').collect();
        let point: Vec<f32> = t[7]
            .trim_start_matches("point ")
            .trim_matches(|c| c == '(' || c == ')')
            .split(", ")
            .map(|c| c.parse().unwrap())
            .collect();
        out.insert(
            t[1].to_string(),
            (
                t[6].to_string(),
                t[4].to_string(),
                [point[0], point[1], point[2]],
            ),
        );
    }
    out
}

fn report(retail: &[String], ours: &[String]) {
    if std::env::var_os("LH_REPORT").is_some() {
        eprintln!("retail:\n{}\nours:\n{}", retail.join("\n"), ours.join("\n"));
    }
}

#[test]
fn a_leaning_body_and_a_leaning_shooters_rounds_land_as_on_retail() {
    if vcod_common::testing::game_fs_11().is_none() {
        return;
    }
    let retail = probe_lines(FIXTURE.lines());
    let ours = ours();
    let ours = probe_lines(ours.iter().map(String::as_str));
    report(&retail, &ours);
    let site = |l: &[String]| l.iter().find(|l| l.starts_with("PROBE site")).cloned();
    let strip = |s: Option<String>| s.map(|s| s.splitn(4, ' ').nth(3).unwrap().to_string());
    assert_eq!(strip(site(&retail)), strip(site(&ours)), "the pair site");

    let (rg, og) = (grids(&retail), grids(&ours));
    assert_eq!(
        rg.keys().collect::<Vec<_>>(),
        og.keys().collect::<Vec<_>>(),
        "the grid rows"
    );
    let mut bad = Vec::new();
    for (k, r) in &rg {
        let o = &og[k];
        let off = r.iter().zip(o).enumerate().any(|(i, (r, o))| {
            !gap(&k.0, &k.1, &k.2, i)
                && match (r, o) {
                    (Some(r), Some(o)) => (r - o).abs() > CELL_NEAR,
                    (None, None) => false,
                    _ => true,
                }
        });
        if off {
            bad.push(format!("{k:?}:\n  retail {r:?}\n  ours   {o:?}"));
        }
    }
    assert!(
        bad.is_empty(),
        "grid rows off retail's:\n{}",
        bad.join("\n")
    );

    let (rh, oh) = (hits(&retail), hits(&ours));
    let mut bad = Vec::new();
    for (phase, r) in &rh {
        let o = oh.get(phase);
        let near = o.is_some_and(|o| {
            o.0 == r.0 && o.1 == r.1 && (0..3).all(|i| (o.2[i] - r.2[i]).abs() <= POINT_NEAR)
        });
        if !near {
            bad.push(format!("{phase}: retail {r:?} ours {o:?}"));
        }
    }
    for phase in oh.keys().filter(|p| !rh.contains_key(*p)) {
        bad.push(format!("{phase}: ours hit {:?}, retail none", oh[phase]));
    }
    assert!(bad.is_empty(), "rounds off retail's:\n{}", bad.join("\n"));
}
