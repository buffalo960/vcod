//! A player DObj's per-channel clock: when the clip on a channel started, at
//! which phase and rate it plays, and the clips it is still blending out of.
//! Retail keeps it in the client info's two lerp frames (`ci+0x37c` legs,
//! `ci+0x3ac` torso) and the XAnim tree's node times and weights;
//! `BG_PlayerAnimation`'s per-channel updater (`game.mp.i386.so` 0x2aba0) and
//! its anim setter (0x2a600) drive it once an end frame, and the cgame's copy
//! of the two (`cgame_mp_x86.dll` 0x30003be0, 0x300036b0) once a drawn frame
//! (docs/research/cod11-movers.md, section 15, "A moving body's clip" and
//! "The drawn body's clip").

use crate::animscript::movetype_bit;
use crate::animtree::{AnimMotion, PlayerAnims};
use glam::Vec3;

/// The restart toggle on a wire anim value.
const TOGGLE: i32 = 0x200;

/// `eFlags` 0x800: a body `cloneplayer` made under 250 ms ago, whose death
/// anim plays from its start; without it the setter shows the death anim's
/// last frame at once (0x2a600, `cgame_mp_x86.dll` 0x300036b0).
const EF_FRESH_CORPSE: i32 = 0x800;

/// The two clocks an update reads. The server's end frame passes
/// `level.time` for both; the cgame plays its clips and starts its blends on
/// `cg.time` and measures the rate whenever the newest snapshot's
/// `serverTime` (`bg+0x4`, `cg.latestSnapshotTime`) moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Times {
    pub clock_ms: i32,
    pub snapshot_ms: i32,
}

impl Times {
    /// An end frame at level time `now_ms`.
    pub fn end_frame(now_ms: i32) -> Times {
        Times {
            clock_ms: now_ms,
            snapshot_ms: now_ms,
        }
    }
}

/// What an update reads off the entity besides its anim: the client
/// number the first phase spreads by (`ci+0x8`), `pos.trBase` and `eFlags`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub client_num: i32,
    pub tr_base: Vec3,
    pub eflags: i32,
}

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

/// A node's weight easing to its goal: `XAnimSetGoalWeight` (`cod_lnxded`
/// 0x080bd674) gives it `|goal - weight| * time` to get there and the
/// per-frame step (0x080bb130) moves it the remaining distance over the
/// remaining goal time, which is a straight line from where the setter found
/// it at a slope of one over the blend time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Ramp {
    from: f32,
    to: f32,
    start_ms: i32,
    ms: f32,
}

impl Ramp {
    fn new(from: f32, to: f32, start_ms: i32, ms: f32) -> Ramp {
        Ramp {
            from,
            to,
            start_ms,
            ms,
        }
    }

    fn at(&self, now_ms: i32) -> f32 {
        if self.ms <= 0.0 {
            return self.to;
        }
        let s = (now_ms.wrapping_sub(self.start_ms) as f32 / self.ms).max(0.0);
        if s >= 1.0 {
            // 0x080bb130 lands on the goal.
            return self.to;
        }
        self.from + (self.to - self.from) * s
    }
}

/// The legs node's weight while a torso anim plays (`game.mp.i386.so`
/// 0x6eee4).
const LEGS_UNDER_TORSO: f32 = 0.01;

/// The weights of `main`'s two children in the player's anim tree, which
/// only the torso setter moves (0x2aa62..0x2ab5b): a torso anim sends the
/// `torso` node to 1 and the `legs` node to 0.01, no torso anim sends them
/// to 0 and 1, each over the torso's blend.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Nodes {
    torso: Ramp,
    legs: Ramp,
}

impl Default for Nodes {
    /// The legs leaf's first goal weight raises its ancestors to 1 at once
    /// (0x080bdf64 with the legs' blend 0); the torso node waits for a
    /// torso anim.
    fn default() -> Nodes {
        Nodes {
            torso: Ramp::new(0.0, 0.0, 0, 0.0),
            legs: Ramp::new(1.0, 1.0, 0, 0.0),
        }
    }
}

