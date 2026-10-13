//! Particle simulation: instantiates a parsed `.efx` [`efx::Effect`],
//! steps it, and flattens it into [`FxQuad`]/[`FxLight`] data. No wgpu, no
//! wire knowledge. Engine semantics: `docs/research/efx-grammar.md`, R1 to R10.

use crate::fx::efx::{self, Effect, Emitter, R1, R3};
use crate::marks::MarkRequest;
use glam::Vec3;
use std::collections::{HashMap, HashSet, VecDeque};
use vcod_common::collision::{CONTENTS_SOLID, CollisionWorld};
use vcod_common::pk3::Pk3Fs;
use vcod_common::pmove::aim::angles_to_axis;

pub const MAX_PARTICLES: usize = 2048;
/// How many effects deep a chain of `playfx`, `emitfx`, `impactfx` and
/// `deathfx` may run. Retail has no guard but its 1800-slot pool; the
/// stock chains go 5 deep at most, and `fx/explosions/moltov_hit.efx`
/// replays itself on impact (grammar doc R10).
pub const MAX_CHAIN_DEPTH: u8 = 8;
/// `CEmitter::Update` walks its path in 12 ms steps (CoDMP.exe 0x48fd10).
const EMIT_STEP_MS: i64 = 12;
/// Fx lights the prop vertex shader takes, nearest the camera (`vs_prop`).
pub const MAX_LIGHTS: usize = 8;
/// `RE_AddLightToScene`'s queue (0x4e9b00): later lights in a frame drop.
pub const MAX_SCENE_LIGHTS: usize = 32;

// Bullet tracers are hardcoded in the retail client, not an `.efx`:
// CG_Tracer @ cgame_mp_x86.dll 0x30039590, CG_DrawTracer @ 0x300390c0.
// Cvar table and segment setup: grammar doc R8. Addresses below are cgame VAs.

/// `cg_tracerchance` default, cvar row @ 0x30074e54.
const TRACER_CHANCE: f32 = 0.4;
/// `cg_tracerwidth` default, cvar row @ 0x30074e64. A half-width.
const TRACER_WIDTH: f32 = 0.8;
/// `cg_tracerSpeed` default, cvar row @ 0x30074e74. Units/sec.
const TRACER_SPEED: f32 = 4500.0;
/// `cg_tracerlength` default, cvar row @ 0x30074e84. The full streak length.
const TRACER_LENGTH: f32 = 160.0;
/// Flesh branch only: shorter shots draw nothing (`ds:0x30069524` at 0x30039370).
const TRACER_FLESH_MIN_DIST: f32 = 100.0;
/// Flesh branch only: the segment's near end is drawn from
/// `[50, dist - 10)` (`ds:0x3006958c`, `ds:0x30069590` at 0x300393a0).
const TRACER_FLESH_NEAR: f32 = 50.0;
const TRACER_FLESH_SPREAD_CUT: f32 = 60.0;
/// Registered in CG_RegisterGraphics @ 0x30020da0.
const TRACER_SHADER: &str = "gfx/misc/tracer";

/// Where an effect is planted; decides emitter orientation.
#[derive(Clone, Copy, Debug)]
pub enum SpawnAt {
    /// No direction: `EV_PLAY_FX` and an `ET_FX` entity with a zero
    /// `origin2`, which play on [`UP_AXIS`].
    Point {
        pos: Vec3,
    },
    Surface {
        pos: Vec3,
        normal: Vec3,
    },
    Directed {
        pos: Vec3,
        dir: Vec3,
    },
    /// On a tag's own axis, `[forward, left, up]`: syscall 0xe5, what
    /// `EV_PLAY_FX_ON_TAG` plays through (docs/research/efx-grammar.md R10).
    Oriented {
        pos: Vec3,
        axis: [Vec3; 3],
    },
}

/// A tag `playFxOnTag` played on: the entity, and the event's parm, which
/// names the tag through configstring `CS_FX_TAGS + slot`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TagBolt {
    pub entity: u32,
    pub slot: i32,
}

/// A tag's world pose this frame: origin and `[forward, left, up]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TagPose {
    pub origin: Vec3,
    pub axis: [Vec3; 3],
}

/// What a played effect is bolted to (the bolt record of R10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Bolt {
    /// A shooter's muzzle, by [`FxSystem::set_bolts`] key.
    Muzzle(u32),
    Tag(TagBolt),
}

/// Verts in world space, ring order for the [0,1,2, 0,2,3] index pattern.
pub struct FxQuad {
    pub verts: [[f32; 3]; 4],
    pub uvs: [[f32; 2]; 4],
    pub rgba: [f32; 4],
    pub shader: String,
}

pub struct FxLight {
    pub pos: [f32; 3],
    pub radius: f32,
    pub rgb: [f32; 3],
}

/// A drawn `Emitter` debris model (`CEmitter::Draw`, CoDMP.exe 0x48f9c0):
/// the model at `origin` with its axes `[forward, left, up]` scaled by
/// the element's `size` (grammar doc R9).
#[derive(Clone, Debug, PartialEq)]
pub struct FxModel {
    pub model: String,
    pub origin: Vec3,
    pub axis: [Vec3; 3],
}

/// A `Sound` block cue for the audio system.
#[derive(Clone, Debug, PartialEq)]
pub struct FxSound {
    pub alias: String,
    pub pos: Vec3,
    pub delay_s: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum Behavior {
    Billboard,
    Decal,
    Stretch,
    Light,
    /// `Emitter`: an optional debris model, physics, no sprite (R9).
    Emitter,
    /// `FxRunner`: starts its `playfx` effects, draws nothing (R10).
    Runner,
    NoOp,
}

/// `OrientedParticle` and `Cylinder` draw as camera-facing sprites. `Line`'s
/// `origin2` is not parsed, so it stretches along velocity like `Tail`.
/// `Sound` is handled by [`spawn_sound_cues`]. An `Emitter` flies its
/// model and plays its `emitfx` along the way.
fn classify(kind: &str) -> Behavior {
    match kind {
        "Particle" | "OrientedParticle" | "Cylinder" => Behavior::Billboard,
        "Decal" => Behavior::Decal,
        "Tail" | "Line" => Behavior::Stretch,
        "Light" => Behavior::Light,
        "Emitter" => Behavior::Emitter,
        "FxRunner" => Behavior::Runner,
        _ => Behavior::NoOp,
    }
}

struct Particle {
    spawn: f32, // absolute seconds, after delay
    life: f32,  // seconds
    /// Bolt-local while `bolt` is set, like `vel` and `accel`.
    pos: Vec3,
    vel: Vec3,
    /// `acceleration` with `gravity` added on z, world space.
    accel: Vec3,
    rot: f32, // degrees
    rot_delta: f32,
    /// Half-extent at spawn and at death (grammar doc R4).
    size0: f32,
    size1: f32,
    len0: f32,
    len1: f32, // 0 = billboard, else stretch backwards along vel
    alpha0: f32,
    alpha1: f32,
    rgb0: Vec3,
    rgb1: Vec3,
    physics: bool,
    impact_kills: bool,
    /// `bounce`: the share of the reflected velocity a hit keeps.
    elasticity: f32,
    /// Played where the particle hits, until it comes to rest.
    impact_fx: Option<String>,
    /// Played where its life runs out (R10).
    death_fx: Option<String>,
    shader: String,
    is_light: bool,
    /// `Some` for an `Emitter` element, which draws no sprite.
    debris: Option<Debris>,
    /// An `Emitter`'s `emitfx` state.
    emit: Option<Emit>,
    /// How many effects up the chain this particle's effect sits.
    depth: u8,
    /// A `relative` element of an effect played on a tag rides that tag
    /// (R10).
    bolt: Option<Bolt>,
}

/// `CEmitter`'s emit cursor (CoDMP.exe 0x48fcd4, grammar doc R10): the
/// path it walks is its spawn origin and velocity under its current
/// acceleration, not its traced position, so bounces do not move it.
struct Emit {
    fx: String,
    pos: Vec3,
    vel: Vec3,
    last_ms: i64,
    /// Units to the next play.
    spacing: f32,
    density: f32,
    variance: f32,
}

/// An effect another one starts: an `FxRunner`'s `playfx`, an `Emitter`'s
/// `emitfx`, a hit's `impactfx`, a death's `deathfx`.
struct Chain {
    due: f32,
    path: String,
    at: SpawnAt,
    depth: u8,
    bolt: Option<Bolt>,
}

/// A tag an effect was played on, this frame: its origin and axis. A
/// muzzle's axis is the frame [`tangent_basis`] builds on its forward, as
/// a `Directed` spawn has; a `playFxOnTag` tag's is its own.
#[derive(Clone, Copy)]
struct BoltFrame {
    origin: Vec3,
    axis: (Vec3, Vec3, Vec3),
}

impl BoltFrame {
    fn new((origin, dir): (Vec3, Vec3)) -> BoltFrame {
        BoltFrame {
            origin,
            axis: tangent_basis(dir.normalize_or_zero()),
        }
    }

    fn tag(pose: TagPose) -> BoltFrame {
        let [x, y, z] = pose.axis;
        BoltFrame {
            origin: pose.origin,
            axis: (x, y, z),
        }
    }

    fn rotate(&self, v: Vec3) -> Vec3 {
        let (x, y, z) = self.axis;
        x * v.x + y * v.y + z * v.z
    }

