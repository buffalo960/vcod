//! Draws the cgame's impact marks (`crate::marks`) through the world's
//! stage pipelines: each mark shader per lightmap page is its own material,
//! as the renderer copies the shader per surface lightmap (CoDMP.exe
//! 0x4e9369), and its draws take the shader's sort in the frame's list.

use std::collections::HashMap;

use vcod_common::bsp::{self, DrawVert};
use vcod_common::pk3::Pk3Fs;
use vcod_common::shader::{
    BlendFactor, Bundle, DepthFunc, ImageRef, RgbGen, AlphaGen, Shader, ShaderLib, Stage, TcGen,
    TexEnv,
};

use super::{
    BundleViews, STAGE_PARAMS_SIZE, STAGE_VARIANTS, StageBatch, stage_animated,
    stage_bind_groups, stage_draw_facts, stage_params,
};
use crate::marks::{MAX_MARK_VERTS, MAX_MARKS, MarkPool, MarkRequest, MarkWorld};

type StagePipelines = [[[wgpu::RenderPipeline; 2]; 2]; STAGE_VARIANTS];

/// Params slots for mark shader stages; the stock decals use a few dozen.
const MAX_MARK_SLOTS: u64 = 256;
const MAX_VERTS: usize = MAX_MARKS * MAX_MARK_VERTS;
const MAX_INDICES: usize = MAX_MARKS * (MAX_MARK_VERTS - 2) * 3;

/// A mark shader's parse and its params slot per stage.
struct MarkShader {
    shader: Shader,
    sort: f32,
    slots: Vec<u32>,
}

/// One run of marks sharing a shader and a lightmap page, in draw order.
pub(super) struct MarkRun {
    key: (String, u16),
    first: u32,
    count: u32,
    pub(super) sort: f32,
}

/// What `MarkPass::update` borrows from the renderer.
pub(super) struct MarkCtx<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub fs: &'a Pk3Fs,
    pub shader_lib: &'a ShaderLib,
    pub material_layout: &'a wgpu::BindGroupLayout,
    pub stage_layout: &'a wgpu::BindGroupLayout,
    pub white_view: &'a wgpu::TextureView,
    pub diffuse_sampler: &'a wgpu::Sampler,
    pub lightmap_sampler: &'a wgpu::Sampler,
    pub image_gamma: Option<&'a [u8; 256]>,
}

pub(super) struct MarkPass {
    pool: MarkPool,
    world: MarkWorld,
    lightmaps: Vec<wgpu::TextureView>,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    params_buf: wgpu::Buffer,
    params_stride: u64,
    slots_used: u32,
    /// `None`: no stage draws, or out of params slots; warned once.
    shaders: HashMap<String, Option<MarkShader>>,
    materials: HashMap<(String, u16), Vec<StageBatch>>,
    views: HashMap<String, wgpu::TextureView>,
    pub(super) runs: Vec<MarkRun>,
}

impl MarkPass {
    pub(super) fn new(device: &wgpu::Device) -> MarkPass {
        let align = device.limits().min_uniform_buffer_offset_alignment as u64;
        let params_stride = STAGE_PARAMS_SIZE.div_ceil(align) * align;
        let buf = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        MarkPass {
            pool: MarkPool::default(),
            world: MarkWorld::default(),
            lightmaps: Vec::new(),
            vertex_buf: buf(
                "mark vertices",
                (MAX_VERTS * std::mem::size_of::<DrawVert>()) as u64,
                wgpu::BufferUsages::VERTEX,
            ),
            index_buf: buf(
                "mark indices",
                (MAX_INDICES * 4) as u64,
                wgpu::BufferUsages::INDEX,
            ),
            params_buf: buf(
                "mark stage params",
                MAX_MARK_SLOTS * params_stride,
                wgpu::BufferUsages::UNIFORM,
            ),
            params_stride,
            slots_used: 0,
            shaders: HashMap::new(),
            materials: HashMap::new(),
            views: HashMap::new(),
            runs: Vec::new(),
        }
    }

