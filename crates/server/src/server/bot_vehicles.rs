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
