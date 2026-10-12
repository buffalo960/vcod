//! `vc_shadows`: real-time sun shadows of dynamic casters, a vcod-only
//! option (retail draws none; docs/research/cod11-light-grid-and-leaf-lights.md
//! section 12).
//!
//! The world's own sun shadows are baked into its lightmaps, so only what
//! moves casts here: snapshot entities (players, corpses, entity models)
//! and brush models off their spawn pose. Casters whose texture cuts
//! (foliage, fences, grilles) are alpha-tested as their main pass is, in
//! both layers. One 2048-texel orthographic map
//! from the sun, fitted around the view, holds two layers: the static
//! world (soups and props at spawn pose) and the dynamic casters. A
//! full-screen pass after the opaque draws reads the scene depth and
//! darkens each pixel by the sun's share of its light where the dynamic
//! layer hides the sun and the static layer does not, so indoor and
//! already-shadowed points keep their baked light.

use glam::{Mat4, Vec3, Vec4};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use vcod_common::static_light::Sun;

/// Shadow map side in texels.
const MAP_SIZE: u32 = 2048;
/// Half the map's side in world units: 1.5 units a texel.
const HALF_EXTENT: f32 = 1536.0;
const TEXEL: f32 = 2.0 * HALF_EXTENT / MAP_SIZE as f32;
/// How far ahead of the eye the map is centred, along the horizontal view.
const AHEAD: f32 = 768.0;
/// The centre snaps to this grid in light space: a whole number of texels,
/// so the texel grid holds still, and coarse, so the static layer is
/// redrawn only when the view has moved this far.
const SNAP: f32 = 192.0;
/// Shadows fade out over the outer part of the map, in light NDC.
const FADE_START: f32 = 0.8;
/// Receiver offsets against acne: along its normal, and towards the sun.
const NORMAL_OFFSET: f32 = 3.0;
const DEPTH_BIAS: f32 = 1.0;
/// Uniform slots: the plain light matrix, then one per moved brush model,
/// the last [`STATIC_SLOTS`] for the static redraw's pieces.
const SLOT_STRIDE: u64 = 256;
const MAX_SLOTS: usize = 128;
/// A static redraw is at most two exposed strips, each cut in four by the
/// map's wrap.
const STATIC_SLOTS: usize = 8;
const MASK_FLOATS: usize = 4 * 16 + 10 * 4;
const MAP_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// One static caster: a soup or prop's `(first, count)` into the
/// world's indices and its bounds.
pub(crate) struct CasterRange {
    pub(crate) first: u32,
    pub(crate) count: u32,
    pub(crate) bounds: (Vec3, Vec3),
    /// The batch whose texture alpha-tests it (resolved through a
    /// [`CutBinding`] at draw time); `None` draws it solid.
    pub(crate) cut: Option<u32>,
}

/// One group of casters in [`Casters`]' index buffer: the solid ones, then
/// per alpha-tested batch its run.
#[derive(Default)]
struct Group {
    solid: std::ops::Range<u32>,
    cuts: Vec<(std::ops::Range<u32>, u32)>,
}

impl Group {
    fn indices(&self) -> u32 {
        self.solid.len() as u32 + self.cuts.iter().map(|(r, _)| r.len() as u32).sum::<u32>()
    }
}

/// How an alpha-tested batch samples its texture: its group 1 (the world's
/// material layout) and, for a scripted stage, its group 2 and params
/// offset. `stage: None` tests at half like the implicit path.
pub(crate) struct CutBinding<'a> {
    pub(crate) mat: &'a wgpu::BindGroup,
    pub(crate) stage: Option<(&'a wgpu::BindGroup, u32)>,
}

/// The layouts the alpha-tested world casters share with the world pass.
pub(crate) struct WorldLayouts<'a> {
    pub(crate) material: &'a wgpu::BindGroupLayout,
    pub(crate) stage: &'a wgpu::BindGroupLayout,
}

/// Side of the cells the static casters are grouped in, so a static
/// redraw skips the cells outside the light's box.
const CELL: f32 = 1024.0;

/// The static casters of one loaded world: an index buffer over the
/// world's vertices, the world's soups and the props grouped by cell, then
/// each submodel's soups.
pub(crate) struct Casters {
    id: u64,
    index_buf: wgpu::Buffer,
    /// Per cell, its casters in `index_buf` and its bounds.
    cells: Vec<(Group, Vec3, Vec3)>,
    /// Per lump-27 model, its casters in `index_buf` and their bounds;
    /// model 0's is empty.
    model_ranges: Vec<(Group, Vec3, Vec3)>,
    /// The world's bounds, which fix the map's depth range.
    bounds: (Vec3, Vec3),
    tris: u32,
    /// Of those, alpha-tested.
    cut_tris: u32,
}

impl Casters {
    /// `world` holds model 0's soups and the props, `models[m]` submodel
    /// `m`'s soups.
    pub(crate) fn build(
        device: &wgpu::Device,
        cpu_indices: &[u32],
        world: &[CasterRange],
        models: &[Vec<CasterRange>],
    ) -> Casters {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let mut by_cell: std::collections::BTreeMap<(i32, i32), Vec<&CasterRange>> =
            Default::default();
        for r in world {
            let c = (r.bounds.0 + r.bounds.1) * 0.5;
            let key = ((c.x / CELL).floor() as i32, (c.y / CELL).floor() as i32);
            by_cell.entry(key).or_default().push(r);
        }
        let mut indices: Vec<u32> = Vec::new();
        let cells: Vec<_> = by_cell
            .into_values()
            .map(|rs| push_group(&mut indices, cpu_indices, rs))
            .collect();
        let model_ranges: Vec<_> = models
            .iter()
            .map(|rs| push_group(&mut indices, cpu_indices, rs.iter().collect()))
            .collect();
        let cut_indices: u32 = cells
            .iter()
            .chain(&model_ranges)
            .map(|c| &c.0)
            .map(|g| g.indices() - g.solid.len() as u32)
            .sum();
        let bounds = cells.iter().fold(
            (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
            |(a, b), c| (a.min(c.1), b.max(c.2)),
        );
        let tris = indices.len() as u32 / 3;
        log::info!(
            "shadow casters: {tris} tris, {} alpha-tested",
            cut_indices / 3
        );
        // a buffer can't be empty
        indices.resize(indices.len().max(3), 0);
        use wgpu::util::DeviceExt;
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shadow casters"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        Casters {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            index_buf,
            cells,
            model_ranges,
            bounds,
            tris,
            cut_tris: cut_indices / 3,
        }
    }
}

