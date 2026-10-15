//! United Offensive vehicle part poses: what `uo_cgame_mp_x86.dll`'s
//! vehicle DObj callback (0x30020540) sets on a vehicle or a wreck from a
//! snapshot pair, and where a seated player draws
//! (docs/research/coduo-diff.md, section 9, "Client rendering", and 18).

use crate::collision::ModelTri;
use crate::net::msg::EntityState;
use crate::net::protocol::Protocol;
use crate::skeleton::{PoseBuffer, Skeleton};
use crate::turretpose::angles_quat;
use crate::xmodel::XModel;
use glam::{Quat, Vec3};

/// The vehicle file's `type` as the server packs it into `surfType`:
/// `4 wheel` 1, `tank` 2, `artillery` 5 (live captures and 0x300081e0).
pub const KIND_FOUR_WHEEL: i32 = 1;
pub const KIND_TANK: i32 = 2;
pub const KIND_ARTILLERY: i32 = 5;

/// The wire fields the callback reads off one entityState.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleFields {
    /// `origin2[0]` and `origin2[2]`: body pitch and roll, 8 bits of
    /// `127 + 127 * deg / 30`.
    pub body_packed: [i32; 2],
    /// `angles2[0]` and `angles2[1]`: turret pitch and yaw off the hull.
    pub turret: [f32; 2],
    /// `angles2[2]` and `origin2[1]`: the gunner's pitch and yaw.
    pub gunner: [f32; 2],
    /// `leanf`: the front wheels' steer.
    pub steer: f32,
    /// `time`: the tread and tyre clock in ms, 16 bits unsigned.
    pub tread_ms: i32,
    /// `surfType`: the vehicle kind.
    pub kind: i32,
}

impl VehicleFields {
    pub fn read(e: &EntityState, p: &Protocol) -> VehicleFields {
        let f = |n: &str| e.field_f32(p, n);
        let i = |n: &str| e.field_i32(p, n);
        VehicleFields {
            body_packed: [i("origin2[0]"), i("origin2[2]")],
            turret: [f("angles2[0]"), f("angles2[1]")],
            gunner: [f("angles2[2]"), f("origin2[1]")],
            steer: f("leanf"),
            tread_ms: i("time"),
            kind: i("surfType"),
        }
    }
}

/// One frame's control angles (pitch, yaw, roll in degrees) and clocks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehiclePose {
    pub body: [f32; 3],
    pub turret: [f32; 3],
    pub barrel: [f32; 3],
    pub gunner_base: [f32; 3],
    pub gunner_gun: [f32; 3],
    pub steer: f32,
    /// The material clock in seconds: the refEntity's `shaderTime` is
    /// `(cg.time - time) / 1000`, so the scrolling tread and the turning
    /// tyre stages run on the lerped `time` alone (0x30021660).
    pub shader_time: f32,
    pub kind: i32,
}

/// `LerpAngle` (0x3004bd00): `b` brought within 180 of `a`, the result
/// left unwrapped.
fn lerp_angle(a: f32, mut b: f32, f: f32) -> f32 {
    if b - a > 180.0 {
        b -= 360.0;
    }
    if b - a < -180.0 {
        b += 360.0;
    }
    a + (b - a) * f
}

fn unpack_tilt(v: i32) -> f32 {
    (v as f32 - 127.0) * 30.0 / 127.0
}

/// The callback's lerps from `cur` towards `next` by `f`, the snapshot
/// pair's interpolation fraction. A gunner seat the local client sits in
/// takes its view instead (0x300207c0); this is the remote case.
pub fn lerp(cur: &VehicleFields, next: &VehicleFields, f: f32) -> VehiclePose {
    let la = |a: f32, b: f32| lerp_angle(a, b, f);
    let tilt = |i: usize| {
        la(
            unpack_tilt(cur.body_packed[i]),
            unpack_tilt(next.body_packed[i]),
        )
    };
    let t = cur.tread_ms as f32 * 0.001;
    VehiclePose {
        body: [tilt(0), 0.0, tilt(1)],
        turret: [0.0, la(cur.turret[1], next.turret[1]), 0.0],
        barrel: [la(cur.turret[0], next.turret[0]), 0.0, 0.0],
        gunner_base: [0.0, la(cur.gunner[1], next.gunner[1]), 0.0],
        gunner_gun: [la(cur.gunner[0], next.gunner[0]), 0.0, 0.0],
        steer: la(cur.steer, next.steer),
        // a plain lerp of `time * 0.001`, wrap and all
        shader_time: t + (next.tread_ms as f32 * 0.001 - t) * f,
        kind: cur.kind,
    }
}

