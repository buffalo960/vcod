//! Bots in United Offensive vehicles: walking up to one and boarding it
//! with the use key, driving it along the navigation graph's waypoints,
//! laying its guns, and getting out. Every input is a key a client sends
//! from a seat (docs/research/coduo-vehicles.md 1, 2): the use press
//! boards and exits, `forwardmove` and `rightmove` drive, the view lays the
//! seat's gun and `buttons` 0x1 fires it. The server half, what the seat
//! and the vehicles near the bot look like, is
//! `server/bot_vehicles.rs`. docs/research/bot-objectives.md, "Vehicles".

use super::{Bot, BotView, Goal, aim_angles, cmd_angles, yaw_diff};
use vcod_common::net::msg::{self, UserCmd};

/// A vehicle this near across is worth walking to.
pub(crate) const SEEK_RANGE: f32 = 900.0;
/// The use press goes in this near the hull, and the walk stops this near.
const PRESS_REACH: f32 = 90.0;
const STAND_OFF: f32 = 30.0;
/// Ticks a walk to one vehicle may take, and the presses at its side,
/// before the bot gives it up.
const SEEK_TICKS: u32 = 400;
const PRESS_TICKS: u32 = 40;
/// Ticks after an exit or a give-up before the bot looks for one again.
const REST_TICKS: u32 = 600;
/// A ride ends after this long, so the bot gets back to the objective.
const RIDE_TICKS: u32 = 1800;
/// Ticks a seat with nothing to drive to, or a gunner's seat with nobody
/// driving, is held before the bot gets out.
const IDLE_TICKS: u32 = 100;
/// A driven vehicle under this speed with the throttle open is stalling;
/// past `STALL_TICKS` of it the bot backs off for `REVERSE_TICKS`, and
/// past `STALLS` back-offs with no ground made it gets out.
const STALL_SPEED: f32 = 30.0;
const STALL_TICKS: u32 = 30;
const REVERSE_TICKS: u32 = 30;
const STALLS: u32 = 4;
/// Ground a ride has to make for its back-offs to start counting afresh.
const PROGRESS: f32 = 300.0;
/// A route's last point this near the hull is reached.
const PASSED: f32 = 48.0;
/// The steer holds off inside this much heading error; a tank turns in
/// place past `TURN_IN_PLACE` before it drives on.
const STEER_SLACK: f32 = 6.0;
const TURN_IN_PLACE: f32 = 50.0;
/// A seat's gun fires once laid this near the enemy in yaw, within range.
const LAID_DEG: f32 = 4.0;
const GUN_RANGE: f32 = 3000.0;

/// A usable vehicle near the bot, as the server sees it this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleSpot {
    pub num: u32,
    pub origin: [f32; 3],
    /// Hull yaw, degrees.
    pub yaw: f32,
    /// The vehicle file's box, in the hull's frame.
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

impl VehicleSpot {
    /// The box's centre in the world.
    pub fn centre(&self) -> [f32; 3] {
        let c: [f32; 3] = std::array::from_fn(|i| (self.mins[i] + self.maxs[i]) * 0.5);
        let (s, k) = self.yaw.to_radians().sin_cos();
        [
            self.origin[0] + c[0] * k - c[1] * s,
            self.origin[1] + c[0] * s + c[1] * k,
            self.origin[2] + c[2],
        ]
    }

    /// How far across `p` stands off the box, 0 inside it.
    pub fn gap(&self, p: [f32; 3]) -> f32 {
        let (s, k) = self.yaw.to_radians().sin_cos();
        let (dx, dy) = (p[0] - self.origin[0], p[1] - self.origin[1]);
        let local = [dx * k + dy * s, -dx * s + dy * k];
        let out = |i: usize| {
            (self.mins[i] - local[i])
                .max(local[i] - self.maxs[i])
                .max(0.0)
        };
        out(0).hypot(out(1))
    }
}

/// The seat the bot sits in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeatView {
    /// 1 drives, 2 is a gunner's, 3 up ride.
    pub seat: usize,
    pub tank: bool,
    /// The vehicle's origin, the hull yaw (degrees) and its forward speed,
    /// units/s.
    pub origin: [f32; 3],
    pub yaw: f32,
    pub speed: f32,
    /// The world yaw the seat's gun points along: a tank's turret for its
    /// driver, the MG for a gunner. `None` for a seat with no gun.
    pub gun_yaw: Option<f32>,
    /// Somebody sits in the driver's seat.
    pub driven: bool,
}

