//! The playing client's own events, played off the prediction and not again
//! off the snapshot that confirms them: retail cgame's predictable events,
//! deduped per sequence and event the way Q3's
//! `CG_CheckChangedPredictableEvents` does.

use vcod_common::net::events::{EVENT_RING, GameEvent};
use vcod_common::net::protocol::Protocol;
use vcod_common::pmove::predict::Predicted;

use super::input::Stance;

/// How many sequences back a played event is remembered: Q3's
/// `MAX_PREDICTED_EVENTS`. Divides 256, so `seq % REMEMBERED` survives the wrap.
const REMEMBERED: usize = 16;

/// `seq - from` on the wire's 8-bit ring, signed: negative is behind.
fn ahead(seq: i32, from: i32) -> i32 {
    i32::from((seq - from) as u8 as i8)
}

#[derive(Default)]
pub struct PredictedEvents {
    /// One past the newest sequence played, `None` while not predicting.
    played_to: Option<i32>,
    /// `(seq, event)` played, at `seq % REMEMBERED`. `None`: nothing played
    /// at that slot since tracking started.
    played: [Option<(i32, i32)>; REMEMBERED],
}

impl PredictedEvents {
    /// Not predicting: the snapshot drain plays the whole ring.
    pub fn stop(&mut self) {
        *self = PredictedEvents::default();
    }

    /// Starts tracking at the snapshot drain's own mark, so what it has not
    /// played yet plays here, once. A no-op while tracking.
    pub fn start_after(&mut self, drained: Option<i32>) {
        if self.played_to.is_none() {
            self.played_to = drained.map(|s| s & 0xff);
        }
    }

    fn was_played(&self, seq: i32, event: i32) -> bool {
        self.played[seq as usize % REMEMBERED] == Some((seq, event))
    }

    fn record(&mut self, seq: i32, event: i32) {
        self.played[seq as usize % REMEMBERED] = Some((seq, event));
    }

    /// `(seq, event, parm)` to play off the predicted ring, oldest first:
    /// slots past the mark, and slots under it whose event is not the one
    /// played there (a replay on a new snapshot changed it).
    pub fn take_predicted(
        &mut self,
        pred_seq: i32,
        events: [i32; 4],
        parms: [i32; 4],
    ) -> Vec<(i32, i32, i32)> {
        let pred_seq = pred_seq & 0xff;
        let Some(mark) = self.played_to else {
            self.played_to = Some(pred_seq);
            return Vec::new();
        };
        // A respawn restarts the ring at 0 (AGENTS.md, Gotchas).
        if ahead(pred_seq, mark) < -EVENT_RING {
            self.stop();
            self.played_to = Some(pred_seq);
            return Vec::new();
        }
        let mut out = Vec::new();
        for s in pred_seq - EVENT_RING..pred_seq {
            let seq = s & 0xff;
            let slot = (s & 3) as usize;
            let event = events[slot];
            let changed = self.played[seq as usize % REMEMBERED]
                .is_some_and(|(at, e)| at == seq && e != event);
            if ahead(seq, mark) >= 0 || changed {
                self.record(seq, event);
                out.push((seq, event, parms[slot]));
            }
        }
        if ahead(pred_seq, mark) > 0 {
            self.played_to = Some(pred_seq);
        }
        out
    }

    /// Whether a snapshot's playerstate-ring `event` at `seq` still has to
    /// play: false only for the copy of one played at that sequence.
    pub fn filter_snapshot(&mut self, seq: i32, event: i32) -> bool {
        let seq = seq & 0xff;
        let below = self.played_to.is_some_and(|mark| ahead(seq, mark) < 0);
        if below && self.was_played(seq, event) {
            return false;
        }
        self.record(seq, event);
        if self.played_to.is_some() && !below {
            self.played_to = Some((seq + 1) & 0xff);
        }
        true
    }
}

/// Whether a playerstate or entity event rides the body the view rides.
fn on_view_body(ev: &GameEvent, view_body: Option<u32>) -> bool {
    view_body.is_some_and(|b| ev.entity_num == u32::MAX || ev.entity_num == b)
}

/// The weapon an own pickup event names: `CG_ItemPickup` (cgame 0x3001da80)
/// is reached from the three pickup events on the body the view rides (call
/// at 0x3001e0ff) and acts on a weapon row only, items 1..=`max_weapons`
/// (64, UO 128), configstring 7's weapons
/// (docs/research/cod11-gsc-object-model.md, section 15).
/// `PlayInput::picked_up` decides whether it becomes the selection.
pub fn picked_up_weapon(ev: &GameEvent, view_body: Option<u32>, p: &Protocol) -> Option<u8> {
    use vcod_common::net::event_ids::{EV_AMMO_PICKUP, EV_ITEM_PICKUP, EV_ITEM_PICKUP_QUIET};
    let pickup = matches!(
        ev.event,
        EV_ITEM_PICKUP | EV_ITEM_PICKUP_QUIET | EV_AMMO_PICKUP
    );
    let weapon = (1..=p.max_weapons as i32).contains(&ev.parm);
    (pickup && on_view_body(ev, view_body) && weapon).then_some(ev.parm as u8)
}