    /// A new map: its triangles and lightmap pages; the old map's marks go.
    pub(super) fn load(&mut self, world: MarkWorld, lightmaps: Vec<wgpu::TextureView>) {
        self.unload();
        log::info!("marks: {} world triangles take marks", world.triangle_count());
        self.world = world;
        self.lightmaps = lightmaps;
    }

    pub(super) fn unload(&mut self) {
        self.pool.clear();
        self.world = MarkWorld::default();
        self.lightmaps.clear();
        self.materials.clear();
        self.runs.clear();
    }

    /// Search paths changed: shaders and images resolve again.
    pub(super) fn reopen(&mut self) {
        self.shaders.clear();
        self.materials.clear();
        self.views.clear();
        self.slots_used = 0;
    }

    pub(super) fn live(&self) -> usize {
        self.pool.len()
    }

    /// One frame of `CG_ImpactMark` and `CG_AddMarks`: place `requests`,
    /// drop the expired, fade the rest and upload them newest first, one
    /// run per (shader, lightmap) ordered by sort. With `cg_marks 0` the
    /// cgame neither places nor draws nor expires marks.
    pub(super) fn update(
        &mut self,
        ctx: &MarkCtx,
        requests: &[MarkRequest],
        now_ms: i32,
        time: f32,
        enabled: bool,
    ) {
        self.runs.clear();
        if !enabled {
            return;
        }
        for req in requests {
            self.pool.impact(&self.world, req, now_ms);
        }
        self.pool.expire(now_ms);

        let mut groups: Vec<((String, u16), Vec<usize>)> = Vec::new();
        for (i, m) in self.pool.newest_first().enumerate() {
            match groups
                .iter_mut()
                .find(|(k, _)| k.0 == m.shader && k.1 == m.lightmap)
            {
                Some((_, v)) => v.push(i),
                None => groups.push(((m.shader.clone(), m.lightmap), vec![i])),
            }
        }
        groups.retain(|(key, _)| self.material(ctx, key).is_some());
        let marks: Vec<_> = self.pool.newest_first().collect();
        let mut verts: Vec<DrawVert> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut runs = Vec::new();
        for (key, members) in groups {
            let Some(sort) = self.shaders[&key.0].as_ref().map(|s| s.sort) else {
                continue;
            };
            let first = indices.len() as u32;
            for i in members {
                let m = marks[i];
                let rgba = m.rgba_at(now_ms);
                let base = verts.len() as u32;
                verts.extend(m.verts.iter().map(|v| DrawVert {
                    pos: v.pos,
                    uv: v.st,
                    lm_uv: v.lm,
                    normal: [0.0; 3],
                    color: rgba,
                }));
                for k in 1..m.verts.len().saturating_sub(1) as u32 {
                    indices.extend_from_slice(&[base, base + k, base + k + 1]);
                }
            }
            runs.push(MarkRun {
                key,
                first,
                count: indices.len() as u32 - first,
                sort,
            });
        }
        // stable: newest first within a sort
        runs.sort_by(|a, b| a.sort.total_cmp(&b.sort));
        ctx.queue
            .write_buffer(&self.vertex_buf, 0, bytemuck::cast_slice(&verts));
        ctx.queue
            .write_buffer(&self.index_buf, 0, bytemuck::cast_slice(&indices));
        for ms in self.shaders.values().flatten() {
            for (si, &slot) in ms.slots.iter().enumerate() {
                if stage_animated(&ms.shader, si)
                    && let Some(p) = stage_params(&ms.shader, si, time)
                {
                    ctx.queue.write_buffer(
                        &self.params_buf,
                        u64::from(slot) * self.params_stride,
                        bytemuck::bytes_of(&p),
                    );
                }
            }
        }
        self.runs = runs;
    }