/// Appends `rs`' indices as one [`Group`]: the solid casters first, then
/// one run per cutting batch. Returns it with the group's bounds.
fn push_group(
    indices: &mut Vec<u32>,
    cpu: &[u32],
    mut rs: Vec<&CasterRange>,
) -> (Group, Vec3, Vec3) {
    rs.sort_by_key(|r| r.cut);
    let start = indices.len() as u32;
    let mut g = Group {
        solid: start..start,
        cuts: Vec::new(),
    };
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for r in rs {
        let at = indices.len() as u32;
        indices.extend_from_slice(&cpu[r.first as usize..(r.first + r.count) as usize]);
        let end = indices.len() as u32;
        match (r.cut, g.cuts.last_mut()) {
            (None, _) => g.solid.end = end,
            (Some(k), Some((run, last))) if *last == k => run.end = end,
            (Some(k), _) => g.cuts.push((at..end, k)),
        }
        lo = lo.min(r.bounds.0);
        hi = hi.max(r.bounds.1);
    }
    (g, lo, hi)
}

/// Whether a box reaches the light's box in x and y (rows 0 and 1 of the
/// orthographic `light_vp`).
fn in_light_box(light_vp: &Mat4, lo: Vec3, hi: Vec3) -> bool {
    let (c, e) = ((lo + hi) * 0.5, (hi - lo) * 0.5);
    [light_vp.row(0), light_vp.row(1)].iter().all(|r| {
        let at = r.truncate().dot(c) + r.w;
        at.abs() <= 1.0 + r.truncate().abs().dot(e)
    })
}

/// A rectangle of shadow-map texels on the light-space grid: texel `(x, y)`
/// spans `x..x + 1` texels along the light's right and `y..y + 1` along
/// its down, from the light-space origin. The grid holds still in the
/// world, so the static layer's texels stay valid across recentres.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TexRect {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

/// The map's side in texels, as a grid coordinate.
const SIDE: i32 = MAP_SIZE as i32;

impl TexRect {
    /// The map's window with its corner at `o`.
    fn window(o: (i32, i32)) -> TexRect {
        TexRect {
            x0: o.0,
            y0: o.1,
            x1: o.0 + SIDE,
            y1: o.1 + SIDE,
        }
    }
}

/// The part of the window at `new` the static layer lacks when it holds
/// the window at `old`: all of it without overlap, else a column strip and
/// a row strip.
fn exposed(old: Option<(i32, i32)>, new: (i32, i32)) -> Vec<TexRect> {
    let n = TexRect::window(new);
    let Some(old) = old.filter(|o| (o.0 - new.0).abs() < SIDE && (o.1 - new.1).abs() < SIDE) else {
        return vec![n];
    };
    if old == new {
        return Vec::new();
    }
    let o = TexRect::window(old);
    let mut out = Vec::new();
    let (cx0, cx1) = (n.x0.max(o.x0), n.x1.min(o.x1));
    if cx0 > n.x0 {
        out.push(TexRect { x1: cx0, ..n });
    }
    if cx1 < n.x1 {
        out.push(TexRect { x0: cx1, ..n });
    }
    let (ry0, ry1) = (n.y0.max(o.y0), n.y1.min(o.y1));
    let mid = TexRect {
        x0: cx0,
        x1: cx1,
        ..n
    };
    if ry0 > n.y0 {
        out.push(TexRect { y1: ry0, ..mid });
    }
    if ry1 < n.y1 {
        out.push(TexRect { y0: ry1, ..mid });
    }
    out
}

/// `r` cut where the map wraps: each piece with its corner in the map.
/// The static layer stores texel `(x, y)` at `(x mod SIDE, y mod SIDE)`.
fn on_torus(r: TexRect) -> Vec<(TexRect, (u32, u32))> {
    let cuts = |a: i32, b: i32| {
        (a.div_euclid(SIDE)..=(b - 1).div_euclid(SIDE))
            .map(move |k| (a.max(k * SIDE), b.min((k + 1) * SIDE)))
    };
    let mut out = Vec::new();
    for (y0, y1) in cuts(r.y0, r.y1) {
        for (x0, x1) in cuts(r.x0, r.x1) {
            let at = (x0.rem_euclid(SIDE) as u32, y0.rem_euclid(SIDE) as u32);
            out.push((TexRect { x0, y0, x1, y1 }, at));
        }
    }
    out
}

/// The light's axes and depth range for one sun and world: everything in
/// its matrix but the window.
struct LightBasis {
    right: Vec3,
    up: Vec3,
    dir: Vec3,
    zmax: f32,
    span: f32,
}

