//! The bots' navigation graph: a grid flood-filled out of the map's spawn
//! points, each edge proven by running pmove along it, and A* over it.
//! Design and build times: `docs/research/bot-navigation.md`.

use crate::game::trigger::{BrushHull, box_contacts_hulls};
use crate::server::FRAME_MS;
use glam::Vec3;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use vcod_common::collision::{CollisionWorld, MASK_PLAYERSOLID, Prim};
use vcod_common::movetrace::MoveWorld;
use vcod_common::net::msg::{NULL_USERCMD, UserCmd};
use vcod_common::pmove::PlayerState;
use vcod_common::pmove::cmd::{ANGLE2SHORT, EventRing, chop, player_step};

/// Grid pitch bounds in world units. The finest is about a capsule width
/// plus the slack a doorway leaves; a large map coarsens toward the other
/// to keep its node count near [`TARGET_CELLS`] (bot-navigation.md,
/// "Spacing"). At 64 mp_hurtgen's Retrieval bunker, the objective's room,
/// came out as islands its stairs never joined.
const SPACING_MIN: f32 = 32.0;
const SPACING_MAX: f32 = 48.0;
/// The cells the spawn points' bounding box is cut into.
const TARGET_CELLS: f32 = 20_000.0;
/// How far past the spawn points' bounding box the flood goes. Stock maps
/// put spawns at the edges of play; past this lies the scenery behind the
/// boundary (mp_hurtgen's forest is ~2000 units of it).
const MARGIN: f32 = 512.0;
/// Two nodes in one grid column are one node when their floors are closer
/// than this; a full storey is ~128.
const Z_MERGE: f32 = 40.0;
/// A walk counts as arrived this close to its target, horizontally. One 50
/// ms tick at run speed covers 9.5 units, so the walk cannot step over it.
/// A walk bound for a node's floor also ends this close to its height
/// (bot-navigation.md, "The flood": mp_ship's hull beam).
const ARRIVE: f32 = 8.0;
/// A walk that drops further than this is refused: past
/// `bg_fallDamageMinHeight` (256) the drop hurts.
const MAX_DROP: f32 = 200.0;
/// An edge whose ends differ by less than this in height is level, and a
/// walk one way stands in for the walk back.
const LEVEL: f32 = 1.0;
/// The flood stops here; a stock map fills well under it.
const MAX_NODES: usize = 60_000;
/// The longest a walk runs with a ladder in it: 15 s, ~1400 units of climb
/// at the 95 units/s a full-rate climb makes. mp_ship's hold ladder is 560.
const LADDER_TICKS: usize = 300;
/// Degrees up a climbing body looks; past ~9 deg `ladder_move`'s upscale
/// saturates. Shared with the bots, which climb the same way.
pub const LADDER_PITCH: f32 = 45.0;
/// A* gives up past this many expansions and reports no path.
const MAX_EXPANSIONS: usize = 40_000;

const CARDINALS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
const DIAGONALS: [(i32, i32); 4] = [(1, 1), (-1, 1), (-1, -1), (1, -1)];

pub struct NavGraph {
    /// Feet origins where pmove came to rest.
    pub nodes: Vec<Vec3>,
    /// Directed: `edges[a]` holds every `b` a run from `a` reaches. A drop
    /// off a ledge has no edge back.
    pub edges: Vec<Vec<u32>>,
    /// The edges a walk proved only with a jump; a bot following one jumps
    /// the way the walk did ([`jump_cue`]).
    pub jumps: HashSet<(u32, u32)>,
    /// The jump edges proved from a standstill at their foot, jumping at the
    /// lip ([`NavGraph::link_leaps`]): a bot comes to rest at the foot first.
    pub leaps: HashSet<(u32, u32)>,
    /// Drops deeper than a jump whose walk from 4 units aside of their node
    /// falls: a plan takes one only where there
    /// is no other way ([`RISK_COST`], [`NavGraph::mark_risky`]).
    pub risky: HashSet<(u32, u32)>,
    /// The grid pitch the graph was built on.
    pub spacing: f32,
    /// Nodes by grid column, for lookups only; nothing iterates it.
    columns: HashMap<(i32, i32), Vec<u32>>,
    /// Each node's strongly connected component ([`Self::components`]), and
    /// each component's nodes in index order: a roam picks its destination
    /// among the nodes its start reaches both ways.
    comp: Vec<u32>,
    members: Vec<Vec<u32>>,
    /// Per component, a bitset of the components it reaches (itself
    /// included): an unreachable goal fails before any search. A stock map
    /// has 1 to 13 components.
    reach: Vec<Vec<u64>>,
    /// The world's ladders ([`ladders`]), for the build.
    ladder_boxes: Vec<(Vec3, Vec3)>,
}

/// The spawn points' horizontal bounding box.
fn seed_bounds(seeds: &[[f32; 3]]) -> (glam::Vec2, glam::Vec2) {
    seeds.iter().fold(
        (glam::Vec2::splat(f32::MAX), glam::Vec2::splat(f32::MIN)),
        |(lo, hi), s| {
            let p = glam::Vec2::new(s[0], s[1]);
            (lo.min(p), hi.max(p))
        },
    )
}

impl NavGraph {
    /// Floods the grid from `seeds` (feet origins, typically every spawn
    /// point of the map) and keeps an edge only where a standing run, one
    /// usercmd per 50 ms through the shared cmd step, arrives. No node
    /// stands where a body would touch one of `hazards`.
    pub fn build(world: &CollisionWorld, seeds: &[[f32; 3]], hazards: &[BrushHull]) -> NavGraph {
        // The pitch cuts the spawns' box into about TARGET_CELLS, rounded up
        // to a multiple of 16; the flood stays within MARGIN of the box.
        let (lo, hi) = seed_bounds(seeds);
        let area = (hi - lo).max(glam::Vec2::ZERO).element_product();
        let spacing = ((area / TARGET_CELLS).sqrt() / 16.0).ceil() * 16.0;
        let (lo, hi) = (
            lo - glam::Vec2::splat(MARGIN),
            hi + glam::Vec2::splat(MARGIN),
        );
        let inside = |p: Vec3| {
            let p = p.truncate();
            p.cmpge(lo).all() && p.cmple(hi).all()
        };
        let mut g = NavGraph {
            nodes: Vec::new(),
            edges: Vec::new(),
            jumps: HashSet::new(),
            leaps: HashSet::new(),
            risky: HashSet::new(),
            spacing: spacing.clamp(SPACING_MIN, SPACING_MAX),
            columns: HashMap::new(),
            comp: Vec::new(),
            members: Vec::new(),
            reach: Vec::new(),
            ladder_boxes: ladders(world),
        };
        let mut queue = Vec::new();
        for seed in seeds {
            let Some(p) = settle(world, Vec3::from(*seed), 0.0) else {
                continue;
            };
            // On the lattice when the seed can walk to its own column's
            // centre, so every later walk targets a node's real spot.
            let c = g.column(p);
            let p = walk(world, &g.ladder_boxes, p, g.center(c)).map_or(p, |(p, _)| p);
            if g.find(c, p.z).is_none() {
                queue.push(g.add(c, p));
            }
        }
        g.flood(world, hazards, &inside, queue);
        let ends = g.link_ladders(world, hazards);
        g.flood(world, hazards, &inside, ends);
        g.link_back(world);
        g.link_leaps(world);
        g.mark_risky(world);
        g.index_components();
        g
    }

    /// Marks [`Self::risky`]: every drop deeper than a jump off a jump
    /// edge, walked again in the first gait that
    /// lands it from its node, from 4 units aside each way. mp_ship's ledge
    /// at (3685.5, -354.3, 400.1), narrow over the hold at 92, lands the
    /// back down from (3647, -352.9, 611.1) and misses from 4 units south;
    /// bots that came onto it fell past the ledge whatever they were held
    /// to (bot-navigation.md section 3).
    fn mark_risky(&mut self, world: &CollisionWorld) {
        let drops: Vec<(u32, u32)> = self
            .edges
            .iter()
            .enumerate()
            .flat_map(|(a, out)| out.iter().map(move |&b| (a as u32, b)))
            .filter(|&(a, b)| {
                let (p, q) = (self.nodes[a as usize], self.nodes[b as usize]);
                q.z < p.z - vcod_common::pmove::JUMP_HEIGHT && !self.jumps.contains(&(a, b))
            })
            .collect();
        let risky = par_map(&drops, |&(a, b)| {
            let (p, q) = (self.nodes[a as usize], self.nodes[b as usize]);
            let lands = |from: Vec3, gait| {
                matches!(
                    walk_as(world, from, q.truncate(), Some(q.z), gait),
                    Walked::Arrived(..)
                )
            };
            // No gait lands it node to node: the edge stands on a walk that
            // ran from beside the node (mp_ship's deck at (3515, -323, 631)
            // over the ledge at 580, which bots ran off and fell past).
            let Some(gait) = [Gait::Forward, Gait::Backward, Gait::Creep]
                .into_iter()
                .find(|&gait| lands(p, gait))
            else {
                return true;
            };
            [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y].iter().any(|&d| {
                let aside = p + d * RISK_ASIDE;
                let from = settle(world, aside, 0.0).unwrap_or(aside);
                matches!(
                    walk_as(world, from, q.truncate(), Some(q.z), gait),
                    Walked::Fell
                )
            })
        });
        self.risky = drops
            .into_iter()
            .zip(risky)
            .filter_map(|(e, r)| r.then_some(e))
            .collect();
    }

    /// Floods the grid out of `frontier`: the spawn points' nodes, then the
    /// ladders' ends, whose floors a walk between neighbouring columns may
    /// never reach. New nodes stay `inside` and out of `hazards`.
    fn flood(
        &mut self,
        world: &CollisionWorld,
        hazards: &[BrushHull],
        inside: &dyn Fn(Vec3) -> bool,
        mut frontier: Vec<u32>,
    ) {
        // Breadth-first by layers. A layer's walks depend only on where its
        // nodes stand, so they run on every core, and the merge takes them in
        // job order, which keeps the graph identical to a serial flood
        // whatever the thread count. A layer's diagonals run two layers late,
        // once the square's other three corners have walked their sides.
        let mut late: VecDeque<Vec<u32>> = VecDeque::new();
        loop {
            let diagonal = if late.len() >= 2 || frontier.is_empty() {
                late.pop_front().unwrap_or_default()
            } else {
                Vec::new()
            };
            if frontier.is_empty() && diagonal.is_empty() {
                break;
            }
            let jobs: Vec<(u32, &[(i32, i32); 4])> = frontier
                .iter()
                .map(|&n| (n, &CARDINALS))
                .chain(diagonal.iter().map(|&n| (n, &DIAGONALS)))
                .collect();
            let ends = par_map(&jobs, |&(n, dirs)| {
                let (cx, cy) = self.column(self.nodes[n as usize]);
                // A rest on a ladder's top is no node: only its exits lead
                // on from there (`link_ladders`).
                dirs.map(|(dx, dy)| {
                    self.step(world, n, (cx + dx, cy + dy))
                        .filter(|&(p, _)| !perched(world, p))
                })
            });
            let mut next = Vec::new();
            let mut recheck = Vec::new();
            for (&(n, dirs), ends) in jobs.iter().zip(ends) {
                let (cx, cy) = self.column(self.nodes[n as usize]);
                for (&(dx, dy), to) in dirs.iter().zip(ends) {
                    let Some((to, jumped)) = to else {
                        continue;
                    };
                    let c = (cx + dx, cy + dy);
                    let from = self.nodes[n as usize];
                    let m = match self.find(c, to.z) {
                        // A rest off the column's node's floor proves no
                        // edge to it: walk to the node itself.
                        Some(m) if (self.nodes[m as usize].z - to.z).abs() >= ARRIVE => {
                            recheck.push((n, m, c, to, jumped, false));
                            continue;
                        }
                        // Nor does one beside a node off the column's centre
                        // at the foot of a drop deeper than a jump: the
                        // landing can be narrow (bot-navigation.md section 3).
                        Some(m)
                            if to.z < from.z - vcod_common::pmove::JUMP_HEIGHT
                                && (self.nodes[m as usize] - to).truncate().length() > 1.0 =>
                        {
                            recheck.push((n, m, c, to, jumped, true));
                            continue;
                        }
                        Some(m) => m,
                        None if self.nodes.len() < MAX_NODES
                            && inside(to)
                            && !hazard(hazards, to) =>
                        {
                            let m = self.add(c, to);
                            next.push(m);
                            m
                        }
                        None => continue,
                    };
                    self.link_as(n, m, jumped);
                }
            }
            let walked = par_map(&recheck, |&(n, m, ..)| {
                let to = self.nodes[m as usize];
                let from = self.nodes[n as usize];
                walk_to(world, &self.ladder_boxes, from, to.truncate(), Some(to.z))
            });
            for (&(n, m, c, to, jumped, beside), w) in recheck.iter().zip(walked) {
                if let Some((_, jumped)) = w {
                    self.link_as(n, m, jumped);
                    continue;
                }
                if beside {
                    continue;
                }
                // Then the rest is a floor of its own.
                let m = match self.find(c, to.z) {
                    Some(m) if (self.nodes[m as usize].z - to.z).abs() < ARRIVE => m,
                    _ if self.nodes.len() < MAX_NODES && inside(to) && !hazard(hazards, to) => {
                        let m = self.add(c, to);
                        next.push(m);
                        m
                    }
                    _ => continue,
                };
                self.link_as(n, m, jumped);
            }
            if !frontier.is_empty() {
                late.push_back(frontier);
            }
            frontier = next;
        }
    }

    /// The spawn points in the strongly connected component holding the most
    /// of them: the census bot-navigation.md's tables report. Each spawn is
    /// looked up from where a body dropped at it lands, where a bot spawned
    /// there stands; from the spawn's own origin, often 100 units up, the
    /// nearest node can be one on a crate beside it.
    pub fn spawns_in_one_component(&self, world: &CollisionWorld, spawns: &[[f32; 3]]) -> usize {
        let mut count = BTreeMap::new();
        for s in spawns {
            let feet = settle(world, Vec3::from(*s), 0.0).map_or(*s, Into::into);
            if let Some(n) = self.nearest(feet) {
                *count.entry(self.comp[n as usize]).or_insert(0) += 1;
            }
        }
        count.into_values().max().unwrap_or(0)
    }

    fn index_components(&mut self) {
        self.comp = self.components();
        let count = self.comp.iter().max().map_or(0, |&c| c as usize + 1);
        self.members = vec![Vec::new(); count];
        for (n, &c) in self.comp.iter().enumerate() {
            self.members[c as usize].push(n as u32);
        }
        // The condensation's edges, then a walk of them from each component.
        let mut out = vec![HashSet::new(); count];
        for (a, next) in self.edges.iter().enumerate() {
            let ca = self.comp[a];
            for &b in next {
                let cb = self.comp[b as usize];
                if cb != ca {
                    out[ca as usize].insert(cb);
                }
            }
        }
        let words = count.div_ceil(64);
        self.reach = (0..count)
            .map(|root| {
                let mut bits = vec![0u64; words];
                let mut stack = vec![root as u32];
                bits[root / 64] |= 1 << (root % 64);
                while let Some(c) = stack.pop() {
                    for &d in &out[c as usize] {
                        let (w, b) = (d as usize / 64, d % 64);
                        if bits[w] & (1 << b) == 0 {
                            bits[w] |= 1 << b;
                            stack.push(d);
                        }
                    }
                }
                bits
            })
            .collect();
    }

    /// Whether some path leads from node `a` to node `b`.
    pub fn reaches(&self, a: u32, b: u32) -> bool {
        let (Some(&ca), Some(&cb)) = (self.comp.get(a as usize), self.comp.get(b as usize)) else {
            return false;
        };
        self.reach[ca as usize][cb as usize / 64] & (1 << (cb % 64)) != 0
    }

    /// The node `from` reaches that lies nearest `to`, `to` itself when it
    /// is reached; `None` when that is `from` and `from` is not `to`.
    fn nearest_reached(&self, from: u32, to: u32) -> Option<u32> {
        if self.reaches(from, to) {
            return Some(to);
        }
        let goal = self.nodes[to as usize];
        let ca = self.comp[from as usize] as usize;
        let best = self
            .members
            .iter()
            .enumerate()
            .filter(|(c, _)| self.reach[ca][c / 64] & (1 << (c % 64)) != 0)
            .flat_map(|(_, m)| m)
            .min_by(|a, b| {
                let d = |n: u32| self.nodes[n as usize].distance_squared(goal);
                d(**a).total_cmp(&d(**b)).then(a.cmp(b))
            })?;
        (*best != from).then_some(*best)
    }

    /// The flood walks to a neighbouring column only, so a ledge across a
    /// gap is never tried: mp_depot's Retrieval documents lie on a crate top
    /// 32 units over a step, a 32-unit gap between them. Every node up to two
    /// columns off and between a step and a jump higher, that three edges
    /// don't already reach, is run at from a standstill with a jump at the
    /// lip ([`Gait::Leap`]). A ladder's head is no foot: a body comes to
    /// rest there where the climb tips it over, a few units off the spot
    /// the leap would be proved from (mp_ship's hold ladder, section 2
    /// "Jumps").
    fn link_leaps(&mut self, world: &CollisionWorld) {
        use vcod_common::pmove::{JUMP_HEIGHT, STEPSIZE};
        let mut heads: HashSet<u32> = HashSet::new();
        for (a, out) in self.edges.iter().enumerate() {
            let z = self.nodes[a].z + JUMP_HEIGHT;
            heads.extend(out.iter().filter(|&&b| self.nodes[b as usize].z > z));
        }
        let mut jobs = Vec::new();
        for (a, &p) in self.nodes.iter().enumerate() {
            if heads.contains(&(a as u32)) {
                continue;
            }
            let (cx, cy) = self.column(p);
            let mut near: Option<HashSet<u32>> = None;
            for dx in -2..=2i32 {
                for dy in -2..=2i32 {
                    if dx.abs().max(dy.abs()) < 2 {
                        continue;
                    }
                    for &m in self.columns.get(&(cx + dx, cy + dy)).into_iter().flatten() {
                        let rise = self.nodes[m as usize].z - p.z;
                        if rise > STEPSIZE
                            && rise <= JUMP_HEIGHT
                            && !near
                                .get_or_insert_with(|| self.within(a as u32, 3))
                                .contains(&m)
                        {
                            jobs.push((a as u32, m));
                        }
                    }
                }
            }
        }
        let walked = par_map(&jobs, |&(a, b)| {
            let to = self.nodes[b as usize];
            let from = self.nodes[a as usize];
            matches!(
                walk_as(world, from, to.truncate(), Some(to.z), Gait::Leap),
                Walked::Arrived(..)
            )
        });
        for (&(a, b), ok) in jobs.iter().zip(walked) {
            if ok && !self.edges[a as usize].contains(&b) {
                self.link_as(a, b, true);
                self.leaps.insert((a, b));
            }
        }
    }

    /// The nodes `hops` edges or fewer from `a`.
    fn within(&self, a: u32, hops: usize) -> HashSet<u32> {
        let mut seen = HashSet::from([a]);
        let mut layer = vec![a];
        for _ in 0..hops {
            let mut next = Vec::new();
            for n in layer {
                for &m in &self.edges[n as usize] {
                    if seen.insert(m) {
                        next.push(m);
                    }
                }
            }
            layer = next;
        }
        seen
    }

    /// The flood walks toward a column's centre, so a node that stands off
    /// its centre (a spawn point against a wall, which never reached it) is
    /// walked out of but never into: its neighbours aim at the centre and
    /// meet the wall. Every one-way edge between nodes on one floor is
    /// walked back node to node.
    fn link_back(&mut self, world: &CollisionWorld) {
        let mut jobs = Vec::new();
        for (a, out) in self.edges.iter().enumerate() {
            let p = self.nodes[a];
            for &b in out {
                let q = self.nodes[b as usize];
                if (q.z - p.z).abs() < Z_MERGE && !self.edges[b as usize].contains(&(a as u32)) {
                    jobs.push((b, a as u32));
                }
            }
        }
        self.walk_jobs(world, &jobs, true);
    }

    /// The flood walks only to a neighbouring column, which rarely lines up
    /// with a ladder. So each ladder face gets two nodes of its own: a foot
    /// in front of the face and a head where a climb up it comes to rest,
    /// linked where the bots' own steering proves the way, and each linked
    /// to the graph around it (bot-navigation.md, "Ladders").
    fn link_ladders(&mut self, world: &CollisionWorld, hazards: &[BrushHull]) -> Vec<u32> {
        let faces: Vec<(Vec3, Vec3, Vec3)> = self
            .ladder_boxes
            .iter()
            .copied()
            .flat_map(|(lo, hi)| {
                let ext = hi - lo;
                let axis = if ext.x < ext.y { Vec3::X } else { Vec3::Y };
                [(lo, hi, axis), (lo, hi, -axis)]
            })
            .collect();
        let rungs = par_map(&faces, |&(lo, hi, n)| {
            rung(world, lo, hi, n).map(|r| (r, ladder_exits(world, r.1, n)))
        });
        let mut ends = Vec::new();
        let safe = |r: &(Vec3, Vec3, bool, bool)| !hazard(hazards, r.0) && !hazard(hazards, r.1);
        for ((foot, head, up, down), exits) in rungs.into_iter().flatten().filter(|(r, _)| safe(r))
        {
            let f = self.add(self.column(foot), foot);
            let h = self.add(self.column(head), head);
            if up {
                self.link(f, h);
            }
            if down {
                self.link(h, f);
            }
            ends.push(f);
            // A head perched on the ladder's top leads only to the floor
            // its exits walked to: the walks across a hatch from the perch
            // arrive or fall by a few units (bot-navigation.md, "Ladders").
            if exits.is_empty() {
                ends.push(h);
            }
            for (exit, back) in exits.into_iter().filter(|&(e, _)| !hazard(hazards, e)) {
                let e = self.add(self.column(exit), exit);
                self.link(h, e);
                if back {
                    self.link(e, h);
                }
                ends.push(e);
            }
        }
        // Each end to the nodes on its floor around it, each way walked.
        let mut jobs = Vec::new();
        for &e in &ends {
            let p = self.nodes[e as usize];
            let (cx, cy) = self.column(p);
            for dx in -2..=2 {
                for dy in -2..=2 {
                    for &m in self.columns.get(&(cx + dx, cy + dy)).into_iter().flatten() {
                        let q = self.nodes[m as usize];
                        if m != e && (q.z - p.z).abs() < Z_MERGE {
                            jobs.extend([(e, m), (m, e)]);
                        }
                    }
                }
            }
        }
        self.walk_jobs(world, &jobs, false);
        ends
    }

    /// Walks each `(a, b)` node to node on `b`'s floor and links the ones
    /// that arrive. With `sidestep`, a walk that does not is tried again
    /// at [`SIDESTEP`] either side of `b`, across the line: a run pinned on
    /// a door frame's corner can slide past it a few units over.
    fn walk_jobs(&mut self, world: &CollisionWorld, jobs: &[(u32, u32)], sidestep: bool) {
        let walked = par_map(jobs, |&(a, b)| {
            let to = self.nodes[b as usize];
            let from = self.nodes[a as usize];
            let across = (to - from).truncate().perp().normalize_or_zero() * SIDESTEP;
            let tries: &[f32] = if sidestep { &[0.0, 1.0, -1.0] } else { &[0.0] };
            tries.iter().find_map(|&k| {
                let target = to.truncate() + across * k;
                match walk_as(world, from, target, Some(to.z), Gait::Forward) {
                    Walked::Arrived(_, jumped) => Some(jumped),
                    _ => None,
                }
            })
        });
        for (&(a, b), jumped) in jobs.iter().zip(walked) {
            if let Some(jumped) = jumped {
                self.link_as(a, b, jumped);
            }
        }
    }

    /// Where a run from node `n` toward column `c`'s centre comes to rest,
    /// skipping the walk when the graph already proves the answer: a level
    /// cardinal edge already walked the other way (half a flood's walks), or
    /// a level diagonal across a square whose four sides run both ways.
    fn step(&self, world: &CollisionWorld, n: u32, c: (i32, i32)) -> Option<(Vec3, bool)> {
        let from = self.nodes[n as usize];
        let (cx, cy) = self.column(from);
        let level = |m: u32| (self.nodes[m as usize].z - from.z).abs() < LEVEL;
        // Walked without a jump: a jump one way says nothing of the other.
        let has =
            |a: u32, b: u32| self.edges[a as usize].contains(&b) && !self.jumps.contains(&(a, b));
        if let Some(d) = self.find(c, from.z).filter(|&d| level(d)) {
            let proven = if c.0 == cx || c.1 == cy {
                has(d, n)
            } else {
                match (self.find((c.0, cy), from.z), self.find((cx, c.1), from.z)) {
                    (Some(x), Some(y)) => {
                        level(x)
                            && level(y)
                            && [(n, x), (x, d), (n, y), (y, d)]
                                .iter()
                                .all(|&(a, b)| has(a, b) && has(b, a))
                    }
                    _ => false,
                }
            };
            if proven {
                return Some((self.nodes[d as usize], false));
            }
        }
        walk(world, &self.ladder_boxes, from, self.center(c))
    }

    fn column(&self, p: Vec3) -> (i32, i32) {
        (
            (p.x / self.spacing).round() as i32,
            (p.y / self.spacing).round() as i32,
        )
    }

    fn center(&self, c: (i32, i32)) -> glam::Vec2 {
        glam::Vec2::new(c.0 as f32, c.1 as f32) * self.spacing
    }

    fn link(&mut self, a: u32, b: u32) {
        self.link_as(a, b, false);
    }

    /// Links `a -> b`, a jump edge when `jumped`. A walk that proves an edge
    /// without a jump clears its jump.
    fn link_as(&mut self, a: u32, b: u32, jumped: bool) {
        if a == b {
            return;
        }
        if !self.edges[a as usize].contains(&b) {
            self.edges[a as usize].push(b);
            if jumped {
                self.jumps.insert((a, b));
            }
        } else if !jumped {
            self.jumps.remove(&(a, b));
        }
    }

    fn add(&mut self, c: (i32, i32), p: Vec3) -> u32 {
        let n = self.nodes.len() as u32;
        self.nodes.push(p);
        self.edges.push(Vec::new());
        self.columns.entry(c).or_default().push(n);
        n
    }

    /// Column `c`'s node nearest height `z`, within [`Z_MERGE`].
    fn find(&self, c: (i32, i32), z: f32) -> Option<u32> {
        let dz = |n: u32| (self.nodes[n as usize].z - z).abs();
        self.columns
            .get(&c)?
            .iter()
            .copied()
            .filter(|&n| dz(n) < Z_MERGE)
            .min_by(|&a, &b| dz(a).total_cmp(&dz(b)))
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.iter().map(Vec::len).sum()
    }

    /// The node nearest `p` among the 5x5 columns around it, a floor's height
    /// difference costing four times a horizontal unit so a node one storey
    /// up loses to one beside the feet. `None` off the graph.
    pub fn nearest(&self, p: [f32; 3]) -> Option<u32> {
        let p = Vec3::from(p);
        let (cx, cy) = self.column(p);
        let mut best: Option<(f32, u32)> = None;
        for dy in -2..=2 {
            for dx in -2..=2 {
                let Some(col) = self.columns.get(&(cx + dx, cy + dy)) else {
                    continue;
                };
                for &n in col {
                    let d = self.nodes[n as usize] - p;
                    let score = d.truncate().length() + 4.0 * d.z.abs();
                    // Ties go to the lower index, so the pick never depends
                    // on the order the columns are visited in.
                    if best.is_none_or(|(s, b)| score < s || (score == s && n < b)) {
                        best = Some((score, n));
                    }
                }
            }
        }
        best.map(|(_, n)| n)
    }

    /// Whether [`Self::nearest`] to `p` has no edge out: a ledge the flood
    /// found no way off but a fall (mp_ship's bow gutters at y -152 and 281,
    /// z 312, under a roof too steep to climb and over a drop of 376).
    pub fn dead_end(&self, p: [f32; 3]) -> bool {
        self.nearest(p)
            .is_some_and(|n| self.edges[n as usize].is_empty())
    }

    /// Whether [`Self::nearest`] to `p` stands more than a step off its
    /// height: a body off the graph, whose plan starts at a node it walks
    /// to ([`Self::nearest_walkable`]).
    pub fn off_floor(&self, p: Vec3) -> bool {
        self.nearest(p.into())
            .is_some_and(|n| (self.nodes[n as usize].z - p.z).abs() > vcod_common::pmove::STEPSIZE)
    }

    /// For a body on a ladder, [`Self::nearest`] among the ends of the
    /// climbs around it: nodes an edge climbs more than a jump from or to.
    /// The nearest node to a body on the rungs is often a floor behind its
    /// back, which a ladder move does not reach (mp_ship's ladder at
    /// x 3354, where a plan from the floor at (3427, 546, 128) held a
    /// climber on the rungs for 100 ticks). `nearest` when there is none.
    pub fn nearest_ladder_end(&self, p: Vec3) -> Option<u32> {
        use vcod_common::pmove::JUMP_HEIGHT;
        let (cx, cy) = self.column(p);
        let near: Vec<u32> = (-2..=2)
            .flat_map(|dy| (-2..=2).map(move |dx| (cx + dx, cy + dy)))
            .filter_map(|c| self.columns.get(&c))
            .flatten()
            .copied()
            .collect();
        let z = |n: u32| self.nodes[n as usize].z;
        let mut ends: HashSet<u32> = HashSet::new();
        for &n in &near {
            for &b in &self.edges[n as usize] {
                if z(b) > z(n) + JUMP_HEIGHT {
                    ends.extend([n, b]);
                }
            }
        }
        let score = |n: u32| {
            let d = self.nodes[n as usize] - p;
            d.truncate().length() + 4.0 * d.z.abs()
        };
        ends.into_iter()
            .filter(|n| near.contains(n))
            .min_by(|&a, &b| score(a).total_cmp(&score(b)).then(a.cmp(&b)))
            .or_else(|| self.nearest(p.into()))
    }