/// The bot's own vehicle state, carried across ticks.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Rider {
    /// The vehicle walked to, and the ticks spent on it.
    target: Option<VehicleSpot>,
    seek_ticks: u32,
    press_ticks: u32,
    /// Ticks before another vehicle is looked for.
    rest: u32,
    /// Ticks in the seat, and the ticks with nothing to do there.
    seated: u32,
    idle: u32,
    /// A use press goes in on every other tick: the server boards and exits
    /// on the press's edge.
    pressed: bool,
    /// The seat is being left.
    leaving: bool,
    /// Stall bookkeeping: ticks stalling, back-off ticks left and the way
    /// it steers, back-offs since the ride last made [`PROGRESS`], from
    /// where.
    stall: u32,
    reverse: u32,
    reverse_right: i8,
    stalls: u32,
    progress_from: [f32; 3],
    /// Boardings and exits, for the gates.
    pub(crate) boarded: u32,
}

impl Rider {
    /// The vehicle the bot walks to, for [`Bot::goal`].
    pub(crate) fn heading_to(&self) -> Option<[f32; 3]> {
        self.target.map(|t| t.origin)
    }
}

impl Bot {
    /// Where the bot walks to board a vehicle, ahead of its objective.
    pub(crate) fn vehicle_goal(&self, view: &BotView) -> Option<Goal> {
        if view.seat.is_some() {
            return None;
        }
        self.rider.heading_to().map(Goal::To)
    }

    /// The tick's cmd in or around a vehicle; `None` to play on foot as
    /// usual.
    pub(crate) fn think_vehicle(&mut self, view: &BotView, cmd: UserCmd) -> Option<UserCmd> {
        self.rider.rest = self.rider.rest.saturating_sub(1);
        if let Some(seat) = view.seat {
            if self.rider.seated == 0 {
                self.rider.boarded += 1;
                self.rider.target = None;
                self.rider.progress_from = seat.origin;
            }
            self.rider.seated += 1;
            // Whatever the walk was doing starts afresh once out.
            self.disengage();
            self.restart_stall(view.origin);
            return Some(self.think_seat(view, seat, cmd));
        }
        if self.rider.seated > 0 {
            // Out, by choice or by the vehicle's death.
            self.rider = Rider {
                rest: REST_TICKS,
                boarded: self.rider.boarded,
                ..Rider::default()
            };
        }
        self.think_boarding(view, cmd)
    }

    /// A death drops the walk and the seat.
    pub(crate) fn vehicle_life_over(&mut self) {
        self.rider = Rider {
            boarded: self.rider.boarded,
            ..Rider::default()
        };
    }

    /// Times the bot got into a seat.
    pub fn boardings(&self) -> u32 {
        self.rider.boarded
    }

    /// Picks a vehicle to walk to, walks there and presses use at its side.
    fn think_boarding(&mut self, view: &BotView, mut cmd: UserCmd) -> Option<UserCmd> {
        let free = view.enemy.is_none()
            && matches!(self.stage, super::Stage::Wander)
            && view.frag_near.is_none()
            && !self.objective_busy(view)
            && self.forced.is_none();
        if !free {
            self.rider.target = None;
            return None;
        }
        // The pick is refreshed off this tick's list: a vehicle somebody
        // else took, or one driven off, drops out of it.
        let near = |s: &VehicleSpot| s.gap(view.origin) < SEEK_RANGE;
        let target = match self.rider.target {
            Some(t) => view.vehicles.iter().find(|s| s.num == t.num).copied(),
            None if self.rider.rest == 0 => view
                .vehicles
                .iter()
                .filter(|s| near(s))
                .min_by(|a, b| a.gap(view.origin).total_cmp(&b.gap(view.origin)))
                .copied(),
            None => None,
        };
        let Some(t) = target else {
            self.rider.target = None;
            self.rider.seek_ticks = 0;
            self.rider.press_ticks = 0;
            return None;
        };
        self.rider.target = Some(t);
        self.rider.seek_ticks += 1;
        let gap = t.gap(view.origin);
        if gap > PRESS_REACH {
            if self.rider.seek_ticks > SEEK_TICKS {
                self.give_up_vehicle();
            }
            // The graph's waypoint takes it there (`Bot::goal`).
            return None;
        }
        self.rider.press_ticks += 1;
        if self.rider.press_ticks > PRESS_TICKS {
            self.give_up_vehicle();
            return None;
        }
        let (pitch, yaw) = aim_angles(view, t.centre());
        cmd.angles = cmd_angles(view, [pitch, yaw]);
        cmd.forward = if gap > STAND_OFF { 127 } else { 0 };
        self.rider.pressed = !self.rider.pressed;
        if self.rider.pressed {
            cmd.buttons |= msg::BUTTON_USE;
        }
        self.restart_stall(view.origin);
        Some(cmd)
    }

