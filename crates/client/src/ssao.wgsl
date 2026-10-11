// vc_ssao (crate::ssao): GTAO at half resolution off the MSAA depth's
// sample 0, a separable depth-aware blur, and a multiply into the MSAA
// colour between the opaque and the blended halves of the frame.

struct Params {
    // x, y: the projection's x and y scales; z near; w far
    proj: vec4<f32>,
    // xy full size, zw half size, in pixels
    size: vec4<f32>,
    // x strength; y radius in world units; z fog mode (0 off, 1 exp, 2 linear); w fog density
    ao: vec4<f32>,
    // x fog near, y fog far (GL_LINEAR)
    fog: vec4<f32>,
};
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var t_depth: texture_depth_multisampled_2d;
// The previous pass's half-resolution output.
@group(0) @binding(2) var t_in: texture_2d<f32>;

const PI: f32 = 3.14159265;
const SLICES: i32 = 2;
const STEPS: i32 = 4;
// Depth below this is the viewmodel's squeezed range (renderer::VM_DEPTH_RANGE).
const GUN_DEPTH: f32 = 0.2;
// Half-resolution pixels; a near wall would otherwise sample the whole screen.
const MAX_RADIUS_PX: f32 = 64.0;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let q = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(q * 2.0 - 1.0, 0.0, 1.0);
}

// View distance along the forward axis for a depth-buffer value, or 0 for
// the cleared far plane (sky) and the viewmodel.
fn linear_z(d: f32) -> f32 {
    if (d >= 1.0 || d < GUN_DEPTH) {
        return 0.0;
    }
    let n = p.proj.z;
    let f = p.proj.w;
    return f * n / (f - d * (f - n));
}

// View-space position, x right, y down, z forward, for a half-res uv.
fn view_pos(uv: vec2<f32>, z: f32) -> vec3<f32> {
    let ndc = uv * 2.0 - 1.0;
    return vec3<f32>(ndc.x * z / p.proj.x, ndc.y * z / p.proj.y, z);
}

fn half_size() -> vec2<i32> {
    return vec2<i32>(p.size.zw);
}

fn load_z(c: vec2<i32>) -> f32 {
    return textureLoad(t_in, clamp(c, vec2<i32>(0), half_size() - 1), 0).r;
}

// Pass 1: linear depth at half resolution off sample 0 of the top-left
// pixel of each 2x2 block. A resolve would average a silhouette's two
// surfaces into a depth neither has; sample 0 is always a real surface.
@fragment
fn fs_depth(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let full = vec2<i32>(p.size.xy);
    let c = min(vec2<i32>(pos.xy) * 2, full - 1);
    return vec4<f32>(linear_z(textureLoad(t_depth, c, 0)), 0.0, 0.0, 0.0);
}

fn interleaved_noise(c: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(c, vec2<f32>(0.06711056, 0.00583715))));
}