    /// [`Self::nearest`], or of its nearest few by the same score the first
    /// a walk from `p` arrives at on its floor without a jump (a bot jumps
    /// only on a jump edge, never to its first node): a body that fell off its
    /// path can stand where the nearest node is a wall away. mp_ship's lip at
    /// (5424, -187, 344), under the shelf at 396 and over a stair, has the
    /// floor at 352 beyond the shelf's side nearest; the stair node at 320
    /// is a walk off the lip. Where none of those arrives, the nearest few
    /// more than a jump below, flat, get the same walk: a pit off the graph
    /// whose way out is a drop scores the floors over its rim nearest
    /// (mp_ship's boat at (5250, -500, 27), bot-navigation.md section 3).
    pub fn nearest_walkable(&self, world: &CollisionWorld, p: Vec3) -> Option<u32> {
        use vcod_common::pmove::JUMP_HEIGHT;
        let (cx, cy) = self.column(p);
        let mut near: Vec<(f32, u32)> = (-2..=2)
            .flat_map(|dy| (-2..=2).map(move |dx| (cx + dx, cy + dy)))
            .filter_map(|c| self.columns.get(&c))
            .flatten()
            .map(|&n| {
                let d = self.nodes[n as usize] - p;
                (d.truncate().length() + 4.0 * d.z.abs(), n)
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let walks = |n: u32| {
            let to = self.nodes[n as usize];
            matches!(
                walk_as(world, p, to.truncate(), Some(to.z), Gait::Forward),
                Walked::Arrived(_, false)
            )
        };
        let tried = &near[..near.len().min(WALKABLE_TRIES)];
        let mut below: Vec<(f32, u32)> = near
            .iter()
            .filter(|&&(_, n)| {
                self.nodes[n as usize].z < p.z - JUMP_HEIGHT && !tried.iter().any(|t| t.1 == n)
            })
            .map(|&(_, n)| ((self.nodes[n as usize] - p).truncate().length(), n))
            .collect();
        below.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        tried
            .iter()
            .chain(below.iter().take(WALKABLE_TRIES))
            .find(|&&(_, n)| walks(n))
            .or(near.first())
            .map(|&(_, n)| n)
    }

    /// A* from `from` to `to` over the directed edges, Euclidean cost and
    /// heuristic. The node list runs from `from` to `to` inclusive; `None`
    /// when `to` is unreachable or the search runs past its budget.
    pub fn path(&self, from: u32, to: u32) -> Option<Vec<u32>> {
        self.path_avoiding(from, to, &[])
    }

    /// [`Self::path`] without the directed edges in `avoid`.
    pub fn path_avoiding(&self, from: u32, to: u32, avoid: &[(u32, u32)]) -> Option<Vec<u32>> {
        if !self.reaches(from, to) {
            return None;
        }
        match Search::new(self, from, to, avoid.to_vec()).run(self, &mut { u32::MAX }) {
            Plan::Found(p) => Some(p),
            _ => None,
        }
    }

    /// [`Self::path`], or when `to` is out of reach, the path to the
    /// reachable node nearest it: a goal the graph does not reach (an
    /// objective down a bunker's stairs, on a pitch too coarse for them)
    /// is still closed in on along the graph, not wandered at.
    pub fn path_toward(&self, from: u32, to: u32) -> Option<Vec<u32>> {
        if from as usize >= self.nodes.len() || to as usize >= self.nodes.len() {
            return None;
        }
        self.path(from, self.nearest_reached(from, to)?)
    }

    /// Each node's strongly connected component, numbered from 0 (Kosaraju,
    /// iterative). Two nodes share a number when each reaches the other.
    pub fn components(&self) -> Vec<u32> {
        let n = self.nodes.len();
        // Pass 1: finish order of a DFS over the forward edges.
        let mut order = Vec::with_capacity(n);
        let mut seen = vec![false; n];
        for root in 0..n {
            if seen[root] {
                continue;
            }
            seen[root] = true;
            let mut stack = vec![(root as u32, 0usize)];
            while let Some((v, i)) = stack.last_mut() {
                if let Some(&w) = self.edges[*v as usize].get(*i) {
                    *i += 1;
                    if !seen[w as usize] {
                        seen[w as usize] = true;
                        stack.push((w, 0));
                    }
                } else {
                    order.push(*v);
                    stack.pop();
                }
            }
        }
        // Pass 2: flood the reversed edges in reverse finish order.
        let mut back = vec![Vec::new(); n];
        for (a, out) in self.edges.iter().enumerate() {
            for &b in out {
                back[b as usize].push(a as u32);
            }
        }
        let mut comp = vec![u32::MAX; n];
        let mut next = 0;
        for &root in order.iter().rev() {
            if comp[root as usize] != u32::MAX {
                continue;
            }
            comp[root as usize] = next;
            let mut stack = vec![root];
            while let Some(v) = stack.pop() {
                for &w in &back[v as usize] {
                    if comp[w as usize] == u32::MAX {
                        comp[w as usize] = next;
                        stack.push(w);
                    }
                }
            }
            next += 1;
        }
        comp
    }

    /// Test-facing: a graph from explicit nodes and directed edges, on the
    /// finest pitch.
    pub fn from_parts(nodes: Vec<Vec3>, edges: Vec<Vec<u32>>) -> NavGraph {
        let mut g = NavGraph {
            nodes: Vec::new(),
            edges,
            jumps: HashSet::new(),
            leaps: HashSet::new(),
            risky: HashSet::new(),
            spacing: SPACING_MIN,
            columns: HashMap::new(),
            comp: Vec::new(),
            members: Vec::new(),
            reach: Vec::new(),
            ladder_boxes: Vec::new(),
        };
        for p in nodes {
            let c = g.column(p);
            g.columns.entry(c).or_default().push(g.nodes.len() as u32);
            g.nodes.push(p);
        }
        g.index_components();
        g
    }
}

/// The graphs built so far this process, by map. A map cycle comes back
/// to a map without a second build, and the test gates that start a server
/// per test share one.
type CacheKey = (String, usize, usize, usize, usize, Vec<[u32; 3]>);
static CACHE: std::sync::Mutex<Vec<(CacheKey, Arc<NavGraph>)>> = std::sync::Mutex::new(Vec::new());

/// The key carries the collision's counts and the spawn points too, so a
/// test world built without the static props is a different graph from the
/// real one.
fn cache_key(map: &str, world: &crate::world::World) -> CacheKey {
    let c = &world.collision;
    (
        map.to_ascii_lowercase(),
        c.brushes.len(),
        c.tris.len(),
        c.model_tris.len(),
        world.hazards.len(),
        world
            .spawn_points
            .iter()
            .map(|p| p.map(f32::to_bits))
            .collect(),
    )
}

/// The cached graph under `key`, else `build`'s, cached. The lock is held
/// across the build, so two servers asking at once build it once.
fn cached(key: CacheKey, build: impl FnOnce() -> NavGraph) -> Arc<NavGraph> {
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, g)) = cache.iter().find(|(k, _)| *k == key) {
        return g.clone();
    }
    let t = std::time::Instant::now();
    let g = Arc::new(build());
    log::info!(
        "nav graph for {}: {} nodes, {} edges, spacing {}, built in {} ms",
        key.0,
        g.len(),
        g.edge_count(),
        g.spacing,
        t.elapsed().as_millis()
    );
    cache.push((key, g.clone()));
    g
}

/// `map`'s graph, built on first use on the calling thread.
pub fn graph_for(map: &str, world: &crate::world::World) -> Arc<NavGraph> {
    cached(cache_key(map, world), || {
        NavGraph::build(&world.collision, &world.spawn_points, &world.hazards)
    })
}

/// A graph on its way: built on a thread of its own over a snapshot of the
/// collision, so the server keeps ticking through a build of a second or
/// more (bot-navigation.md, "Build times").
pub struct NavJob {
    ready: Option<Arc<NavGraph>>,
    building: Option<std::thread::JoinHandle<Arc<NavGraph>>>,
}

impl NavJob {
    /// The cached graph when there is one, else a build started over a copy
    /// of `world`'s collision as it stands now: the links and poses scripts
    /// change later never reach the build.
    pub fn start(map: &str, world: &crate::world::World) -> NavJob {
        let key = cache_key(map, world);
        // A lock held elsewhere is a build in progress; the thread waits on
        // it, never the tick.
        if let Ok(cache) = CACHE.try_lock()
            && let Some((_, g)) = cache.iter().find(|(k, _)| *k == key)
        {
            return NavJob {
                ready: Some(g.clone()),
                building: None,
            };
        }
        let collision = world.collision.clone();
        let seeds = world.spawn_points.clone();
        let hazards = world.hazards.clone();
        let build = move || {
            // One core stays the tick's, so a build doesn't jitter the
            // schedule it runs beside.
            SPARE_CORE.set(true);
            cached(key, || NavGraph::build(&collision, &seeds, &hazards))
        };
        NavJob {
            ready: None,
            building: Some(std::thread::spawn(build)),
        }
    }

    /// The graph once it is built; `wait` blocks until it is.
    pub fn poll(&mut self, wait: bool) -> Option<Arc<NavGraph>> {
        if let Some(h) = self.building.take_if(|h| wait || h.is_finished()) {
            self.ready = Some(h.join().expect("the nav build panicked"));
        }
        self.ready.clone()
    }
}

/// Horizontal distance at which a waypoint counts as reached.
const REACH: f32 = 24.0;
/// A waypoint further than this many grid steps away means the bot has left
/// its path (knocked back, fell off a ledge): plan again.
const OFF_PATH_STEPS: f32 = 4.0;
/// Ticks without closing on the waypoint before the follower gives up on
/// it, and the ticks it then leaves the bot to its own unstick.
const STUCK_TICKS: u32 = 40;
const REST_TICKS: u32 = 20;
/// How far, flat, the last waypoint may be for a body to wait on it while a
/// plan is searched for ([`Follower::rest_point`]), and for how many ticks.
const WAIT_REACH: f32 = 48.0;
const WAIT_TICKS: u32 = 40;
/// The most of the tick's plan budget one follower's search takes, so a
/// long search leaves some for the bots after it.
const PLAN_SLICE: u32 = 2000;
/// How long the edge a follower got stuck on stays out of its plans. What
/// blocks a proven edge is mostly a body: a bot guarding the bomb at the
/// foot of mp_rocket's stairs held another there for 11 s.
const AVOID_TICKS: u32 = 200;
/// How far below where its path ended a body walking at a point goal has
/// to drop before the follower plans for the point again: more than a stair
/// or a step off a kerb.
const ARRIVED_DROP: f32 = 48.0;
/// How near a leap's foot, flat, a bot comes to rest before it leaps.
pub const LEAP_FOOT: f32 = 12.0;
/// A jump edge's top counts as reached only within this of its height:
/// under a stair step, over the slack a slope leaves.
const JUMP_PASS: f32 = 16.0;
/// A node the next edge drops more than a step from counts as reached only
/// within this of its height (bot-navigation.md, section 3).
const DROP_PASS: f32 = 8.0;
/// What a [`NavGraph::risky`] drop adds to a plan's cost: a long way round
/// is taken first.
const RISK_COST: f32 = 4000.0;
/// How far aside of a drop's node [`NavGraph::mark_risky`] walks it again.
const RISK_ASIDE: f32 = 4.0;
/// How many of the nearest nodes [`NavGraph::nearest_walkable`] walks to.
const WALKABLE_TRIES: usize = 8;
/// A body on the ground this far under its waypoint, off a ladder, has
/// fallen off the path.
const FELL_BELOW: f32 = 48.0;
/// Within this of a waypoint flat, a walk on that fails from where the bot
/// stands fails from the node ([`Follower::edge_holds`]).
const ON_NODE: f32 = 12.0;
/// A roam goal is picked at least this far away when it can be.
const ROAM_MIN: f32 = 1000.0;
/// A bot this near a ladder, flat, waits aside for a climber on it
/// ([`Follower::ladder_yield`]); a player this near is on its rungs, and
/// this near the bot's end in height about to step off there; the spot
/// the bot waits at is at least this far from the ladder.
const YIELD_NEAR: f32 = 64.0;
const CLIMBER_NEAR: f32 = 32.0;
const CLIMBER_CLOSE: f32 = 64.0;
const YIELD_CLEAR: f32 = 32.0;
/// Two climbers this near in height share the rungs' top abreast
/// ([`Follower::abreast_yield`]): a standing body's height.
const ABREAST_HEIGHT: f32 = 70.0;
/// A climber coming up this far under a body backing down the same rungs
/// sends it back up ([`Follower::head_on_yield`]): two bodies' height.
const HEAD_ON: f32 = 2.0 * ABREAST_HEIGHT;
/// The longest wait: a climb of mp_ship's 296-unit ladders takes about
/// 110 ticks. With the climber still, the most of it: a climber that hangs
/// on with the way clear is stuck for another reason (mp_ship's ladder at
/// x 3808). A wait that runs out is not taken again for `YIELD_REST`.
const YIELD_TICKS: u32 = 200;
const YIELD_HUNG: u32 = 40;
const YIELD_REST: u32 = 200;
/// A seen enemy has to move this far off the planned destination before the
/// path is planned again.
const RETARGET: f32 = 128.0;

/// What the body a [`Follower`] steers stands on this tick.
#[derive(Clone, Copy)]
pub struct Footing {
    /// At rest with a jump ready ([`ready_to_leap`]).
    pub still: bool,
    pub on_ground: bool,
    pub on_ladder: bool,
    /// `ps.velocity`: a drop is re-proved at the pace the body comes onto it.
    pub velocity: Vec3,
}

impl Footing {
    #[cfg(test)]
    const STANDING: Footing = Footing {
        still: true,
        on_ground: true,
        on_ladder: false,
        velocity: Vec3::ZERO,
    };
}

/// One bot's walk along the graph: the destination, the planned path and
/// the bookkeeping that notices when it has gone wrong. The server keeps one
/// per bot and asks it for the waypoint every tick.
#[derive(Default)]
pub struct Follower {
    dest: Option<u32>,
    path: Vec<u32>,
    /// Index into `path` of the current waypoint.
    next: usize,
    /// Closest the bot has come to the current waypoint, and the ticks since
    /// it last closed in.
    best: f32,
    idle: u32,
    rest: u32,
    /// The last waypoint handed out, and the ticks since one was: where
    /// the body waits while a plan is searched for ([`Self::rest_point`]).
    last: Option<Vec3>,
    waited: u32,
    /// A point goal's path is walked out; the bot heads at the point itself.
    arrived: bool,
    /// Where the body stood when the path was walked out. Walking at the
    /// point off the graph can drop it a floor (mp_depot's Retrieval
    /// documents sit by an upper floor's edge), and from there the point is
    /// planned for again.
    arrived_at: Vec3,
    /// Calls to [`Self::waypoint`], one per tick: the clock `avoid` runs on.
    clock: u32,
    /// Edges it got stuck on, left out of its plans until the tick given.
    avoid: Vec<(u32, u32, u32)>,
    /// The plan under way, and whether it leaves out `avoid` (a plain one
    /// follows if it fails).
    search: Option<Search>,
    round: bool,
    /// The threat an [`Goal::Away`](crate::bots::Goal::Away) destination
    /// was picked against.
    threat: Option<Vec3>,
    /// The body fell off its path: the next plan starts at a node it walks
    /// to ([`NavGraph::nearest_walkable`]).
    fell: bool,
    /// The body was on a ladder when the plan began: it starts at one of
    /// the ladder's ends ([`NavGraph::nearest_ladder_end`]).
    on_rungs: bool,
    /// The plan under way starts at a node a walk from the body reached
    /// or at a ladder's end, not merely the nearest.
    proved_start: bool,
    /// The last drop asked about, and whether the graph's walk ran off it
    /// ([`Self::runs_off`]).
    run_off: Option<((u32, u32), bool)>,
    /// The last edge asked about, and whether only a creep proved it
    /// ([`Self::creeps_off`]).
    creep_off: Option<((u32, u32), bool)>,
    /// The creep whose top the body comes to rest on before it heads on
    /// ([`Self::creep_top`]).
    stop_at: Option<(u32, u32)>,
    /// The edge whose first node the body comes to rest on and heads on
    /// from only at rest: its pace flung it off the way on
    /// ([`Self::flings`]).
    held: Option<(u32, u32)>,
    /// The scale the body runs at, which [`Self::flings`]' walk runs at:
    /// the held weapon's `moveSpeedScale`, cut to the graph's 1 for a
    /// light weapon on the ground (`paced_key`), so only a heavy one's
    /// shows. A body can still pass 190 units/s off a slope or a fall.
    pub move_scale: f32,
    /// Ticks spent waiting aside for a climber ([`Self::ladder_yield`]),
    /// those of them with the climber still, and the ticks before it waits
    /// again once a wait ran out.
    yield_ticks: u32,
    yield_hung: u32,
    yield_rest: u32,
}

impl Follower {
    /// The waypoint toward `goal` for a bot standing at `at` on `body`'s
    /// footing. `world`
    /// re-proves a leap from where the bot rests (none in unit tests).
    /// `budget` is the tick's shared A* budget in expanded nodes; a bot whose
    /// plan it does not finish gets no waypoint this tick and resumes the
    /// search next tick, so neither one long plan nor many bots planning at
    /// once (the tick the graph lands) stall a tick.
    /// `rand` picks roam destinations.
    #[allow(clippy::too_many_arguments)]
    pub fn waypoint(
        &mut self,
        g: &NavGraph,
        world: Option<&CollisionWorld>,
        goal: crate::bots::Goal,
        at: [f32; 3],
        body: Footing,
        budget: &mut u32,
        rand: &mut dyn FnMut() -> i32,
    ) -> Option<[f32; 3]> {
        let w = self.follow(g, world, goal, at, body, budget, rand);
        if let Some(w) = w {
            self.last = Some(w.into());
            self.waited = 0;
        } else {
            self.waited += 1;
        }
        w
    }

    /// Where a body on the ground with no waypoint this tick comes to rest
    /// while the next plan is searched for: the last waypoint, when it
    /// stands within [`WAIT_REACH`] on the body's floor, for up to
    /// [`WAIT_TICKS`]. Wandering on at a run, bots left the crow's nest's
    /// top for its hatch and mp_ship's shelf at (5403, -160, 396) for a
    /// slope they never came off (bot-navigation.md section 3). Not while
    /// the follower rests on purpose: a body stuck before its first node
    /// is left to its own unstick.
    pub fn rest_point(&self, at: [f32; 3], body: Footing) -> Option<[f32; 3]> {
        let p = self.last?;
        let at = Vec3::from(at);
        (self.rest == 0
            && self.waited <= WAIT_TICKS
            && body.on_ground
            && !body.on_ladder
            && (p - at).truncate().length() < WAIT_REACH
            && (p.z - at.z).abs() < vcod_common::pmove::STEPSIZE)
            .then_some(p.into())
    }

    /// [`Self::waypoint`] before the bookkeeping.
    #[allow(clippy::too_many_arguments)]
    fn follow(
        &mut self,
        g: &NavGraph,
        world: Option<&CollisionWorld>,
        goal: crate::bots::Goal,
        at: [f32; 3],
        body: Footing,
        budget: &mut u32,
        rand: &mut dyn FnMut() -> i32,
    ) -> Option<[f32; 3]> {
        use crate::bots::Goal;
        self.clock += 1;
        self.stop_at = None;
        let clock = self.clock;
        self.avoid.retain(|e| e.2 > clock);
        if self.rest > 0 {
            self.rest -= 1;
            return None;
        }
        let here = Vec3::from(at);
        match goal {
            Goal::Hold => {
                self.reset();
                return None;
            }
            Goal::Roam => {}
            Goal::Away(t) => {
                let t = Vec3::from(t);
                if self.threat.is_none_or(|p| p.distance(t) > RETARGET) {
                    self.reset();
                    self.threat = Some(t);
                }
            }
            Goal::To(p) => {
                let moved = self
                    .dest
                    .is_none_or(|d| g.nodes[d as usize].distance(Vec3::from(p)) > RETARGET);
                if moved {
                    self.reset();
                    self.dest = g.nearest(p);
                } else if self.arrived {
                    let fell = here.z < self.arrived_at.z - ARRIVED_DROP;
                    if !fell {
                        return Some(p);
                    }
                    self.reset();
                    self.dest = g.nearest(p);
                }
            }
        }
        if self.path.is_empty() {
            self.on_rungs = body.on_ladder;
            self.path = self.plan(g, world, goal, here, budget, rand)?;
            self.next = 0;
            self.best = f32::INFINITY;
            self.idle = 0;
        }
        let flat = |p: Vec3| (p - here).truncate().length();
        // Past a waypoint once inside its reach, or once nearer the next one
        // than the waypoint itself is: that keeps a bot from doubling back
        // to touch a node it cut the corner on. Either only on the
        // waypoint's floor: a ladder's head is right above its foot.
        while let Some(&w) = self.path.get(self.next) {
            // The top of a jump is passed only standing on it: from its
            // foot the node is in reach flat and the jump not yet made.
            // So is a node a drop leaves from: from under it the drop is
            // not where the graph proved it.
            let jump = self.jumping(g);
            let rise = if jump {
                JUMP_PASS
            } else if self.dropping(g) {
                DROP_PASS
            } else {
                48.0
            };
            // A leap's foot is passed only at rest on it: the leap was
            // proved from a standstill there. A creep's top is held while
            // the body comes onto it faster than [`CREEP_TOP_PACE`].
            let leap = self
                .path
                .get(self.next + 1)
                .is_some_and(|&b| g.leaps.contains(&(w, b)));
            let wp = g.nodes[w as usize];
            // A node held for a body too fast for the walk on is passed only
            // from at rest on it, like a leap's foot.
            let held = self
                .path
                .get(self.next + 1)
                .is_some_and(|&b| self.held == Some((w, b)));
            let foot = leap
                || held
                || (body.on_ground
                    && !body.on_ladder
                    && body.velocity.truncate().length() > CREEP_TOP_PACE
                    && world.is_some_and(|world| self.creep_top(world, g)));
            let w = wp;
            // From under it, only from its floor: a ladder's head is passed
            // standing at its lip, not from the rungs a storey down.
            let below = rise.min(vcod_common::pmove::STEPSIZE);
            // A ladder's head only standing on it: the climb proved it facing
            // the rungs to the top, and a body that turns for the next node
            // on the rungs or in the air over the lip lets go of them
            // (mp_ship's mast, section 3). A jump's top too: in the air
            // beside the ledge, under its floor, the jump is not yet made
            // (mp_ship's ledge at (3347, 250, 616)).
            // On the rungs, a node below only from its floor: the back
            // down that proved it ran onto that floor, and a body that
            // turns for the next node from above it hangs on the rungs
            // under the floor's lip (mp_ship's hold ladder at z 304).
            let above = if body.on_ladder {
                vcod_common::pmove::STEPSIZE
            } else {
                rise
            };
            // And no node off the rungs in the air: a body that turns for
            // the next node in the air over a narrow shelf flies off its
            // side (mp_ship's shelf at (5384, -161, 396)), and one sliding
            // down terrain too steep to stand on turns off a patch it never
            // landed on and slides on into mp_hurtgen's crease (section 3).
            // On the rungs, not a node more than a step below.
            let high = here.z > w.z + vcod_common::pmove::STEPSIZE;
            // A plan begun on the rungs starts at a ladder's end
            // ([`NavGraph::nearest_ladder_end`]); one over the feet is the
            // head, passed only on the ground like any climb's: from the
            // rungs under mp_ship's crow's nest plate a bot turned for the
            // rim's exit and fell down the hatch.
            let head_first = self.next == 0 && w.z > here.z;
            let level = here.z > w.z - below
                && here.z < w.z + above
                && (body.on_ground
                    || (body.on_ladder && !(self.climbing(g) || jump || high || head_first)));
            // A ladder's foot is passed only on it, past cutting nothing:
            // the climb was proved from there, and a body beside the face
            // heading for the head runs past the rungs (mp_ship, section 3).
            let rung = self.rising(g);
            // Nor is a jump's top cut past: a body still in the air beside
            // it is nearer the next node too, and falls back to the foot.
            // Nor a drop's top: the drop was proved from there, and a body
            // cutting across for the bottom pins on whatever stands between
            // (mp_ship's ladder head at (3298, -380, 352), its foot east
            // behind the wall at y -333).
            // Nor a jump's foot: the jump was proved from there, and a body
            // that cut to its top from a ledge beside it jumped off that
            // ledge (mp_ship's at (3468, -210, 798), section 3).
            let past = !foot
                && !jump
                && !rung
                && self.path.get(self.next + 1).is_some_and(|&n| {
                    if g.jumps.contains(&(self.path[self.next], n)) {
                        return false;
                    }
                    let n = g.nodes[n as usize];
                    n.z >= w.z - vcod_common::pmove::JUMP_HEIGHT
                        && flat(n) < (n - w).truncate().length()
                });
            let reached = if foot {
                flat(w) < LEAP_FOOT && body.still
            } else if rung {
                // Square in front of the rungs, the run turns into the face
                // from anywhere near (`face_ahead`); off to the side, only
                // the foot itself proved the climb.
                let square = g.face_ahead(w).is_some();
                flat(w) < if square { REACH } else { ARRIVE }
            } else {
                flat(w) < REACH
            };
            if level && reached && leap && !self.leap_holds(world, here, g) {
                // The leap fails from where this body came to rest: round it.
                let edge = (self.path[self.next], self.path[self.next + 1]);
                self.avoid.push((edge.0, edge.1, self.clock + AVOID_TICKS));
                self.reset();
                return None;
            }
            // An edge planned while it was avoided had no way round it, and
            // is taken as it is: checked again, it would be avoided again
            // every tick and the bot stand without a waypoint (mp_ship's
            // corner at (5375, -193, 352), where a body from the east jams
            // 2 units past the node on the shelf's slope and its seam).
            let planned_round = self.path.get(self.next + 1).is_some_and(|&b| {
                let a = self.path[self.next];
                self.avoid.iter().any(|e| (e.0, e.1) == (a, b))
            });
            // A body that a light weapon's pace flings off the way on comes
            // to rest on the node first and heads on only from at rest
            // ([`Self::flings`]). Passed on the way to rest at the first
            // pace whose walk arrived, bots came onto mp_hurtgen's slope at
            // (338, 2830, -25) and slid into its crease.
            if level
                && (reached || past)
                && (!foot || held)
                && !planned_round
                && self.flings(world, here, body, g)
            {
                let edge = (self.path[self.next], self.path[self.next + 1]);
                if held {
                    // Flung from at rest too: round it.
                    self.avoid.push((edge.0, edge.1, self.clock + AVOID_TICKS));
                    self.reset();
                    return None;
                }
                self.held = Some(edge);
                break;
            }
            // So does one whose back down off a drop, at its pace, falls
            // short of the floor it was proved onto ([`Self::backs_short`]).
            if level
                && (reached || past)
                && (!foot || held)
                && !planned_round
                && self.backs_short(world, here, body, g)
            {
                let edge = (self.path[self.next], self.path[self.next + 1]);
                if held {
                    self.avoid.push((edge.0, edge.1, self.clock + AVOID_TICKS));
                    self.reset();
                    return None;
                }
                self.held = Some(edge);
                break;
            }
            if level
                && (reached || past)
                && (!foot || held)
                && !planned_round
                && !self.edge_holds(world, here, body.velocity, g)
            {
                // The walk on was proved from the node, and fails from here:
                // close in on the node first, and from on it, round the
                // edge.
                let edge = (self.path[self.next], self.path[self.next + 1]);
                if (flat(w) < ON_NODE || held) && here.z < w.z + ARRIVE {
                    self.avoid.push((edge.0, edge.1, self.clock + AVOID_TICKS));
                    self.reset();
                    return None;
                }
                break;
            }
            // Taken as it is, but from at rest on the node: passed at the
            // pace that had it avoided, bots ran off the crow's nest's top
            // at (4928, 61, 1254) on a turn, over the spar at 1212 and down
            // to the deck at 400 (bot-navigation.md section 3).
            if level
                && (reached || past)
                && planned_round
                && !held
                && !self.edge_holds(world, here, body.velocity, g)
            {
                self.held = Some((self.path[self.next], self.path[self.next + 1]));
                break;
            }
            if level && (reached || past) {
                self.held = None;
                self.next += 1;
                self.best = f32::INFINITY;
                self.idle = 0;
            } else {
                break;
            }
        }
        let Some(&w) = self.path.get(self.next) else {
            // Arrived. A roam picks somewhere new; a point is walked at.
            self.path.clear();
            return match goal {
                Goal::To(p) => {
                    self.arrived = true;
                    self.arrived_at = here;
                    Some(p)
                }
                _ => {
                    self.dest = None;
                    None
                }
            };
        };
        let w = g.nodes[w as usize];
        if flat(w) > OFF_PATH_STEPS * g.spacing {
            self.path.clear();
            return None;
        }
        // Fallen off the path: a waypoint more than a climb over the feet,
        // and no ladder leads to it. Plan again from where the body landed,
        // round the edge it fell off (mp_ship's lip at (5424, -187, 344),
        // under the shelf at 396).
        if body.on_ground && self.next > 0 && !self.climbing(g) && w.z - here.z > FELL_BELOW {
            let edge = (self.path[self.next - 1], self.path[self.next]);
            self.avoid.push((edge.0, edge.1, self.clock + AVOID_TICKS));
            self.reset();
            self.fell = true;
            return None;
        }
        // A plan's first node more than a step over the feet is a floor
        // the body left while the search ran: a bot that wandered off
        // mp_ship's crow's nest top at 1254 onto the hatch's rim at 1219
        // headed back for the top, pinned on its face and slid along it
        // down the hatch. Plan again from a node it walks to.
        if body.on_ground
            && !body.on_ladder
            && self.next == 0
            && !self.proved_start
            && w.z - here.z > vcod_common::pmove::STEPSIZE
        {
            self.reset();
            self.fell = true;
            return None;
        }
        // Progress counts height too: a climb closes on nothing flat.
        let d = flat(w) + (w.z - here.z).abs();
        if d < self.best - 1.0 {
            self.best = d;
            self.idle = 0;
        } else {
            self.idle += 1;
            if self.idle > STUCK_TICKS {
                // Stuck on the way from the last node: plan round that edge
                // next tick. Before the first node, leave the bot to its own
                // unstick for a spell.
                if let Some(from) = self.next.checked_sub(1).map(|i| self.path[i]) {
                    let to = self.path[self.next];
                    self.avoid.push((from, to, self.clock + AVOID_TICKS));
                } else {
                    self.rest = REST_TICKS;
                }
                self.reset();
                return None;
            }
        }
        // Up a ladder from its foot: into the face first, the way the climb
        // that found the head went, then at the head once level with it.
        if self.climbing(g)
            && here.z < w.z - vcod_common::pmove::STEPSIZE
            && let Some(p) = g.face_ahead(g.nodes[self.path[self.next - 1] as usize])
        {
            return Some([p.x, p.y, w.z]);
        }
        Some(w.into())
    }