    fn give_up_vehicle(&mut self) {
        self.rider.target = None;
        self.rider.seek_ticks = 0;
        self.rider.press_ticks = 0;
        self.rider.rest = REST_TICKS;
    }

    /// One tick in a seat: drive, lay the gun, or get out.
    fn think_seat(&mut self, view: &BotView, seat: SeatView, mut cmd: UserCmd) -> UserCmd {
        // The view stays where it is unless a gun or the walk-in turns it.
        let mut aim = [view.view[0], view.view[1]];
        let enemy = view
            .enemy
            .filter(|e| super::dist_sq(seat.origin, e.origin) < GUN_RANGE * GUN_RANGE);
        if seat.seat == 1 {
            self.drive(view, seat, &mut cmd);
        } else if seat.driven || enemy.is_some() {
            self.rider.idle = 0;
        } else {
            self.rider.idle += 1;
        }
        if let Some(e) = enemy {
            let (pitch, yaw) = aim_angles(view, e.origin);
            aim = [pitch, yaw];
            // A four-wheel's driver has no gun (0x1 sounds the horn); a
            // passenger fires his own weapon, a tap at a time.
            let laid = match seat.gun_yaw {
                Some(g) => yaw_diff(yaw, g).abs() < LAID_DEG,
                None => seat.seat > 2 && yaw_diff(yaw, view.view[1]).abs() < LAID_DEG,
            };
            let gun = seat.gun_yaw.is_some() || seat.seat > 2;
            if self.shoot && gun && laid && !view.friend_in_line {
                let tap =
                    seat.gun_yaw.is_some() || view.automatic || self.rider.seated.is_multiple_of(2);
                if tap {
                    cmd.buttons |= msg::BUTTON_ATTACK;
                }
            }
        } else if seat.gun_yaw.is_some() && seat.seat == 1 {
            // A tank's turret looks the way it drives.
            aim = [0.0, seat.yaw];
        }
        cmd.angles = cmd_angles(view, aim);
        let done = self.rider.seated > RIDE_TICKS || self.rider.idle > IDLE_TICKS;
        if done || self.rider.leaving {
            self.rider.leaving = true;
            cmd.forward = 0;
            cmd.right = 0;
            cmd.buttons &= !msg::BUTTON_ATTACK;
            self.rider.pressed = !self.rider.pressed;
            if self.rider.pressed {
                cmd.buttons |= msg::BUTTON_USE;
            }
        }
        cmd
    }

    /// The driver's keys toward the waypoint: `rightmove` steers both
    /// kinds, a tank turns in place, a four-wheel only rolling
    /// (coduo-vehicles.md 2, 12).
    fn drive(&mut self, view: &BotView, seat: SeatView, cmd: &mut UserCmd) {
        let r = &mut self.rider;
        let to = view
            .waypoint
            .filter(|w| (w[0] - seat.origin[0]).hypot(w[1] - seat.origin[1]) > PASSED);
        // A fight holds a tank still for the gun; a four-wheel drives on.
        let hold = seat.tank && view.enemy.is_some();
        let Some(to) = to.filter(|_| !hold) else {
            r.stall = 0;
            r.idle = if hold { 0 } else { r.idle + 1 };
            return;
        };
        r.idle = 0;
        if super::dist_sq(seat.origin, r.progress_from) > PROGRESS * PROGRESS {
            r.progress_from = seat.origin;
            r.stalls = 0;
        }
        if r.reverse > 0 {
            r.reverse -= 1;
            cmd.forward = -127;
            cmd.right = r.reverse_right;
            return;
        }
        let want = (to[1] - seat.origin[1])
            .atan2(to[0] - seat.origin[0])
            .to_degrees();
        let off = yaw_diff(want, seat.yaw);
        // Positive `rightmove` turns the hull clockwise, the yaw down.
        cmd.right = if off > STEER_SLACK {
            -127
        } else if off < -STEER_SLACK {
            127
        } else {
            0
        };
        cmd.forward = if seat.tank && off.abs() > TURN_IN_PLACE {
            0
        } else {
            127
        };
        if cmd.forward != 0 && seat.speed.abs() < STALL_SPEED {
            r.stall += 1;
        } else {
            r.stall = 0;
        }
        if r.stall >= STALL_TICKS {
            r.stall = 0;
            r.stalls += 1;
            r.reverse = REVERSE_TICKS;
            // Backing out steers the other way round the obstacle.
            r.reverse_right = if cmd.right == 0 { 127 } else { cmd.right };
            if r.stalls >= STALLS {
                r.leaving = true;
            }
        }
    }
}
