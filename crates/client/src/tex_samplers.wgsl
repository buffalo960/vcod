
// r_textureMode's samplers (texture_settings::TexSamplers), in group 0 so a
// filter change rebuilds only the group-0 bind groups. A stage's clampmap
// bundles sample through the clamp one (renderer::STAGE_FLAG_CLAMP).
@group(0) @binding(9) var s_tex: sampler;
@group(0) @binding(10) var s_tex_clamp: sampler;

const F_CLAMP: u32 = 16777216u;

fn sample_image(t: texture_2d<f32>, uv: vec2<f32>, flags: u32) -> vec4<f32> {
    if ((flags & F_CLAMP) != 0u) {
        return textureSample(t, s_tex_clamp, uv);
    }
    return textureSample(t, s_tex, uv);
}