// Pass 2: ground-truth AO (Jimenez et al. 2016, after XeGTAO's slice
// integral): per slice the two horizons within `radius`, cosine-weighted
// against the normal projected into the slice. Out: (visibility, z).
@fragment
fn fs_ao(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let c = vec2<i32>(pos.xy);
    let z = load_z(c);
    if (z <= 0.0) {
        return vec4<f32>(1.0, 0.0, 0.0, 0.0);
    }
    let hs = p.size.zw;
    let uv = (vec2<f32>(c) + 0.5) / hs;
    let pc = view_pos(uv, z);

    // The normal off the nearer neighbour on each axis, so a silhouette
    // doesn't bend it.
    let zl = load_z(c + vec2<i32>(-1, 0));
    let zr = load_z(c + vec2<i32>(1, 0));
    let zu = load_z(c + vec2<i32>(0, -1));
    let zd = load_z(c + vec2<i32>(0, 1));
    let px = 1.0 / hs;
    var dx: vec3<f32>;
    if (zr > 0.0 && (zl <= 0.0 || abs(zr - z) < abs(zl - z))) {
        dx = view_pos(uv + vec2<f32>(px.x, 0.0), zr) - pc;
    } else if (zl > 0.0) {
        dx = pc - view_pos(uv - vec2<f32>(px.x, 0.0), zl);
    } else {
        return vec4<f32>(1.0, z, 0.0, 0.0);
    }
    var dy: vec3<f32>;
    if (zd > 0.0 && (zu <= 0.0 || abs(zd - z) < abs(zu - z))) {
        dy = view_pos(uv + vec2<f32>(0.0, px.y), zd) - pc;
    } else if (zu > 0.0) {
        dy = pc - view_pos(uv - vec2<f32>(0.0, px.y), zu);
    } else {
        return vec4<f32>(1.0, z, 0.0, 0.0);
    }
    let view_v = normalize(-pc);
    var normal = normalize(cross(dy, dx));
    if (dot(normal, view_v) < 0.0) {
        normal = -normal;
    }

    let radius = p.ao.y;
    let radius_px = min(radius * p.proj.x * 0.5 * hs.x / z, MAX_RADIUS_PX);
    if (radius_px < 1.0) {
        return vec4<f32>(1.0, z, 0.0, 0.0);
    }
    let falloff_range = 0.615 * radius;
    let falloff_mul = -1.0 / falloff_range;
    let falloff_add = (radius - falloff_range) / falloff_range + 1.0;

    let noise_slice = interleaved_noise(pos.xy);
    let noise_step = interleaved_noise(pos.xy + vec2<f32>(5.588238, 5.588238));
    var visibility = 0.0;
    for (var s = 0; s < SLICES; s++) {
        let phi = (f32(s) + noise_slice) * PI / f32(SLICES);
        let omega = vec2<f32>(cos(phi), sin(phi));
        let dir = vec3<f32>(omega, 0.0);
        let ortho = dir - dot(dir, view_v) * view_v;
        let axis = normalize(cross(ortho, view_v));
        let proj_n = normal - axis * dot(normal, axis);
        let proj_n_len = length(proj_n);
        if (proj_n_len < 1e-4) {
            visibility += 1.0;
            continue;
        }
        let sign_n = select(-1.0, 1.0, dot(ortho, proj_n) >= 0.0);
        let cos_n = clamp(dot(proj_n, view_v) / proj_n_len, 0.0, 1.0);
        let n = sign_n * acos(cos_n);
        let low0 = cos(n + PI * 0.5);
        let low1 = cos(n - PI * 0.5);
        var hcos0 = low0;
        var hcos1 = low1;
        for (var j = 0; j < STEPS; j++) {
            var t = (f32(j) + noise_step) / f32(STEPS);
            t = t * t;
            let off = omega * (t * (radius_px - 1.0) + 1.0);
            let c0 = vec2<i32>(floor(pos.xy + off));
            let c1 = vec2<i32>(floor(pos.xy - off));
            let z0 = load_z(c0);
            let z1 = load_z(c1);
            if (z0 > 0.0) {
                let d0 = view_pos((vec2<f32>(c0) + 0.5) / hs, z0) - pc;
                let l0 = length(d0);
                let w0 = clamp(l0 * falloff_mul + falloff_add, 0.0, 1.0);
                hcos0 = max(hcos0, mix(low0, dot(d0 / l0, view_v), w0));
            }
            if (z1 > 0.0) {
                let d1 = view_pos((vec2<f32>(c1) + 0.5) / hs, z1) - pc;
                let l1 = length(d1);
                let w1 = clamp(l1 * falloff_mul + falloff_add, 0.0, 1.0);
                hcos1 = max(hcos1, mix(low1, dot(d1 / l1, view_v), w1));
            }
        }
        var h0 = -acos(clamp(hcos1, -1.0, 1.0));
        var h1 = acos(clamp(hcos0, -1.0, 1.0));
        h0 = n + clamp(h0 - n, -PI * 0.5, PI * 0.5);
        h1 = n + clamp(h1 - n, -PI * 0.5, PI * 0.5);
        let arc0 = (cos_n + 2.0 * h0 * sin(n) - cos(2.0 * h0 - n)) * 0.25;
        let arc1 = (cos_n + 2.0 * h1 * sin(n) - cos(2.0 * h1 - n)) * 0.25;
        visibility += proj_n_len * (arc0 + arc1);
    }
    return vec4<f32>(clamp(visibility / f32(SLICES), 0.0, 1.0), z, 0.0, 0.0);
}

