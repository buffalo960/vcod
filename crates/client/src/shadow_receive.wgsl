
// ---- vc_shadows on surfaces drawn after the mask (crate::shadows) ----
// Appended to shader.wgsl and dynamic_model.wgsl. A stage flagged
// F_SUN_SHADOW sorts past the shadow mask and lays its own colour over what
// is behind it (glass, water, blended decals), so it darkens that colour
// itself by the mask's rule; what shows through was darkened by the mask.

// The head of shadow_mask.wgsl's Mask; share.x is 0 while vc_shadows is off.
struct SunShadow {
    inv_view_proj: mat4x4<f32>,
    light_vp: mat4x4<f32>,
    sun_dir: vec4<f32>,
    share: vec4<f32>,
    map: vec4<f32>,
};
@group(0) @binding(2) var<uniform> sun_shadow_u: SunShadow;
// layer 0 the static world, layer 1 the dynamic casters
@group(0) @binding(3) var t_sun_shadow: texture_depth_2d_array;
@group(0) @binding(4) var s_sun_shadow: sampler_comparison;

const F_SUN_SHADOW: u32 = 8388608u;

fn sun_pcf(uv: vec2<f32>, z: f32, layer: i32, r: i32) -> f32 {
    var sum = 0.0;
    for (var y = -r; y <= r; y++) {
        for (var x = -r; x <= r; x++) {
            let o = vec2<f32>(f32(x), f32(y)) * sun_shadow_u.map.x;
            sum += textureSampleCompareLevel(t_sun_shadow, s_sun_shadow, uv + o, layer, z);
        }
    }
    let n = f32(2 * r + 1);
    return sum / (n * n);
}

// What the mask multiplies a pixel at `p` by, before fog; `face_n` need not
// be unit or face the eye. 1 when vc_shadows is off.
fn sun_shadow(p: vec3<f32>, face_n: vec3<f32>, eye: vec3<f32>) -> f32 {
    if (sun_shadow_u.share.x <= 0.0) {
        return 1.0;
    }
    var n = normalize(face_n);
    if (dot(n, eye - p) < 0.0) {
        n = -n;
    }
    let ndl = dot(n, sun_shadow_u.sun_dir.xyz);
    if (ndl <= 0.0) {
        return 1.0;
    }
    let lp = sun_shadow_u.light_vp * vec4<f32>(p + n * sun_shadow_u.share.w, 1.0);
    let edge = max(abs(lp.x), abs(lp.y));
    if (edge >= 1.0) {
        return 1.0;
    }
    let uv = vec2<f32>(lp.x * 0.5 + 0.5, 0.5 - lp.y * 0.5);
    let z = lp.z - sun_shadow_u.share.z;
    let t = 2.0 * sun_shadow_u.map.x;
    var probe = textureSampleCompareLevel(t_sun_shadow, s_sun_shadow, uv, 1, z);
    for (var i = 0; i < 4; i++) {
        let o = vec2<f32>(select(-t, t, (i & 1) != 0), select(-t, t, (i & 2) != 0));
        probe += textureSampleCompareLevel(t_sun_shadow, s_sun_shadow, uv + o, 1, z);
    }
    if (probe >= 5.0) {
        return 1.0;
    }
    let lit_static = sun_pcf(uv, z, 0, 1);
    let hidden = 1.0 - sun_pcf(uv, z, 1, 2);
    let sun = sun_shadow_u.share.x * ndl;
    let k = sun / (sun + sun_shadow_u.share.y);
    let fade = 1.0 - smoothstep(sun_shadow_u.map.y, 1.0, edge);
    return 1.0 - k * lit_static * hidden * fade;
}
