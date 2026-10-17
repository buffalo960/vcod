//! A leaning target's hit pose and a leaning shooter's rounds, ours against
//! retail 1.1d's. `client-probes/probe_leanhit` holds an allies shooter and
//! an axis target 256 units apart and drives both clients' stance, lean and
//! trigger through `PROBE_IN` lines (`--probe-uo-weapon direct`). Each
//! phase it crosses the target with a grid of `bulletTrace`s, whose cells
//! read the posed body (docs/research/cod11-combat.md 3, 16.4), then the
//! sighted springfield fires once from the shooter's eye, leaned in the
//! last three phases (2.1); the damage callback logs where it struck.
//!
//! `probe_bones 1` captures also link a mark to eleven bones before each
//! grid and log them with the body's origin, which splits the pose from
//! the boxes. One capture each from 1.1d, 1.5b and UO with them, and the
//! first 1.1d one without.
//!
//! `LH_REPORT=1` prints both logs and each phase's median cell. Needs
//! `COD_DIR` at a 1.1, 1.5 or UO install.

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

/// How far a grid cell may sit from retail's, units along the ray.
const CELL_NEAR: f32 = 0.5;
/// How far a bone or the body's origin may sit from retail's, units on
/// each axis; retail logs an offset to six figures.
const BONE_NEAR: f32 = 0.005;
/// How far a round's point may sit from retail's: the scoped idle sway
/// moves 0.45 degrees a second at most, and retail logs the point to the
/// hundredth.
const POINT_NEAR: f32 = 0.5;

/// Cells `(view, height, column)` per phase where ours may sit more than
/// [`CELL_NEAR`] off retail's, or hit where retail misses or the other way.
type Gaps = &'static [(&'static str, &'static [(char, u32, usize)])];

/// One retail capture of the probe and the join times its recipe gave.
struct Case {
    fixture: &'static str,
    /// Server time before ours' target connects, and how long after it the
    /// shooter does, set so ours' target spawns and takes each stance on
    /// retail's frames: his idle clip runs on the level time and his slot,
    /// and the setup's prone and stand wait for the shooter.
    join_ms: i64,
    late_ms: i64,
    /// Retail's origin z less ours, on every phase: the body's own
    /// offset, carried by every bone of a standing or crouched body and by
    /// none of a prone one, which rests on its own ground trace.
    stand_dz: f32,
    /// Phases whose round may strike another part than retail's.
    round_gaps: &'static [&'static str],
    gaps: Gaps,
}

fn gap(gaps: Gaps, phase: &str, view: &str, height: &str, column: usize) -> bool {
    let (v, h) = (view.chars().next().unwrap(), height.parse::<u32>().unwrap());
    gaps.iter()
        .any(|(p, cells)| *p == phase && cells.contains(&(v, h, column)))
}

/// The join times of the `probe_bones` captures.
const JOIN_11: i64 = 8550;
const LATE_11: i64 = 2950;
const JOIN_UO: i64 = 8600;
const LATE_UO: i64 = 3000;

/// 1.5b's target stands 0.0402 under ours after the hold's `setOrigin`
/// drop. Where that drop of a unit and a bit stops rides the cmd times:
/// ours lands anywhere from 0.013 over the spot to 0.125 under it as the
/// split moves a millisecond, and the 16, 16 and 18 ms split puts the
/// 1.1d and UO captures' target on retail's height to the bit. The net
/// probe logs no cmd times, so 1.5b's split is unknown
/// (docs/research/cod11-combat.md 3.6).
const STAND_DZ_15: f32 = -0.0402;
/// The cells 1.5b's standing height moves past [`CELL_NEAR`]: rays grazing
/// a box edge 0.04 units lower (with [`STAND_DZ_15`] every bone matches).
const CELL_GAPS_15: Gaps = &[
    ("s_left", &[('s', 40, 4)]),
    ("s_left_t_right", &[('s', 40, 2)]),
    ("s_right", &[('s', 58, 4)]),
    ("t_crouch_left", &[('s', 22, 5)]),
    ("t_crouch_right", &[('f', 22, 1)]),
];