    fn unrotate(&self, v: Vec3) -> Vec3 {
        let (x, y, z) = self.axis;
        Vec3::new(v.dot(x), v.dot(y), v.dot(z))
    }
}

/// An `Emitter` element's own state (`CEmitter`, CoDMP.exe 0x4a02c0).
struct Debris {
    model: Option<String>,
    /// Euler angles, degrees.
    angles: Vec3,
    /// Degrees per 100 ms (0x490300 scales by `frametime * 0.01`).
    angle_delta: Vec3,
}

/// xorshift64. Also used by `audio::AudioSystem`.
pub(crate) struct Rng(u64);

impl Rng {
    /// `seed` is forced non-zero; xorshift64 is stuck at 0.
    pub(crate) fn new(seed: u64) -> Rng {
        Rng(seed.max(1))
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub(crate) fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub(crate) fn range(&mut self, a: f32, b: f32) -> f32 {
        a + self.next_f32() * (b - a)
    }
}

fn sample_r1(rng: &mut Rng, r: R1) -> f32 {
    rng.range(r.a, r.b)
}

fn sample_r3(rng: &mut Rng, r: R3) -> Vec3 {
    Vec3::new(
        rng.range(r.a[0], r.b[0]),
        rng.range(r.a[1], r.b[1]),
        rng.range(r.a[2], r.b[2]),
    )
}

/// Only `linear` animates; an unflagged curve holds `start` (grammar doc R3).
fn curve_animates(flags: &[String]) -> bool {
    flags.iter().any(|f| f == "linear")
}

/// Non-animating curves collapse onto `start`. Both ends are always sampled
/// so the rng stream does not depend on the flags.
fn sample_curve(rng: &mut Rng, c: &efx::Curve) -> (f32, f32) {
    let a = sample_r1(rng, c.start);
    let b = sample_r1(rng, c.end);
    if curve_animates(&c.flags) {
        (a, b)
    } else {
        (a, a)
    }
}

/// [`sample_curve`] for the `rgb` curve.
fn sample_curve_v3(rng: &mut Rng, c: &efx::CurveV3) -> (Vec3, Vec3) {
    let a = sample_r3(rng, c.start);
    let b = sample_r3(rng, c.end);
    if curve_animates(&c.flags) {
        (a, b)
    } else {
        (a, a)
    }
}

fn spawn_pos(at: SpawnAt) -> Vec3 {
    match at {
        SpawnAt::Point { pos }
        | SpawnAt::Surface { pos, .. }
        | SpawnAt::Directed { pos, .. }
        | SpawnAt::Oriented { pos, .. } => pos,
    }
}

/// The effect axis a direction plays on: `MakeNormalVectors` (CoDMP.exe
/// 0x42e1b0), which syscall 0xe4 (0x4a1800) runs on the forward before
/// `PlayEffect` (docs/research/efx-grammar.md R11). The right vector is the
/// forward's components rotated, `(z, -x, y)`, with the forward projected
/// out, and up is right x forward.
fn tangent_basis(axis: Vec3) -> (Vec3, Vec3, Vec3) {
    let r = Vec3::new(axis.z, -axis.x, axis.y);
    let right = (r - axis * r.dot(axis)).normalize_or_zero();
    (axis, right, right.cross(axis))
}

/// Syscall 0xe3's fixed axis (CoDMP.exe 0x4936c0), what an effect played
/// with no direction gets: forward up, then world x and y.
const UP_AXIS: (Vec3, Vec3, Vec3) = (Vec3::Z, Vec3::X, Vec3::Y);

/// Rotate the 2D basis `(x, y)` by `rad` radians in their shared plane.
fn rotate2(x: Vec3, y: Vec3, rad: f32) -> (Vec3, Vec3) {
    let (s, c) = rad.sin_cos();
    (x * c + y * s, y * c - x * s)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// orgOnSphere / orgOnCylinder placement (grammar doc R6); otherwise `base`.
fn distribute_origin(rng: &mut Rng, em: &Emitter, base: Vec3) -> Vec3 {
    use std::f32::consts::TAU;
    if em.spawn_flags.iter().any(|f| f == "orgOnSphere") {
        let radius = sample_r1(rng, em.radius);
        let theta = rng.next_f32() * TAU;
        // Uniform on the unit sphere: cos(phi) uniform in [-1, 1].
        let cos_phi = 1.0 - 2.0 * rng.next_f32();
        let sin_phi = (1.0 - cos_phi * cos_phi).max(0.0).sqrt();
        let dir = Vec3::new(sin_phi * theta.cos(), sin_phi * theta.sin(), cos_phi);
        base + dir * radius
    } else if em.spawn_flags.iter().any(|f| f == "orgOnCylinder") {
        let radius = sample_r1(rng, em.radius);
        let height = sample_r1(rng, em.height);
        let theta = rng.next_f32() * TAU;
        let z = rng.range(-height * 0.5, height * 0.5);
        base + Vec3::new(radius * theta.cos(), radius * theta.sin(), z)
    } else {
        base
    }
}

/// One [`FxSound`] per alias of a `Sound` block; origin and delay are
/// sampled once per block, since it has no `count`.
fn spawn_sound_cues(rng: &mut Rng, em: &Emitter, at: SpawnAt) -> Vec<FxSound> {
    if em.kind != "Sound" || em.sounds.is_empty() {
        return Vec::new();
    }
    let pos = spawn_pos(at) + sample_r3(rng, em.origin);
    let delay_s = sample_r1(rng, em.delay) / 1000.0;
    em.sounds
        .iter()
        .map(|alias| FxSound {
            alias: alias.clone(),
            pos,
            delay_s,
        })
        .collect()
}

/// A `Decal` element waiting for its delay.
struct PendingMark {
    due: f32,
    req: MarkRequest,
}

/// Where one spawn's elements land.
struct SpawnOut<'a> {
    particles: &'a mut VecDeque<Particle>,
    marks: &'a mut Vec<PendingMark>,
    chains: &'a mut Vec<Chain>,
}

/// What one effect spawn shares between its elements.
#[derive(Clone, Copy)]
struct SpawnCx {
    at: SpawnAt,
    now: f32,
    /// The camera, for `cullrange`.
    view: Option<Vec3>,
    depth: u8,
    /// The tag the effect was played on, and its frame when it is posed.
    bolt: Option<(Bolt, Option<BoltFrame>)>,
}

/// The emitter frame of `at`: forward along its direction or normal.
fn spawn_frame(at: SpawnAt) -> Option<(Vec3, Vec3, Vec3)> {
    match at {
        SpawnAt::Directed { dir, .. } => Some(tangent_basis(dir.normalize_or_zero())),
        SpawnAt::Surface { normal, .. } => Some(tangent_basis(normal.normalize_or_zero())),
        SpawnAt::Point { .. } => Some(UP_AXIS),
        SpawnAt::Oriented {
            axis: [x, y, z], ..
        } => Some((x, y, z)),
    }
}

/// `at` moved to `pos`, its direction or normal kept.
fn spawn_moved(at: SpawnAt, pos: Vec3) -> SpawnAt {
    match at {
        SpawnAt::Point { .. } => SpawnAt::Point { pos },
        SpawnAt::Surface { normal, .. } => SpawnAt::Surface { pos, normal },
        SpawnAt::Directed { dir, .. } => SpawnAt::Directed { pos, dir },
        SpawnAt::Oriented { axis, .. } => SpawnAt::Oriented { pos, axis },
    }
}

/// The sampled `origin` in the emitter frame unless cheapOrgCalc (R6),
/// then spread by orgOnSphere/orgOnCylinder: (centre, spawn point).
fn element_origin(
    rng: &mut Rng,
    em: &Emitter,
    at: SpawnAt,
    frame: Option<(Vec3, Vec3, Vec3)>,
) -> (Vec3, Vec3) {
    let mut offset = sample_r3(rng, em.origin);
    if let Some((x, y, z)) = frame
        && !em.spawn_flags.iter().any(|f| f == "cheapOrgCalc")
    {
        offset = offset.x * x + offset.y * y + offset.z * z;
    }
    let center = spawn_pos(at) + offset;
    (center, distribute_origin(rng, em, center))
}

fn spawn_emitter(rng: &mut Rng, out: &mut SpawnOut, em: &Emitter, cx: &SpawnCx) {
    let at = cx.at;
    let now = cx.now;
    let behavior = classify(&em.kind);
    if behavior == Behavior::NoOp {
        return;
    }
    // The effect runner skips an element when the effect is played past
    // its `cullrange` from the view, and never tests it again (CoDMP.exe
    // 0x493a1f, grammar doc R9).
    if em.cullrange > 0.0
        && let Some(view) = cx.view
        && (spawn_pos(at) - view).length() > em.cullrange
    {
        return;
    }
    // A `relative` element rides the tag; with the tag gone it is not
    // created (0x494050's 0x200000 branch, R10).
    let bolt = match cx.bolt {
        Some((id, frame)) if em.flags.iter().any(|f| f == "relative") => match frame {
            Some(frame) => Some((id, frame)),
            None => return,
        },
        _ => None,
    };
    // Clamped so a malformed count (1e9) does not spin the loop before
    // push_capped evicts.
    let count = (sample_r1(rng, em.count).round().max(0.0) as usize).min(MAX_PARTICLES);
    for _ in 0..count {
        let delay = sample_r1(rng, em.delay) / 1000.0;
        let spawn = now + delay;
        if behavior == Behavior::Runner {
            // Case 10 of the spawner: the element's origin and frame, then
            // PlayEffect on one `playfx` entry (0x492780 picks).
            let (_, pos) = element_origin(rng, em, at, spawn_frame(at));
            if let Some(fx) = pick(rng, &em.play_fx) {
                out.chains.push(Chain {
                    due: spawn,
                    path: efx::chained_path(fx),
                    at: spawn_moved(at, pos),
                    depth: cx.depth + 1,
                    bolt: cx.bolt.map(|(id, _)| id),
                });
            }
            continue;
        }
        let life = sample_r1(rng, em.life) / 1000.0;
        if behavior == Behavior::Decal {
            out.marks.push(PendingMark {
                due: spawn,
                req: decal_request(rng, em, at, life),
            });
            continue;
        }
        if life <= 0.0 {
            // An explicit `life 0` would make the age fraction 0/0.
            continue;
        }

        let mut vel = sample_r3(rng, em.velocity);
        let mut accel = sample_r3(rng, em.accel);
        // Emitter-frame vectors rotate onto `dir`/`normal` unless the
        // absoluteVel/absoluteAccel spawnFlags (not flags) keep them
        // world-space (grammar doc R6).
        let frame = spawn_frame(at);
        if let Some((x, y, z)) = frame {
            if !em.spawn_flags.iter().any(|f| f == "absoluteVel") {
                vel = vel.x * x + vel.y * y + vel.z * z;
            }
            if !em.spawn_flags.iter().any(|f| f == "absoluteAccel") {
                accel = accel.x * x + accel.y * y + accel.z * z;
            }
        }

        // evenDistribution is not implemented.
        let (center, base) = element_origin(rng, em, at, frame);

        // axisFromSphere: velocity points radially out from the spawn center,
        // magnitude kept. Without orgOnSphere/orgOnCylinder, base == center.
        if em.spawn_flags.iter().any(|f| f == "axisFromSphere")
            && em
                .spawn_flags
                .iter()
                .any(|f| f == "orgOnSphere" || f == "orgOnCylinder")
        {
            let radial = (base - center).normalize_or_zero();
            if radial != Vec3::ZERO {
                vel = radial * vel.length();
            }
        }

        let mut pos = base;

        // Gravity joins the acceleration on world z (0x494050).
        accel.z += sample_r1(rng, em.gravity);
        let rot = sample_r1(rng, em.rotation);
        let rot_delta = sample_r1(rng, em.rotation_delta);
        let (size0, size1) = sample_curve(rng, &em.size);
        // `length` is a tail-only field (grammar doc R3); sprites carrying
        // one (fx/muzzleflashes/heavy.efx) stay square.
        let (len0, len1) = match sample_curve(rng, &em.length) {
            l if behavior == Behavior::Stretch => l,
            _ => (0.0, 0.0),
        };
        let (alpha0, alpha1) = sample_curve(rng, &em.alpha);
        let (rgb0, rgb1) = sample_curve_v3(rng, &em.rgb);
        // `bounce` and an `impactfx` list each turn physics on (R9). A
        // bolted particle does not trace: vcod integrates it in the tag's
        // frame.
        let physics = bolt.is_none()
            && (em.flags.iter().any(|f| f == "usePhysics")
                || em.bounce.is_some()
                || !em.impact_fx.is_empty());
        let impact_kills = em.flags.iter().any(|f| f == "impactKills");
        let elasticity = em.bounce.map_or(0.0, |b| sample_r1(rng, b));
        let impact_fx = pick(rng, &em.impact_fx).cloned();
        let death_fx = pick(rng, &em.death_fx).map(|f| efx::chained_path(f));
        let shader = pick(rng, &em.shaders).cloned().unwrap_or_default();
        let debris = (behavior == Behavior::Emitter).then(|| {
            // The models list turns `useModel` on (0x49cae0).
            let model = pick(rng, &em.models).cloned();
            let mut angles = sample_r3(rng, em.angle);
            // A bolted model's angles are in the tag's frame, whose forward
            // is the spawn direction.
            if let Some((x, _, _)) = frame.filter(|_| bolt.is_none()) {
                angles += forward_angles(x);
            }
            Debris {
                model,
                angles,
                angle_delta: sample_r3(rng, em.angle_delta),
            }
        });
        // `CEmitter` (0x4a02c0) starts its cursor at its own origin and
        // velocity; the first spacing is drawn like every later one.
        let emit = match pick(rng, &em.emit_fx) {
            Some(fx) if behavior == Behavior::Emitter => {
                let density = sample_r1(rng, em.density);
                let variance = sample_r1(rng, em.variance);
                Some(Emit {
                    fx: efx::chained_path(fx),
                    pos,
                    vel,
                    last_ms: (spawn * 1000.0).round() as i64,
                    spacing: emit_spacing(rng, density, variance),
                    density,
                    variance,
                })
            }
            _ => None,
        };
        let mut emit = emit;
        if let Some((_, frame)) = bolt {
            pos = frame.unrotate(pos - frame.origin);
            vel = frame.unrotate(vel);
            accel = frame.unrotate(accel);
            if let Some(e) = &mut emit {
                e.pos = pos;
                e.vel = vel;
            }
        }

        let p = Particle {
            spawn,
            life,
            pos,
            vel,
            accel,
            rot,
            rot_delta,
            size0,
            size1,
            len0,
            len1,
            alpha0,
            alpha1,
            rgb0,
            rgb1,
            physics,
            impact_kills,
            elasticity,
            impact_fx,
            death_fx,
            shader,
            is_light: behavior == Behavior::Light,
            debris,
            emit,
            depth: cx.depth,
            bolt: bolt.map(|(id, _)| id),
        };

        push_capped(out.particles, p, MAX_PARTICLES);
    }
}

/// The next `emitfx` spacing: `density` plus `variance` times a draw in
/// [-1, 1) (0x4902a9; `CEmitter`'s constructor draws the first the same way).
fn emit_spacing(rng: &mut Rng, density: f32, variance: f32) -> f32 {
    density + variance * (2.0 * rng.next_f32() - 1.0)
}

/// What a `Decal` element hands `CG_ImpactMark` (CoDMP.exe 0x496896): a
/// shader from its list, the start `rgb`, `alpha` and `size` (the radius),
/// `rotation`, `life`, along the surface normal, alpha fading. Decals carry
/// no `origin` or `velocity` in the corpus.
fn decal_request(rng: &mut Rng, em: &Emitter, at: SpawnAt, life: f32) -> MarkRequest {
    let dir = match at {
        SpawnAt::Surface { normal, .. } => normal,
        SpawnAt::Directed { dir, .. } => dir,
        SpawnAt::Oriented { axis, .. } => axis[0],
        SpawnAt::Point { .. } => Vec3::Z,
    };
    let orientation = sample_r1(rng, em.rotation);
    let (radius, _) = sample_curve(rng, &em.size);
    let (alpha, _) = sample_curve(rng, &em.alpha);
    let (rgb, _) = sample_curve_v3(rng, &em.rgb);
    let shader = if em.shaders.is_empty() {
        String::new()
    } else {
        let idx = ((rng.next_f32() * em.shaders.len() as f32) as usize).min(em.shaders.len() - 1);
        em.shaders[idx].clone()
    };
    MarkRequest {
        shader,
        origin: spawn_pos(at),
        dir,
        orientation,
        rgba: [rgb.x, rgb.y, rgb.z, alpha],
        alpha_fade: true,
        radius,
        life_ms: (life * 1000.0).round() as i32,
    }
}

/// One entry at random, as the shader, model and impact lists are drawn.
fn pick<'a, T>(rng: &mut Rng, list: &'a [T]) -> Option<&'a T> {
    if list.is_empty() {
        return None;
    }
    let idx = ((rng.next_f32() * list.len() as f32) as usize).min(list.len() - 1);
    list.get(idx)
}

/// `vectoangles`: pitch (positive down) and yaw of `dir`, roll 0.
fn forward_angles(dir: Vec3) -> Vec3 {
    let yaw = dir.y.atan2(dir.x).to_degrees();
    let pitch = -dir.z.atan2(dir.truncate().length()).to_degrees();
    Vec3::new(pitch, yaw, 0.0)
}

fn push_capped(q: &mut VecDeque<Particle>, p: Particle, cap: usize) {
    q.push_back(p);
    while q.len() > cap {
        q.pop_front();
    }
}

