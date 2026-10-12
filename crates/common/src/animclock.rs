//! A player DObj's per-channel clock: when the clip on a channel started, at
//! which phase, and at what rate it plays. Retail keeps it in the client
//! info's two lerp frames (`ci+0x37c` legs, `ci+0x3ac` torso) and the XAnim
//! tree's node times; `BG_PlayerAnimation`'s per-channel updater
//! (`game.mp.i386.so` 0x2aba0) and its anim setter (0x2a600) drive it once
//! an end frame (docs/research/cod11-movers.md, section 15, "A moving body's
//! clip").

use crate::animtree::PlayerAnims;
use glam::Vec3;

/// The restart toggle on a wire anim value.
const TOGGLE: i32 = 0x200;

/// One channel's clip clock. The clip's time in seconds at level time `t` is
/// `base_s + (t - base_ms) * rate / 1000`: the engine advances a node by
/// `frametime * rate` at each frame's start, and the rate only changes at an
/// end frame, so the clock rebases there.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ChannelClock {
    /// `lf+0x10`: the wire value last set, toggle included.
    anim: i32,
    base_ms: i32,
    base_s: f32,
    /// `lf+0x28`, the node's rate.
    rate: f32,
    /// `lf+0x2c` and `lf+0x1c`: the level time and `pos.trBase` the rate was
    /// last measured from; 0 is unset, as the zeroed lerp frame reads.
    old_ms: i32,
    old_pos: Vec3,
}

impl ChannelClock {
    /// A clock already on `anim`, started at `start_ms` and playing at rate
    /// 1: what a channel reads with no movement anim on it.
    pub fn started(anim: i32, start_ms: i32) -> ChannelClock {
        ChannelClock {
            anim,
            base_ms: start_ms,
            rate: 1.0,
            ..ChannelClock::default()
        }
    }

    /// The clip's time in seconds at level time `now_ms`, unwrapped; the
    /// pose wraps a looping clip.
    pub fn seconds(&self, now_ms: i32) -> f32 {
        self.base_s + now_ms.wrapping_sub(self.base_ms).max(0) as f32 * 0.001 * self.rate
    }

    /// The rate the clip plays at.
    pub fn rate(&self) -> f32 {
        self.rate
    }

    /// 0x2aba0 for one channel at the end frame of level time `now_ms`:
    /// start the clip when `wire` differs from the last one set, then measure
    /// the rate off how far `tr_base`, the entity's snapped origin, moved
    /// since the last end frame.
    pub fn update(
        &mut self,
        anims: &PlayerAnims,
        wire: i32,
        now_ms: i32,
        client_num: i32,
        tr_base: Vec3,
    ) {
        let index = wire & !TOGGLE;
        if wire != self.anim {
            self.set(anims, wire, now_ms, client_num);
        }
        if index == 0 {
            return;
        }
        let motion = anims.motion(index).unwrap_or_default();
        if motion.move_speed == 0 || self.old_ms == 0 {
            // 0x2ada5: rate 1, and the measurement starts here.
            self.set_rate(now_ms, 1.0);
        } else if self.old_ms != now_ms {
            // 0x2ac58..0x2ac9d: distance over seconds over `moveSpeed`.
            let dist = if motion.vertical {
                (self.old_pos.z - tr_base.z).abs()
            } else {
                self.old_pos.distance(tr_base)
            };
            let secs = now_ms.wrapping_sub(self.old_ms) as f32 * 0.001;
            let mut rate = dist / secs / motion.move_speed as f32;
            if rate < 0.1 {
                // 0x2acba..0x2acf6: a ladder anim stops under 0.01.
                rate = if rate < 0.01 && motion.vertical {
                    0.0
                } else {
                    0.1
                };
            } else if rate > 2.0 {
                // 0x2acf8..0x2ad61: a fast clip may run less over.
                let cap = if motion.vertical {
                    4.0
                } else if motion.move_speed > 150 {
                    2.0
                } else if motion.move_speed < 20 {
                    3.0
                } else {
                    3.0 - (motion.move_speed - 20) as f32 / 130.0
                };
                rate = rate.min(cap);
            }
            self.set_rate(now_ms, rate);
        }
        self.old_ms = now_ms;
        self.old_pos = tr_base;
    }

    /// The setter 0x2a600's clock half: the new clip restarts at 0, and a
    /// looping movement clip starts at the phase of the looping movement
    /// clip it replaces, or else at one the level time and the client
    /// number spread out (0x2a858..0x2a919, `trap_XAnimSetTime` 0x2aa5a).
    fn set(&mut self, anims: &PlayerAnims, wire: i32, now_ms: i32, client_num: i32) {
        let old = anims
            .motion(self.anim & !TOGGLE)
            .filter(|_| self.anim & !TOGGLE != 0);
        let new = anims.motion(wire & !TOGGLE).filter(|_| wire & !TOGGLE != 0);
        let phase = match (new, old) {
            (Some(n), Some(o)) if n.move_speed != 0 && n.looping => {
                if o.move_speed != 0 && o.looping && o.duration > 0.0 {
                    (self.seconds(now_ms) / o.duration).rem_euclid(1.0)
                } else {
                    spread_phase(now_ms, client_num)
                }
            }
            (Some(n), None) if n.move_speed != 0 && n.looping => spread_phase(now_ms, client_num),
            _ => 0.0,
        };
        self.anim = wire;
        self.base_ms = now_ms;
        self.base_s = phase * new.map_or(0.0, |n| n.duration);
    }

    fn set_rate(&mut self, now_ms: i32, rate: f32) {
        self.base_s = self.seconds(now_ms);
        self.base_ms = now_ms;
        self.rate = rate;
    }
}

/// `((bg.time / 100 + clientNum * 360) % 1000) * 0.001`, the normalized time
/// a movement clip starts at when nothing it replaces gives it one.
fn spread_phase(now_ms: i32, client_num: i32) -> f32 {
    ((now_ms / 100 + client_num * 360) % 1000) as f32 * 0.001
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spread_phase_steps_a_tenth_of_a_second_and_a_client() {
        assert_eq!(spread_phase(12_345, 0), 0.123);
        assert_eq!(spread_phase(12_345, 1), 0.483);
        assert_eq!(
            spread_phase(99_999, 3),
            (999 + 1080) as f32 % 1000.0 * 0.001
        );
    }

    #[test]
    fn a_rate_change_rebases_without_a_jump() {
        let mut c = ChannelClock::started(122, 1000);
        assert_eq!(c.seconds(1500), 0.5);
        c.set_rate(1500, 2.0);
        assert_eq!(c.seconds(1500), 0.5);
        assert_eq!(c.seconds(1550), 0.6);
    }
}
