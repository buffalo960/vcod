//! Vector and scalar math builtins. None of these touch the entity table or
//! the configstrings, so every one takes its arguments and returns a value
//! with no `GameHost` state to consult.

use crate::game::host::GameHost;
use vcod_gsc::{Cx, ErrorKind, Target, Value};

pub type Builtin = fn(&mut GameHost, &mut Cx, Option<Target>, &[Value]) -> Result<Value, ErrorKind>;

pub const NAMES: &[(&str, Builtin)] = &[
    ("distance", distance),
    ("length", length),
    ("vectornormalize", vector_normalize),
    ("vectortoangles", vector_to_angles),
    ("anglestoforward", angles_to_forward),
    ("anglestoright", angles_to_right),
    ("anglestoup", angles_to_up),
    ("randomfloat", random_float),
    ("sin", sin),
    ("cos", cos),
    ("tan", tan),
    ("asin", asin),
    ("acos", acos),
    ("atan", atan),
];

pub fn lookup(folded: &str) -> Option<Builtin> {
    NAMES.iter().find(|(n, _)| *n == folded).map(|(_, f)| *f)
}

fn vec_len(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// `distance(a, b)`: the straight-line distance between two points.
pub fn distance(
    _host: &mut GameHost,
    _cx: &mut Cx,
    _recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let (Some(Value::Vector(a)), Some(Value::Vector(b))) = (args.first(), args.get(1)) else {
        return Err(ErrorKind::BadType("distance takes two vectors"));
    };
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    Ok(Value::Float(vec_len(d)))
}

/// `length(v)`: the vector's own magnitude.
pub fn length(
    _host: &mut GameHost,
    _cx: &mut Cx,
    _recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let Some(Value::Vector(v)) = args.first() else {
        return Err(ErrorKind::BadType("length takes a vector"));
    };
    Ok(Value::Float(vec_len(*v)))
}

/// `vectorNormalize(v)`: a unit vector in the same direction. A zero-length
/// input returns `(0,0,0)` rather than dividing by zero, the Q3/RTCW
/// lineage's `VectorNormalize` convention; not measured against retail, no
/// probe covers a zero-length `vectorNormalize` call yet.
pub fn vector_normalize(
    _host: &mut GameHost,
    _cx: &mut Cx,
    _recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let Some(Value::Vector(v)) = args.first() else {
        return Err(ErrorKind::BadType("vectorNormalize takes a vector"));
    };
    let len = vec_len(*v);
    if len == 0.0 {
        return Ok(Value::Vector([0.0, 0.0, 0.0]));
    }
    Ok(Value::Vector([v[0] / len, v[1] / len, v[2] / len]))
}

/// `vectorToAngles(v)`: `vectoangles` (1.1d 0x3de94). Unlike Q3's, the
/// pitch is `atan2(z, |xy|) * -180 / PI` with 360 added while negative, so
/// both angles come back in [0, 360): a vector 16 degrees down reads 16,
/// one up 345, straight up 270 and straight down 90 (`.rodata` 0x72ae8,
/// 0x72b08). Each angle is stored as a float before the 360 is added.
/// Roll is 0. Measured: `client-probes/probe_dmgtouch` on retail UO logs
/// the aim it builds this way (cod11-combat.md 14.8).
pub fn vector_to_angles(
    _host: &mut GameHost,
    _cx: &mut Cx,
    _recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let Some(Value::Vector(v)) = args.first() else {
        return Err(ErrorKind::BadType("vectorToAngles takes a vector"));
    };
    Ok(Value::Vector(vectoangles(*v)))
}

/// `vectoangles` on the x87: the atans in double, each angle stored as a
/// float and then wrapped into [0, 360). The engine's own callers use it
/// too: a missile's launch angles and the damage feedback's two bytes.
pub fn vectoangles(v: [f32; 3]) -> [f32; 3] {
    if v[0] == 0.0 && v[1] == 0.0 {
        let pitch = if v[2] > 0.0 { 270.0 } else { 90.0 };
        return [pitch, 0.0, 0.0];
    }
    let wrap = |a: f64| {
        let a = a as f32;
        if a < 0.0 {
            (a as f64 + 360.0) as f32
        } else {
            a
        }
    };
    let (x, y, z) = (v[0] as f64, v[1] as f64, v[2] as f64);
    let yaw = wrap(y.atan2(x) * 180.0 / std::f64::consts::PI);
    let fwd = (x * x + y * y).sqrt() as f32 as f64;
    let pitch = wrap(z.atan2(fwd) * -180.0 / std::f64::consts::PI);
    [pitch, yaw, 0.0]
}

/// `vectoyaw` (1.1d 0x3dccc): the atan in double, stored as a float, and
/// 360 added on the stack when it is negative, so a caller that truncates
/// it reads the sum unrounded. 0 for a vector with no horizontal part.
pub(crate) fn vectoyaw(v: [f32; 3]) -> f64 {
    if v[1] == 0.0 && v[0] == 0.0 {
        return 0.0;
    }
    let yaw = (f64::from(v[1]).atan2(f64::from(v[0])) * 180.0 / std::f64::consts::PI) as f32;
    if yaw < 0.0 {
        f64::from(yaw) + 360.0
    } else {
        f64::from(yaw)
    }
}

/// `anglesToForward(angles)`: `AngleVectors`'s forward half, same lineage as
/// `vectorToAngles` above and its inverse on a unit vector.
pub fn angles_to_forward(
    _host: &mut GameHost,
    _cx: &mut Cx,
    _recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let Some(Value::Vector(a)) = args.first() else {
        return Err(ErrorKind::BadType("anglesToForward takes an angles vector"));
    };
    let (sp, cp) = a[0].to_radians().sin_cos();
    let (sy, cy) = a[1].to_radians().sin_cos();
    Ok(Value::Vector([cp * cy, cp * sy, -sp]))
}

/// `anglesToRight(angles)` and `anglesToUp(angles)`: `AngleVectors`' right
/// and up.
fn angle_vectors(args: &[Value]) -> Result<[[f32; 3]; 3], ErrorKind> {
    let Some(Value::Vector(a)) = args.first() else {
        return Err(ErrorKind::BadType("anglesTo* takes an angles vector"));
    };
    let (sp, cp) = a[0].to_radians().sin_cos();
    let (sy, cy) = a[1].to_radians().sin_cos();
    let (sr, cr) = a[2].to_radians().sin_cos();
    Ok([
        [cp * cy, cp * sy, -sp],
        [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp],
        [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp],
    ])
}

pub fn angles_to_right(
    _host: &mut GameHost,
    _cx: &mut Cx,
    _recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    Ok(Value::Vector(angle_vectors(args)?[1]))
}

pub fn angles_to_up(
    _host: &mut GameHost,
    _cx: &mut Cx,
    _recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    Ok(Value::Vector(angle_vectors(args)?[2]))
}

/// `randomFloat(range)`: a real uniform draw in `[0, range)` from
/// `GameHost::rng`. Retail seeds `randomFloat` from the level clock;
/// nothing here needs to match its sequence, only to actually cover the
/// range, which the fixed-seed xorshift64* draw does.
pub fn random_float(
    host: &mut GameHost,
    _cx: &mut Cx,
    _recv: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let range = match args.first() {
        Some(Value::Int(i)) => *i as f32,
        Some(Value::Float(f)) => *f,
        _ => return Err(ErrorKind::BadType("randomFloat takes a number")),
    };
    Ok(Value::Float(host.rand_unit() * range))
}

/// `Scr_GetFloat(0)`: an int reads as its value.
fn float_arg(args: &[Value], usage: &'static str) -> Result<f32, ErrorKind> {
    match args.first() {
        Some(Value::Int(i)) => Ok(*i as f32),
        Some(Value::Float(f)) => Ok(*f),
        _ => Err(ErrorKind::BadType(usage)),
    }
}

/// Degrees to radians the way the builtins do it, `x * M_PI / 180` in
/// doubles (1.1 0x5e5ef; UO 0x6b551 is the same table row).
fn radians(deg: f32) -> f64 {
    deg as f64 * std::f64::consts::PI / 180.0
}

/// An `asin`/`acos`/`atan` result back to degrees: the f32 radians times
/// 180 (a float) over `M_PI` (a double), stored as a float (1.1 0x5e73f).
fn degrees(rad: f64) -> Value {
    Value::Float(((rad as f32 * 180.0) as f64 / std::f64::consts::PI) as f32)
}

/// `asin`/`acos` refuse anything outside [-1, 1] with `"%g out of range"`
/// (1.1 .rodata 0x77500).
fn unit_arg(args: &[Value], usage: &'static str) -> Result<f32, ErrorKind> {
    let x = float_arg(args, usage)?;
    if !(-1.0..=1.0).contains(&x) {
        return Err(ErrorKind::Custom(format!(
            "{} out of range",
            vcod_gsc::format_g(x)
        )));
    }
    Ok(x)
}

/// `sin(degrees)` (1.1 0x5e5dc, UO 0x6b551).
pub fn sin(
    _: &mut GameHost,
    _: &mut Cx,
    _: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let x = float_arg(args, "sin takes a number")?;
    Ok(Value::Float(radians(x).sin() as f32))
}

/// `cos(degrees)` (1.1 0x5e618, UO 0x6b59b).
pub fn cos(
    _: &mut GameHost,
    _: &mut Cx,
    _: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let x = float_arg(args, "cos takes a number")?;
    Ok(Value::Float(radians(x).cos() as f32))
}

/// `tan(degrees)` (1.1 0x5e654, UO 0x6b5e5): sine over cosine of the f32
/// radians, `"divide by 0"` (0x774f4) when the cosine is 0.
pub fn tan(
    _: &mut GameHost,
    _: &mut Cx,
    _: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let r = radians(float_arg(args, "tan takes a number")?) as f32;
    let (s, c) = (r.sin(), r.cos());
    if c == 0.0 {
        return Err(ErrorKind::Custom("divide by 0".into()));
    }
    Ok(Value::Float(s / c))
}

/// `asin(x)` in degrees (1.1 0x5e6d0, UO 0x6b667): `atan2(x, sqrt(1 - x*x))`.
pub fn asin(
    _: &mut GameHost,
    _: &mut Cx,
    _: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let x = unit_arg(args, "asin takes a number")? as f64;
    Ok(degrees(x.atan2((1.0 - x * x).sqrt())))
}

/// `acos(x)` in degrees (1.1 0x5e768, UO 0x6b6fe): `atan2(sqrt(1 - x*x), x)`.
pub fn acos(
    _: &mut GameHost,
    _: &mut Cx,
    _: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let x = unit_arg(args, "acos takes a number")? as f64;
    Ok(degrees((1.0 - x * x).sqrt().atan2(x)))
}

/// `atan(x)` in degrees (1.1 0x5e804, UO 0x6b795).
pub fn atan(
    _: &mut GameHost,
    _: &mut Cx,
    _: Option<Target>,
    args: &[Value],
) -> Result<Value, ErrorKind> {
    let x = float_arg(args, "atan takes a number")?;
    Ok(degrees((x as f64).atan()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testing::fixture;

    /// Retail's math builtins, checked against values the retail captures
    /// pin where they exist and against the definition where they do not.
    #[test]
    fn the_vector_builtins_do_what_their_names_say() {
        let (mut vm, mut host) = fixture();
        vm.with_cx(|cx| {
            let a = Value::Vector([0.0, 0.0, 0.0]);
            let b = Value::Vector([3.0, 4.0, 0.0]);
            assert_eq!(
                distance(&mut host, cx, None, &[a, b]).unwrap(),
                Value::Float(5.0)
            );
            assert_eq!(
                length(&mut host, cx, None, &[b]).unwrap(),
                Value::Float(5.0)
            );
            let Value::Vector(n) = vector_normalize(&mut host, cx, None, &[b]).unwrap() else {
                panic!()
            };
            assert!((n[0] - 0.6).abs() < 1e-6 && (n[1] - 0.8).abs() < 1e-6);
        });
    }

    /// Retail's wrap: down is a small positive pitch, up wraps past 270.
    #[test]
    fn vector_to_angles_wraps_both_angles_like_retail() {
        assert_eq!(vectoangles([0.0, 0.0, 1.0]), [270.0, 0.0, 0.0]);
        assert_eq!(vectoangles([0.0, 0.0, -1.0]), [90.0, 0.0, 0.0]);
        let down = vectoangles([-300.0, 0.0, -85.92]);
        assert!(
            (down[0] - 15.99).abs() < 0.01 && down[1] == 180.0,
            "{down:?}"
        );
        let up = vectoangles([0.0, -1.0, 1.0]);
        assert!((up[0] - 315.0).abs() < 1e-4 && (up[1] - 270.0).abs() < 1e-4);
    }

    /// `vectorToAngles` and `anglesToForward` are inverses on a unit vector.
    #[test]
    fn vector_to_angles_round_trips_through_angles_to_forward() {
        let (mut vm, mut host) = fixture();
        vm.with_cx(|cx| {
            let v = Value::Vector([1.0, 0.0, 0.0]);
            let ang = vector_to_angles(&mut host, cx, None, &[v]).unwrap();
            let Value::Vector(f) = angles_to_forward(&mut host, cx, None, &[ang]).unwrap() else {
                panic!()
            };
            assert!((f[0] - 1.0).abs() < 1e-5 && f[1].abs() < 1e-5 && f[2].abs() < 1e-5);
        });
    }

    /// A real uniform draw, not a truncated one: several hundred calls to
    /// `randomFloat(1)` must all land in `[0, 1)` and at least one must
    /// exceed 0.5. A generator that saturates well below 1 (as dividing a
    /// `subsec_nanos` draw by `u32::MAX` once did, capping every draw near
    /// 0.23) would fail the second assertion.
    #[test]
    fn randomfloat_draws_a_real_uniform_value_in_range() {
        let (mut vm, mut host) = fixture();
        vm.with_cx(|cx| {
            let mut saw_above_half = false;
            for _ in 0..500 {
                let Value::Float(f) = random_float(&mut host, cx, None, &[Value::Int(1)]).unwrap()
                else {
                    panic!()
                };
                assert!((0.0..1.0).contains(&f), "{f} out of [0, 1)");
                saw_above_half |= f > 0.5;
            }
            assert!(
                saw_above_half,
                "500 draws of randomFloat(1) never exceeded 0.5"
            );
        });
    }

    /// The trig builtins take and give degrees, and the inverse ones refuse
    /// a value off [-1, 1] as retail does.
    #[test]
    fn the_trig_builtins_work_in_degrees() {
        let (mut vm, mut host) = fixture();
        vm.with_cx(|cx| {
            let f = |v: Result<Value, ErrorKind>| match v.unwrap() {
                Value::Float(f) => f,
                other => panic!("{other:?}"),
            };
            assert!((f(sin(&mut host, cx, None, &[Value::Int(30)])) - 0.5).abs() < 1e-6);
            assert!((f(cos(&mut host, cx, None, &[Value::Int(60)])) - 0.5).abs() < 1e-6);
            assert!((f(tan(&mut host, cx, None, &[Value::Float(45.0)])) - 1.0).abs() < 1e-6);
            assert!((f(asin(&mut host, cx, None, &[Value::Float(0.5)])) - 30.0).abs() < 1e-4);
            assert!((f(acos(&mut host, cx, None, &[Value::Float(0.5)])) - 60.0).abs() < 1e-4);
            assert!((f(atan(&mut host, cx, None, &[Value::Int(1)])) - 45.0).abs() < 1e-4);
            assert_eq!(
                acos(&mut host, cx, None, &[Value::Int(2)]),
                Err(ErrorKind::Custom("2 out of range".into()))
            );
        });
    }
}
