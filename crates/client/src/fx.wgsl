// World-space fx quads: depth-tested, never writing depth. Per-vertex rgba
// multiplies the texture; `Renderer::set_fx_quads` has already scaled it by
// the material's gen. `fs_main` is straight alpha, `fs_additive` is
// `GL_ONE GL_ONE` for glow materials (`assets::Shaders::additive`).

// Same uniform as shader.wgsl; the fog fields are documented there.
struct Camera {
    view_proj: mat4x4<f32>,
    time_pad: vec4<f32>,
    eye_fog_mode: vec4<f32>,
    fog_color_density: vec4<f32>,
    fog_range: vec4<f32>,
    view_fwd: vec4<f32>,
}
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var tex: texture_2d<f32>;

// vc_softParticles (crate::soft_particles); only the soft entry points
// read group 2, and only their pipelines declare it.
struct SoftParams {
    // x near, y far, z vc_softParticles
    p: vec4<f32>,
}
@group(2) @binding(0) var<uniform> soft: SoftParams;
@group(2) @binding(1) var scene_z: texture_2d<f32>;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) rgba: vec4<f32>,
    @location(3) size: f32,
}
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) rgba: vec4<f32>,
    @location(2) world_pos: vec3<f32>,
    @location(3) size: f32,
}
@vertex fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = camera.view_proj * vec4(in.pos, 1.0);
    out.uv = in.uv;
    out.rgba = in.rgba;
    out.world_pos = in.pos;
    out.size = in.size;
    return out;
}

// glFog GL_EXP / GL_LINEAR factors (see shader.wgsl); depth along the view
// forward, the fixed-function fog coordinate without GL_NV_fog_distance.
fn fog_amount(world_pos: vec3<f32>) -> f32 {
    let mode = camera.eye_fog_mode.w;
    if (mode == 0.0) {
        return 0.0;
    }
    let d = max(dot(world_pos - camera.eye_fog_mode.xyz, camera.view_fwd.xyz), 0.0);
    if (mode == 1.0) {
        return 1.0 - exp(-camera.fog_color_density.a * d);
    }
    let span = max(camera.fog_range.y - camera.fog_range.x, 0.0001);
    return clamp((d - camera.fog_range.x) / span, 0.0, 1.0);
}

// Fogged like the surface a decal sits on, so bullet holes and smoke
// take the same colour as the wall behind them.
fn shade(in: VsOut) -> vec4<f32> {
    let t = sample_image(tex, in.uv, F_CLAMP);
    let rgb = mix(t.rgb * in.rgba.rgb, camera.fog_color_density.rgb, fog_amount(in.world_pos));
    return vec4(rgb, t.a * in.rgba.a);
}

// A sprite without `useAlpha` carries its fade in its rgb and alpha 1
// (`fx::sim`, CoDMP.exe 0x48e7d0); a `GL_ONE GL_ONE` material reads no
// source alpha, so `Renderer::set_fx_quads` hands it alpha 1 too. What alpha
// is left scales the colour. Texture alpha is never read.
// Additive fog fades toward black, not the fog colour (Q3 fogColorMask): adding
// fog colour would brighten glows the further into the fog they sit.
fn shade_additive(in: VsOut) -> vec4<f32> {
    let t = sample_image(tex, in.uv, F_CLAMP);
    let rgb = t.rgb * in.rgba.rgb * in.rgba.a * (1.0 - fog_amount(in.world_pos));
    return vec4(rgb, 0.0);
}

@fragment fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return shade(in);
}

@fragment fn fs_additive(in: VsOut) -> @location(0) vec4<f32> {
    return shade_additive(in);
}

// The fade over the gap to the opaque scene behind: a sprite's fade
// distance is half its smaller half-extent times vc_softParticles, at least
// a unit and at most 32, so a spark at a wall stays visible and a big
// explosion card does not vanish into the ground.
fn soft_fade(frag: vec4<f32>, size: f32) -> f32 {
    let n = soft.p.x;
    let f = soft.p.y;
    let scene = textureLoad(scene_z, vec2<i32>(frag.xy), 0).r;
    let z = f * n / (f - frag.z * (f - n));
    let dist = soft.p.z * clamp(size * 0.5, 1.0, 32.0);
    return clamp((scene - z) / dist, 0.0, 1.0);
}

@fragment fn fs_soft(in: VsOut) -> @location(0) vec4<f32> {
    let c = shade(in);
    return vec4(c.rgb, c.a * soft_fade(in.clip, in.size));
}

@fragment fn fs_soft_additive(in: VsOut) -> @location(0) vec4<f32> {
    let c = shade_additive(in);
    return vec4(c.rgb * soft_fade(in.clip, in.size), 0.0);
}