    /// The path toward the destination, once the search for it is done.
    /// A search starts only while `budget` has some left and runs at most
    /// [`PLAN_SLICE`] of it a tick; a destination the start does not reach
    /// fails before any search, and a point goal then heads for the reached
    /// node nearest it instead.
    fn plan(
        &mut self,
        g: &NavGraph,
        world: Option<&CollisionWorld>,
        goal: crate::bots::Goal,
        here: Vec3,
        budget: &mut u32,
        rand: &mut dyn FnMut() -> i32,
    ) -> Option<Vec<u32>> {
        if *budget == 0 {
            return None;
        }
        if self.search.is_none() {
            self.proved_start = true;
            let start = match world {
                _ if self.on_rungs => g.nearest_ladder_end(here),
                Some(w) if std::mem::take(&mut self.fell) || g.off_floor(here) => {
                    g.nearest_walkable(w, here)
                }
                _ => {
                    self.proved_start = false;
                    g.nearest(here.into())
                }
            }?;
            if self.dest.is_none() {
                self.dest = match goal {
                    crate::bots::Goal::Roam => g.roam_from(start, here, rand),
                    crate::bots::Goal::Away(t) => g.flee_from(start, here, t.into(), rand),
                    _ => None,
                };
            }
            let to = match goal {
                crate::bots::Goal::To(_) => self.dest.and_then(|d| g.nearest_reached(start, d)),
                _ => self.dest.filter(|&d| g.reaches(start, d)),
            };
            let Some(to) = to else {
                // Unreachable from here: a roam picks again next time.
                self.reset();
                self.rest = REST_TICKS;
                return None;
            };
            // Round the edges it got stuck on when there is a way round,
            // else through them again: whatever blocked one may have moved.
            let avoid: Vec<(u32, u32)> = self.avoid.iter().map(|e| (e.0, e.1)).collect();
            self.round = !avoid.is_empty();
            self.search = Some(Search::new(g, start, to, avoid));
        }
        let search = self.search.as_mut()?;
        let mut slice = (*budget).min(PLAN_SLICE);
        let before = slice;
        let plan = search.run(g, &mut slice);
        // A plan that finds its path at once still costs one.
        *budget -= (before - slice).max(1).min(*budget);
        match plan {
            Plan::Pending => None,
            Plan::Found(path) => {
                self.search = None;
                Some(path)
            }
            Plan::Failed if self.round => {
                let s = self.search.take()?;
                self.round = false;
                self.search = Some(Search::new(g, s.from, s.to, Vec::new()));
                None
            }
            Plan::Failed => {
                self.reset();
                self.rest = REST_TICKS;
                None
            }
        }
    }

    /// Whether the leap from the current waypoint, a leap's foot, arrives run
    /// from `at`, where the bot came to rest: the graph proved it from the
    /// node, and a body a few units nearer the lip leaves the ground before
    /// it has the pace to clear the gap. Without a world, taken as proved.
    fn leap_holds(&self, world: Option<&CollisionWorld>, at: Vec3, g: &NavGraph) -> bool {
        let (Some(world), Some(&top)) = (world, self.path.get(self.next + 1)) else {
            return true;
        };
        let top = g.nodes[top as usize];
        matches!(
            walk_as(world, at, top.truncate(), Some(top.z), Gait::Leap),
            Walked::Arrived(..)
        )
    }

    /// Whether the walk from the current waypoint to the next arrives run
    /// from `at`: the graph proved it from the node, and over a hull beam's
    /// slope a body a few units aside slides back. A climb or a drop
    /// steeper than a jump is taken as proved (a ladder, a back down), and
    /// so is a walk from under the node's floor (a body on a ladder under
    /// its head) or without a world. From over it the walk has to arrive:
    /// mp_ship's lip at (5424, -187, 344) stands 24 over a stair node,
    /// within reach of it flat, and the walk on from there pins under a
    /// shelf. Off a jump edge the walk has to arrive
    /// without one: the bot jumps only on the graph's jump edges, and a
    /// body a few units aside of a hull beam's end pins on it where the
    /// walk from the node slides past (bot-navigation.md section 3).
    fn edge_holds(
        &mut self,
        world: Option<&CollisionWorld>,
        at: Vec3,
        pace: Vec3,
        g: &NavGraph,
    ) -> bool {
        let (Some(world), Some(&a), Some(&b)) = (
            world,
            self.path.get(self.next),
            self.path.get(self.next + 1),
        ) else {
            return true;
        };
        let jump = g.jumps.contains(&(a, b));
        let edge = (a, b);
        let (a, b) = (g.nodes[a as usize], g.nodes[b as usize]);
        if at.z < a.z - ARRIVE {
            return true;
        }
        // A run off a drop has to arrive at the pace the body comes onto
        // it: mp_ship's slot between the stair's top and the shelf at
        // (5403, -160, 396) lands a run straight at it from anywhere near
        // the node, and misses from a turn off the shelf. A drop the run
        // misses from the node itself was backed down, and is taken as
        // proved.
        if b.z < a.z - vcod_common::pmove::JUMP_HEIGHT {
            if !self.runs_off(world, edge, a, b) {
                return true;
            }
            return matches!(
                walk_with(
                    world,
                    at,
                    Some(pace),
                    b.truncate(),
                    Some(b.z),
                    Gait::Forward
                ),
                Walked::Arrived(_, false)
            );
        }
        if (b.z - a.z).abs() > vcod_common::pmove::JUMP_HEIGHT {
            return true;
        }
        match walk_with(
            world,
            at,
            Some(pace),
            b.truncate(),
            Some(b.z),
            Gait::Forward,
        ) {
            Walked::Arrived(_, jumped) => jump || !jumped,
            _ => false,
        }
    }

    /// Whether a body faster than the walk's run, at `pace` and its
    /// weapon's [`Self::move_scale`], falls off the way on from the current
    /// waypoint where [`Self::edge_holds`]' walk at the run's own scale
    /// holds: a run over mp_depot's roof peak at (-2660, -1216, 552) at 220
    /// units/s flies off the crest and over the roof's far edge, where one
    /// at 190 lands inside it (bot-navigation.md section 3). Only a fall:
    /// read for every miss at the weapon's scale, the verdict held and
    /// rerouted bots on mp_ship far more often and more of them died. At
    /// rest on a held node the same walk decides whether the edge is taken
    /// at all: a weapon at 1.16 is back at 220 by the crest from a
    /// standstill 17 units short of it.
    fn flings(
        &self,
        world: Option<&CollisionWorld>,
        at: Vec3,
        body: Footing,
        g: &NavGraph,
    ) -> bool {
        let (Some(world), Some(&a), Some(&b)) = (
            world,
            self.path.get(self.next),
            self.path.get(self.next + 1),
        ) else {
            return false;
        };
        // Only a walk down off a crest, more than a step and no more than
        // a jump: a body flies off one. A climb, a jump or a backed drop
        // is taken another way; a drop run off needs the running start a
        // stop takes away (held on mp_ship's (3648, -317, 618), bots ran
        // off its drop to the ledge at 400 from rest and died); and a miss
        // on the level or uphill held bots off mp_hurtgen's (292, 2785,
        // -52) onto a way round that slides into its crease.
        let (from, to) = (g.nodes[a as usize], g.nodes[b as usize]);
        let drop = from.z - to.z;
        let run = drop > vcod_common::pmove::STEPSIZE
            && drop <= vcod_common::pmove::JUMP_HEIGHT
            && !g.jumps.contains(&(a, b));
        let fast = body.velocity.truncate().length() > vcod_common::pmove::SPEED_RUN;
        run && body.on_ground
            && (fast || (body.still && self.move_scale > 1.0))
            && matches!(
                self.walk_paced(world, at, body.velocity, g.nodes[b as usize], Gait::Forward),
                Walked::Fell
            )
    }

    /// Whether the way on from the current waypoint is a drop deeper than a
    /// jump that a run off misses, so the bot backs down it, and that the
    /// bot's own back down ([`Gait::Creep`]: a run, creeping at the lip)
    /// from `at` at the body's pace misses: a body that comes onto it off
    /// a turn carries its pace over the lip (mp_ship's ledge at
    /// (3686, -354, 400) over the hold, and the hold ladder's top at
    /// (4319, -133, 600), bot-navigation.md section 3). At rest on a held
    /// node the walk decides whether the edge is taken at all.
    fn backs_short(
        &mut self,
        world: Option<&CollisionWorld>,
        at: Vec3,
        body: Footing,
        g: &NavGraph,
    ) -> bool {
        let (Some(world), Some(&a), Some(&b)) = (
            world,
            self.path.get(self.next),
            self.path.get(self.next + 1),
        ) else {
            return false;
        };
        let (from, to) = (g.nodes[a as usize], g.nodes[b as usize]);
        to.z < from.z - vcod_common::pmove::JUMP_HEIGHT
            && body.on_ground
            && !g.jumps.contains(&(a, b))
            && !self.runs_off(world, (a, b), from, to)
            && !matches!(
                self.walk_paced(world, at, body.velocity, to, Gait::Creep),
                Walked::Arrived(..)
            )
    }

    /// The walk from `at` onto `to`'s floor with the body moving at `pace`
    /// and at its weapon's [`Self::move_scale`]: how this bot comes onto
    /// the edge, where the graph's walks ran at 1 from the node.
    fn walk_paced(
        &self,
        world: &CollisionWorld,
        at: Vec3,
        pace: Vec3,
        to: Vec3,
        gait: Gait,
    ) -> Walked {
        let mut slid = Vec::new();
        walk_sliding(
            world,
            at,
            Some(pace),
            self.move_scale,
            to.truncate(),
            Some(to.z),
            gait,
            &mut slid,
        )
    }

    /// Where to wait instead of following the path: the bot is at one end
    /// of the ladder its path takes next, off the rungs, and another player
    /// on those rungs is about to step off there or hangs there. Two players
    /// meeting head-on at a ladder pin each other: the climber cannot step
    /// off where the other stands (mp_ship's ladder at (3354, -380),
    /// bot-navigation.md section 3). The bot steps to the neighbour of its
    /// end farthest from the ladder until the rungs clear, at most
    /// [`YIELD_TICKS`] ([`YIELD_HUNG`] of them with the climber still).
    /// `climbers` are the other players on a ladder, as (origin, velocity).
    pub fn ladder_yield(
        &mut self,
        g: &NavGraph,
        at: Vec3,
        body: Footing,
        climbers: &[(Vec3, Vec3)],
    ) -> Option<Vec3> {
        self.yield_rest = self.yield_rest.saturating_sub(1);
        let spot = self.yield_spot(g, at, body, climbers);
        if let Some((p, moving)) = spot
            && self.yield_ticks < YIELD_TICKS
            && self.yield_hung < YIELD_HUNG
        {
            self.yield_ticks += 1;
            self.yield_hung += u32::from(!moving);
            return Some(p);
        }
        if self.yield_ticks > 0 {
            if spot.is_some() {
                self.yield_rest = YIELD_REST;
            }
            self.yield_ticks = 0;
            self.yield_hung = 0;
            // The way back to the waypoint is progress afresh.
            self.best = f32::INFINITY;
            self.idle = 0;
        }
        None
    }

    /// Whether the body, climbing toward a ladder's head on the rungs, lets
    /// another climber beside it go first: one within two half-widths flat
    /// and a body's height in z that is higher, or level and first in
    /// (y, x) order, so of two abreast exactly one gives way; only to one
    /// still climbing, or a climber that hangs would hold it off for good.
    /// mp_ship's ladder at x 3792 is 25 wide and the hatch over its top
    /// lets a body through only between y 58 and 72; two bots that climbed
    /// it abreast at y 46 and 76, 30 apart, each pressed at the lane the
    /// other's capsule stood in and neither moved again (bot-navigation.md
    /// section 3). The one that gives way climbs down until the other is
    /// off the rungs. `climbers` as in [`Self::ladder_yield`].
    pub fn abreast_yield(
        &self,
        g: &NavGraph,
        at: Vec3,
        body: Footing,
        climbers: &[(Vec3, Vec3)],
    ) -> bool {
        use vcod_common::pmove::HALF_WIDTH;
        if !body.on_ladder || !self.climbing(g) {
            return false;
        }
        climbers.iter().any(|&(c, v)| {
            let dz = c.z - at.z;
            v.z > 1.0
                && (c - at).truncate().length() < 2.0 * HALF_WIDTH + 2.0
                && dz.abs() < ABREAST_HEIGHT
                && (dz > 1.0 || (dz.abs() <= 1.0 && (c.y, c.x) < (at.y, at.x)))
        })
    }

    /// On the rungs backing down a ladder with another climber coming up
    /// under the body: the head to climb back to. A body backing down
    /// onto a climber's head stands on it and slides off; down mp_ship's
    /// crow's nest ladder that is a fall to the deck at 400 (bot-navigation.md
    /// section 3). At the head the bot waits aside ([`Self::ladder_yield`]).
    /// `climbers` as in [`Self::ladder_yield`].
    pub fn head_on_yield(
        &self,
        g: &NavGraph,
        at: Vec3,
        body: Footing,
        climbers: &[(Vec3, Vec3)],
    ) -> Option<Vec3> {
        use vcod_common::pmove::{HALF_WIDTH, JUMP_HEIGHT};
        if !body.on_ladder {
            return None;
        }
        let i = self.next.checked_sub(1)?;
        let (head, foot) = (
            g.nodes[*self.path.get(i)? as usize],
            g.nodes[*self.path.get(self.next)? as usize],
        );
        if foot.z > head.z - JUMP_HEIGHT {
            return None;
        }
        climbers
            .iter()
            .any(|&(c, v)| {
                v.z > 1.0
                    && c.z < at.z
                    && at.z - c.z < HEAD_ON
                    && (c - at).truncate().length() < 2.0 * HALF_WIDTH + 2.0
            })
            .then_some(head)
    }

    fn yield_spot(
        &self,
        g: &NavGraph,
        at: Vec3,
        body: Footing,
        climbers: &[(Vec3, Vec3)],
    ) -> Option<(Vec3, bool)> {
        use vcod_common::pmove::{JUMP_HEIGHT, STEPSIZE};
        if body.on_ladder || self.yield_rest > 0 || climbers.is_empty() {
            return None;
        }
        // The edge under way, else the one on from the waypoint.
        let i = self.next;
        let (e, f) = [i.checked_sub(1).map(|h| (h, i)), Some((i, i + 1))]
            .into_iter()
            .flatten()
            .filter_map(|(a, b)| Some((*self.path.get(a)?, *self.path.get(b)?)))
            .find(|&(a, b)| (g.nodes[b as usize].z - g.nodes[a as usize].z).abs() > JUMP_HEIGHT)?;
        let (end, far) = (g.nodes[e as usize], g.nodes[f as usize]);
        let rungs = *g
            .ladder_boxes
            .iter()
            .find(|b| box_flat(b, end) < LADDER_NEAR && box_flat(b, far) < LADDER_NEAR)?;
        if (at.z - end.z).abs() > STEPSIZE || box_flat(&rungs, at) > YIELD_NEAR {
            return None;
        }
        let coming: Vec<Vec3> = climbers
            .iter()
            .filter(|&&(c, v)| {
                box_flat(&rungs, c) < CLIMBER_NEAR
                    && c.z > rungs.0.z - STEPSIZE
                    && c.z < rungs.1.z + STEPSIZE
                    && (end.z - c.z).abs() < CLIMBER_CLOSE
                    && (end.z - c.z) * v.z >= 0.0
            })
            .map(|&(_, v)| v)
            .collect();
        if coming.is_empty() {
            return None;
        }
        let moving = coming.iter().any(|v| v.z.abs() > 1.0);
        let spot = g.edges[e as usize]
            .iter()
            .map(|&m| g.nodes[m as usize])
            .filter(|q| (q.z - end.z).abs() < STEPSIZE && box_flat(&rungs, *q) >= YIELD_CLEAR)
            .max_by(|a, b| box_flat(&rungs, *a).total_cmp(&box_flat(&rungs, *b)))?;
        Some((spot, moving))
    }

    /// The bot's own stall circle caught the body on the way to the
    /// current waypoint and sent it on an unstick spell: plan round the
    /// edge under way. The spell runs the body off the path, and the plan
    /// from where it ends took the same edge back into the same trap
    /// (mp_ship's hold crates at (2234, 494, -42), bot-navigation.md
    /// section 3).
    pub fn stuck(&mut self) {
        if let Some(from) = self.next.checked_sub(1).and_then(|i| self.path.get(i))
            && let Some(&to) = self.path.get(self.next)
        {
            self.avoid.push((*from, to, self.clock + AVOID_TICKS));
            self.reset();
        }
    }

    /// Test-facing: the current waypoint's node and the one after it.
    pub fn current(&self) -> (Option<u32>, Option<u32>) {
        (
            self.path.get(self.next).copied(),
            self.path.get(self.next + 1).copied(),
        )
    }

    /// Whether the current waypoint is a leap's foot, or the node of an edge
    /// the body comes onto too fast ([`Self::stop_at`], [`Self::held`]): the bot comes to
    /// rest on it before it heads on.
    pub fn holding(&self, g: &NavGraph) -> bool {
        match (self.path.get(self.next), self.path.get(self.next + 1)) {
            (Some(&a), Some(&b)) => {
                g.leaps.contains(&(a, b))
                    || self.stop_at == Some((a, b))
                    || self.held == Some((a, b))
            }
            _ => false,
        }
    }

    /// Whether the current waypoint is the top of a creep that misses off a
    /// turn ([`Self::creeps_off`]), which the bot slows onto before it backs
    /// off: the graph's walk crept from the node with a run straight at the
    /// drop behind it, and a bot that came onto mp_depot's roof drop off a
    /// turn left the lip at 81 units/s to the walk's 14 and fell past the
    /// ledge. Re-proved at the body's pace each tick instead, the creep
    /// read as arriving by a floor 3 under the ledge that the bot's own
    /// fall missed; held at every creep's top, bots slowed onto mp_ship's
    /// lip node (5402, 321, 344) from 8 over it and bounced off the stair
    /// below (bot-navigation.md section 3).
    fn creep_top(&mut self, world: &CollisionWorld, g: &NavGraph) -> bool {
        if !self.creeps_off(world, g) {
            return false;
        }
        self.stop_at = Some((self.path[self.next], self.path[self.next + 1]));
        true
    }

    /// Whether the way on from the current waypoint is a drop more than a
    /// jump deep that neither a run nor a back down from the node arrives
    /// at, so the graph's walk crept it ([`Gait::Creep`]), that misses
    /// coming onto it off a turn ([`creep_takes_a_turn`]) and arrives from
    /// rest: where a stop helps. Held at every creep that misses off a
    /// turn, mp_ship's bots slowed onto its lip node (5402, 321, 344) and
    /// bounced off the stair below, and stalled about twice as long over
    /// the sweep (bot-navigation.md section 3). A body
    /// that comes onto it off a turn carries its pace over the lip and past
    /// the grab: mp_depot's thin ledges at (-2749, -1281, 326) and
    /// (-2755, -1150, 326), 182 under the roof (bot-navigation.md
    /// section 3). So is a creep that arrives only from rest: the back down
    /// off mp_ship's mast head (3675, -95.1, 1147.1) on the spar beside the
    /// rungs' top. Walked once per edge.
    fn creeps_off(&mut self, world: &CollisionWorld, g: &NavGraph) -> bool {
        let (Some(&a), Some(&b)) = (self.path.get(self.next), self.path.get(self.next + 1)) else {
            return false;
        };
        if let Some((e, creep)) = self.creep_off
            && e == (a, b)
        {
            return creep;
        }
        let (from, to) = (g.nodes[a as usize], g.nodes[b as usize]);
        let arrives = |pace, gait| {
            matches!(
                walk_with(world, from, pace, to.truncate(), Some(to.z), gait),
                Walked::Arrived(..)
            )
        };
        // Or only from rest: a ladder's back down `rung` proved that way.
        let creep = to.z < from.z - vcod_common::pmove::JUMP_HEIGHT
            && !self.runs_off(world, (a, b), from, to)
            && arrives(Some(Vec3::ZERO), Gait::Creep)
            && ((!arrives(None, Gait::Backward)
                && !creep_takes_a_turn(world, from, to.truncate(), Some(to.z)))
                || !arrives(None, Gait::Creep));
        self.creep_off = Some(((a, b), creep));
        creep
    }

    /// Whether the way to the current waypoint is a leap.
    pub fn leaping(&self, g: &NavGraph) -> bool {
        let Some(i) = self.next.checked_sub(1) else {
            return false;
        };
        match (self.path.get(i), self.path.get(self.next)) {
            (Some(&a), Some(&b)) => g.leaps.contains(&(a, b)),
            _ => false,
        }
    }

    /// Up a ladder from a foot square in front of the rungs, the foot: the
    /// climb that proved the head went up level with it along the face,
    /// and a body that grabbed the rungs off that line strafes back to it
    /// (`bots::Bot::steer`).
    pub fn climb_line(&self, g: &NavGraph) -> Option<glam::Vec2> {
        if !self.climbing(g) {
            return None;
        }
        let foot = g.nodes[self.path[self.next - 1] as usize];
        g.face_ahead(foot).map(|_| foot.truncate())
    }

    /// Whether the edge on from the current waypoint climbs more than a
    /// jump: the waypoint is a ladder's foot.
    fn rising(&self, g: &NavGraph) -> bool {
        match (self.path.get(self.next), self.path.get(self.next + 1)) {
            (Some(&a), Some(&b)) => {
                g.nodes[b as usize].z > g.nodes[a as usize].z + vcod_common::pmove::JUMP_HEIGHT
            }
            _ => false,
        }
    }

    /// Whether the edge on from the current waypoint drops more than a step.
    fn dropping(&self, g: &NavGraph) -> bool {
        match (self.path.get(self.next), self.path.get(self.next + 1)) {
            (Some(&a), Some(&b)) => {
                g.nodes[b as usize].z < g.nodes[a as usize].z - vcod_common::pmove::STEPSIZE
            }
            _ => false,
        }
    }

    /// Whether the way to the current waypoint climbs more than a jump: a
    /// ladder's foot to its head.
    fn climbing(&self, g: &NavGraph) -> bool {
        let Some(i) = self.next.checked_sub(1) else {
            return false;
        };
        match (self.path.get(i), self.path.get(self.next)) {
            (Some(&a), Some(&b)) => {
                g.nodes[b as usize].z > g.nodes[a as usize].z + vcod_common::pmove::JUMP_HEIGHT
            }
            _ => false,
        }
    }

    /// Whether the way to the current waypoint, a drop of more than
    /// `deep`, is taken at a run rather than backed onto: whether the
    /// graph's walk proved it running off ([`Self::runs_off`]). The pass
    /// onto it re-proved the run at the body's pace (`edge_holds`).
    pub fn running_off(
        &mut self,
        g: &NavGraph,
        world: Option<&CollisionWorld>,
        at: Vec3,
        deep: f32,
    ) -> bool {
        let (Some(world), Some(i)) = (world, self.next.checked_sub(1)) else {
            return false;
        };
        let (Some(&a), Some(&b)) = (self.path.get(i), self.path.get(self.next)) else {
            return false;
        };
        let (from, to) = (g.nodes[a as usize], g.nodes[b as usize]);
        to.z < at.z - deep && self.runs_off(world, (a, b), from, to)
    }

    /// Whether a forward walk from `from` arrives on `to`'s floor without a
    /// jump: the first gait the graph's walk tries (`walk_to`), so a drop
    /// it misses was backed down. Walked once per edge.
    fn runs_off(&mut self, world: &CollisionWorld, edge: (u32, u32), from: Vec3, to: Vec3) -> bool {
        if let Some((e, run)) = self.run_off
            && e == edge
        {
            return run;
        }
        let run = matches!(
            walk_as(world, from, to.truncate(), Some(to.z), Gait::Forward),
            Walked::Arrived(_, false)
        );
        self.run_off = Some((edge, run));
        run
    }

    /// Whether the way to the current waypoint is a jump edge.
    pub fn jumping(&self, g: &NavGraph) -> bool {
        let Some(i) = self.next.checked_sub(1) else {
            return false;
        };
        match (self.path.get(i), self.path.get(self.next)) {
            (Some(&a), Some(&b)) => g.jumps.contains(&(a, b)),
            _ => false,
        }
    }

    /// On a jump edge with the body's jump still cooling down
    /// (`PM_CheckJump` refuses one inside `JUMP_COOLDOWN_MS` of the last),
    /// the edge's foot to come to rest on until it is ready: the graph's
    /// walk proved the edge with a jump to hand. A bot that jumped onto
    /// mp_depot's crate at (-2275, 131, 46) ran on for the rim at 66 with
    /// the jump refused and off the crate's edge into the open crate below
    /// (bot-navigation.md section 3).
    pub fn jump_wait(&self, g: &NavGraph, at: [f32; 3], ps: &PlayerState) -> Option<[f32; 3]> {
        if !self.jumping(g)
            || !ps.on_ground
            || ps.on_ladder
            || ps.since_jump_ms >= vcod_common::pmove::JUMP_COOLDOWN_MS
        {
            return None;
        }
        let foot = g.nodes[self.path[self.next - 1] as usize];
        ((foot.z - at[2]).abs() < vcod_common::pmove::STEPSIZE).then_some(foot.into())
    }

    fn reset(&mut self) {
        self.held = None;
        self.dest = None;
        self.search = None;
        self.path.clear();
        self.arrived = false;
    }
}

impl NavGraph {
    /// For a body at `p` on a ladder, the point level with it across the
    /// face and at the middle of the ladder along it: a climb strafes
    /// toward it, since what overhangs a ladder (a spar, a hatch's rim)
    /// mostly pins a body at its edge (section 3).
    pub fn ladder_middle(&self, p: Vec3) -> Option<glam::Vec2> {
        self.ladder_boxes
            .iter()
            .filter(|(lo, hi)| p.z > lo.z - 80.0 && p.z < hi.z + 8.0)
            .filter_map(|&(lo, hi)| {
                let ext = hi - lo;
                let (a, b) = if ext.x < ext.y { (0, 1) } else { (1, 0) };
                let away = (lo[a] - p[a]).max(p[a] - hi[a]).max(0.0);
                let aside = (lo[b] - p[b]).max(p[b] - hi[b]).max(0.0);
                let mut m = p.truncate();
                m[b] = (lo[b] + hi[b]) * 0.5;
                (away < 32.0 && aside < 16.0).then_some((away + aside, m))
            })
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .map(|x| x.1)
    }

    /// The point a climb from the ladder foot `foot` runs at: straight into
    /// the nearest ladder face it stands square in front of, as the climb
    /// in [`rung`] did from there. The heading at the head instead drifts
    /// along the face from a foot a few units aside and grabs the rungs at
    /// their edge (section 3). A foot off to the side (a flood walk that
    /// met the ladder) keeps the heading its walk proved.
    fn face_ahead(&self, foot: Vec3) -> Option<glam::Vec2> {
        self.ladder_boxes
            .iter()
            .filter(|(lo, hi)| foot.z > lo.z - 48.0 && foot.z < hi.z)
            .filter_map(|&(lo, hi)| {
                // The face's axis is the box's thinner one; `a` across the
                // face, `b` along it.
                let ext = hi - lo;
                let (a, b) = if ext.x < ext.y { (0, 1) } else { (1, 0) };
                let plane = if foot[a] > hi[a] {
                    hi[a]
                } else if foot[a] < lo[a] {
                    lo[a]
                } else {
                    return None;
                };
                let away = (foot[a] - plane).abs();
                let aside = (lo[b] - foot[b]).max(foot[b] - hi[b]);
                let mut p = foot.truncate();
                p[a] = plane;
                (away < 48.0 && aside <= 0.0).then_some((away, p))
            })
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .map(|x| x.1)
    }

    /// The nodes a roam or a flight from node `start` picks among: the
    /// largest component the start reaches, its own included. A pick the
    /// start cannot reach would cost a search of everything it can; one in
    /// its own component alone keeps a bot spawned on an island with only
    /// drops off it (`mp_depot`'s platform at z 27) pacing it for good, and
    /// one in any reached component lets a bot roam into a pit it never
    /// leaves.
    fn roam_pool(&self, start: u32) -> Option<&[u32]> {
        let own = *self.comp.get(start as usize)? as usize;
        let bits = &self.reach[own];
        let best = (0..self.members.len())
            .filter(|&c| bits[c / 64] & (1 << (c % 64)) != 0)
            .max_by_key(|&c| (self.members[c].len(), c == own))?;
        Some(&self.members[best])
    }