/// Per-frame inputs [`step_pool`] shares between particles.
struct StepCx<'a> {
    dt: f32,
    now: f32,
    world: Option<&'a CollisionWorld>,
    bolts: &'a HashMap<Bolt, BoltFrame>,
}

/// Delayed particles wait, expired ones drop. Effects the step starts
/// (impacts, emits, deaths) land in `chains`.
fn step_pool(pool: &mut VecDeque<Particle>, cx: &StepCx, rng: &mut Rng, chains: &mut Vec<Chain>) {
    let (dt, now) = (cx.dt, cx.now);
    pool.retain_mut(|p| {
        if now < p.spawn {
            return true;
        }
        let frame = match p.bolt {
            None => None,
            Some(id) => match cx.bolts.get(&id) {
                Some(&f) => Some(f),
                // The tag is gone: `Update` fails and `Die` runs with the
                // `impactKills` bit still set (0x49f1e0, R10).
                None => {
                    if !p.impact_kills {
                        death_chain(p, None, now, rng, chains);
                    }
                    return false;
                }
            },
        };
        if p.life <= 0.0 || now - p.spawn > p.life {
            death_chain(p, frame, now, rng, chains);
            return false;
        }
        let before = p.pos;
        if !update_origin(p, dt, cx.world, now, chains) {
            return false;
        }
        p.rot += p.rot_delta * dt;
        if let Some(d) = &mut p.debris {
            // A resting model's spin dies away (CEmitter::Update, 0x48fa2f).
            if p.pos == before {
                d.angle_delta *= 0.7;
            }
            d.angles += d.angle_delta * (dt * 1000.0 * 0.01);
        }
        if let Some(mut e) = p.emit.take() {
            let now_ms = (now * 1000.0).round() as i64;
            let forward = p.debris.as_ref().map_or(Vec3::X, |d| {
                Vec3::from(angles_to_axis(d.angles.to_array())[0])
            });
            for pos in emit_along(&mut e, p.accel, now_ms, rng) {
                let (pos, dir) = match frame {
                    Some(f) => (f.origin + f.rotate(pos), f.rotate(forward)),
                    None => (pos, forward),
                };
                chains.push(Chain {
                    due: now,
                    path: e.fx.clone(),
                    at: SpawnAt::Directed { pos, dir },
                    depth: p.depth + 1,
                    bolt: p.bolt,
                });
            }
            p.emit = Some(e);
        }
        true
    });
}

/// `Die` (CoDMP.exe 0x48dda0): a `deathfx` plays at the particle along a
/// random direction, unless an `impactKills` hit ended it. The pool clears
/// that bit before it retires an expired particle (0x49f1e0), so only a
/// hit skips the effect; vcod drops a hit particle in [`update_origin`].
fn death_chain(
    p: &Particle,
    frame: Option<BoltFrame>,
    now: f32,
    rng: &mut Rng,
    chains: &mut Vec<Chain>,
) {
    let Some(fx) = &p.death_fx else {
        return;
    };
    let dir = Vec3::new(
        2.0 * rng.next_f32() - 1.0,
        2.0 * rng.next_f32() - 1.0,
        2.0 * rng.next_f32() - 1.0,
    )
    .normalize_or_zero();
    let pos = frame.map_or(p.pos, |f| f.origin + f.rotate(p.pos));
    chains.push(Chain {
        due: now,
        path: fx.clone(),
        at: SpawnAt::Directed { pos, dir },
        depth: p.depth + 1,
        bolt: None,
    });
}

/// `CEmitter::Update`'s `emitFx` loop (CoDMP.exe 0x48fcd4..0x4902ef): walk
/// the cursor's path in 12 ms steps up to `now_ms`; once a step lands
/// `spacing` or more from the last play, a Newton step on the squared
/// distance (`3` at 0x5690c8, `0.25` at 0x568ed0, `1000` at 0x569090)
/// pulls the time back to where the distance is `spacing`, no earlier
/// than the step before. The effect plays there, the cursor moves to it
/// and a new spacing is drawn. Returns the play positions.
fn emit_along(e: &mut Emit, accel: Vec3, now_ms: i64, rng: &mut Rng) -> Vec<Vec3> {
    let mut out = Vec::new();
    let at = |e: &Emit, ms: i64| {
        let t = ms as f32 * 0.001;
        e.pos + e.vel * t + 0.5 * accel * t * t
    };
    let mut clock = e.last_ms;
    let mut s = 0;
    while clock < now_ms {
        s += EMIT_STEP_MS;
        let p = at(e, s);
        let thresh2 = e.spacing * e.spacing;
        if (p - e.pos).length_squared() < thresh2 {
            clock += EMIT_STEP_MS;
            continue;
        }
        let t = s as f32 * 0.001;
        let (v2, va, a2) = (
            e.vel.length_squared(),
            e.vel.dot(accel),
            accel.length_squared(),
        );
        let slope = 2.0 * t * v2 + 3.0 * t * t * va + t * t * t * a2;
        if slope != 0.0 {
            let excess = t * t * v2 + t * t * t * va + 0.25 * t * t * t * t * a2 - thresh2;
            // Never at or before the last play, or a zero spacing would
            // spin here; retail has no such floor.
            s = (s - (excess / slope * 1000.0).round() as i64)
                .max(clock - e.last_ms)
                .max(1);
        }
        let p = at(e, s);
        out.push(p);
        e.vel += accel * (s as f32 * 0.001);
        e.pos = p;
        e.last_ms += s;
        clock = e.last_ms;
        s = 0;
        e.spacing = emit_spacing(rng, e.density, e.variance);
    }
    out
}

/// `CParticle::UpdateOrigin` (CoDMP.exe 0x48df20, grammar doc R9): the
/// velocity takes the frame's acceleration first, then the position moves
/// on the new velocity plus half the acceleration. A physics particle
/// traces that move against solid brushes; a hit plays its impact effect,
/// kills an `impactKills` particle, or reflects the velocity scaled by
/// `bounce` and parks the particle at the hit. False when it dies.
fn update_origin(
    p: &mut Particle,
    dt: f32,
    world: Option<&CollisionWorld>,
    now: f32,
    chains: &mut Vec<Chain>,
) -> bool {
    p.vel += p.accel * dt;
    let new_pos = p.pos + p.vel * dt + 0.5 * p.accel * dt * dt;
    if p.physics
        && let Some(world) = world
    {
        let tr = world.point_trace(p.pos, new_pos, CONTENTS_SOLID, false);
        if tr.fraction < 1.0 && !tr.startsolid && !tr.allsolid {
            if let Some(fx) = &p.impact_fx {
                chains.push(Chain {
                    due: now,
                    path: efx::chained_path(fx),
                    at: SpawnAt::Surface {
                        pos: tr.endpos,
                        normal: tr.normal,
                    },
                    depth: p.depth + 1,
                    bolt: None,
                });
            }
            if p.impact_kills {
                return false;
            }
            p.vel += p.accel * (tr.fraction * dt);
            p.vel = (p.vel - 2.0 * p.vel.dot(tr.normal) * tr.normal) * p.elasticity;
            // Too slow off a floor: it rests, and stops tracing.
            if tr.normal.z > 0.0 && p.vel.z < 4.0 {
                p.vel = Vec3::ZERO;
                p.accel = Vec3::ZERO;
                p.physics = false;
                p.impact_fx = None;
            }
            p.pos = p.pos.lerp(new_pos, tr.fraction);
            return true;
        }
    }
    p.pos = new_pos;
    true
}

/// Farthest from `cam_pos` first, the order `Renderer::set_fx_quads` draws in.
pub fn sort_back_to_front(quads: &mut [FxQuad], cam_pos: Vec3) {
    quads.sort_by(|a, b| {
        let da = (quad_center(a) - cam_pos).length_squared();
        let db = (quad_center(b) - cam_pos).length_squared();
        // total_cmp: a NaN must not panic here.
        db.total_cmp(&da)
    });
}

fn quad_center(q: &FxQuad) -> Vec3 {
    q.verts.iter().map(|v| Vec3::from(*v)).sum::<Vec3>() / 4.0
}

fn make_quad(center: Vec3, x: Vec3, y: Vec3, rgb: Vec3, alpha: f32, shader: &str) -> FxQuad {
    let mut verts = [[0.0f32; 3]; 4];
    let mut uvs = [[0.0f32; 2]; 4];
    for (i, (u, v)) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
        .into_iter()
        .enumerate()
    {
        verts[i] = (center + x * (u * 2.0 - 1.0) + y * (v * 2.0 - 1.0)).to_array();
        uvs[i] = [u, v];
    }
    FxQuad {
        verts,
        uvs,
        rgba: [rgb.x, rgb.y, rgb.z, alpha],
        shader: shader.to_string(),
    }
}

/// Unit vector perpendicular to `axis`, preferring `view_dir`. The fallbacks
/// cover a segment aimed at the camera, which would collapse the quad.
fn stretch_side_dir(axis: Vec3, view_dir: Vec3, cam_up: Vec3, cam_right: Vec3) -> Vec3 {
    for candidate in [view_dir, cam_up, cam_right] {
        let c = axis.cross(candidate);
        if c.length_squared() > 1e-8 {
            return c.normalize();
        }
    }
    Vec3::ZERO
}

/// A particle's world origin and velocity; `None` when its tag is gone.
fn world_state(p: &Particle, bolts: &HashMap<Bolt, BoltFrame>) -> Option<(Vec3, Vec3)> {
    match p.bolt {
        None => Some((p.pos, p.vel)),
        Some(id) => {
            let f = *bolts.get(&id)?;
            Some((f.origin + f.rotate(p.pos), f.rotate(p.vel)))
        }
    }
}

fn live(p: &Particle, now: f32) -> bool {
    now >= p.spawn && p.life > 0.0 && now - p.spawn <= p.life
}

fn push_quads(
    pool: &VecDeque<Particle>,
    bolts: &HashMap<Bolt, BoltFrame>,
    cam_pos: Vec3,
    cam_right: Vec3,
    cam_up: Vec3,
    now: f32,
    out: &mut Vec<FxQuad>,
) {
    for p in pool {
        if p.is_light || p.debris.is_some() || !live(p, now) {
            continue;
        }
        let Some((pos, vel)) = world_state(p, bolts) else {
            continue;
        };
        let t = ((now - p.spawn) / p.life).clamp(0.0, 1.0);
        // `size` is a half-extent (grammar doc R4).
        let half = lerp(p.size0, p.size1, t);
        let alpha = lerp(p.alpha0, p.alpha1, t);
        let rgb = p.rgb0.lerp(p.rgb1, t);
        let rad = p.rot.to_radians();

        if p.len0 != 0.0 || p.len1 != 0.0 {
            // Anchored at the head, trailing `length` back along travel,
            // `2 * size` wide (grammar doc R5).
            let len = lerp(p.len0, p.len1, t);
            let head = pos;
            let tail = head - vel.normalize_or_zero() * len;
            let mid = (head + tail) * 0.5;
            let axis = (head - tail) * 0.5;
            let view_dir = (cam_pos - mid).normalize_or_zero();
            let side = stretch_side_dir(axis, view_dir, cam_up, cam_right) * half;
            out.push(make_quad(mid, axis, side, rgb, alpha, &p.shader));
        } else {
            let (rx, ry) = rotate2(cam_right, cam_up, rad);
            out.push(make_quad(pos, rx * half, ry * half, rgb, alpha, &p.shader));
        }
    }
}

/// Retail's moving tracer: `CG_SpawnTracer` @ 0x30038f30 starts a linear
/// trajectory at the muzzle, `CG_AddLocalEntities` @ 0x300201b0 draws
/// `[p, p + length*dir]` off it (0x3001ffe0) until it expires, so the tail
/// leaves the muzzle. Lifetime in seconds: the tail's travel to
/// `impact - length*dir`, truncated to whole ms.
fn moving_tracer_life(muzzle: Vec3, impact: Vec3) -> f32 {
    let dir = (impact - muzzle).normalize_or_zero();
    let travel = (impact - dir * TRACER_LENGTH - muzzle).length();
    (travel * 1000.0 / TRACER_SPEED).trunc() / 1000.0
}

/// A tracer as a stretch particle: `head` leads, the tail trails `len` back
/// along `vel`.
fn tracer_particle(spawn: f32, life: f32, head: Vec3, vel: Vec3, len: f32) -> Particle {
    Particle {
        spawn,
        life,
        pos: head,
        vel,
        accel: Vec3::ZERO,
        rot: 0.0,
        rot_delta: 0.0,
        size0: TRACER_WIDTH,
        size1: TRACER_WIDTH,
        len0: len,
        len1: len,
        alpha0: 1.0,
        alpha1: 1.0,
        rgb0: Vec3::ONE,
        rgb1: Vec3::ONE,
        physics: false,
        impact_kills: false,
        elasticity: 0.0,
        impact_fx: None,
        death_fx: None,
        shader: TRACER_SHADER.to_string(),
        is_light: false,
        debris: None,
        emit: None,
        depth: 0,
        bolt: None,
    }
}

pub struct FxSystem {
    cache: HashMap<String, Option<Effect>>,
    warned: HashSet<String>,
    particles: VecDeque<Particle>,
    marks: Vec<PendingMark>,
    /// Effects other effects started, for [`Self::spawn_chained`].
    chains: Vec<Chain>,
    /// The camera, for `cullrange`.
    view: Option<Vec3>,
    /// This frame's tags effects are played on, see [`Self::set_bolts`]
    /// and [`Self::set_tag_bolts`].
    bolts: HashMap<Bolt, BoltFrame>,
    rng: Rng,
}

