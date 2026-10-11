//! `func_bobbing` and `func_pendulum` against the retail captures.
//!
//! `movers/mp_carentan-dm-sine.txt` is retail's `games_mp.log` under
//! `client-probes/probe_bobbing.gsc` with five script-spawned sine movers,
//! up to the frame the server dropped the map; `-sine-map.txt` is the same
//! probe on mp_carentan with two bombzone brush models rewritten into a
//! map-placed bobbing and pendulum ([`sine_bsp`]); `-sine-wire.txt` is what
//! a client was sent of both. Ours runs the probe as its gametype with one
//! client joined. `docs/research/cod11-movers.md` section 18 reads the runs.
//!
//! `SINE_REPORT=1` prints every compared row.
//!
//! Needs `COD_DIR`; without the paks the test returns early.

mod common;

use common::{ADDR, ClientEnd, FRAME_MS, Join, Queues, holding, step_at};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::protocol::PROTOCOL_V1;
use vcod_common::net::trajectory::Trajectory;
use vcod_common::net::{NetClient, NetEvent};

const MAP: &str = "mp_carentan";
const SPAWNED: &str = "tests/fixtures/movers/mp_carentan-dm-sine.txt";
const PLACED: &str = "tests/fixtures/movers/mp_carentan-dm-sine-map.txt";
const WIRE: &str = "tests/fixtures/movers/mp_carentan-dm-sine-wire.txt";
const PROBE: &str =
    include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_bobbing.gsc");

/// `logPrint`'s two decimals.
const TOL: f32 = 0.011;

fn report() -> bool {
    std::env::var("SINE_REPORT").is_ok_and(|v| v == "1")
}

/// What one entity was first sent as: `eType`, `solid`, `index`,
/// `eFlags`, then `pos` and `apos`.
type Sent = ([i32; 4], Trajectory, Trajectory);

struct Ours {
    log: Vec<String>,
    /// Whether the level was dropped.
    dropped: bool,
    wire: BTreeMap<u32, Sent>,
}

/// mp_carentan with `*5` a `func_bobbing` and `*6` a `func_pendulum`: the
/// fixture header's keys, the rewritten entity lump appended to the file
/// and lump 29's `(length, offset)` entry pointed at it.
fn sine_bsp(mut bytes: Vec<u8>) -> Vec<u8> {
    let dir = 8 + 29 * 8;
    let word = |b: &[u8], at: usize| i32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize;
    let (length, offset) = (word(&bytes, dir), word(&bytes, dir + 4));
    let mut lump = String::from_utf8_lossy(&bytes[offset..offset + length]).into_owned();
    let old = |n: u32| {
        format!(
            "\"script_gameobjectname\" \"bombzone\"\n\"classname\" \"script_brushmodel\"\n\"model\" \"*{n}\""
        )
    };
    let new5 = "\"classname\" \"func_bobbing\"\n\"spawnflags\" \"1\"\n\"speed\" \"3\"\n\
        \"height\" \"40\"\n\"phase\" \"100\"\n\"origin\" \"16 32 8\"\n\"model\" \"*5\"";
    let new6 =
        "\"classname\" \"func_pendulum\"\n\"speed\" \"20\"\n\"phase\" \"100\"\n\"model\" \"*6\"";
    for (n, new) in [(5, new5), (6, new6)] {
        assert_eq!(lump.matches(&old(n)).count(), 1, "*{n}");
        lump = lump.replace(&old(n), new);
    }
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
    let at = bytes.len();
    bytes.extend_from_slice(lump.trim_end_matches('\0').as_bytes());
    bytes.push(0);
    let len = (bytes.len() - at) as i32;
    bytes[dir..dir + 4].copy_from_slice(&len.to_le_bytes());
    bytes[dir + 4..dir + 8].copy_from_slice(&(at as i32).to_le_bytes());
    bytes
}

