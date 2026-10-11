//! `vc_shadows`: real-time sun shadows of dynamic casters, a vcod-only
//! option (retail draws none; docs/research/cod11-light-grid-and-leaf-lights.md
//! section 12).
//!
//! The world's own sun shadows are baked into its lightmaps, so only what
//! moves casts here: snapshot entities (players, corpses, entity models)
//! and brush models off their spawn pose. One 2048-texel orthographic map
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
/// Uniform slots: the plain light matrix, then one per moved brush model.
const SLOT_STRIDE: u64 = 256;
const MAX_SLOTS: usize = 128;
const MASK_FLOATS: usize = 4 * 16 + 9 * 4;
const MAP_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The static casters of one loaded world: an index buffer over the
/// world's vertices, soups grouped by lump-27 model, props after.
pub(crate) struct Casters {
    id: u64,
    index_buf: wgpu::Buffer,
    count: u32,
    /// Per lump-27 model, its range in `index_buf`.
    model_ranges: Vec<std::ops::Range<u32>>,
    /// The world's bounds, which fix the map's depth range.
    bounds: (Vec3, Vec3),
}

impl Casters {
    /// `soups` is `(model, first, count)` into `cpu_indices` per caster
    /// soup; `props` the props' ranges.
    pub(crate) fn build(
        device: &wgpu::Device,
        cpu_indices: &[u32],
        soups: &[(u32, u32, u32)],
        props: &[(u32, u32)],
        model_count: usize,
        bounds: (Vec3, Vec3),
    ) -> Casters {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let mut by_model: Vec<Vec<(u32, u32)>> = vec![Vec::new(); model_count.max(1)];
        for &(m, first, count) in soups {
            if let Some(v) = by_model.get_mut(m as usize) {
                v.push((first, count));
            }
        }
        let mut indices = Vec::new();
        let mut model_ranges = Vec::with_capacity(by_model.len());
        let push = |ranges: &[(u32, u32)], indices: &mut Vec<u32>| {
            let start = indices.len() as u32;
            for &(first, count) in ranges {
                indices.extend_from_slice(&cpu_indices[first as usize..(first + count) as usize]);
            }
            start..indices.len() as u32
        };
        for ranges in &by_model {
            model_ranges.push(push(ranges, &mut indices));
        }
        push(props, &mut indices);
        let count = indices.len() as u32;
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
            count,
            model_ranges,
            bounds,
        }
    }
}

/// The light's view-projection for a view at `eye` looking along `fwd`:
/// an orthographic box along `-sun` whose depth spans `bounds`, centred
/// [`AHEAD`] in front of the eye and snapped to [`SNAP`] in light space.
/// Depth 0 is the side nearest the sun.
pub(crate) fn light_matrix(sun: Vec3, eye: Vec3, fwd: Vec3, bounds: (Vec3, Vec3)) -> Mat4 {
    let dir = sun.normalize();
    let f = -dir;
    let hint = if f.z.abs() > 0.99 { Vec3::X } else { Vec3::Z };
    let right = f.cross(hint).normalize();
    let up = right.cross(f);
    let flat = Vec3::new(fwd.x, fwd.y, 0.0).normalize_or_zero();
    let focus = eye + flat * AHEAD;
    let snap = |v: f32| (v / SNAP).round() * SNAP;
    let (cx, cy) = (snap(focus.dot(right)), snap(focus.dot(up)));
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
    let span = zmax - zmin;
    let h = HALF_EXTENT;
    Mat4::from_cols(
        (right / h).extend(-cx / h),
        (up / h).extend(-cy / h),
        (-dir / span).extend(zmax / span),
        Vec4::W,
    )
    .transpose()
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
    /// Whether the in-flight frame drew the static layer.
    static_ran: bool,
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
            static_ran: false,
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
}

pub(crate) struct ShadowPass {
    _map: wgpu::Texture,
    layer_views: [wgpu::TextureView; 2],
    array_view: wgpu::TextureView,
    compare_sampler: wgpu::Sampler,
    slots_buf: wgpu::Buffer,
    slots_bg: wgpu::BindGroup,
    world_pipeline: wgpu::RenderPipeline,
    entity_pipeline: wgpu::RenderPipeline,
    mask_layout: wgpu::BindGroupLayout,
    mask_buf: wgpu::Buffer,
    mask_pipeline: wgpu::RenderPipeline,
    /// The light matrix and casters the static layer holds.
    static_key: Option<([f32; 16], u64)>,
    /// This frame's matrix, set by [`ShadowPass::prepare`].
    light_vp: Mat4,
    static_due: bool,
    moved: usize,
    timer: Option<Timer>,
    pub(crate) stats: ShadowStats,
}

