// deformVertexes, appended to every module that draws a shader stage
// (world, shadow cutouts, entities, viewmodel). The module's `stage`
// carries `deform` and `deform_kinds` (renderer::StageParams) and
// noise.wgsl rides along. Retail runs a shader's records in script order
// on the same vertices (CoDMP.exe 0x5149b0), so each reads the position
// and normal the one before it left.

// renderer::DEFORM_*, retail's deform numbers; a renderer test holds them
const DEFORM_WAVE: u32 = 1u;
const DEFORM_FLAP_S: u32 = 2u;
const DEFORM_FLAP_T: u32 = 3u;
const DEFORM_NORMAL: u32 = 4u;
const DEFORM_BULGE: u32 = 6u;
const DEFORM_MOVE: u32 = 7u;

struct Deformed {
    pos: vec3<f32>,
    normal: vec3<f32>,
};

fn deform_vertex(pos: vec3<f32>, normal: vec3<f32>, uv: vec2<f32>) -> Deformed {
    var p = pos;
    var n = normal;
    for (var i = 0u; i < 3u; i++) {
        let k = stage.deform_kinds[i];
        let d = stage.deform[i];
        if (k == DEFORM_WAVE || k == DEFORM_FLAP_S || k == DEFORM_FLAP_T) {
            // wave (0x513570): travelling along +x+y+z, along the normal;
            // flap (0x5136e0) scales it by the base s or t
            var scale = d.x + d.y * sin(6.2831853 * (d.z + (p.x + p.y + p.z) * d.w));
            if (k == DEFORM_FLAP_S) {
                scale *= uv.x;
            } else if (k == DEFORM_FLAP_T) {
                scale *= uv.y;
            }
            p = p + n * scale;
        } else if (k == DEFORM_NORMAL) {
            // normal (0x513990): each axis takes amp times the noise at
            // 0.98 * the vertex (x offset 0, 100, 200), then renormalizes
            let q = p * 0.98;
            let m = n + d.x * vec3<f32>(
                noise_get4f(vec4<f32>(q, d.y)),
                noise_get4f(vec4<f32>(q.x + 100.0, q.y, q.z, d.y)),
                noise_get4f(vec4<f32>(q.x + 200.0, q.y, q.z, d.y)),
            );
            n = select(m, normalize(m), dot(m, m) > 0.0);
        } else if (k == DEFORM_BULGE) {
            // bulge (0x513ad0): sin(s * width + t * speed) * height along
            // the normal
            p = p + n * (sin(uv.x * d.x + d.z) * d.y);
        } else if (k == DEFORM_MOVE) {
            // move (0x513b70): one offset for every vertex
            p = p + d.xyz;
        }
    }
    return Deformed(p, n);
}
