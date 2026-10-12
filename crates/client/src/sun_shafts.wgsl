// vc_sunShafts (crate::sun_shafts): the sky's pixels near the sun at half
// resolution, a radial blur toward the sun's screen position, and an
// additive composite into the resolved scene before bloom and the HUD.

struct Params {
    // xy the sun in uv (top-left origin), z width / height, w strength * fade
    sun: vec4<f32>,
    // x 1 / identityLight
    tone: vec4<f32>,
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var t_depth: texture_depth_multisampled_2d;

// The sky source fades to nothing this far from the sun (uv, aspect-corrected).
const SOURCE_RADIUS: f32 = 0.5;
// Sky dimmer than this display value casts nothing, so an overcast sky
// shafts from its bright patches instead of glowing whole.
const THRESHOLD: f32 = 0.6;
const SAMPLES: i32 = 64;
// The march covers this share of the way to the sun.
const DENSITY: f32 = 0.95;
const DECAY: f32 = 0.965;

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

// Pass 1, half resolution: the scene where sample 0 of the top-left pixel
// of the 2x2 block is the cleared far plane (the sky), weighted toward the
// sun. Everything else is black, so only open sky casts shafts.
@fragment
fn fs_mask(in: VsOut) -> @location(0) vec4<f32> {
    let full = vec2<i32>(textureDimensions(t_depth));
    let c = min(vec2<i32>(in.pos.xy) * 2, full - 1);
    if (textureLoad(t_depth, c, 0) < 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let d = (in.uv - params.sun.xy) * vec2<f32>(params.sun.z, 1.0);
    let w = 1.0 - smoothstep(0.0, SOURCE_RADIUS, length(d));
    let rgb = textureSampleLevel(src, samp, in.uv, 0.0).rgb;
    let disp = min(rgb * params.tone.x, vec3<f32>(1.0));
    let m = max(disp.r, max(disp.g, disp.b));
    let e = max(m - THRESHOLD, 0.0) / (1.0 - THRESHOLD);
    return vec4<f32>(rgb * e * w * w, 1.0);
}

// Pass 2, half resolution: each pixel marches toward the sun summing the
// mask with a decaying weight (Mitchell, GPU Gems 3, ch. 13).
@fragment
fn fs_rays(in: VsOut) -> @location(0) vec4<f32> {
    let step = (in.uv - params.sun.xy) * (DENSITY / f32(SAMPLES));
    var uv = in.uv;
    var decay = 1.0;
    var sum = vec3<f32>(0.0);
    var norm = 0.0;
    for (var i = 0; i < SAMPLES; i++) {
        uv -= step;
        sum += textureSampleLevel(src, samp, uv, 0.0).rgb * decay;
        norm += decay;
        decay *= DECAY;
    }
    return vec4<f32>(sum / norm, 1.0);
}

// Pass 3, full resolution, blended GL_ONE GL_ONE into the resolved scene.
@fragment
fn fs_composite(in: VsOut) -> @location(0) vec4<f32> {
    let rays = textureSampleLevel(src, samp, in.uv, 0.0).rgb;
    return vec4<f32>(rays * params.sun.w, 0.0);
}
