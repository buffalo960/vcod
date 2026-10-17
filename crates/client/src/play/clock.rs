//! The client's server time, retail's `cl.serverTime`: what every entity,
//! the HUD and the usercmds are stamped and drawn at. Local time plus a
//! delta that slews toward each new snapshot by a millisecond or two, so a
//! late, early or lost snapshot never steps it (docs/protocol-1.1.md, "The
//! client's clock").

/// `CL_AdjustTimeDelta`'s thresholds (CoDMP.exe 0x404bf0): past this the
/// delta is reset to the snapshot's.
const RESET_TIME: i32 = 500;
/// Past this the delta moves halfway to the snapshot's.
const FAST_ADJUST: i32 = 100;
/// `cl_timeNudge`'s clamp in `CL_SetCGameTime` (CoDMP.exe 0x404d60).
const MAX_NUDGE: i32 = 30;
/// `CL_FinishMove` (CoDMP.exe 0x40b690) stamps no cmd past this beyond the
/// snapshot.
const MAX_CMD_AHEAD: i32 = 5000;
/// `timedemo`'s step a frame (`* 0x32`, CoDMP.exe 0x404d60).
const TIMEDEMO_MS: i32 = 50;

/// How the delta has moved, for the F3 overlay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClockStats {
    pub resets: u32,
    pub fast: u32,
    /// Slow drifts forward (+1) and back (-2).
    pub ahead: u32,
    pub back: u32,
    /// Frames whose time reached the newest snapshot's minus 5.
    pub extrapolated: u32,
}

/// How a demo's clock runs this frame, off `cl_freezeDemo` and
/// `timedemo` (`CL_SetCGameTime`, CoDMP.exe 0x404d60).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DemoClock {
    /// `cl_freezeDemo`: the time holds where it was.
    pub freeze: bool,
    /// `timedemo`'s frame count (`clc.timeDemoFrames`, 0x15ef014), this
    /// frame counted: the time is the first snapshot's plus 50 ms a frame,
    /// frozen or not.
    pub timedemo_frames: Option<i32>,
}

#[derive(Default)]
pub struct ServerClock {
    /// The snapshot message number seen last frame, to tell a new one.
    last_snap: Option<u32>,
    /// `cl.serverTimeDelta`; `None` until the first snapshot.
    delta: Option<i32>,
    /// `cl.oldServerTime`: the time never runs back below it.
    old_server_time: i32,
    /// `cl.oldFrameServerTime`: the snapshot time last frame.
    old_frame_snap: i32,
    /// `cl.extrapolatedSnapshot`.
    extrapolated: bool,
    /// `clc.timeDemoBaseTime` (0x15ef01c): the first snapshot's time.
    demo_base: i32,
    server_time: i32,
    pub stats: ClockStats,
}

impl ServerClock {
    /// `CL_SetCGameTime` for one frame: `realtime` is local ms, `snap` the
    /// newest snapshot's serverTime, `new_snapshot` whether one arrived since
    /// the last frame, `nudge` `cl_timeNudge`. Returns the frame's time.
    pub fn frame(&mut self, realtime: i32, snap: i32, new_snapshot: bool, nudge: i32) -> i32 {
        // `CL_FirstSnapshot` (CoDMP.exe 0x404d00), which consumes the new
        // snapshot flag; the same again, flag kept, when the snapshot clock
        // runs backwards.
        let mut new_snapshot = new_snapshot;
        if self.delta.is_none() {
            self.first_snapshot(realtime, snap);
            new_snapshot = false;
        } else if snap < self.old_frame_snap {
            self.first_snapshot(realtime, snap);
        }
        let delta = self.delta.expect("seeded above");
        self.old_frame_snap = snap;
        let nudge = nudge.clamp(-MAX_NUDGE, MAX_NUDGE);
        self.server_time = (realtime + delta - nudge).max(self.old_server_time);
        self.old_server_time = self.server_time;
        if realtime + delta >= snap - 5 {
            self.extrapolated = true;
            self.stats.extrapolated += 1;
        }
        if new_snapshot {
            self.adjust_delta(realtime, snap);
        }
        self.server_time
    }

    /// [`Self::frame`], telling a new snapshot by its message number, as
    /// every parsed snapshot sets `cl.newSnapshots`: a 1.5 match timeout
    /// sends snapshots whose time stands still, and the adjusts hold the
    /// clock there (docs/research/cod15-protocol-diff.md, "Match timeout").
    pub fn update(&mut self, realtime: i32, snap: i32, snap_num: u32, nudge: i32) -> i32 {
        let new_snapshot = self.last_snap != Some(snap_num);
        self.last_snap = Some(snap_num);
        self.frame(realtime, snap, new_snapshot, nudge)
    }