/// The probe on ours, the map's bytes through `patch`, with one client
/// joined, for `frames` server frames or until the level drops.
fn run_ours(patch: fn(Vec<u8>) -> Vec<u8>, frames: usize) -> Option<Ours> {
    let mut fs = vcod_common::testing::game_fs()?;
    let bsp_path = fs.resolve_map(MAP).expect("map in the mounted paks");
    let bytes = patch(fs.read(&bsp_path).unwrap());
    fs.overlay(&bsp_path, bytes.clone());
    let bsp = vcod_common::bsp::parse(&bytes).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(MAP, "probe_bobbing"), now);
    sv.overlay_script("maps/mp/gametypes/probe_bobbing", PROBE);
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    let q = Rc::new(RefCell::new(Queues::default()));
    let mut cl = NetClient::start_with_qport(ClientEnd(q.clone()), now, 0x2001);
    let mut join = Join::new("allies", "m1carbine_mp");
    let mut log = Vec::new();
    let mut wire = BTreeMap::new();
    let mut dropped = false;
    for _ in 0..frames {
        now += Duration::from_millis(FRAME_MS as u64);
        cl.send_frame(&holding(&cl));
        for e in step_at(&mut sv, ADDR, &q, &mut cl, now) {
            if let NetEvent::ServerCommand(tokens) = e {
                join.on_server_command(&tokens, &mut cl, now);
            }
        }
        // A dropped level's log outlives it.
        log = sv.script_log().to_vec();
        if sv.is_killed() {
            dropped = true;
            break;
        }
        if let Some(s) = cl.snapshots().newest() {
            let p = &PROTOCOL_V1;
            for (&n, e) in &s.entities {
                if matches!(e.field_i32(p, "eType"), 5) {
                    wire.entry(n).or_insert((
                        ["eType", "solid", "index", "eFlags"].map(|f| e.field_i32(p, f)),
                        Trajectory::read(e, p, "pos"),
                        Trajectory::read(e, p, "apos"),
                    ));
                }
            }
        }
    }
    if report() {
        println!(
            "aborts {:?}\nlog head {:?}",
            sv.script_aborts(),
            &log[..log.len().min(4)]
        );
    }
    Some(Ours { log, dropped, wire })
}

/// The time of a `PROBE f` line.
fn f_time(line: &str) -> Option<i32> {
    let rest = &line[line.find("PROBE f ")? + 8..];
    rest.split_whitespace().next()?.parse().ok()
}

/// Every `(x, y, z)` in a line.
fn vectors(line: &str) -> Vec<[f32; 3]> {
    line.split('(')
        .skip(1)
        .filter_map(|s| {
            let v: Vec<f32> = s
                .split(')')
                .next()?
                .split(',')
                .filter_map(|t| t.trim().parse().ok())
                .collect();
            (v.len() == 3).then(|| [v[0], v[1], v[2]])
        })
        .collect()
}

/// The `PROBE ent` lines without their vectors, and the vectors, by tag.
fn ents(lines: &[&str]) -> BTreeMap<String, (String, Vec<[f32; 3]>)> {
    lines
        .iter()
        .filter_map(|l| {
            let rest = &l[l.find("PROBE ent ")? + 10..];
            let words: String = rest.split(" (").next()?.to_string();
            let tag = words.split_whitespace().next()?.to_string();
            Some((tag, (words, vectors(rest))))
        })
        .collect()
}

/// The `PROBE f` lines' vectors by time.
fn frames(lines: &[&str]) -> BTreeMap<i32, Vec<[f32; 3]>> {
    lines
        .iter()
        .filter_map(|l| Some((f_time(l)?, vectors(l))))
        .collect()
}

fn far(a: &[[f32; 3]], b: &[[f32; 3]]) -> bool {
    a.len() != b.len()
        || a.iter()
            .zip(b)
            .any(|(x, y)| (0..3).any(|i| (x[i] - y[i]).abs() > TOL))
}

/// The probe lines of both runs agree: the spawn lines exactly but for the
/// vectors, every frame retail logged within `logPrint`'s rounding.
fn compare(retail_text: &str, ours: &Ours) -> Vec<String> {
    let retail: Vec<&str> = retail_text
        .lines()
        .filter(|l| !l.starts_with('#'))
        .collect();
    let mine: Vec<&str> = ours.log.iter().map(String::as_str).collect();
    let mut bad = Vec::new();
    let (r, o) = (ents(&retail), ents(&mine));
    if report() {
        println!("retail {r:?}\nours   {o:?}");
    }
    if r.len() != o.len() {
        bad.push(format!("ents: retail {r:?} ours {o:?}"));
    }
    for (tag, rv) in &r {
        match o.get(tag) {
            Some(ov) if ov.0 == rv.0 && !far(&ov.1, &rv.1) => {}
            ov => bad.push(format!("ent {tag}: retail {rv:?} ours {ov:?}")),
        }
    }
    let (r, o) = (frames(&retail), frames(&mine));
    for (t, rv) in &r {
        let ov = o.get(t);
        if report() {
            println!("{t}: retail {rv:?} ours {ov:?}");
        }
        if ov.is_none_or(|ov| far(ov, rv)) {
            bad.push(format!("f {t}: retail {rv:?} ours {ov:?}"));
        }
    }
    bad
}