/// 1.5b's and UO's crouched round under the target's left lean strikes
/// `torso_lower` 0.06 higher than ours, which takes `torso_upper` a unit
/// nearer: the ray grazes the two boxes' seam. The target's bones and
/// every grid cell of the phase match, so it is the shooter's ray, which
/// 1.1d's capture places right (combat doc 3.6).
const ROUND_GAPS_15_UO: &[&str] = &["t_crouch_left"];

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

fn ours(fs: vcod_common::pk3::Pk3Fs, c: &Case, bones: bool) -> Vec<String> {
    let proto = vcod_server::server::install_protocol(&fs);
    let map = "mp_carentan";
    let bsp_path = fs.resolve_map(map).expect("map in the paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(map, "probe_leanhit"), now);
    sv.set_protocol(proto);
    sv.overlay_script("maps/mp/gametypes/probe_leanhit", SCRIPT);
    if bones {
        sv.set_cvar("probe_bones", "1");
    }
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    common::run_alone(&mut sv, &mut now, c.join_ms);
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
        framed: true,
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
        if ca.is_none() && i * common::FRAME_MS >= c.late_ms {
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

/// Each `PROBE bone` and `PROBE body` row's offset off the target's spot,
/// keyed `(phase, tag, frame)`, the body's under the tag `origin`, the frame
/// counted from the phase's first row of that tag.
fn bone_rows(lines: &[String]) -> BTreeMap<(String, String, usize), [f32; 3]> {
    let mut out = BTreeMap::new();
    let mut seen: BTreeMap<(String, String), usize> = BTreeMap::new();
    for l in lines {
        let rest = match (l.strip_prefix("PROBE bone "), l.strip_prefix("PROBE body ")) {
            (Some(r), _) => r.to_string(),
            (_, Some(r)) => {
                let (t, rest) = r.split_once(' ').unwrap();
                let (phase, d) = rest.split_once(' ').unwrap();
                format!("{t} {phase} origin {d}")
            }
            _ => continue,
        };
        let t: Vec<&str> = rest.split(' ').collect();
        let n = t.len();
        let tag = t[2..n - 3].join(" ");
        let d: Vec<f32> = t[n - 3..].iter().map(|v| v.parse().unwrap()).collect();
        let frame = seen.entry((t[1].into(), tag.clone())).or_default();
        out.insert((t[1].into(), tag, *frame), [d[0], d[1], d[2]]);
        *frame += 1;
    }
    out
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

/// Each phase's median cell distance from retail's, for `LH_REPORT`.
fn report_cells(
    rg: &BTreeMap<(String, String, String), Vec<Option<f32>>>,
    og: &BTreeMap<(String, String, String), Vec<Option<f32>>>,
) {
    if std::env::var_os("LH_REPORT").is_none() {
        return;
    }
    let mut per: BTreeMap<&str, Vec<f32>> = BTreeMap::new();
    for (k, r) in rg {
        let d = per.entry(&k.0).or_default();
        for (r, o) in r.iter().zip(&og[k]) {
            if let (Some(r), Some(o)) = (r, o) {
                d.push((r - o).abs());
            }
        }
    }
    for (phase, mut d) in per {
        d.sort_by(f32::total_cmp);
        eprintln!("{phase:16} median cell {:.4}", d[d.len() / 2]);
    }
}

fn check(fs: vcod_common::pk3::Pk3Fs, c: &Case) {
    let retail = probe_lines(c.fixture.lines());
    let bones = retail.iter().any(|l| l.starts_with("PROBE bone "));
    let ours = ours(fs, c, bones);
    let ours = probe_lines(ours.iter().map(String::as_str));
    report(&retail, &ours);
    let site = |l: &[String]| l.iter().find(|l| l.starts_with("PROBE site")).cloned();
    let strip = |s: Option<String>| s.map(|s| s.splitn(4, ' ').nth(3).unwrap().to_string());
    assert_eq!(strip(site(&retail)), strip(site(&ours)), "the pair site");
    let mut bad = Vec::new();

    let (rb, ob) = (bone_rows(&retail), bone_rows(&ours));
    assert_eq!(
        rb.keys().collect::<Vec<_>>(),
        ob.keys().collect::<Vec<_>>(),
        "the bone rows"
    );
    for (k, r) in &rb {
        let mut o = ob[k];
        if k.1 == "origin" || !k.0.contains("prone") {
            o[2] += c.stand_dz;
        }
        if (0..3).any(|i| (r[i] - o[i]).abs() > BONE_NEAR) {
            bad.push(format!("bone {k:?}: retail {r:?} ours {o:?}"));
        }
    }

    let (rg, og) = (grids(&retail), grids(&ours));
    assert_eq!(
        rg.keys().collect::<Vec<_>>(),
        og.keys().collect::<Vec<_>>(),
        "the grid rows"
    );
    report_cells(&rg, &og);
    for (k, r) in &rg {
        let o = &og[k];
        let off = r.iter().zip(o).enumerate().any(|(i, (r, o))| {
            !gap(c.gaps, &k.0, &k.1, &k.2, i)
                && match (r, o) {
                    (Some(r), Some(o)) => (r - o).abs() > CELL_NEAR,
                    (None, None) => false,
                    _ => true,
                }
        });
        if off {
            bad.push(format!("cells {k:?}:\n  retail {r:?}\n  ours   {o:?}"));
        }
    }

    let (rh, oh) = (hits(&retail), hits(&ours));
    for (phase, r) in &rh {
        if c.round_gaps.contains(&phase.as_str()) {
            continue;
        }
        let o = oh.get(phase);
        let near = o.is_some_and(|o| {
            o.0 == r.0 && o.1 == r.1 && (0..3).all(|i| (o.2[i] - r.2[i]).abs() <= POINT_NEAR)
        });
        if !near {
            bad.push(format!("round {phase}: retail {r:?} ours {o:?}"));
        }
    }
    for phase in oh.keys().filter(|p| !rh.contains_key(*p)) {
        bad.push(format!(
            "round {phase}: ours hit {:?}, retail none",
            oh[phase]
        ));
    }
    assert!(bad.is_empty(), "off retail's:\n{}", bad.join("\n"));
}

#[test]
fn a_leaning_body_and_a_leaning_shooters_rounds_land_as_on_retail() {
    let Some(fs) = vcod_common::testing::game_fs_11() else {
        return;
    };
    check(
        fs,
        &Case {
            fixture: include_str!("fixtures/leanhit/mp_carentan-leanhit.txt"),
            join_ms: 8550,
            late_ms: 3450,
            stand_dz: 0.0,
            round_gaps: &[],
            gaps: &[],
        },
    );
}

#[test]
fn a_leaning_bodys_bones_sit_as_on_retail() {
    let Some(fs) = vcod_common::testing::game_fs_11() else {
        return;
    };
    check(
        fs,
        &Case {
            fixture: include_str!("fixtures/leanhit/mp_carentan-leanhit-bones.txt"),
            join_ms: JOIN_11,
            late_ms: LATE_11,
            stand_dz: 0.0,
            round_gaps: &[],
            gaps: &[],
        },
    );
}

/// The 1.5b install's paks, `None` elsewhere (a UO install's `main/` is
/// 1.5's, so this runs there too).
fn fs_15() -> Option<vcod_common::pk3::Pk3Fs> {
    let fs = vcod_common::testing::game_fs()?;
    let v6 = vcod_common::net::protocol::PROTOCOL_V6.version;
    (vcod_server::server::install_protocol(&fs).version == v6).then_some(fs)
}

#[test]
fn a_leaning_body_on_15_poses_as_on_retail() {
    let Some(fs) = fs_15() else {
        return;
    };
    check(
        fs,
        &Case {
            fixture: include_str!("fixtures/leanhit/mp_carentan-leanhit-1.5.txt"),
            join_ms: 8200,
            late_ms: 2800,
            stand_dz: STAND_DZ_15,
            round_gaps: ROUND_GAPS_15_UO,
            gaps: CELL_GAPS_15,
        },
    );
}

#[test]
fn a_leaning_body_on_uo_poses_as_on_retail() {
    let Some(fs) = vcod_common::testing::uo_fs() else {
        return;
    };
    check(
        fs,
        &Case {
            fixture: include_str!("fixtures/leanhit/mp_carentan-leanhit-uo.txt"),
            join_ms: JOIN_UO,
            late_ms: LATE_UO,
            stand_dz: 0.0,
            round_gaps: ROUND_GAPS_15_UO,
            gaps: &[],
        },
    );
}