/// `EV_NOAMMO` (149) and `EV_DROPWEAPON` (196) on the body the view rides
/// run the out-of-ammo switch 0x300381b0 (arms 0x3001e1d4 and 0x3001e1f9,
/// gate 0x3001e21b, jump 0x3001e241), `PlayInput::out_of_ammo`.
pub fn runs_out_of_ammo(ev: &GameEvent, view_body: Option<u32>) -> bool {
    use vcod_common::net::event_ids::{EV_DROPWEAPON, EV_NOAMMO};
    matches!(ev.event, EV_NOAMMO | EV_DROPWEAPON) && on_view_body(ev, view_body)
}

/// The stance an `EV_STANCE_FORCE_*` puts the client in: retail cgame's
/// handler (0x3001df6d) sets `cl_stance` to 0, 1 or 2 for an event of its
/// own client while `cl_stanceTemp` is 0 (`stance_temp` false), and ignores
/// another client's.
pub fn forced_stance(ev: &GameEvent, own_client: i32, stance_temp: bool) -> Option<Stance> {
    use vcod_common::net::event_ids::{
        EV_STANCE_FORCE_CROUCH, EV_STANCE_FORCE_PRONE, EV_STANCE_FORCE_STAND,
    };
    if ev.client_num != own_client || stance_temp {
        return None;
    }
    match ev.event {
        EV_STANCE_FORCE_STAND => Some(Stance::Stand),
        EV_STANCE_FORCE_CROUCH => Some(Stance::Crouch),
        EV_STANCE_FORCE_PRONE => Some(Stance::Prone),
        _ => None,
    }
}