    /// A roam destination: of a handful of random nodes in
    /// [`Self::roam_pool`] of node `start`, which stands at `from`, the
    /// first at least [`ROAM_MIN`] away, else the farthest.
    fn roam_from(&self, start: u32, from: Vec3, rand: &mut dyn FnMut() -> i32) -> Option<u32> {
        let pool = self.roam_pool(start)?;
        let mut best: Option<(f32, u32)> = None;
        for _ in 0..8 {
            let n = pool[rand() as usize % pool.len()];
            let d = self.nodes[n as usize].distance(from);
            if d >= ROAM_MIN {
                return Some(n);
            }
            if best.is_none_or(|(b, _)| d > b) {
                best = Some((d, n));
            }
        }
        best.map(|(_, n)| n)
    }
}

/// Where a [`Search`] stands after a call to [`Search::run`].
#[derive(Debug, PartialEq)]
pub enum Plan {
    /// The node list from `from` to `to` inclusive.
    Found(Vec<u32>),
    /// No way, or past [`MAX_EXPANSIONS`].
    Failed,
    /// The budget ran out first; call again.
    Pending,
}

/// An A* under way, over the directed edges with Euclidean cost and
/// heuristic. [`Search::run`] expands at most its budget per call, so a long
/// plan spreads over ticks rather than stalling one.
pub struct Search {
    from: u32,
    to: u32,
    avoid: Vec<(u32, u32)>,
    cost: Vec<f32>,
    came: Vec<u32>,
    open: BinaryHeap<Open>,
    expanded: usize,
}

impl Search {
    pub fn new(g: &NavGraph, from: u32, to: u32, avoid: Vec<(u32, u32)>) -> Search {
        let n = g.nodes.len();
        let mut s = Search {
            from,
            to,
            avoid,
            cost: vec![f32::INFINITY; n],
            came: vec![u32::MAX; n],
            open: BinaryHeap::new(),
            expanded: 0,
        };
        if (from as usize) < n && (to as usize) < n {
            s.cost[from as usize] = 0.0;
            s.open.push(Open {
                f: g.nodes[from as usize].distance(g.nodes[to as usize]),
                node: from,
            });
        }
        s
    }

    /// Expands nodes until the path is found or ruled out, each one taken
    /// from `budget`; [`Plan::Pending`] once `budget` is 0.
    pub fn run(&mut self, g: &NavGraph, budget: &mut u32) -> Plan {
        let goal = g.nodes.get(self.to as usize).copied().unwrap_or(Vec3::ZERO);
        let h = |i: u32| g.nodes[i as usize].distance(goal);
        while let Some(&Open { f, node }) = self.open.peek() {
            if node == self.to {
                let mut out = vec![node];
                let mut at = node;
                while at != self.from {
                    at = self.came[at as usize];
                    out.push(at);
                }
                out.reverse();
                return Plan::Found(out);
            }
            // A stale heap entry: the node was reached cheaper since.
            if f > self.cost[node as usize] + h(node) + 1e-3 {
                self.open.pop();
                continue;
            }
            if *budget == 0 {
                return Plan::Pending;
            }
            self.open.pop();
            *budget -= 1;
            self.expanded += 1;
            if self.expanded > MAX_EXPANSIONS {
                return Plan::Failed;
            }
            let here = g.nodes[node as usize];
            for &next in &g.edges[node as usize] {
                if self.avoid.contains(&(node, next)) {
                    continue;
                }
                let risk = if g.risky.contains(&(node, next)) {
                    RISK_COST
                } else {
                    0.0
                };
                let c = self.cost[node as usize] + here.distance(g.nodes[next as usize]) + risk;
                if c < self.cost[next as usize] {
                    self.cost[next as usize] = c;
                    self.came[next as usize] = node;
                    self.open.push(Open {
                        f: c + h(next),
                        node: next,
                    });
                }
            }
        }
        Plan::Failed
    }
}

impl NavGraph {
    /// A destination away from `threat`: of a handful of random nodes in
    /// [`Self::roam_pool`] of node `start`, which stands at `from`, the one
    /// farthest from the threat among those nearer the bot than the threat
    /// (a way there that runs past the threat is no escape), else the
    /// farthest of all.
    fn flee_from(
        &self,
        start: u32,
        from: Vec3,
        threat: Vec3,
        rand: &mut dyn FnMut() -> i32,
    ) -> Option<u32> {
        let pool = self.roam_pool(start)?;
        let mut best: Option<(bool, f32, u32)> = None;
        for _ in 0..12 {
            let n = pool[rand() as usize % pool.len()];
            let p = self.nodes[n as usize];
            let key = (p.distance(from) < p.distance(threat), p.distance(threat));
            if best.is_none_or(|(s, d, _)| key > (s, d)) {
                best = Some((key.0, key.1, n));
            }
        }
        best.map(|(_, _, n)| n)
    }
}

/// A* open-list entry; a min-heap on `f`, ties to the lower node index.
#[derive(PartialEq)]
struct Open {
    f: f32,
    node: u32,
}

impl Eq for Open {}

impl Ord for Open {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .f
            .total_cmp(&self.f)
            .then_with(|| other.node.cmp(&self.node))
    }
}

impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

thread_local! {
    /// Set on a [`NavJob`]'s thread: its [`par_map`]s leave one core free.
    static SPARE_CORE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// `f` over `items` on every core (all but one under [`SPARE_CORE`]),
/// results in `items` order.
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let threads = if SPARE_CORE.get() {
        cores.saturating_sub(1).max(1)
    } else {
        cores
    };
    if threads == 1 || items.len() < 16 {
        return items.iter().map(f).collect();
    }
    // Handed out one at a time: a walk into open ground is a few ticks and
    // one along a wall runs out its budget, so even chunks would leave most
    // cores idle behind the slowest.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut out: Vec<Option<R>> = (0..items.len()).map(|_| None).collect();
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads.min(items.len()))
            .map(|_| {
                s.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(item) = items.get(i) else {
                            return done;
                        };
                        done.push((i, f(item)));
                    }
                })
            })
            .collect();
        for h in handles {
            for (i, r) in h.join().expect("a nav walk panicked") {
                out[i] = Some(r);
            }
        }
    });
    out.into_iter()
        .map(|r| r.expect("every item mapped"))
        .collect()
}

/// The world's ladders as axial boxes: the `SURF_LADDER` brushes of the
/// world model, with touching ones merged, since a tall ladder is often
/// several brushes stacked.
fn ladders(world: &CollisionWorld) -> Vec<(Vec3, Vec3)> {
    let mut boxes: Vec<(Vec3, Vec3)> = world
        .brushes
        .iter()
        .filter(|b| b.model == 0 && b.surface_flags & vcod_common::collision::SURF_LADDER != 0)
        .filter_map(|b| {
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for &(n, d) in &b.planes {
                for i in 0..3 {
                    if n[i] == 1.0 {
                        hi[i] = d;
                    } else if n[i] == -1.0 {
                        lo[i] = -d;
                    }
                }
            }
            lo.cmple(hi).all().then_some((lo, hi))
        })
        .collect();
    let mut merged = true;
    while merged {
        merged = false;
        'outer: for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                let (a, b) = (boxes[i], boxes[j]);
                if (a.0 - 2.0).cmple(b.1).all() && (b.0 - 2.0).cmple(a.1).all() {
                    boxes[i] = (a.0.min(b.0), a.1.max(b.1));
                    boxes.swap_remove(j);
                    merged = true;
                    break 'outer;
                }
            }
        }
    }
    boxes
}

/// A ladder face's foot and head, `(foot, head, up, down)`: the foot is a
/// body settled in front of the face with normal `n`, the head where a climb
/// from there facing the face comes to rest a few steps past the top. `up`
/// and `down` say whether the bots' steering between the two arrives: a run
/// at the head looking up, a back down onto the face (`bots::steer`).
fn rung(world: &CollisionWorld, lo: Vec3, hi: Vec3, n: Vec3) -> Option<(Vec3, Vec3, bool, bool)> {
    let mid = (lo + hi) * 0.5;
    let half = (hi - lo).dot(n.abs()) * 0.5;
    let front = mid.truncate() + n.truncate() * (half + 16.0);
    let side = glam::Vec2::new(-n.y, n.x);
    let probe = PlayerState::spawn(lo, 0.0);
    // A ladder brush often runs on below the floor it stands on, and a wall
    // can crowd one edge of it, so the drop starts at the first height in
    // front of it a body fits, the middle first, then up to 16 units aside.
    let start = (1..=FOOT_TRIES)
        .map(|i| lo.z + 16.0 * i as f32)
        .take_while(|&z| z < hi.z)
        .flat_map(|z| [0.0, -8.0, 8.0, -16.0, 16.0].map(|s| (front + side * s).extend(z)))
        .find(|&p| !world.box_trace(p, p, probe.mins(), probe.maxs()).startsolid)?;
    // Dropped facing away, or an airborne grab would hang it on the face.
    let foot = settle(world, start, (n.y).atan2(n.x).to_degrees())?;
    if foot.z < lo.z - 48.0 || under_ground(world, foot) {
        return None;
    }
    let yaw = (-n.y).atan2(-n.x).to_degrees();
    let mut ps = PlayerState::spawn(foot, yaw);
    let mut sim = Sim::default();
    let mut climbed = false;
    let mut top: Option<Vec3> = None;
    for _ in 0..LADDER_TICKS {
        let pitch = if ps.on_ladder { -LADDER_PITCH } else { 0.0 };
        // A few steps in past the lip, then the keys come up and it stands.
        let forward = match top {
            Some(t) if (ps.origin - t).truncate().length() >= LADDER_STEP_IN => 0,
            _ => 127,
        };
        let cmd = UserCmd {
            forward,
            angles: [(pitch * ANGLE2SHORT) as i32, (yaw * ANGLE2SHORT) as i32, 0],
            ..NULL_USERCMD
        };
        sim.tick(world, &mut ps, &cmd);
        climbed |= ps.on_ladder;
        if top.is_none() && climbed && ps.on_ground && ps.origin.z > foot.z + 48.0 {
            top = Some(ps.origin);
        }
        // Stopped past the top, or pinned at the lip against whatever
        // stands beyond it.
        if top.is_some() && ps.on_ground && ps.velocity.length() < 1.0 {
            break;
        }
    }
    top?;
    if !ps.on_ground || under_ground(world, ps.origin) {
        return None;
    }
    let head = ps.origin;
    let arrives = |from: Vec3, to: Vec3, gait| {
        matches!(
            walk_as(world, from, to.truncate(), Some(to.z), gait),
            Walked::Arrived(_, false)
        )
    };
    let up = arrives(foot, head, Gait::Forward);
    // The creep from rest too: off a head on a spar beside the rungs' top
    // the running start carries the body out of their reach before it has
    // dropped the 8 under the top they hold from (mp_ship's mast at
    // (3675, -86.5), section 3).
    let down = arrives(head, foot, Gait::Creep)
        || matches!(
            walk_with(
                world,
                head,
                Some(Vec3::ZERO),
                foot.truncate(),
                Some(foot.z),
                Gait::Creep
            ),
            Walked::Arrived(_, false)
        );
    (up || down).then_some((foot, head, up, down))
}

/// Whether a body standing at `p` stands on a ladder brush's top with no
/// other ground under its middle: mp_ship's ladder plates come up through
/// hatches, and a body pinned at the top stands on the plate's 1-unit edge
/// over the hole.
fn perched(world: &CollisionWorld, p: Vec3) -> bool {
    let ps = PlayerState::spawn(p, 0.0);
    let t = world.box_trace(p + Vec3::Z, p - Vec3::Z, ps.mins(), ps.maxs());
    t.fraction < 1.0
        && t.surface_flags & vcod_common::collision::SURF_LADDER != 0
        && !footed(world, p)
}

/// Whether a body standing at `p` has ground right under its middle, not
/// a ladder's: the floor beside a hatch, not the hole's rim.
fn footed(world: &CollisionWorld, p: Vec3) -> bool {
    let down = p - Vec3::Z * vcod_common::pmove::STEPSIZE;
    let t = world.point_trace(p + Vec3::Z, down, MASK_PLAYERSOLID, false);
    t.fraction < 1.0 && t.surface_flags & vcod_common::collision::SURF_LADDER == 0
}

/// Where a [`perched`] ladder head walks off along the face, either way,
/// onto its own floor with ground under its middle ([`footed`]): `(exit, back)`,
/// `back` when the walk from the exit to the head arrives too. Empty for a
/// head that is not perched or has no such floor beside it.
fn ladder_exits(world: &CollisionWorld, head: Vec3, n: Vec3) -> Vec<(Vec3, bool)> {
    if !perched(world, head) {
        return Vec::new();
    }
    let side = glam::Vec2::new(-n.y, n.x);
    [side, -side]
        .into_iter()
        .filter_map(|dir| {
            EXIT_STEPS.iter().find_map(|&d| {
                let to = head.truncate() + dir * d;
                match walk_as(world, head, to, Some(head.z), Gait::Forward) {
                    Walked::Arrived(p, false) if footed(world, p) => Some(p),
                    _ => None,
                }
            })
        })
        .filter(|&p| !under_ground(world, p))
        .map(|p| {
            let back = walk_as(world, p, head.truncate(), Some(head.z), Gait::Forward);
            (p, matches!(back, Walked::Arrived(_, false)))
        })
        .collect()
}

/// How far along the face a perched head's exit is looked for.
const EXIT_STEPS: [f32; 3] = [24.0, 32.0, 48.0];

/// Heights, 16 units apart from the ladder's bottom, a foot's drop is
/// tried from.
const FOOT_TRIES: usize = 6;
/// How far a climb walks on past the top of a ladder before it stops.
const LADDER_STEP_IN: f32 = 24.0;

/// Whether a standing body with its feet at `p` touches one of `hazards`.
pub(crate) fn hazard(hazards: &[BrushHull], p: Vec3) -> bool {
    use vcod_common::pmove::{HALF_WIDTH, Stance};
    let half = Vec3::new(HALF_WIDTH, HALF_WIDTH, Stance::Stand.height() * 0.5);
    !hazards.is_empty() && box_contacts_hulls(p + Vec3::Z * half.z, half, Vec3::ZERO, hazards)
}

/// A wall whose normal meets a heading under this cosine (against it, more
/// than 45 degrees off square) is slid along, not stopped at.
const GRAZE: f32 = std::f32::consts::FRAC_1_SQRT_2;
/// How far apart along a heading [`drop_ahead`] feels for the floor.
const DROP_SAMPLE: f32 = 16.0;

/// How deep the first drop deeper than a jump (one a body cannot come
/// back up) is that a body standing at `p`, walking `look` units along the
/// flat unit vector `dir`, meets before a wall stops it; infinite past
/// the default `bg_fallDamageMinHeight`, where a fall starts to hurt.
/// Floors are felt every [`DROP_SAMPLE`], each against the one before, so
/// a stair down is no drop unless it runs more than a jump down within
/// the look. Each is felt with the standing capsule: one
/// standing on a lip it overhangs is held by it.
/// A floor too steep to stand on whose spot drains into a crease
/// ([`Creases`]) reads as a drop that hurts: a body sent down it never
/// comes out (mp_hurtgen's crease, bot-navigation.md section 3).
pub(crate) fn drop_ahead(
    world: &CollisionWorld,
    p: Vec3,
    dir: Vec3,
    look: f32,
    creases: &mut Creases,
) -> Option<f32> {
    use vcod_common::pmove::{HALF_WIDTH, JUMP_HEIGHT, MIN_WALK_NORMAL, STEPSIZE, Stance};
    let mins = Vec3::new(-HALF_WIDTH, -HALF_WIDTH, 0.0);
    let maxs = Vec3::new(HALF_WIDTH, HALF_WIDTH, Stance::Stand.height());
    let up = p + Vec3::Z * STEPSIZE;
    let sweep = |dir: Vec3| world.box_trace(up, up + dir * look, mins, maxs - Vec3::Z * STEPSIZE);
    let mut t = sweep(dir);
    let mut dir = dir;
    // Against a wall the heading only grazes, the body slides along it:
    // feel along the slide instead (mp_ship's wall at (6111, -304, 97),
    // whose foot drops 41 to the deck).
    let n = t.normal.truncate().extend(0.0);
    let slide = dir - n * dir.dot(n);
    if t.fraction * look < DROP_SAMPLE && dir.dot(n) > -GRAZE && slide.length() > 0.1 {
        dir = slide.normalize();
        t = sweep(dir);
    }
    if t.startsolid || t.allsolid {
        return None;
    }
    let reach = t.fraction * look;
    let hurts = vcod_common::pmove::FallHeights::default().min;
    let mut floor = p.z;
    // A sample every DROP_SAMPLE, and half way between for a hole a body
    // falls through where the samples either side stand on its rims (a
    // point trace first, the capsule only where it finds no floor), and
    // one where the sweep stops: a body run at a wall ends up there, over
    // whatever lies before it (mp_ship's crow's nest plate, 11 units from
    // the top's face across the hatch).
    let half = DROP_SAMPLE / 2.0;
    let last = (reach > WALL_NEAR && reach % half > 1.0).then_some(reach);
    let mut d = half;
    while let Some(d_at) = (d <= reach).then_some(d).or(last.filter(|&r| d - half < r)) {
        let at = (p + dir * d_at).truncate().extend(floor + STEPSIZE);
        let between = d_at == d && d % DROP_SAMPLE != 0.0;
        d += half;
        if between
            && world
                .point_trace(
                    at,
                    at - Vec3::Z * (STEPSIZE + JUMP_HEIGHT),
                    MASK_PLAYERSOLID,
                    false,
                )
                .fraction
                < 1.0
        {
            continue;
        }
        let t = world.box_trace(at, at - Vec3::Z * (STEPSIZE + JUMP_HEIGHT), mins, maxs);
        if t.startsolid {
            return None;
        }
        let fall = |from: Vec3, mins: Vec3, maxs: Vec3| {
            let deep = world.box_trace(from, from - Vec3::Z * (STEPSIZE + hurts), mins, maxs);
            if deep.fraction >= 1.0 {
                f32::INFINITY
            } else {
                deep.fraction * (STEPSIZE + hurts) - STEPSIZE
            }
        };
        if t.fraction >= 1.0 {
            return Some(fall(at, mins, maxs));
        }
        // Held by the capsule's side on a hole's rim, too steep to stand
        // on, with nothing under its middle: the body slides off into the
        // hole (the crow's nest hatch, about 40 by 46 units).
        if t.normal.z < MIN_WALK_NORMAL {
            let under = world.point_trace(
                t.endpos,
                t.endpos - Vec3::Z * (STEPSIZE + JUMP_HEIGHT),
                MASK_PLAYERSOLID,
                false,
            );
            if under.fraction >= 1.0 {
                return Some(fall(t.endpos, Vec3::ZERO, Vec3::ZERO));
            }
        }
        if t.normal.z > 0.0 && t.normal.z < MIN_WALK_NORMAL && creases.drains(world, t.endpos) {
            return Some(f32::INFINITY);
        }
        floor = t.endpos.z;
        // A slope or a stair as steep runs as far down within the look:
        // mp_ship's hull slope at (3752, 340-414) runs 49 down in 75 to a
        // wedge off the graph.
        if floor < p.z - JUMP_HEIGHT {
            return Some(p.z - floor);
        }
    }
    None
}

/// Which spots on ground too steep to stand on drain into a crease
/// ([`held_off_ground`]), by 8-unit cell. A level's world is fixed, and the
/// test runs a body for up to 50 ticks, too dear for every bot every tick.
#[derive(Default)]
pub struct Creases(std::collections::HashMap<[i32; 3], bool>);

impl Creases {
    fn drains(&mut self, world: &CollisionWorld, p: Vec3) -> bool {
        let cell = (p / 8.0).floor().as_ivec3().to_array();
        *self
            .0
            .entry(cell)
            .or_insert_with(|| held_off_ground(world, PlayerState::spawn(p, 0.0), Sim::default()))
    }
}

/// How near a wall [`wall_ahead`] calls a heading blocked.
const WALL_NEAR: f32 = 4.0;

/// Whether a body standing at `p` meets a wall within [`WALL_NEAR`] along
/// the flat unit vector `dir`, square enough that it pins rather than
/// slides ([`GRAZE`]). [`drop_ahead`] reads such a heading as no drop.
pub(crate) fn wall_ahead(world: &CollisionWorld, p: Vec3, dir: Vec3) -> bool {
    use vcod_common::pmove::{HALF_WIDTH, STEPSIZE, Stance};
    let mins = Vec3::new(-HALF_WIDTH, -HALF_WIDTH, 0.0);
    let maxs = Vec3::new(HALF_WIDTH, HALF_WIDTH, Stance::Stand.height() - STEPSIZE);
    let up = p + Vec3::Z * STEPSIZE;
    let t = world.box_trace(up, up + dir * WALL_NEAR, mins, maxs);
    t.startsolid || (t.fraction < 1.0 && dir.dot(t.normal.truncate().extend(0.0)) <= -GRAZE)
}

/// Idle cmds until a body dropped at `p` stands on something; `None` when it
/// falls out of the world or never lands.
fn settle(world: &CollisionWorld, p: Vec3, yaw: f32) -> Option<Vec3> {
    let mut ps = PlayerState::spawn(p + Vec3::Z, yaw);
    let idle = UserCmd {
        angles: [0, (yaw * ANGLE2SHORT) as i32, 0],
        ..NULL_USERCMD
    };
    let mut sim = Sim::default();
    for _ in 0..40 {
        sim.tick(world, &mut ps, &idle);
        if ps.on_ground {
            return Some(ps.origin);
        }
    }
    None
}

/// A walk's tick budget for `dist` units: twice the run time, plus slack for
/// a stair or a slide along a wall.
fn run_ticks(dist: f32) -> usize {
    (dist / (vcod_common::pmove::SPEED_RUN * 0.05) * 2.0) as usize + 4
}

/// How far to either side of a node a walk back to it is retried
/// (`NavGraph::link_back`). VERIFIED (measured, `mp_rocket`): the walk west
/// from (11858, 4224, 262) pins on a corner at x 11840 aimed at the node at
/// (11803, 4226), and arrives aimed 8 units north of it.
const SIDESTEP: f32 = 8.0;
/// A body on the ground faster than this, units/s, slows onto a creep's
/// top before it backs off ([`Follower::creep_top`]).
const CREEP_TOP_PACE: f32 = 100.0;
/// The pace across a creep's way a bot comes onto its top at, off a turn
/// ([`creep_takes_a_turn`]): seed 183's 148 on mp_depot's roof.
const TURN_PACE: f32 = 150.0;
/// How far across from a ladder box a fall is retried backing down.
const LADDER_NEAR: f32 = 96.0;
/// The ticks a walk's budget stretches by while the body is in the air:
/// a fall of [`MAX_DROP`] takes 14.
const FALL_TICKS: usize = 16;

/// Inside this of its target, horizontally, a body backing onto a ladder
/// below a ledge creeps: an airborne grab reaches 17 units past the box
/// (`check_ladder_move`, the box shrunk 6 and the trace 8), and at a run the
/// body is past that within the tick it leaves the lip.
const CREEP_RADIUS: f32 = 24.0;
/// The creep's move key, about 16 units/s. The body leaves the lip at 15
/// units past the face and is first low enough to grab (under the ladder's
/// top) a tick later, so two ticks of travel have to stay under 2 units.
const CREEP: i8 = 15;
/// [`CREEP`], for the bots' tests.
#[cfg(test)]
pub const CREEP_KEY: i8 = CREEP;
/// The ticks the creep adds to a backing walk's budget.
const CREEP_TICKS: usize = 16;

/// The move key a body backing toward a point `flat` units away sends:
/// full off the ground near it and on a ladder, a creep at the lip.
pub fn back_move(flat: f32, on_ladder: bool) -> i8 {
    if on_ladder || flat > CREEP_RADIUS {
        -127
    } else {
        -CREEP
    }
}

/// The yaw a climb holds: square into the ladder's face, whatever the
/// head's bearing. Aimed at a head off to one side, the body slides along
/// the rungs to the ladder's edge and pins under what overhangs it there
/// (mp_ship's ladder at x 3354, bot-navigation.md "Ladders").
pub fn climb_yaw(ladder_normal: Vec3) -> Option<f32> {
    let n = ladder_normal.truncate();
    (n.length_squared() > 0.01).then(|| (-n.y).atan2(-n.x).to_degrees())
}

/// The heading a creep holds. The lip is a step or two from the point it
/// backs toward, and re-aiming as the body passes over it would turn it
/// round in the air, away from the face its grab traces toward.
#[derive(Default, Clone, Copy)]
pub struct Creep(Option<f32>);

impl Creep {
    /// The yaw to send with move key `forward`: `want` at full speed, the
    /// first creeping tick's yaw for as long as the creep lasts.
    pub fn hold(&mut self, forward: i8, want: f32) -> f32 {
        if forward == -CREEP {
            *self.0.get_or_insert(want)
        } else {
            self.0 = None;
            want
        }
    }

    /// Whether the last key sent was the creep's.
    pub fn creeping(&self) -> bool {
        self.0.is_some()
    }
}

/// How a walk is driven: facing its way, or backing along it facing the
/// other way, which is how a body gets onto a ladder below a ledge (the grab
/// traces along the view, `ladder_move`), at a run or creeping at the lip
/// ([`back_move`]).
#[derive(Clone, Copy, PartialEq)]
enum Gait {
    Forward,
    /// [`Gait::Forward`] that also jumps at a lip ([`lip_ahead`]): across a
    /// gap to a ledge (`NavGraph::link_leaps`).
    Leap,
    Backward,
    Creep,
}

/// What a walk came to.
enum Walked {
    /// Where it came to rest, and whether it took a jump to get there.
    Arrived(Vec3, bool),
    Blocked,
    /// It left the ground and never arrived; a ladder below may still take it.
    Fell,
}

/// [`walk_as`] forward, backward when the forward run fell, and creeping
/// when that fell too. The creep is last because it is slow: a back down
/// off every ledge on a map is most of a build, and only a ladder below
/// needs it, so neither is tried away from one ([`ladder_near`]).
fn walk(
    world: &CollisionWorld,
    ladders: &[(Vec3, Vec3)],
    from: Vec3,
    target: glam::Vec2,
) -> Option<(Vec3, bool)> {
    walk_to(world, ladders, from, target, None)
}

/// [`walk`], arriving only on `floor` when given.
fn walk_to(
    world: &CollisionWorld,
    ladders: &[(Vec3, Vec3)],
    from: Vec3,
    target: glam::Vec2,
    floor: Option<f32>,
) -> Option<(Vec3, bool)> {
    for gait in [Gait::Forward, Gait::Backward, Gait::Creep] {
        if gait == Gait::Creep && !ladder_near(ladders, from) {
            return None;
        }
        match walk_as(world, from, target, floor, gait) {
            Walked::Arrived(p, jumped) => return Some((p, jumped)),
            Walked::Blocked => return None,
            Walked::Fell => {}
        }
    }
    None
}

/// Whether a creep from `from` also arrives with the body coming onto it
/// off a turn, at [`TURN_PACE`] across the way to `target` either side,
/// where the graph's walk ran straight at the drop. mp_depot's thin ledges
/// at (-2749, -1281, 326) and (-2755, -1150, 326), 182 under the roof, land
/// only the straight run (bot-navigation.md section 3).
fn creep_takes_a_turn(
    world: &CollisionWorld,
    from: Vec3,
    target: glam::Vec2,
    floor: Option<f32>,
) -> bool {
    let dir = (target - from.truncate()).normalize_or_zero();
    let across = glam::Vec2::new(-dir.y, dir.x) * TURN_PACE;
    [across, -across].into_iter().all(|v| {
        matches!(
            walk_with(world, from, Some(v.extend(0.0)), target, floor, Gait::Creep),
            Walked::Arrived(..)
        )
    })
}

/// How far `p` lies from a ladder box, flat.
fn box_flat(b: &(Vec3, Vec3), p: Vec3) -> f32 {
    let q = p.truncate().clamp(b.0.truncate(), b.1.truncate());
    q.distance(p.truncate())
}

/// Whether a ladder box comes within [`LADDER_NEAR`] of `p` across and
/// spans heights from a drop below it to a storey above: the only place
/// a back down off a ledge can find a grab.
fn ladder_near(ladders: &[(Vec3, Vec3)], p: Vec3) -> bool {
    ladders.iter().any(|&(lo, hi)| {
        let near = Vec3::new(LADDER_NEAR, LADDER_NEAR, 0.0);
        let (lo, hi) = (lo - near, hi + near);
        (lo.x..=hi.x).contains(&p.x)
            && (lo.y..=hi.y).contains(&p.y)
            && hi.z >= p.z - MAX_DROP
            && lo.z <= p.z + Z_MERGE
    })
}

/// Runs a body from `from` toward `target`, re-aiming every tick, and
/// reports where it came to rest when it got within [`ARRIVE`] on the
/// ground (and of the height `floor`, when given) without falling past
/// [`MAX_DROP`], and whether it jumped on the way: once where a forward
/// run stalls, or at the lip for a [`Gait::Leap`].
fn walk_as(
    world: &CollisionWorld,
    from: Vec3,
    target: glam::Vec2,
    floor: Option<f32>,
    gait: Gait,
) -> Walked {
    walk_with(world, from, None, target, floor, gait)
}

/// [`walk_as`] with the body moving at `pace` to begin with, in place of
/// the gait's own start.
///
/// A run from the gait's own start that slides on ground too steep to stand
/// on arrives only if no spot it slid over drains into a crease: a body
/// left there at rest comes to rest held off the ground. A bot comes onto
/// an edge off a turn and slower than the run, and on such a slope the pace
/// it carries, not its keys, says where it goes (mp_hurtgen's crease,
/// bot-navigation.md section 3).
fn walk_with(
    world: &CollisionWorld,
    from: Vec3,
    pace: Option<Vec3>,
    target: glam::Vec2,
    floor: Option<f32>,
    gait: Gait,
) -> Walked {
    let mut slid = Vec::new();
    let walked = walk_sliding(world, from, pace, 1.0, target, floor, gait, &mut slid);
    let proving = pace.is_none() && matches!(gait, Gait::Forward | Gait::Backward);
    if proving
        && matches!(walked, Walked::Arrived(..))
        && slid
            .iter()
            .any(|&p| held_off_ground(world, PlayerState::spawn(p, 0.0), Sim::default()))
    {
        return Walked::Blocked;
    }
    walked
}

