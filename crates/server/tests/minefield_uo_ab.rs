//! A United Offensive minefield kill on ours against retail's capture of it
//! (`crates/common/tests/fixtures/net/*-uo-mine.bin`: the watcher of
//! `client-probes/probe_minefield` with `probe_field 1` on mp_pavlov, the
//! victim a second plain net probe; recipe in docs/protocol-1.1.md, "A
//! player's `pos.trBase` is whole units"). Ours runs the same probe with
//! the same two clients, the watcher first.
//!
//! `MINE_REPORT=1` prints both sides' rows. Needs `COD_DIR` at a UO
//! install; without one it returns early.

mod common;

use common::{ADDR, ADDR_B, ClientEnd, Queues};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::NetClient;
use vcod_common::net::event_ids::EV_SOUND_ALIAS;
use vcod_common::net::events::EventTracker;
use vcod_common::net::msg::UserCmd;
use vcod_common::net::protocol::PROTOCOL_V22;
use vcod_common::net::snapshot::Snapshot;

const MAP: &str = "mp_pavlov";
const PROBE: &str = "probe_minefield";
/// The victim's slot: the watcher joins first.
const VICTIM: u32 = 1;

/// What the watcher saw: the victim's `pos.trBase` per snapshot while he
/// is on the wire, then the `explo_mine` entity's (the stock
/// `playSoundinSpace` model, spawned at the victim's `origin`).
#[derive(Debug, Default)]
struct Seen {
    victim: Vec<(i32, [f32; 3])>,
    sound: Option<[f32; 3]>,
}

impl Seen {
    fn take(&mut self, tracker: &mut EventTracker, s: &Snapshot) {
        let p = &PROTOCOL_V22;
        if let Some(e) = s
            .entities
            .get(&VICTIM)
            .filter(|e| e.field_i32(p, "eType") == 1)
        {
            let base = [0, 1, 2].map(|i| e.field_f32(p, &format!("pos.trBase[{i}]")));
            self.victim.push((s.server_time, base));
        }
        for ev in tracker.drain(s, p) {
            let model = ev.entity_num != VICTIM && ev.entity_num != u32::MAX;
            if ev.event == EV_SOUND_ALIAS && model && self.sound.is_none() {
                self.sound = Some(ev.pos);
            }
        }
    }

    /// From the `setOrigin` that puts the victim in the field (his base
    /// jumps by more than 100 units) to his last row.
    fn in_field(&self) -> &[(i32, [f32; 3])] {
        let at = self
            .victim
            .windows(2)
            .position(|w| (w[1].1[0] - w[0].1[0]).abs() > 100.0)
            .expect("the victim put in the field");
        &self.victim[at + 1..]
    }
}

fn retail() -> Seen {
    let mut seen = Seen::default();
    let mut tracker = EventTracker::new();
    for f in vcod_common::testing::fixture_frames(
        &PROTOCOL_V22,
        "net/gamestate-uo-mine.bin",
        "net/snapshots-uo-mine.bin",
    ) {
        seen.take(&mut tracker, &f.snapshot);
    }
    seen
}

fn ours() -> Seen {
    let fs = vcod_common::testing::uo_fs().unwrap();
    let src = format!("../gsc/tests/fixtures/semantics/client-probes/{PROBE}.gsc");
    let probe = std::fs::read_to_string(src).expect("read the probe");
    let bsp_path = fs.resolve_map(MAP).expect("the map in the paks");
    let bsp = vcod_common::bsp::parse(&fs.read(&bsp_path).unwrap()).unwrap();
    let mut now = Instant::now();
    let mut sv = vcod_server::Server::new(common::cfg(MAP, PROBE), now);
    sv.set_protocol(&PROTOCOL_V22);
    sv.set_cvar("probe_field", "1");
    sv.overlay_script(&format!("maps/mp/gametypes/{PROBE}"), &probe);
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    common::run_alone(&mut sv, &mut now, common::PROBE_JOIN_MS);
    let qa = Rc::new(RefCell::new(Queues::default()));
    let qb = Rc::new(RefCell::new(Queues::default()));
    let mut watcher = NetClient::start_with_qport(ClientEnd(qa.clone()), now, 0x2001);
    let mut victim = None;
    let mut seen = Seen::default();
    let mut tracker = EventTracker::new();
    let mut last = None;
    for frame in 0..20_000 / common::FRAME_MS {
        now += Duration::from_millis(common::FRAME_MS as u64);
        // The victim two seconds behind, as the recipe starts him.
        if frame == 2_000 / common::FRAME_MS {
            victim = Some(NetClient::start_with_qport(
                ClientEnd(qb.clone()),
                now,
                0x2002,
            ));
        }
        watcher.send_frame(&UserCmd::default());
        if let Some(v) = victim.as_mut() {
            v.send_frame(&UserCmd::default());
        }
        for (addr, q) in [(ADDR, &qa), (ADDR_B, &qb)] {
            let pending: Vec<Vec<u8>> = q.borrow_mut().to_server.drain(..).collect();
            for p in pending {
                sv.handle_packet(addr, &p, now);
            }
        }
        sv.tick(now);
        for (to, p) in sv.take_outgoing() {
            let q = if to == ADDR { &qa } else { &qb };
            q.borrow_mut().to_client.push_back(p);
        }
        watcher.pump_at(now);
        if let Some(v) = victim.as_mut() {
            v.pump_at(now);
        }
        if let Some(s) = watcher.snapshots().newest()
            && last != Some(s.server_time)
        {
            last = Some(s.server_time);
            seen.take(&mut tracker, s);
        }
    }
    seen
}

fn report(side: &str, seen: &Seen) {
    if std::env::var_os("MINE_REPORT").is_none() {
        return;
    }
    for (t, b) in &seen.victim {
        println!("{side} t {t} victim {b:?}");
    }
    println!("{side} sound {:?}", seen.sound);
}

/// Every snapshot carries the victim's `pos.trBase` in whole units, the
/// origin truncated toward zero, the `setOrigin` frame's included. The
/// `explo_mine` model, spawned at his unsnapped `origin` where he landed,
/// keeps the fraction on both (docs/protocol-1.1.md, "A player's
/// `pos.trBase` is whole units"). The fall's later rows ride each client's
/// cmd clock, which the in-process client does not replay.
#[test]
fn the_minefield_victim_goes_out_in_whole_units() {
    if vcod_common::testing::uo_fs().is_none() {
        return;
    }
    let r = retail();
    let o = ours();
    report("retail", &r);
    report("ours", &o);
    for (side, seen) in [("retail", &r), ("ours", &o)] {
        assert!(seen.victim.len() > 50, "{side}: {} rows", seen.victim.len());
        for (t, b) in &seen.victim {
            assert!(b.iter().all(|v| v.fract() == 0.0), "{side} t {t}: {b:?}");
        }
        let sound = seen.sound.expect("the explo_mine model");
        assert!(sound[2].fract() != 0.0, "{side}: {sound:?}");
    }
    assert_eq!(o.in_field()[0].1, r.in_field()[0].1);
    let (rs, os) = (r.sound.unwrap(), o.sound.unwrap());
    assert!(
        (0..3).all(|i| (os[i] - rs[i]).abs() < 0.1),
        "retail {rs:?}, ours {os:?}"
    );
}