/// An orthographic box along `-sun` whose depth spans `bounds`, depth 0
/// the side nearest the sun.
fn light_basis(sun: Vec3, bounds: (Vec3, Vec3)) -> LightBasis {
    let dir = sun.normalize();
    let f = -dir;
    let hint = if f.z.abs() > 0.99 { Vec3::X } else { Vec3::Z };
    let right = f.cross(hint).normalize();
    let up = right.cross(f);
    let (lo, hi) = bounds;
    let (mut zmin, mut zmax) = (f32::MAX, f32::MIN);
    for i in 0..8 {
        let c = Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        let z = c.dot(dir);
        zmin = zmin.min(z);
        zmax = zmax.max(z);
    }
    zmin -= 16.0;
    zmax += 16.0;
    LightBasis {
        right,
        up,
        dir,
        zmax,
        span: zmax - zmin,
    }
}

impl LightBasis {
    /// The window's corner for a view at `eye` looking along `fwd`: centred
    /// [`AHEAD`] in front of the eye, snapped to [`SNAP`] in light space.
    fn window(&self, eye: Vec3, fwd: Vec3) -> (i32, i32) {
        let flat = Vec3::new(fwd.x, fwd.y, 0.0).normalize_or_zero();
        let focus = eye + flat * AHEAD;
        let step = (SNAP / TEXEL) as i32;
        let kx = (focus.dot(self.right) / SNAP).round() as i32;
        let ky = (focus.dot(self.up) / SNAP).round() as i32;
        (kx * step - SIDE / 2, -ky * step - SIDE / 2)
    }

    /// The view-projection that puts `r` on the whole viewport.
    fn matrix(&self, r: TexRect) -> Mat4 {
        let (hx, hy) = (
            (r.x1 - r.x0) as f32 * 0.5 * TEXEL,
            (r.y1 - r.y0) as f32 * 0.5 * TEXEL,
        );
        let (cx, cy) = (
            (r.x0 + r.x1) as f32 * 0.5 * TEXEL,
            -(r.y0 + r.y1) as f32 * 0.5 * TEXEL,
        );
        Mat4::from_cols(
            (self.right / hx).extend(-cx / hx),
            (self.up / hy).extend(-cy / hy),
            (-self.dir / self.span).extend(self.zmax / self.span),
            Vec4::W,
        )
        .transpose()
    }
}

/// The light's view-projection for a view at `eye` looking along `fwd`
/// (see [`light_basis`], [`LightBasis::window`]).
#[cfg(test)]
fn light_matrix(sun: Vec3, eye: Vec3, fwd: Vec3, bounds: (Vec3, Vec3)) -> Mat4 {
    let b = light_basis(sun, bounds);
    b.matrix(TexRect::window(b.window(eye, fwd)))
}

/// One piece of a static redraw: its matrix and where it sits in the map.
struct Piece {
    vp: Mat4,
    at: (u32, u32),
    size: (u32, u32),
}

/// A snapshot entity surface that casts: its buffers, instance slot,
/// index range, and the image and params of its first depth-writing stage.
pub(crate) struct EntCaster<'a> {
    pub(crate) vertex_buf: &'a wgpu::Buffer,
    pub(crate) index_buf: &'a wgpu::Buffer,
    pub(crate) instance: u32,
    pub(crate) first_index: u32,
    pub(crate) index_count: u32,
    pub(crate) base_vertex: i32,
    pub(crate) image: &'a wgpu::BindGroup,
    pub(crate) params_offset: u32,
}

/// The entity draws' shared bindings.
pub(crate) struct EntBindings<'a> {
    pub(crate) instance_buf: &'a wgpu::Buffer,
    pub(crate) instance_stride: u64,
    pub(crate) bones: &'a wgpu::BindGroup,
    pub(crate) params: &'a wgpu::BindGroup,
}

/// The layouts the entity caster pipeline shares with the entity pass.
pub(crate) struct EntLayouts<'a> {
    pub(crate) skin: &'a wgpu::BindGroupLayout,
    pub(crate) bones: &'a wgpu::BindGroupLayout,
    pub(crate) params: &'a wgpu::BindGroupLayout,
    pub(crate) vertex: wgpu::VertexBufferLayout<'a>,
    pub(crate) instance: wgpu::VertexBufferLayout<'a>,
}

/// GPU timestamps around the caster passes and the mask: ms each.
struct Timer {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    /// A readback is in flight; set again by its callback when mapped.
    pending: bool,
    mapped: Arc<AtomicBool>,
    period_ns: f32,
    /// The in-flight frame's static redraw: its pieces and triangles.
    static_ran: Option<(usize, u32)>,
}

const QUERIES: u32 = 6;

impl Timer {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Timer> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let size = u64::from(QUERIES) * 8;
        Some(Timer {
            queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("shadow timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: QUERIES,
            }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("shadow timestamp resolve"),
                size,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("shadow timestamp readback"),
                size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            pending: false,
            mapped: Arc::new(AtomicBool::new(false)),
            period_ns: queue.get_timestamp_period(),
            static_ran: None,
        })
    }

    fn writes(&self, pair: u32) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        (!self.pending).then_some(wgpu::RenderPassTimestampWrites {
            query_set: &self.queries,
            beginning_of_pass_write_index: Some(pair * 2),
            end_of_pass_write_index: Some(pair * 2 + 1),
        })
    }
}

/// F3 readout: GPU ms of the last measured frame, smoothed.
#[derive(Default, Clone, Copy)]
pub(crate) struct ShadowStats {
    pub(crate) static_ms: f32,
    pub(crate) dynamic_ms: f32,
    pub(crate) mask_ms: f32,
    pub(crate) static_draws: u64,
    pub(crate) casters: usize,
    pub(crate) static_tris: u32,
    /// Of those, alpha-tested.
    pub(crate) static_cut: u32,
    /// Of those, drawn at the last static redraw, and of those alpha-tested.
    pub(crate) static_drawn: u32,
    pub(crate) static_cut_drawn: u32,
    /// CPU ms of `prepare` and `encode_maps`, smoothed.
    pub(crate) cpu_ms: f32,
}

