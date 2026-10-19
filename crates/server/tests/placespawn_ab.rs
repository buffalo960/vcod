//! `placeSpawnpoint` against retail, to the float's last bit, on every
//! stock map of all three servers.
//!
//! `client-probes/probe_placebits` runs as the gametype on both sides and
//! logs where every spawnpoint a stock gametype drops lands, its height as
//! sign, whole part and 2^-24ths. Retail's lines are
//! `tests/fixtures/spawnpoints/placebits-retail.txt`, whose header carries
//! the recipe. A drop that differs is a player spawned somewhere retail
//! never puts one: the point drop this gate's first version replaced left
//! mp_harbor's entity 241 at z -262144, below the world, and a player
//! spawned there fell for the rest of that life
//! (cod11-gsc-object-model.md 16.1). The low bits matter too: a spawn
//! think's fall from rest ties `PmoveSingle`'s velocity clamp, so they pick
//! the velocity a sloped spawn leaves (cod15-movement.md 4).
//!
//! The rows ours still misses are [`BITS_GAPS`], each pinned to what ours
//! prints.
//!
//! Needs `COD_DIR`; without the paks it returns early. `PLACEBITS_MAP=<map>`
//! runs one map.

mod common;

use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Instant;

const BITS_SRC: &str = "../gsc/tests/fixtures/semantics/client-probes/probe_placebits.gsc";
const BITS_FIXTURE: &str = "tests/fixtures/spawnpoints/placebits-retail.txt";