impl Default for FxSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FxSystem {
    pub fn new() -> FxSystem {
        FxSystem {
            cache: HashMap::new(),
            warned: HashSet::new(),
            particles: VecDeque::new(),
            marks: Vec::new(),
            chains: Vec::new(),
            view: None,
            bolts: HashMap::new(),
            rng: Rng::new(0x9E3779B97F4A7C15),
        }
    }

    fn ensure_cached(&mut self, fs: &Pk3Fs, path: &str) {
        if self.cache.contains_key(path) {
            return;
        }
        let effect = match fs.read(path) {
            None => {
                if self.warned.insert(path.to_string()) {
                    log::warn!("fx {path}: not found");
                }
                None
            }
            Some(bytes) => {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                match efx::parse(&text) {
                    Ok(e) => Some(e),
                    Err(err) => {
                        if self.warned.insert(path.to_string()) {
                            log::warn!("fx {path}: {err}");
                        }
                        None
                    }
                }
            }
        };
        self.cache.insert(path.to_string(), effect);
    }

    /// A download reopened the search path: effect files parse again, and a
    /// miss may now resolve.
    pub fn reopen(&mut self) {
        self.cache.clear();
        self.warned.clear();
    }

    /// A new map: world-space particles and marks are meaningless there.
    pub fn clear(&mut self) {
        self.particles.clear();
        self.marks.clear();
        self.chains.clear();
        self.bolts.clear();
    }

    /// The camera this frame's spawns are culled against.
    pub fn set_view(&mut self, pos: Vec3) {
        self.view = Some(pos);
    }

    /// This frame's tags, as (origin, forward) by key: the shooters'
    /// muzzles, `u32::MAX` the viewmodel's. A `relative` element of an
    /// effect played on one rides it, and dies with it.
    pub fn set_bolts(&mut self, bolts: &HashMap<u32, (Vec3, Vec3)>) {
        self.bolts.retain(|b, _| !matches!(b, Bolt::Muzzle(_)));
        self.bolts.extend(
            bolts
                .iter()
                .map(|(&id, &f)| (Bolt::Muzzle(id), BoltFrame::new(f))),
        );
    }

    /// This frame's poses of the `playFxOnTag` tags [`Self::tag_bolts`]
    /// named. A `relative` element on a tag missing here dies, as a bolt
    /// that fails to pose does (R10).
    pub fn set_tag_bolts(&mut self, tags: &HashMap<TagBolt, TagPose>) {
        self.bolts.retain(|b, _| !matches!(b, Bolt::Tag(_)));
        self.bolts.extend(
            tags.iter()
                .map(|(&t, &pose)| (Bolt::Tag(t), BoltFrame::tag(pose))),
        );
    }

    /// The `playFxOnTag` tags a live element or a pending chain rides,
    /// which the caller poses for [`Self::set_tag_bolts`] each frame.
    pub fn tag_bolts(&self) -> HashSet<TagBolt> {
        let particles = self.particles.iter().filter_map(|p| p.bolt);
        let chains = self.chains.iter().filter_map(|c| c.bolt);
        particles
            .chain(chains)
            .filter_map(|b| match b {
                Bolt::Tag(t) => Some(t),
                Bolt::Muzzle(_) => None,
            })
            .collect()
    }

    /// `CG_PlayFxOnTag` (cgame 0x3001db80): the effect on the tag's pose
    /// through syscall 0xe5, bolted to it. Only its `relative` elements
    /// follow the tag after this frame (`PlayEffect` 0x493870, R10).
    pub fn spawn_on_tag(
        &mut self,
        fs: &Pk3Fs,
        path: &str,
        tag: TagBolt,
        pose: TagPose,
        now: f32,
    ) -> Vec<FxSound> {
        self.bolts.insert(Bolt::Tag(tag), BoltFrame::tag(pose));
        let at = SpawnAt::Oriented {
            pos: pose.origin,
            axis: pose.axis,
        };
        let first_chain = self.chains.len();
        let mut sounds = self.spawn_effect(fs, path, at, now, 0, Some(Bolt::Tag(tag)));
        sounds.extend(self.drain_chains(fs, now, first_chain));
        sounds
    }

    /// Missing or broken files warn once and no-op. Returns the `Sound`
    /// cues for the audio system.
    pub fn spawn(&mut self, fs: &Pk3Fs, path: &str, at: SpawnAt, now: f32) -> Vec<FxSound> {
        self.spawn_marked(fs, path, at, None, now, true)
    }

    /// [`Self::spawn`] on the tag `bolt` (a [`Self::set_bolts`] key) when
    /// it is set; with `marks` false its `Decal` elements, and those of
    /// the effects it starts at once, place nothing, as
    /// `EV_ROCKET_EXPLODE_NOMARKS` has the cgame refuse them.
    pub fn spawn_marked(
        &mut self,
        fs: &Pk3Fs,
        path: &str,
        at: SpawnAt,
        bolt: Option<u32>,
        now: f32,
        marks: bool,
    ) -> Vec<FxSound> {
        let first_mark = self.marks.len();
        let first_chain = self.chains.len();
        let mut sounds = self.spawn_effect(fs, path, at, now, 0, bolt.map(Bolt::Muzzle));
        // An `FxRunner` without a delay plays inside PlayEffect (0x493870).
        sounds.extend(self.drain_chains(fs, now, first_chain));
        if !marks {
            self.marks.truncate(first_mark);
        }
        sounds
    }

    fn spawn_effect(
        &mut self,
        fs: &Pk3Fs,
        path: &str,
        at: SpawnAt,
        now: f32,
        depth: u8,
        bolt: Option<Bolt>,
    ) -> Vec<FxSound> {
        if depth > MAX_CHAIN_DEPTH {
            return Vec::new();
        }
        self.ensure_cached(fs, path);
        let cx = SpawnCx {
            at,
            now,
            view: self.view,
            depth,
            bolt: bolt.map(|id| (id, self.bolts.get(&id).copied())),
        };
        let mut sounds = Vec::new();
        if let Some(Some(effect)) = self.cache.get(path) {
            let mut out = SpawnOut {
                particles: &mut self.particles,
                marks: &mut self.marks,
                chains: &mut self.chains,
            };
            for em in &effect.emitters {
                spawn_emitter(&mut self.rng, &mut out, em, &cx);
                sounds.extend(spawn_sound_cues(&mut self.rng, em, at));
            }
        }
        sounds
    }

    /// Spawns the chained effects due by `now` from `chains[from..]`,
    /// and those they start in turn.
    fn drain_chains(&mut self, fs: &Pk3Fs, now: f32, from: usize) -> Vec<FxSound> {
        let mut sounds = Vec::new();
        let mut i = from;
        while i < self.chains.len() {
            if self.chains[i].due > now {
                i += 1;
                continue;
            }
            let c = self.chains.remove(i);
            sounds.extend(self.spawn_effect(fs, &c.path, c.at, now, c.depth, c.bolt));
        }
        sounds
    }

    /// Plays the effects due by `now` that others started: delayed
    /// `playfx`, and the `impactfx`, `emitfx` and `deathfx` the steps
    /// since the last call raised.
    pub fn spawn_chained(&mut self, fs: &Pk3Fs, now: f32) -> Vec<FxSound> {
        self.drain_chains(fs, now, 0)
    }

    /// One tracer from `muzzle` towards `impact`, per the retail client's
    /// hardcoded tracer (grammar doc R8). `flesh` is `surfType == 7`, which
    /// retail draws as a one-frame segment instead of a moving streak.
    /// `frame_dt` is the frame's length in seconds, for the spawn back-date.
    /// False when the chance roll or the flesh distance gate drops the shot.
    pub fn spawn_tracer(
        &mut self,
        muzzle: Vec3,
        impact: Vec3,
        flesh: bool,
        now: f32,
        frame_dt: f32,
    ) -> bool {
        if self.rng.next_f32() >= TRACER_CHANCE {
            return false;
        }
        let delta = impact - muzzle;
        let dist = delta.length();
        let dir = delta.normalize_or_zero();
        let p = if flesh {
            if dist < TRACER_FLESH_MIN_DIST {
                return false;
            }
            let near = TRACER_FLESH_NEAR + (dist - TRACER_FLESH_SPREAD_CUT) * self.rng.next_f32();
            let far = (near + TRACER_LENGTH).min(dist);
            tracer_particle(
                now,
                // Drawn on the spawn frame only; `step` drops it before it moves.
                f32::MIN_POSITIVE,
                muzzle + dir * far,
                dir,
                far - near,
            )
        } else {
            // `(rand() % cg.frametime) / 2` in whole ms.
            let frame_ms = (frame_dt * 1000.0) as u64;
            let back = if frame_ms == 0 {
                0.0
            } else {
                ((self.rng.next_u64() % frame_ms) / 2) as f32 / 1000.0
            };
            let vel = dir * TRACER_SPEED;
            tracer_particle(
                now - back,
                moving_tracer_life(muzzle, impact),
                muzzle + dir * TRACER_LENGTH + vel * back,
                vel,
                TRACER_LENGTH,
            )
        };
        push_capped(&mut self.particles, p, MAX_PARTICLES);
        true
    }

    #[cfg(test)]
    pub fn spawn_effect_for_test(&mut self, effect: Effect, at: SpawnAt, now: f32) -> Vec<FxSound> {
        let mut sounds = Vec::new();
        let cx = SpawnCx {
            at,
            now,
            view: self.view,
            depth: 0,
            bolt: None,
        };
        let mut out = SpawnOut {
            particles: &mut self.particles,
            marks: &mut self.marks,
            chains: &mut self.chains,
        };
        for em in &effect.emitters {
            spawn_emitter(&mut self.rng, &mut out, em, &cx);
            sounds.extend(spawn_sound_cues(&mut self.rng, em, at));
        }
        sounds
    }

    /// Advance the simulation. `world` enables usePhysics collision. Call
    /// [`Self::spawn_chained`] after it for the effects the step started.
    pub fn step(&mut self, dt: f32, now: f32, world: Option<&CollisionWorld>) {
        let cx = StepCx {
            dt,
            now,
            world,
            bolts: &self.bolts,
        };
        step_pool(&mut self.particles, &cx, &mut self.rng, &mut self.chains);
    }

    /// This frame's `Emitter` debris models.
    pub fn models(&self, now: f32) -> Vec<FxModel> {
        self.particles
            .iter()
            .filter(|p| live(p, now))
            .filter_map(|p| {
                let d = p.debris.as_ref()?;
                let model = d.model.clone()?;
                let (origin, _) = world_state(p, &self.bolts)?;
                let frame = p.bolt.and_then(|id| self.bolts.get(&id)).copied();
                let t = ((now - p.spawn) / p.life).clamp(0.0, 1.0);
                let scale = lerp(p.size0, p.size1, t);
                let axis = angles_to_axis(d.angles.to_array()).map(|a| {
                    let a = Vec3::from(a) * scale;
                    frame.map_or(a, |f| f.rotate(a))
                });
                Some(FxModel {
                    model,
                    origin,
                    axis,
                })
            })
            .collect()
    }

    /// This frame's quads, far to near from `cam_pos`; the renderer draws
    /// in order.
    pub fn build_quads(
        &self,
        cam_pos: Vec3,
        cam_right: Vec3,
        cam_up: Vec3,
        now: f32,
    ) -> Vec<FxQuad> {
        let mut out = Vec::new();
        push_quads(
            &self.particles,
            &self.bolts,
            cam_pos,
            cam_right,
            cam_up,
            now,
            &mut out,
        );
        sort_back_to_front(&mut out, cam_pos);
        out
    }

    /// Live Light-kind particles as point lights: the first
    /// `MAX_SCENE_LIGHTS` in spawn order, as retail queues them while it
    /// draws, then sorted nearest `cam_pos` first.
    pub fn lights(&self, cam_pos: Vec3, now: f32) -> Vec<FxLight> {
        let mut candidates: Vec<(f32, FxLight)> = self
            .particles
            .iter()
            .filter(|p| p.is_light && live(p, now))
            .filter_map(|p| {
                let (pos, _) = world_state(p, &self.bolts)?;
                let t = ((now - p.spawn) / p.life).clamp(0.0, 1.0);
                let radius = lerp(p.size0, p.size1, t);
                let rgb = p.rgb0.lerp(p.rgb1, t);
                let dist_sq = (pos - cam_pos).length_squared();
                Some((
                    dist_sq,
                    FxLight {
                        pos: pos.to_array(),
                        radius,
                        rgb: rgb.to_array(),
                    },
                ))
            })
            .take(MAX_SCENE_LIGHTS)
            .collect();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        candidates.into_iter().map(|(_, l)| l).collect()
    }

    /// The `Decal` elements due by `now`. Their `cullrange` was tested
    /// at spawn.
    pub fn take_marks(&mut self, now: f32) -> Vec<MarkRequest> {
        let mut out = Vec::new();
        self.marks.retain(|m| {
            if m.due > now {
                return true;
            }
            out.push(m.req.clone());
            false
        });
        out
    }

