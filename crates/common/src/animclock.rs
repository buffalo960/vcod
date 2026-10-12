//! A player DObj's per-channel clock: when the clip on a channel started, at
//! which phase and rate it plays, and the clips it is still blending out of.
//! Retail keeps it in the client info's two lerp frames (`ci+0x37c` legs,
//! `ci+0x3ac` torso) and the XAnim tree's node times and weights;
//! `BG_PlayerAnimation`'s per-channel updater (`game.mp.i386.so` 0x2aba0) and
//! its anim setter (0x2a600) drive it once an end frame
//! (docs/research/cod11-movers.md, section 15, "A moving body's clip").

use crate::animscript::movetype_bit;
use crate::animtree::{AnimMotion, PlayerAnims};
use glam::Vec3;

/// The restart toggle on a wire anim value.
const TOGGLE: i32 = 0x200;

/// How many clips a channel keeps blending out at once. A change lands at
/// most once an end frame and the longest blend is 250 ms, so a sixth one is
/// at weight 0 by the time it would be pushed out.
const FADES: usize = 5;

/// One clip on a channel: the clip's time in seconds at level time `t` is
/// `base_s + (t - base_ms) * rate / 1000`. The engine advances a node by
/// `frametime * rate` at each frame's start, and the rate only changes at an
/// end frame, so the clock rebases there.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Play {
    /// The anim index, toggle off.
    index: i32,
    base_ms: i32,
    base_s: f32,
    rate: f32,
}

impl Play {
    fn seconds(&self, now_ms: i32) -> f32 {
        self.base_s + now_ms.wrapping_sub(self.base_ms).max(0) as f32 * 0.001 * self.rate
    }
}

/// A node's weight easing to its goal: XAnim moves it the remaining
/// distance over the remaining goal time each frame, which is a straight
/// line from where the setter found it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Ramp {
    from: f32,
    to: f32,
    start_ms: i32,
    ms: i32,
}

impl Ramp {
    fn new(from: f32, to: f32, start_ms: i32, ms: i32) -> Ramp {
        Ramp {
            from,
            to,
            start_ms,
            ms,
        }
    }

    fn at(&self, now_ms: i32) -> f32 {
        if self.ms <= 0 {
            return self.to;
        }
        let s = (now_ms.wrapping_sub(self.start_ms) as f32 / self.ms as f32).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * s
    }
}

/// One clip of a posed channel: what it plays, how far in, and its share.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layer {
    /// The anim index, toggle off.
    pub index: i32,
    /// The clip's time in seconds, unwrapped; the pose wraps a looping clip.
    pub seconds: f32,
    pub weight: f32,
}

/// One channel's clips and the rate measurement behind its current one.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ChannelClock {
    /// `lf+0x10`: the wire value last set, toggle included.
    anim: i32,
    current: Play,
    weight: Ramp,
    /// Earlier clips still blending out.
    fading: [Option<(Play, Ramp)>; FADES],
    /// `lf+0x2c` and `lf+0x1c`: the level time and `pos.trBase` the rate was
    /// last measured from; 0 is unset, as the zeroed lerp frame reads.
    old_ms: i32,
    old_pos: Vec3,
}

impl ChannelClock {
    /// A clock already on `anim` at full weight, started at `start_ms` and
    /// playing at rate 1: what a channel reads with no movement anim on it.
    pub fn started(anim: i32, start_ms: i32) -> ChannelClock {
        ChannelClock {
            anim,
            current: Play {
                index: anim & !TOGGLE,
                base_ms: start_ms,
                base_s: 0.0,
                rate: 1.0,
            },
            weight: Ramp::new(1.0, 1.0, start_ms, 0),
            ..ChannelClock::default()
        }
    }

    /// The current clip's time in seconds at level time `now_ms`.
    pub fn seconds(&self, now_ms: i32) -> f32 {
        self.current.seconds(now_ms)
    }

    /// The rate the current clip plays at.
    pub fn rate(&self) -> f32 {
        self.current.rate
    }

    /// Every clip with weight at `now_ms`, those blending out first and the
    /// current one last.
    pub fn layers(&self, now_ms: i32) -> Vec<Layer> {
        let layer = |p: &Play, w: &Ramp| Layer {
            index: p.index,
            seconds: p.seconds(now_ms),
            weight: w.at(now_ms),
        };
        self.fading
            .iter()
            .flatten()
            .map(|(p, w)| layer(p, w))
            .chain(std::iter::once(layer(&self.current, &self.weight)))
            .filter(|l| l.index != 0 && l.weight > 0.0)
            .collect()
    }

