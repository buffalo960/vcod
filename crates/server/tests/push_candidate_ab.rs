//! `G_MoverPush`'s candidate trace against the retail capture
//! `movers/mp_carentan-dm-pushcand.txt` (its header carries the recipe):
//! `client-probes/probe_pushcand.gsc` runs the slab into one player standing
//! clear, two stacked on one spot, two overlapping by half a box and one
//! sunk into the floor. The trace is a full `trap_Trace` that must name the
//! pusher, so the world or another player met first keeps a player off the
//! list and the slab passes through him (docs/research/cod11-movers.md 12,
//! "The candidate trace"). A spectator joins first, as on retail, which
//! keeps `StuckInClient` off and the stacked pair stacked. Where the slab
//! names itself first a pair it cannot place stalls it on every frame,
//! the last frame of its move included (section 12, "A blocked mover").
//!
//! Needs `COD_DIR`; without the paks the test returns early.

mod common;

use common::{ADDR, ADDR_B, ADDR_C, Queues, holding, step_trio};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

const MAP: &str = "mp_carentan";
const FIXTURE: &str = "tests/fixtures/movers/mp_carentan-dm-pushcand.txt";

/// Rows whose z the gate does not compare, `(tag, slot)`, each a fall the
/// push does not decide:
/// - a player standing on the street: where a `setOrigin` drop rests within
///   the ground trace's quarter unit rides the client's cmd times (movers
///   doc 21), and retail's probe client logged none (-31.81 to -31.84
///   there, -31.87 here);
/// - the stacked and overlapping pairs once the slab stalls on them: a
///   blocked push leaves them unlinked until their next cmd, so the first
///   to move falls a cmd's worth through the other (retail -30.10 to
///   -30.46, and the half pair's second down on the street), where ours
///   clips a body against every playing sim whether it is in the area
///   tree or not.
const Z_GAPS: &[(&str, &str)] = &[
    ("set_alone", "1"),
    ("set_alone", "2"),
    ("f_alone", "1"),
    ("f_alone", "2"),
    ("after_alone", "1"),
    ("after_alone", "2"),
    ("cleared", "1"),
    ("cleared", "2"),
    ("set_sunk", "2"),
    ("f_sunk", "2"),
    ("after_sunk", "2"),
    ("f_stack", "2"),
    ("after_stack", "1"),
    ("after_stack", "2"),
    ("f_half", "2"),
    ("after_half", "2"),
];

/// Every logged origin, `tag slot (x, y, z)`, the slab's as slot `bz`, with
/// the frame tags' level times dropped (ours joins on its own clock) and z
/// cut from the [`Z_GAPS`] rows.
fn rows<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<String> {
    lines
        .filter_map(|l| l.find("PROBE ").map(|i| &l[i + 6..]))
        .filter_map(|l| {
            let mut it = l.splitn(3, ' ');
            let tag = it.next()?;
            let slot = it.next()?;
            let at = it.next()?;
            if !at.starts_with('(') {
                return None;
            }
            let tag = match tag.strip_prefix("f_") {
                Some(t) => format!("f_{}", t.rsplit_once('_')?.0),
                None => tag.to_string(),
            };
            let at = if Z_GAPS.contains(&(tag.as_str(), slot)) {
                at.rsplit_once(", ")?.0.to_string()
            } else {
                at.to_string()
            };
            Some(format!("{tag} {slot} {at}"))
        })
        .collect()
}

#[test]
fn a_push_lists_only_players_its_trace_names_the_pusher_for() {
    let Some(fs) = vcod_common::testing::game_fs_11() else {
        eprintln!("COD_DIR unset or has no main/: skipping");
        return;
    };
    let bsp_path = fs.resolve_map(MAP).expect("map in the mounted paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(MAP, "probe_pushcand"), now);
    sv.overlay_script(
        "maps/mp/gametypes/probe_pushcand",
        include_str!("../../gsc/tests/fixtures/semantics/client-probes/probe_pushcand.gsc"),
    );
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    let q: [Rc<RefCell<Queues>>; 3] = Default::default();
    let [mut cs, mut ca, mut cb] = common::join_trio(
        &mut sv,
        [&q[0], &q[1], &q[2]],
        &mut now,
        [
            ("spectator", "m1carbine_mp"),
            ("allies", "m1carbine_mp"),
            ("allies", "m1carbine_mp"),
        ],
    );
    for _ in 0..1600 {
        now += Duration::from_millis(50);
        for c in [&mut cs, &mut ca, &mut cb] {
            c.send_frame(&holding(c));
        }
        step_trio(
            &mut sv,
            (ADDR, &q[0], &mut cs),
            (ADDR_B, &q[1], &mut ca),
            (ADDR_C, &q[2], &mut cb),
            now,
        );
        if sv.script_log().iter().any(|l| l.contains("PROBE done")) {
            break;
        }
    }
    assert_eq!(sv.script_aborts(), Vec::<String>::new());
    let ours = rows(sv.script_log().iter().map(String::as_str));
    let retail_text = std::fs::read_to_string(FIXTURE).unwrap();
    let retail = rows(retail_text.lines());
    assert!(retail.len() > 50, "the fixture has no rows");
    assert_eq!(ours, retail, "\nours:\n{}", ours.join("\n"));
}