    /// (live particles, pending marks, lights) for the F3 overlay's `fx` line.
    pub fn counts(&self) -> (usize, usize, usize) {
        let lights = self.particles.iter().filter(|p| p.is_light).count();
        let particles = self.particles.len() - lights;
        (particles, self.marks.len(), lights)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::efx::{Curve, Emitter, R1};
    use glam::Vec3;

    /// A minimal one-particle emitter: no ranges, so behavior is exact.
    #[allow(clippy::field_reassign_with_default)]
    fn simple_emitter() -> Emitter {
        let mut e = Emitter::default();
        e.kind = "Particle".into();
        e.count = R1 { a: 1.0, b: 1.0 };
        e.life = R1 {
            a: 1000.0,
            b: 1000.0,
        }; // 1s
        e.velocity.a = [10.0, 0.0, 0.0];
        e.velocity.b = [10.0, 0.0, 0.0];
        e.gravity = R1 {
            a: -100.0,
            b: -100.0,
        };
        e.size = Curve {
            start: R1 { a: 2.0, b: 2.0 },
            end: R1 { a: 4.0, b: 4.0 },
            flags: vec!["linear".into()],
        };
        e.alpha = Curve {
            start: R1 { a: 1.0, b: 1.0 },
            end: R1 { a: 0.0, b: 0.0 },
            flags: vec!["linear".into()],
        };
        e.shaders = vec!["gfx/effects/test".into()];
        // World-space vectors, so a `Point` spawn's up axis leaves them be.
        e.spawn_flags = vec!["absoluteVel".into(), "absoluteAccel".into()];
        e
    }

    fn sys_with(e: Emitter, at: SpawnAt) -> FxSystem {
        let mut s = FxSystem::new();
        s.spawn_effect_for_test(Effect { emitters: vec![e] }, at, 0.0);
        s
    }

    #[test]
    fn particle_integrates_velocity_and_gravity_and_expires() {
        let mut s = sys_with(simple_emitter(), SpawnAt::Point { pos: Vec3::ZERO });
        s.step(0.5, 0.5, None);
        let q = s.build_quads(Vec3::new(0.0, -100.0, 0.0), Vec3::X, Vec3::Z, 0.5);
        assert_eq!(q.len(), 1);
        // Retail's order: v = (10, 0, -50) first, then x = 10*0.5 = 5,
        // z = -50*0.5 - 0.5*100*0.25 = -37.5. The quad center is the mean
        // of its verts.
        let c = quad_center(&q[0]);
        assert!(
            (c.x - 5.0).abs() < 1e-3 && (c.z + 37.5).abs() < 1e-3,
            "{c:?}"
        );
        // half-size at mid-life: 2 + (4-2)*0.5 = 3
        assert!((quad_half_x(&q[0]) - 3.0).abs() < 1e-3);
        assert!((q[0].rgba[3] - 0.5).abs() < 1e-3);
        s.step(0.6, 1.1, None); // past 1s life
        assert!(s.build_quads(Vec3::ZERO, Vec3::X, Vec3::Z, 1.1).is_empty());
    }

    #[test]
    fn delay_holds_particle_invisible() {
        let mut e = simple_emitter();
        e.delay = R1 { a: 500.0, b: 500.0 };
        let mut s = sys_with(e, SpawnAt::Point { pos: Vec3::ZERO });
        s.step(0.25, 0.25, None);
        assert!(s.build_quads(Vec3::ZERO, Vec3::X, Vec3::Z, 0.25).is_empty());
        s.step(0.5, 0.75, None);
        assert_eq!(s.build_quads(Vec3::ZERO, Vec3::X, Vec3::Z, 0.75).len(), 1);
    }

    #[test]
    fn malformed_count_is_clamped_at_spawn_not_just_evicted_after() {
        let mut e = simple_emitter();
        e.count = R1 { a: 1e9, b: 1e9 };
        let s = sys_with(e, SpawnAt::Point { pos: Vec3::ZERO });
        assert_eq!(s.counts().0, MAX_PARTICLES);
    }

    #[test]
    fn particle_cap_evicts_oldest() {
        let mut e = simple_emitter();
        e.count = R1 { a: 1.0, b: 1.0 };
        let mut s = FxSystem::new();
        for i in 0..(MAX_PARTICLES + 10) {
            s.spawn_effect_for_test(
                Effect {
                    emitters: vec![e.clone()],
                },
                SpawnAt::Point {
                    pos: Vec3::new(i as f32, 0.0, 0.0),
                },
                0.0,
            );
        }
        assert_eq!(s.counts().0, MAX_PARTICLES);
    }

    #[test]
    fn quads_sorted_far_to_near() {
        let mut s = FxSystem::new();
        for x in [10.0f32, 200.0, 50.0] {
            s.spawn_effect_for_test(
                Effect {
                    emitters: vec![simple_emitter()],
                },
                SpawnAt::Point {
                    pos: Vec3::new(x, 0.0, 0.0),
                },
                0.0,
            );
        }
        let q = s.build_quads(Vec3::ZERO, Vec3::Y, Vec3::Z, 0.0);
        let xs: Vec<f32> = q.iter().map(|q| quad_center(q).x).collect();
        assert_eq!(xs, vec![200.0, 50.0, 10.0]);
    }

    fn quad_center(q: &FxQuad) -> Vec3 {
        q.verts.iter().map(|v| Vec3::from(*v)).sum::<Vec3>() / 4.0
    }
    fn quad_half_x(q: &FxQuad) -> f32 {
        (Vec3::from(q.verts[1]) - Vec3::from(q.verts[0])).length() / 2.0
    }

    #[test]
    fn sound_and_trigger_kinds_spawn_nothing() {
        for kind in ["Sound", "FxRunner"] {
            let mut e = simple_emitter();
            e.kind = kind.into();
            let s = sys_with(e, SpawnAt::Point { pos: Vec3::ZERO });
            assert_eq!(s.counts(), (0, 0, 0), "kind {kind} should be a no-op");
        }
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn spawn_returns_sound_cues_at_the_spawn_point() {
        let mut em = Emitter::default();
        em.kind = "Sound".to_string();
        em.sounds = vec!["kaboom".to_string()];
        em.delay = R1 { a: 250.0, b: 250.0 };
        let effect = Effect { emitters: vec![em] };
        let mut fx = FxSystem::new();
        let cues = fx.spawn_effect_for_test(
            effect,
            SpawnAt::Point {
                pos: Vec3::new(1.0, 2.0, 3.0),
            },
            0.0,
        );
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].alias, "kaboom");
        assert_eq!(cues[0].pos, Vec3::new(1.0, 2.0, 3.0));
        assert!((cues[0].delay_s - 0.25).abs() < 1e-6);
        assert_eq!(fx.counts(), (0, 0, 0));
    }

    #[test]
    fn decal_kind_hands_the_mark_code_a_request_not_a_quad() {
        let mut e = simple_emitter();
        e.kind = "Decal".into();
        e.rotation = R1 { a: 90.0, b: 90.0 };
        e.cullrange = 1200.0;
        let mut s = sys_with(
            e,
            SpawnAt::Surface {
                pos: Vec3::new(0.0, 0.0, 10.0),
                normal: Vec3::Z,
            },
        );
        assert_eq!(s.counts(), (0, 1, 0));
        assert!(s.build_quads(Vec3::ZERO, Vec3::X, Vec3::Y, 0.0).is_empty());
        let marks = s.take_marks(0.0);
        assert_eq!(
            marks,
            vec![MarkRequest {
                shader: "gfx/effects/test".into(),
                origin: Vec3::new(0.0, 0.0, 10.0),
                dir: Vec3::Z,
                orientation: 90.0,
                rgba: [1.0; 4],
                alpha_fade: true,
                radius: 2.0,
                life_ms: 1000,
            }]
        );
        assert_eq!(s.counts(), (0, 0, 0));
    }

