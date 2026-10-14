//! The view from a United Offensive vehicle seat: where `CG_VehicleView`
//! (uo cgame 0x30040810) puts the camera off the posed vehicle's tags, the
//! gunner's view held on its gun, and a four-wheel's riders turned with it
//! (0x30040580). docs/research/coduo-vehicles.md, 5.

use crate::fx::sim::TagPose;
use glam::{Mat3, Vec3};
use vcod_common::pmove::aim::angles_to_axis;

/// `centity` +0x88, the vehicle entity's `surfType`: its file's type.
pub const CLASS_FOUR_WHEEL: i32 = 1;
pub const CLASS_TANK: i32 = 2;
pub const CLASS_ARTILLERY: i32 = 5;

/// The seat the view sits in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seat {
    /// `ps.vehPos`: 1 the driver, 2 the gunner, 3.. passengers.
    pub pos: i32,
    /// `ps.vehType`.
    pub kind: i32,
    /// The vehicle entity's `surfType` (`CLASS_*`).
    pub class: i32,
}

impl Seat {
    /// The four-wheel's seat 3, which keeps its own weapon (cgame
    /// 0x30008210).
    pub fn own_weapon(&self) -> bool {
        self.kind == 1 && self.pos == 3
    }

    /// The field of view in a seat that draws no gun (`CG_AddViewWeapon`
    /// 0x30046570 draws one only for the jeep passenger), `None` for that
    /// one (0x3003ffc0): the driver's sight on a
    /// weapon with one closes toward 70 by the sight's fraction; every other
    /// seat but the jeep passenger's keeps `cg_fov` (the passenger zooms
    /// with its own weapon, as on foot).
    pub fn fov(&self, cg_fov: f32, frac: f32, weapon_has_sight: bool) -> Option<f32> {
        if self.own_weapon() {
            return None;
        }
        Some(if self.pos == 1 && weapon_has_sight {
            cg_fov - (cg_fov - 70.0) * frac
        } else {
            cg_fov
        })
    }
}

/// `0x300407c0`: the tag a seat's chase camera turns about.
fn pivot_tag(pos: i32) -> &'static str {
    match pos {
        2 => "tag_secondary_gun",
        3 => "tag_passenger",
        4 => "tag_passenger2",
        5 => "tag_passenger3",
        6 => "tag_passenger4",
        _ => "tag_turret",
    }
}

/// `BG_GetVehiclePosTag` (cgame 0x30008190).
fn seat_tag(pos: i32) -> &'static str {
    match pos {
        1 => "tag_player",
        2 => "tag_secondary_player",
        3 => "tag_passenger",
        4 => "tag_passenger2",
        5 => "tag_passenger3",
        6 => "tag_passenger4",
        _ => "tag_player",
    }
}

/// Where the seat puts the view, before the clip trace.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeatCamera {
    pub pos: Vec3,
    /// Pitch and roll the refdef takes from the gun instead of the view,
    /// degrees, wire convention (the gunner).
    pub gun_pitch_roll: Option<[f32; 2]>,
    /// The view the client's own angles snap to (trap 0xf7), pitch and
    /// yaw degrees: the gunner's view put back on its gun.
    pub snap: Option<[f32; 2]>,
    /// The clip trace's start: `tag_body`, else the vehicle's origin, 20 up.
    pub trace_from: Vec3,
}

