//! The server half of bots in United Offensive vehicles
//! (`crate::bots::vehicle`): each bot's seat, and the vehicles it could
//! board, as the use key would find them (`game::vehicle::seat::usable`).

use super::*;
use crate::bots::{SeatView, VehicleSpot};
use crate::game::vehicle::info::KIND_TANK;

/// `vc_bot_vehicles`: 0 keeps bots on foot.
pub(crate) const VEHICLES_CVAR: &str = "vc_bot_vehicles";

impl Server {
    /// Fills [`crate::bots::BotView::seat`] and `vehicles` for this tick's
    /// views. A level with no vehicle leaves them empty.
    pub(super) fn bot_vehicle_views(&mut self, views: &mut [(usize, crate::bots::BotView)]) {
        let on = crate::game::builtins::cvar::atoi(&self.live_cvar(VEHICLES_CVAR, "1")) != 0;
        let Some(rt) = self.script.as_mut().filter(|_| on) else {
            return;
        };
        if rt.host.vehicles.live.is_empty() {
            return;
        }
        rt.with_host(|host, cx| {
            let order = host.vehicles.in_slot_order();
            for (slot, view) in views.iter_mut() {
                if !view.playing {
                    continue;
                }
                view.seat = seat_view(host, *slot);
                if view.seat.is_some() {
                    continue;
                }
                let reach = crate::bots::VEHICLE_SEEK_RANGE;
                for &id in &order {
                    if host.ents.get(id).is_none_or(|e| e.unlinked) {
                        continue;
                    }
                    let Some(v) = host.vehicles.live.get(&id) else {
                        continue;
                    };
                    let info = host.vehicles.info_of(v);
                    let spot = VehicleSpot {
                        num: id.0,
                        origin: v.pose.origin,
                        yaw: v.pose.angles[1],
                        mins: info.mins,
                        maxs: info.maxs,
                    };
                    if spot.gap(view.origin) < reach
                        && crate::game::vehicle::seat::usable(host, cx, id, *slot)
                    {
                        view.vehicles.push(spot);
                    }
                }
            }
        });
    }

    /// Test-facing: each live vehicle in pool order, as a bot would see it.
    pub fn test_vehicles(&self) -> Vec<VehicleSpot> {
        let Some(rt) = self.script.as_ref() else {
            return Vec::new();
        };
        let host = &rt.host;
        host.vehicles
            .in_slot_order()
            .into_iter()
            .filter_map(|id| {
                let v = host.vehicles.live.get(&id)?;
                let info = host.vehicles.info_of(v);
                Some(VehicleSpot {
                    num: id.0,
                    origin: v.pose.origin,
                    yaw: v.pose.angles[1],
                    mins: info.mins,
                    maxs: info.maxs,
                })
            })
            .collect()
    }
}

/// The seat `slot` sits in, off the vehicle's record and wire fields.
fn seat_view(host: &crate::game::host::GameHost, slot: usize) -> Option<SeatView> {
    let (id, seat) = host.vehicles.seat_of(slot)?;
    let v = host.vehicles.live.get(&id)?;
    let info = host.vehicles.info_of(v);
    let tank = info.kind == KIND_TANK;
    let yaw = v.pose.angles[1];
    // The turret's yaw off the hull, and the gunner's off the turret
    // (coduo-vehicles.md 2).
    let gun_yaw = match seat {
        1 if tank => Some(yaw + v.wire.angles2[1]),
        2 if info.has_gunner => Some(yaw + v.wire.angles2[1] + v.wire.origin2[1]),
        _ => None,
    };
    Some(SeatView {
        seat,
        tank,
        origin: v.pose.origin,
        yaw,
        speed: v.forward_speed,
        gun_yaw,
        driven: v.seats[1].is_some(),
    })
}

