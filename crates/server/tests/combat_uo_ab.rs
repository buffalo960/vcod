//! The retail United Offensive 1.51b combat capture
//! `fixtures/playerstate/mp_cassino-dm-combat-uo.txt` (`--save-combat
//! --capture-tag uo` beside `COD_VERSION=uo tools/run_server.sh`, axis with
//! the kar98k) replayed on a protocol 22 server: per step, the shots fired
//! and the values the weapon and torso channels took (docs/research/
//! coduo-diff.md 20).
//!
//! Needs `COD_DIR` at a UO install; without one it returns early.

mod common;

use std::collections::BTreeSet;
use vcod_common::net::event_ids::{EV_FIRE_WEAPON, EV_FIRE_WEAPON_LASTSHOT};
use vcod_common::net::protocol::PROTOCOL_V22;

const FIXTURE: &str = "tests/fixtures/playerstate/mp_cassino-dm-combat-uo.txt";

/// The walk to the firing spot: where it ends depends on where the spawn
/// put the player, so only the steps after it compare.
const SKIPPED: &[&str] = &["advance"];

/// What one step did on one side.
#[derive(Debug, PartialEq)]
struct Summary {
    shots: usize,
    weaponstate: BTreeSet<i32>,
    weap_anim: BTreeSet<i32>,
    torso_anim: BTreeSet<i32>,
}

/// Shots out of an event ring sampled once a snapshot: every event the
/// sequence gained, read out of its slot (four, overwritten as it fires).
fn shots(samples: &[(i32, [i32; 4])]) -> usize {
    samples
        .windows(2)
        .map(|w| {
            let (from, to) = (w[0].0, w[1].0);
            (from..to)
                .filter(|s| {
                    matches!(
                        w[1].1[(s & 3) as usize],
                        EV_FIRE_WEAPON | EV_FIRE_WEAPON_LASTSHOT
                    )
                })
                .count()
        })
        .sum()
}

/// The `weapAnim` values of `(weaponstate, weapAnim)` samples, less the lone
/// idle sample between a shot and its rechamber, which the cmd grid decides
/// (`common::rechamber_bridge`).
fn bridged(samples: &[(i32, i32)]) -> BTreeSet<i32> {
    let states: Vec<i32> = samples.iter().map(|s| s.0).collect();
    samples
        .iter()
        .enumerate()
        .filter(|(i, _)| !common::rechamber_bridge(&states, *i))
        .map(|(_, s)| s.1)
        .collect()
}

#[test]
fn the_kar98k_fires_and_reloads_as_on_uo() {
    let Some(fs) = vcod_common::testing::uo_fs() else {
        return;
    };
    let text = std::fs::read_to_string(FIXTURE).unwrap();
    let weapon = vcod_server::defs::UO.weapon_index("kar98k_mp").unwrap() as u8;
    let steps = common::parse_fixture(&text, weapon);
    let ours = common::replay_as(
        &PROTOCOL_V22,
        "mp_cassino",
        "dm",
        &steps,
        ("axis", "kar98k_mp"),
        fs,
        None,
    );
    let p = &PROTOCOL_V22;
    let mut diffs = Vec::new();
    let mut fired = 0;
    for (step, samples) in steps.iter().zip(&ours) {
        if SKIPPED.contains(&step.label.as_str()) {
            continue;
        }
        let retail = Summary {
            shots: shots(
                &step
                    .trace
                    .iter()
                    .map(|t| (t.event_sequence, t.events))
                    .collect::<Vec<_>>(),
            ),
            weaponstate: step.trace.iter().map(|t| t.weaponstate).collect(),
            weap_anim: bridged(
                &step
                    .trace
                    .iter()
                    .map(|t| (t.weaponstate, t.weap_anim))
                    .collect::<Vec<_>>(),
            ),
            torso_anim: step.trace.iter().map(|t| t.torso_anim & 511).collect(),
        };
        let field = |s: &common::Sample, n: &str| s.ps.field_i32(p, n);
        let ring: Vec<(i32, [i32; 4])> = samples
            .iter()
            .map(|s| {
                let ev = |k: usize| (p.event_to_v1)(field(s, &format!("events[{k}]")));
                (field(s, "eventSequence"), [ev(0), ev(1), ev(2), ev(3)])
            })
            .collect();
        let ours = Summary {
            shots: shots(&ring),
            weaponstate: samples.iter().map(|s| field(s, "weaponstate")).collect(),
            weap_anim: bridged(
                &samples
                    .iter()
                    .map(|s| (field(s, "weaponstate"), field(s, "weapAnim")))
                    .collect::<Vec<_>>(),
            ),
            torso_anim: samples
                .iter()
                .map(|s| field(s, "torsoAnim") & 511)
                .collect(),
        };
        fired += retail.shots;
        if retail != ours {
            diffs.push(format!(
                "{}: retail {retail:?}\n    ours {ours:?}",
                step.label
            ));
        }
    }
    assert!(
        diffs.is_empty(),
        "{} step(s) differ\n{}",
        diffs.len(),
        diffs.join("\n")
    );
    // Single shot, two sustained, crouched and prone.
    assert_eq!(fired, 5, "the capture's shots");
}