/// The spawnpoints ours places off retail's height, as (server, map,
/// entity, ours' sign, whole part and 2^-24ths). A row fails once ours
/// moves at all, closed or not. cod11-gsc-object-model.md 16.1 has what
/// each group rests on.
const BITS_GAPS: &[(&str, &str, u32, &str)] = &[
    // United Offensive only: a capsule whose top touches a brush's
    // underside at the start (mp_sicily's roof brushes 1259 and 3508 at
    // z 1136) sweeps up through it on ours and stops at once on retail,
    // whose down trace then starts at the spawnpoint. Treating the touch
    // as outside matches these six and breaks 1.1d's mp_powcamp 287/288;
    // the binary's brush clip reads alike on all three. Not pinned, and
    // neither is mp_uo_stanjel 557, which starts in a playerclip brush and
    // lands 33 units higher on retail.
    ("uo", "mp_sicily", 181, "+ 1152 2097152"),
    ("uo", "mp_sicily", 182, "+ 1152 2097152"),
    ("uo", "mp_sicily", 183, "+ 1152 2097152"),
    ("uo", "mp_sicily", 184, "+ 1152 2097152"),
    ("uo", "mp_sicily", 185, "+ 1152 2097152"),
    ("uo", "mp_sicily", 186, "+ 1152 2097152"),
    ("uo", "mp_uo_stanjel", 557, "+ 408 2097152"),
    // A floor at z 0 under a sloped ceiling: retail lands 2^-14 short of
    // ours' 0.125. Not pinned.
    ("uo", "mp_uo_powcamp", 332, "+ 0 2097152"),
    ("uo", "mp_uo_powcamp", 334, "+ 0 2097152"),
    ("uo", "mp_uo_powcamp", 288, "+ 0 2097152"),
    // Last-bit residuals on sloped terrain and brush contacts, 1 to 160
    // ulps: the terrain face and edge clips recomputed in 80-bit precision
    // land where ours does, so the difference is upstream (the record
    // build) or in a step not yet transcribed. Not pinned.
    ("1.1d", "mp_brecourt", 211, "+ 38 11810176"),
    ("1.1d", "mp_brecourt", 231, "+ 45 1869120"),
    ("1.1d", "mp_brecourt", 245, "+ 36 9098432"),
    ("1.1d", "mp_dawnville", 168, "+ 26 1406848"),
    ("1.1d", "mp_dawnville", 182, "- 15 4825344"),
    ("1.1d", "mp_dawnville", 125, "+ 53 12702784"),
    ("1.1d", "mp_hurtgen", 146, "+ 106 11812224"),
    ("1.1d", "mp_hurtgen", 204, "- 76 13886464"),
    ("1.1d", "mp_hurtgen", 147, "+ 106 11812224"),
    ("1.1d", "mp_pavlov", 164, "- 8 8255552"),
    ("1.1d", "mp_pavlov", 111, "- 14 11546880"),
    ("1.1d", "mp_pavlov", 142, "- 106 7589120"),
    ("1.1d", "mp_railyard", 156, "+ 15 8784320"),
    ("1.1d", "mp_rocket", 80, "+ 243 12263168"),
    ("1.1d", "mp_rocket", 149, "+ 250 2840832"),
    ("1.1d", "mp_rocket", 140, "+ 253 10993408"),
    ("1.1d", "mp_rocket", 86, "+ 225 16260864"),
    ("1.1d", "mp_rocket", 88, "+ 226 15843584"),
    ("1.1d", "mp_rocket", 94, "+ 254 12892928"),
    ("1.1d", "mp_rocket", 97, "+ 222 407296"),
    ("1.1d", "mp_rocket", 98, "+ 230 16646912"),
    ("1.1d", "mp_rocket", 100, "+ 226 1577728"),
    ("1.1d", "mp_rocket", 103, "+ 253 1732352"),
    ("1.1d", "mp_rocket", 266, "+ 234 12870912"),
    ("1.1d", "mp_rocket", 271, "+ 254 7371520"),
    ("1.5b", "mp_bocage", 181, "+ 20 11853952"),
    ("1.5b", "mp_bocage", 182, "+ 19 15885184"),
    ("1.5b", "mp_bocage", 184, "+ 19 2629120"),
    ("1.5b", "mp_brecourt", 211, "+ 38 11810944"),
    ("1.5b", "mp_brecourt", 231, "+ 45 1869120"),
    ("1.5b", "mp_brecourt", 212, "+ 42 16589440"),
    ("1.5b", "mp_brecourt", 222, "- 13 9783872"),
    ("1.5b", "mp_carentan", 273, "- 5 4254016"),
    ("1.5b", "mp_dawnville", 168, "+ 26 1406464"),
    ("1.5b", "mp_dawnville", 182, "- 15 4825408"),
    ("1.5b", "mp_dawnville", 125, "+ 53 12702720"),
    ("1.5b", "mp_dawnville", 126, "+ 124 7979520"),
    ("1.5b", "mp_dawnville", 128, "+ 40 2097024"),
    ("1.5b", "mp_dawnville", 134, "- 43 14680064"),
    ("1.5b", "mp_hurtgen", 146, "+ 106 11812224"),
    ("1.5b", "mp_hurtgen", 147, "+ 106 11812224"),
    ("1.5b", "mp_neuville", 172, "+ 4 4482816"),
    ("1.5b", "mp_neuville", 183, "+ 12 8553344"),
    ("1.5b", "mp_neuville", 201, "+ 20 11221248"),
    ("1.5b", "mp_neuville", 90, "+ 2 12574592"),
    ("1.5b", "mp_neuville", 117, "- 8 15384640"),
    ("1.5b", "mp_neuville", 243, "+ 1 9961216"),
    ("1.5b", "mp_pavlov", 164, "- 8 8255360"),
    ("1.5b", "mp_pavlov", 111, "- 14 11546816"),
    ("1.5b", "mp_pavlov", 142, "- 106 7589248"),
    ("1.5b", "mp_pavlov", 297, "- 104 6216320"),
    ("1.5b", "mp_pavlov", 244, "- 17 7718656"),
    ("1.5b", "mp_railyard", 156, "+ 15 8784512"),
    ("1.5b", "mp_railyard", 91, "+ 15 8928576"),
    ("1.5b", "mp_stalingrad", 192, "+ 46 12088320"),
    ("1.5b", "mp_stalingrad", 193, "+ 33 8396032"),
    ("1.5b", "mp_stalingrad", 249, "+ 27 13510144"),
    ("1.5b", "mp_tigertown", 135, "+ 193 8502272"),
    ("1.5b", "mp_tigertown", 118, "+ 246 11834624"),
    ("uo", "mp_arnhem", 640, "+ 1 12575552"),
    ("uo", "mp_arnhem", 596, "+ 3 2750144"),
    ("uo", "mp_arnhem", 423, "+ 8 10962112"),
    ("uo", "mp_berlin", 205, "+ 698 7202816"),
    ("uo", "mp_berlin", 268, "+ 698 7202816"),
    ("uo", "mp_berlin", 298, "+ 674 7561216"),
    ("uo", "mp_berlin", 406, "+ 718 6306816"),
    ("uo", "mp_carentan", 273, "- 5 4254016"),
    ("uo", "mp_foy", 346, "+ 3 10213888"),
    ("uo", "mp_foy", 738, "- 205 15830272"),
    ("uo", "mp_foy", 225, "+ 197 9566720"),
    ("uo", "mp_italy", 547, "- 94 12428928"),
    ("uo", "mp_italy", 548, "- 100 9261056"),
    ("uo", "mp_italy", 558, "- 13 12020608"),
    ("uo", "mp_italy", 563, "- 83 1521536"),
    ("uo", "mp_italy", 581, "- 61 4493952"),
    ("uo", "mp_italy", 480, "- 94 12428928"),
    ("uo", "mp_italy", 481, "- 100 9261056"),
    ("uo", "mp_italy", 491, "- 13 12020608"),
    ("uo", "mp_italy", 497, "- 83 1521408"),
    ("uo", "mp_italy", 516, "- 61 4493952"),
    ("uo", "mp_italy", 748, "- 62 8296832"),
    ("uo", "mp_italy", 763, "- 143 6340352"),
    ("uo", "mp_italy", 674, "- 63 2800960"),
    ("uo", "mp_italy", 675, "- 67 8776832"),
    ("uo", "mp_italy", 629, "- 61 4253952"),
    ("uo", "mp_italy", 652, "- 142 4253952"),
    ("uo", "mp_italy", 653, "- 142 4253952"),
    ("uo", "mp_italy", 402, "- 61 13289088"),
    ("uo", "mp_italy", 403, "- 61 4253952"),
    ("uo", "mp_italy", 383, "- 142 13289216"),
    ("uo", "mp_italy", 385, "- 142 4253952"),
    ("uo", "mp_kursk", 450, "+ 366 68608"),
    ("uo", "mp_kursk", 451, "+ 362 3602944"),
    ("uo", "mp_kursk", 495, "+ 357 16302592"),
    ("uo", "mp_kursk", 496, "+ 368 4960256"),
    ("uo", "mp_kursk", 497, "+ 370 8408576"),
    ("uo", "mp_ponyri", 746, "+ 24 7547392"),
    ("uo", "mp_ponyri", 282, "+ 24 7547392"),
    ("uo", "mp_ponyri", 708, "+ 14 2097152"),
    ("uo", "mp_streets", 164, "- 31 14680128"),
    ("uo", "mp_streets", 189, "- 23 14680000"),
    ("uo", "mp_streets", 120, "- 31 14680128"),
    ("uo", "mp_streets", 352, "- 150 5676032"),
    ("uo", "mp_streets", 311, "- 153 15331328"),
    ("uo", "mp_streets", 224, "- 31 14680000"),
    ("uo", "mp_streets", 226, "- 23 14680000"),
    ("uo", "mp_uo_carentan", 95, "- 5 4254016"),
    ("uo", "mp_uo_carentan", 164, "- 22 4023808"),
    ("uo", "mp_uo_dawnville", 385, "+ 26 1407488"),
    ("uo", "mp_uo_dawnville", 399, "- 15 4825408"),
    ("uo", "mp_uo_dawnville", 346, "+ 53 12702720"),
    ("uo", "mp_uo_dawnville", 349, "+ 40 2097024"),
    ("uo", "mp_uo_dawnville", 354, "- 43 14680064"),
    ("uo", "mp_uo_dawnville", 109, "- 8 14383296"),
    ("uo", "mp_uo_dawnville", 110, "- 12 355392"),
    ("uo", "mp_uo_dawnville", 111, "- 14 12154560"),
    ("uo", "mp_uo_hurtgen", 239, "+ 106 11812224"),
    ("uo", "mp_uo_hurtgen", 273, "+ 106 11812224"),
    ("uo", "mp_uo_powcamp", 73, "+ 32 2097280"),
    ("uo", "mp_uo_powcamp", 90, "+ 32 2097152"),
    ("uo", "mp_uo_powcamp", 93, "+ 32 2097024"),
    ("uo", "mp_uo_powcamp", 95, "+ 32 2097024"),
    ("uo", "mp_uo_powcamp", 96, "+ 32 2097152"),
    ("uo", "mp_uo_powcamp", 101, "+ 32 2097152"),
    ("uo", "mp_uo_powcamp", 102, "+ 32 2097024"),
    ("uo", "mp_uo_powcamp", 311, "+ 32 2097152"),
    ("uo", "mp_uo_powcamp", 312, "+ 32 2097152"),
    ("uo", "mp_uo_powcamp", 327, "+ 32 2097280"),
    ("uo", "mp_uo_stanjel", 255, "+ 137 5300224"),
    ("uo", "mp_uo_stanjel", 257, "+ 141 13248256"),
    ("uo", "mp_uo_stanjel", 537, "+ 293 10603520"),
];

