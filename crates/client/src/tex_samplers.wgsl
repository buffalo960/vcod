// r_textureMode's samplers (texture_settings::TexSamplers), in group 0 so a
// filter change rebuilds only the group-0 bind groups. A stage's clampmap
// bundles sample through the clamp ones (renderer::STAGE_FLAG_CLAMP). An
// image without mipmaps (one level: `nomipmaps`, the dlight blob) samples
// GL_LINEAR whatever the mode, as retail sets it on images without flag 1
// (CoDMP.exe 0x4ec07e).
@group(0) @binding(9) var s_tex: sampler;
@group(0) @binding(10) var s_tex_clamp: sampler;
@group(0) @binding(11) var s_lin: sampler;
@group(0) @binding(12) var s_lin_clamp: sampler;

const F_CLAMP: u32 = 16777216u;

fn sample_image(t: texture_2d<f32>, uv: vec2<f32>, flags: u32) -> vec4<f32> {
    let mipped = textureNumLevels(t) > 1u;
    if ((flags & F_CLAMP) != 0u) {
        if (mipped) {
            return textureSample(t, s_tex_clamp, uv);
        }
        return textureSample(t, s_lin_clamp, uv);
    }
    if (mipped) {
        return textureSample(t, s_tex, uv);
    }
    return textureSample(t, s_lin, uv);
}