/// The `[spawned]` or `[map]` half of the wire fixture, by entity number.
fn retail_wire(section: &str) -> BTreeMap<u32, Sent> {
    let text = std::fs::read_to_string(WIRE).unwrap();
    let mut out: BTreeMap<u32, Sent> = BTreeMap::new();
    let mut ride: BTreeMap<u32, [i32; 3]> = BTreeMap::new();
    let mut inside = false;
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        if line.starts_with('[') {
            inside = line == format!("[{section}]");
            continue;
        }
        if !inside {
            continue;
        }
        let kv = |key: &str| -> &str {
            line.split_whitespace()
                .find_map(|w| w.strip_prefix(&format!("{key}=")))
                .unwrap()
        };
        if line.starts_with("RIDE_ENT") {
            let hex = |s: &str| {
                i32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap() as u32 as i32
            };
            ride.insert(
                kv("num").parse().unwrap(),
                [
                    hex(kv("solid")),
                    kv("index").parse().unwrap(),
                    hex(kv("eFlags")),
                ],
            );
            continue;
        }
        let w: Vec<&str> = line.split_whitespace().collect();
        let num: u32 = w[1].parse().unwrap();
        let etype: i32 = w[5].parse().unwrap();
        let traj = |at: usize| {
            let int = |k: usize| w[at + k].parse::<i32>().unwrap();
            let vec = |k: usize| {
                let v: Vec<f32> = w[at + k]
                    .trim_matches(['[', ']'])
                    .split(',')
                    .map(|t| t.parse().unwrap())
                    .collect();
                glam::Vec3::new(v[0], v[1], v[2])
            };
            Trajectory {
                tr_type: int(2),
                tr_time: int(4),
                tr_duration: int(6),
                base: vec(8),
                delta: vec(10),
            }
        };
        let r = ride[&num];
        out.insert(num, ([etype, r[0], r[1], r[2]], traj(6), traj(17)));
    }
    out
}

/// Five script-spawned sine movers: spawned at the world origin whatever
/// the argument, swinging along the axis the spawnflags pick (none for the
/// pendulum), sent as `eType` 5 on model 0, and the map dropped on the frame
/// after 4000, one period of the bobbing on.
#[test]
fn spawned_sine_movers_swing_and_drop_the_map_like_retail() {
    let Some(ours) = run_ours(|b| b, 200) else {
        eprintln!("COD_DIR unset or has no main/: skipping");
        return;
    };
    let text = std::fs::read_to_string(SPAWNED).unwrap();
    let bad = compare(&text, &ours);
    assert!(bad.is_empty(), "{}", bad.join("\n"));
    // Retail's last line is the frame it dropped on.
    let last = text.lines().filter_map(f_time).max();
    assert_eq!(last, Some(4000));
    assert!(text.contains("PROBE_ERROR Reached_BinaryMover: bad moverState"));
    assert!(ours.dropped);
    assert_eq!(ours.log.iter().filter_map(|l| f_time(l)).max(), last);
    // The wire run spawned one bobbing (299) and one pendulum (300); in
    // this run they are b0 (299) and p0 (302).
    let retail = retail_wire("spawned");
    if report() {
        println!("retail {retail:?}\nours {:?}", ours.wire);
    }
    assert_eq!(ours.wire.get(&299), retail.get(&299), "the bobbing");
    assert_eq!(ours.wire.get(&302), retail.get(&300), "the pendulum");
}

/// The map-placed pair: their keys read, the `origin` key dropped, the
/// pendulum's period off its model's floor, and both sent on their own
/// models.
#[test]
fn map_placed_sine_movers_read_their_keys_like_retail() {
    let Some(ours) = run_ours(sine_bsp, 900) else {
        eprintln!("COD_DIR unset or has no main/: skipping");
        return;
    };
    let text = std::fs::read_to_string(PLACED).unwrap();
    let bad = compare(&text, &ours);
    assert!(bad.is_empty(), "{}", bad.join("\n"));
    assert!(!ours.dropped);
    let retail = retail_wire("map");
    if report() {
        println!("retail {retail:?}\nours {:?}", ours.wire);
    }
    for n in [177, 178] {
        assert_eq!(ours.wire.get(&n), retail.get(&n), "entity {n}");
    }
}