pub(crate) struct ShadowPass {
    _map: wgpu::Texture,
    layer_views: [wgpu::TextureView; 2],
    array_view: wgpu::TextureView,
    compare_sampler: wgpu::Sampler,
    /// The static layer's: it wraps (see [`on_torus`]).
    static_sampler: wgpu::Sampler,
    slots_buf: wgpu::Buffer,
    slots_bg: wgpu::BindGroup,
    world_pipeline: wgpu::RenderPipeline,
    /// Fills a static piece's viewport with the far depth.
    clear_pipeline: wgpu::RenderPipeline,
    /// Alpha-tested world casters: `[implicit, scripted stage]`.
    cut_pipelines: [wgpu::RenderPipeline; 2],
    entity_pipeline: wgpu::RenderPipeline,
    mask_layout: wgpu::BindGroupLayout,
    mask_buf: wgpu::Buffer,
    mask_pipeline: wgpu::RenderPipeline,
    /// The light basis (as its matrix at the origin) and casters the
    /// static layer was drawn for, and the window it holds.
    static_held: Option<(([f32; 16], u64), (i32, i32))>,
    /// This frame's static redraw, set by [`ShadowPass::prepare`]: whether
    /// it starts from a cleared map, and its pieces.
    static_full: bool,
    pieces: Vec<Piece>,
    moved: usize,
    timer: Option<Timer>,
    cpu_start: std::time::Instant,
    pub(crate) stats: ShadowStats,
}