// Pass 3, horizontal then vertical: 5 taps with half-weight rims, one
// period of the 4x4 noise, skipping taps off the centre's surface.
fn blur(pos: vec4<f32>, dir: vec2<i32>) -> vec4<f32> {
    let c = vec2<i32>(pos.xy);
    let z = textureLoad(t_in, c, 0).g;
    if (z <= 0.0) {
        return vec4<f32>(1.0, 0.0, 0.0, 0.0);
    }
    var sum = 0.0;
    var wsum = 0.0;
    for (var i = -2; i <= 2; i++) {
        let s = textureLoad(t_in, clamp(c + dir * i, vec2<i32>(0), half_size() - 1), 0).rg;
        if (s.g <= 0.0) {
            continue;
        }
        let rim = select(1.0, 0.5, abs(i) == 2);
        let w = rim * max(0.0, 1.0 - abs(s.g - z) / (z * 0.05));
        sum += s.r * w;
        wsum += w;
    }
    return vec4<f32>(sum / max(wsum, 1e-4), z, 0.0, 0.0);
}

@fragment
fn fs_blur_h(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    return blur(pos, vec2<i32>(1, 0));
}

@fragment
fn fs_blur_v(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    return blur(pos, vec2<i32>(0, 1));
}

fn fog_amount(z: f32) -> f32 {
    if (p.ao.z == 1.0) {
        return 1.0 - exp(-p.ao.w * z);
    }
    if (p.ao.z == 2.0) {
        return clamp((z - p.fog.x) / max(p.fog.y - p.fog.x, 0.0001), 0.0, 1.0);
    }
    return 0.0;
}

// Pass 4's triangle sits at the viewmodel's far depth and passes only
// where the stored depth is further, so the depth test keeps the gun's
// samples out one by one.
@vertex
fn vs_apply(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let q = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(q * 2.0 - 1.0, GUN_DEPTH, 1.0);
}

// Pass 4, per pixel: a joint bilateral upsample of the blurred AO at
// sample 0's depth, multiplied into the colour (blend dst * src). The sky,
// the viewmodel and a pixel with no matching half-res texel keep their
// colour; fog fades the darkening out with its own factor.
@fragment
fn fs_apply(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let z = linear_z(textureLoad(t_depth, vec2<i32>(pos.xy), 0));
    if (z <= 0.0) {
        discard;
    }
    // half-res texel j holds full-res pixel 2j (fs_depth)
    let h = pos.xy * 0.5 - 0.25;
    let base = vec2<i32>(floor(h));
    let f = fract(h);
    var sum = 0.0;
    var wsum = 0.0;
    var best = 1e9;
    for (var k = 0; k < 4; k++) {
        let o = vec2<i32>(k & 1, k >> 1u);
        let s = textureLoad(t_in, clamp(base + o, vec2<i32>(0), half_size() - 1), 0).rg;
        if (s.g <= 0.0) {
            continue;
        }
        let bw = mix(1.0 - f.x, f.x, f32(o.x)) * mix(1.0 - f.y, f.y, f32(o.y));
        let dz = abs(s.g - z) / z;
        best = min(best, dz);
        let w = (bw + 1e-3) / (1e-3 + dz);
        sum += s.r * w;
        wsum += w;
    }
    // a sliver the half-res pass missed (a thin edge, the gun's rim)
    if (wsum <= 0.0 || best > 0.1) {
        discard;
    }
    let ao = pow(clamp(sum / wsum, 0.0, 1.0), p.ao.x);
    let k = mix(ao, 1.0, fog_amount(z));
    // the multiply would change no byte; skip the blend's bandwidth
    if (k > 0.998) {
        discard;
    }
    return vec4<f32>(k, k, k, 1.0);
}
