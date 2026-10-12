
// ---- vc_shadows on surfaces drawn after the mask (crate::shadows) ----
// Appended to shader.wgsl and dynamic_model.wgsl. A stage flagged
// F_SUN_SHADOW sorts past the shadow mask and lays its own colour over what
// is behind it (glass, water, blended decals), so it darkens that colour
// itself by the mask's rule; what shows through was darkened by the mask.

// shadow_mask.wgsl's Mask; share.x is 0 while vc_shadows is off.
struct SunShadow {
    inv_view_proj: mat4x4<f32>,
    light_vp: mat4x4<f32>,
    sun_dir: vec4<f32>,
    share: vec4<f32>,
    map: vec4<f32>,
    fog: array<vec4<f32>, 4>,
    static_at: vec4<f32>,
};
@group(0) @binding(2) var<uniform> sun_shadow_u: SunShadow;
// the dynamic casters over the window
@group(0) @binding(3) var t_sun_shadow: texture_depth_2d;
@group(0) @binding(4) var s_sun_shadow: sampler_comparison;
// the static world, wrapping (shadow_mask.wgsl's static_at)
@group(0) @binding(5) var t_sun_static: texture_depth_2d;
@group(0) @binding(6) var s_sun_static: sampler_comparison;

const F_SUN_SHADOW: u32 = 8388608u;

fn sun_pcf(uv: vec2<f32>, z: f32, is_static: bool, r: i32) -> f32 {
    var sum = 0.0;
    let st = sun_shadow_u.static_at;
    let suv = st.xy + uv * st.z;
    for (var y = -r; y <= r; y++) {
        for (var x = -r; x <= r; x++) {
            let o = vec2<f32>(f32(x), f32(y));
            if (is_static) {
                sum += textureSampleCompareLevel(t_sun_static, s_sun_static, suv + o * st.w, z);
            } else {
                sum += textureSampleCompareLevel(t_sun_shadow, s_sun_shadow, uv + o * sun_shadow_u.map.x, z);
            }
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
    var probe = textureSampleCompareLevel(t_sun_shadow, s_sun_shadow, uv, z);
    for (var i = 0; i < 4; i++) {
        let o = vec2<f32>(select(-t, t, (i & 1) != 0), select(-t, t, (i & 2) != 0));
        probe += textureSampleCompareLevel(t_sun_shadow, s_sun_shadow, uv + o, z);
    }
    if (probe >= 5.0) {
        return 1.0;
    }
    let lit_static = sun_pcf(uv, z, true, 1);
    let hidden = 1.0 - sun_pcf(uv, z, false, 2);
    let sun = sun_shadow_u.share.x * ndl;
    let k = sun / (sun + sun_shadow_u.share.y);
    let fade = 1.0 - smoothstep(sun_shadow_u.map.y, 1.0, edge);
    return 1.0 - k * lit_static * hidden * fade;
}