/// `main`'s `torso` and `legs` node weights at one instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeWeights {
    pub torso: f32,
    pub legs: f32,
}

impl Default for NodeWeights {
    fn default() -> NodeWeights {
        NodeWeights {
            torso: 0.0,
            legs: 1.0,
        }
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
    /// `lf+0x2c` and `lf+0x1c`: the snapshot time and `pos.trBase` the rate
    /// was last measured from; 0 is unset, as the zeroed lerp frame reads.
    old_ms: i32,
    old_pos: Vec3,
    /// `lf+0x28`: the rate last measured, which every update hands the
    /// current clip.
    rate: f32,
    /// The torso channel's: the two nodes its setter weights.
    nodes: Nodes,
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
            weight: Ramp::new(1.0, 1.0, start_ms, 0.0),
            rate: 1.0,
            ..ChannelClock::default()
        }
    }

    /// The wire value last set, toggle included (`lf+0x10`).
    pub fn anim(&self) -> i32 {
        self.anim
    }

    /// The current clip's time in seconds at level time `now_ms`.
    pub fn seconds(&self, now_ms: i32) -> f32 {
        self.current.seconds(now_ms)
    }

    /// The rate the current clip plays at.
    pub fn rate(&self) -> f32 {
        self.current.rate
    }

    /// The torso clock's `torso` and `legs` node weights at `now_ms`; a legs
    /// clock reads the defaults.
    pub fn node_weights(&self, now_ms: i32) -> NodeWeights {
        NodeWeights {
            torso: self.nodes.torso.at(now_ms),
            legs: self.nodes.legs.at(now_ms),
        }
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

    /// 0x2aba0 (and the cgame's 0x30003be0) for one channel: start the clip
    /// when `wire` differs from the last one set, then, when the snapshot
    /// time moved, measure the rate off how far `seen.tr_base`, the entity's
    /// snapped origin, moved since the last measurement. `legs` picks the
    /// legs lerp frame, whose setter blends by stance (0x2a6ff).
    pub fn update(&mut self, anims: &PlayerAnims, legs: bool, wire: i32, seen: &Seen, at: Times) {
        self.update_in(anims, legs, wire, seen, at, None);
    }

    /// [`Self::update`] under 1.5's and UO's setter, whose blends a crouch
    /// or prone change stretches: `window` is the client's end of that
    /// stretch (`ci+0x4a4` 1.5b, `ci+0x4c0` UO), shared by both channels,
    /// the legs first.
    pub fn update_windowed(
        &mut self,
        anims: &PlayerAnims,
        legs: bool,
        wire: i32,
        seen: &Seen,
        at: Times,
        window: &mut i32,
    ) {
        self.update_in(anims, legs, wire, seen, at, Some(window));
    }

    fn update_in(
        &mut self,
        anims: &PlayerAnims,
        legs: bool,
        wire: i32,
        seen: &Seen,
        at: Times,
        window: Option<&mut i32>,
    ) {
        let index = wire & !TOGGLE;
        if wire != self.anim {
            self.set(anims, wire, legs, seen, at.clock_ms, window);
        }
        if index == 0 {
            return;
        }
        let motion = anims.motion(index).unwrap_or_default();
        let now_ms = at.snapshot_ms;
        let tr_base = seen.tr_base;
        if motion.move_speed == 0 || self.old_ms == 0 {
            // 0x2ada5: rate 1, and the measurement starts here.
            self.rate = 1.0;
            self.old_ms = now_ms;
            self.old_pos = tr_base;
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
            self.rate = rate;
            self.old_ms = now_ms;
            self.old_pos = tr_base;
        }
        // `trap_XAnimSetAnimRate` with `lf+0x28` on every call (0x2adef,
        // cgame 0x30003e2e), so a clip the setter just started takes the last
        // measured rate even on a frame that measured nothing.
        if self.current.rate != self.rate {
            self.set_rate(at.clock_ms, self.rate);
        }
    }

    /// The setter 0x2a600: the old clip blends out and the new one in over
    /// [`blend_ms`], the new one restarting at 0 or, for a looping movement
    /// clip, at the phase of the looping movement clip it replaces, else at
    /// one the level time and the client number spread out (0x2a858..0x2a919,
    /// `trap_XAnimSetTime` 0x2aa5a). A death anim on a body not freshly
    /// cloned goes straight to its last frame at full weight, every other
    /// clip on the channel dropped (0x2a97b..0x2aa5a, cgame 0x3000395d..0x300039f5).
    fn set(
        &mut self,
        anims: &PlayerAnims,
        wire: i32,
        legs: bool,
        seen: &Seen,
        now_ms: i32,
        window: Option<&mut i32>,
    ) {
        let motion = |i: i32| anims.motion(i).filter(|_| i != 0);
        let (old_index, new_index) = (self.anim & !TOGGLE, wire & !TOGGLE);
        let (old, new) = (motion(old_index), motion(new_index));
        if let Some(n) = new.filter(|n| n.death && seen.eflags & EF_FRESH_CORPSE == 0) {
            self.anim = wire;
            self.current = Play {
                index: new_index,
                base_ms: now_ms,
                base_s: n.duration,
                rate: 1.0,
            };
            self.weight = Ramp::new(1.0, 1.0, now_ms, 0.0);
            self.fading = [None; FADES];
            return;
        }
        let client_num = seen.client_num;
        let phase = match new {
            Some(n) if n.move_speed != 0 && n.looping => match old {
                Some(o) if o.move_speed != 0 && o.looping && o.duration > 0.0 => {
                    (self.current.seconds(now_ms) / o.duration).rem_euclid(1.0)
                }
                _ => spread_phase(now_ms, client_num),
            },
            _ => 0.0,
        };
        let blend = match window {
            None => blend_ms(anims, old_index, new_index, legs),
            Some(w) => blend_ms_windowed(anims, old_index, new_index, legs, now_ms, w),
        } as f32;
        // `XAnimClearGoalWeight` (0x080bf53c) sends the old node to 0 over
        // the whole blend time; the clips already fading keep their own.
        let old_weight = self.weight.at(now_ms);
        let mut fading: Vec<(Play, Ramp)> = self
            .fading
            .iter()
            .flatten()
            .copied()
            .chain(std::iter::once((
                self.current,
                Ramp::new(old_weight, 0.0, now_ms, blend),
            )))
            .filter(|(p, w)| p.index != 0 && w.at(now_ms) > 0.0)
            .collect();
        // A node already blending out comes back from the weight it had, in
        // the part of the blend time that distance takes.
        let from = match fading.iter().position(|(p, _)| p.index == new_index) {
            Some(i) => fading.remove(i).1.at(now_ms),
            None => 0.0,
        };
        fading.sort_by(|a, b| b.1.at(now_ms).total_cmp(&a.1.at(now_ms)));
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
        self.weight = Ramp::new(from, 1.0, now_ms, (1.0 - from) * blend);
        if !legs {
            let (torso, legs) = if new_index == 0 {
                (0.0, 1.0)
            } else {
                (1.0, LEGS_UNDER_TORSO)
            };
            self.nodes = Nodes {
                torso: goal(self.nodes.torso, torso, now_ms, blend),
                legs: goal(self.nodes.legs, legs, now_ms, blend),
            };
        }
    }

    fn set_rate(&mut self, now_ms: i32, rate: f32) {
        self.current.base_s = self.current.seconds(now_ms);
        self.current.base_ms = now_ms;
        self.current.rate = rate;
    }
}

