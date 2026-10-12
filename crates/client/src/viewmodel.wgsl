// First-person viewmodel: the world's fov with its own near plane, bind pose
// baked into the vertices, placed in view space by the motion transform.
// Each stage of a surface's material draws as dynamic_model.wgsl draws an
// entity's: lit per vertex by its light set (here in view space), or its
// rgbGen colour, with alphaGen, alphaFunc, tcMods and tcGen environment.

// One GL light; `entity_light::GpuLight`.
struct EntLight {
    pos: vec4<f32>,      // w 0: xyz is the unit vector towards the light
    diffuse: vec4<f32>,  // w spot exponent
    ambient: vec4<f32>,  // w cosine of the spot cutoff, below -1 for none
    atten: vec4<f32>,    // constant, linear, quadratic
    spot_dir: vec4<f32>,
};
struct LightSet {
    ambient: vec4<f32>,  // light model ambient; w the light count
    lights: array<EntLight, 8>,
};

struct VmUniform {
    proj: mat4x4<f32>,      // viewmodel projection (world fov, own near plane)
    model: mat4x4<f32>,     // motion transform (view space)
    lights: LightSet,       // view space
    misc: vec4<f32>,        // x identityLight
};
@group(0) @binding(0) var<uniform> u: VmUniform;
@group(1) @binding(0) var t_diffuse: texture_2d<f32>;
// View-space skin matrices; identity reproduces the baked bind pose.
@group(2) @binding(0) var<uniform> bones: array<mat4x4<f32>, 64>;

// The stage's `renderer::StageParams`; only the fields read here are named.
struct StageParams {
    uv0: mat3x2<f32>,
    uv1: mat3x2<f32>,
    turb01: vec4<f32>,
    // rgbGen colour and alphaGen value
    tint: vec4<f32>,
    flags: u32,
};
@group(3) @binding(0) var<uniform> stage: StageParams;

const F_AF_GT0: u32 = 16u;
const F_AF_LT128: u32 = 32u;
const F_AF_GE128: u32 = 48u;
const F_TINT_LIGHT: u32 = 4096u;
const F_LIT: u32 = 8192u;
const F_BUNDLE0_ENV: u32 = 16384u;
const ATEST128: f32 = 0.5019607843137255;

struct VsOut {
    @builtin(position) @invariant pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // GL's vertex colour, framebuffer units
    @location(1) light: vec3<f32>,
};

// tcGen environment, as dynamic_model.wgsl's `env_uv`.
fn env_uv(to_eye: vec3<f32>, n: vec3<f32>) -> vec2<f32> {
    let v = normalize(to_eye);
    let r = 2.0 * dot(n, v) * n - v;
    return vec2<f32>(0.5 + 0.5 * r.y, 0.5 - 0.5 * r.z);
}

// Same as `gl_lighting` in dynamic_model.wgsl.
fn gl_lighting(ls: LightSet, p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var c = ls.ambient.rgb;
    let count = min(u32(ls.ambient.w), 8u);
    for (var i = 0u; i < count; i++) {
        let l = ls.lights[i];
        var to_l = l.pos.xyz;
        var att = 1.0;
        if (l.pos.w != 0.0) {
            let v = l.pos.xyz - p;
            let d = length(v);
            to_l = v / max(d, 1e-6);
            att = 1.0 / (l.atten.x + (l.atten.y + l.atten.z * d) * d);
            if (l.ambient.w >= -1.0) {
                let s = dot(-to_l, normalize(l.spot_dir.xyz));
                if (s < l.ambient.w) {
                    att = 0.0;
                } else if (l.diffuse.w > 0.0) {
                    att *= pow(max(s, 0.0), l.diffuse.w);
                }
            }
        }
        c += att * (l.ambient.rgb + max(dot(n, to_l), 0.0) * l.diffuse.rgb);
    }
    return clamp(c, vec3(0.0), vec3(1.0));
}

@vertex
fn vs_main(
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) bi: vec4<u32>,
    @location(4) bw: vec4<f32>,
) -> VsOut {
    var p = vec4<f32>(0.0);
    var n = vec3<f32>(0.0);
    for (var i = 0u; i < 4u; i++) {
        let m = bones[bi[i]];
        p += bw[i] * (m * vec4<f32>(pos, 1.0));
        // rigid bone transforms, so the upper 3x3 is valid for normals
        n += bw[i] * (mat3x3<f32>(m[0].xyz, m[1].xyz, m[2].xyz) * normal);
    }
    let view = u.model * p;
    var out: VsOut;
    out.pos = u.proj * view;
    let vn = normalize((u.model * vec4<f32>(n, 0.0)).xyz);
    var base = uv;
    if ((stage.flags & F_BUNDLE0_ENV) != 0u) {
        // view space to the weapon's frame; the eye is the view origin
        let to_local = transpose(mat3x3<f32>(
            normalize(u.model[0].xyz),
            normalize(u.model[1].xyz),
            normalize(u.model[2].xyz),
        ));
        base = env_uv(to_local * -view.xyz, to_local * vn);
    }
    out.uv = stage.uv0 * vec3<f32>(base, 1.0);
    out.light = gl_lighting(u.lights, view.xyz, vn);
    return out;
}

// alphaFunc decode, as dynamic_model.wgsl's.
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

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let tex = sample_image(t_diffuse, in.uv, stage.flags);
    var c: vec3<f32>;
    if ((stage.flags & F_LIT) != 0u) {
        c = in.light;
    } else {
        c = stage.tint.rgb;
        if ((stage.flags & F_TINT_LIGHT) != 0u) {
            c = c * u.misc.x;
        }
    }
    let a = tex.a * stage.tint.a;
    if (!alphafunc_pass(a)) { discard; }
    return vec4<f32>(tex.rgb * c, a);
}