impl ShadowPass {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene_format: wgpu::TextureFormat,
        samples: u32,
        ent: EntLayouts<'_>,
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
            slots_buf,
            slots_bg,
            world_pipeline,
            entity_pipeline,
            mask_layout,
            mask_buf,
            mask_pipeline,
            static_key: None,
            light_vp: Mat4::IDENTITY,
            static_due: false,
            moved: 0,
            timer: Timer::new(device, queue),
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
        let light_vp = light_matrix(sun.dir, eye, fwd, casters.bounds);
        self.light_vp = light_vp;
        let key = (light_vp.to_cols_array(), casters.id);
        self.static_due = self.static_key != Some(key);
        self.static_key = Some(key);
        self.moved = moved.len().min(MAX_SLOTS - 1);
        let slots = std::iter::once(light_vp).chain(moved.iter().map(|&pose| light_vp * pose));
        for (i, m) in slots.take(MAX_SLOTS).enumerate() {
            queue.write_buffer(
                &self.slots_buf,
                i as u64 * SLOT_STRIDE,
                bytemuck::cast_slice(&m.to_cols_array()),
            );
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
        queue.write_buffer(&self.mask_buf, 0, bytemuck::cast_slice(&u));
    }

    /// The caster passes, before the frame's main pass: the static layer
    /// when the matrix or the world changed, the dynamic layer every frame.
    pub(crate) fn encode_maps(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        world_vertices: &wgpu::Buffer,
        casters: &Casters,
        moved_models: &[usize],
        ents: &[EntCaster<'_>],
        eb: &EntBindings<'_>,
    ) {
        let pass_desc = |view, tw| wgpu::RenderPassDescriptor {
            label: Some("shadow casters"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: tw,
            occlusion_query_set: None,
            multiview_mask: None,
        };
        let timer = self.timer.as_ref();
        if self.static_due {
            self.stats.static_draws += 1;
            let mut pass = encoder.begin_render_pass(&pass_desc(
                &self.layer_views[0],
                timer.and_then(|t| t.writes(0)),
            ));
            pass.set_pipeline(&self.world_pipeline);
            pass.set_bind_group(0, &self.slots_bg, &[0]);
            pass.set_vertex_buffer(0, world_vertices.slice(..));
            pass.set_index_buffer(casters.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..casters.count, 0, 0..1);
        }
        let mut pass = encoder.begin_render_pass(&pass_desc(
            &self.layer_views[1],
            timer.and_then(|t| t.writes(1)),
        ));
        pass.set_pipeline(&self.world_pipeline);
        pass.set_vertex_buffer(0, world_vertices.slice(..));
        pass.set_index_buffer(casters.index_buf.slice(..), wgpu::IndexFormat::Uint32);
        for (i, &m) in moved_models.iter().take(self.moved).enumerate() {
            let Some(r) = casters.model_ranges.get(m) else {
                continue;
            };
            if r.is_empty() {
                continue;
            }
            pass.set_bind_group(0, &self.slots_bg, &[((i + 1) as u64 * SLOT_STRIDE) as u32]);
            pass.draw_indexed(r.clone(), 0, 0..1);
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
        let static_due = self.static_due;
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
        let smooth = |old: &mut f32, new: f32| *old = if *old == 0.0 { new } else { *old * 0.9 + new * 0.1 };
        if t.static_ran {
            smooth(&mut self.stats.static_ms, ms(0));
        }
        smooth(&mut self.stats.dynamic_ms, ms(2));
        smooth(&mut self.stats.mask_ms, ms(4));
    }

    /// The F3 line.
    pub(crate) fn debug_line(&self) -> String {
        let s = &self.stats;
        if self.timer.is_none() {
            return format!(
                "shadow: casters {} static draws {} (no gpu timer)",
                s.casters, s.static_draws
            );
        }
        format!(
            "shadow: casters {} static {} gpu static {:.2} dyn {:.2} mask {:.2}ms",
            s.casters, s.static_draws, s.static_ms, s.dynamic_ms, s.mask_ms
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
        let bounds = (Vec3::new(-3000.0, -2000.0, -500.0), Vec3::new(2500.0, 3000.0, 1200.0));
        let eye = Vec3::new(100.0, 200.0, 50.0);
        let m = light_matrix(sun, eye, Vec3::X, bounds);
        // the eye sits inside the map, short of its centre by about AHEAD
        let e = ndc(m, eye);
        assert!(e.x.abs() < 1.0 && e.y.abs() < 1.0, "{e}");
        let ahead = ndc(m, eye + Vec3::X * AHEAD);
        let snap_ndc = SNAP / HALF_EXTENT;
        assert!(ahead.x.abs() <= snap_ndc && ahead.y.abs() <= snap_ndc, "{ahead}");
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
    fn light_matrix_holds_still_within_a_snap_step() {
        let sun = Vec3::new(0.3, 0.4, 0.8).normalize();
        let bounds = (Vec3::splat(-4000.0), Vec3::splat(4000.0));
        let a = light_matrix(sun, Vec3::new(10.0, 10.0, 0.0), Vec3::Y, bounds);
        let b = light_matrix(sun, Vec3::new(14.0, 12.0, 0.0), Vec3::Y, bounds);
        assert_eq!(a, b);
    }

    #[test]
    fn mask_and_cast_shaders_validate() {
        use wgpu::naga;
        for src in [include_str!("shadow_cast.wgsl"), include_str!("shadow_mask.wgsl")] {
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
