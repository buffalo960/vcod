// vc_softParticles (crate::soft_particles): sample 0 of the MSAA depth as
// linear view depth, for the fx pipelines after the frame's split.

struct Params {
    // x near, y far, z vc_softParticles
    p: vec4<f32>,
};
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var t_depth: texture_depth_multisampled_2d;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let q = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(q * 2.0 - 1.0, 0.0, 1.0);
}

// The cleared far plane reads as `far`, past any sprite's fade.
@fragment
fn fs_copy(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let d = textureLoad(t_depth, vec2<i32>(pos.xy), 0);
    let n = params.p.x;
    let f = params.p.y;
    return vec4<f32>(f * n / (f - d * (f - n)), 0.0, 0.0, 0.0);
}