    /// The stage draws for a (shader, lightmap), built on first use.
    fn material(&mut self, ctx: &MarkCtx, key: &(String, u16)) -> Option<&Vec<StageBatch>> {
        if !self.shaders.contains_key(&key.0) {
            let ms = self.register(ctx, &key.0);
            self.shaders.insert(key.0.clone(), ms);
        }
        let ms = self.shaders[&key.0].as_ref()?;
        if !self.materials.contains_key(key) {
            let lm_view = match key.1 {
                bsp::NO_LIGHTMAP => None,
                i => self.lightmaps.get(i as usize),
            };
            let mut bundle_views = BundleViews {
                views: std::mem::take(&mut self.views),
                table: ctx.image_gamma,
            };
            let mut batches = Vec::new();
            for (si, st) in ms.shader.stages.iter().enumerate() {
                let Some((variant, two_sided, bias)) = stage_draw_facts(&ms.shader, si) else {
                    continue;
                };
                let (mat, stage) = stage_bind_groups(
                    ctx.device,
                    ctx.queue,
                    ctx.fs,
                    ctx.material_layout,
                    ctx.stage_layout,
                    &self.params_buf,
                    STAGE_PARAMS_SIZE,
                    ctx.white_view,
                    lm_view,
                    ctx.diffuse_sampler,
                    ctx.lightmap_sampler,
                    &mut bundle_views,
                    st,
                );
                batches.push(StageBatch {
                    slot: ms.slots[si],
                    variant,
                    two_sided,
                    bias,
                    mat,
                    stage,
                });
            }
            self.views = bundle_views.views;
            self.materials.insert(key.clone(), batches);
        }
        self.materials.get(key)
    }

    /// Parses a mark shader and fills its params slots. A name with no
    /// script draws its image blended under vertex colour.
    fn register(&mut self, ctx: &MarkCtx, name: &str) -> Option<MarkShader> {
        let shader = match ctx.shader_lib.get(name) {
            Some(sh) if !sh.stages.is_empty() => sh.clone(),
            Some(_) => {
                log::warn!("mark shader {name}: no stages, not drawn");
                return None;
            }
            None => {
                log::warn!("mark shader {name}: no script, drawing its image blended");
                implicit_mark_shader(name)
            }
        };
        let n = shader.stages.len() as u32;
        if u64::from(self.slots_used + n) > MAX_MARK_SLOTS {
            log::warn!("mark shader {name}: out of params slots, not drawn");
            return None;
        }
        let slots: Vec<u32> = (self.slots_used..self.slots_used + n).collect();
        self.slots_used += n;
        for (si, &slot) in slots.iter().enumerate() {
            if let Some(p) = stage_params(&shader, si, 0.0) {
                ctx.queue.write_buffer(
                    &self.params_buf,
                    u64::from(slot) * self.params_stride,
                    bytemuck::bytes_of(&p),
                );
            }
        }
        Some(MarkShader {
            sort: shader.sort_value(),
            shader,
            slots,
        })
    }

    /// Draws run `ri`'s stages back to back; the caller has bound the
    /// world camera at group 0.
    pub(super) fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        ri: usize,
        pipelines: &StagePipelines,
        time: f32,
    ) {
        let run = &self.runs[ri];
        let Some(batches) = self.materials.get(&run.key) else {
            return;
        };
        pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
        pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint32);
        for sb in batches {
            pass.set_pipeline(
                &pipelines[sb.variant as usize][usize::from(sb.two_sided)][usize::from(sb.bias)],
            );
            pass.set_bind_group(1, sb.mat.bind_group(time), &[]);
            pass.set_bind_group(
                2,
                sb.stage.bind_group(time),
                &[u32::try_from(u64::from(sb.slot) * self.params_stride)
                    .expect("mark params slot offset fits a dynamic offset")],
            );
            pass.draw_indexed(run.first..run.first + run.count, 0, 0..1);
        }
    }
}

fn implicit_mark_shader(name: &str) -> Shader {
    let bundle = |image| Bundle {
        image,
        anim: None,
        clamp: false,
        tcmods: Vec::new(),
        tc_gen: TcGen::Base,
        env: TexEnv::Modulate,
    };
    Shader {
        name: name.to_string(),
        stages: vec![Stage {
            bundles: vec![
                bundle(ImageRef::Path(name.to_string())),
                bundle(ImageRef::Lightmap),
            ],
            blend: Some((BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha)),
            depth_write: None,
            alpha_func: None,
            depth_func: DepthFunc::LessEqual,
            rgb_gen: RgbGen::Vertex,
            alpha_gen: AlphaGen::Vertex,
        }],
        polygon_offset: true,
        ..Shader::default()
    }
}