/// The bit-exact capture's `PROBE` lines, keyed by the map of each
/// `## <server> <map>` heading under `server`.
fn bits_retail(server: &str) -> BTreeMap<String, Vec<String>> {
    let text = std::fs::read_to_string(BITS_FIXTURE)
        .unwrap_or_else(|e| panic!("read {BITS_FIXTURE}: {e}"));
    let mut maps: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut map: Option<String> = None;
    for line in text.lines().map(str::trim_end) {
        if let Some(head) = line.strip_prefix("## ") {
            let (srv, m) = head.split_once(' ').expect("## <server> <map>");
            map = (srv == server).then(|| m.to_string());
        } else if let (Some(m), true) = (&map, line.starts_with("PROBE ")) {
            maps.entry(m.clone()).or_default().push(line.to_string());
        }
    }
    maps
}

/// `map`'s spawnpoints placed as `probe_placebits` prints them, on `fs`'s
/// install.
fn bits_ours(fs: &Rc<vcod_common::pk3::Pk3Fs>, probe: &str, map: &str) -> Vec<String> {
    let proto = vcod_server::server::install_protocol(fs);
    let bsp = vcod_common::bsp::parse(&fs.read(&fs.resolve_map(map).unwrap()).unwrap()).unwrap();
    let mut sv = vcod_server::Server::new(common::cfg(map, "probe_placebits"), Instant::now());
    sv.set_protocol(proto);
    sv.overlay_script("maps/mp/gametypes/probe_placebits", probe);
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(fs)));
    sv.load_scripts(fs.clone()).expect("load the scripts");
    sv.script_log()
        .iter()
        .filter(|l| l.starts_with("PROBE class ") || l.starts_with("PROBE placebits "))
        .map(|l| l.trim_end().to_string())
        .collect()
}