/// `CG_VehicleView` (0x30040810) without its trace. `view` is the
/// predicted view, wire degrees; `frac` the sight's fraction; `tag` poses a
/// tag of the vehicle as drawn this frame; `origin` its drawn origin.
pub fn camera(
    seat: Seat,
    view: [f32; 3],
    frac: f32,
    origin: Vec3,
    tag: &mut dyn FnMut(&str) -> Option<TagPose>,
) -> Option<SeatCamera> {
    let trace_from = tag("tag_body").map_or(origin, |t| t.origin) + Vec3::Z * 20.0;
    let out = |pos| {
        Some(SeatCamera {
            pos,
            gun_pitch_roll: None,
            snap: None,
            trace_from,
        })
    };
    if seat.class == CLASS_ARTILLERY && frac == 0.0 {
        return out(tag("tag_chasecam")?.origin);
    }
    if seat.pos == 2 {
        let g = tag("tag_secondary_gun")?;
        let [f, _, u] = g.axis;
        let eye = tag("tag_secondary_view").map_or(g.origin + f * -20.0 + u * 6.5, |t| t.origin);
        let [pitch, yaw, roll] = axis_to_angles(g.axis);
        // 0x30040c3d: off by more than 0.4 degrees of pitch, the client's
        // angles are put on the gun's.
        let snap = (vcod_common::pmove::angle_normalize180(view[0] - pitch).abs() > 0.4)
            .then_some([pitch, yaw]);
        return Some(SeatCamera {
            pos: eye,
            gun_pitch_roll: Some([pitch, roll]),
            snap,
            trace_from,
        });
    }
    let four_wheel = seat.class == CLASS_FOUR_WHEEL;
    if seat.own_weapon() || (frac == 0.0) == four_wheel {
        if four_wheel && let Some(t) = tag(seat_tag(seat.pos)) {
            let [f, _, u] = t.axis;
            return out(t.origin - f * 12.0 + u * 32.0);
        }
        if let Some(t) = tag("tag_aimdownbarrel") {
            return out(t.origin);
        }
        let t = tag("tag_barrel")?;
        let [f, l, u] = t.axis;
        return out(t.origin + f * 30.0 - l * 3.0 + u * 10.0);
    }
    // The chase camera: the chasecam's offset from the pivot, taken into
    // the pivot's frame and turned by the view.
    let t = tag(pivot_tag(seat.pos))?;
    let [tf, tl, tu] = t.axis;
    let offset = match tag("tag_chasecam") {
        Some(c) => {
            let o = c.origin - t.origin;
            if four_wheel { o * 2.5 } else { o }
        }
        None => tf * -200.0 + tu * 32.0,
    };
    let local = Vec3::new(offset.dot(tf), offset.dot(tl), offset.dot(tu));
    let [rf, rl, ru] = angles_to_axis([view[0], view[1], 0.0]).map(Vec3::from);
    let mut rotated = rf * local.x + rl * local.y + ru * local.z;
    if seat.class == CLASS_TANK {
        rotated.z *= 0.75;
    }
    if four_wheel && frac != 0.0 {
        rotated -= ru * 135.0;
    }
    let len = offset.length();
    let dist = rotated.length();
    if dist > 2.0 * len {
        rotated *= 2.0 * len / dist;
    } else if dist < 0.5 * len && dist > 0.0 {
        rotated *= 0.5 * len / dist;
    }
    out(t.origin + rotated)
}

/// The end of `CG_VehicleView`: a ±8 box from `trace_from` to the camera
/// under mask 0x211, retraced under 0x11 when it started in a 0x200 brush;
/// a hit stops the camera there, `(1 - fraction) * 32` higher, and a second
/// trace from the start brings it back inside.
pub fn clip(cam: &SeatCamera, trace: &mut dyn FnMut(Vec3, Vec3, u32) -> (f32, Vec3, bool)) -> Vec3 {
    let (mut fraction, mut end, startsolid) = trace(cam.trace_from, cam.pos, 0x211);
    if startsolid {
        (fraction, end, _) = trace(cam.trace_from, cam.pos, 0x11);
    }
    if fraction == 1.0 {
        return cam.pos;
    }
    let raised = end + Vec3::Z * ((1.0 - fraction) * 32.0);
    (_, end, _) = trace(cam.trace_from, raised, 0x11);
    end
}