/// The steering wheel's control: a fixed tilt with the steer as roll, and
/// a 2-unit shift (0x30020b8b).
const STEERING_PITCH: f32 = 210.0;
const STEERING_RATIO: f32 = 0.75;
const STEERING_SHIFT: Vec3 = Vec3::new(2.0, 0.0, 0.0);

/// Sets the control tags on `pose`. A control replaces the bone's local
/// rotation in its parent's frame, and its translation adds to the local
/// one; every vehicle tag binds unrotated, so that is also the bind turned
/// by the control. Missing tags are skipped, as `DObjGetBoneIndex` -1 is.
///
/// Not posed: each wheel's suspension, which traces `time2 / 1000` units
/// either way of the wheel (8 or 24 on the wire, so under 0.03 units), and
/// a front wheel off a zero steer, which takes the same offset.
pub fn apply(pose: &mut PoseBuffer, skel: &Skeleton, v: &VehiclePose) {
    let mut set = |name: &str, angles: [f32; 3], shift: Vec3| {
        if let Some(bi) = skel.bone_index(name) {
            let (pos, _) = pose.local(bi);
            pose.set_local(bi, pos + shift, angles_quat(angles));
        }
    };
    set("tag_body", v.body, Vec3::ZERO);
    set("tag_turret", v.turret, Vec3::ZERO);
    set("tag_barrel", v.barrel, Vec3::ZERO);
    let base = if v.kind == KIND_FOUR_WHEEL {
        "tag_secondary_base"
    } else {
        "tag_secondary_player"
    };
    set(base, v.gunner_base, Vec3::ZERO);
    set("tag_secondary_gun", v.gunner_gun, Vec3::ZERO);
    if v.kind == KIND_FOUR_WHEEL {
        set(
            "tag_steeringwheel",
            [STEERING_PITCH, 0.0, v.steer * STEERING_RATIO],
            STEERING_SHIFT,
        );
    }
    if v.steer != 0.0 {
        for wheel in ["tag_wheel_front_left", "tag_wheel_front_right"] {
            set(wheel, [0.0, v.steer, 0.0], Vec3::ZERO);
        }
    }
}

/// The tag seat `seat` (1 driver, 2 gunner, 3..6 passengers) rides
/// (`BG_GetVehiclePosTag`, cgame 0x30008190).
pub fn seat_tag(seat: i32) -> Option<&'static str> {
    Some(match seat {
        1 => "tag_player",
        2 => "tag_secondary_player",
        3 => "tag_passenger",
        4 => "tag_passenger2",
        5 => "tag_passenger3",
        6 => "tag_passenger4",
        _ => return None,
    })
}

/// A seated player's shift off its seat tag, in the tag's axes (0x300081e0
/// keyed on the vehicle's kind).
pub fn seat_offset(kind: i32, seat: i32) -> Vec3 {
    match (kind, seat) {
        (KIND_ARTILLERY, _) => Vec3::new(0.0, -24.0, 0.0),
        (KIND_TANK, 2) => Vec3::new(10.0, 5.0, -39.0),
        (KIND_TANK, 1) => Vec3::new(0.0, 0.0, -20.0),
        _ => Vec3::ZERO,
    }
}

/// Whether the cgame draws a remote player in `seat` (0x3003392b): a
/// tank's driver sits closed in (and the server never sends one).
pub fn seat_drawn(kind: i32, seat: i32) -> bool {
    !(kind == KIND_TANK && seat == 1)
}

