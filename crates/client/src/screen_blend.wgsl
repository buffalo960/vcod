// The shellshock screen blend (crate::screen_blend): retail's render
// commands 9 and 10. fs_blend draws the last capture over the finished 3D
// view at alpha params.x with the `<screen>` shader's SRC_ALPHA,
// ONE_MINUS_SRC_ALPHA (state 0x65); fs_copy captures the result.
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var<uniform> params: vec4<f32>;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_blend(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(textureLoad(src, vec2<i32>(pos.xy), 0).rgb, params.x);
}

@fragment
fn fs_copy(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(src, vec2<i32>(pos.xy), 0);
}
