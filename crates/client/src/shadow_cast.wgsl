// vc_shadows caster passes (crate::shadows): depth from the sun's side.
// World soups and moved brush models through `vs_world`, snapshot
// entities skinned as dynamic_model.wgsl does, alpha-tested by their
// first depth-writing stage.

// The light's view-projection, times a moved brush model's pose.
@group(0) @binding(0) var<uniform> light_vp: mat4x4<f32>;

@vertex
fn vs_world(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
    return light_vp * vec4<f32>(pos, 1.0);
}

// A full-viewport triangle at the far depth: clears a static piece.
@vertex
fn vs_clear(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 1.0, 1.0);
}

@group(1) @binding(0) var t_skin: texture_2d<f32>;
@group(0) @binding(1) var s_skin: sampler;
@group(2) @binding(0) var<storage, read> bones: array<mat4x4<f32>>;

// The stage's `renderer::StageParams`; only the fields read here are named.
struct StageParams {
    uv0: mat3x2<f32>,
    uv1: mat3x2<f32>,
    turb01: vec4<f32>,
    tint: vec4<f32>,
    flags: u32,
};
@group(3) @binding(0) var<uniform> stage: StageParams;

const F_AF_GT0: u32 = 16u;
const F_AF_LT128: u32 = 32u;
const F_AF_GE128: u32 = 48u;
const ATEST128: f32 = 0.5019607843137255;

fn alphafunc_pass(a: f32) -> bool {
    if ((stage.flags & F_AF_GE128) == F_AF_GE128) {
        return a >= ATEST128;
    }
    if ((stage.flags & F_AF_LT128) != 0u) {
        return a < ATEST128;
    }
    if ((stage.flags & F_AF_GT0) != 0u) {
        return a > 0.0;
    }
    return true;
}

struct EntOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_entity(
    @location(0) pos: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) bi: vec4<u32>,
    @location(4) bw: vec4<f32>,
    @location(5) m0: vec4<f32>,
    @location(6) m1: vec4<f32>,
    @location(7) m2: vec4<f32>,
    @location(8) m3: vec4<f32>,
    @location(9) bone_base: u32,
) -> EntOut {
    var p = vec4<f32>(0.0);
    for (var i = 0u; i < 4u; i++) {
        p += bw[i] * (bones[bone_base + bi[i]] * vec4<f32>(pos, 1.0));
    }
    var out: EntOut;
    out.clip = light_vp * (mat4x4<f32>(m0, m1, m2, m3) * p);
    out.uv = stage.uv0 * vec3<f32>(uv, 1.0);
    return out;
}

@fragment
fn fs_entity(in: EntOut) {
    let a = textureSample(t_skin, s_skin, in.uv).a * stage.tint.a;
    if (!alphafunc_pass(a)) {
        discard;
    }
}