/// Where a body off the ground left to itself comes to rest is off the
/// ground too: it never lands in [`SETTLE_TICKS`] and then stays within a
/// unit of one spot for [`HELD_TICKS`]. A crease, not a bounce on a slope
/// or a fall. A player left on mp_hurtgen's slope slides into its crease
/// and stays (bot-navigation.md section 3).
fn held_off_ground(world: &CollisionWorld, mut ps: PlayerState, mut sim: Sim) -> bool {
    let mut idle = || {
        sim.tick(world, &mut ps, &NULL_USERCMD);
        !ps.on_ground && !ps.on_ladder
    };
    if !(0..SETTLE_TICKS).all(|_| idle()) {
        return false;
    }
    let at = ps.origin;
    (0..HELD_TICKS).all(|_| {
        sim.tick(world, &mut ps, &NULL_USERCMD);
        !ps.on_ground && !ps.on_ladder && ps.origin.distance(at) < 1.0
    })
}

const SETTLE_TICKS: usize = 40;
const HELD_TICKS: usize = 10;

/// [`walk_with`] without the crease test, the body's weapon moving it at
/// `move_scale`; `slid` gathers the spots where the body was off the
/// ground against a plane too steep to stand on.
#[allow(clippy::too_many_arguments)]
fn walk_sliding(
    world: &CollisionWorld,
    from: Vec3,
    pace: Option<Vec3>,
    move_scale: f32,
    target: glam::Vec2,
    floor: Option<f32>,
    gait: Gait,
    slid: &mut Vec<Vec3>,
) -> Walked {
    let dist = target.distance(from.truncate());
    let on_floor = |z: f32| floor.is_none_or(|f| (z - f).abs() < ARRIVE);
    if dist < ARRIVE && on_floor(from.z) {
        return Walked::Arrived(from, false);
    }
    // A running start: a following bot never stops at a node, and the
    // spin-up from a standstill was a third of every walk.
    let mut ps = PlayerState::spawn(from, 0.0);
    let dir = (target - from.truncate()) / dist;
    // A leap starts standing: a bot comes to rest at its foot first.
    if let Some(v) = pace {
        ps.velocity = v.truncate().extend(0.0);
    } else if gait != Gait::Leap {
        ps.velocity = (dir * vcod_common::pmove::SPEED_RUN).extend(0.0);
    }
    let mut sim = Sim::scaled(move_scale);
    let mut budget = run_ticks(dist);
    // A floor-bound walk is a ladder's (`link_ladders`): room to reach it.
    if floor.is_some() {
        budget = budget.max(60);
    }
    if gait == Gait::Creep {
        budget += CREEP_TICKS;
    }
    let mut stalled = 0;
    let mut creep = Creep::default();
    let mut climbed = false;
    // The highest the body held on the rungs: a walk to a floor no higher
    // than its start that goes up a ladder and over the top arrives by a
    // way a bot heading for the target never takes (mp_depot's ladder at
    // (-417, 1112), bot-navigation.md section 3).
    let mut rung_top = f32::MIN;
    let mut fell = false;
    // Where the body last stood or held on; a fall is measured from here.
    let mut support_z = from.z;
    let mut tick = 0;
    let mut airborne = false;
    let cap = budget + FALL_TICKS + run_ticks(dist);
    // A forward run pinned on the ground jumps once (`jump_key`); one that
    // still doesn't arrive reports what it came to before the jump.
    let mut jumped = false;
    let mut jump = false;
    let mut fell_before_jump = false;
    // A body in the air when the budget runs out is let land: a drop down
    // a steep companionway is slower than the run it replaced.
    while tick < budget || (airborne && tick < budget + FALL_TICKS) {
        tick += 1;
        let before = ps.origin;
        let to = target - ps.origin.truncate();
        let (yaw, forward) = match gait {
            Gait::Forward | Gait::Leap => (to.y.atan2(to.x).to_degrees(), 127),
            Gait::Backward => ((-to.y).atan2(-to.x).to_degrees(), -127),
            Gait::Creep => {
                let forward = back_move(to.length(), ps.on_ladder);
                let yaw = (-to.y).atan2(-to.x).to_degrees();
                (creep.hold(forward, yaw), forward)
            }
        };
        // On a ladder the view goes up, where `ladder_move` climbs at full
        // rate (a level view climbs at a third); backing off it climbs down.
        let pitch = if ps.on_ladder { -LADDER_PITCH } else { 0.0 };
        let mut cmd = UserCmd {
            forward,
            up: if std::mem::take(&mut jump) { 127 } else { 0 },
            angles: [(pitch * ANGLE2SHORT) as i32, (yaw * ANGLE2SHORT) as i32, 0],
            ..NULL_USERCMD
        };
        if gait == Gait::Leap && !jumped && ps.on_ground {
            // The lip ([`lip_ahead`]): the tick run without the jump, kept
            // when it stays on the ground, run again with it when it does not.
            let (mut trial, mut run) = (ps, sim.clone());
            run.tick(world, &mut trial, &cmd);
            if trial.on_ground || trial.on_ladder {
                (ps, sim) = (trial, run);
            } else {
                jumped = true;
                fell_before_jump = fell;
                budget = budget.max(tick + JUMP_TICKS + run_ticks(to.length()));
                cmd.up = 127;
                sim.tick(world, &mut ps, &cmd);
            }
        } else {
            sim.tick(world, &mut ps, &cmd);
        }
        // Landed past the budget: the run back to the target from there.
        if airborne && ps.on_ground && tick >= budget {
            let rest = run_ticks(target.distance(ps.origin.truncate()));
            budget = budget.max((tick + rest).min(cap));
        }
        airborne = !ps.on_ground && !ps.on_ladder;
        if ps.on_ladder {
            rung_top = rung_top.max(ps.origin.z);
        }
        if ps.sliding() && !ps.on_ladder {
            slid.push(ps.origin);
        }
        if ps.on_ground || ps.on_ladder {
            support_z = ps.origin.z;
        } else {
            fell = true;
            if ps.origin.z < support_z - MAX_DROP {
                return Walked::Fell;
            }
        }
        if ps.on_ground && target.distance(ps.origin.truncate()) < ARRIVE && on_floor(ps.origin.z) {
            if under_ground(world, ps.origin)
                || floor.is_some_and(|f| rung_top > f.max(from.z) + vcod_common::pmove::JUMP_HEIGHT)
            {
                return Walked::Blocked;
            }
            return Walked::Arrived(ps.origin, jumped);
        }
        // Right under a floor more than a step up, the jump a bot makes
        // there (`jump_cue`): re-aimed at a point overhead, the run only
        // jitters and never stalls.
        if gait == Gait::Forward
            && !jumped
            && ps.on_ground
            && floor.is_some_and(|f| f - ps.origin.z > vcod_common::pmove::STEPSIZE)
            && target.distance(ps.origin.truncate()) < JUMP_UNDER
            && jump_clear(world, &ps, target)
        {
            jumped = true;
            jump = true;
            fell_before_jump = fell;
            stalled = 0;
            let rest = run_ticks(target.distance(ps.origin.truncate()));
            budget = budget.max(tick + JUMP_TICKS + rest);
            continue;
        }
        // A climb is slower than a run and straight up, so the budget
        // stretches while it goes on, with the run still left past the top,
        // and progress is the height gained.
        let moved = if ps.on_ladder {
            let rest = run_ticks(target.distance(ps.origin.truncate()));
            budget = budget.max(tick + rest).min(LADDER_TICKS + rest);
            (ps.origin.z - before.z).abs() * 4.0
        } else {
            (ps.origin - before).length()
        };
        // Pinned against a wall: two ticks without moving two units (half
        // one, creeping). A run that only slides along a wall, or overshoots
        // off a ledge and turns back, keeps its budget, and so does one
        // hanging in the air at the top of a ladder before it tips over.
        climbed |= ps.on_ladder;
        let least = if forward.abs() < 127 { 0.5 } else { 2.0 };
        let hanging = (climbed || jumped) && !ps.on_ground && !ps.on_ladder;
        if moved < least && !hanging {
            stalled += 1;
            if stalled == 2 {
                if gait == Gait::Forward
                    && !jumped
                    && ps.on_ground
                    && jump_clear(world, &ps, target)
                {
                    jumped = true;
                    jump = true;
                    fell_before_jump = fell;
                    stalled = 0;
                    let rest = run_ticks(target.distance(ps.origin.truncate()));
                    budget = budget.max(tick + JUMP_TICKS + rest);
                    continue;
                }
                break;
            }
        } else {
            stalled = 0;
        }
    }
    if jumped {
        fell = fell_before_jump;
    }
    if fell { Walked::Fell } else { Walked::Blocked }
}

/// Under this many units a tick a body is pinned: a run against a ledge's
/// face, which a jump may clear.
const PINNED: f32 = 2.0;
/// A body this close under a jump edge's top, flat, jumps whatever its pace:
/// past it, a run carries it off the ledge's foot (mp_depot's stair to the
/// documents' step).
const JUMP_UNDER: f32 = 16.0;
/// The ticks a jump stays in the air: 2 * 249.8 / 800 s.
const JUMP_TICKS: usize = 13;

/// Whether a body on a jump edge jumps now, its target `flat` away and
/// `rise` above. A leap jumps at the lip ([`lip_ahead`]), as its walk did;
/// any other jump where the walk stalled, pinned (closing on the target
/// under [`PINNED`] units a tick), or right under a target above it
/// ([`JUMP_UNDER`]). Never once standing on the top.
pub fn jump_cue(leap: bool, lip: bool, closing: f32, flat: f32, rise: f32) -> bool {
    use vcod_common::pmove::STEPSIZE;
    if flat < REACH && rise.abs() < JUMP_PASS {
        false
    } else if leap {
        lip
    } else {
        closing < PINNED || (flat < JUMP_UNDER && rise > STEPSIZE)
    }
}

/// The jump key for a cue: pressed on the ground when the key was up last
/// cmd. `PM_CheckJump` refuses a held key, so a second try needs a release.
pub fn jump_key(on_ground: bool, cue: bool, held: bool) -> i8 {
    if on_ground && cue && !held { 127 } else { 0 }
}

/// A body at rest on the ground with its jump off cooldown: what a leap was
/// proved from.
pub fn ready_to_leap(ps: &PlayerState) -> bool {
    ps.on_ground
        && ps.velocity.truncate().length() * (FRAME_MS as f32 / 1000.0) < PINNED
        && ps.since_jump_ms >= vcod_common::pmove::JUMP_COOLDOWN_MS
}

/// Key `k` cut to the graph's pace for a weapon moving the body at
/// `move_scale`: a following bot holds this on the ground (`Bot::think`),
/// so a light weapon runs at the 190 units/s the graph's walks ran at.
pub fn paced_key(k: i8, move_scale: f32) -> i8 {
    if move_scale > 1.0 {
        (f32::from(k) / move_scale).round() as i8
    } else {
        k
    }
}

/// [`paced_key`] for the weapon `ps` holds.
fn paced_key_for(
    k: i8,
    ps: &PlayerState,
    weapons: &[Option<vcod_common::weapon::WeaponDef>],
) -> i8 {
    let scale = weapons
        .get(ps.weapon as usize)
        .and_then(Option::as_ref)
        .map_or(1.0, |d| d.move_speed_scale);
    paced_key(k, scale)
}

/// Whether the body `ps` loses the ground in the coming tick running at
/// `target`: the tick's move, run on a copy. A leap jumps there
/// (`Gait::Leap`, and the bots on its edges). The move, not the pace: a
/// body speeding up from rest covers half again what its velocity says,
/// and a bot that runs faster than the walk's body (`weapons`' move
/// scale) walked off mp_depot's step to the documents' crate a tick
/// before the lip read (bot-navigation.md, "Jumps").
pub fn lip_ahead(
    world: &CollisionWorld,
    ps: &PlayerState,
    target: glam::Vec2,
    weapons: &[Option<vcod_common::weapon::WeaponDef>],
) -> bool {
    let mut ps = *ps;
    let to = target - ps.origin.truncate();
    let cmd = UserCmd {
        server_time: FRAME_MS,
        forward: paced_key_for(127, &ps, weapons),
        angles: [0, (to.y.atan2(to.x).to_degrees() * ANGLE2SHORT) as i32, 0],
        ..NULL_USERCMD
    };
    let dt = FRAME_MS as f32 / 1000.0;
    let mw = MoveWorld::bare(world);
    player_step(
        &mut ps,
        &mut [0; 3],
        &mut EventRing::default(),
        &cmd,
        dt,
        &mw,
        weapons,
    );
    !ps.on_ground && !ps.on_ladder
}

/// Ticks an [`unpin_yaw`] heading is run for, and held by the bot.
pub const UNPIN_TICKS: usize = 4;
/// A body slower than this, units/s, on the ground short of its waypoint
/// is checked for a pin ([`unpin_yaw`]).
pub const UNPIN_SPEED: f32 = 20.0;

/// A heading that frees the body `ps`, standing still on the ground and
/// pinned short of `to` on its floor: `None` when the run straight at `to`
/// closes 2 units in [`UNPIN_TICKS`]; else, of the headings 10 to 90
/// degrees either side of it, every 10, and the compass axes within 90 of
/// it, the one whose run, held for those ticks,
/// closes on `to` most and by 8 or more, staying on the ground within a
/// step of `to`'s floor and off `hazards`. A body at rest in a crease where
/// two faces meet does not move at all toward a point a few degrees past
/// one of them, and the same run with a running start slides through
/// (mp_ship's corner at x 5375.1, bot-navigation.md section 3).
pub fn unpin_yaw(
    world: &CollisionWorld,
    hazards: &[BrushHull],
    ps: &PlayerState,
    to: Vec3,
    weapons: &[Option<vcod_common::weapon::WeaponDef>],
) -> Option<f32> {
    let flat = (to - ps.origin).truncate();
    if !ps.on_ground
        || ps.on_ladder
        || ps.velocity.truncate().length() >= UNPIN_SPEED
        || flat.length() < ARRIVE
        || (to.z - ps.origin.z).abs() > vcod_common::pmove::STEPSIZE
    {
        return None;
    }
    let mw = MoveWorld::bare(world);
    let dt = FRAME_MS as f32 / 1000.0;
    // How much nearer `to` a run held at `yaw` ends, when it keeps its
    // footing on the floor all the way.
    let key = paced_key_for(127, ps, weapons);
    let closes = |yaw: f32| {
        let mut ps = *ps;
        let mut delta = [0; 3];
        let mut ring = EventRing::default();
        for i in 0..UNPIN_TICKS {
            let cmd = UserCmd {
                server_time: FRAME_MS * (i as i32 + 1),
                forward: key,
                angles: [0, (yaw * ANGLE2SHORT) as i32, 0],
                ..NULL_USERCMD
            };
            player_step(&mut ps, &mut delta, &mut ring, &cmd, dt, &mw, weapons);
            if !ps.on_ground
                || (ps.origin.z - to.z).abs() > vcod_common::pmove::STEPSIZE
                || hazard(hazards, ps.origin)
            {
                return None;
            }
        }
        Some(flat.length() - (to - ps.origin).truncate().length())
    };
    let straight = flat.y.atan2(flat.x).to_degrees();
    if closes(straight).is_none_or(|d| d >= 2.0) {
        return None;
    }
    // Most faces are axial, and the run exactly along one slides where
    // one a hair into it pins: the axes go in as they are.
    let axes = [0.0f32, 90.0, 180.0, 270.0]
        .into_iter()
        .filter(|a| ((a - straight + 540.0).rem_euclid(360.0) - 180.0).abs() < 90.0);
    (1..=9)
        .flat_map(|i| [straight + i as f32 * 10.0, straight - i as f32 * 10.0])
        .chain(axes)
        .filter_map(|yaw| Some((closes(yaw)?, yaw)))
        .filter(|&(d, _)| d >= 8.0)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, yaw)| yaw)
}

/// Ticks a [`push_off`] flight is followed before it counts as failed.
const PUSH_OFF_TICKS: usize = 40;

/// The yaw of a push-off from the rungs `ps` hangs on toward `to`, a
/// floor behind the climber's back: `Some` when `to` lies 8 or more
/// units out from the face, between a jump under and a step over the
/// body, and the push-off run on a copy lands on that floor within 64
/// units of `to`. The flight is the jump facing out from the face, then
/// the back key, held facing the same way, until the body lands. A ladder
/// move climbs and strafes along the face but never steps off it, so a
/// floor behind the climber is left only by the jump, and the jump alone
/// carries a body about 60 units out (bot-navigation.md section 3, the
/// ladder at (2150, -214)).
pub fn push_off(
    world: &CollisionWorld,
    ps: &PlayerState,
    to: Vec3,
    weapons: &[Option<vcod_common::weapon::WeaponDef>],
) -> Option<f32> {
    let n = ps.ladder_normal.truncate();
    if !ps.on_ladder || n.length_squared() < 0.01 {
        return None;
    }
    let n = n.normalize();
    let dz = to.z - ps.origin.z;
    let out = (to.truncate() - ps.origin.truncate()).dot(n);
    if out < 8.0 || !(-vcod_common::pmove::JUMP_HEIGHT..=vcod_common::pmove::STEPSIZE).contains(&dz)
    {
        return None;
    }
    let yaw = n.y.atan2(n.x).to_degrees();
    let mut ps = *ps;
    let mut delta = [0; 3];
    let mut ring = EventRing::default();
    let mw = MoveWorld::bare(world);
    let dt = FRAME_MS as f32 / 1000.0;
    for i in 0..PUSH_OFF_TICKS {
        let cmd = UserCmd {
            server_time: FRAME_MS * (i as i32 + 1),
            forward: if i == 0 { 0 } else { -127 },
            up: if i == 0 { 127 } else { 0 },
            angles: [0, (yaw * ANGLE2SHORT) as i32, 0],
            ..NULL_USERCMD
        };
        player_step(&mut ps, &mut delta, &mut ring, &cmd, dt, &mw, weapons);
        if i == 0 && ps.on_ladder {
            return None;
        }
        if ps.on_ground || ps.on_ladder {
            break;
        }
    }
    let landed = ps.on_ground
        && !ps.on_ladder
        && (ps.origin.z - to.z).abs() <= vcod_common::pmove::STEPSIZE
        && ps.origin.truncate().distance(to.truncate()) < 64.0;
    landed.then_some(yaw)
}

/// Whether a jump from where `ps` stands could clear what pins it: room
/// ahead toward `target` at the top of the jump, or under a ceiling lower
/// than the jump's height, as high as the body rises. A wall to the
/// ceiling fails it, which keeps a jump off the walks that end at one.
fn jump_clear(world: &CollisionWorld, ps: &PlayerState, target: glam::Vec2) -> bool {
    let (mins, maxs) = (ps.mins(), ps.maxs());
    let up = world.box_trace(
        ps.origin,
        ps.origin + Vec3::Z * vcod_common::pmove::JUMP_HEIGHT,
        mins,
        maxs,
    );
    if up.startsolid {
        return false;
    }
    let top = up.endpos;
    let dir = (target - ps.origin.truncate())
        .normalize_or_zero()
        .extend(0.0);
    let ahead = world.box_trace(top, top + dir * 16.0, mins, maxs);
    !ahead.startsolid && ahead.fraction == 1.0
}

/// Whether `p` lies beneath a sheet of ground. Terrain and patches are
/// surfaces, not solids, so a run that finds a gap at a sheet's edge can
/// walk the map's floor under them (mp_hurtgen's, 48 units below its river
/// bed). From there the ray up either stops on the underside of a triangle
/// whose own face, `(b - a) x (c - a)`, points up, or passes through a
/// one-sided terrain triangle and the same ray back down hits it. Both rays
/// see what a player clips, not the brushes only a shot meets. Nothing else
/// the ray back down meets is ground over the point: a patch (mp_ship's
/// mast ladder stands 540 units under a spar's top facet), or a ceiling
/// just over the head that the ray starts against (a low doorway on
/// mp_pavlov kept 540 nodes behind it off the graph). The rays start a step
/// over the feet, not at the head: mp_carentan's terrain runs 38 units over
/// the base brush west of its boundary wall, and from the head a flood
/// seeded there walked under it into the town.
fn under_ground(world: &CollisionWorld, p: Vec3) -> bool {
    let start = p + Vec3::Z * vcod_common::pmove::STEPSIZE;
    let up = world.point_trace(start, start + Vec3::Z * 8192.0, MASK_PLAYERSOLID, false);
    if let Some(Prim::Tri(t)) = up.hit {
        let [a, b, c] = world.tris[t as usize];
        if (b - a).cross(c - a).normalize_or_zero().z > 0.7 {
            return true;
        }
    }
    let down = world.point_trace(up.endpos - Vec3::Z, start, MASK_PLAYERSOLID, false);
    matches!(down.hit, Some(Prim::Tri(_)))
}

/// The cmd times of one 50 ms tick for a body that starts it at `start`:
/// one, or on the rungs off the ground three to six, the count cycling
/// with the tick, as a client's frames come. Off the top of a ladder the
/// step-up has to clear the lip before `PM_CheckLadderMove`'s box lets go
/// 8 units under the ladder's top, and the step's slide carries the wall
/// glue's 500 units/s: whether it clears depends on the frame's length and
/// phase. In 50 ms frames, or any one length, a body can bob under a lip
/// for good where retail's client, whose frames vary, gets over on the
/// next try (docs/research/bot-navigation.md, "Ladders"). Shared by the
/// bots (`Server::step_bots`) and the graph's walks, so a bot climbs the
/// way the graph proved it.
pub fn tick_times(start: i32, climbing: bool) -> impl Iterator<Item = i32> {
    let n = if climbing {
        LADDER_SUBSTEPS[(start / FRAME_MS).rem_euclid(LADDER_SUBSTEPS.len() as i32) as usize]
    } else {
        1
    };
    (1..=n).map(move |i| start + FRAME_MS * i / n)
}

/// Cmds a climbing body sends a tick, by tick ([`tick_times`]).
const LADDER_SUBSTEPS: [i32; 4] = [5, 4, 3, 6];

/// The bits of a client a walk carries between cmds.
#[derive(Clone, Default)]
struct Sim {
    time: i32,
    delta: [i32; 3],
    ring: EventRing,
    /// Weapon 0's def, when the body moves at a `moveSpeedScale` other
    /// than 1 ([`Sim::scaled`]).
    weapons: Option<Arc<[Option<vcod_common::weapon::WeaponDef>]>>,
}

impl Sim {
    /// A walk whose body holds a weapon moving it at `move_scale`, as weapon
    /// 0: the walk's body holds no other.
    fn scaled(move_scale: f32) -> Sim {
        if move_scale <= 0.0 || move_scale == 1.0 {
            return Sim::default();
        }
        let map = HashMap::from([("moveSpeedScale".to_string(), move_scale.to_string())]);
        Sim {
            weapons: Some(Arc::new([Some(vcod_common::weapon::WeaponDef::from_map(
                &map,
            ))])),
            ..Sim::default()
        }
    }

    fn tick(&mut self, world: &CollisionWorld, ps: &mut PlayerState, cmd: &UserCmd) {
        let mw = MoveWorld::bare(world);
        let weapons = self.weapons.as_deref().unwrap_or(&[]);
        for server_time in tick_times(self.time, ps.on_ladder && !ps.on_ground) {
            let cmd = UserCmd {
                server_time,
                ..*cmd
            };
            for (step, dt) in chop(self.time, &cmd) {
                player_step(ps, &mut self.delta, &mut self.ring, &step, dt, &mw, weapons);
            }
            self.time = server_time;
        }
    }
}