/// The seat the newest snapshot's playerstate sits in, with its vehicle's
/// entity number and angles; `None` on foot or off a UO server.
pub fn of_snapshot(
    p: &vcod_common::net::protocol::Protocol,
    s: &vcod_common::net::snapshot::Snapshot,
) -> Option<(Seat, u32, [f32; 3])> {
    use vcod_common::net::flags::{EF_IN_VEHICLE_V22, ET_VEHICLE_V22};
    let int = |n: &str| s.ps.field_i32(p, n);
    if p.version != 22 || int("eFlags") & EF_IN_VEHICLE_V22 == 0 {
        return None;
    }
    let num = u32::try_from(int("viewlocked_entNum")).ok()?;
    let ent = s
        .entities
        .get(&num)
        .filter(|e| e.field_i32(p, "eType") == ET_VEHICLE_V22)?;
    let seat = Seat {
        pos: int("vehPos"),
        kind: int("vehType"),
        class: ent.field_i32(p, "surfType"),
    };
    Some((seat, num, ent.angles(p)))
}

/// `AxisToAngles`: wire pitch, yaw and roll of a `[forward, left, up]`.
pub fn axis_to_angles(axis: [Vec3; 3]) -> [f32; 3] {
    let [f, l, _] = axis;
    let yaw = f.y.atan2(f.x).to_degrees();
    let pitch = -f.z.atan2((f.x * f.x + f.y * f.y).sqrt()).to_degrees();
    let [_, l0, u0] = angles_to_axis([pitch, yaw, 0.0]).map(Vec3::from);
    let roll = l.dot(u0).atan2(l.dot(l0)).to_degrees();
    [pitch, yaw, roll]
}

/// 0x30040580: a four-wheel's riders but the gunner and the passenger with
/// its own weapon have their view turned with the vehicle, frame to frame:
/// the pitch while the sight is down, the yaw while `eFlags` 0x200000
/// (the mount's look-along-the-hull flag) is set.
#[derive(Default)]
pub struct FourWheelTurn {
    /// The vehicle's axis last frame.
    prev: Option<Mat3>,
}

impl FourWheelTurn {
    /// Forgets the vehicle: out of a seat, or in one this does not turn.
    pub fn reset(&mut self) {
        self.prev = None;
    }

    /// The view turned by the vehicle's turn since the last call, pitch and
    /// yaw degrees; `None` when nothing is written.
    pub fn step(
        &mut self,
        view: [f32; 3],
        vehicle_angles: [f32; 3],
        sight_down: bool,
        hull_yaw: bool,
    ) -> Option<[f32; 2]> {
        let cur = mat(angles_to_axis(vehicle_angles).map(Vec3::from));
        let prev = self.prev.replace(cur)?;
        // Rows are the axes: V' = V * prev^T * cur.
        let v = mat(angles_to_axis(view).map(Vec3::from));
        let turned = v * prev.transpose() * cur;
        let rows = [turned.row(0), turned.row(1), turned.row(2)];
        let [pitch, yaw, _] = axis_to_angles(rows);
        let pitch = if sight_down { view[0] } else { pitch };
        let yaw = if hull_yaw { yaw } else { view[1] };
        Some([pitch, yaw])
    }
}