/// `XAnimSetGoalWeight` (0x080bd674) on a node: from wherever `ramp` stands
/// to `to` over `|to - weight| * blend_ms`.
fn goal(ramp: Ramp, to: f32, now_ms: i32, blend_ms: f32) -> Ramp {
    let from = ramp.at(now_ms);
    Ramp::new(from, to, now_ms, (to - from).abs() * blend_ms)
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
    // The anim's own `+0x40` wins when it is set (0x2a6ab, cgame 0x3000374c).
    if let Some(b) = n.map(|n| n.blend_ms).filter(|&b| b > 0) {
        return b;
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

/// The blend of 1.5's and UO's setter (1.5b 0x28ced, UO 0x2ce5f): the
/// anim's own, else 170 into a still clip, 250 into one out of a movement
/// clip and 120 into a movement clip, 200 into no anim, and never under
/// what is left of `window`. A legs change of crouch or prone class opens a
/// new 400 ms one, which the torso's change in the same end frame reads
/// too. The legs blend in at once from no anim at all.
fn blend_ms_windowed(
    anims: &PlayerAnims,
    old: i32,
    new: i32,
    legs: bool,
    now_ms: i32,
    window: &mut i32,
) -> i32 {
    let record = |i: i32| (i != 0).then(|| anims.motion(i).unwrap_or_default());
    let (o, n) = (record(old), record(new));
    if n.is_some() && legs {
        let crouched = movetype_bit("idlecr") | movetype_bit("walkcr") | movetype_bit("walkcrbk");
        let prone =
            movetype_bit("idleprone") | movetype_bit("walkprone") | movetype_bit("walkpronebk");
        let class = |i: i32| {
            let m = anims.record(i).movetypes;
            (m & crouched != 0, m & prone != 0)
        };
        if class(old) != class(new) {
            *window = now_ms.wrapping_add(400);
        }
    }
    if o.is_none() && legs {
        return 0;
    }
    let own = n.map_or(200, |n| n.blend_ms);
    let rule = match n {
        Some(n) if n.blend_ms >= 1 => -1,
        Some(n) if n.move_speed != 0 => 120,
        _ if o.is_some_and(|o| o.move_speed != 0) => 250,
        _ => 170,
    };
    own.max(rule.max(window.wrapping_sub(now_ms)))
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
        let r = Ramp::new(0.25, 1.0, 1000, 200.0);
        assert_eq!(r.at(1000), 0.25);
        assert_eq!(r.at(1100), 0.625);
        assert_eq!(r.at(1300), 1.0);
        assert_eq!(Ramp::new(0.0, 1.0, 1000, 0.0).at(1000), 1.0);
    }

    /// The first anim a stock event clause names on `torso` or the legs.
    fn event_anim(anims: &PlayerAnims, event: &str, torso: bool) -> i32 {
        let block = anims.script.event(event).expect(event);
        let name = block
            .clauses
            .iter()
            .flat_map(|c| if torso { &c.torso } else { &c.legs })
            .next()
            .expect("an anim line")
            .name
            .clone();
        anims.wire_of(&name).expect(&name)
    }

    fn seen(x: f32, eflags: i32) -> Seen {
        Seen {
            client_num: 3,
            tr_base: Vec3::new(x, 0.0, 0.0),
            eflags,
        }
    }

    /// The cgame's update runs every drawn frame but measures only when the
    /// newest snapshot moved, and the clip's time runs on the drawn clock
    /// without a jump across the rate change.
    #[test]
    fn a_drawn_clock_measures_once_a_snapshot() {
        let Some(fs) = crate::testing::game_fs() else {
            return;
        };
        let anims = PlayerAnims::load(&fs).unwrap();
        let run = anims.wire_of("pb_combatrun_forward_loop").unwrap();
        let speed = anims.motion(run).unwrap().move_speed as f32;
        let mut c = ChannelClock::default();
        let mut last = None;
        // 10 units a snapshot is 200 u/s; snapshots every 50 ms, drawn
        // frames every 7, the newest snapshot 40 ms ahead of the drawn clock.
        for r in (10_000..10_500).step_by(7) {
            let snap = (r + 40) / 50 * 50;
            let at = Times {
                clock_ms: r,
                snapshot_ms: snap,
            };
            let before = c.seconds(r);
            c.update(&anims, true, run, &seen(snap as f32 * 0.2, 0), at);
            if let Some(prev) = last {
                // The rate change rebases on this frame's clock.
                assert_eq!(c.seconds(r), before, "{r}");
                assert!(c.seconds(r) > prev);
            }
            last = Some(c.seconds(r));
        }
        assert_eq!(c.rate(), 200.0 / speed);
        // A drawn frame with no newer snapshot keeps the rate.
        c.update(
            &anims,
            true,
            run,
            &seen(0.0, 0),
            Times {
                clock_ms: 10_500,
                snapshot_ms: 10_500,
            },
        );
        c.update(
            &anims,
            true,
            run,
            &seen(500.0, 0),
            Times {
                clock_ms: 10_507,
                snapshot_ms: 10_500,
            },
        );
        assert_eq!(c.rate(), 200.0 / speed);
    }

    /// A death anim plays from its start on a body `cloneplayer` just made
    /// and shows its last frame, alone, on any other.
    #[test]
    fn a_death_anim_starts_only_on_a_fresh_body() {
        let Some(fs) = crate::testing::game_fs() else {
            return;
        };
        let anims = PlayerAnims::load(&fs).unwrap();
        let death = event_anim(&anims, "death", false);
        let m = anims.motion(death).unwrap();
        assert!(m.death && m.move_speed == 0 && !m.looping);
        let idle = anims.wire_of("pb_stand_alert").unwrap();
        for (eflags, start, layers) in [(EF_FRESH_CORPSE, 0.0, 2), (0, m.duration, 1)] {
            let mut c = ChannelClock::default();
            c.update(
                &anims,
                true,
                idle,
                &seen(0.0, eflags),
                Times::end_frame(1000),
            );
            c.update(
                &anims,
                true,
                death,
                &seen(0.0, eflags),
                Times::end_frame(1050),
            );
            // Halfway through the fresh body's 170 ms blend out of the idle.
            let l = c.layers(1135);
            assert_eq!(l.len(), layers);
            assert!((l.last().unwrap().seconds - start - 0.085).abs() < 1e-4);
        }
    }

    /// A torso anim sends the torso node to 1 and the legs node to 0.01 over
    /// its blend, each from where it stood; clearing it sends them back.
    #[test]
    fn a_torso_anim_weights_the_torso_node_over_the_legs() {
        let Some(fs) = crate::testing::game_fs() else {
            return;
        };
        let anims = PlayerAnims::load(&fs).unwrap();
        let fire = event_anim(&anims, "fireweapon", true);
        let mut c = ChannelClock::default();
        let at = |t| Times::end_frame(t);
        c.update(&anims, false, 0, &seen(0.0, 0), at(1000));
        assert_eq!(c.node_weights(1000), NodeWeights::default());
        c.update(&anims, false, fire, &seen(0.0, 0), at(1000));
        let w = c.node_weights(1015);
        assert!(
            (w.torso - 0.5).abs() < 1e-6 && (w.legs - 0.5).abs() < 1e-6,
            "{w:?}"
        );
        assert_eq!(
            c.node_weights(1030),
            NodeWeights {
                torso: 1.0,
                legs: LEGS_UNDER_TORSO
            }
        );
        // Back to no anim halfway: the torso node from 1 over the whole
        // blend, the legs node from 0.01 over 0.99 of it.
        c.update(&anims, false, 0, &seen(0.0, 0), at(2000));
        let blend = blend_ms(&anims, fire, 0, false) as f32;
        assert_eq!(blend % 2.0, 0.0);
        let w = c.node_weights(2000 + (blend / 2.0) as i32);
        assert!((w.torso - 0.5).abs() < 1e-6, "{w:?}");
        assert!((w.legs - 0.51).abs() < 1e-6, "{w:?}");
    }

    /// A `FIREWEAPON` anim carries its own 30 ms blend (0x2888d).
    #[test]
    fn a_fire_anim_blends_in_over_30_ms() {
        let Some(fs) = crate::testing::game_fs() else {
            return;
        };
        let anims = PlayerAnims::load(&fs).unwrap();
        let fire = event_anim(&anims, "fireweapon", true);
        assert_eq!(anims.motion(fire).unwrap().blend_ms, 30);
        assert_eq!(blend_ms(&anims, 0, fire, false), 30);
    }
}