    #[test]
    fn a_nomarks_spawn_drops_its_decals_and_keeps_the_rest() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let at = SpawnAt::Surface {
            pos: Vec3::ZERO,
            normal: Vec3::Z,
        };
        let path = "fx/explosions/grenade2.efx";
        let mut s = FxSystem::new();
        s.spawn_marked(&fs, path, at, None, 0.0, false);
        assert_eq!(s.counts().1, 0);
        assert!(s.counts().0 > 0);
        s.spawn(&fs, path, at, 0.0);
        assert_eq!(s.take_marks(0.0).len(), 1);
    }

    #[test]
    fn a_decal_spawned_past_its_cullrange_places_no_mark() {
        let mut e = simple_emitter();
        e.kind = "Decal".into();
        e.cullrange = 100.0;
        let mut s = FxSystem::new();
        s.set_view(Vec3::new(0.0, 101.0, 0.0));
        s.spawn_effect_for_test(
            Effect { emitters: vec![e] },
            SpawnAt::Point { pos: Vec3::ZERO },
            0.0,
        );
        assert!(s.take_marks(0.0).is_empty());
        assert_eq!(s.counts(), (0, 0, 0));
    }

    #[test]
    fn light_kind_contributes_a_light_not_a_quad() {
        let mut e = simple_emitter();
        e.kind = "Light".into();
        let s = sys_with(
            e,
            SpawnAt::Point {
                pos: Vec3::new(1.0, 2.0, 3.0),
            },
        );
        assert_eq!(s.counts(), (0, 0, 1));
        assert!(s.build_quads(Vec3::ZERO, Vec3::X, Vec3::Z, 0.0).is_empty());
        let lights = s.lights(Vec3::ZERO, 0.0);
        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].pos, [1.0, 2.0, 3.0]);
        assert!((lights[0].radius - 2.0).abs() < 1e-3); // size.start at t=0
        assert_eq!(lights[0].rgb, [1.0, 1.0, 1.0]); // rgb absent -> white
    }

    #[test]
    fn lights_are_returned_nearest_cam_pos_first() {
        let mut e = simple_emitter();
        e.kind = "Light".into();
        let mut s = FxSystem::new();
        for x in [50.0f32, 5.0, 20.0] {
            s.spawn_effect_for_test(
                Effect {
                    emitters: vec![e.clone()],
                },
                SpawnAt::Point {
                    pos: Vec3::new(x, 0.0, 0.0),
                },
                0.0,
            );
        }
        let lights = s.lights(Vec3::ZERO, 0.0);
        let xs: Vec<f32> = lights.iter().map(|l| l.pos[0]).collect();
        assert_eq!(xs, vec![5.0, 20.0, 50.0]);
    }

    /// Retail's scene queue keeps the first 32 lights added, near or far.
    #[test]
    fn lights_keep_the_first_32_spawned() {
        let mut e = simple_emitter();
        e.kind = "Light".into();
        let mut s = FxSystem::new();
        for i in 0..40 {
            s.spawn_effect_for_test(
                Effect {
                    emitters: vec![e.clone()],
                },
                SpawnAt::Point {
                    pos: Vec3::new(1000.0 - i as f32, 0.0, 0.0),
                },
                0.0,
            );
        }
        let lights = s.lights(Vec3::ZERO, 0.0);
        assert_eq!(lights.len(), MAX_SCENE_LIGHTS);
        assert_eq!(lights[0].pos[0], 1000.0 - 31.0);
    }

    /// Syscall 0xe4's `MakeNormalVectors` and 0xe3's fixed axis: a forward
    /// along +x gets right -y and up +z, and no direction plays forward up,
    /// right +x, up +y.
    #[test]
    fn spawn_frames_are_retails_axes() {
        let (f, r, u) = tangent_basis(Vec3::X);
        assert_eq!((f, r, u), (Vec3::X, Vec3::NEG_Y, Vec3::Z));
        let d = Vec3::new(0.6, 0.0, 0.8);
        let (_, r, u) = tangent_basis(d);
        // (z, -x, y) = (0.8, -0.6, 0) less its 0.48 along d, normalized.
        let want = (Vec3::new(0.8, -0.6, 0.0) - d * 0.48).normalize();
        assert!((r - want).length() < 1e-6, "{r:?}");
        assert!((u - r.cross(d)).length() < 1e-6);
        assert!(r.dot(d).abs() < 1e-6 && u.dot(d).abs() < 1e-6);
        let point = spawn_frame(SpawnAt::Point { pos: Vec3::ZERO });
        assert_eq!(point, Some((Vec3::Z, Vec3::X, Vec3::Y)));
    }

    #[test]
    fn directed_spawn_rotates_velocity_into_dir_frame() {
        let mut e = simple_emitter();
        e.spawn_flags.clear();
        e.velocity = R3 {
            a: [1.0, 0.0, 0.0],
            b: [1.0, 0.0, 0.0],
        };
        let s = sys_with(
            e,
            SpawnAt::Directed {
                pos: Vec3::ZERO,
                dir: Vec3::new(0.0, 0.0, 1.0),
            },
        );
        let p = &s.particles[0];
        assert!((p.vel - Vec3::Z).length() < 1e-4, "{:?}", p.vel);
    }

    #[test]
    fn directed_spawn_rotates_origin_into_dir_frame() {
        let mut e = simple_emitter();
        e.velocity = R3 {
            a: [0.0; 3],
            b: [0.0; 3],
        };
        e.gravity = R1 { a: 0.0, b: 0.0 };
        e.origin = R3 {
            a: [10.0, 0.0, 0.0],
            b: [10.0, 0.0, 0.0],
        };
        let s = sys_with(
            e,
            SpawnAt::Directed {
                pos: Vec3::ZERO,
                dir: Vec3::new(0.0, 0.0, 1.0),
            },
        );
        let p = &s.particles[0];
        assert!((p.pos - Vec3::Z * 10.0).length() < 1e-4, "{:?}", p.pos);
    }

    #[test]
    fn cheap_org_calc_keeps_origin_offset_in_world_space() {
        let mut e = simple_emitter();
        e.velocity = R3 {
            a: [0.0; 3],
            b: [0.0; 3],
        };
        e.gravity = R1 { a: 0.0, b: 0.0 };
        e.origin = R3 {
            a: [10.0, 0.0, 0.0],
            b: [10.0, 0.0, 0.0],
        };
        e.spawn_flags = vec!["cheapOrgCalc".into()];
        let s = sys_with(
            e,
            SpawnAt::Directed {
                pos: Vec3::ZERO,
                dir: Vec3::new(0.0, 0.0, 1.0),
            },
        );
        let p = &s.particles[0];
        assert!((p.pos - Vec3::X * 10.0).length() < 1e-4, "{:?}", p.pos);
    }

    #[test]
    fn accel_integrates_in_retails_frame_order() {
        let mut e = simple_emitter();
        e.velocity = R3 {
            a: [0.0, 0.0, 0.0],
            b: [0.0, 0.0, 0.0],
        };
        e.gravity = R1 { a: 0.0, b: 0.0 };
        e.accel = R3 {
            a: [5.0, 0.0, 0.0],
            b: [5.0, 0.0, 0.0],
        };
        let mut s = sys_with(e, SpawnAt::Point { pos: Vec3::ZERO });
        s.step(0.5, 0.5, None);
        s.step(0.5, 1.0, None);
        // v 2.5 then x 1.25 + 0.625; v 5 then x += 2.5 + 0.625: 5, not
        // the closed form's 2.5 (CoDMP.exe 0x48df20).
        let q = s.build_quads(Vec3::new(100.0, 0.0, 0.0), Vec3::Y, Vec3::Z, 1.0);
        let c = quad_center(&q[0]);
        assert!((c.x - 5.0).abs() < 1e-4, "{c:?}");
    }

    #[test]
    fn stretch_quad_axis_is_parallel_to_velocity_with_curve_length() {
        let mut e = simple_emitter();
        e.kind = "Tail".into();
        e.velocity = R3 {
            a: [0.0, 10.0, 0.0],
            b: [0.0, 10.0, 0.0],
        };
        e.length = Curve {
            start: R1 { a: 6.0, b: 6.0 },
            end: R1 { a: 12.0, b: 12.0 },
            flags: vec!["linear".into()],
        };
        let s = sys_with(e, SpawnAt::Point { pos: Vec3::ZERO });
        // t = 0.5 (life is 1s, spawn at 0): length(t) = 6 + (12-6)*0.5 = 9
        let q = s.build_quads(Vec3::new(100.0, 0.0, 0.0), Vec3::X, Vec3::Z, 0.5);
        assert_eq!(q.len(), 1);
        let edge = Vec3::from(q[0].verts[1]) - Vec3::from(q[0].verts[0]);
        // Long edge parallel to velocity, exactly length(t) (grammar doc R5).
        assert!((edge.normalize() - Vec3::Y).length() < 1e-4, "{edge:?}");
        assert!((edge.length() - 9.0).abs() < 1e-3, "{edge:?}");
    }

    #[test]
    fn stretch_quad_is_anchored_at_the_head_and_trails_backwards() {
        let mut e = simple_emitter();
        e.kind = "Tail".into();
        e.gravity = R1 { a: 0.0, b: 0.0 };
        e.velocity = R3 {
            a: [0.0, 10.0, 0.0],
            b: [0.0, 10.0, 0.0],
        };
        e.length = Curve {
            start: R1 { a: 8.0, b: 8.0 },
            end: R1 { a: 8.0, b: 8.0 },
            flags: vec![],
        };
        let s = sys_with(e, SpawnAt::Point { pos: Vec3::ZERO });
        let q = s.build_quads(Vec3::new(100.0, 0.0, 0.0), Vec3::X, Vec3::Z, 0.0);
        assert_eq!(q.len(), 1);
        let ys: Vec<f32> = q[0].verts.iter().map(|v| v[1]).collect();
        let max = ys.iter().cloned().fold(f32::MIN, f32::max);
        let min = ys.iter().cloned().fold(f32::MAX, f32::min);
        // Head sits exactly on the particle (y = 0), tail 8 units behind it.
        assert!(max.abs() < 1e-4, "{ys:?}");
        assert!((min + 8.0).abs() < 1e-4, "{ys:?}");
    }

    /// `fx/muzzleflashes/heavy.efx` carries a `Particle` with `length 12 ->
    /// 275`; retail evaluates `length` for tails only (grammar doc R3), so
    /// the sprite stays `2 * size` square instead of trailing back down the
    /// barrel.
    #[test]
    fn length_curve_on_a_particle_does_not_stretch_it() {
        let mut e = simple_emitter();
        e.gravity = R1 { a: 0.0, b: 0.0 };
        e.length = Curve {
            start: R1 { a: 12.0, b: 12.0 },
            end: R1 { a: 275.0, b: 275.0 },
            flags: vec!["linear".into()],
        };
        let s = sys_with(e, SpawnAt::Point { pos: Vec3::ZERO });
        let q = s.build_quads(Vec3::new(0.0, -100.0, 0.0), Vec3::X, Vec3::Z, 0.5);
        assert_eq!(q.len(), 1);
        let w = (Vec3::from(q[0].verts[1]) - Vec3::from(q[0].verts[0])).length();
        let h = (Vec3::from(q[0].verts[2]) - Vec3::from(q[0].verts[1])).length();
        // size at t = 0.5 is 3, so 6 across both ways.
        assert!((w - 6.0).abs() < 1e-3, "{w}");
        assert!((h - 6.0).abs() < 1e-3, "{h}");
    }

    #[test]
    fn curve_without_linear_flag_holds_its_start_value() {
        let mut e = simple_emitter();
        e.gravity = R1 { a: 0.0, b: 0.0 };
        e.size = Curve {
            start: R1 { a: 3.0, b: 3.0 },
            end: R1 { a: 40.0, b: 40.0 },
            flags: vec![],
        };
        let s = sys_with(e, SpawnAt::Point { pos: Vec3::ZERO });
        for now in [0.0f32, 0.5, 0.99] {
            let q = s.build_quads(Vec3::new(0.0, -100.0, 0.0), Vec3::X, Vec3::Z, now);
            assert_eq!(q.len(), 1);
            assert!((quad_half_x(&q[0]) - 3.0).abs() < 1e-4, "{now}");
        }
    }

    #[test]
    fn sprite_size_is_a_half_extent() {
        let mut e = simple_emitter();
        e.gravity = R1 { a: 0.0, b: 0.0 };
        e.velocity = R3 {
            a: [0.0; 3],
            b: [0.0; 3],
        };
        e.size = Curve {
            start: R1 { a: 3.0, b: 3.0 },
            end: R1 { a: 3.0, b: 3.0 },
            flags: vec![],
        };
        let s = sys_with(e, SpawnAt::Point { pos: Vec3::ZERO });
        let q = s.build_quads(Vec3::new(0.0, -100.0, 0.0), Vec3::X, Vec3::Z, 0.0);
        assert_eq!(q.len(), 1);
        let width = (Vec3::from(q[0].verts[1]) - Vec3::from(q[0].verts[0])).length();
        let height = (Vec3::from(q[0].verts[2]) - Vec3::from(q[0].verts[1])).length();
        assert!((width - 6.0).abs() < 1e-4, "{width}");
        assert!((height - 6.0).abs() < 1e-4, "{height}");
    }

    /// Retries past the 0.4 chance roll; false if 50 rolls all failed.
    fn spawn_tracer_retrying(
        s: &mut FxSystem,
        impact: Vec3,
        flesh: bool,
        now: f32,
        frame_dt: f32,
    ) -> bool {
        (0..50).any(|_| s.spawn_tracer(Vec3::ZERO, impact, flesh, now, frame_dt))
    }

    /// The one tracer's (tail, head) x along a +x shot, and its width.
    fn tracer_extent(s: &FxSystem, now: f32) -> Option<(f32, f32, f32)> {
        let q = s.build_quads(Vec3::new(0.0, -500.0, 0.0), Vec3::X, Vec3::Z, now);
        let q = q.first()?;
        assert_eq!(q.shader, "gfx/misc/tracer");
        let xs = q.verts.map(|v| v[0]);
        let width = (Vec3::from(q.verts[2]) - Vec3::from(q.verts[1])).length();
        Some((
            xs.iter().cloned().fold(f32::MAX, f32::min),
            xs.iter().cloned().fold(f32::MIN, f32::max),
            width,
        ))
    }

    #[test]
    fn tracer_tail_leaves_the_muzzle_and_the_head_leads_by_its_length() {
        let mut s = FxSystem::new();
        let impact = Vec3::new(4000.0, 0.0, 0.0);
        assert!(spawn_tracer_retrying(&mut s, impact, false, 0.0, 0.0));
        let (tail, head, width) = tracer_extent(&s, 0.0).unwrap();
        assert!(tail.abs() < 1e-3, "tail {tail} must sit on the muzzle");
        assert!((head - 160.0).abs() < 1e-3, "{head}");
        assert!((width - 1.6).abs() < 1e-3, "{width}");
        // p(t) = muzzle + 4500 * t along the shot, head = p + 160.
        let mut now = 0.0;
        for _ in 0..40 {
            s.step(0.02, now + 0.02, None);
            now += 0.02;
            let Some((tail, head, _)) = tracer_extent(&s, now) else {
                break;
            };
            assert!((tail - 4500.0 * now).abs() < 1e-2, "{now}: {tail}");
            assert!((head - tail - 160.0).abs() < 1e-2, "{now}: {head}");
            assert!(head <= impact.x + 1e-2, "{now}: {head} past the impact");
        }
    }

    #[test]
    fn tracer_lives_until_its_tail_reaches_length_short_of_the_impact() {
        let mut s = FxSystem::new();
        // (4000 - 160) / 4500 s = 853.33 ms, truncated to 853.
        assert!(spawn_tracer_retrying(
            &mut s,
            Vec3::new(4000.0, 0.0, 0.0),
            false,
            0.0,
            0.0
        ));
        s.step(0.853, 0.853, None);
        let (_, head, _) = tracer_extent(&s, 0.853).unwrap();
        assert!((head - (0.853 * 4500.0 + 160.0)).abs() < 1e-1, "{head}");
        s.step(0.002, 0.855, None);
        assert!(tracer_extent(&s, 0.855).is_none());
    }

    #[test]
    fn tracer_backdate_moves_the_tail_ahead_never_behind() {
        let mut s = FxSystem::new();
        let impact = Vec3::new(4000.0, 0.0, 0.0);
        for i in 0..200 {
            s.clear();
            let now = i as f32;
            assert!(spawn_tracer_retrying(&mut s, impact, false, now, 0.016));
            let (tail, head, _) = tracer_extent(&s, now).unwrap();
            // `(rand() % 16) / 2` ms, so at most 7 ms of travel.
            assert!((0.0..=4500.0 * 0.007 + 1e-2).contains(&tail), "{tail}");
            assert!((head - tail - 160.0).abs() < 1e-2);
        }
    }

    #[test]
    fn tracer_short_of_its_length_still_starts_at_the_muzzle() {
        // Retail's moving branch has no distance gate.
        let mut s = FxSystem::new();
        assert!(spawn_tracer_retrying(
            &mut s,
            Vec3::new(80.0, 0.0, 0.0),
            false,
            0.0,
            0.0
        ));
        let (tail, head, _) = tracer_extent(&s, 0.0).unwrap();
        assert!(
            tail.abs() < 1e-3 && (head - 160.0).abs() < 1e-3,
            "{tail} {head}"
        );
    }

    #[test]
    fn flesh_tracer_is_a_one_frame_segment_inside_the_shot() {
        let mut s = FxSystem::new();
        for i in 0..100 {
            s.clear();
            let now = i as f32;
            assert!(spawn_tracer_retrying(
                &mut s,
                Vec3::new(1000.0, 0.0, 0.0),
                true,
                now,
                0.016
            ));
            let (near, far, _) = tracer_extent(&s, now).unwrap();
            // near in [50, 1000 - 10), far = min(near + 160, 1000).
            assert!((50.0..990.0).contains(&near), "{near}");
            assert!(
                (far - (near + 160.0).min(1000.0)).abs() < 1e-2,
                "{near} {far}"
            );
            s.step(0.016, now + 0.016, None);
            assert!(tracer_extent(&s, now + 0.016).is_none());
        }
    }

    #[test]
    fn flesh_tracer_is_skipped_for_short_shots() {
        let mut s = FxSystem::new();
        assert!(!spawn_tracer_retrying(
            &mut s,
            Vec3::new(80.0, 0.0, 0.0),
            true,
            0.0,
            0.0
        ));
        assert_eq!(s.counts().0, 0);
    }

    #[test]
    fn clear_drops_particles_and_marks_but_keeps_the_cache() {
        let mut fx = FxSystem::new();
        assert!(spawn_tracer_retrying(
            &mut fx,
            Vec3::new(4000.0, 0.0, 0.0),
            false,
            0.0,
            0.0
        ));
        fx.cache.insert("x".into(), None);
        fx.clear();
        assert_eq!(fx.counts(), (0, 0, 0));
        assert!(fx.cache.contains_key("x"));
    }

    #[test]
    fn stretch_quad_falls_back_to_cam_up_when_velocity_is_parallel_to_view_dir() {
        let mut e = simple_emitter();
        e.kind = "Tail".into();
        e.velocity = R3 {
            a: [0.0, 0.0, 10.0],
            b: [0.0, 0.0, 10.0],
        };
        e.length = Curve {
            start: R1 { a: 6.0, b: 6.0 },
            end: R1 { a: 6.0, b: 6.0 },
            flags: vec![],
        };
        let s = sys_with(e, SpawnAt::Point { pos: Vec3::ZERO });
        // Camera along the velocity axis degenerates the primary cross product.
        let q = s.build_quads(Vec3::new(0.0, 0.0, 100.0), Vec3::X, Vec3::Y, 0.0);
        assert_eq!(q.len(), 1);
        let side_edge = Vec3::from(q[0].verts[2]) - Vec3::from(q[0].verts[1]);
        // size.start is 2; the side edge is 2 * size, not collapsed.
        assert!((side_edge.length() - 4.0).abs() < 1e-3, "{side_edge:?}");
    }

    #[test]
    fn life_zero_emitter_does_not_panic_and_produces_no_quads() {
        // Spawn, step and build at the same `now`, as main.rs does.
        let mut e = simple_emitter();
        e.life = R1 { a: 0.0, b: 0.0 };
        let mut s = sys_with(e, SpawnAt::Point { pos: Vec3::ZERO });
        s.step(0.0, 0.0, None);
        assert!(s.build_quads(Vec3::ZERO, Vec3::X, Vec3::Z, 0.0).is_empty());
        assert_eq!(s.counts(), (0, 0, 0));
    }

    #[test]
    fn directed_spawn_with_absolute_vel_keeps_world_space_velocity() {
        // absoluteVel is a spawnFlags token, not a flags token.
        let mut e = simple_emitter();
        e.velocity = R3 {
            a: [1.0, 0.0, 0.0],
            b: [1.0, 0.0, 0.0],
        };
        e.spawn_flags = vec!["absoluteVel".into()];
        let s = sys_with(
            e,
            SpawnAt::Directed {
                pos: Vec3::ZERO,
                dir: Vec3::new(0.0, 0.0, 1.0),
            },
        );
        let p = &s.particles[0];
        assert!((p.vel - Vec3::X).length() < 1e-4, "{:?}", p.vel);
    }

    #[test]
    fn surface_spawn_rotates_non_decal_velocity_into_normal_hemisphere() {
        let mut e = simple_emitter();
        e.spawn_flags.clear();
        e.velocity = R3 {
            a: [1.0, 0.0, 0.0],
            b: [1.0, 0.0, 0.0],
        };
        let s = sys_with(
            e,
            SpawnAt::Surface {
                pos: Vec3::ZERO,
                normal: Vec3::Z,
            },
        );
        let p = &s.particles[0];
        assert!(p.vel.dot(Vec3::Z) > 0.0, "{:?}", p.vel);
        assert!((p.vel - Vec3::Z).length() < 1e-4, "{:?}", p.vel);
    }

    #[test]
    fn org_on_sphere_distributes_spawn_positions_on_a_sphere() {
        let mut e = simple_emitter();
        e.count = R1 { a: 8.0, b: 8.0 };
        e.spawn_flags = vec!["orgOnSphere".into()];
        e.radius = R1 { a: 50.0, b: 50.0 };
        let s = sys_with(e, SpawnAt::Point { pos: Vec3::ZERO });
        assert_eq!(s.counts().0, 8);
        let positions: Vec<Vec3> = s.particles.iter().map(|p| p.pos).collect();
        assert!(positions.windows(2).any(|w| (w[0] - w[1]).length() > 1.0));
        for p in &positions {
            assert!(p.length() <= 50.0 + 1e-3, "{p:?}");
        }
    }

    /// fx/impacts/default_hit.efx's debris: `orgOnSphere axisFromSphere`,
    /// `velocity 700 0 0`.
    #[test]
    fn axis_from_sphere_points_velocity_radially_outward() {
        let mut e = simple_emitter();
        e.count = R1 { a: 8.0, b: 8.0 };
        e.spawn_flags = vec!["orgOnSphere".into(), "axisFromSphere".into()];
        e.radius = R1 { a: 50.0, b: 50.0 };
        e.velocity = R3 {
            a: [700.0, 0.0, 0.0],
            b: [700.0, 0.0, 0.0],
        };
        let s = sys_with(e, SpawnAt::Point { pos: Vec3::ZERO });
        assert_eq!(s.counts().0, 8);
        for p in &s.particles {
            let radial = p.pos.normalize_or_zero(); // center is the origin
            assert!((p.vel.length() - 700.0).abs() < 1e-2, "{:?}", p.vel);
            assert!(
                (p.vel.normalize_or_zero() - radial).length() < 1e-3,
                "vel={:?} pos={:?}",
                p.vel,
                p.pos
            );
        }
    }

    #[test]
    fn spawn_loads_and_caches_a_real_effect_file_and_warns_once_on_missing() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let mut s = FxSystem::new();
        s.spawn(
            &fs,
            "fx/impacts/default_hit.efx",
            SpawnAt::Point { pos: Vec3::ZERO },
            0.0,
        );
        let (particles, marks, _lights) = s.counts();
        assert!(particles + marks > 0);
        // Warn-once is exercised, not asserted.
        s.spawn(
            &fs,
            "fx/does/not/exist.efx",
            SpawnAt::Point { pos: Vec3::ZERO },
            0.0,
        );
        s.spawn(
            &fs,
            "fx/does/not/exist.efx",
            SpawnAt::Point { pos: Vec3::ZERO },
            0.0,
        );
        assert_eq!(s.counts(), (particles, marks, _lights));
    }

    #[test]
    fn physics_particle_bounces_off_the_floor_and_damps() {
        let world = vcod_common::collision::test_world(&[]);
        let mut e = simple_emitter();
        e.velocity = R3 {
            a: [0.0, 0.0, -50.0],
            b: [0.0, 0.0, -50.0],
        };
        e.gravity = R1 { a: 0.0, b: 0.0 };
        e.flags = vec!["usePhysics".into()];
        // floor top face is at z=0 (see collision::test_world)
        let mut s = sys_with(
            e,
            SpawnAt::Point {
                pos: Vec3::new(0.0, 0.0, 1.0),
            },
        );
        s.step(0.1, 0.1, Some(&world));
        let q = s.build_quads(Vec3::new(0.0, 0.0, 100.0), Vec3::X, Vec3::Y, 0.1);
        assert_eq!(q.len(), 1);
        let c = quad_center(&q[0]);
        // stopped just above the floor, not fallen through it
        assert!(c.z > 0.0 && c.z < 1.0, "{c:?}");
    }

    #[test]
    fn physics_particle_with_impact_kills_is_dropped_on_collision() {
        let world = vcod_common::collision::test_world(&[]);
        let mut e = simple_emitter();
        e.velocity = R3 {
            a: [0.0, 0.0, -50.0],
            b: [0.0, 0.0, -50.0],
        };
        e.gravity = R1 { a: 0.0, b: 0.0 };
        e.flags = vec!["usePhysics".into(), "impactKills".into()];
        let mut s = sys_with(
            e,
            SpawnAt::Point {
                pos: Vec3::new(0.0, 0.0, 1.0),
            },
        );
        s.step(0.1, 0.1, Some(&world));
        assert_eq!(s.counts().0, 0);
    }

    /// Retail tests `cullrange` once, when the effect plays (0x493a1f).
    #[test]
    fn cullrange_is_tested_at_spawn_only_and_zero_never_culls() {
        let far = SpawnAt::Point {
            pos: Vec3::new(1000.0, 0.0, 0.0),
        };
        let mut e = simple_emitter();
        e.cullrange = 10.0;
        let spawn_seen_from = |view: Vec3, e: Emitter| {
            let mut s = FxSystem::new();
            s.set_view(view);
            s.spawn_effect_for_test(Effect { emitters: vec![e] }, far, 0.0);
            s
        };
        let s = spawn_seen_from(Vec3::ZERO, e.clone());
        assert_eq!(s.counts().0, 0);
        // Spawned in range, it draws from anywhere afterwards.
        let s = spawn_seen_from(Vec3::new(995.0, 0.0, 0.0), e);
        assert_eq!(s.build_quads(Vec3::ZERO, Vec3::X, Vec3::Z, 0.0).len(), 1);
        let s = spawn_seen_from(Vec3::ZERO, simple_emitter());
        assert_eq!(s.build_quads(Vec3::ZERO, Vec3::X, Vec3::Z, 0.0).len(), 1);
    }

    /// `fx/shellejects/pistol.efx` from `pak5.pk3`, verbatim but for the
    /// line endings.
    const PISTOL_BRASS: &str = "Emitter\n{\n\tflags\t\t\t\tuseModel usePhysics\n\tlife\t\t\t\t5000\n\tcullrange\t\t\t100\n\tbounce\t\t\t\t0.5\n\tangle\t\t\t\t360 360 360 -360 -360 -360\n\tangleDelta\t\t\t133 133 133 -133 -133 -133\n\tvelocity\t\t\t100 0 0\n\tacceleration\t\t132 132 132 -132 -132 -132\n\tgravity\t\t\t\t-800\n\tsize\n\t{\n\t\tstart\t\t\t0.09\n\t\tend\t\t\t\t0.09\n\t\tflags\t\t\tlinear\n\t}\n\tmodels\n\t[\n\t\txmodel/shellcasing\n\t]\n}\n\nEmitter\n{\n\tname\t\t\t\tCopy of Unnamed Emitter 0\n\tflags\t\t\t\tusePhysics impactKills impactFx\n\tlife\t\t\t\t5000\n\tbounce\t\t\t\t0.6\n\tangle\t\t\t\t360 360 360 -360 -360 -360\n\tangleDelta\t\t\t133 133 133 -133 -133 -133\n\tvelocity\t\t\t100 0 0\n\tgravity\t\t\t\t-800\n\tsize\n\t{\n\t\tstart\t\t\t0.09\n\t\tend\t\t\t\t0.09\n\t\tflags\t\t\tlinear\n\t}\n\timpactfx\n\t[\n\t\tfx/shellejects/shelleject_pistolsound\n\t]\n}\n";

    fn brass_at(pos: Vec3, dir: Vec3) -> FxSystem {
        let mut s = FxSystem::new();
        s.set_view(pos);
        s.spawn_effect_for_test(
            efx::parse(PISTOL_BRASS).unwrap(),
            SpawnAt::Directed { pos, dir },
            0.0,
        );
        s
    }

    #[test]
    fn brass_flies_one_case_model_and_one_unseen_sound_particle() {
        let s = brass_at(Vec3::new(0.0, 0.0, 40.0), Vec3::Y);
        assert_eq!(s.counts(), (2, 0, 0));
        assert!(s.build_quads(Vec3::ZERO, Vec3::X, Vec3::Z, 0.0).is_empty());
        let models = s.models(0.0);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].model, "xmodel/shellcasing");
        for a in models[0].axis {
            assert!((a.length() - 0.09).abs() < 1e-5, "{a:?}");
        }
    }

    #[test]
    fn brass_leaves_along_the_tag_forward_and_falls() {
        let mut s = brass_at(Vec3::new(0.0, 0.0, 40.0), Vec3::Y);
        s.step(0.05, 0.05, None);
        let p = s.models(0.05)[0].origin;
        // 100 u/s out along +Y, give or take the +-132 random acceleration.
        assert!(p.y > 4.0 && p.y < 6.0 && p.x.abs() < 0.5, "{p:?}");
        assert!(p.z < 40.0, "{p:?}");
    }

    #[test]
    fn brass_bounces_then_rests_on_the_floor_and_its_spin_dies_away() {
        let world = vcod_common::collision::test_world(&[]);
        let mut s = brass_at(Vec3::new(0.0, 0.0, 20.0), Vec3::X);
        let mut now = 0.0;
        let mut top_after_first_hit: Option<f32> = None;
        let mut hit = false;
        for _ in 0..200 {
            now += 0.01;
            s.step(0.01, now, Some(&world));
            let z = s.models(now)[0].origin.z;
            if z < 1.0 {
                hit = true;
            }
            if hit {
                top_after_first_hit = Some(top_after_first_hit.map_or(z, |t: f32| t.max(z)));
            }
        }
        // It bounced back up off the floor, below where it started.
        let top = top_after_first_hit.expect("reached the floor");
        assert!(top > 1.0 && top < 20.0, "{top}");
        let p = &s.particles.iter().find(|p| p.debris.is_some()).unwrap();
        assert_eq!(p.vel, Vec3::ZERO);
        assert!(!p.physics, "a resting case stops tracing");
        assert!(p.pos.z >= 0.0 && p.pos.z < 0.5, "{:?}", p.pos);
        let spin = p.debris.as_ref().unwrap().angle_delta.length();
        assert!(spin < 1e-3, "{spin}");
    }

    #[test]
    fn brass_sound_particle_queues_its_impact_effect_and_dies() {
        let world = vcod_common::collision::test_world(&[]);
        let mut s = brass_at(Vec3::new(0.0, 0.0, 10.0), Vec3::X);
        let mut now = 0.0;
        while s.chains.is_empty() && now < 2.0 {
            now += 0.01;
            s.step(0.01, now, Some(&world));
        }
        assert_eq!(s.chains.len(), 1);
        let Chain { path, at, .. } = &s.chains[0];
        assert_eq!(path, "fx/shellejects/shelleject_pistolsound.efx");
        match at {
            SpawnAt::Surface { pos, normal } => {
                assert!(pos.z.abs() < 0.5, "{pos:?}");
                assert_eq!(*normal, Vec3::Z);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            s.counts().0,
            1,
            "the case flies on, the sound particle is gone"
        );
    }

    #[test]
    fn brass_past_its_cullrange_from_the_view_spawns_nothing() {
        let mut s = FxSystem::new();
        s.set_view(Vec3::new(500.0, 0.0, 0.0));
        s.spawn_effect_for_test(
            efx::parse(PISTOL_BRASS).unwrap(),
            SpawnAt::Directed {
                pos: Vec3::ZERO,
                dir: Vec3::X,
            },
            0.0,
        );
        // The model element culls at 100; the sound element has no cullrange.
        assert_eq!(s.counts().0, 1);
        assert!(s.models(0.0).is_empty());
    }

    // Chaining (grammar doc R10).

    /// A system whose cache holds `effects` by path, as if read from a pak.
    fn sys_with_files(effects: &[(&str, Vec<Emitter>)]) -> FxSystem {
        let mut s = FxSystem::new();
        for (path, emitters) in effects {
            s.cache.insert(
                path.to_string(),
                Some(Effect {
                    emitters: emitters.clone(),
                }),
            );
        }
        s
    }

    #[allow(clippy::field_reassign_with_default)]
    fn runner(play: &str) -> Emitter {
        let mut e = Emitter::default();
        e.kind = "FxRunner".into();
        e.play_fx = vec![play.into()];
        e
    }

    fn still_particle() -> Emitter {
        let mut e = simple_emitter();
        e.velocity = R3 {
            a: [0.0; 3],
            b: [0.0; 3],
        };
        e.gravity = R1 { a: 0.0, b: 0.0 };
        e
    }

    fn centers(s: &FxSystem, now: f32) -> Vec<Vec3> {
        s.build_quads(Vec3::new(0.0, 0.0, 1000.0), Vec3::X, Vec3::Y, now)
            .iter()
            .map(quad_center)
            .collect()
    }

    #[test]
    fn runner_plays_its_playfx_count_times_at_its_origin_in_the_spawn_frame() {
        let mut r = runner("fx/child");
        r.count = R1 { a: 3.0, b: 3.0 };
        r.origin = R3 {
            a: [10.0, 0.0, 0.0],
            b: [10.0, 0.0, 0.0],
        };
        let mut s = sys_with_files(&[
            ("fx/parent.efx", vec![r]),
            ("fx/child.efx", vec![still_particle()]),
        ]);
        let at = SpawnAt::Directed {
            pos: Vec3::new(1.0, 2.0, 3.0),
            dir: Vec3::Y,
        };
        s.spawn(&Pk3Fs::empty(), "fx/parent.efx", at, 0.0);
        let c = centers(&s, 0.0);
        assert_eq!(c.len(), 3);
        for p in c {
            assert!((p - Vec3::new(1.0, 12.0, 3.0)).length() < 1e-4, "{p:?}");
        }
    }

    #[test]
    fn runner_delay_holds_its_effect_until_due() {
        let mut r = runner("fx/child");
        r.delay = R1 { a: 500.0, b: 500.0 };
        let mut s = sys_with_files(&[
            ("fx/parent.efx", vec![r]),
            ("fx/child.efx", vec![still_particle()]),
        ]);
        let fs = Pk3Fs::empty();
        s.spawn(
            &fs,
            "fx/parent.efx",
            SpawnAt::Point { pos: Vec3::ZERO },
            0.0,
        );
        assert_eq!(s.counts().0, 0);
        s.spawn_chained(&fs, 0.4);
        assert_eq!(s.counts().0, 0);
        s.spawn_chained(&fs, 0.5);
        assert_eq!(s.counts().0, 1);
        assert!(s.chains.is_empty());
    }

    #[test]
    fn a_runner_past_its_cullrange_starts_nothing() {
        let mut r = runner("fx/child");
        r.cullrange = 100.0;
        let mut s = sys_with_files(&[
            ("fx/parent.efx", vec![r]),
            ("fx/child.efx", vec![still_particle()]),
        ]);
        s.set_view(Vec3::new(0.0, 200.0, 0.0));
        s.spawn(
            &Pk3Fs::empty(),
            "fx/parent.efx",
            SpawnAt::Point { pos: Vec3::ZERO },
            0.0,
        );
        assert_eq!(s.counts().0, 0);
    }

    /// `fx/explosions/moltov_hit.efx` replays itself; vcod stops the chain.
    #[test]
    fn a_self_playing_effect_stops_at_the_chain_depth() {
        let mut s = sys_with_files(&[("fx/loop.efx", vec![runner("fx/loop"), still_particle()])]);
        s.spawn(
            &Pk3Fs::empty(),
            "fx/loop.efx",
            SpawnAt::Point { pos: Vec3::ZERO },
            0.0,
        );
        assert_eq!(s.counts().0, MAX_CHAIN_DEPTH as usize + 1);
        assert!(s.chains.is_empty());
    }

    /// An `Emitter` flying 200 u/s with `density 10` plays its `emitfx`
    /// about every 10 units: the 12 ms walk passes 10 units at 60 ms and
    /// the Newton step pulls the play back to 51 ms (10.2 units). The
    /// second play at 102 ms comes out of the 100 ms frame.
    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn emitter_plays_its_emitfx_every_density_units_along_its_path() {
        let mut e = Emitter::default();
        e.kind = "Emitter".into();
        e.life = R1 { a: 100.0, b: 100.0 };
        e.velocity = R3 {
            a: [200.0, 0.0, 0.0],
            b: [200.0, 0.0, 0.0],
        };
        e.density = R1 { a: 10.0, b: 10.0 };
        e.emit_fx = vec!["fx/puff".into()];
        let mut s = sys_with_files(&[("fx/emit.efx", vec![e])]);
        let at = SpawnAt::Directed {
            pos: Vec3::ZERO,
            dir: Vec3::X,
        };
        s.spawn(&Pk3Fs::empty(), "fx/emit.efx", at, 0.0);
        let mut plays = Vec::new();
        for i in 1..=10 {
            let now = i as f32 * 0.01;
            s.step(0.01, now, None);
            for c in s.chains.drain(..) {
                assert_eq!(c.path, "fx/puff.efx");
                assert_eq!(c.depth, 1);
                plays.push((now, spawn_pos(c.at)));
            }
        }
        assert_eq!(plays.len(), 2, "{plays:?}");
        // The walk steps past `now` once: the 50 ms frame samples 60 ms.
        assert!((plays[0].0 - 0.05).abs() < 1e-4, "{plays:?}");
        assert!((plays[0].1 - Vec3::new(10.2, 0.0, 0.0)).length() < 1e-3);
        assert!((plays[1].1 - Vec3::new(20.4, 0.0, 0.0)).length() < 1e-3);
    }

    #[test]
    fn deathfx_plays_where_the_particle_expires() {
        let mut e = simple_emitter();
        e.life = R1 { a: 100.0, b: 100.0 };
        e.death_fx = vec!["fx/smoke/whitesmoke".into()];
        let mut s = sys_with(e, SpawnAt::Point { pos: Vec3::ZERO });
        s.step(0.1, 0.1, None);
        assert!(s.chains.is_empty());
        let last = s.particles[0].pos;
        s.step(0.01, 0.11, None);
        assert_eq!(s.counts().0, 0);
        assert_eq!(s.chains.len(), 1);
        let c = &s.chains[0];
        assert_eq!(c.path, "fx/smoke/whitesmoke.efx");
        assert_eq!(spawn_pos(c.at), last);
        assert!(matches!(c.at, SpawnAt::Directed { dir, .. } if (dir.length() - 1.0).abs() < 1e-4));
    }

    #[test]
    fn an_impact_kill_plays_no_deathfx() {
        let world = vcod_common::collision::test_world(&[]);
        let mut e = simple_emitter();
        e.velocity = R3 {
            a: [0.0, 0.0, -50.0],
            b: [0.0, 0.0, -50.0],
        };
        e.flags = vec!["usePhysics".into(), "impactKills".into()];
        e.death_fx = vec!["fx/x".into()];
        let mut s = sys_with(
            e,
            SpawnAt::Point {
                pos: Vec3::new(0.0, 0.0, 1.0),
            },
        );
        s.step(0.1, 0.1, Some(&world));
        assert_eq!(s.counts().0, 0);
        assert!(s.chains.is_empty());
    }

    /// A `relative` element of an effect played on a tag rides it; the
    /// others stay where they spawned.
    #[test]
    fn relative_elements_ride_their_tag_and_die_with_it() {
        let mut rel = still_particle();
        rel.flags = vec!["relative".into()];
        rel.origin = R3 {
            a: [5.0, 0.0, 0.0],
            b: [5.0, 0.0, 0.0],
        };
        let mut fixed = rel.clone();
        fixed.flags.clear();
        let mut s = sys_with_files(&[("fx/flash.efx", vec![rel, fixed])]);
        let mut bolts = HashMap::from([(7, (Vec3::ZERO, Vec3::X))]);
        s.set_bolts(&bolts);
        let at = SpawnAt::Directed {
            pos: Vec3::ZERO,
            dir: Vec3::X,
        };
        s.spawn_marked(&Pk3Fs::empty(), "fx/flash.efx", at, Some(7), 0.0, true);
        assert_eq!(s.counts().0, 2);
        bolts.insert(7, (Vec3::new(100.0, 0.0, 0.0), Vec3::Y));
        s.set_bolts(&bolts);
        s.step(0.01, 0.01, None);
        let mut c = centers(&s, 0.01);
        c.sort_by(|a, b| a.x.total_cmp(&b.x));
        assert!((c[0] - Vec3::new(5.0, 0.0, 0.0)).length() < 1e-4, "{c:?}");
        assert!((c[1] - Vec3::new(100.0, 5.0, 0.0)).length() < 1e-4, "{c:?}");
        s.set_bolts(&HashMap::new());
        s.step(0.01, 0.02, None);
        assert_eq!(s.counts().0, 1);
    }

    /// `playFxOnTag` plays on the tag's own axis (syscall 0xe5); its
    /// `relative` elements ride the tag while the caller poses it and die
    /// once it stops, and the rest stay where they spawned.
    #[test]
    fn an_effect_on_a_tag_plays_on_its_axis_and_rides_it() {
        let mut rel = still_particle();
        rel.flags = vec!["relative".into()];
        rel.origin = R3 {
            a: [5.0, 0.0, 0.0],
            b: [5.0, 0.0, 0.0],
        };
        let mut fixed = rel.clone();
        fixed.flags.clear();
        let mut s = sys_with_files(&[("fx/tag.efx", vec![rel, fixed])]);
        let tag = TagBolt {
            entity: 70,
            slot: 1,
        };
        // Forward +y, left -x, up +z.
        let axis = [Vec3::Y, -Vec3::X, Vec3::Z];
        let pose = TagPose {
            origin: Vec3::new(10.0, 0.0, 0.0),
            axis,
        };
        s.spawn_on_tag(&Pk3Fs::empty(), "fx/tag.efx", tag, pose, 0.0);
        assert_eq!(s.tag_bolts(), HashSet::from([tag]));
        for c in centers(&s, 0.0) {
            assert!((c - Vec3::new(10.0, 5.0, 0.0)).length() < 1e-4, "{c:?}");
        }
        let moved = TagPose {
            origin: Vec3::new(0.0, 100.0, 0.0),
            axis,
        };
        s.set_tag_bolts(&HashMap::from([(tag, moved)]));
        s.step(0.01, 0.01, None);
        let mut c = centers(&s, 0.01);
        c.sort_by(|a, b| a.y.total_cmp(&b.y));
        assert!((c[0] - Vec3::new(10.0, 5.0, 0.0)).length() < 1e-4, "{c:?}");
        assert!((c[1] - Vec3::new(0.0, 105.0, 0.0)).length() < 1e-4, "{c:?}");
        s.set_tag_bolts(&HashMap::new());
        s.step(0.01, 0.02, None);
        assert_eq!(s.counts().0, 1);
        assert!(s.tag_bolts().is_empty());
    }

    #[test]
    fn a_relative_element_on_an_unposed_tag_is_not_created() {
        let mut rel = still_particle();
        rel.flags = vec!["relative".into()];
        let mut s = sys_with_files(&[("fx/flash.efx", vec![rel, still_particle()])]);
        let at = SpawnAt::Point { pos: Vec3::ZERO };
        s.spawn_marked(&Pk3Fs::empty(), "fx/flash.efx", at, Some(7), 0.0, true);
        assert_eq!(s.counts().0, 1);
    }

    /// `grenade2.efx`'s `FxRunner` plays `fx/impacts/blast_flash2` at
    /// once, and its `Emitter` plays `radstones1sml` along its path.
    #[test]
    fn retail_grenade_runs_its_playfx_and_emitfx() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let at = SpawnAt::Surface {
            pos: Vec3::new(0.0, 0.0, 1.0),
            normal: Vec3::Z,
        };
        let mut alone = FxSystem::new();
        alone.set_view(Vec3::ZERO);
        alone.spawn_effect_for_test(
            efx::parse(&String::from_utf8_lossy(
                &fs.read("fx/explosions/grenade2.efx").unwrap(),
            ))
            .unwrap(),
            at,
            0.0,
        );
        let mut s = FxSystem::new();
        s.set_view(Vec3::ZERO);
        s.spawn(&fs, "fx/explosions/grenade2.efx", at, 0.0);
        assert!(s.counts().0 > alone.counts().0, "the runner's flash");
        let mut emitted = 0;
        for i in 1..=10 {
            let now = i as f32 * 0.016;
            s.step(0.016, now, None);
            emitted += s.chains.len();
            s.spawn_chained(&fs, now);
        }
        assert!(emitted > 0);
    }
}