/// A matrix whose rows are `axis`.
fn mat(axis: [Vec3; 3]) -> Mat3 {
    Mat3::from_cols(axis[0], axis[1], axis[2]).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(origin: Vec3, angles: [f32; 3]) -> TagPose {
        TagPose {
            origin,
            axis: angles_to_axis(angles).map(Vec3::from),
        }
    }

    #[test]
    fn axis_to_angles_inverts_angles_to_axis() {
        for a in [[10.0, 30.0, 0.0], [-20.0, -150.0, 15.0], [5.0, 90.0, -40.0]] {
            let b = axis_to_angles(angles_to_axis(a).map(Vec3::from));
            for i in 0..3 {
                assert!((a[i] - b[i]).abs() < 1e-3, "{a:?} {b:?}");
            }
        }
    }

    /// A tank driver at the hip looks from the chase camera, orbiting
    /// `tag_turret` by the view: turned half round, the camera swings to
    /// the turret's other side.
    #[test]
    fn a_tank_driver_rides_the_chase_camera() {
        let seat = Seat {
            pos: 1,
            kind: 2,
            class: CLASS_TANK,
        };
        let turret = Vec3::new(0.0, 0.0, 100.0);
        let mut tag = |name: &str| match name {
            "tag_turret" => Some(pose(turret, [0.0; 3])),
            "tag_chasecam" => Some(pose(turret + Vec3::new(-200.0, 0.0, 40.0), [0.0; 3])),
            _ => None,
        };
        let ahead = camera(seat, [0.0, 0.0, 0.0], 0.0, Vec3::ZERO, &mut tag).unwrap();
        assert!((ahead.pos - (turret + Vec3::new(-200.0, 0.0, 30.0))).length() < 1e-3);
        let back = camera(seat, [0.0, 180.0, 0.0], 0.0, Vec3::ZERO, &mut tag).unwrap();
        assert!((back.pos - (turret + Vec3::new(200.0, 0.0, 30.0))).length() < 1e-2);
        // Sighted, the driver looks down the barrel.
        let mut tag = |name: &str| match name {
            "tag_aimdownbarrel" => Some(pose(Vec3::new(5.0, 6.0, 7.0), [0.0; 3])),
            _ => None,
        };
        let sighted = camera(seat, [0.0; 3], 1.0, Vec3::ZERO, &mut tag).unwrap();
        assert_eq!(sighted.pos, Vec3::new(5.0, 6.0, 7.0));
    }

    /// The gunner's eye is `tag_secondary_view`, its pitch the gun's, and a
    /// view off the gun's pitch snaps to the gun.
    #[test]
    fn the_gunner_looks_along_its_gun() {
        let seat = Seat {
            pos: 2,
            kind: 2,
            class: CLASS_TANK,
        };
        let mut tag = |name: &str| match name {
            "tag_secondary_gun" => Some(pose(Vec3::ZERO, [-10.0, 45.0, 0.0])),
            "tag_secondary_view" => Some(pose(Vec3::new(1.0, 2.0, 3.0), [0.0; 3])),
            _ => None,
        };
        let on_gun = camera(seat, [-10.0, 45.0, 0.0], 0.0, Vec3::ZERO, &mut tag).unwrap();
        assert_eq!(on_gun.pos, Vec3::new(1.0, 2.0, 3.0));
        assert!(on_gun.snap.is_none());
        let off = camera(seat, [20.0, 90.0, 0.0], 0.0, Vec3::ZERO, &mut tag).unwrap();
        let [p, y] = off.snap.unwrap();
        assert!((p + 10.0).abs() < 1e-3 && (y - 45.0).abs() < 1e-3);
    }

    #[test]
    fn a_clipped_camera_rises_by_what_was_cut() {
        let cam = SeatCamera {
            pos: Vec3::new(100.0, 0.0, 0.0),
            gun_pitch_roll: None,
            snap: None,
            trace_from: Vec3::ZERO,
        };
        let mut wall = |a: Vec3, b: Vec3, _mask: u32| {
            let f = if b.x > 50.0 { 50.0 / (b.x - a.x) } else { 1.0 };
            (f, a + (b - a) * f, false)
        };
        let at = clip(&cam, &mut wall);
        assert_eq!(at.x, 50.0);
        assert!(at.z > 0.0);
    }

    /// A jeep turning 10 degrees left turns the driver's view with it.
    #[test]
    fn a_four_wheel_turns_its_riders() {
        let mut t = FourWheelTurn::default();
        assert!(
            t.step([0.0, 30.0, 0.0], [0.0, 0.0, 0.0], false, true)
                .is_none()
        );
        let [p, y] = t
            .step([0.0, 30.0, 0.0], [0.0, 10.0, 0.0], false, true)
            .unwrap();
        assert!(p.abs() < 1e-3 && (y - 40.0).abs() < 1e-3, "{p} {y}");
        // Without the hull flag the yaw stays.
        let [_, y] = t
            .step([0.0, 30.0, 0.0], [0.0, 20.0, 0.0], false, false)
            .unwrap();
        assert_eq!(y, 30.0);
    }
}