impl ShadowPass {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene_format: wgpu::TextureFormat,
        samples: u32,
        ent: EntLayouts<'_>,
        world: WorldLayouts<'_>,
    ) -> ShadowPass {
        let map = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow map"),
            size: wgpu::Extent3d {
                width: MAP_SIZE,
                height: MAP_SIZE,
                depth_or_array_layers: 2,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: MAP_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let layer = |i: u32| {
            map.create_view(&wgpu::TextureViewDescriptor {
                label: Some("shadow map layer"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: i,
                array_layer_count: Some(1),
                ..Default::default()
            })
        };
        let layer_views = [layer(0), layer(1)];
        let array_view = map.create_view(&wgpu::TextureViewDescriptor {
            label: Some("shadow map array"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let compare_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow compare sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let static_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow static compare sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        let slots_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow light slots layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(64),
                },
                count: None,
            }],
        });
        let slots_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow light slots"),
            size: MAX_SLOTS as u64 * SLOT_STRIDE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let slots_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow light slots"),
            layout: &slots_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &slots_buf,
                    offset: 0,
                    size: wgpu::BufferSize::new(64),
                }),
            }],
        });

        let cast = device.create_shader_module(wgpu::include_wgsl!("shadow_cast.wgsl"));
        // Casters draw two-sided; the receiver's offsets handle acne.
        let depth = Some(wgpu::DepthStencilState {
            format: MAP_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: wgpu::DepthBiasState {
                constant: 0,
                slope_scale: 1.0,
                clamp: 0.0,
            },
        });
        let world_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow world layout"),
            bind_group_layouts: &[Some(&slots_layout)],
            immediate_size: 0,
        });
        let world_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow world casters"),
            layout: Some(&world_layout),
            vertex: wgpu::VertexState {
                module: &cast,
                entry_point: Some("vs_world"),
                compilation_options: Default::default(),
                // the map's 44-byte vertex; only the position is read
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 44,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                })],
            },
            fragment: None,
            primitive: Default::default(),
            depth_stencil: depth.clone(),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let clear_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow clear layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let clear_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow static clear"),
            layout: Some(&clear_layout),
            vertex: wgpu::VertexState {
                module: &cast,
                entry_point: Some("vs_clear"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: None,
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: MAP_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let cut = device.create_shader_module(wgpu::include_wgsl!("shadow_cut.wgsl"));
        let cut_pipeline = |label, groups: &[Option<&wgpu::BindGroupLayout>], vs, fs| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: groups,
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &cut,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    // the map's 44-byte vertex: position, uv, colour
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: 44,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x3,
                                offset: 0,
                                shader_location: 0,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 12,
                                shader_location: 1,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Unorm8x4,
                                offset: 40,
                                shader_location: 4,
                            },
                        ],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &cut,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[],
                }),
                primitive: Default::default(),
                depth_stencil: depth.clone(),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let cut_pipelines = [
            cut_pipeline(
                "shadow cutout casters",
                &[Some(&slots_layout), Some(world.material)],
                "vs_cut",
                "fs_cut",
            ),
            cut_pipeline(
                "shadow cutout stage casters",
                &[Some(&slots_layout), Some(world.material), Some(world.stage)],
                "vs_cut_stage",
                "fs_cut_stage",
            ),
        ];
        let entity_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow entity layout"),
            bind_group_layouts: &[
                Some(&slots_layout),
                Some(ent.skin),
                Some(ent.bones),
                Some(ent.params),
            ],
            immediate_size: 0,
        });
        let entity_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow entity casters"),
            layout: Some(&entity_layout),
            vertex: wgpu::VertexState {
                module: &cast,
                entry_point: Some("vs_entity"),
                compilation_options: Default::default(),
                buffers: &[Some(ent.vertex), Some(ent.instance)],
            },
            fragment: Some(wgpu::FragmentState {
                module: &cast,
                entry_point: Some("fs_entity"),
                compilation_options: Default::default(),
                targets: &[],
            }),
            primitive: Default::default(),
            depth_stencil: depth,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        let mask_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow mask layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(MASK_FLOATS as u64 * 4),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: samples > 1,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let mask_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow mask uniform"),
            size: MASK_FLOATS as u64 * 4,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mask = device.create_shader_module(wgpu::include_wgsl!("shadow_mask.wgsl"));
        let mask_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow mask pipeline layout"),
            bind_group_layouts: &[Some(&mask_layout)],
            immediate_size: 0,
        });
        // dst * src on rgb, the frame's alpha kept
        let multiply = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::Src,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let mask_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow mask"),
            layout: Some(&mask_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &mask,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &mask,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: scene_format,
                    blend: Some(multiply),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: samples,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        });

        ShadowPass {
            _map: map,
            layer_views,
            array_view,
            compare_sampler,
            static_sampler,
            slots_buf,
            slots_bg,
            world_pipeline,
            clear_pipeline,
            cut_pipelines,
            entity_pipeline,
            mask_layout,
            mask_buf,
            mask_pipeline,
            static_held: None,
            static_full: false,
            pieces: Vec::new(),
            moved: 0,
            timer: Timer::new(device, queue),
            cpu_start: std::time::Instant::now(),
            stats: ShadowStats::default(),
        }
    }

    /// This frame's uniforms. `camera` is the frame's Camera block
    /// (`renderer::CAMERA_FLOATS`), whose fog tail the mask copies; `moved`
    /// the poses of the brush models off their spawn pose.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        sun: &Sun,
        casters: &Casters,
        view_proj: Mat4,
        eye: Vec3,
        fwd: Vec3,
        camera: &[f32],
        viewport: (f32, f32),
        moved: &[Mat4],
    ) {
        self.read_timer();
        self.cpu_start = std::time::Instant::now();
        self.stats.static_tris = casters.tris;
        self.stats.static_cut = casters.cut_tris;
        let basis = light_basis(sun.dir, casters.bounds);
        let window = basis.window(eye, fwd);
        let light_vp = basis.matrix(TexRect::window(window));
        // The static layer keeps what it holds of the new window and draws
        // only the strips it lacks.
        let key = (
            basis.matrix(TexRect::window((0, 0))).to_cols_array(),
            casters.id,
        );
        let held = self.static_held.filter(|h| h.0 == key).map(|h| h.1);
        self.static_full = held.is_none();
        self.pieces = exposed(held, window)
            .into_iter()
            .flat_map(on_torus)
            .map(|(r, at)| Piece {
                vp: basis.matrix(r),
                at,
                size: ((r.x1 - r.x0) as u32, (r.y1 - r.y0) as u32),
            })
            .collect();
        self.static_held = Some((key, window));
        self.moved = moved.len().min(MAX_SLOTS - 1 - STATIC_SLOTS);
        let slots = std::iter::once(light_vp).chain(moved.iter().map(|&pose| light_vp * pose));
        let write = |i: usize, m: Mat4| {
            queue.write_buffer(
                &self.slots_buf,
                i as u64 * SLOT_STRIDE,
                bytemuck::cast_slice(&m.to_cols_array()),
            )
        };
        for (i, m) in slots.take(self.moved + 1).enumerate() {
            write(i, m);
        }
        for (i, p) in self.pieces.iter().enumerate() {
            write(MAX_SLOTS - STATIC_SLOTS + i, p.vp);
        }
        let (lo, hi) = casters.bounds;
        let span = {
            let d = sun.dir.normalize();
            let ext = (hi - lo).abs();
            ext.x * d.x.abs() + ext.y * d.y.abs() + ext.z * d.z.abs() + 32.0
        };
        let mut u = [0.0f32; MASK_FLOATS];
        u[..16].copy_from_slice(&view_proj.inverse().to_cols_array());
        u[16..32].copy_from_slice(&light_vp.to_cols_array());
        u[32..36].copy_from_slice(&sun.dir.normalize().extend(0.0).to_array());
        u[36..40].copy_from_slice(&[
            sun.sun,
            sun.sky + sun.ambient,
            DEPTH_BIAS / span,
            NORMAL_OFFSET,
        ]);
        u[40..44].copy_from_slice(&[1.0 / MAP_SIZE as f32, FADE_START, viewport.0, viewport.1]);
        // eye_fog_mode, fog_color_density, fog_range, view_fwd
        u[44..60].copy_from_slice(&camera[20..36]);
        // the window's corner in the wrapping static layer, in uv
        let wrap = |v: i32| v.rem_euclid(SIDE) as f32 / MAP_SIZE as f32;
        u[60..64].copy_from_slice(&[wrap(window.0), wrap(window.1), 0.0, 0.0]);
        queue.write_buffer(&self.mask_buf, 0, bytemuck::cast_slice(&u));
    }

    /// Draws `g`'s alpha-tested runs under the light slot at `slot`;
    /// `bound` tracks which cut pipeline is set. Returns the indices drawn.
    fn draw_cuts<'a>(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        g: &Group,
        slot: u32,
        cut: &dyn Fn(u32) -> Option<CutBinding<'a>>,
        bound: &mut Option<bool>,
    ) -> u32 {
        let mut drawn = 0;
        for (r, k) in &g.cuts {
            let Some(b) = cut(*k) else {
                continue;
            };
            let staged = b.stage.is_some();
            if *bound != Some(staged) {
                pass.set_pipeline(&self.cut_pipelines[usize::from(staged)]);
                *bound = Some(staged);
            }
            pass.set_bind_group(0, &self.slots_bg, &[slot]);
            pass.set_bind_group(1, b.mat, &[]);
            if let Some((stage, offset)) = b.stage {
                pass.set_bind_group(2, stage, &[offset]);
            }
            pass.draw_indexed(r.clone(), 0, 0..1);
            drawn += r.len() as u32;
        }
        drawn
    }

    /// The caster passes, before the frame's main pass: the static layer's
    /// pieces when the window or the world changed, the dynamic layer every
    /// frame.
    /// `cut` resolves an alpha-tested caster's batch to its bindings.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn encode_maps<'a>(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        world_vertices: &wgpu::Buffer,
        casters: &Casters,
        moved_models: &[usize],
        ents: &[EntCaster<'_>],
        eb: &EntBindings<'_>,
        cut: &dyn Fn(u32) -> Option<CutBinding<'a>>,
    ) {
        let pass_desc = |view, load, tw| wgpu::RenderPassDescriptor {
            label: Some("shadow casters"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view,
                depth_ops: Some(wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: tw,
            occlusion_query_set: None,
            multiview_mask: None,
        };
        let timer = self.timer.as_ref();
        if !self.pieces.is_empty() {
            self.stats.static_draws += 1;
            let load = if self.static_full {
                wgpu::LoadOp::Clear(1.0)
            } else {
                wgpu::LoadOp::Load
            };
            let mut pass = encoder.begin_render_pass(&pass_desc(
                &self.layer_views[0],
                load,
                timer.and_then(|t| t.writes(0)),
            ));
            pass.set_vertex_buffer(0, world_vertices.slice(..));
            pass.set_index_buffer(casters.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            let (mut drawn, mut cut_drawn) = (0, 0);
            for (i, p) in self.pieces.iter().enumerate() {
                let slot = ((MAX_SLOTS - STATIC_SLOTS + i) as u64 * SLOT_STRIDE) as u32;
                let (x, y) = (p.at.0 as f32, p.at.1 as f32);
                pass.set_viewport(x, y, p.size.0 as f32, p.size.1 as f32, 0.0, 1.0);
                if !self.static_full {
                    pass.set_pipeline(&self.clear_pipeline);
                    pass.draw(0..3, 0..1);
                }
                pass.set_pipeline(&self.world_pipeline);
                pass.set_bind_group(0, &self.slots_bg, &[slot]);
                // submodels at their spawn pose, where the bake saw them
                let groups: Vec<&Group> = casters
                    .cells
                    .iter()
                    .chain(&casters.model_ranges)
                    .filter(|(g, lo, hi)| g.indices() > 0 && in_light_box(&p.vp, *lo, *hi))
                    .map(|(g, _, _)| g)
                    .collect();
                for g in &groups {
                    if !g.solid.is_empty() {
                        drawn += g.solid.len() as u32;
                        pass.draw_indexed(g.solid.clone(), 0, 0..1);
                    }
                }
                let mut bound = None;
                for g in &groups {
                    cut_drawn += self.draw_cuts(&mut pass, g, slot, cut, &mut bound);
                }
            }
            self.stats.static_drawn = (drawn + cut_drawn) / 3;
            self.stats.static_cut_drawn = cut_drawn / 3;
        }
        let mut pass = encoder.begin_render_pass(&pass_desc(
            &self.layer_views[1],
            wgpu::LoadOp::Clear(1.0),
            timer.and_then(|t| t.writes(1)),
        ));
        pass.set_pipeline(&self.world_pipeline);
        pass.set_vertex_buffer(0, world_vertices.slice(..));
        pass.set_index_buffer(casters.index_buf.slice(..), wgpu::IndexFormat::Uint32);
        for (i, &m) in moved_models.iter().take(self.moved).enumerate() {
            let Some((g, _, _)) = casters.model_ranges.get(m) else {
                continue;
            };
            if g.indices() == 0 {
                continue;
            }
            let slot = ((i + 1) as u64 * SLOT_STRIDE) as u32;
            pass.set_pipeline(&self.world_pipeline);
            pass.set_bind_group(0, &self.slots_bg, &[slot]);
            if !g.solid.is_empty() {
                pass.draw_indexed(g.solid.clone(), 0, 0..1);
            }
            self.draw_cuts(&mut pass, g, slot, cut, &mut None);
        }
        if !ents.is_empty() {
            pass.set_pipeline(&self.entity_pipeline);
            pass.set_bind_group(0, &self.slots_bg, &[0]);
            pass.set_bind_group(2, eb.bones, &[]);
            for c in ents {
                let off = u64::from(c.instance) * eb.instance_stride;
                pass.set_vertex_buffer(0, c.vertex_buf.slice(..));
                pass.set_vertex_buffer(1, eb.instance_buf.slice(off..off + eb.instance_stride));
                pass.set_index_buffer(c.index_buf.slice(..), wgpu::IndexFormat::Uint16);
                pass.set_bind_group(1, c.image, &[]);
                pass.set_bind_group(3, eb.params, &[c.params_offset]);
                pass.draw_indexed(
                    c.first_index..c.first_index + c.index_count,
                    c.base_vertex,
                    0..1,
                );
            }
        }
        self.stats.casters = ents.len();
        let ms = self.cpu_start.elapsed().as_secs_f32() * 1000.0;
        self.stats.cpu_ms = self.stats.cpu_ms * 0.9 + ms * 0.1;
    }

    /// The mask over the multisampled scene; its own pass between the
    /// frame's opaque draws and the rest.
    pub(crate) fn encode_mask(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) {
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow mask"),
            layout: &self.mask_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.mask_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.compare_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.static_sampler),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shadow mask"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: self.timer.as_ref().and_then(|t| t.writes(2)),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.mask_pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Resolves this frame's timestamps; call after the mask, before submit.
    pub(crate) fn resolve_timer(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let static_due =
            (!self.pieces.is_empty()).then_some((self.pieces.len(), self.stats.static_drawn));
        let Some(t) = self.timer.as_mut().filter(|t| !t.pending) else {
            return;
        };
        encoder.resolve_query_set(&t.queries, 0..QUERIES, &t.resolve, 0);
        encoder.copy_buffer_to_buffer(&t.resolve, 0, &t.readback, 0, None);
        t.static_ran = static_due;
        t.pending = true;
        t.mapped.store(false, Ordering::Release);
    }

    /// Starts the readback; call after the frame's submit.
    pub(crate) fn after_submit(&mut self) {
        let Some(t) = self.timer.as_mut().filter(|t| t.pending) else {
            return;
        };
        if t.mapped.load(Ordering::Acquire) {
            return;
        }
        let flag = t.mapped.clone();
        t.readback.map_async(wgpu::MapMode::Read, .., move |r| {
            if r.is_ok() {
                flag.store(true, Ordering::Release);
            }
        });
    }

    fn read_timer(&mut self) {
        let Some(t) = self.timer.as_mut().filter(|t| t.pending) else {
            return;
        };
        if !t.mapped.load(Ordering::Acquire) {
            return;
        }
        let ticks: Vec<u64> = match t.readback.get_mapped_range(..) {
            Ok(view) => bytemuck::cast_slice(&view).to_vec(),
            Err(_) => return,
        };
        t.readback.unmap();
        t.mapped.store(false, Ordering::Release);
        t.pending = false;
        let ms = |i: usize| ticks[i + 1].wrapping_sub(ticks[i]) as f32 * t.period_ns / 1e6;
        let smooth = |old: &mut f32, new: f32| {
            *old = if *old == 0.0 {
                new
            } else {
                *old * 0.9 + new * 0.1
            }
        };
        if let Some((pieces, tris)) = t.static_ran {
            log::debug!(
                "shadow static redraw: {:.2} ms gpu, {pieces} pieces, {tris} tris",
                ms(0)
            );
            smooth(&mut self.stats.static_ms, ms(0));
        }
        smooth(&mut self.stats.dynamic_ms, ms(2));
        smooth(&mut self.stats.mask_ms, ms(4));
    }

    /// The F3 line.
    pub(crate) fn debug_line(&self) -> String {
        let s = &self.stats;
        let head = format!(
            "shadow: ents {} static {}k/{}k tris cut {}k/{}k x{} cpu {:.2}ms",
            s.casters,
            s.static_drawn / 1000,
            s.static_tris / 1000,
            s.static_cut_drawn / 1000,
            s.static_cut / 1000,
            s.static_draws,
            s.cpu_ms
        );
        if self.timer.is_none() {
            return head;
        }
        format!(
            "{head} gpu static {:.2} dyn {:.2} mask {:.2}ms",
            s.static_ms, s.dynamic_ms, s.mask_ms
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ndc(m: Mat4, p: Vec3) -> Vec3 {
        let c = m * p.extend(1.0);
        c.truncate() / c.w
    }

    #[test]
    fn light_matrix_frames_the_view_and_the_world_depth() {
        let sun = Vec3::new(-0.6, -0.35, 0.7).normalize();
        let bounds = (
            Vec3::new(-3000.0, -2000.0, -500.0),
            Vec3::new(2500.0, 3000.0, 1200.0),
        );
        let eye = Vec3::new(100.0, 200.0, 50.0);
        let m = light_matrix(sun, eye, Vec3::X, bounds);
        // the eye sits inside the map, short of its centre by about AHEAD
        let e = ndc(m, eye);
        assert!(e.x.abs() < 1.0 && e.y.abs() < 1.0, "{e}");
        let ahead = ndc(m, eye + Vec3::X * AHEAD);
        let snap_ndc = SNAP / HALF_EXTENT;
        assert!(
            ahead.x.abs() <= snap_ndc && ahead.y.abs() <= snap_ndc,
            "{ahead}"
        );
        // depth: the world's corners land in 0..1, nearer the sun is smaller
        for i in 0..8 {
            let c = Vec3::new(
                if i & 1 == 0 { bounds.0.x } else { bounds.1.x },
                if i & 2 == 0 { bounds.0.y } else { bounds.1.y },
                if i & 4 == 0 { bounds.0.z } else { bounds.1.z },
            );
            let z = ndc(m, c).z;
            assert!((0.0..=1.0).contains(&z), "{c} -> {z}");
        }
        assert!(ndc(m, eye + sun * 100.0).z < e.z);
        // a point straight along the sun ray shares the eye's texel
        let up = ndc(m, eye + sun * 500.0);
        assert!((up.truncate() - e.truncate()).length() < 1e-4);
    }

    #[test]
    fn a_group_puts_solid_casters_first_and_one_run_per_cutting_batch() {
        let cpu: Vec<u32> = (0..12).collect();
        let r = |first, cut| CasterRange {
            first,
            count: 3,
            bounds: (Vec3::splat(first as f32), Vec3::splat(first as f32 + 1.0)),
            cut,
        };
        let rs = [r(0, Some(7)), r(3, None), r(6, Some(7)), r(9, Some(2))];
        let mut out = vec![99];
        let (g, lo, hi) = push_group(&mut out, &cpu, rs.iter().collect());
        assert_eq!(g.solid, 1..4);
        assert_eq!(g.cuts, vec![(4..7, 2), (7..13, 7)]);
        assert_eq!(&out[1..4], &[3, 4, 5]);
        assert_eq!(&out[4..7], &[9, 10, 11]);
        assert_eq!(g.indices(), 12);
        assert_eq!((lo, hi), (Vec3::ZERO, Vec3::splat(10.0)));
    }

    #[test]
    fn light_box_test_keeps_what_it_reaches() {
        let sun = Vec3::new(0.0, 0.0, 1.0);
        let bounds = (Vec3::splat(-8000.0), Vec3::splat(8000.0));
        let m = light_matrix(sun, Vec3::ZERO, Vec3::X, bounds);
        let at = |x: f32| {
            in_light_box(
                &m,
                Vec3::new(x, 0.0, 0.0),
                Vec3::new(x + 100.0, 100.0, 50.0),
            )
        };
        assert!(at(0.0));
        // the box spans AHEAD +- HALF_EXTENT along x, give or take a snap
        assert!(at(AHEAD + HALF_EXTENT - SNAP - 50.0));
        assert!(!at(AHEAD + HALF_EXTENT + SNAP + 10.0));
        assert!(!at(AHEAD - HALF_EXTENT - SNAP - 200.0));
    }

    #[test]
    fn light_matrix_holds_still_within_a_snap_step() {
        let sun = Vec3::new(0.3, 0.4, 0.8).normalize();
        let bounds = (Vec3::splat(-4000.0), Vec3::splat(4000.0));
        let a = light_matrix(sun, Vec3::new(10.0, 10.0, 0.0), Vec3::Y, bounds);
        let b = light_matrix(sun, Vec3::new(14.0, 12.0, 0.0), Vec3::Y, bounds);
        assert_eq!(a, b);
    }

    /// Every texel of a window, by where the static layer stores it.
    fn stored(rects: &[TexRect]) -> std::collections::HashMap<(u32, u32), (i32, i32)> {
        let mut m = std::collections::HashMap::new();
        for &r in rects {
            for (p, at) in on_torus(r) {
                assert!(at.0 + (p.x1 - p.x0) as u32 <= MAP_SIZE);
                assert!(at.1 + (p.y1 - p.y0) as u32 <= MAP_SIZE);
                for y in p.y0..p.y1 {
                    for x in p.x0..p.x1 {
                        let k = (at.0 + (x - p.x0) as u32, at.1 + (y - p.y0) as u32);
                        assert!(m.insert(k, (x, y)).is_none(), "{k:?} drawn twice");
                    }
                }
            }
        }
        m
    }

    #[test]
    fn a_recentre_draws_only_the_exposed_texels_where_the_layer_wraps() {
        let step = (SNAP / TEXEL) as i32;
        let start = (5 * step - SIDE / 2, -3 * step - SIDE / 2);
        let mut layer = stored(&exposed(None, start));
        assert_eq!(layer.len(), (SIDE * SIDE) as usize);
        assert!(exposed(Some(start), start).is_empty());
        // straight, diagonal, back, and a jump past the window
        for d in [(1, 0), (1, 1), (-2, 0), (0, -3), (-1, 2), (20, 0)] {
            let old = layer.values().map(|&(x, y)| (x, y)).min().unwrap();
            let new = (old.0 + d.0 * step, old.1 + d.1 * step);
            let rects = exposed(Some(old), new);
            let drawn = stored(&rects);
            let expect = (d.0.abs() * step).min(SIDE) * SIDE
                + (SIDE - (d.0.abs() * step).min(SIDE)) * (d.1.abs() * step).min(SIDE);
            assert_eq!(drawn.len(), expect as usize, "{d:?}");
            layer.extend(drawn);
            // the layer now holds exactly the new window, each texel at its
            // wrapped spot
            let w = TexRect::window(new);
            for (&(u, v), &(x, y)) in &layer {
                assert!(
                    (w.x0..w.x1).contains(&x) && (w.y0..w.y1).contains(&y),
                    "{d:?}"
                );
                assert_eq!(
                    (x.rem_euclid(SIDE) as u32, y.rem_euclid(SIDE) as u32),
                    (u, v)
                );
            }
        }
    }

    #[test]
    fn a_piece_matrix_lands_its_texels_where_the_window_does() {
        let sun = Vec3::new(0.3, 0.4, 0.8).normalize();
        let b = light_basis(sun, (Vec3::splat(-4000.0), Vec3::splat(4000.0)));
        let w = TexRect::window(b.window(Vec3::new(300.0, -700.0, 40.0), Vec3::Y));
        let full = b.matrix(w);
        let piece = TexRect {
            x0: w.x0 + 1000,
            y0: w.y0 + 300,
            x1: w.x0 + 1128,
            y1: w.y1,
        };
        let pm = b.matrix(piece);
        // a world point's texel, through the window and through the piece
        let p = b.right * ((w.x0 + 1050) as f32 + 0.25) * TEXEL
            - b.up * ((w.y0 + 400) as f32 + 0.75) * TEXEL
            + b.dir * 37.0;
        let texel = |m: Mat4, r: TexRect| {
            let n = ndc(m, p);
            (
                r.x0 as f32 + (n.x * 0.5 + 0.5) * (r.x1 - r.x0) as f32,
                r.y0 as f32 + (0.5 - n.y * 0.5) * (r.y1 - r.y0) as f32,
                n.z,
            )
        };
        let (a, c) = (texel(full, w), texel(pm, piece));
        assert!(
            (a.0 - c.0).abs() < 1e-2 && (a.1 - c.1).abs() < 1e-2,
            "{a:?} {c:?}"
        );
        assert!((a.0 - (w.x0 + 1050) as f32 - 0.25).abs() < 1e-2, "{a:?}");
        assert_eq!(a.2, c.2);
    }

    #[test]
    fn mask_and_cast_shaders_validate() {
        use wgpu::naga;
        for src in [
            include_str!("shadow_cast.wgsl"),
            include_str!("shadow_cut.wgsl"),
            include_str!("shadow_mask.wgsl"),
        ] {
            let module = naga::front::wgsl::parse_str(src)
                .unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::default(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
        }
    }
}