/// Every spawn point in an ents lump: the gametypes' `mp_*_spawn*` classes
/// and `info_player_start`.
pub fn spawn_points(entities: &str) -> Vec<[f32; 3]> {
    vcod_common::bsp::entity_blocks(entities)
        .iter()
        .filter(|b| {
            b.get("classname").is_some_and(|c| {
                c == "info_player_start" || (c.starts_with("mp_") && c.contains("_spawn"))
            })
        })
        .filter_map(|b| vcod_common::bsp::parse_vec3(b.get("origin")?))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 0 - 1 - 2 in a row with a shortcut 0 -> 3 -> 2 that is longer, and a
    /// one-way drop 2 -> 4.
    fn tiny() -> NavGraph {
        NavGraph::from_parts(
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(32.0, 0.0, 0.0),
                Vec3::new(64.0, 0.0, 0.0),
                Vec3::new(32.0, 64.0, 0.0),
                Vec3::new(96.0, 0.0, -100.0),
            ],
            vec![vec![1, 3], vec![0, 2], vec![1, 4], vec![0, 2], vec![]],
        )
    }

    #[test]
    fn a_star_takes_the_shorter_way() {
        assert_eq!(tiny().path(0, 2), Some(vec![0, 1, 2]));
        assert_eq!(tiny().path(2, 2), Some(vec![2]));
    }

    #[test]
    fn a_one_way_drop_has_no_way_back() {
        let g = tiny();
        assert_eq!(g.path(0, 4), Some(vec![0, 1, 2, 4]));
        assert_eq!(g.path(4, 0), None);
    }

    #[test]
    fn a_drop_splits_the_components() {
        let c = tiny().components();
        assert!(c[0] == c[1] && c[1] == c[2] && c[2] == c[3]);
        assert_ne!(c[4], c[0], "the node below the drop is its own component");
    }

    /// Six nodes in a two-way row along +x, 32 apart.
    fn row() -> NavGraph {
        let nodes = (0..6)
            .map(|i| Vec3::new(i as f32 * 32.0, 0.0, 0.0))
            .collect();
        let edges = (0..6u32)
            .map(|i| {
                [i.wrapping_sub(1), i + 1]
                    .into_iter()
                    .filter(|&n| n < 6)
                    .collect()
            })
            .collect();
        NavGraph::from_parts(nodes, edges)
    }

    /// With no waypoint while the next plan is searched for, the body waits
    /// on the last one handed out, near it on its floor, for a while.
    #[test]
    fn a_body_waits_on_its_last_waypoint_while_a_plan_is_searched_for() {
        use crate::bots::Goal;
        let g = row();
        let mut f = Follower::default();
        let wp = |f: &mut Follower, goal, mut budget| {
            f.waypoint(
                &g,
                None,
                goal,
                [0.0; 3],
                Footing::STANDING,
                &mut budget,
                &mut || 0,
            )
        };
        assert_eq!(
            wp(&mut f, Goal::To([160.0, 0.0, 0.0]), 100),
            Some([32.0, 0.0, 0.0])
        );
        // A new goal, and no budget left this tick to plan for it.
        assert_eq!(wp(&mut f, Goal::To([0.0, 0.0, 0.0]), 0), None);
        let at = [10.0, 0.0, 0.0];
        assert_eq!(f.rest_point(at, Footing::STANDING), Some([32.0, 0.0, 0.0]));
        let rungs = Footing {
            on_ground: false,
            on_ladder: true,
            ..Footing::STANDING
        };
        assert_eq!(f.rest_point(at, rungs), None, "not on a ladder");
        assert_eq!(
            f.rest_point([10.0, 0.0, 40.0], Footing::STANDING),
            None,
            "a floor up"
        );
        for _ in 0..WAIT_TICKS {
            wp(&mut f, Goal::To([0.0, 0.0, 0.0]), 0);
        }
        assert_eq!(f.rest_point(at, Footing::STANDING), None, "waited out");
    }

    /// The run off mp_ship's mast platform at (3564, -127, 896) onto the
    /// narrow ledge at 760 lands at the graph's 190 units/s and flies over
    /// it at a light weapon's 228, to the ledge at 480 that killed 15
    /// bots in 384 seeds: why a following bot runs at the graph's pace
    /// (`paced_key`).
    #[test]
    fn the_run_off_the_mast_platform_lands_only_at_the_graphs_pace() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let top = Vec3::new(3564.0, -127.0, 896.125);
        let ledge = Vec3::new(3520.0, -162.0, 760.0);
        let run = |scale: f32| {
            let pace = (ledge - top).truncate().normalize() * 190.0 * scale;
            walk_sliding(
                &world.collision,
                top,
                Some(pace.extend(0.0)),
                scale,
                ledge.truncate(),
                Some(ledge.z),
                Gait::Forward,
                &mut Vec::new(),
            )
        };
        assert!(matches!(run(1.0), Walked::Arrived(..)));
        assert!(matches!(run(1.2), Walked::Fell));
        assert_eq!(paced_key(127, 1.2), 106);
    }

    /// mp_ship's deck at (3515, -323, 631) has an edge to the narrow ledge
    /// at 580 that no gait lands node to node: bots ran it and fell past
    /// the ledge to the hold. It is risky.
    #[test]
    fn a_drop_no_gait_lands_node_to_node_is_risky() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let g = graph_for("mp_ship", &world);
        let deck = g.nearest([3515.0, -323.0, 631.0]).unwrap();
        let ledge = g.nearest([3488.0, -358.0, 580.0]).unwrap();
        assert!(g.edges[deck as usize].contains(&ledge));
        let (p, q) = (g.nodes[deck as usize], g.nodes[ledge as usize]);
        for gait in [Gait::Forward, Gait::Backward, Gait::Creep] {
            assert!(!matches!(
                walk_as(&world.collision, p, q.truncate(), Some(q.z), gait),
                Walked::Arrived(..)
            ));
        }
        assert!(g.risky.contains(&(deck, ledge)));
    }

    #[test]
    fn a_follower_hands_out_the_path_then_the_point() {
        use crate::bots::Goal;
        let g = row();
        let mut f = Follower::default();
        let goal = Goal::To([170.0, 0.0, 40.0]);
        let mut budget = 100;
        let mut rand = || 0;
        let mut at = [0.0, 0.0, 0.0];
        let mut seen = Vec::new();
        let mut planned = None;
        for _ in 0..10 {
            let Some(w) = f.waypoint(
                &g,
                None,
                goal,
                at,
                Footing::STANDING,
                &mut budget,
                &mut rand,
            ) else {
                panic!("no waypoint at {at:?}");
            };
            seen.push(w[0]);
            planned.get_or_insert(budget);
            if w == [170.0, 0.0, 40.0] {
                break;
            }
            at = w;
        }
        assert_eq!(seen, [32.0, 64.0, 96.0, 128.0, 160.0, 170.0]);
        assert_eq!(Some(budget), planned, "one plan for the whole walk");
    }

    /// A `side` x `side` two-way grid 32 apart, and one node 1000 units off
    /// it with no edges: an island no plan reaches.
    fn grid_and_island(side: u32) -> NavGraph {
        let mut nodes: Vec<Vec3> = (0..side * side)
            .map(|i| Vec3::new((i % side) as f32 * 32.0, (i / side) as f32 * 32.0, 0.0))
            .collect();
        let mut edges: Vec<Vec<u32>> = (0..side * side)
            .map(|i| {
                let (x, y) = (i % side, i / side);
                let mut out = Vec::new();
                if x > 0 {
                    out.push(i - 1);
                }
                if x + 1 < side {
                    out.push(i + 1);
                }
                if y > 0 {
                    out.push(i - side);
                }
                if y + 1 < side {
                    out.push(i + side);
                }
                out
            })
            .collect();
        nodes.push(Vec3::new(-1000.0, -1000.0, 0.0));
        edges.push(Vec::new());
        NavGraph::from_parts(nodes, edges)
    }

    /// The island is ruled out by the component table, not by a search of
    /// all 40000 nodes: the plan heads for the grid's corner nearest it at
    /// the cost of the walk there, a handful of expansions.
    #[test]
    fn an_unreachable_goal_fails_without_a_search() {
        let g = grid_and_island(200);
        let island = g.nodes.len() as u32 - 1;
        assert!(!g.reaches(0, island) && !g.reaches(island, 0));
        assert_eq!(g.path(5, island), None);
        let mut f = Follower::default();
        let mut budget = 4000;
        let at = [64.0, 64.0, 0.0];
        let w = f.waypoint(
            &g,
            None,
            crate::bots::Goal::To([-1000.0, -1000.0, 0.0]),
            at,
            Footing::STANDING,
            &mut budget,
            &mut || 0,
        );
        assert!(w.is_some(), "heads for the corner");
        assert!(4000 - budget < 10, "spent {}", 4000 - budget);
        assert_eq!(f.path.last(), Some(&0), "ends at the corner");
        // From the island nothing is reached: no search and no waypoint.
        let mut f = Follower::default();
        let mut budget = 4000;
        let w = f.waypoint(
            &g,
            None,
            crate::bots::Goal::To([0.0; 3]),
            [-1000.0, -1000.0, 0.0],
            Footing::STANDING,
            &mut budget,
            &mut || 0,
        );
        assert_eq!((w, budget), (None, 4000));
    }

    /// A plan longer than one tick's slice resumes where it stopped: no
    /// tick spends more than [`PLAN_SLICE`], and the path still arrives.
    #[test]
    fn a_long_plan_spreads_over_ticks() {
        // A serpentine: rows joined at alternate ends, so A*'s straight-line
        // guess is wrong nearly everywhere.
        let side = 120u32;
        let nodes = (0..side * side)
            .map(|i| Vec3::new((i % side) as f32 * 32.0, (i / side) as f32 * 32.0, 0.0))
            .collect();
        let edges = (0..side * side)
            .map(|i| {
                let (x, y) = (i % side, i / side);
                let mut out = Vec::new();
                if x > 0 {
                    out.push(i - 1);
                }
                if x + 1 < side {
                    out.push(i + 1);
                }
                let open = if y % 2 == 0 { side - 1 } else { 0 };
                if x == open && y + 1 < side {
                    out.push(i + side);
                }
                let below = if y % 2 == 1 { side - 1 } else { 0 };
                if x == below && y > 0 {
                    out.push(i - side);
                }
                out
            })
            .collect();
        let g = NavGraph::from_parts(nodes, edges);
        let goal = crate::bots::Goal::To([0.0, (side - 1) as f32 * 32.0, 0.0]);
        let mut f = Follower::default();
        let mut ticks = 0;
        let w = loop {
            let mut budget = 4000;
            let w = f.waypoint(
                &g,
                None,
                goal,
                [0.0; 3],
                Footing::STANDING,
                &mut budget,
                &mut || 0,
            );
            assert!(
                4000 - budget <= PLAN_SLICE,
                "tick {ticks} spent {}",
                4000 - budget
            );
            ticks += 1;
            if w.is_some() || ticks > 100 {
                break w;
            }
        };
        assert!(ticks > 1, "one tick held the whole plan");
        assert_eq!(w, Some([32.0, 0.0, 0.0]));
        assert_eq!(f.path.len() as u32, side * side);
    }

    #[test]
    fn a_spent_plan_budget_leaves_the_bot_without_a_waypoint() {
        let g = row();
        let mut f = Follower::default();
        let w = f.waypoint(
            &g,
            None,
            crate::bots::Goal::Roam,
            [0.0; 3],
            Footing::STANDING,
            &mut 0,
            &mut || 0,
        );
        assert_eq!(w, None);
    }

    #[test]
    fn a_roam_heads_for_a_far_node() {
        let g = row();
        let mut f = Follower::default();
        let mut picks = [1, 5].into_iter();
        let mut rand = || picks.next().unwrap_or(0);
        let w = f.waypoint(
            &g,
            None,
            crate::bots::Goal::Roam,
            [0.0; 3],
            Footing::STANDING,
            &mut 100,
            &mut rand,
        );
        assert_eq!(w, Some([32.0, 0.0, 0.0]));
        // Nothing is ROAM_MIN away on a 160-unit row: the farthest pick wins.
        assert_eq!(f.dest, Some(5));
    }

    /// Away from a threat at one end of the row, the farthest pick that is
    /// nearer the bot than the threat wins.
    #[test]
    fn a_flight_heads_away_from_the_threat() {
        let g = row();
        let mut f = Follower::default();
        let mut picks = [0, 4, 1, 5, 3].into_iter().cycle();
        let mut rand = || picks.next().unwrap();
        let goal = crate::bots::Goal::Away([0.0, 0.0, 0.0]);
        let w = f.waypoint(
            &g,
            None,
            goal,
            [64.0, 0.0, 0.0],
            Footing::STANDING,
            &mut 100,
            &mut rand,
        );
        assert_eq!(f.dest, Some(5));
        assert_eq!(w, Some([96.0, 0.0, 0.0]));
        // The threat moves past the far end: the flight turns round.
        let goal = crate::bots::Goal::Away([300.0, 0.0, 0.0]);
        f.waypoint(
            &g,
            None,
            goal,
            [64.0, 0.0, 0.0],
            Footing::STANDING,
            &mut 100,
            &mut rand,
        );
        assert_eq!(f.dest, Some(0));
    }

    /// A bot pinned on its way from node 1 to node 2 (by a body guarding the
    /// stairs, say) plans round that edge, through node 3, once it gives up.
    #[test]
    fn a_follower_stuck_on_an_edge_plans_round_it() {
        let g = NavGraph::from_parts(
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(32.0, 0.0, 0.0),
                Vec3::new(64.0, 0.0, 0.0),
                Vec3::new(60.0, 30.0, 0.0),
            ],
            vec![vec![1], vec![0, 2, 3], vec![1, 3], vec![1, 2]],
        );
        let mut f = Follower::default();
        let goal = crate::bots::Goal::To([64.0, 0.0, 0.0]);
        let mut budget = 1000;
        let at = [32.0, 0.0, 0.0];
        let mut last = None;
        for _ in 0..=STUCK_TICKS + 1 {
            last = f.waypoint(
                &g,
                None,
                goal,
                at,
                Footing::STANDING,
                &mut budget,
                &mut || 0,
            );
        }
        assert_eq!(last, None, "still steering at a waypoint it never nears");
        let w = f.waypoint(
            &g,
            None,
            goal,
            at,
            Footing::STANDING,
            &mut budget,
            &mut || 0,
        );
        assert_eq!(
            w,
            Some([60.0, 30.0, 0.0]),
            "planned through the blocked edge"
        );
    }

    /// The bot's own stall circle sends it on a spell: the edge under way
    /// is planned round, and the plan from wherever the spell leaves it
    /// does not take it again.
    #[test]
    fn a_spell_plans_round_the_edge_it_began_on() {
        let g = NavGraph::from_parts(
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(32.0, 0.0, 0.0),
                Vec3::new(64.0, 0.0, 0.0),
                Vec3::new(60.0, 30.0, 0.0),
            ],
            vec![vec![1], vec![0, 2, 3], vec![1, 3], vec![1, 2]],
        );
        let mut f = Follower::default();
        let goal = crate::bots::Goal::To([64.0, 0.0, 0.0]);
        let mut budget = 1000;
        let mut at = |f: &mut Follower, p: [f32; 3]| {
            f.waypoint(&g, None, goal, p, Footing::STANDING, &mut budget, &mut || 0)
        };
        assert_eq!(at(&mut f, [32.0, 0.0, 0.0]), Some([64.0, 0.0, 0.0]));
        f.stuck();
        assert_eq!(f.current(), (None, None), "the path is dropped");
        assert_eq!(
            at(&mut f, [32.0, 0.0, 0.0]),
            Some([60.0, 30.0, 0.0]),
            "round the edge it was stuck on"
        );
    }

    /// mp_ship's crow's nest: a bot planned from the top at 1254, walked off
    /// it onto the hatch's rim at 1219 while the search ran, and headed
    /// back for the top's node 35 over its feet with no way up from there;
    /// it pinned on the top's face and slid down the hatch. A plan whose
    /// first node is a floor over the feet is planned again from a node a
    /// walk reaches.
    #[test]
    fn a_plan_that_starts_a_floor_up_is_planned_again() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let g = graph_for("mp_ship", &world);
        let w = Some(&world.collision);
        let goal = crate::bots::Goal::To([4935.7, 126.9, 1254.125]);
        let mut f = Follower::default();
        let top = [4928.5, 60.75, 1254.125];
        let rim = [4961.2, 38.7, 1219.125];
        let mut budget = 1;
        assert_eq!(
            f.waypoint(&g, w, goal, top, Footing::STANDING, &mut budget, &mut || 0),
            None,
            "the search runs on"
        );
        let at = |f: &mut Follower| {
            let mut budget = 100_000;
            f.waypoint(&g, w, goal, rim, Footing::STANDING, &mut budget, &mut || 0)
        };
        assert_eq!(at(&mut f), None, "the plan from the top is dropped");
        let wp = at(&mut f).expect("a plan from the rim");
        assert!(wp[2] < rim[2] + 1.0, "{wp:?}");
    }

    /// West of mp_carentan's boundary wall the map's base brush runs on
    /// under the terrain; a flood seeded there reached the town at this
    /// point, with the terrain sheet under 72 units over the feet.
    #[test]
    fn carentans_floor_under_a_low_terrain_sheet_is_under_ground() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let entry = fs.resolve_map("mp_carentan").unwrap();
        let bsp = vcod_common::bsp::parse(&fs.read(&entry).unwrap()).unwrap();
        let world = crate::world::World::from_bsp(&bsp, Some(&fs));
        let w = &world.collision;
        let p = Vec3::new(389.0, 575.0, -46.7);
        assert!(under_ground(w, p));
    }

    /// mp_carentan's graph from the real collision: every spawn point sits on
    /// it, and nearly all of them reach each other both ways.
    #[test]
    fn carentans_spawns_reach_each_other() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let entry = fs
            .resolve_map("mp_carentan")
            .expect("mp_carentan in the paks");
        let bsp = vcod_common::bsp::parse(&fs.read(&entry).unwrap()).unwrap();
        let world = crate::world::World::from_bsp(&bsp, Some(&fs));
        let g = graph_for("mp_carentan", &world);
        let on: Vec<u32> = world
            .spawn_points
            .iter()
            .filter_map(|s| g.nearest(*s))
            .collect();
        assert_eq!(on.len(), world.spawn_points.len(), "a spawn off the graph");
        let comp = g.components();
        let mut count = std::collections::BTreeMap::new();
        for n in &on {
            *count.entry(comp[*n as usize]).or_insert(0) += 1;
        }
        let most = count.values().max().copied().unwrap_or(0);
        assert!(
            most * 100 >= on.len() * 95,
            "only {most} of {} spawns in one component",
            on.len()
        );
    }

    /// A ladder whose head is right above its foot: the follower keeps the
    /// head as its waypoint up the climb, though the climb never closes in
    /// flat, and does not skip it for the next node from a floor below.
    /// A goal the graph does not reach is closed in on as far as it goes.
    #[test]
    fn a_path_toward_an_island_ends_at_the_node_nearest_it() {
        let g = NavGraph::from_parts(
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(32.0, 0.0, 0.0),
                Vec3::new(64.0, 0.0, 0.0),
                Vec3::new(200.0, 0.0, 0.0),
            ],
            vec![vec![1], vec![0, 2], vec![1], vec![]],
        );
        assert_eq!(g.path(0, 3), None);
        assert_eq!(g.path_toward(0, 3), Some(vec![0, 1, 2]));
        assert_eq!(g.path_toward(3, 0), None, "nowhere to go from the island");
    }

    /// Walking at a point past the path's end can drop the body a floor;
    /// from there the point is planned for again, not walked at blind.
    #[test]
    fn a_follower_that_falls_off_its_point_plans_again() {
        let g = NavGraph::from_parts(
            vec![
                Vec3::new(0.0, 0.0, 100.0),
                Vec3::new(32.0, 0.0, 100.0),
                Vec3::new(64.0, 0.0, 0.0),
                Vec3::new(32.0, 0.0, 0.0),
            ],
            vec![vec![1], vec![0, 2], vec![3, 1], vec![2]],
        );
        let mut f = Follower::default();
        let goal = crate::bots::Goal::To([40.0, 0.0, 120.0]);
        let mut budget = 1000;
        // Standing on the path's last node: walked out, head at the point.
        let at = [32.0, 0.0, 100.0];
        assert_eq!(
            f.waypoint(
                &g,
                None,
                goal,
                at,
                Footing::STANDING,
                &mut budget,
                &mut || 0
            ),
            Some([40.0, 0.0, 120.0])
        );
        assert_eq!(
            f.waypoint(
                &g,
                None,
                goal,
                at,
                Footing::STANDING,
                &mut budget,
                &mut || 0
            ),
            Some([40.0, 0.0, 120.0])
        );
        // Fell to the floor below: a new plan back up, its waypoints nodes.
        let below = [34.0, 0.0, 0.0];
        let before = budget;
        let w = f.waypoint(
            &g,
            None,
            goal,
            below,
            Footing::STANDING,
            &mut budget,
            &mut || 0,
        );
        assert!(budget < before, "planned again");
        assert!(
            w.is_some_and(|w| g.nodes.contains(&Vec3::from(w))),
            "still walking at the point: {w:?}"
        );
    }

    /// Floor, a ledge 16 up, a top 32 up, the floor beyond: from the ledge,
    /// beside the top and in reach of it flat, the bot still heads for the
    /// top, where the drop on was proved.
    #[test]
    fn a_node_a_drop_leaves_from_is_passed_only_on_it() {
        let g = NavGraph::from_parts(
            vec![
                Vec3::new(64.0, 0.0, -64.0),
                Vec3::new(32.0, 0.0, -48.0),
                Vec3::new(0.0, 0.0, -32.0),
                Vec3::new(-48.0, 0.0, -64.0),
            ],
            vec![vec![1], vec![0, 2], vec![1, 3], vec![]],
        );
        let mut f = Follower::default();
        let goal = crate::bots::Goal::To([-48.0, 0.0, -64.0]);
        let mut budget = 1000;
        let mut at = |p: [f32; 3]| {
            f.waypoint(
                &g,
                None,
                goal,
                p,
                Footing {
                    still: false,
                    on_ground: true,
                    on_ladder: false,
                    velocity: Vec3::ZERO,
                },
                &mut budget,
                &mut || 0,
            )
        };
        assert_eq!(at([64.0, 0.0, -64.0]), Some([32.0, 0.0, -48.0]));
        assert_eq!(at([12.0, 0.0, -48.0]), Some([0.0, 0.0, -32.0]));
        assert_eq!(at([2.0, 0.0, -32.0]), Some([-48.0, 0.0, -64.0]));
    }

    /// A body in the air beside a jump's top is nearer the next node than
    /// the top is, and still not on it.
    #[test]
    fn a_jumps_top_is_not_cut_past_in_the_air() {
        let mut g = NavGraph::from_parts(
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(32.0, 0.0, 36.0),
                Vec3::new(32.0, 64.0, 36.0),
            ],
            vec![vec![1], vec![0, 2], vec![1]],
        );
        g.jumps.insert((0, 1));
        let mut f = Follower::default();
        let goal = crate::bots::Goal::To([32.0, 64.0, 36.0]);
        let mut budget = 100;
        let mut at = |p: [f32; 3]| {
            f.waypoint(
                &g,
                None,
                goal,
                p,
                Footing {
                    still: false,
                    on_ground: true,
                    on_ladder: false,
                    velocity: Vec3::ZERO,
                },
                &mut budget,
                &mut || 0,
            )
        };
        assert_eq!(at([0.0, 0.0, 0.0]), Some([32.0, 0.0, 36.0]));
        // 25 flat from the top, 15 under it, mid-jump.
        assert_eq!(at([20.0, 22.0, 30.0]), Some([32.0, 0.0, 36.0]));
        assert_eq!(at([32.0, 2.0, 36.0]), Some([32.0, 64.0, 36.0]));
    }

    /// mp_ship's ledge at (3347, 250, 616): a bot passed the jump's top at
    /// 652 from 15 under it in the air, within reach flat, headed on for
    /// the floor at 664 and fell back to the deck. The top is passed only
    /// on the ground.
    #[test]
    fn a_jumps_top_is_passed_only_on_the_ground() {
        let mut g = NavGraph::from_parts(
            vec![
                Vec3::new(3361.3, 256.0, 616.125),
                Vec3::new(3366.2, 224.0, 652.125),
                Vec3::new(3328.2, 252.4, 664.125),
            ],
            vec![vec![1], vec![0, 2], vec![1]],
        );
        g.jumps.insert((0, 1));
        let mut f = Follower::default();
        let goal = crate::bots::Goal::To([3328.2, 252.4, 664.125]);
        let mut budget = 100;
        let top = Some([3366.2, 224.0, 652.125]);
        let mut at = |p: [f32; 3], on_ground: bool| {
            let body = Footing {
                still: false,
                on_ground,
                on_ladder: false,
                velocity: Vec3::ZERO,
            };
            f.waypoint(&g, None, goal, p, body, &mut budget, &mut || 0)
        };
        assert_eq!(at([3367.5, 239.1, 616.125], true), top);
        assert_eq!(at([3349.2, 239.1, 637.1], false), top, "in the air");
        assert_eq!(
            at([3366.0, 230.0, 652.125], true),
            Some([3328.2, 252.4, 664.125])
        );
    }

    /// mp_hurtgen's slope west of the crease at (437, 2824, -97): a body
    /// left at rest there, or on the slope above it where retail's
    /// `probe_wedge` slid into it, comes to rest in the air; on the patch
    /// of the node (428.2, 2783.2, -60.3) it stands. The run from the node
    /// (385.2, 2831.8, -22.2) down to (385.6, 2785.7, -52.6) arrives, but
    /// over spots that drain into the crease, so it proves no edge.
    #[test]
    fn a_slide_over_ground_that_drains_into_a_crease_does_not_hold() {
        let Some(world) = map_world("mp_hurtgen") else {
            return;
        };
        let w = &world.collision;
        let held = |x: f32, y: f32, z: f32| {
            held_off_ground(
                w,
                PlayerState::spawn(Vec3::new(x, y, z), 0.0),
                Sim::default(),
            )
        };
        assert!(held(436.8, 2824.3, -97.2));
        assert!(held(416.7, 2799.0, -60.0));
        assert!(held(392.1, 2797.3, -55.1));
        assert!(!held(428.2, 2783.2, -59.0));
        let node = |x: f32, y: f32, z: f32| settle(w, Vec3::new(x, y, z + 2.0), 0.0).unwrap();
        let (top, patch) = (node(385.2, 2831.8, -22.2), node(385.6, 2785.7, -52.6));
        let mut slid = Vec::new();
        let straight = walk_sliding(
            w,
            top,
            None,
            1.0,
            patch.truncate(),
            None,
            Gait::Forward,
            &mut slid,
        );
        assert!(matches!(straight, Walked::Arrived(..)) && !slid.is_empty());
        let walked = walk_with(w, top, None, patch.truncate(), None, Gait::Forward);
        assert!(!matches!(walked, Walked::Arrived(..)));
    }

    /// mp_depot's ladder at (-417, 1112) up to the roof at 133: a walk
    /// from its foot to the floor at (-484, 1051), 7 units below a bracket
    /// and a wall, climbed the rungs, crossed the roof and dropped back to
    /// the floor beside the target. A bot heading there pins in the corner
    /// at (-433, 1129, -26) instead.
    #[test]
    fn a_level_walk_over_a_ladders_top_proves_no_edge() {
        let Some(world) = map_world("mp_depot") else {
            return;
        };
        let w = &world.collision;
        let (foot, floor) = (
            Vec3::new(-416.5, 1130.0, -31.875),
            Vec3::new(-484.1, 1051.2, -31.875),
        );
        let walked = walk_with(
            w,
            foot,
            None,
            floor.truncate(),
            Some(floor.z),
            Gait::Forward,
        );
        assert!(!matches!(walked, Walked::Arrived(..)));
        // The same walk without a floor arrives on the roof, over it.
        let roof = walk_with(w, foot, None, floor.truncate(), None, Gait::Forward);
        assert!(matches!(roof, Walked::Arrived(p, _) if p.z > 100.0));
    }

    /// mp_hurtgen, seed 43: a bot slid down the terrain west of the crease
    /// at (437, 2824, -97), in the air from (379, 2807, -34) on, turned for
    /// the node after (381.7, 2790.8) 12 units past it and for the next
    /// 22 past that, never landing on either patch, and slid into the
    /// crease. A node level with the body is passed only on the ground.
    #[test]
    fn a_node_is_not_passed_sliding_past_it_in_the_air() {
        let g = NavGraph::from_parts(
            vec![
                Vec3::new(381.7, 2790.8, -53.6),
                Vec3::new(428.2, 2783.2, -60.3),
                Vec3::new(480.1, 2833.2, -123.2),
            ],
            vec![vec![1], vec![2], vec![]],
        );
        let mut f = Follower::default();
        let goal = crate::bots::Goal::To([480.1, 2833.2, -123.2]);
        let mut budget = 100;
        let mut at = |p: [f32; 3], on_ground: bool| {
            let body = Footing {
                still: false,
                on_ground,
                on_ladder: false,
                velocity: Vec3::ZERO,
            };
            f.waypoint(&g, None, goal, p, body, &mut budget, &mut || 0)
        };
        let slid = [392.1, 2797.3, -55.1];
        assert_eq!(at(slid, false), Some([381.7, 2790.8, -53.6]));
        assert_eq!(at(slid, true), Some([428.2, 2783.2, -60.3]));
    }

    /// mp_ship, seed 693: from (3365, -323, 352) the bot was nearer the
    /// ladder's foot (3370, -380, 56) than the head (3298, -380, 352) it
    /// backs down from, cut past the head and pinned on the wall at
    /// y -333 above the foot. A drop's top is passed only on reaching it.
    #[test]
    fn a_drops_top_is_not_cut_past() {
        let g = NavGraph::from_parts(
            vec![
                Vec3::new(3353.0, -321.0, 352.0),
                Vec3::new(3298.0, -380.0, 352.0),
                Vec3::new(3370.0, -380.0, 56.0),
            ],
            vec![vec![1], vec![2], vec![]],
        );
        let mut f = Follower::default();
        let goal = crate::bots::Goal::To([3370.0, -380.0, 56.0]);
        let mut budget = 100;
        let mut at =
            |p: [f32; 3]| f.waypoint(&g, None, goal, p, Footing::STANDING, &mut budget, &mut || 0);
        assert_eq!(at([3353.0, -321.0, 352.0]), Some([3298.0, -380.0, 352.0]));
        assert_eq!(at([3365.4, -324.2, 352.1]), Some([3298.0, -380.0, 352.0]));
    }

    /// mp_ship's hold ladder: a bot backing down the rungs passed the node
    /// of the floor at 304 beside it from 42 above, headed level for the
    /// next node and hung on the rungs under the floor's lip. On a ladder a
    /// node below is passed only from a step over its floor.
    #[test]
    fn a_node_below_the_rungs_is_passed_only_from_its_floor() {
        let g = NavGraph::from_parts(
            vec![
                Vec3::new(4349.5, -93.1, 600.125),
                Vec3::new(4318.5, -96.0, 304.125),
                Vec3::new(4280.5, -96.1, 304.125),
            ],
            vec![vec![1], vec![2], vec![1]],
        );
        let mut f = Follower::default();
        let goal = crate::bots::Goal::To([4280.5, -96.1, 304.125]);
        let mut budget = 100;
        let floor = Some([4318.5, -96.0, 304.125]);
        let mut at = |p: [f32; 3]| {
            let body = Footing {
                still: false,
                on_ground: false,
                on_ladder: true,
                velocity: Vec3::ZERO,
            };
            f.waypoint(&g, None, goal, p, body, &mut budget, &mut || 0)
        };
        assert_eq!(at([4349.5, -93.1, 600.125]), floor);
        assert_eq!(at([4328.5, -93.1, 346.3]), floor, "42 above");
        assert_eq!(at([4322.0, -93.1, 315.0]), Some([4280.5, -96.1, 304.125]));
    }

    /// mp_ship's hull beam: from its edge node a walk over the beam to the
    /// floor beyond arrives, from 16 units north along the edge it slides
    /// back off the slope. A bot that passed the node there stalled 120
    /// ticks.
    #[test]
    fn a_walk_on_proved_from_the_node_is_proved_again_from_the_body() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let w = &world.collision;
        let edge = Vec3::new(2247.1, 511.2, -47.875);
        let beyond = Vec3::new(2203.5, 511.4, -63.875);
        let g = NavGraph::from_parts(vec![edge, beyond], vec![vec![1], vec![0]]);
        let mut f = Follower::default();
        let goal = crate::bots::Goal::To(beyond.into());
        let mut budget = 100;
        // Running at the floor beyond, as a follower on the path does.
        let mut at = |p: Vec3| {
            let run = (beyond - p).truncate().normalize() * vcod_common::pmove::SPEED_RUN;
            f.waypoint(
                &g,
                Some(w),
                goal,
                p.into(),
                Footing {
                    still: false,
                    on_ground: true,
                    on_ladder: false,
                    velocity: run.extend(0.0),
                },
                &mut budget,
                &mut || 0,
            )
        };
        // In reach of the edge node along the ledge, but the walk on slides
        // back from here: close in on the node.
        let along = Vec3::new(2256.2498, 523.75616, -47.87189);
        assert!(!matches!(
            walk_as(w, along, beyond.truncate(), Some(beyond.z), Gait::Forward),
            Walked::Arrived(..)
        ));
        assert_eq!(at(along), Some(edge.into()));
        assert_eq!(at(edge), Some(beyond.into()));
    }

    /// mp_depot by (-64, 352): a body run off a ledge at z 12 comes to rest
    /// on a step's corner at z 3, right under a floor at z 27 and under a
    /// ceiling 37 over the feet. A flood walk resting there was merged into
    /// the floor's node until 2026-10-09; the walk to the node itself jumps.
    #[test]
    fn a_walk_jumps_onto_a_floor_right_overhead_under_a_ceiling() {
        let Some(world) = map_world("mp_depot") else {
            return;
        };
        let w = &world.collision;
        let from = Vec3::new(-28.519163, 352.00345, 12.125);
        let node = Vec3::new(-64.23049, 352.02036, 27.125);
        let free = walk_as(w, from, node.truncate(), None, Gait::Forward);
        assert!(matches!(free, Walked::Arrived(p, false) if (p.z - 3.125).abs() < 0.5));
        let on = walk_as(w, from, node.truncate(), Some(node.z), Gait::Forward);
        assert!(matches!(on, Walked::Arrived(p, true) if (p.z - node.z).abs() < 0.5));
    }

    #[test]
    fn a_follower_climbs_to_a_head_above_its_foot() {
        let g = NavGraph::from_parts(
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(10.0, 0.0, 200.0),
                Vec3::new(40.0, 0.0, 200.0),
            ],
            vec![vec![1], vec![0, 2], vec![1]],
        );
        let mut f = Follower::default();
        let goal = crate::bots::Goal::To([40.0, 0.0, 200.0]);
        let mut budget = 100;
        let mut at = [0.0, 0.0, 0.0];
        // 2.5 units a tick, the climb rate looking up, until the head's
        // floor is a step off: from further down the rungs the next node
        // is no way on.
        while at[2] <= 182.0 {
            let w = f.waypoint(
                &g,
                None,
                goal,
                at,
                Footing::STANDING,
                &mut budget,
                &mut || 0,
            );
            assert_eq!(w, Some([10.0, 0.0, 200.0]), "at z {}", at[2]);
            at[2] += 2.5;
        }
        at[0] = 10.0;
        at[2] = 200.0;
        // Level with it but still on the rungs: turning for the next node
        // there lets go of them (mp_ship's mast).
        let climbing = Footing {
            still: false,
            on_ground: false,
            on_ladder: true,
            velocity: Vec3::ZERO,
        };
        let w = f.waypoint(&g, None, goal, at, climbing, &mut budget, &mut || 0);
        assert_eq!(w, Some([10.0, 0.0, 200.0]), "on the rungs");
        let w = f.waypoint(
            &g,
            None,
            goal,
            at,
            Footing::STANDING,
            &mut budget,
            &mut || 0,
        );
        assert_eq!(w, Some([40.0, 0.0, 200.0]), "on the head's floor");
    }

    /// Two climbers abreast on the rungs, 30 apart: exactly one gives way,
    /// the lower, or of two level the later in (y, x) order. One a body's
    /// height above, or off to the side, or a bot not climbing, changes
    /// nothing (mp_ship's hatch over the ladder at x 3792, seed 417).
    #[test]
    fn of_two_climbers_abreast_one_gives_way() {
        let g = NavGraph::from_parts(
            vec![
                Vec3::new(3808.0, 65.0, 304.0),
                Vec3::new(3783.0, 64.0, 623.0),
            ],
            vec![vec![1], vec![]],
        );
        let f = Follower {
            path: vec![0, 1],
            next: 1,
            ..Follower::default()
        };
        let rungs = Footing {
            still: false,
            on_ground: false,
            on_ladder: true,
            velocity: Vec3::new(0.0, 0.0, 53.0),
        };
        let (a, b) = (
            Vec3::new(3808.1, 46.2, 600.0),
            Vec3::new(3808.1, 76.3, 600.0),
        );
        let up = Vec3::Z * 53.0;
        assert!(!f.abreast_yield(&g, a, rungs, &[(b, up)]), "first in y");
        assert!(f.abreast_yield(&g, b, rungs, &[(a, up)]), "gives way");
        let higher = b + Vec3::Z * 10.0;
        assert!(
            f.abreast_yield(&g, a, rungs, &[(higher, up)]),
            "lower gives way"
        );
        assert!(!f.abreast_yield(&g, higher, rungs, &[(a, up)]));
        let above = b + Vec3::Z * 80.0;
        assert!(
            !f.abreast_yield(&g, a, rungs, &[(above, up)]),
            "in a column"
        );
        let aside = b + Vec3::Y * 10.0;
        assert!(!f.abreast_yield(&g, a, rungs, &[(aside, up)]), "40 apart");
        assert!(
            !f.abreast_yield(&g, b, Footing::STANDING, &[(a, up)]),
            "off the rungs"
        );
        let down = Follower {
            path: vec![1, 0],
            next: 1,
            ..Follower::default()
        };
        assert!(
            !down.abreast_yield(&g, b, rungs, &[(a, up)]),
            "climbing down"
        );
    }

    /// A climb from a foot square in front of a ladder runs into the face
    /// until it is level with the head; from a foot off to the side it runs
    /// at the head, the way the walk that linked it went.
    #[test]
    fn a_climb_runs_into_the_face_from_a_foot_in_front_of_it() {
        let mut g = NavGraph::from_parts(
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(-40.0, 10.0, 200.0),
                Vec3::new(0.0, 40.0, 0.0),
            ],
            vec![vec![1], vec![0], vec![1]],
        );
        // A ladder plate 1 thick at x -17, 24 wide along y.
        g.ladder_boxes = vec![(Vec3::new(-18.0, -12.0, -8.0), Vec3::new(-17.0, 12.0, 200.0))];
        let goal = crate::bots::Goal::To([-40.0, 10.0, 200.0]);
        let mut f = Follower::default();
        let mut budget = 100;
        let w = |f: &mut Follower, budget: &mut u32, at: [f32; 3]| {
            f.waypoint(&g, None, goal, at, Footing::STANDING, budget, &mut || 0)
        };
        assert_eq!(w(&mut f, &mut budget, [0.0; 3]), Some([-17.0, 0.0, 200.0]));
        assert_eq!(
            w(&mut f, &mut budget, [-2.0, 0.0, 190.0]),
            Some([-40.0, 10.0, 200.0]),
            "level with the head"
        );
        let mut f = Follower::default();
        let at = [0.0, 40.0, 0.0];
        assert_eq!(w(&mut f, &mut budget, at), Some([-40.0, 10.0, 200.0]));
        assert_eq!(
            g.ladder_middle(Vec3::new(-2.0, 9.0, 100.0)),
            Some(glam::Vec2::new(-2.0, 0.0))
        );
    }

    #[test]
    fn a_creep_holds_its_first_heading() {
        let mut c = Creep::default();
        assert_eq!(back_move(100.0, false), -127);
        assert_eq!(c.hold(-127, 10.0), 10.0);
        assert_eq!(back_move(20.0, false), -CREEP);
        assert_eq!(c.hold(-CREEP, 20.0), 20.0);
        assert_eq!(c.hold(-CREEP, 200.0), 20.0, "turned round over the lip");
        assert_eq!(back_move(20.0, true), -127, "full speed down the ladder");
        assert_eq!(c.hold(-127, 30.0), 30.0);
        assert_eq!(c.hold(-CREEP, 40.0), 40.0, "a new creep, a new heading");
    }

    /// mp_ship's ladders, each from its open face: up and down both proved
    /// by the bots' own steering. One plain, one whose head stands right
    /// above its foot in a shaft, one only the creep gets down, and the five
    /// that got no rung before 2026-10-07. The climb under the mast's spar
    /// ends on the spar's edge at 1147 beside the plate, and the creep back
    /// off it at a run is 40 units out before it has dropped the 8 under the
    /// plate `PM_CheckLadderMove`'s box needs (2026-10-12): up only.
    #[test]
    fn ships_ladders_are_climbed_both_ways() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let Some(entry) = fs.resolve_map("mp_ship") else {
            return;
        };
        let bsp = vcod_common::bsp::parse(&fs.read(&entry).unwrap()).unwrap();
        let world = crate::world::World::from_bsp(&bsp, Some(&fs));
        let boxes = ladders(&world.collision);
        for (at, n, foot_z, head_z, want_down) in [
            ([5872.5, 47.0], Vec3::X, 352.125, 480.125, true),
            ([3711.5, 56.0], -Vec3::X, 760.125, 992.125, true),
            ([3792.5, 65.0], Vec3::X, 304.125, 616.125, true),
            // The hold's 560-unit ladder, the mast's under a spar, a crow's
            // nest: climbs past the old 10 s budget, or under a patch.
            ([4332.0, -77.5], -Vec3::Y, 56.125, 615.125, true),
            ([3675.0, -86.5], -Vec3::Y, 896.125, 1147.125, true),
            ([4975.5, 64.0], -Vec3::X, 692.125, 1219.125, true),
            // A brush running on below the floor, a wall crowding one edge.
            ([6487.0, 324.5], Vec3::Y, 80.125, 216.125, true),
            ([3600.5, -119.0], Vec3::X, 760.125, 896.125, true),
        ] {
            let &(lo, hi) = boxes
                .iter()
                .find(|(lo, hi)| (0..2).all(|i| lo[i] <= at[i] && at[i] <= hi[i]))
                .unwrap_or_else(|| panic!("no ladder at {at:?}"));
            let (foot, head, up, down) =
                rung(&world.collision, lo, hi, n).unwrap_or_else(|| panic!("{at:?}: no rung"));
            assert!((foot.z - foot_z).abs() < 1.0, "{at:?}: foot {foot}");
            assert!((head.z - head_z).abs() < 1.0, "{at:?}: head {head}");
            assert!(up && down == want_down, "{at:?}: up {up} down {down}");
            assert!(
                rung(&world.collision, lo, hi, -n).is_none(),
                "{at:?}: the back"
            );
        }
    }

    /// mp_hurtgen's minefields ring the axis spawn: a bot that wandered off
    /// it died about every 12 s and ended the Retrieval round.
    #[test]
    fn hurtgens_minefields_are_hazards() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let Some(entry) = fs.resolve_map("mp_hurtgen") else {
            return;
        };
        let bsp = vcod_common::bsp::parse(&fs.read(&entry).unwrap()).unwrap();
        let world = crate::world::World::from_bsp(&bsp, Some(&fs));
        assert!(
            hazard(&world.hazards, Vec3::new(6198.0, -302.0, 188.0)),
            "where the bot died"
        );
        assert!(
            !hazard(&world.hazards, Vec3::new(6304.0, 296.0, 96.125)),
            "the spawn beside it"
        );
    }

    /// A jump edge stays a jump until a walk proves it without one, and a
    /// jump one way is no proof of a plain walk the other.
    #[test]
    fn a_plain_walk_clears_a_jump_edge() {
        let mut g = row();
        g.link_as(0, 2, true);
        assert!(g.jumps.contains(&(0, 2)));
        g.link_as(0, 2, true);
        assert!(g.jumps.contains(&(0, 2)), "a second jump keeps it a jump");
        g.link_as(0, 2, false);
        assert!(!g.jumps.contains(&(0, 2)));
        g.link_as(2, 4, false);
        g.link_as(2, 4, true);
        assert!(!g.jumps.contains(&(2, 4)), "a plain edge stays plain");
    }

    #[test]
    fn a_leap_jumps_only_at_the_lip() {
        // flat, rise: the top 40 off and 32 up.
        assert!(!jump_cue(true, false, 0.0, 40.0, 32.0), "pinned is no cue");
        assert!(jump_cue(true, true, 9.5, 40.0, 32.0));
        // A jump onto a ledge: pinned at its face, or right under the top.
        assert!(jump_cue(false, false, 0.5, 30.0, 32.0));
        assert!(
            !jump_cue(false, false, 9.5, 30.0, 32.0),
            "still running at it"
        );
        assert!(jump_cue(false, false, 9.5, 10.0, 32.0));
        assert!(!jump_cue(false, false, 9.5, 10.0, 10.0), "under a step");
        // On the top: no more jumping.
        assert!(!jump_cue(false, false, 0.0, 10.0, 2.0));
        assert!(!jump_cue(true, true, 0.0, 10.0, 2.0));
        assert_eq!(jump_key(true, true, false), 127);
        assert_eq!(jump_key(true, true, true), 0, "a held key is refused");
        assert_eq!(jump_key(false, true, false), 0);
    }

    /// A leap's foot is held until the bot stands still on it, then the
    /// top is the waypoint, flagged a leap.
    #[test]
    fn a_follower_comes_to_rest_at_a_leaps_foot() {
        let mut g = NavGraph::from_parts(
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(32.0, 0.0, 0.0),
                Vec3::new(96.0, 0.0, 32.0),
            ],
            vec![vec![1], vec![0, 2], vec![1]],
        );
        g.jumps.insert((1, 2));
        g.leaps.insert((1, 2));
        let mut f = Follower::default();
        let goal = crate::bots::Goal::To([96.0, 0.0, 32.0]);
        let mut budget = 100;
        let foot = Some([32.0, 0.0, 0.0]);
        let at = [30.0, 0.0, 0.0];
        assert_eq!(
            f.waypoint(
                &g,
                None,
                goal,
                at,
                Footing {
                    still: false,
                    on_ground: true,
                    on_ladder: false,
                    velocity: Vec3::ZERO,
                },
                &mut budget,
                &mut || 0
            ),
            foot
        );
        assert!(f.holding(&g) && !f.jumping(&g));
        // Running past the foot, nearer the top than the foot is: held.
        let at = [40.0, 0.0, 0.0];
        assert_eq!(
            f.waypoint(
                &g,
                None,
                goal,
                at,
                Footing {
                    still: false,
                    on_ground: true,
                    on_ladder: false,
                    velocity: Vec3::ZERO,
                },
                &mut budget,
                &mut || 0
            ),
            foot
        );
        let at = [33.0, 0.0, 0.0];
        assert_eq!(
            f.waypoint(
                &g,
                None,
                goal,
                at,
                Footing::STANDING,
                &mut budget,
                &mut || 0
            ),
            Some([96.0, 0.0, 32.0])
        );
        assert!(f.jumping(&g) && f.leaping(&g) && !f.holding(&g));
    }

    /// A roam picks among the nodes its start reaches both ways: two rows
    /// with no edge between them, and a pick that would land in the other.
    #[test]
    fn a_roam_stays_in_its_component() {
        let nodes = (0..6)
            .map(|i| Vec3::new(i as f32 * 32.0, (i / 3) as f32 * 512.0, 0.0))
            .collect();
        let edges = vec![vec![1], vec![0, 2], vec![1], vec![4], vec![3, 5], vec![4]];
        let g = NavGraph::from_parts(nodes, edges);
        let mut picks = [4, 5].into_iter();
        let mut rand = || picks.next().unwrap_or(0);
        let d = g.roam_from(0, Vec3::ZERO, &mut rand);
        assert!(d.is_some_and(|d| d < 3), "picked {d:?}");
    }

    /// From an island whose only ways off are drops, a roam picks in the
    /// largest component below; from there it never picks the pit the
    /// largest one drops into.
    #[test]
    fn a_roam_leaves_an_island_and_keeps_out_of_a_pit() {
        // 0-1 island, drops to 2; 2-5 a row; 5 drops to the pit 6.
        let nodes = (0..7)
            .map(|i| Vec3::new(i as f32 * 32.0, 0.0, 0.0))
            .collect();
        let edges = vec![
            vec![1],
            vec![0, 2],
            vec![3],
            vec![2, 4],
            vec![3, 5],
            vec![4, 6],
            vec![],
        ];
        let g = NavGraph::from_parts(nodes, edges);
        let mut picks = (0..8).map(|i| i % 2);
        let d = g.roam_from(0, Vec3::ZERO, &mut || picks.next().unwrap());
        assert!(d.is_some_and(|d| (2..6).contains(&d)), "picked {d:?}");
        for i in 0..4 {
            let d = g.roam_from(5, Vec3::ZERO, &mut || i);
            assert!(d.is_some_and(|d| (2..6).contains(&d)), "picked {d:?}");
        }
    }

    fn map_world(map: &str) -> Option<crate::world::World> {
        let fs = vcod_common::testing::game_fs()?;
        let entry = fs.resolve_map(map)?;
        let bsp = vcod_common::bsp::parse(&fs.read(&entry).unwrap()).unwrap();
        Some(crate::world::World::from_bsp(&bsp, Some(&fs)))
    }

    /// mp_ship, seed 5: a bot against the slanted wall at (6111, -304, 97)
    /// headed south slid along it and off the ledge 41 units to the deck.
    /// The sweep due south touches the wall at once; the slide finds it.
    /// mp_ship's hull slope north of (3762, 339, 352): no sample a jump
    /// under the one before, but 49 down within 75 units, to a wedge off
    /// the graph at (3752, 414, 303) that a seed 5 unstick heading ran a
    /// bot into.
    #[test]
    fn a_drop_ahead_is_felt_down_a_steep_slope() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let at = Vec3::new(3762.4, 338.9, 352.125);
        let depth = drop_ahead(&world.collision, at, Vec3::Y, 64.0, &mut Creases::default());
        assert!(depth.is_some_and(|d| d > 39.0), "{depth:?}");
    }

    #[test]
    fn a_drop_ahead_is_felt_along_a_wall_it_slides_on() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let at = Vec3::new(6095.754, -303.89618, 97.125);
        let depth = drop_ahead(
            &world.collision,
            at,
            -Vec3::Y,
            64.0,
            &mut Creases::default(),
        );
        assert!(depth.is_some_and(|d| (d - 41.0).abs() < 1.0), "{depth:?}");
        assert_eq!(
            drop_ahead(&world.collision, at, Vec3::Y, 64.0, &mut Creases::default()),
            None
        );
    }

    /// mp_hurtgen, seed 67: an unstick heading north-east from
    /// (411.7, 2786, -57.7) ran a bot down the slope into the crease at
    /// (437, 2824, -97). The slope falls under a jump within the look, so
    /// it reads as a drop only through the crease it drains into.
    #[test]
    fn a_slope_draining_into_a_crease_reads_as_a_drop_that_hurts() {
        let Some(world) = map_world("mp_hurtgen") else {
            return;
        };
        let at = Vec3::new(411.7, 2786.0, -57.7);
        let ne = Vec3::new(1.0, 1.0, 0.0).normalize();
        let mut creases = Creases::default();
        let depth = drop_ahead(&world.collision, at, ne, 64.0, &mut creases);
        assert_eq!(depth, Some(f32::INFINITY));
        // Down the slope west of its foot there is no crease.
        let w = drop_ahead(&world.collision, at, -Vec3::X, 64.0, &mut creases);
        assert_eq!(w, None);
    }

    /// mp_ship's deck lip at (3285.5, -448.5, 56.125), where seed 5's
    /// unstick heading walked a bot off to the hull floor at -64: the deck
    /// runs on east and north, the drop lies west and south.
    #[test]
    fn a_drop_ahead_is_felt_off_a_decks_lip() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let lip = Vec3::new(3285.5413, -448.488, 56.125);
        let drops: Vec<bool> = (0..8)
            .map(|i| {
                let (s, c) = (i as f32 * 45.0f32).to_radians().sin_cos();
                drop_ahead(
                    &world.collision,
                    lip,
                    Vec3::new(c, s, 0.0),
                    64.0,
                    &mut Creases::default(),
                )
                .is_some()
            })
            .collect();
        assert_eq!((drops[0], drops[2]), (false, false), "{drops:?}");
        assert_eq!((drops[4], drops[6]), (true, true), "{drops:?}");
    }

    /// mp_depot's documents lie on a crate top at z 148 across a gap from a
    /// step at 116: a run off the step falls, a standing-start leap from it
    /// lands on the crate.
    #[test]
    fn depots_crate_takes_a_leap() {
        let Some(world) = map_world("mp_depot") else {
            return;
        };
        let w = &world.collision;
        let step = settle(w, Vec3::new(-1820.0, -609.0, 120.0), 0.0).expect("the step");
        assert!((step.z - 116.125).abs() < 0.5, "step at {step}");
        let top = glam::Vec2::new(-1888.0, -537.0);
        assert!(matches!(
            walk_as(w, step, top, Some(148.125), Gait::Forward),
            Walked::Fell
        ));
        assert!(matches!(
            walk_as(w, step, top, Some(148.125), Gait::Leap),
            Walked::Arrived(p, true) if (p.z - 148.125).abs() < 0.5
        ));
    }

    /// mp_depot, seed 7: a bot with a carbine (move scale over 1) at rest
    /// by the leap's foot on the step, running for the crate, covered 8.9
    /// units in the tick after the lip read clear off its pace of 5.1, and
    /// walked off the step's edge without the jump. The lip is the tick's
    /// move: it reads on the last tick on the ground, whatever the speed.
    #[test]
    fn the_lip_reads_on_the_last_tick_on_the_ground() {
        let Some(world) = map_world("mp_depot") else {
            return;
        };
        let fs = vcod_common::testing::game_fs().unwrap();
        let weapons = crate::weapons::WeaponTable::load(&fs);
        let w = &world.collision;
        let target = glam::Vec2::new(-1856.1, -538.4);
        let mut ps = PlayerState::spawn(Vec3::new(-1779.3, -607.2, 116.125), 0.0);
        ps.weapon = crate::configstrings::weapon_index("m1carbine_mp").unwrap() as u8;
        let mw = MoveWorld::bare(w);
        let mut last = None;
        for tick in 1..20 {
            let lip = lip_ahead(w, &ps, target, weapons.defs());
            let was = ps.on_ground;
            let to = target - ps.origin.truncate();
            let cmd = UserCmd {
                server_time: tick * FRAME_MS,
                forward: 127,
                angles: [0, (to.y.atan2(to.x).to_degrees() * ANGLE2SHORT) as i32, 0],
                ..NULL_USERCMD
            };
            let dt = FRAME_MS as f32 / 1000.0;
            player_step(
                &mut ps,
                &mut [0; 3],
                &mut EventRing::default(),
                &cmd,
                dt,
                &mw,
                weapons.defs(),
            );
            if tick > 1 && was && !ps.on_ground {
                last = Some(lip);
                break;
            }
        }
        assert_eq!(
            last,
            Some(true),
            "the run leaves the step's edge, and the lip reads first"
        );
    }

    /// mp_ship, seed 245: the drop from the crow's nest's top at
    /// (4928.5, 60.8, 1254) to the spar at 1212 had been avoided, and with
    /// no way round it the plan took it again. Passed as it was at the
    /// pace of a turn onto the node, the run flew past the spar to the
    /// deck at 400. It is held: the body comes to rest on the node first.
    #[test]
    fn an_edge_with_no_way_round_is_taken_from_rest() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let top = Vec3::new(4928.5, 60.8, 1254.125);
        let spar = Vec3::new(4888.3, 64.6, 1211.65);
        let g = NavGraph::from_parts(vec![top, spar], vec![vec![1], vec![]]);
        let mut f = Follower {
            path: vec![0, 1],
            avoid: vec![(0, 1, u32::MAX)],
            ..Follower::default()
        };
        let mut body = Footing {
            still: false,
            on_ground: true,
            on_ladder: false,
            velocity: Vec3::new(-118.0, 120.0, 0.0),
        };
        let at = [4940.6, 48.3, 1254.125];
        let mut budget = 100;
        let mut rand = || 0;
        let mut wp = |f: &mut Follower, at: [f32; 3], body: Footing| {
            f.waypoint(
                &g,
                Some(&world.collision),
                crate::bots::Goal::Roam,
                at,
                body,
                &mut budget,
                &mut rand,
            )
        };
        assert_eq!(
            wp(&mut f, at, body),
            Some(top.into()),
            "held at the turn's pace"
        );
        assert!(f.holding(&g));
        body.velocity = Vec3::ZERO;
        body.still = true;
        assert_eq!(
            wp(&mut f, (top + Vec3::new(3.0, -2.0, 0.0)).into(), body),
            Some(spar.into()),
            "from rest"
        );
    }

    /// mp_depot, seed 9: a bot jumped onto the crate at (-2275, 131, 46)
    /// and ran on for the open crate's rim at 66, 20 over it, with its
    /// jump still cooling down. The jump under the rim was refused and the
    /// run went off the crate's edge into the open crate at 23, where the
    /// rim is out of a jump's reach. The bot waits on the foot until the
    /// jump is ready, as the graph's walk had it.
    #[test]
    fn a_jump_edge_waits_on_its_foot_for_the_jump() {
        let Some(world) = map_world("mp_depot") else {
            return;
        };
        let w = &world.collision;
        let mw = MoveWorld::bare(w);
        let foot = Vec3::new(-2275.2, 131.3, 46.125);
        let top = Vec3::new(-2277.2, 95.5, 66.125);
        // The run from the foot with the jump key on the cue under the top.
        let run = |since_jump_ms: f32| {
            let mut ps = PlayerState::spawn(foot, 0.0);
            ps.since_jump_ms = since_jump_ms;
            ps.velocity = ((top - foot).truncate().normalize() * 190.0).extend(0.0);
            for tick in 1..30 {
                let to = top.truncate() - ps.origin.truncate();
                let cue = jump_cue(false, false, 9.5, to.length(), top.z - ps.origin.z);
                let cmd = UserCmd {
                    server_time: tick * FRAME_MS,
                    forward: 127,
                    up: jump_key(ps.on_ground, cue, false),
                    angles: [0, (to.y.atan2(to.x).to_degrees() * ANGLE2SHORT) as i32, 0],
                    ..NULL_USERCMD
                };
                let dt = FRAME_MS as f32 / 1000.0;
                player_step(
                    &mut ps,
                    &mut [0; 3],
                    &mut EventRing::default(),
                    &cmd,
                    dt,
                    &mw,
                    &[],
                );
                if ps.on_ground && (ps.origin.z - top.z).abs() < 1.0 {
                    return true;
                }
            }
            false
        };
        assert!(run(f32::INFINITY), "a jump to hand lands on the rim");
        assert!(!run(150.0), "a jump refused runs off the crate");
        let mut g = NavGraph::from_parts(vec![foot, top], vec![vec![1], vec![]]);
        g.jumps.insert((0, 1));
        let f = Follower {
            path: vec![0, 1],
            next: 1,
            ..Follower::default()
        };
        let mut ps = PlayerState::spawn(foot + Vec3::new(0.0, -6.0, 0.0), 0.0);
        ps.since_jump_ms = 150.0;
        ps.on_ground = true;
        let at = ps.origin.into();
        assert_eq!(f.jump_wait(&g, at, &ps), Some(foot.into()));
        ps.since_jump_ms = vcod_common::pmove::JUMP_COOLDOWN_MS;
        assert_eq!(f.jump_wait(&g, at, &ps), None, "ready");
    }

    /// mp_ship's ladders at x 3354 (y 496-520) and x 3369 (y -368 to -379),
    /// climbed on their east side: a body on the rungs is nearest a floor
    /// behind its back, at 128 or 320, which no ladder move reaches. A plan
    /// from the rungs starts at the ladder's foot or head instead.
    #[test]
    fn a_plan_from_the_rungs_starts_at_a_ladders_end() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let g = NavGraph::build(&world.collision, &world.spawn_points, &world.hazards);
        let near = |n: Option<u32>, at: [f32; 3]| {
            n.is_some_and(|n| g.nodes[n as usize].distance(Vec3::from(at)) < 1.0)
        };
        let low = Vec3::new(3369.1, 495.1, 154.7);
        assert!(
            near(g.nearest(low.into()), [3426.9, 545.6, 127.7]),
            "behind the climber"
        );
        assert!(
            near(g.nearest_ladder_end(low), [3370.0, 508.0, 56.125]),
            "the foot"
        );
        let high = Vec3::new(3369.2, -379.1, 300.0);
        assert!(
            near(g.nearest(high.into()), [3399.1, -380.5, 320.125]),
            "behind the climber"
        );
        assert!(
            near(g.nearest_ladder_end(high), [3358.6, -384.2, 352.125]),
            "a head"
        );
    }

    /// mp_ship's slot between the stair's top and the shelf: the graph's
    /// walk from the shelf node, a running start straight at the floor
    /// 192 under it, lands. A bot that comes along the shelf at a run and
    /// turns onto the drop falls past the stair: the node is held while the
    /// walk at the body's pace fails, and passed from at rest on it.
    #[test]
    fn a_drop_is_proved_again_at_the_pace_the_bot_comes_onto_it() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let w = &world.collision;
        let shelf = Vec3::new(5402.5635, -160.24321, 396.125);
        let slot = Vec3::new(5404.874, -192.03677, 204.125);
        assert!(matches!(
            walk_as(w, shelf, slot.truncate(), Some(slot.z), Gait::Forward),
            Walked::Arrived(_, false)
        ));
        let g = NavGraph::from_parts(vec![shelf, slot], vec![vec![1], vec![]]);
        let mut f = Follower::default();
        let goal = crate::bots::Goal::To(slot.into());
        let mut budget = 100;
        let mut at = |p: Vec3, velocity: Vec3| {
            let body = Footing {
                still: velocity == Vec3::ZERO,
                on_ground: true,
                on_ladder: false,
                velocity,
            };
            f.waypoint(&g, Some(w), goal, p.into(), body, &mut budget, &mut || 0)
        };
        let run = Vec3::new(-218.0, -2.0, 0.0);
        assert_eq!(
            at(Vec3::new(5417.173, -160.85359, 396.125), run),
            Some(shelf.into())
        );
        assert_eq!(
            at(shelf, Vec3::ZERO),
            Some(slot.into()),
            "from at rest on the node"
        );
    }

    /// mp_depot's thin ledge (-2749, -1281, 326), 182 under the roof's node
    /// (-2750, -1244, 508) by the ladder at x -2750: only a creep from the
    /// node lands on it. Seed 183's bot came onto it off a turn, running
    /// west at 148 units/s, and fell past the ledge to the yard. The node is
    /// held until the body is under [`CREEP_TOP_PACE`].
    #[test]
    fn a_creeps_top_is_passed_only_slow() {
        let Some(world) = map_world("mp_depot") else {
            return;
        };
        let w = &world.collision;
        let roof = Vec3::new(-2750.1372, -1244.103, 508.125);
        let ledge = Vec3::new(-2748.646, -1280.8044, 326.125);
        let to = ledge.truncate();
        for gait in [Gait::Forward, Gait::Backward] {
            assert!(matches!(
                walk_as(w, roof, to, Some(ledge.z), gait),
                Walked::Fell
            ));
        }
        for pace in [None, Some(Vec3::ZERO)] {
            assert!(matches!(
                walk_with(w, roof, pace, to, Some(ledge.z), Gait::Creep),
                Walked::Arrived(..)
            ));
        }
        let g = NavGraph::from_parts(vec![roof, ledge], vec![vec![1], vec![]]);
        let mut f = Follower::default();
        let goal = crate::bots::Goal::To(ledge.into());
        let mut budget = 100;
        let mut at = |f: &mut Follower, p: Vec3, velocity: Vec3| {
            let body = Footing {
                still: velocity == Vec3::ZERO,
                on_ground: true,
                on_ladder: false,
                velocity,
            };
            let wp = f.waypoint(&g, Some(w), goal, p.into(), body, &mut budget, &mut || 0);
            (wp, f.holding(&g))
        };
        let seed_183 = Vec3::new(-2742.7, -1246.2, 508.125);
        assert_eq!(
            at(&mut f, seed_183, Vec3::new(-148.0, -16.0, 0.0)),
            (Some(roof.into()), true),
            "comes to rest on the node"
        );
        assert_eq!(
            at(&mut f, roof, Vec3::ZERO),
            (Some(ledge.into()), false),
            "from at rest on the node"
        );
        assert_eq!(
            at(
                &mut Follower::default(),
                seed_183,
                Vec3::new(-60.0, -10.0, 0.0)
            ),
            (Some(ledge.into()), false),
            "slow enough"
        );
    }

    /// mp_ship's ledge at (3685.5, -354.3, 400.1), 211 under the deck
    /// node (3647, -352.9, 611.1) west of it and narrow over the hold at
    /// 92: a run off the node misses it, a back down lands. A bot that came
    /// along the deck from the north and turned onto it carried its pace
    /// south over the ledge and died in the hold; the node is held and
    /// passed from at rest on it.
    #[test]
    fn a_backed_drop_is_proved_again_at_the_pace_the_bot_comes_onto_it() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let w = &world.collision;
        let deck = Vec3::new(3647.0, -352.9, 611.125);
        let ledge = Vec3::new(3685.5, -354.3, 400.125);
        let to = ledge.truncate();
        assert!(matches!(
            walk_as(w, deck, to, Some(ledge.z), Gait::Forward),
            Walked::Fell
        ));
        let g = NavGraph::from_parts(vec![deck, ledge], vec![vec![1], vec![]]);
        let goal = crate::bots::Goal::To(ledge.into());
        let mut budget = 100;
        let mut at = |f: &mut Follower, p: Vec3, velocity: Vec3| {
            let body = Footing {
                still: velocity == Vec3::ZERO,
                on_ground: true,
                on_ladder: false,
                velocity,
            };
            let wp = f.waypoint(&g, Some(w), goal, p.into(), body, &mut budget, &mut || 0);
            (wp, f.holding(&g))
        };
        let mut f = Follower::default();
        assert_eq!(
            at(&mut f, deck, Vec3::new(0.0, -190.0, 0.0)),
            (Some(deck.into()), true),
            "coming along the deck from the north"
        );
        assert_eq!(
            at(&mut f, deck, Vec3::ZERO),
            (Some(ledge.into()), false),
            "from at rest on the node"
        );
    }

    /// mp_ship's deck at (3647.9, -316.9, 618.5): the flood's back down
    /// toward the ledge column's centre (3680, -352) landed beside the
    /// node already there at (3685.5, -354.3, 400.1), and no walk from the
    /// one to the other lands; bots that took the edge died in the hold
    /// under it. A drop's rest beside a node off its column's centre is
    /// walked to the node itself.
    #[test]
    fn a_drop_beside_a_node_off_its_columns_centre_is_walked_to_the_node() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let g = graph_for("mp_ship", &world);
        let node = |p: [f32; 3]| {
            g.nearest(p)
                .filter(|&n| g.nodes[n as usize].distance(Vec3::from(p)) < 1.0)
                .unwrap_or_else(|| panic!("no node at {p:?}"))
        };
        let ledge = node([3685.5, -354.3, 400.125]);
        let north = node([3647.9, -316.9, 618.5]);
        let west = node([3647.0, -352.9, 611.125]);
        assert!(!g.edges[north as usize].contains(&ledge), "from the north");
        assert!(g.edges[west as usize].contains(&ledge), "from the west");
        assert!(
            g.risky.contains(&(west, ledge)),
            "missed from 4 units south"
        );
    }

    /// A plan takes a risky drop only where there is no other way.
    #[test]
    fn a_plan_goes_round_a_risky_drop() {
        let mut g = NavGraph::from_parts(
            vec![
                Vec3::new(0.0, 0.0, 200.0),
                Vec3::new(64.0, 0.0, 0.0),
                Vec3::new(0.0, 64.0, 200.0),
                Vec3::new(64.0, 64.0, 0.0),
            ],
            vec![vec![1, 2], vec![], vec![3], vec![1]],
        );
        assert_eq!(g.path(0, 1), Some(vec![0, 1]));
        g.risky.insert((0, 1));
        assert_eq!(g.path(0, 1), Some(vec![0, 2, 3, 1]), "round it");
        g.edges[2].clear();
        assert_eq!(g.path(0, 1), Some(vec![0, 1]), "the only way");
    }

    /// mp_ship's mast: the head (3675, -95.1, 1147.1) stands on the spar
    /// beside the rungs' top, and only a creep from rest gets back onto
    /// them. A bot that comes onto the head at a run is held there until it
    /// is slow.
    #[test]
    fn a_creep_down_proved_only_from_rest_is_passed_only_slow() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let w = &world.collision;
        let head = Vec3::new(3675.0, -95.13204, 1147.125);
        let foot = Vec3::new(3675.0, -103.0, 896.125);
        let g = NavGraph::from_parts(vec![head, foot], vec![vec![1], vec![]]);
        let goal = crate::bots::Goal::To(foot.into());
        let mut budget = 100;
        let mut at = |f: &mut Follower, p: Vec3, velocity: Vec3| {
            let body = Footing {
                still: velocity == Vec3::ZERO,
                on_ground: true,
                on_ladder: false,
                velocity,
            };
            let wp = f.waypoint(&g, Some(w), goal, p.into(), body, &mut budget, &mut || 0);
            (wp, f.holding(&g))
        };
        let mut f = Follower::default();
        let along = head + Vec3::new(-10.0, 0.0, 0.0);
        assert_eq!(
            at(&mut f, along, Vec3::new(190.0, 0.0, 0.0)),
            (Some(head.into()), true),
            "along the spar at a run"
        );
        assert_eq!(
            at(&mut f, head, Vec3::ZERO),
            (Some(foot.into()), false),
            "from at rest on the head"
        );
    }

    /// mp_depot's roof peak (-2660, -1216, 552): a run east over it flies
    /// off the crest and lands 67 units on at 190 units/s, on the narrow
    /// flat at 508 inside the roof's edge at x -2570. Seed 45's bot held a
    /// weapon at 1.16, came over at 220 and flew past the edge to the yard.
    /// The walk at its weapon's scale falls: the peak is held, and from at
    /// rest on it, where the same walk falls again, the edge is rounded.
    #[test]
    fn a_run_over_a_crest_too_fast_for_the_weapon_is_held_at_the_top() {
        let Some(world) = map_world("mp_depot") else {
            return;
        };
        let w = &world.collision;
        let peak = Vec3::new(-2660.245, -1215.9744, 552.13025);
        let east = Vec3::new(-2626.9204, -1215.9722, 531.5182);
        let g = NavGraph::from_parts(vec![peak, east], vec![vec![1], vec![]]);
        let goal = crate::bots::Goal::To(east.into());
        let mut budget = 100;
        let mut at = |f: &mut Follower, p: Vec3, velocity: Vec3| {
            let body = Footing {
                still: velocity == Vec3::ZERO,
                on_ground: true,
                on_ladder: false,
                velocity,
            };
            let wp = f.waypoint(&g, Some(w), goal, p.into(), body, &mut budget, &mut || 0);
            (wp, f.holding(&g))
        };
        let scaled = |move_scale: f32| Follower {
            move_scale,
            ..Follower::default()
        };
        let seed_45 = Vec3::new(-2663.9, -1216.1, 552.125);
        // Where seed 45's bot, held on the peak, came to rest: 6 short.
        let rest = Vec3::new(-2666.2, -1215.7, 552.125);
        assert_eq!(
            at(&mut scaled(1.0), seed_45, Vec3::new(190.0, 0.0, 0.0)),
            (Some(east.into()), false),
            "at the walk's own pace"
        );
        let mut f = scaled(1.16);
        assert_eq!(
            at(&mut f, seed_45, Vec3::new(220.0, 0.0, 0.0)),
            (Some(peak.into()), true),
            "with a light weapon"
        );
        assert_eq!(
            at(&mut f, rest, Vec3::ZERO),
            (None, false),
            "flung from at rest on the peak too: round it"
        );
        assert!(f.avoid.iter().any(|e| (e.0, e.1) == (0, 1)));
        assert_eq!(
            at(&mut scaled(1.0), rest, Vec3::ZERO),
            (Some(east.into()), false),
            "at the walk's own scale from rest"
        );
    }

    /// mp_ship: a run down the steep stair to the hull's deck at z 104 is in
    /// the air past its run time (the walk lets it land), and the spawns
    /// in a lifeboat at z 394 get out over its 40-unit gunwale only with a
    /// jump.
    #[test]
    fn ships_hull_and_lifeboat_walks() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let w = &world.collision;
        let stair = settle(w, Vec3::new(3430.0, 288.0, 170.0), 0.0).expect("the stair");
        let boxes = ladders(w);
        let down = walk(w, &boxes, stair, glam::Vec2::new(3456.0, 288.0));
        assert!(
            down.is_some_and(|(p, jumped)| p.z < 110.0 && !jumped),
            "down the stair: {down:?}"
        );
        let boat = settle(w, Vec3::new(4224.0, -376.0, 440.0), 0.0).expect("the gunwale");
        let out = walk(w, &boxes, boat, glam::Vec2::new(4192.0, -384.0));
        assert!(
            out.is_some_and(|(p, jumped)| p.z < 360.0 && jumped),
            "out of the boat: {out:?}"
        );
    }

    /// mp_ship's hull corridor at (3825, -800, -64): a beam's end stands
    /// across the line from 3 units north of the node at (3837, -805). A
    /// run from there pins on it and arrives only by jumping, which a bot
    /// does only on a jump edge, so the edge does not hold from there.
    #[test]
    fn an_edge_that_needs_a_jump_from_aside_does_not_hold() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let w = &world.collision;
        let a = Vec3::new(3837.1023, -804.9357, -63.875);
        let b = Vec3::new(3879.2532, -799.9733, -63.875);
        let mut g = NavGraph::from_parts(vec![a, b], vec![vec![1], vec![]]);
        let mut f = Follower {
            path: vec![0, 1],
            ..Follower::default()
        };
        let aside = Vec3::new(3822.0, -802.0, -63.875);
        assert!(f.edge_holds(Some(w), a, Vec3::ZERO, &g), "from the node");
        assert!(
            !f.edge_holds(Some(w), aside, Vec3::ZERO, &g),
            "3 units north"
        );
        g.jumps.insert((0, 1));
        assert!(
            f.edge_holds(Some(w), aside, Vec3::ZERO, &g),
            "on a jump edge"
        );
    }

    /// mp_ship's mast hatch: the climb's head stands on the ladder plate's
    /// 1-unit top over the hole, and the hatch's floor lies either side of
    /// the plate along the face. The mast's own top has none.
    #[test]
    fn a_head_perched_on_a_ladder_plate_exits_along_the_face() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let w = &world.collision;
        let hatch = Vec3::new(3704.8699, 56.0, 992.125);
        assert!(perched(w, hatch));
        let exits = ladder_exits(w, hatch, -Vec3::X);
        assert_eq!(exits.len(), 2, "{exits:?}");
        for (p, back) in exits {
            assert!(back && footed(w, p) && (p.z - hatch.z).abs() < 1.0, "{p}");
            assert!((p.y - hatch.y).abs() > 24.0, "{p}");
        }
        let mast = Vec3::new(3675.0, -101.12528, 1145.126);
        assert!(perched(w, mast) && ladder_exits(w, mast, -Vec3::Y).is_empty());
        // A head on a deck is no perch.
        let deck = Vec3::new(3751.1277, 65.0, 616.125);
        assert!(!perched(w, deck) && ladder_exits(w, deck, Vec3::X).is_empty());
    }

    /// mp_ship's hold ladder: its head stands on a lip over the shaft. A
    /// leap from the head to the ledge at 641 arrives, but a climb tips the
    /// body onto the lip 4 units south of it, and from there the leap
    /// falls. A ladder's head is no leap's foot.
    #[test]
    fn a_ladder_head_is_no_leaps_foot() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let w = &world.collision;
        let foot = Vec3::new(4332.0, -94.0, 56.125);
        let head = Vec3::new(4332.0, -87.12833, 615.125);
        let top = Vec3::new(4322.311, -158.08282, 641.125);
        let leap = |from: Vec3| walk_as(w, from, top.truncate(), Some(top.z), Gait::Leap);
        assert!(matches!(leap(head), Walked::Arrived(_, true)));
        let tipped = Vec3::new(4333.2227, -91.545074, 615.125);
        assert!(matches!(leap(tipped), Walked::Fell));
        let mut g = NavGraph::from_parts(vec![foot, head, top], vec![vec![1], vec![0], vec![]]);
        g.link_leaps(w);
        assert!(g.leaps.is_empty(), "{:?}", g.leaps);
        // Not reached by the climb, the same spot is a foot.
        let mut g = NavGraph::from_parts(vec![foot, head, top], vec![vec![], vec![0], vec![]]);
        g.link_leaps(w);
        assert!(g.leaps.contains(&(1, 2)), "{:?}", g.leaps);
    }

    /// mp_ship's hull beam: a body stands on its side's edge 16 under the
    /// top. A walk from there aimed 8 units beside a top node comes to rest
    /// on that edge again, which is not the node's floor.
    #[test]
    fn a_walk_back_onto_ships_hull_beam_arrives_only_on_top() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let w = &world.collision;
        let edge = Vec3::new(2208.5088, 444.68066, -47.875);
        let top = Vec3::new(2172.9297, 418.1561, -31.875);
        let aside = top.truncate() + (top - edge).truncate().perp().normalize() * SIDESTEP;
        let walked = walk_as(w, edge, aside, Some(top.z), Gait::Forward);
        assert!(!matches!(walked, Walked::Arrived(..)));
        // The way it went: onto the edge at the beam's foot.
        let free = walk_as(w, edge, aside, None, Gait::Forward);
        assert!(matches!(free, Walked::Arrived(p, false) if (p.z - edge.z).abs() < 0.5));
    }

    /// mp_ship's hold ladder: a floor at z 304 opens west of the shaft,
    /// its edge 15 units from where a body grabbed at x 4328.5 hangs. On
    /// the rungs only the strafe moves a body along the face: backing
    /// down facing the face and sliding toward the floor's node lands on
    /// it; without the slide the body passes the lip and hangs under it.
    /// mp_ship's ladder at (2150, -214): the floor at 104 lies behind a
    /// climber hanging level with it, 16 units out from the rungs. The
    /// push-off lands on it only braked; run on toward the floor, the
    /// jump carries the body past its edge into the hull at -64.
    #[test]
    fn a_push_off_lands_on_the_floor_behind_the_rungs() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let w = &world.collision;
        let floor = Vec3::new(2144.741, -229.90549, 104.125);
        let mut ps = PlayerState::spawn(Vec3::new(2146.5, -214.1, 140.0), 90.0);
        let mut sim = Sim::default();
        // Let go of the key 6 above: a 95 units/s descent stops in 5.6.
        for _ in 0..80 {
            let down = if ps.origin.z > 109.0 { -127 } else { 0 };
            let cmd = UserCmd {
                forward: down,
                angles: [
                    (-LADDER_PITCH * ANGLE2SHORT) as i32,
                    (90.0 * ANGLE2SHORT) as i32,
                    0,
                ],
                ..NULL_USERCMD
            };
            sim.tick(w, &mut ps, &cmd);
        }
        assert!(
            ps.on_ladder && (ps.origin.z - floor.z).abs() < 4.0,
            "{}",
            ps.origin
        );
        let yaw = push_off(w, &ps, floor, &[]).expect("a push-off");
        assert!(
            (yaw - 270.0).abs() < 1.0 || (yaw + 90.0).abs() < 1.0,
            "{yaw}"
        );
        // Run on at the floor, the same jump overshoots.
        let mut free = ps;
        for i in 0..40 {
            let cmd = UserCmd {
                forward: if i == 0 { 0 } else { 127 },
                up: if i == 0 { 127 } else { 0 },
                angles: [0, (yaw * ANGLE2SHORT) as i32, 0],
                ..NULL_USERCMD
            };
            sim.tick(w, &mut free, &cmd);
            if free.on_ground {
                break;
            }
        }
        assert!(free.on_ground && free.origin.z < 0.0, "{}", free.origin);
        // Climbing past the floor, or at its foot, there is none.
        let mut low = ps;
        low.origin.z = 60.0;
        assert_eq!(push_off(w, &low, floor, &[]), None);
        assert_eq!(
            push_off(w, &ps, Vec3::new(2160.0, -156.1, 216.1), &[]),
            None
        );
    }

    /// mp_ship's corners at (5375.1, 321.1, 352.1) and (5375.1, -193.1,
    /// 352.1): at rest against the shelf's face, the run at the node
    /// west-south-west (north-west) does not move the body; due west frees
    /// it.
    /// mp_ship's boat at (5250, -500, 27): a dish off the graph that a bot
    /// falls into from the deck at 375, walled off from the deck at 56
    /// beside it, its way out a drop over its rim to the floor at -64.
    /// The deck nodes score nearest; the plan starts under the rim.
    #[test]
    fn a_body_in_a_pit_off_the_graph_plans_from_the_floor_over_its_rim() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let deck = [
            Vec3::new(5280.0, -408.9, 56.125),
            Vec3::new(5312.2, -408.2, 56.125),
            Vec3::new(5344.0, -446.6, 56.125),
        ];
        let floor = Vec3::new(5279.3, -483.9, -63.875);
        let nodes: Vec<Vec3> = deck.iter().copied().chain([floor]).collect();
        let g = NavGraph::from_parts(nodes, vec![vec![]; 4]);
        let body = Vec3::new(5270.0, -450.0, 26.5);
        assert!(g.off_floor(body));
        assert_eq!(g.nearest(body.into()), Some(0));
        assert_eq!(g.nearest_walkable(&world.collision, body), Some(3));
    }

    #[test]
    fn a_body_pinned_in_a_crease_runs_aside() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let w = &world.collision;
        let to = Vec3::new(5344.0938, 312.65878, 352.125);
        let settled = |p: Vec3| {
            let mut ps = PlayerState::spawn(p, 0.0);
            Sim::default().tick(w, &mut ps, &NULL_USERCMD);
            ps
        };
        let ps = settled(Vec3::new(5375.1, 321.1, 352.125));
        let yaw = unpin_yaw(w, &[], &ps, to, &[]).expect("a way out");
        let mut run = ps;
        let mut sim = Sim::default();
        for _ in 0..UNPIN_TICKS {
            let cmd = UserCmd {
                forward: 127,
                angles: [0, (yaw * ANGLE2SHORT) as i32, 0],
                ..NULL_USERCMD
            };
            sim.tick(w, &mut run, &cmd);
        }
        assert!(run.origin.x < 5360.0, "{yaw} {}", run.origin);
        // From the node the run at it is free.
        let node = settled(Vec3::new(5373.6216, 326.14114, 352.125));
        assert_eq!(unpin_yaw(w, &[], &node, to, &[]), None);
        // The south side's crease, 50 degrees off the way north-west.
        let ps = settled(Vec3::new(5375.1, -193.1, 352.125));
        let to = Vec3::new(5344.4, -156.1, 352.125);
        assert!(unpin_yaw(w, &[], &ps, to, &[]).is_some());
    }

    #[test]
    fn a_back_down_slides_along_the_face_onto_a_floor_beside_the_ladder() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let w = &world.collision;
        let node = glam::Vec2::new(4318.5, -96.0);
        let down = |slide: bool| {
            let mut ps = PlayerState::spawn(Vec3::new(4328.5, -93.1, 420.0), 90.0);
            let mut sim = Sim::default();
            for _ in 0..200 {
                let yaw = climb_yaw(ps.ladder_normal).unwrap_or(90.0);
                let right = if slide {
                    crate::bots::along_face(yaw, ps.origin.into(), node.into())
                } else {
                    0
                };
                let cmd = UserCmd {
                    forward: -127,
                    right,
                    angles: [
                        (-LADDER_PITCH * ANGLE2SHORT) as i32,
                        (yaw * ANGLE2SHORT) as i32,
                        0,
                    ],
                    ..NULL_USERCMD
                };
                sim.tick(w, &mut ps, &cmd);
                if ps.on_ground {
                    break;
                }
            }
            ps
        };
        let slid = down(true);
        assert!(
            slid.on_ground && (slid.origin.z - 304.125).abs() < 0.5,
            "{}",
            slid.origin
        );
        let plain = down(false);
        assert!(
            !(plain.on_ground && (plain.origin.z - 304.125).abs() < 0.5),
            "{}",
            plain.origin
        );
    }

    /// mp_ship's lip at (5424, -187, 344): the 396 shelf's side and the
    /// slope under it stand between a body there and the floor at 352
    /// beyond, the nearest nodes by height; the stair node at 320 is a
    /// walk off the lip.
    #[test]
    fn a_plan_starts_at_a_node_the_body_walks_to() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let nodes = vec![
            Vec3::new(5472.0, -125.98952, 352.125),
            Vec3::new(5440.0015, -125.48199, 352.125),
            Vec3::new(5503.9976, -125.821175, 352.125),
            Vec3::new(5407.997, -125.86806, 352.125),
            Vec3::new(5536.001, -125.32347, 352.125),
            Vec3::new(5413.697, -193.1254, 320.125),
        ];
        let g = NavGraph::from_parts(nodes, vec![vec![]; 6]);
        let lip = Vec3::new(5469.5, -176.1, 344.125);
        assert_eq!(g.nearest(lip.into()), Some(0));
        assert_eq!(g.nearest_walkable(&world.collision, lip), Some(5));
    }

    /// A bot that ran off the side of mp_ship's shelf at 396 landed on a
    /// lip at 344 and pushed at the shelf node 52 over it until the stuck
    /// count ran out. On the ground more than a climb under its waypoint,
    /// off a ladder, it plans again, round the edge it fell off.
    #[test]
    fn a_follower_that_fell_off_its_path_plans_again() {
        let g = NavGraph::from_parts(
            vec![
                Vec3::new(5343.4, -155.3, 448.125),
                Vec3::new(5383.6, -161.1, 396.125),
                Vec3::new(5402.6, -160.2, 396.125),
            ],
            vec![vec![1], vec![2], vec![1]],
        );
        let mut f = Follower::default();
        let goal = crate::bots::Goal::To([5402.6, -160.2, 396.125]);
        let mut budget = 100;
        let mut at =
            |p: [f32; 3]| f.waypoint(&g, None, goal, p, Footing::STANDING, &mut budget, &mut || 0);
        assert_eq!(
            at([5343.4, -155.3, 448.125]),
            Some([5383.6, -161.1, 396.125])
        );
        assert_eq!(at([5453.9, -181.9, 344.125]), None);
        assert!(
            f.avoid.iter().any(|e| (e.0, e.1) == (0, 1)),
            "the drop it fell off"
        );
    }

    /// A ladder's head at 352 with its foot at 56 east of the rungs, and
    /// two deck nodes west of the head: a bot at the head about to back
    /// down waits at the far one while a climber comes up near the top,
    /// not while it goes down or is far below, and never on the rungs
    /// itself.
    #[test]
    fn a_bot_at_a_ladders_end_waits_aside_for_a_climber() {
        let nodes = vec![
            Vec3::new(0.0, 0.0, 352.0),
            Vec3::new(40.0, 0.0, 56.0),
            Vec3::new(-48.0, 0.0, 352.0),
            Vec3::new(-24.0, 20.0, 352.0),
        ];
        let mut g = NavGraph::from_parts(nodes, vec![vec![1, 2, 3], vec![], vec![0], vec![0]]);
        g.ladder_boxes = vec![(Vec3::new(4.0, -12.0, 56.0), Vec3::new(5.0, 12.0, 352.0))];
        let mut f = Follower {
            path: vec![0, 1],
            next: 1,
            ..Default::default()
        };
        let body = Footing {
            still: false,
            ..Footing::STANDING
        };
        let at = Vec3::new(-2.0, 0.0, 352.0);
        let up = [(Vec3::new(20.0, 0.0, 300.0), Vec3::new(0.0, 0.0, 60.0))];
        let hung = [(Vec3::new(20.0, 10.0, 352.5), Vec3::ZERO)];
        let down = [(Vec3::new(20.0, 0.0, 300.0), Vec3::new(0.0, 0.0, -60.0))];
        // Far down the rungs, the climb is the time it takes; the two pass
        // on the ladder.
        let low = [(Vec3::new(20.0, 0.0, 200.0), Vec3::new(0.0, 0.0, 60.0))];
        assert_eq!(f.ladder_yield(&g, at, body, &low), None);
        assert_eq!(f.ladder_yield(&g, at, body, &up), Some(g.nodes[2]));
        assert_eq!(f.ladder_yield(&g, at, body, &hung), Some(g.nodes[2]));
        assert_eq!(f.ladder_yield(&g, at, body, &down), None);
        assert_eq!(f.ladder_yield(&g, at, body, &[]), None);
        let rungs = Footing {
            on_ladder: true,
            ..body
        };
        assert_eq!(f.ladder_yield(&g, at, rungs, &up), None);
        // Walking to the head with the ladder next waits too.
        f.next = 0;
        assert_eq!(f.ladder_yield(&g, at, body, &up), Some(g.nodes[2]));
        // A wait that runs out is not taken again for a while.
        for _ in 1..YIELD_TICKS {
            assert!(f.ladder_yield(&g, at, body, &up).is_some());
        }
        assert_eq!(f.ladder_yield(&g, at, body, &up), None);
        assert_eq!(f.ladder_yield(&g, at, body, &up), None);
        // A climber hanging still is waited for a shorter while.
        let mut f = Follower {
            path: vec![0, 1],
            next: 1,
            ..Default::default()
        };
        let waited = (0..YIELD_TICKS).take_while(|_| f.ladder_yield(&g, at, body, &hung).is_some());
        assert_eq!(waited.count() as u32, YIELD_HUNG);
    }

    /// A plan begun on the rungs under mp_ship's crow's nest plate starts
    /// at the head; passed from the rungs 8 under it, the bot turned for
    /// the rim's exit, let go and fell down the hatch. It is passed only
    /// on the ground.
    #[test]
    fn a_plan_from_the_rungs_passes_its_head_only_on_the_ground() {
        let head = Vec3::new(4963.9, 64.0, 1219.125);
        let exit = Vec3::new(4963.9, 35.5, 1219.125);
        let g = NavGraph::from_parts(vec![head, exit], vec![vec![1], vec![0]]);
        let mut f = Follower {
            path: vec![0, 1],
            ..Follower::default()
        };
        let rungs = Footing {
            still: false,
            on_ground: false,
            on_ladder: true,
            velocity: Vec3::new(0.0, 0.0, 120.0),
        };
        let mut budget = 100;
        let mut rand = || 0;
        let goal = crate::bots::Goal::Roam;
        let at = [4959.9, 62.4, 1210.9];
        assert_eq!(
            f.waypoint(&g, None, goal, at, rungs, &mut budget, &mut rand),
            Some(head.into())
        );
        let ground = Footing {
            on_ground: true,
            on_ladder: false,
            ..rungs
        };
        let at = [4962.0, 63.0, 1219.125];
        assert_eq!(
            f.waypoint(&g, None, goal, at, ground, &mut budget, &mut rand),
            Some(exit.into())
        );
    }

    /// mp_ship's crow's nest head stands on the ladder's plate in a hatch
    /// about 40 by 46 units, 11 units from the top's face to the west. The
    /// sweep west stops at the face before the first sample, and a sample
    /// north-west stands on the hatch's rim: the probe read no drop, and a
    /// spell's heading ran bots down the hatch.
    #[test]
    fn the_drop_probe_feels_the_crows_nest_hatch() {
        let Some(world) = map_world("mp_ship") else {
            return;
        };
        let plate = Vec3::new(4963.9, 64.0, 1219.125);
        let mut creases = Creases::default();
        let mut drop = |yaw: f32| {
            let (s, c) = yaw.to_radians().sin_cos();
            drop_ahead(
                &world.collision,
                plate,
                Vec3::new(c, s, 0.0),
                64.0,
                &mut creases,
            )
        };
        assert_eq!(drop(180.0), Some(f32::INFINITY));
        assert_eq!(drop(135.0), Some(f32::INFINITY));
        // The exit along the rim stays open.
        assert_eq!(drop(270.0), None);
    }

    /// Backing down a ladder with a climber coming up under it, a body
    /// climbs back to the head: one that met the climber stood on its head
    /// and slid off mp_ship's crow's nest ladder to the deck.
    #[test]
    fn a_bot_backing_down_onto_a_climber_climbs_back_up() {
        let head = Vec3::new(0.0, 0.0, 1219.0);
        let foot = Vec3::new(-4.0, 0.0, 692.0);
        let g = NavGraph::from_parts(vec![head, foot], vec![vec![1], vec![]]);
        let mut f = Follower {
            path: vec![0, 1],
            next: 1,
            ..Default::default()
        };
        let rungs = Footing {
            still: false,
            on_ground: false,
            on_ladder: true,
            velocity: Vec3::new(0.0, 0.0, -120.0),
        };
        let at = Vec3::new(-4.0, 1.0, 1131.0);
        let up = |z: f32| [(Vec3::new(-4.0, 2.0, z), Vec3::new(0.0, 0.0, 100.0))];
        assert_eq!(f.head_on_yield(&g, at, rungs, &up(1054.0)), Some(head));
        assert_eq!(f.head_on_yield(&g, at, rungs, &up(900.0)), None, "far down");
        let down = [(Vec3::new(-4.0, 2.0, 1054.0), Vec3::new(0.0, 0.0, -100.0))];
        assert_eq!(
            f.head_on_yield(&g, at, rungs, &down),
            None,
            "going down too"
        );
        let ground = Footing {
            on_ground: true,
            on_ladder: false,
            ..rungs
        };
        assert_eq!(
            f.head_on_yield(&g, head, ground, &up(1180.0)),
            None,
            "off the rungs"
        );
        // Climbing up, the climber below is behind.
        f.path = vec![1, 0];
        assert_eq!(f.head_on_yield(&g, at, rungs, &up(1054.0)), None);
    }

    #[test]
    fn nearest_prefers_the_floor_under_the_feet() {
        let mut g = tiny();
        // A node straight above node 1, one storey up.
        g = NavGraph::from_parts(
            [g.nodes.clone(), vec![Vec3::new(32.0, 0.0, 128.0)]].concat(),
            [g.edges.clone(), vec![vec![]]].concat(),
        );
        assert_eq!(g.nearest([30.0, 2.0, 10.0]), Some(1));
        assert_eq!(g.nearest([30.0, 2.0, 140.0]), Some(5));
        assert_eq!(g.nearest([5000.0, 0.0, 0.0]), None);
    }
}
