// vc_hdr's tonemap and vc_grade / vc_vignette / vc_grain (crate::post): the
// world target in, retail's framebuffer bytes out, before the HUD draws.
// Everything here works on the display value at gamma 1, framebuffer /
// identityLight, and writes framebuffer units back, so the gamma pass's
// ramp (r_gamma, the overbright doubling) still runs after it.
struct Params {
    // x: identityLight, y: 1 with vc_hdr, z: vc_grade, w: vc_vignette
    a: vec4<f32>,
    // x: vc_grain, y: grain seed (changes 24 times a second), zw: unused
    b: vec4<f32>,
};

@group(0) @binding(0) var world: texture_2d<f32>;
@group(0) @binding(1) var<uniform> params: Params;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

// Display values up to KNEE pass unchanged (retail's look); above it a
// Reinhard-extended shoulder, C1 at the knee, takes WHITE to display white
// where retail clips at 1. Full screen one opaque draw reaches 2.0, and
// most stacked blends (the sun, flares, fx) land between 1 and 2.
const KNEE: f32 = 0.9;
const WHITE: f32 = 2.0;

fn tonemap(d: vec3<f32>) -> vec3<f32> {
    let x = max(d - KNEE, vec3<f32>(0.0)) / (1.0 - KNEE);
    let w = (WHITE - KNEE) / (1.0 - KNEE);
    let f = x * (1.0 + x / (w * w)) / (1.0 + x);
    return select(d, KNEE + (1.0 - KNEE) * f, d > vec3<f32>(KNEE));
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

// A period-film grade at strength 1: a third of the saturation gone, warm
// highlights, cool shadows. Black and white stay put.
fn grade(c: vec3<f32>) -> vec3<f32> {
    let l = luma(c);
    var g = mix(c, vec3<f32>(l), 0.3);
    g += vec3<f32>(0.05, 0.02, -0.04) * l * l;
    g += vec3<f32>(-0.03, 0.0, 0.04) * 4.0 * l * (1.0 - l) * (1.0 - l);
    return g;
}

fn hash(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(0.1031, 0.1030));
    let r = q + dot(q, q.yx + 33.33);
    return fract((r.x + r.y) * r.x);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let il = params.a.x;
    let fb = max(textureLoad(world, vec2<i32>(pos.xy), 0).rgb, vec3<f32>(0.0));
    let d = fb / il;
    var c = select(min(d, vec3<f32>(1.0)), tonemap(d), params.a.y > 0.5);
    if (params.a.z > 0.0) {
        c = mix(c, grade(c), params.a.z);
    }
    if (params.a.w > 0.0) {
        let size = vec2<f32>(textureDimensions(world));
        // 0 at the centre, 1 in the corners, the frame's aspect kept
        let r2 = dot((pos.xy - 0.5 * size) * 2.0, (pos.xy - 0.5 * size) * 2.0) / dot(size, size);
        c *= 1.0 - min(params.a.w, 2.0) * 0.4 * smoothstep(0.15, 1.0, r2);
    }
    if (params.b.x > 0.0) {
        // monochrome, strongest in the mid-tones, as film grain reads
        let n = hash(pos.xy + params.b.y * vec2<f32>(37.0, 17.0)) + hash(pos.xy * 1.37 + params.b.y) - 1.0;
        let l = clamp(luma(c), 0.0, 1.0);
        c += vec3<f32>(n * params.b.x * 0.06 * (1.0 - (2.0 * l - 1.0) * (2.0 * l - 1.0)));
    }
    return vec4<f32>(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)) * il, 1.0);
}