/// How far ahead along its route a driver steers: a hull turns wide, and
/// the graph's nodes stand a grid pitch apart.
const LOOK_AHEAD: f32 = 220.0;
/// A route node this near the hull is passed.
const PASS: f32 = 110.0;
/// A route goes again when its goal moves this far, when it makes no
/// headway on its next node for `HEADWAY_MS`, or after `REPLAN_MS`.
const RETARGET: f32 = 300.0;
const HEADWAY_MS: i32 = 5000;
const REPLAN_MS: i32 = 15_000;
/// A roam drives to a node at least this far off.
const ROAM_MIN: f32 = 1200.0;
/// The node expansions one plan may take.
const PLAN_EXPANSIONS: usize = 30_000;
/// A node a hull fits on: its box, this share of the vehicle's half
/// width, clear of the world and the vehicle clip from `FIT_LIFT` over the
/// feet up `FIT_HEIGHT`, and no edge on the way steeper than a step.
const FIT_SHARE: f32 = 0.75;
const FIT_LIFT: f32 = 18.0;
const FIT_HEIGHT: f32 = 48.0;
const VEHICLE_CLIP: u32 = 0x200;

/// One driver's route over the navigation graph.
pub(super) struct DriveRoute {
    path: Vec<u32>,
    next: usize,
    /// The goal point it was planned for, `None` for a roam.
    goal: Option<[f32; 3]>,
    planned_ms: i32,
    /// Nearest the hull has come to the next node, and when.
    best: f32,
    best_ms: i32,
}

/// The drivers' routes, and which nodes a hull of a given half width fits
/// on, for the graph they were found on.
#[derive(Default)]
pub(super) struct BotDrives {
    routes: BTreeMap<usize, DriveRoute>,
    fit: HashMap<(u32, u32), bool>,
    graph: usize,
}

impl BotDrives {
    pub(super) fn forget(&mut self, slot: usize) {
        self.routes.remove(&slot);
    }
}

impl Server {
    /// The point a bot driving a vehicle steers at toward `goal`, along a
    /// route over the graph's nodes its hull fits on (no ladder, jump or
    /// step-high edge); `None` with nothing to drive to.
    pub(super) fn drive_waypoint(
        &mut self,
        slot: usize,
        goal: crate::bots::Goal,
        seat: &SeatView,
    ) -> Option<[f32; 3]> {
        use crate::bots::Goal;
        let g = self.nav.clone()?;
        let graph = std::sync::Arc::as_ptr(&g) as usize;
        if self.bot_drives.graph != graph {
            self.bot_drives = BotDrives {
                graph,
                ..BotDrives::default()
            };
        }
        let half = self.seated_half_width(slot)?;
        let now = self.sv_time_ms;
        let at = glam::Vec3::from(seat.origin);
        let flat = |p: glam::Vec3| (p - at).truncate().length();
        let want = match goal {
            Goal::Hold => {
                self.bot_drives.routes.remove(&slot);
                return None;
            }
            Goal::To(p) => Some(p),
            Goal::Roam | Goal::Away(_) => None,
        };
        let stale = self.bot_drives.routes.get(&slot).is_none_or(|r| {
            let moved = match (r.goal, want) {
                (Some(a), Some(b)) => glam::Vec3::from(a).distance(b.into()) > RETARGET,
                (None, None) => false,
                _ => true,
            };
            moved
                || r.next >= r.path.len() && want.is_none()
                || now.wrapping_sub(r.best_ms) > HEADWAY_MS
                || now.wrapping_sub(r.planned_ms) > REPLAN_MS
        });
        if stale {
            self.bot_drives.routes.remove(&slot);
            let mut rng = self.bots.get(&slot).map_or(1, |b| b.rng);
            let path = self.plan_drive(&g, half, at, want, &mut rng);
            if let Some(b) = self.bots.get_mut(&slot) {
                b.rng = rng;
            }
            let path = path?;
            self.bot_drives.routes.insert(
                slot,
                DriveRoute {
                    path,
                    next: 0,
                    goal: want,
                    planned_ms: now,
                    best: f32::INFINITY,
                    best_ms: now,
                },
            );
        }
        let r = self.bot_drives.routes.get_mut(&slot)?;
        while r.next < r.path.len() && flat(g.nodes[r.path[r.next] as usize]) < PASS {
            r.next += 1;
            r.best = f32::INFINITY;
            r.best_ms = now;
        }
        let Some(&n) = r.path.get(r.next) else {
            // Driven out: a goal is there, a roam plans again next tick.
            return None;
        };
        let d = flat(g.nodes[n as usize]);
        if d < r.best - 8.0 {
            r.best = d;
            r.best_ms = now;
        }
        let ahead = r.path[r.next..]
            .iter()
            .map(|&n| g.nodes[n as usize])
            .find(|p| flat(*p) >= LOOK_AHEAD)
            .unwrap_or(g.nodes[*r.path.last()? as usize]);
        Some(ahead.into())
    }