/// A predicted ring event in the form the snapshot drain gives the
/// playerstate ring's (`EventTracker::drain`).
pub fn game_event(pred: &Predicted, client_num: i32, event: i32, parm: i32) -> GameEvent {
    GameEvent {
        event,
        parm,
        entity_num: u32::MAX,
        client_num,
        weapon: i32::from(pred.ps.weapon),
        surf_type: 0,
        pos: pred.ps.origin.to_array(),
        dir: [0.0; 3],
        scale: 0,
        other_entity_num: u32::MAX,
        attacker_entity_num: -1,
        e_type: 0,
        eflags: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::registry::{EV_FIRE_WEAPON as FIRE, EV_PAIN as PAIN, EV_RELOAD as RELOAD};

    const PICKUP: i32 = 20;

    fn event(event: i32, parm: i32, entity_num: u32) -> GameEvent {
        GameEvent {
            event,
            parm,
            entity_num,
            client_num: 3,
            weapon: 0,
            surf_type: 0,
            pos: [0.0; 3],
            dir: [0.0; 3],
            scale: 0,
            other_entity_num: u32::MAX,
            attacker_entity_num: -1,
            e_type: 0,
            eflags: 0,
        }
    }

    #[test]
    fn an_own_weapon_pickup_names_the_weapon() {
        use vcod_common::net::event_ids::{EV_AMMO_PICKUP, EV_ITEM_PICKUP};
        use vcod_common::net::protocol::{PROTOCOL_V1, PROTOCOL_V22};
        let (v1, uo) = (&PROTOCOL_V1, &PROTOCOL_V22);
        let me = Some(3);
        assert_eq!(
            picked_up_weapon(&event(EV_ITEM_PICKUP, 6, 3), me, v1),
            Some(6)
        );
        assert_eq!(
            picked_up_weapon(&event(EV_AMMO_PICKUP, 6, u32::MAX), me, v1),
            Some(6)
        );
        // Health, another player, no body, not a pickup.
        assert_eq!(
            picked_up_weapon(&event(EV_ITEM_PICKUP, 68, 3), me, v1),
            None
        );
        assert_eq!(picked_up_weapon(&event(EV_ITEM_PICKUP, 6, 5), me, v1), None);
        assert_eq!(
            picked_up_weapon(&event(EV_ITEM_PICKUP, 6, 3), None, v1),
            None
        );
        assert_eq!(picked_up_weapon(&event(FIRE, 6, 3), me, v1), None);
        // UO's 68 is a weapon row, its small health 131.
        assert_eq!(
            picked_up_weapon(&event(EV_ITEM_PICKUP, 68, 3), me, uo),
            Some(68)
        );
        assert_eq!(
            picked_up_weapon(&event(EV_ITEM_PICKUP, 131, 3), me, uo),
            None
        );
    }

    #[test]
    fn an_own_dry_trigger_or_drop_runs_the_out_of_ammo_switch() {
        use vcod_common::net::event_ids::{EV_DROPWEAPON, EV_NOAMMO};
        let me = Some(3);
        assert!(runs_out_of_ammo(&event(EV_NOAMMO, 0, u32::MAX), me));
        assert!(runs_out_of_ammo(&event(EV_DROPWEAPON, 0, 3), me));
        assert!(!runs_out_of_ammo(&event(EV_NOAMMO, 0, 5), me));
        assert!(!runs_out_of_ammo(&event(EV_NOAMMO, 0, 3), None));
        assert!(!runs_out_of_ammo(&event(RELOAD, 0, 3), me));
    }

    /// Retail cgame's `cl_stance` write (0x3001df6d): our own client's
    /// forced-stance events set the stance, another client's do nothing.
    #[test]
    fn a_forced_stance_event_sets_our_stance() {
        use vcod_common::net::event_ids::{
            EV_STANCE_FORCE_CROUCH, EV_STANCE_FORCE_PRONE, EV_STANCE_FORCE_STAND,
        };
        let ev = |event: i32, client_num: i32| GameEvent {
            event,
            parm: 0,
            entity_num: u32::MAX,
            client_num,
            weapon: 0,
            surf_type: 0,
            pos: [0.0; 3],
            dir: [0.0; 3],
            scale: 0,
            other_entity_num: u32::MAX,
            attacker_entity_num: -1,
            e_type: 0,
            eflags: 0,
        };
        assert_eq!(
            forced_stance(&ev(EV_STANCE_FORCE_STAND, 2), 2, false),
            Some(Stance::Stand)
        );
        assert_eq!(
            forced_stance(&ev(EV_STANCE_FORCE_CROUCH, 2), 2, false),
            Some(Stance::Crouch)
        );
        assert_eq!(
            forced_stance(&ev(EV_STANCE_FORCE_PRONE, 2), 2, false),
            Some(Stance::Prone)
        );
        assert_eq!(
            forced_stance(&ev(EV_STANCE_FORCE_CROUCH, 5), 2, false),
            None
        );
        assert_eq!(forced_stance(&ev(FIRE, 2), 2, false), None);
        // `cl_stanceTemp` non-zero leaves `cl_stance` alone (0x3001dee5).
        assert_eq!(forced_stance(&ev(EV_STANCE_FORCE_CROUCH, 2), 2, true), None);
    }

    const FOOTSTEP: i32 = 1;

    fn tracking(at: i32) -> PredictedEvents {
        let mut e = PredictedEvents::default();
        e.start_after(Some(at));
        e
    }

    #[test]
    fn plays_each_sequence_once() {
        let mut e = tracking(0);
        let ring = [FIRE, RELOAD, 0, 0];
        assert_eq!(
            e.take_predicted(1, ring, [0; 4]),
            [(0, FIRE, 0)],
            "the slot below the sequence"
        );
        assert!(e.take_predicted(1, ring, [0; 4]).is_empty(), "replayed");
        assert_eq!(e.take_predicted(2, ring, [0, 7, 0, 0]), [(1, RELOAD, 7)]);

        // More than a ring behind: the newest four, across the wrap.
        let mut e = tracking(250);
        let got = e.take_predicted(1, [10, 11, 12, 13], [0; 4]);
        assert_eq!(got, [(253, 11, 0), (254, 12, 0), (255, 13, 0), (0, 10, 0)]);
    }

    #[test]
    fn first_sight_plays_nothing() {
        let mut e = PredictedEvents::default();
        assert!(e.take_predicted(3, [FIRE; 4], [0; 4]).is_empty());
        assert_eq!(e.take_predicted(4, [FIRE; 4], [0; 4]), [(3, FIRE, 0)]);
    }

    #[test]
    fn snapshot_copy_is_skipped() {
        let mut e = tracking(0);
        let ring = [FIRE, RELOAD, FIRE, 0];
        e.take_predicted(3, ring, [0; 4]);
        assert!((0..3).all(|s| !e.filter_snapshot(s, ring[s as usize])));
        // The drain's ring walk hands negative sequences across the wrap.
        let mut e = tracking(254);
        e.take_predicted(1, [FIRE; 4], [0; 4]);
        assert!([-2, -1, 0].into_iter().all(|s| !e.filter_snapshot(s, FIRE)));
    }

    /// After a hitch the drain carries sequences more than a ring below the
    /// mark: the ones the prediction played are skipped, the one it could not
    /// reach plays, and nothing resets.
    #[test]
    fn a_hitch_skips_what_was_played_and_plays_the_rest() {
        let mut e = tracking(10);
        let ring = [FIRE, RELOAD, FIRE, RELOAD];
        let played: Vec<_> = e.take_predicted(16, ring, [0; 4]);
        assert_eq!(
            played.iter().map(|p| p.0).collect::<Vec<_>>(),
            [12, 13, 14, 15]
        );
        assert!(e.filter_snapshot(11, ring[3]), "never played");
        for s in 12..=14 {
            assert!(!e.filter_snapshot(s, ring[(s & 3) as usize]), "{s}");
        }
        assert!(e.take_predicted(16, ring, [0; 4]).is_empty());
    }

    /// A respawn restarts the ring at 0 (AGENTS.md, Gotchas): the prediction
    /// resets the mark, or the new life's events are all swallowed.
    #[test]
    fn sequence_going_back_resets() {
        let mut e = tracking(0);
        e.take_predicted(40, [FIRE; 4], [0; 4]);
        assert!(e.take_predicted(1, [FIRE; 4], [0; 4]).is_empty());
        assert!(e.filter_snapshot(0, FIRE), "the drain plays the new life's");
        assert_eq!(e.take_predicted(2, [FIRE; 4], [0; 4]), [(1, FIRE, 0)]);

        // A misprediction a few behind is not a reset.
        let mut e = tracking(0);
        e.take_predicted(10, [FIRE; 4], [0; 4]);
        assert!(e.take_predicted(9, [FIRE; 4], [0; 4]).is_empty());
        assert!(e.take_predicted(10, [FIRE; 4], [0; 4]).is_empty());
    }

    /// A pain event the client cannot predict passes, and the replay that
    /// carries it forward from that snapshot does not play it again.
    #[test]
    fn server_only_events_still_play() {
        let mut e = tracking(0);
        e.take_predicted(3, [FIRE, FIRE, FIRE, 0], [0; 4]);
        assert!(e.filter_snapshot(3, PAIN), "above the mark");
        assert!(
            e.take_predicted(4, [FIRE, FIRE, FIRE, PAIN], [0; 4])
                .is_empty()
        );
        assert_eq!(
            e.take_predicted(5, [FIRE, FIRE, FIRE, PAIN], [0; 4]),
            [(4, FIRE, 0)]
        );
    }

    /// The server put a pickup where the prediction put a fire: the pickup
    /// plays, off either path, once.
    #[test]
    fn a_server_event_under_a_predicted_one_plays() {
        let mut e = tracking(0);
        e.take_predicted(2, [FIRE, FIRE, 0, 0], [0; 4]);
        assert!(e.filter_snapshot(1, PICKUP));
        assert!(e.take_predicted(2, [FIRE, PICKUP, 0, 0], [0; 4]).is_empty());

        // The replay on the new snapshot sees it first.
        let mut e = tracking(0);
        e.take_predicted(2, [FIRE, FIRE, 0, 0], [0; 4]);
        assert_eq!(
            e.take_predicted(2, [FIRE, PICKUP, 0, 0], [0; 4]),
            [(1, PICKUP, 0)]
        );
        assert!(!e.filter_snapshot(1, PICKUP));
    }

    /// A prediction running well ahead of the drained snapshot, as automatic
    /// fire and footsteps do at a few hundred ms of round trip: every copy
    /// the snapshots bring back is still remembered and skipped.
    #[test]
    fn twelve_ahead_every_copy_is_skipped_once() {
        let mut e = tracking(0);
        let event_at = |s: i32| if s % 3 == 0 { FOOTSTEP } else { FIRE };
        let mut ring = [0; 4];
        for s in 0..12 {
            ring[(s & 3) as usize] = event_at(s);
            assert_eq!(e.take_predicted(s + 1, ring, [0; 4]), [(s, event_at(s), 0)]);
        }
        for s in 0..12 {
            assert!(!e.filter_snapshot(s, event_at(s)), "copy of {s} replayed");
        }
    }

    /// A predicted footstep the server never raised: its real fire at that
    /// sequence plays.
    #[test]
    fn a_real_event_over_a_phantom_plays() {
        let mut e = tracking(0);
        e.take_predicted(1, [FOOTSTEP, 0, 0, 0], [0; 4]);
        assert!(e.filter_snapshot(0, FIRE));
    }

    #[test]
    fn not_predicting_filters_nothing() {
        let mut e = tracking(0);
        e.take_predicted(3, [FIRE; 4], [0; 4]);
        e.stop();
        assert!((0..3).all(|s| e.filter_snapshot(s, FIRE)));
        // Back to predicting: from the drain's mark, not the stale one.
        e.stop();
        e.start_after(Some(5));
        assert_eq!(
            e.take_predicted(7, [0, FIRE, RELOAD, 0], [0; 4]),
            [(5, FIRE, 0), (6, RELOAD, 0)]
        );
        e.start_after(Some(0));
        assert!(!e.filter_snapshot(6, RELOAD), "a no-op while tracking");
    }
}
