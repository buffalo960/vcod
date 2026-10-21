//! Vector arithmetic rounded the way the 1.1 binaries' x87 code rounds it:
//! every intermediate kept on the 80-bit stack (an `f64` here) and one
//! rounding to `f32` where the binary stores. The game module's
//! `VectorNormalize` (game.mp 0x3d8d8) and `CrossProduct` (0x3d86c) and the
//! engine's copies (`cod_lnxded` 0x8065a38, 0x80659cc) round alike.

use glam::{DVec3, Vec3};

/// A dot product left on the stack.
pub fn dot(a: Vec3, b: Vec3) -> f64 {
    let (a, b) = (a.as_dvec3(), b.as_dvec3());
    (a.x * b.x + a.y * b.y) + a.z * b.z
}

/// `CrossProduct`: each component rounded once.
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    a.as_dvec3().cross(b.as_dvec3()).as_vec3()
}

/// `VectorNormalize2`: the length stored as a float, each component
/// multiplied by its unrounded reciprocal and rounded once. A zero vector
/// comes out `+0.0` in every component (game.mp 0x3da68).
pub fn normalize(v: Vec3) -> Vec3 {
    let len = dot(v, v).sqrt() as f32;
    if len == 0.0 {
        return Vec3::ZERO;
    }
    (v.as_dvec3() * (1.0 / f64::from(len))).as_vec3()
}

/// `VectorNormalize`, in place: as [`normalize`], but a zero vector is left
/// as it was, signed zeros and all (game.mp 0x3d8d8).
pub fn normalize_in_place(v: Vec3) -> Vec3 {
    if dot(v, v).sqrt() as f32 == 0.0 {
        return v;
    }
    normalize(v)
}

/// `VectorScale` by a factor left on the stack.
pub fn scale(v: Vec3, s: f64) -> Vec3 {
    (v.as_dvec3() * DVec3::splat(s)).as_vec3()
}

/// 1.5b's `VectorNormalize` in place (game.mp 0x32f73), which United
/// Offensive shares: the squared length, the length and its reciprocal each
/// stored as a float, every component rounded once. A zero vector is left
/// as it was.
pub fn normalize_stored(v: Vec3) -> Vec3 {
    let len = f64::from(dot(v, v) as f32).sqrt() as f32;
    if len == 0.0 {
        return v;
    }
    let inv = 1.0 / len;
    (v.as_dvec3() * f64::from(inv)).as_vec3()
}

/// The engine's `AnglesToAxis` as `CM_TransformedBoxTrace` builds it
/// (`cod_lnxded` `AngleVectors` 0x8063388, the right inverted by 0x8065c38):
/// each angle times the double `M_PI * 2 / 360` stored as a float, its
/// `fsincos` stored as floats, and each axis component's sum of products
/// kept on the stack and stored once. Rounding every product to a float
/// instead turns a pitched, yawed and rolled model by a few ulps, and a
/// slide down its face ends thousandths short (movers doc 22).
pub fn angles_to_axis(angles: Vec3) -> [Vec3; 3] {
    let sin_cos = |a: f32| {
        let r = f64::from((f64::from(a) * (std::f64::consts::PI * 2.0 / 360.0)) as f32);
        (f64::from(r.sin() as f32), f64::from(r.cos() as f32))
    };
    let (sp, cp) = sin_cos(angles.x);
    let (sy, cy) = sin_cos(angles.y);
    let (sr, cr) = sin_cos(angles.z);
    let forward = DVec3::new(cp * cy, cp * sy, -sp);
    let left = DVec3::new(sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, sr * cp);
    let up = DVec3::new(cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp);
    [forward.as_vec3(), left.as_vec3(), up.as_vec3()]
}