    /// Half the narrower side of the box of the vehicle `slot` sits in.
    fn seated_half_width(&self, slot: usize) -> Option<f32> {
        let host = &self.script.as_ref()?.host;
        let (id, _) = host.vehicles.seat_of(slot)?;
        let info = host.vehicles.info_of(host.vehicles.live.get(&id)?);
        Some(((info.maxs[0] - info.mins[0]).min(info.maxs[1] - info.mins[1])) * 0.5)
    }

    /// A* from the node nearest `at` to the node nearest `want` (a random
    /// far node for a roam) over the nodes a hull of `half` fits on. A goal
    /// out of reach routes to the reached node nearest it.
    fn plan_drive(
        &mut self,
        g: &crate::nav::NavGraph,
        half: f32,
        at: glam::Vec3,
        want: Option<[f32; 3]>,
        rng: &mut u64,
    ) -> Option<Vec<u32>> {
        let world = self.world.as_ref()?;
        let r = half * FIT_SHARE;
        let key = r as u32;
        let fit = &mut self.bot_drives.fit;
        let mut fits = |n: u32| {
            *fit.entry((n, key)).or_insert_with(|| {
                let p = g.nodes[n as usize] + glam::Vec3::Z * FIT_LIFT;
                !world.collision.box_position_test(
                    p,
                    glam::Vec3::new(-r, -r, 0.0),
                    glam::Vec3::new(r, r, FIT_HEIGHT),
                    vcod_common::collision::CONTENTS_SOLID | VEHICLE_CLIP,
                )
            })
        };
        let from = g.nearest(at.into())?;
        let to = match want {
            Some(p) => g.goal_node(p).or_else(|| g.nearest(p))?,
            None => {
                // A few draws for a far node a hull fits on.
                (0..16).find_map(|_| {
                    let n = (rand_from(rng) as usize % g.nodes.len()) as u32;
                    let p = g.nodes[n as usize];
                    ((p - at).truncate().length() > ROAM_MIN && fits(n)).then_some(n)
                })?
            }
        };
        let goal = g.nodes[to as usize];
        let h = |n: u32| g.nodes[n as usize].distance(goal);
        let mut open = std::collections::BinaryHeap::new();
        let mut cost: HashMap<u32, (f32, u32)> = HashMap::new();
        cost.insert(from, (0.0, from));
        open.push(std::cmp::Reverse((ordered(h(from)), from)));
        let (mut best, mut best_h) = (from, h(from));
        let mut expanded = 0;
        while let Some(std::cmp::Reverse((_, n))) = open.pop() {
            if n == to {
                best = to;
                break;
            }
            expanded += 1;
            if expanded > PLAN_EXPANSIONS {
                break;
            }
            let here = cost[&n].0;
            let p = g.nodes[n as usize];
            if h(n) < best_h {
                (best, best_h) = (n, h(n));
            }
            for &m in &g.edges[n as usize] {
                let q = g.nodes[m as usize];
                let plain = !g.jumps.contains(&(n, m)) && !g.leaps.contains(&(n, m));
                if !plain || (q.z - p.z).abs() > vcod_common::pmove::STEPSIZE || !fits(m) {
                    continue;
                }
                let c = here + p.distance(q);
                if cost.get(&m).is_none_or(|(old, _)| c < *old) {
                    cost.insert(m, (c, n));
                    open.push(std::cmp::Reverse((ordered(c + h(m)), m)));
                }
            }
        }
        let mut path = vec![best];
        while let Some(&(_, prev)) = cost.get(path.last()?) {
            if prev == *path.last()? {
                break;
            }
            path.push(prev);
        }
        path.reverse();
        (path.len() > 1).then_some(path)
    }
}

/// An f32 cost as a heap key; costs are finite and non-negative.
fn ordered(c: f32) -> u32 {
    c.to_bits()
}