/// Where a seated player draws: the seat tag of the vehicle as posed this
/// frame, shifted by [`seat_offset`], turned with the tag (0x30032fe0).
/// `veh` is the vehicle's drawn origin and rotation, `worlds` its posed
/// model-space bones.
pub fn seat_frame(
    skel: &Skeleton,
    worlds: &[(Vec3, Quat)],
    veh: (Vec3, Quat),
    kind: i32,
    seat: i32,
) -> Option<(Vec3, Quat)> {
    let (lp, lr) = *worlds.get(skel.bone_index(seat_tag(seat)?)?)?;
    let rot = veh.1 * lr;
    Some((veh.0 + veh.1 * lp + rot * seat_offset(kind, seat), rot))
}

/// A model's collision triangles as its posed bones carry them, in the
/// model's own frame: each surface moved off its bone's bind onto the
/// bone's entry in `worlds` (the posed model-space bones). The engine's
/// xmodel clip walks every surface in its bone's posed frame, so a tank's
/// turret stops a round where it is turned (docs/research/coduo-vehicles.md
/// 13). Surfaces with contents 0 meet no mask and are left out.
pub fn posed_collision(model: &XModel, worlds: &[(Vec3, Quat)], out: &mut Vec<ModelTri>) {
    for surf in model.collision.iter().filter(|s| s.contents != 0) {
        let (Some(bind), Some(&(wp, wr))) = (model.bones.get(surf.bone), worlds.get(surf.bone))
        else {
            continue;
        };
        let inv = bind.rot.inverse();
        out.extend(surf.tris.iter().map(|t| ModelTri {
            tri: t.map(|v| wr * (inv * (v - bind.pos)) + wp),
            contents: surf.contents,
            surface_flags: surf.flags,
        }));
    }
}