/// Every spawnpoint of every stock map on `server`, to the last bit but
/// for [`BITS_GAPS`]. mp_carentan's entity 92 rests on a terrain edge,
/// which 1.5b and UO sweep on frames their own build made
/// (`bsp-ibsp59-format.md`, "Patch collision on 1.5 and UO"); its 128 and
/// 221 on a near tie between two terrain partitions that only retail's
/// leaf order settles ("The leaf walk").
fn check_bits(fs: vcod_common::pk3::Pk3Fs, server: &str, maps: usize) {
    let fs = Rc::new(fs);
    let probe = std::fs::read_to_string(BITS_SRC).unwrap_or_else(|e| panic!("read the probe: {e}"));
    let mut retail = bits_retail(server);
    assert_eq!(retail.len(), maps, "{BITS_FIXTURE}: {server}'s stock maps");
    // `PLACEBITS_MAP=<map>` runs one map, for a diagnosis.
    if let Ok(only) = std::env::var("PLACEBITS_MAP") {
        retail.retain(|m, _| *m == only);
    }
    let mut diffs = Vec::new();
    for (map, retail) in &retail {
        let ours = bits_ours(&fs, &probe, map);
        assert_eq!(ours.len(), retail.len(), "{server} {map}: line count");
        for (o, r) in ours.iter().zip(retail) {
            let tokens = |l: &str| l.split(' ').map(str::to_string).collect::<Vec<_>>();
            let (ot, rt) = (tokens(o), tokens(r));
            let gap = BITS_GAPS
                .iter()
                .find(|g| g.0 == server && g.1 == map && rt[2] == g.2.to_string());
            // A gap keeps the entity, x and y and differs in the height
            // exactly as pinned.
            let fine = match gap {
                None => o == r,
                Some(g) => o != r && ot[..5] == rt[..5] && ot[6..].join(" ") == g.3,
            };
            if !fine {
                diffs.push(format!("{server} {map}: ours {o} / retail {r}"));
            }
        }
    }
    assert!(
        diffs.is_empty(),
        "{} spawnpoints differ:\n{}",
        diffs.len(),
        diffs.join("\n")
    );
}

#[test]
fn placespawnpoint_matches_retail_to_the_bit() {
    if let Some(fs) = vcod_common::testing::game_fs_11() {
        check_bits(fs, "1.1d", 12);
    }
}

#[test]
fn placespawnpoint_on_15_matches_retail_to_the_bit() {
    let Some(fs) = vcod_common::testing::game_fs() else {
        return;
    };
    let v6 = vcod_common::net::protocol::PROTOCOL_V6.version;
    if vcod_server::server::install_protocol(&fs).version == v6 {
        check_bits(fs, "1.5b", 16);
    }
}

#[test]
fn placespawnpoint_on_uo_matches_retail_to_the_bit() {
    if let Some(fs) = vcod_common::testing::uo_fs() {
        check_bits(fs, "uo", 20);
    }
}
