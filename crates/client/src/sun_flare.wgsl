// The sun flare, its depth probe and the blind / glare wash (sun_flare.rs).
// Vertices arrive in NDC; the scene target holds framebuffer values.

@group(0) @binding(0) var t_flare: texture_2d<f32>;
@group(0) @binding(1) var s_flare: sampler;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = vec4<f32>(in.pos, 1.0);
    // GL's t runs up the screen; the image's first row is t = 0
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

// The wash: rgb added, alpha darkening what is there; the probe writes nothing.
@fragment
fn fs_wash(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}

// `rgbGen vertex`, `alphaGen vertex` over the flare image.
@fragment
fn fs_flare(in: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(t_flare, s_flare, in.uv);
    return vec4<f32>(t.rgb * in.color.rgb, t.a * in.color.a);
}