    /// [`Self::frame`] for a demo, whose delta never adjusts
    /// (`CL_AdjustTimeDelta` returns first while one plays, CoDMP.exe
    /// 0x404bf0). A frozen demo only seeds the delta; `timedemo` then
    /// overwrites the time.
    pub fn update_demo(&mut self, realtime: i32, snap: i32, nudge: i32, how: DemoClock) -> i32 {
        if how.freeze {
            if self.delta.is_none() || snap < self.old_frame_snap {
                self.first_snapshot(realtime, snap);
            }
            self.old_frame_snap = snap;
        } else {
            self.frame(realtime, snap, false, nudge);
        }
        if let Some(n) = how.timedemo_frames {
            self.server_time = self.demo_base + n * TIMEDEMO_MS;
        }
        self.server_time
    }

    /// Whether a snapshot has seeded the clock (`cls.state` 6, `CA_ACTIVE`).
    pub fn started(&self) -> bool {
        self.delta.is_some()
    }

    /// The time [`Self::update_demo`] will draw at `realtime`, before the
    /// frame's snapshot is known: what a demo is read up to. `None`
    /// before the first snapshot.
    pub fn time_at(&self, realtime: i32, nudge: i32, how: DemoClock) -> Option<i32> {
        let delta = self.delta?;
        if let Some(n) = how.timedemo_frames {
            return Some(self.demo_base + n * TIMEDEMO_MS);
        }
        if how.freeze {
            return Some(self.server_time);
        }
        let nudge = nudge.clamp(-MAX_NUDGE, MAX_NUDGE);
        Some((realtime + delta - nudge).max(self.old_server_time))
    }

    /// `CL_FirstSnapshot` (CoDMP.exe 0x404d00).
    fn first_snapshot(&mut self, realtime: i32, snap: i32) {
        self.delta = Some(snap - realtime);
        self.old_server_time = snap;
        self.demo_base = snap;
        self.extrapolated = false;
    }

    /// `CL_AdjustTimeDelta` (CoDMP.exe 0x404bf0).
    fn adjust_delta(&mut self, realtime: i32, snap: i32) {
        let Some(delta) = self.delta else { return };
        let new_delta = snap - realtime;
        let off = (new_delta - delta).abs();
        let delta = if off > RESET_TIME {
            self.stats.resets += 1;
            self.server_time = snap;
            self.old_server_time = snap;
            new_delta
        } else if off > FAST_ADJUST {
            self.stats.fast += 1;
            (delta + new_delta) >> 1
        } else if self.extrapolated {
            self.extrapolated = false;
            self.stats.back += 1;
            delta - 2
        } else {
            self.stats.ahead += 1;
            delta + 1
        };
        self.delta = Some(delta);
    }

    /// The last frame's time.
    pub fn server_time(&self) -> i32 {
        self.server_time
    }

    /// The time a cmd built now carries, held within 5 s of the snapshot.
    pub fn cmd_time(&self, snap: i32) -> i32 {
        self.server_time.min(snap + MAX_CMD_AHEAD)
    }

