// vc_bloom (crate::bloom): bright-pass, dual-filter blur chain, additive
// composite into the resolved scene before the HUD draws.
struct Params {
    // x: threshold on the display value, y: 1 / identityLight,
    // z: composite scale (intensity * identityLight / levels), w: 1 with vc_hdr
    p: vec4<f32>,
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> params: Params;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: VsOut;
    o.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(p.x, 1.0 - p.y);
    return o;
}

fn texel() -> vec2<f32> {
    return 1.0 / vec2<f32>(textureDimensions(src));
}

fn tap(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(src, samp, uv, 0.0).rgb;
}

// The part of a framebuffer colour above the threshold, measured on the
// display value at gamma 1 (framebuffer / identityLight, capped at white),
// so windowed and full screen bloom the same pixels.
fn bright(c: vec3<f32>) -> vec3<f32> {
    if (params.p.w > 0.5) {
        return bright_hdr(c);
    }
    let d = min(c * params.p.y, vec3<f32>(1.0));
    let m = max(d.r, max(d.g, d.b));
    // 0 at the threshold, 1 at white, eased in so the onset has no edge
    let e = max(m - params.p.x, 0.0) / max(1.0 - params.p.x, 1e-3);
    return d * e * e;
}

// vc_hdr: the scene holds display values past white, uncapped. The knee
// sits past 1.0 on the display value (the windowed/full-screen rule above),
// so only what retail would clip glows; a quadratic soft knee that keeps the
// hue. The cap keeps one stacked flash from swamping the chain.
fn bright_hdr(c: vec3<f32>) -> vec3<f32> {
    let d = clamp(c, vec3<f32>(0.0), vec3<f32>(16.0)) * params.p.y;
    let m = max(d.r, max(d.g, d.b));
    let t = params.p.x;
    let k = 0.5 * t;
    var s = clamp(m - t + k, 0.0, 2.0 * k);
    s = s * s / (4.0 * k + 1e-4);
    return d * max(s, m - t) / max(m, 1e-4);
}

fn down(uv: vec2<f32>, pre: bool) -> vec3<f32> {
    let o = texel();
    var c = array<vec3<f32>, 5>(
        tap(uv),
        tap(uv + vec2<f32>(-o.x, -o.y)),
        tap(uv + vec2<f32>(o.x, -o.y)),
        tap(uv + vec2<f32>(-o.x, o.y)),
        tap(uv + vec2<f32>(o.x, o.y)),
    );
    if (pre) {
        for (var i = 0; i < 5; i++) {
            c[i] = bright(c[i]);
        }
    }
    return (c[0] * 4.0 + c[1] + c[2] + c[3] + c[4]) / 8.0;
}

@fragment
fn fs_prefilter(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(down(in.uv, true), 1.0);
}

@fragment
fn fs_down(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(down(in.uv, false), 1.0);
}

// Blended additively onto the next level up, so each level keeps its own
// blur plus every coarser one.
@fragment
fn fs_up(in: VsOut) -> @location(0) vec4<f32> {
    let h = texel() * 0.5;
    let uv = in.uv;
    let s = tap(uv + vec2<f32>(-2.0 * h.x, 0.0))
        + tap(uv + vec2<f32>(2.0 * h.x, 0.0))
        + tap(uv + vec2<f32>(0.0, -2.0 * h.y))
        + tap(uv + vec2<f32>(0.0, 2.0 * h.y))
        + 2.0 * (tap(uv + vec2<f32>(-h.x, -h.y))
            + tap(uv + vec2<f32>(h.x, -h.y))
            + tap(uv + vec2<f32>(-h.x, h.y))
            + tap(uv + vec2<f32>(h.x, h.y)));
    return vec4<f32>(s / 12.0, 1.0);
}

// Back into framebuffer units (identityLight), so the ramp shows the added
// display value in either window mode.
@fragment
fn fs_composite(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(tap(in.uv) * params.p.z, 0.0);
}