/// `surfType` on a seated player: the seat in bits 0-2, the vehicle kind
/// in 3-5, `vehMotion` (1 idle, 2 forward, 3 reverse) in 6-7.
pub fn rider_seat(surf_type: i32) -> i32 {
    surf_type & 7
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xmodel::Bone;

    /// A model of `(name, parent, local_pos)` tags, all bound unrotated.
    pub(crate) fn tag_model(tags: &[(&str, i32, Vec3)]) -> XModel {
        let mut bones: Vec<Bone> = Vec::new();
        for &(name, parent, local_pos) in tags {
            let pos = match usize::try_from(parent) {
                Ok(p) => bones[p].pos + local_pos,
                Err(_) => local_pos,
            };
            bones.push(Bone {
                name: name.into(),
                parent,
                pos,
                rot: Quat::IDENTITY,
                local_pos,
                local_rot: Quat::IDENTITY,
                hit_mins: Vec3::ZERO,
                hit_maxs: Vec3::ZERO,
                hit_location: 0,
            });
        }
        XModel {
            lod: "vehicle".into(),
            surfaces: vec![],
            materials: vec![],
            bones,
            collision: Vec::new(),
            lod_dists: vec![0.0],
            coarse: Vec::new(),
        }
    }

    fn fields(kind: i32) -> VehicleFields {
        VehicleFields {
            body_packed: [127, 127],
            turret: [0.0; 2],
            gunner: [0.0; 2],
            steer: 0.0,
            tread_ms: 0,
            kind,
        }
    }

    #[test]
    fn lerp_angle_takes_the_short_way_and_stays_unwrapped() {
        assert_eq!(lerp_angle(170.0, -170.0, 0.5), 180.0);
        assert_eq!(lerp_angle(-170.0, 170.0, 0.5), -180.0);
        assert_eq!(lerp_angle(10.0, 30.0, 0.25), 15.0);
    }

    #[test]
    fn tilt_unpacks_thirty_degrees_either_way() {
        let mut a = fields(KIND_TANK);
        a.body_packed = [254, 0];
        let v = lerp(&a, &a, 0.0);
        assert_eq!(v.body, [30.0, 0.0, -30.0]);
    }

    /// A tank's barrel pitches about its turret's turned axis: the tags
    /// compose in their parents' frames.
    #[test]
    fn a_barrel_pitches_about_its_turned_turret() {
        let m = tag_model(&[
            ("tag_origin", -1, Vec3::ZERO),
            ("tag_body", 0, Vec3::ZERO),
            ("tag_turret", 1, Vec3::new(0.0, 0.0, 80.0)),
            ("tag_barrel", 2, Vec3::new(40.0, 0.0, 20.0)),
        ]);
        let skel = Skeleton::build(&[&m]);
        let mut f = fields(KIND_TANK);
        f.turret = [10.0, 90.0];
        let v = lerp(&f, &f, 0.0);
        let mut pose = PoseBuffer::new(&skel);
        apply(&mut pose, &skel, &v);
        let (bp, br) = pose.bone_world(&skel, 3);
        assert!(bp.abs_diff_eq(Vec3::new(0.0, 40.0, 100.0), 1e-4), "{bp}");
        // forward is turned to +y and dipped 10 degrees
        let fwd = br * Vec3::X;
        let want = Vec3::new(0.0, 10f32.to_radians().cos(), -10f32.to_radians().sin());
        assert!(fwd.abs_diff_eq(want, 1e-5), "{fwd}");
    }

    /// A collision surface rides its bone: a turret's surface turned 90
    /// degrees with the turret lands where the turned bone carries it.
    #[test]
    fn a_turret_surface_turns_with_its_turret() {
        let mut m = tag_model(&[
            ("tag_origin", -1, Vec3::ZERO),
            ("tag_body", 0, Vec3::ZERO),
            ("tag_turret", 1, Vec3::new(0.0, 0.0, 80.0)),
        ]);
        // A triangle 40 ahead of the turret, baked at the bind.
        let tri = [
            Vec3::new(40.0, 0.0, 80.0),
            Vec3::new(40.0, 10.0, 80.0),
            Vec3::new(40.0, 0.0, 90.0),
        ];
        m.collision.push(crate::xmodel::CollSurf {
            contents: 1,
            flags: 0,
            tris: vec![tri],
            bounds: (Vec3::ZERO, Vec3::ZERO),
            bone: 2,
        });
        let skel = Skeleton::build(&[&m]);
        let mut f = fields(KIND_TANK);
        f.turret = [0.0, 90.0];
        let mut pose = PoseBuffer::new(&skel);
        apply(&mut pose, &skel, &lerp(&f, &f, 0.0));
        let mut out = Vec::new();
        posed_collision(&m, &pose.bone_worlds(&skel), &mut out);
        assert_eq!(out.len(), 1);
        assert!(
            out[0].tri[0].abs_diff_eq(Vec3::new(0.0, 40.0, 80.0), 1e-4),
            "{}",
            out[0].tri[0]
        );
        assert!(
            out[0].tri[1].abs_diff_eq(Vec3::new(-10.0, 40.0, 80.0), 1e-4),
            "{}",
            out[0].tri[1]
        );
    }

    /// The tags the pose reads on `mp_vehicle_panzeriv`, at their bind.
    fn panzer() -> XModel {
        tag_model(&[
            ("tag_origin", -1, Vec3::ZERO),
            ("tag_body", 0, Vec3::new(0.0, 0.0, -0.98)),
            ("tag_wheel_front_left", 0, Vec3::new(73.68, 56.29, -0.04)),
            ("tag_turret", 1, Vec3::new(-13.78, 0.0, 78.74)),
            ("tag_secondary_player", 3, Vec3::new(-26.13, 0.17, 51.74)),
            ("turret_recoil", 3, Vec3::new(0.0, 0.0, 1.29)),
            ("tag_barrel", 5, Vec3::new(42.65, 0.0, 21.70)),
            ("tag_secondary_base", 4, Vec3::new(0.14, 0.10, 0.0)),
            ("tag_secondary_gun", 7, Vec3::new(12.84, -0.11, 9.65)),
        ])
    }

    /// The same of `mp_v_us_lnd_willysjeep`.
    fn jeep() -> XModel {
        tag_model(&[
            ("tag_origin", -1, Vec3::ZERO),
            ("tag_body", 0, Vec3::new(-0.5, 0.0, 20.4)),
            ("tag_wheel_front_left", 0, Vec3::new(62.55, 32.29, -0.26)),
            ("tag_turret", 1, Vec3::new(-38.35, 0.10, 50.35)),
            ("tag_steeringwheel", 1, Vec3::new(25.16, 20.14, 24.76)),
            ("tag_secondary_base", 3, Vec3::new(-0.28, -1.24, -44.56)),
            ("tag_secondary_gun", 5, Vec3::new(-0.03, 0.27, 52.92)),
            ("tag_secondary_player", 5, Vec3::new(-13.50, 2.11, 3.79)),
            ("tag_player", 1, Vec3::new(-0.06, 20.29, 16.68)),
        ])
    }

    fn same_rot(got: Quat, want: [f32; 3]) -> bool {
        let w = angles_quat(want);
        got.abs_diff_eq(w, 1e-4) || got.abs_diff_eq(-w, 1e-4)
    }

    /// Range of a value over a run.
    #[derive(Default)]
    struct Span(Option<(f32, f32)>);

    impl Span {
        fn add(&mut self, v: f32) {
            self.0 = Some(self.0.map_or((v, v), |(lo, hi)| (lo.min(v), hi.max(v))));
        }
        fn width(&self) -> f32 {
            self.0.map_or(0.0, |(lo, hi)| hi - lo)
        }
    }

    /// The retail captures of a driven, crewed Panzer IV and Willys jeep
    /// (`snapshots-uo-tank.bin`, `snapshots-uo-jeep.bin`, coduo-diff.md
    /// 18): every frame pair poses the tags off the fields the callback
    /// reads, at the snapshot and halfway to the next, and the gunner's
    /// seat turns with the hull, the turret and the gunner's yaw. The runs
    /// moved every part, so none of it holds vacuously.
    #[test]
    fn posed_tags_follow_the_captured_fields() {
        use crate::net::flags::{EF_IN_VEHICLE_V22, ET_PLAYER, ET_VEHICLE_V22};
        use crate::net::protocol::PROTOCOL_V22;
        use crate::net::snapshot::tests::regate_as;
        let p = &PROTOCOL_V22;
        for (tag, model, kind) in [
            ("uo-tank", panzer(), KIND_TANK),
            ("uo-jeep", jeep(), KIND_FOUR_WHEEL),
        ] {
            let skel = Skeleton::build(&[&model]);
            let bi = |n: &str| skel.bone_index(n).unwrap();
            // (fields, hull angles, the gunner's seat byte when it is in the frame)
            let mut frames: Vec<(VehicleFields, [f32; 3], Option<i32>)> = Vec::new();
            regate_as(
                p,
                &format!("net/gamestate-{tag}.bin"),
                &format!("net/snapshots-{tag}.bin"),
                false,
                |s| {
                    let Some((&num, e)) = s
                        .entities
                        .iter()
                        .find(|(_, e)| e.field_i32(p, "eType") == ET_VEHICLE_V22)
                    else {
                        return;
                    };
                    let rider = s.entities.values().find(|r| {
                        r.field_i32(p, "eType") == ET_PLAYER
                            && r.field_i32(p, "eFlags") & EF_IN_VEHICLE_V22 != 0
                            && r.field_i32(p, "otherEntityNum") == num as i32
                    });
                    let ang = ["apos.trBase[0]", "apos.trBase[1]", "apos.trBase[2]"]
                        .map(|n| e.field_f32(p, n));
                    frames.push((
                        VehicleFields::read(e, p),
                        ang,
                        rider.map(|r| r.field_i32(p, "surfType")),
                    ));
                },
            );
            assert!(frames.len() > 700, "{tag}: {} frames", frames.len());
            let (mut turret, mut gunner, mut tilt, mut steer, mut tread) = (
                Span::default(),
                Span::default(),
                Span::default(),
                Span::default(),
                Span::default(),
            );
            let mut seated = 0;
            for w in frames.windows(2) {
                let ((a, hull, rider), (b, _, _)) = (&w[0], &w[1]);
                assert_eq!(a.kind, kind, "{tag}");
                let v = lerp(a, b, 0.0);
                let mut pose = PoseBuffer::new(&skel);
                apply(&mut pose, &skel, &v);
                let worlds = pose.bone_worlds(&skel);
                let local = |n: &str| pose.local(bi(n)).1;
                let tilt_of = |i: usize| unpack_tilt(a.body_packed[i]);
                assert!(
                    same_rot(local("tag_body"), [tilt_of(0), 0.0, tilt_of(1)]),
                    "{tag}"
                );
                assert!(
                    same_rot(local("tag_turret"), [0.0, a.turret[1], 0.0]),
                    "{tag}"
                );
                assert!(
                    same_rot(local("tag_secondary_gun"), [a.gunner[0], 0.0, 0.0]),
                    "{tag}"
                );
                assert_eq!(v.shader_time, a.tread_ms as f32 * 0.001);
                if kind == KIND_TANK {
                    assert!(same_rot(local("tag_barrel"), [a.turret[0], 0.0, 0.0]));
                    assert!(same_rot(
                        local("tag_secondary_player"),
                        [0.0, a.gunner[1], 0.0]
                    ));
                } else {
                    assert!(same_rot(
                        local("tag_secondary_base"),
                        [0.0, a.gunner[1], 0.0]
                    ));
                    assert!(same_rot(
                        local("tag_steeringwheel"),
                        [210.0, 0.0, a.steer * 0.75]
                    ));
                    if a.steer != 0.0 {
                        assert!(same_rot(local("tag_wheel_front_left"), [0.0, a.steer, 0.0]));
                    }
                }
                // Halfway, every angle sits between the pair's.
                let h = lerp(a, b, 0.5);
                let mid = |x: f32, y: f32, m: f32| {
                    let y = x + ((y - x + 540.0).rem_euclid(360.0) - 180.0);
                    (m - (x + y) / 2.0).abs() < 1e-3
                };
                assert!(mid(a.turret[1], b.turret[1], h.turret[1]), "{tag}");
                assert!(mid(a.gunner[1], b.gunner[1], h.gunner_base[1]), "{tag}");
                assert!(mid(a.steer, b.steer, h.steer), "{tag}");
                // The gunner's seat turns with the hull, turret and gunner.
                if let Some(surf) = rider {
                    assert_eq!(rider_seat(*surf), 2, "{tag}");
                    assert_eq!((surf >> 3) & 7, kind, "{tag}");
                    let hull_rot = angles_quat(*hull);
                    let (_, seat_rot) =
                        seat_frame(&skel, &worlds, (Vec3::ZERO, hull_rot), kind, 2).unwrap();
                    let yaw = |q: Quat| {
                        let f = q * Vec3::X;
                        f.y.atan2(f.x).to_degrees()
                    };
                    let mut d = yaw(seat_rot) - (hull[1] + a.turret[1] + a.gunner[1]);
                    d = (d + 540.0).rem_euclid(360.0) - 180.0;
                    // the hull and body tilt lean the seat a few degrees
                    assert!(d.abs() < 3.0, "{tag}: seat yaw off by {d}");
                    seated += 1;
                }
                turret.add(a.turret[1]);
                gunner.add(a.gunner[1]);
                tilt.add(tilt_of(0));
                steer.add(a.steer);
                tread.add(a.tread_ms as f32);
            }
            assert!(seated > 600, "{tag}: the gunner sat in {seated} frames");
            assert!(
                gunner.width() > 90.0,
                "{tag}: gunner yaw {}",
                gunner.width()
            );
            assert!(tilt.width() > 2.0, "{tag}: body pitch {}", tilt.width());
            assert!(tread.width() > 10_000.0, "{tag}: tread {}", tread.width());
            if kind == KIND_TANK {
                assert!(
                    turret.width() > 20.0,
                    "{tag}: turret yaw {}",
                    turret.width()
                );
            } else {
                let (lo, hi) = steer.0.unwrap();
                assert!(lo < -30.0 && hi > 30.0, "{tag}: steer {lo}..{hi}");
            }
        }
    }
}