    /// 0x2aba0 for one channel at the end frame of level time `now_ms`:
    /// start the clip when `wire` differs from the last one set, then measure
    /// the rate off how far `tr_base`, the entity's snapped origin, moved
    /// since the last end frame. `legs` picks the legs lerp frame, whose
    /// setter blends by stance (0x2a6ff).
    pub fn update(
        &mut self,
        anims: &PlayerAnims,
        wire: i32,
        legs: bool,
        now_ms: i32,
        client_num: i32,
        tr_base: Vec3,
    ) {
        let index = wire & !TOGGLE;
        if wire != self.anim {
            self.set(anims, wire, legs, now_ms, client_num);
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

    /// The setter 0x2a600: the old clip blends out and the new one in over
    /// [`blend_ms`], the new one restarting at 0 or, for a looping movement
    /// clip, at the phase of the looping movement clip it replaces, else at
    /// one the level time and the client number spread out (0x2a858..0x2a919,
    /// `trap_XAnimSetTime` 0x2aa5a).
    fn set(&mut self, anims: &PlayerAnims, wire: i32, legs: bool, now_ms: i32, client_num: i32) {
        let motion = |i: i32| anims.motion(i).filter(|_| i != 0);
        let (old_index, new_index) = (self.anim & !TOGGLE, wire & !TOGGLE);
        let (old, new) = (motion(old_index), motion(new_index));
        let phase = match new {
            Some(n) if n.move_speed != 0 && n.looping => match old {
                Some(o) if o.move_speed != 0 && o.looping && o.duration > 0.0 => {
                    (self.current.seconds(now_ms) / o.duration).rem_euclid(1.0)
                }
                _ => spread_phase(now_ms, client_num),
            },
            _ => 0.0,
        };
        let blend = blend_ms(anims, old_index, new_index, legs);
        // `XAnimClearGoalWeight` on the old node, and the complete goal
        // weight on the new one sends every sibling to 0 over the same time.
        let mut fading: Vec<(Play, Ramp)> = self
            .fading
            .iter()
            .flatten()
            .copied()
            .chain(std::iter::once((self.current, self.weight)))
            .map(|(p, w)| (p, Ramp::new(w.at(now_ms), 0.0, now_ms, blend)))
            .filter(|(p, w)| p.index != 0 && w.from > 0.0)
            .collect();
        // A node already blending out comes back from the weight it had.
        let from = match fading.iter().position(|(p, _)| p.index == new_index) {
            Some(i) => fading.remove(i).1.from,
            None => 0.0,
        };
        fading.sort_by(|a, b| b.1.from.total_cmp(&a.1.from));
        fading.truncate(FADES);
        self.fading = [None; FADES];
        for (slot, f) in self.fading.iter_mut().zip(fading) {
            *slot = Some(f);
        }
        self.anim = wire;
        self.current = Play {
            index: new_index,
            base_ms: now_ms,
            base_s: phase * new.map_or(0.0, |n| n.duration),
            // The goal weight call's own rate; the updater sets the real one.
            rate: 1.0,
        };
        self.weight = Ramp::new(from, 1.0, now_ms, blend);
    }

    fn set_rate(&mut self, now_ms: i32, rate: f32) {
        self.current.base_s = self.current.seconds(now_ms);
        self.current.base_ms = now_ms;
        self.current.rate = rate;
    }
}

/// `lf->animationTime`, the blend a new anim gets (0x2a698..0x2a855). The
/// legs blend in at once from no anim at all; a stance change takes 200 ms
/// into a movement or turn clip and 250 into a still one; otherwise a
/// movement clip takes 120 ms, and a still one 250 out of a movement clip
/// and 170 out of anything else. Going to no anim is never under 200.
fn blend_ms(anims: &PlayerAnims, old: i32, new: i32, legs: bool) -> i32 {
    let motion =
        |i: i32| -> Option<AnimMotion> { (i != 0).then(|| anims.motion(i).unwrap_or_default()) };
    let (o, n) = (motion(old), motion(new));
    if o.is_none() && legs {
        return 0;
    }
    let still = |o: Option<AnimMotion>| {
        if o.is_some_and(|o| o.move_speed != 0) {
            250
        } else {
            170
        }
    };
    let Some(n) = n else {
        return still(o).max(200);
    };
    // The record's movetype word (`animation_t +0x54`): the crouched and
    // prone classes (0xc4, 0x308), and the two turn movetypes (0xc000).
    let crouched = movetype_bit("idlecr") | movetype_bit("walkcr") | movetype_bit("walkcrbk");
    let prone = movetype_bit("idleprone") | movetype_bit("walkprone") | movetype_bit("walkpronebk");
    let turn = movetype_bit("turnright") | movetype_bit("turnleft");
    let class = |i: i32| {
        let m = anims.record(i).movetypes;
        (m & crouched != 0, m & prone != 0)
    };
    if legs && class(old) != class(new) {
        if n.move_speed != 0 || anims.record(new).movetypes & turn != 0 {
            200
        } else {
            250
        }
    } else if n.move_speed != 0 {
        120
    } else {
        still(o)
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

    #[test]
    fn a_weight_ramp_is_a_straight_line_to_its_goal() {
        let r = Ramp::new(0.25, 1.0, 1000, 200);
        assert_eq!(r.at(1000), 0.25);
        assert_eq!(r.at(1100), 0.625);
        assert_eq!(r.at(1300), 1.0);
        assert_eq!(Ramp::new(0.0, 1.0, 1000, 0).at(1000), 1.0);
    }
}