    /// `cl.serverTimeDelta`, for the overlay.
    pub fn delta(&self) -> Option<i32> {
        self.delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_snapshot_pegs_the_delta() {
        let mut c = ServerClock::default();
        assert_eq!(c.frame(1000, 50_000, true, 0), 50_000);
        assert_eq!(c.delta(), Some(49_000));
        assert_eq!(c.frame(1016, 50_000, false, 0), 50_016);
    }

    #[test]
    fn slow_drift_is_plus_one_or_minus_two() {
        let mut c = ServerClock::default();
        c.frame(0, 10_000, true, 0);
        c.extrapolated = false;
        // Snapshot 50 ms later arriving on time, drawn time well behind it:
        // the delta creeps forward.
        c.delta = Some(10_000 - 60);
        c.frame(50, 10_050, true, 0);
        assert_eq!(c.delta(), Some(10_000 - 59));
        assert_eq!(c.stats.ahead, 1);
        // A frame at or past snap - 5 marks the interval extrapolated; the
        // next snapshot pulls the delta back two.
        c.frame(105, 10_050, false, 0);
        assert!(c.extrapolated);
        c.frame(110, 10_100, true, 0);
        assert_eq!(c.delta(), Some(10_000 - 61));
        assert_eq!(c.stats.back, 1);
    }

    #[test]
    fn fast_adjust_halves_and_reset_snaps() {
        let mut c = ServerClock::default();
        c.frame(0, 10_000, true, 0);
        // 200 ms off: half the way.
        c.frame(50, 10_250, true, 0);
        assert_eq!(c.delta(), Some((10_000 + 10_200) >> 1));
        assert_eq!(c.stats.fast, 1);
        // 2 s off: reset, and the frame draws the snapshot's time.
        assert_eq!(c.frame(100, 12_300, true, 0), 12_300);
        assert_eq!(c.delta(), Some(12_200));
        assert_eq!(c.stats.resets, 1);
    }

    #[test]
    fn never_runs_backwards() {
        let mut c = ServerClock::default();
        c.frame(0, 10_000, true, 0);
        let a = c.frame(40, 10_000, false, 0);
        // A slow-down adjust and a nudge both want an earlier time; held.
        c.delta = Some(c.delta().unwrap() - 20);
        let b = c.frame(41, 10_000, false, 30);
        assert_eq!(a, b);
    }

    #[test]
    fn nudge_is_clamped_to_30() {
        let mut c = ServerClock::default();
        c.frame(0, 10_000, true, 0);
        c.old_server_time = 0;
        assert_eq!(c.frame(100, 10_000, false, 500), 10_100 - 30);
    }

    #[test]
    fn a_snapshot_clock_that_goes_back_repegs() {
        let mut c = ServerClock::default();
        c.frame(0, 50_000, true, 0);
        c.frame(16, 50_000, false, 0);
        assert_eq!(c.frame(32, 100, true, 0), 100);
        // The kept flag runs a slow adjust: the interval read extrapolated.
        assert_eq!(c.frame(48, 100, false, 0), 114);
    }

    #[test]
    fn cmd_time_holds_within_five_seconds() {
        let mut c = ServerClock::default();
        c.frame(0, 10_000, true, 0);
        c.frame(9000, 10_000, false, 0);
        assert_eq!(c.cmd_time(10_000), 15_000);
    }

    /// 20 Hz snapshots over a jittery, lossy link, 125 fps: the drawn time
    /// never steps back, never jumps more than a frame plus the slew, and
    /// settles between the two newest snapshots on all but a few percent of
    /// frames.
    #[test]
    fn settles_behind_the_newest_snapshot_under_jitter() {
        let mut c = ServerClock::default();
        let mut rng = 12345u64;
        let mut rand = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng % 1000) as f64 / 1000.0
        };
        // (arrival local ms, snapshot time)
        let mut arrivals = Vec::new();
        for i in 0..1200 {
            if rand() < 0.02 {
                continue;
            }
            let sent = 1000 + i * 50;
            let at = sent as f64 + 50.0 + (rand() - 0.5) * 20.0;
            arrivals.push((at as i32, 10_000 + i * 50));
        }
        let mut next = 0;
        let mut snap = None;
        let mut last = None;
        let mut behind = Vec::new();
        for realtime in (1000..61_000).step_by(8) {
            let mut new = false;
            while next < arrivals.len() && arrivals[next].0 <= realtime {
                snap = Some(arrivals[next].1);
                next += 1;
                new = true;
            }
            let Some(s) = snap else { continue };
            let t = c.frame(realtime, s, new, 0);
            if let Some(l) = last {
                assert!(t >= l, "ran back");
                assert!(t - l <= 8 + 2, "jumped {} at {realtime}", t - l);
            }
            last = Some(t);
            if realtime > 11_000 {
                behind.push(s - t);
            }
        }
        assert_eq!(c.stats.resets, 0);
        let past = behind.iter().filter(|&&b| b <= 0).count();
        assert!(
            past * 20 < behind.len(),
            "past the newest snapshot on {past} of {} frames",
            behind.len()
        );
        let mean = behind.iter().sum::<i32>() as f64 / behind.len() as f64;
        eprintln!(
            "past {past} of {} frames, mean lag {mean:.1} ms, {:?}",
            behind.len(),
            c.stats
        );
        assert!((5.0..60.0).contains(&mean), "mean lag {mean}");
    }

    #[test]
    fn a_frozen_snapshot_clock_holds_the_drawn_time() {
        // The 1.5b server during a match timeout, 2026-10-13: 20 snapshots
        // a second, all at 43600. Frames every 8 ms for 15 s.
        let mut c = ServerClock::default();
        let mut num = 100;
        let mut drawn = 0;
        for frame in 0..15_000 / 8 {
            let realtime = frame * 8;
            if realtime % 50 < 8 {
                num += 1;
            }
            drawn = c.update(realtime, 43_600, num, 0);
        }
        assert!((43_600..43_800).contains(&drawn), "{drawn}");
    }

    #[test]
    fn a_frozen_demo_holds_and_resumes_from_the_held_time() {
        let run = DemoClock::default();
        let freeze = DemoClock {
            freeze: true,
            ..run
        };
        let mut c = ServerClock::default();
        assert_eq!(c.update_demo(1000, 50_000, 0, run), 50_000);
        assert_eq!(c.update_demo(1100, 50_000, 0, run), 50_100);
        assert_eq!(c.time_at(1500, 0, freeze), Some(50_100));
        assert_eq!(c.update_demo(1500, 50_200, 0, freeze), 50_100);
        // The delta never moved, so thawed the time jumps to the wall's.
        assert_eq!(c.update_demo(1600, 50_200, 0, run), 50_600);
    }

    #[test]
    fn timedemo_steps_50_ms_a_frame_off_the_first_snapshot() {
        let mut c = ServerClock::default();
        let td = |n| DemoClock {
            timedemo_frames: Some(n),
            ..DemoClock::default()
        };
        assert_eq!(c.time_at(1000, 0, td(1)), None);
        c.update_demo(1000, 50_000, 0, DemoClock::default());
        assert_eq!(c.time_at(1001, 0, td(1)), Some(50_050));
        assert_eq!(c.update_demo(1001, 50_050, 0, td(1)), 50_050);
        // Frozen too: timedemo wins.
        let frozen = DemoClock {
            freeze: true,
            timedemo_frames: Some(2),
        };
        assert_eq!(c.update_demo(1002, 50_100, 0, frozen), 50_100);
    }
}
