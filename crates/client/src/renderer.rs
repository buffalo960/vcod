use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use wgpu::util::DeviceExt;

use crate::camera::{Z_FAR, Z_NEAR};
use crate::dlight::{self, DlightBlend};
use crate::entity_light::{self, GpuLightSet};
use crate::entity_material::{
    ENT_STAGE_STRIDE, EntPass, EntSkin, EntityMaterials, PassLayouts, StageImage,
};
use crate::fx::sim::{FxLight, FxQuad, MAX_LIGHTS};
use crate::gamma::GammaPass;
use crate::hud::HudQuad;
use crate::hud_text::{self, HudVert};
use crate::lightmap_blocks::{self, LightmapPage};
use crate::shadows;
use crate::sky;
use crate::sun_flare::{FlareParams, SunFlareGpu};
use vcod_common::assets::{self, Image, ImageData};
use vcod_common::bsp::{self, Bsp, DrawVert};
use vcod_common::deform;
use vcod_common::mesh::{self, Batch, IndexRange, MaterialKind};
use vcod_common::net::FogParams;
use vcod_common::pk3::Pk3Fs;
use vcod_common::props;
use vcod_common::shader::{
    AlphaFunc, AlphaGen, BlendFactor, Bundle, DLIGHT_IMAGE, DepthFunc, DlightDraw, DrawClass,
    ImageRef, RgbGen, SORT_ADDITIVE, SORT_BLEND, SORT_DECAL, SORT_OPAQUE, Shader, ShaderLib,
    Sprite, Stage, SunFile, TcGen, TexEnv, Wave, bundle_affine, bundle_turb, has_animated_tcmods,
    wave_value,
};
use vcod_common::static_light::{SceneLight, StaticLighting};
use vcod_common::vis::{Frustum, Visible, WorldVis};
use vcod_common::xmodel::{self, VmVert};

mod mark_pass;

// The vertex layout below hardcodes the BSP drawvert stride.
const _: () = assert!(std::mem::size_of::<DrawVert>() == 44);
// Ditto the viewmodel vertex stride.
const _: () = assert!(std::mem::size_of::<VmVert>() == 52);

pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub(crate) const MSAA_SAMPLES: u32 = 4;
/// Every pass draws retail's framebuffer bytes into this, blending and
/// clamping in the same byte space; the gamma pass maps it onto the
/// swapchain (docs/research/cod11-gamma.md, section 4).
pub const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// `Camera` in shader.wgsl: proj, time, eye/fog tail, view forward, model.
const CAMERA_FLOATS: usize = 52;
const CAMERA_BYTES: u64 = (CAMERA_FLOATS * 4) as u64;

/// The viewmodel shares the world's fov but has its own near plane, retail's
/// `r_znear_depthhack` default (docs/research/xmodel-v14-format.md, "The
/// view fov").
pub const VM_NEAR: f32 = 0.1;
const VM_FAR: f32 = 500.0;
/// The depth range's front slice the viewmodel is squeezed into, retail's
/// 0..0.2 for a depth-hack entity (CoDMP.exe 0x4d6a7e).
const VM_DEPTH_RANGE: f32 = 0.2;
/// `proj` (64) + `model` (64) + the light set (656), std140-compatible as is.
const VM_UNIFORM_SIZE: u64 = VM_MISC_OFFSET + 16;
/// The light set sits after the projection and motion transform.
const VM_LIGHTS_OFFSET: u64 = 128;
/// `misc` (x identityLight) follows the light set.
const VM_MISC_OFFSET: u64 = VM_LIGHTS_OFFSET + std::mem::size_of::<GpuLightSet>() as u64;
/// Matches `array<mat4x4<f32>, 64>` in viewmodel.wgsl.
const VM_BONE_COUNT: usize = 64;
const VM_BONE_BUF_SIZE: u64 = (VM_BONE_COUNT * 64) as u64;

/// Debug-HUD glyph budget per frame; lines past it are cut.
const MAX_HUD_GLYPHS: usize = 1024;

/// `fx::sim::MAX_PARTICLES` and room for the head icons.
const MAX_FX_QUADS: usize = 2048 + 256;

const MAX_HUD_QUADS: usize = 16384;

/// Per-vertex rgba lets particles, decals and tracers share one pipeline.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct FxVert {
    pos: [f32; 3],
    uv: [f32; 2],
    rgba: [f32; 4],
}

// The vertex layout below hardcodes this stride.
const _: () = assert!(std::mem::size_of::<FxVert>() == 36);

/// The world batches' pipelines. Layers (polygonOffset terrain blends) and
/// overlays (decals, masked cutouts) are coplanar with the world and draw
/// depth-biased on top. `Stage` marks batches expanded into per-stage draws
/// instead. Placement comes from the batch's sort key (`order_draws`).
#[derive(Copy, Clone, PartialEq)]
enum Pass {
    Opaque,
    Prop,
    Layer,
    Overlay,
    Stage,
}

struct DrawCall {
    bind_group: usize,
    pass: Pass,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum CullMode {
    On,
    Locked,
    Off,
}

impl CullMode {
    pub fn next(self) -> Self {
        match self {
            CullMode::On => CullMode::Locked,
            CullMode::Locked => CullMode::Off,
            CullMode::Off => CullMode::On,
        }
    }
}

impl std::fmt::Display for CullMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            CullMode::On => "on",
            CullMode::Locked => "locked",
            CullMode::Off => "off",
        })
    }
}

/// A per-frame draw: `batch` picks the bind group and pass, the range is in
/// the gathered index buffer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DrawRange {
    pub batch: u32,
    pub first: u32,
    pub count: u32,
}

/// One entry of a frame's emission-ordered list.
#[derive(Copy, Clone)]
enum DrawRef {
    /// A legacy batch on its fixed pipeline.
    Legacy(Pass),
    /// An authored stage draw; index into `WorldGpu::stage_batches`.
    Stage(u32),
}

struct FrameDraw {
    kind: DrawRef,
    /// The batch's material sort and its place in the sorted shader list
    /// (`WorldGpu::batch_keys`).
    sort: f32,
    rank: u32,
    /// Index into `WorldGpu::draws`, the gathered visible range.
    src: u32,
}

/// The pipeline a run of world draws has bound.
#[derive(Copy, Clone, PartialEq)]
enum Bound {
    Legacy(Pass),
    Stage(StageVariant, bool, bool),
}

/// One entity or viewmodel surface in draw order.
struct EntDraw {
    /// Instance (or viewmodel part) and its surface.
    inst: u32,
    surf: u32,
    sort: f32,
    order: u32,
    /// A viewmodel surface: depth-hack range and the view camera.
    gun: bool,
}

/// The live entities' and the viewmodel's `(instance, surface, material)`
/// in instance order, in one list sorted as retail's key does: the
/// material's sort, its load order (the sorted index), then the entity
/// number. The gun is added after the packet entities, so on one material
/// it draws after them (docs/research/cod11-shader-scripts.md, §5).
fn sorted_ent_draws<'a>(
    ents: impl Iterator<Item = (u32, u32, &'a EntSkin)>,
    gun: impl Iterator<Item = (u32, u32, &'a EntSkin)>,
) -> Vec<EntDraw> {
    let draw = |is_gun| {
        move |(inst, surf, sk): (u32, u32, &EntSkin)| EntDraw {
            inst,
            surf,
            sort: sk.sort,
            order: sk.order,
            gun: is_gun,
        }
    };
    let mut out: Vec<EntDraw> = ents.map(draw(false)).chain(gun.map(draw(true))).collect();
    // stable: equal keys keep instance then surface order
    out.sort_by(|a, b| {
        a.sort
            .total_cmp(&b.sort)
            .then(a.order.cmp(&b.order))
            .then(a.gun.cmp(&b.gun))
    });
    out
}

/// Where the dlight pass goes in the merged list: after the last
/// world draw that sorts up to opaque, as retail lights a batch right
/// after its base stages (0x500d3a) and only those sorts take it.
fn dlight_slot(items: &[SortedItem], world_sorts: &[f32]) -> usize {
    items
        .iter()
        .rposition(|it| matches!(it, SortedItem::World(wi) if world_sorts[*wi] <= SORT_OPAQUE))
        .map_or(0, |at| at + 1)
}

/// The frame's MSAA colour and depth. With SSAO the frame is two passes
/// split after the last opaque or decal draw (`ssao_split`): the first
/// keeps its MSAA colour for the SSAO multiply, the second loads it and
/// resolves.
struct SceneTargets<'a> {
    msaa: &'a wgpu::TextureView,
    depth: &'a wgpu::TextureView,
    resolve: &'a wgpu::TextureView,
    timer: Option<&'a crate::ssao::GpuTimer>,
    clear: wgpu::Color,
    /// The sun flare's probe query, which runs in the last pass.
    sun_queries: Option<&'a wgpu::QuerySet>,
}

impl SceneTargets<'_> {
    fn begin<'e>(
        &self,
        encoder: &'e mut wgpu::CommandEncoder,
        first: bool,
        last: bool,
    ) -> wgpu::RenderPass<'e> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("map pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.msaa,
                depth_slice: None,
                resolve_target: last.then_some(self.resolve),
                ops: wgpu::Operations {
                    load: if first {
                        wgpu::LoadOp::Clear(self.clear)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    // only the resolved single-sample image is needed
                    store: if last {
                        wgpu::StoreOp::Discard
                    } else {
                        wgpu::StoreOp::Store
                    },
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: self.depth,
                depth_ops: Some(wgpu::Operations {
                    load: if first {
                        wgpu::LoadOp::Clear(1.0)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: self.timer.map(|t| {
                t.writes(
                    first.then_some(crate::ssao::GpuTimer::SCENE_BEGIN),
                    last.then_some(crate::ssao::GpuTimer::SCENE_END),
                )
            }),
            occlusion_query_set: if last { self.sun_queries } else { None },
            multiview_mask: None,
        })
    }
}

/// Where the SSAO multiply goes in the merged list: after the last draw
/// that sorts up to decal, so it darkens the opaque surfaces and the
/// polygonOffset layers and impact marks blended onto them and none of the
/// blends after.
fn ssao_split(
    items: &[SortedItem],
    world: &[f32],
    marks: &[f32],
    models: &[f32],
    fx: &[f32],
) -> usize {
    items
        .iter()
        .rposition(|it| {
            let sort = match *it {
                SortedItem::World(i) => world[i],
                SortedItem::Mark(i) => marks[i],
                SortedItem::Model(i) => models[i],
                SortedItem::Fx(i) => fx[i],
            };
            sort <= SORT_DECAL
        })
        .map_or(0, |at| at + 1)
}

/// Draws one entity surface's stages back to back, as retail's stage
/// iterator does. `bound` tracks the bound pipeline across calls.
fn draw_ent_stages(
    pass: &mut wgpu::RenderPass<'_>,
    mats: &EntityMaterials,
    which: EntPass,
    skin_bg: &wgpu::BindGroup,
    skin: &EntSkin,
    surf: &VmSurface,
    bound: &mut Option<usize>,
) {
    for st in &skin.stages {
        if *bound != Some(st.pipeline) {
            pass.set_pipeline(&mats.pipelines[st.pipeline].1[which as usize]);
            *bound = Some(st.pipeline);
        }
        pass.set_bind_group(
            1,
            match st.image {
                StageImage::Skin => skin_bg,
                StageImage::Own(i) => &mats.images[i],
            },
            &[],
        );
        pass.set_bind_group(
            3,
            &mats.params_bg,
            &[(u64::from(st.slot) * ENT_STAGE_STRIDE) as u32],
        );
        pass.draw_indexed(
            surf.first_index..surf.first_index + surf.index_count,
            surf.base_vertex,
            0..1,
        );
    }
}

/// The emission order of `draws`, into `out`: by the batch's place in the
/// sorted shader list (`keys`, material sort first), each batch's stages
/// back to back in stage order, as retail's draw loop iterates a shader's
/// stages over its surfaces (shader doc 5). Sky soups are never in `draws`.
fn order_draws(
    keys: &[(f32, u32)],
    calls: &[DrawCall],
    stages_of_batch: &[Vec<u32>],
    draws: &[DrawRange],
    out: &mut Vec<FrameDraw>,
) {
    out.clear();
    for (di, d) in draws.iter().enumerate() {
        let b = d.batch as usize;
        let (sort, rank) = keys[b];
        let at = |kind| FrameDraw {
            kind,
            sort,
            rank,
            src: di as u32,
        };
        match calls[b].pass {
            Pass::Stage => out.extend(stages_of_batch[b].iter().map(|&sb| at(DrawRef::Stage(sb)))),
            p => out.push(at(DrawRef::Legacy(p))),
        }
    }
    // stable: a batch keeps its stage order
    out.sort_by_key(|fd| fd.rank);
}

/// A shader as R_SortNewShader (CoDMP.exe 0x4fb790) reads it: its sort and
/// its first stage's bundle 0 and 1 image numbers (0 for none), `None` for
/// a shader without stages.
#[derive(Copy, Clone, Debug)]
struct SortEntry {
    sort: f32,
    images: Option<[u32; 2]>,
}

/// A scripted shader's entry: its stages' images numbered in stage order,
/// then its sort and its sorting stage's two bundle images.
fn staged_sort_entry(sh: &Shader, image: &mut dyn FnMut(&ImageRef) -> u32) -> SortEntry {
    for st in &sh.stages {
        for bu in &st.bundles {
            image(&bu.image);
            for p in bu.anim.iter().flat_map(|a| &a.paths) {
                image(&ImageRef::Path(p.clone()));
            }
        }
    }
    SortEntry {
        sort: sh.sort_value(),
        images: sh.sorting_stage().map(|st| {
            let mut at = |i: usize| st.bundles.get(i).map_or(0, |b| image(&b.image));
            [at(0), at(1)]
        }),
    }
}

/// Each entry's place in retail's sorted shader list when registered in
/// this order. R_SortNewShader walks back from the end and puts the new
/// shader after the first one that sorts lower, or sorts the same with a
/// lower bundle 0 image, or the same bundle 0 image and a bundle 1 image
/// no higher; it passes everything else, a stageless shader and a missing
/// bundle 1 included.
fn sorted_places(entries: &[SortEntry]) -> Vec<u32> {
    let mut list: Vec<usize> = Vec::with_capacity(entries.len());
    for (n, new) in entries.iter().enumerate() {
        let [b0, b1] = new.images.unwrap_or([0, 0]);
        let mut at = list.len();
        while at > 0 {
            let e = &entries[list[at - 1]];
            if e.sort < new.sort {
                break;
            }
            if e.sort == new.sort
                && let Some([e0, e1]) = e.images
            {
                let b0_holds = e0 == 0 || e0 == b0;
                if e0 != 0 && e0 < b0 {
                    break;
                }
                if b0_holds && e1 != 0 && e1 <= b1 {
                    break;
                }
            }
            at -= 1;
        }
        list.insert(at, n);
    }
    let mut place = vec![0; entries.len()];
    for (p, &e) in list.iter().enumerate() {
        place[e] = p as u32;
    }
    place
}

/// Retail's image numbers in load order (`image+0x58`, CoDMP.exe
/// 0x4ec310: the creation count plus 0x400); 0 stays "no image".
#[derive(Default)]
struct ImageNumbers(HashMap<String, u32>);

impl ImageNumbers {
    fn number(&mut self, name: &str) -> u32 {
        let mut key = name.to_ascii_lowercase();
        for ext in [".tga", ".jpg", ".dds"] {
            if let Some(stem) = key.strip_suffix(ext) {
                key.truncate(stem.len());
                break;
            }
        }
        let next = self.0.len() as u32 + 1;
        *self.0.entry(key).or_insert(next)
    }
}

/// One entry of the frame's merged draw list.
#[derive(Copy, Clone, Debug, PartialEq)]
enum SortedItem {
    World(usize),
    /// A run of impact marks (`mark_pass::MarkPass::runs`).
    Mark(usize),
    /// An entity or viewmodel surface.
    Model(usize),
    Fx(usize),
}

/// Merges the world's draws, the mark runs, the entity and viewmodel
/// surfaces and the fx runs, each list already in sort order, by material
/// sort. On a tie the world goes first, then the marks, the models and the
/// fx: the world's materials and images register at map load, before the
/// cgame's (R_SortNewShader breaks ties on image numbers, 0x4fb790). The
/// marks' place among the cgame's lists is vcod's guess.
fn merge_by_sort(world: &[f32], marks: &[f32], models: &[f32], fx: &[f32]) -> Vec<SortedItem> {
    let lists = [world, marks, models, fx];
    let mut at = [0usize; 4];
    let mut out = Vec::with_capacity(lists.iter().map(|l| l.len()).sum());
    loop {
        let mut best: Option<usize> = None;
        for l in 0..4 {
            if at[l] < lists[l].len() && best.is_none_or(|b| lists[l][at[l]] < lists[b][at[b]]) {
                best = Some(l);
            }
        }
        let Some(l) = best else {
            break;
        };
        out.push(match l {
            0 => SortedItem::World(at[l]),
            1 => SortedItem::Mark(at[l]),
            2 => SortedItem::Model(at[l]),
            _ => SortedItem::Fx(at[l]),
        });
        at[l] += 1;
    }
    out
}

/// What a merged list's last draw left bound.
enum Emit {
    None,
    World { set: usize, bound: Option<Bound> },
    Ent { inst: u32, bound: Option<usize> },
    Gun { part: u32, bound: Option<usize> },
    Fx { additive: bool },
}

/// Copies the visible `ranges` out of `src` batch by batch into `out`, one
/// `DrawRange` per batch that got anything. `per_batch` is scratch, kept
/// between frames.
pub(crate) fn gather(
    src: &[u32],
    ranges: impl IntoIterator<Item = IndexRange>,
    batch_count: usize,
    out: &mut Vec<u32>,
    per_batch: &mut Vec<Vec<IndexRange>>,
) -> Vec<DrawRange> {
    per_batch.resize_with(batch_count, Vec::new);
    for b in per_batch.iter_mut() {
        b.clear();
    }
    for r in ranges {
        per_batch[r.batch as usize].push(r);
    }
    out.clear();
    let mut draws = Vec::new();
    for (batch, rs) in per_batch.iter().enumerate() {
        if rs.is_empty() {
            continue;
        }
        let first = out.len() as u32;
        for r in rs {
            out.extend_from_slice(&src[r.first as usize..(r.first + r.count) as usize]);
        }
        draws.push(DrawRange {
            batch: batch as u32,
            first,
            count: out.len() as u32 - first,
        });
    }
    draws
}

struct VmSurface {
    first_index: u32,
    index_count: u32,
    /// Surfaces share one vertex buffer; the parsed u16 indices are uploaded unrebased.
    base_vertex: i32,
    bind_group: usize,
}

struct VmModel {
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    bind_groups: Vec<wgpu::BindGroup>,
    surfaces: Vec<VmSurface>,
    /// Per surface, its skin's name: the key of its material.
    skins: Vec<String>,
    bone_buf: wgpu::Buffer,
    bone_bg: wgpu::BindGroup,
}

/// Built up front so the shader is validated at startup even with no viewmodel set.
struct VmPass {
    uniform_layout: wgpu::BindGroupLayout,
    uniform_buf: wgpu::Buffer,
    uniform_bg: wgpu::BindGroup,
    skin_layout: wgpu::BindGroupLayout,
    bone_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    models: Vec<Rc<VmModel>>,
    /// Per model, per surface, its material; empty until resolved against
    /// this map's entity materials.
    skins: Vec<Vec<Rc<EntSkin>>>,
    /// Every viewmodel part uploaded so far, keyed by [`vm_cache_key`], so a
    /// weapon switch back to a gun (and every gun's shared hands) skips the
    /// upload. Only one viewmodel draws at a time, so parts can share their
    /// bone buffer.
    cache: HashMap<String, Rc<VmModel>>,
}

/// LOD name plus skins: the hands files share one mesh and differ only in
/// their sleeve skins.
fn vm_cache_key(m: &xmodel::XModel) -> String {
    format!("{}|{}", m.lod, m.materials.join("|"))
}

/// Each player is 4-7 part instances, so 32 players plus corpses and dropped
/// weapons land at 200-280.
const MAX_DYNAMIC_INSTANCES: usize = 512;
pub(crate) const DYNAMIC_INSTANCE_STRIDE: u64 = 80;
/// Unskinned instances point at a shared identity block of this many matrices.
pub const MAX_INSTANCE_BONES: usize = 128;

/// Index into the renderer's dynamic model table. `pub(crate)` so other
/// modules' tests can build a dummy handle without a GPU.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ModelHandle(pub(crate) usize);

/// `bones: None` draws the baked bind pose; `Some` is truncated at [`MAX_INSTANCE_BONES`].
pub struct DynamicModelInstance {
    pub model: ModelHandle,
    pub transform: glam::Mat4,
    pub bones: Option<Vec<glam::Mat4>>,
    /// Where the light grid is sampled: the refEntity's lighting origin.
    pub light_origin: glam::Vec3,
}

/// Per-instance vertex data. `bone_base` 0 is the shared identity block.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw {
    pub transform: [f32; 16],
    pub bone_base: u32,
    /// Index into the frame's light sets.
    pub light_set: u32,
    pub _pad: [u32; 2],
}

// The instance vertex layout below hardcodes this stride.
const _: () = assert!(std::mem::size_of::<InstanceRaw>() == 80);

/// Bone buffer slot 0 is a [`MAX_INSTANCE_BONES`]-matrix identity block and
/// `None` bone sets point there. The `usize` per tuple is the model index, unused here.
#[allow(clippy::type_complexity)]
pub fn pack_instances(
    instances: &[(usize, [f32; 16], Option<&[glam::Mat4]>)],
) -> (Vec<InstanceRaw>, Vec<[f32; 16]>) {
    static BONE_CAP_WARNED: std::sync::Once = std::sync::Once::new();

    let mut mats: Vec<[f32; 16]> = vec![glam::Mat4::IDENTITY.to_cols_array(); MAX_INSTANCE_BONES];
    let mut raw = Vec::with_capacity(instances.len());
    for &(_, transform, bones) in instances {
        let bone_base = match bones {
            None => 0u32,
            Some(bones) => {
                let base = mats.len() as u32;
                let bones = if bones.len() > MAX_INSTANCE_BONES {
                    BONE_CAP_WARNED.call_once(|| {
                        log::warn!(
                            "dynamic instance bone set exceeds {MAX_INSTANCE_BONES} bones; truncating"
                        );
                    });
                    &bones[..MAX_INSTANCE_BONES]
                } else {
                    bones
                };
                mats.extend(bones.iter().map(glam::Mat4::to_cols_array));
                base
            }
        };
        raw.push(InstanceRaw {
            transform,
            bone_base,
            light_set: 0,
            _pad: [0; 2],
        });
    }
    (raw, mats)
}

/// `bone_sets[i]` belongs to `set_viewmodel` model i. Absent or empty leaves
/// that model's GPU buffer as it was.
pub struct VmDraw {
    pub transform: glam::Mat4,
    /// World-space lighting origin: the eye, `ps.origin` plus
    /// `viewHeightCurrent` (cgame 0x30036cf0).
    pub light_origin: glam::Vec3,
    /// Horizontal degrees, the world view's own.
    pub fov_x: f32,
    pub bone_sets: Vec<Vec<glam::Mat4>>,
}

pub struct Frame {
    pub view_proj: glam::Mat4,
    /// Camera position, the cell the visibility walk starts from.
    pub eye: glam::Vec3,
    /// Unit view forward. Fog depth is measured along it (eye-space Z),
    /// matching retail without GL_NV_fog_distance.
    pub fwd: glam::Vec3,
    /// The view matrix's up (`camera::up_hint`); with `fwd` it takes world
    /// lights into the viewmodel's view space.
    pub up: glam::Vec3,
    /// Seconds since start; drives tcMod and wave animation in the stage shaders.
    pub time: f32,
    pub cull: CullMode,
    /// Debug-HUD lines, top line first; empty when the overlay is off.
    pub hud_lines: Vec<String>,
}

/// Drawn/total per frame for the F3 `vis` line.
#[derive(Clone, Debug, Default)]
pub struct VisCounts {
    pub mode: Option<CullMode>,
    pub cells: (usize, usize),
    pub soups: (usize, usize),
    pub tris: (usize, usize),
    pub props: (usize, usize),
    /// Occluder volumes built and portals hidden by them this frame (On mode).
    pub occluders: usize,
    pub portals_occluded: usize,
    pub gather_ms: f32,
}

/// Matches `FxLights` in shader.wgsl (std140, vec4 aligned); `vs_prop` reads it.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct FxLightsUniform {
    /// xyz position, w radius; zero radius means unused.
    pos_radius: [[f32; 4]; MAX_LIGHTS],
    /// rgb, w unused.
    color: [[f32; 4]; MAX_LIGHTS],
}

impl FxLightsUniform {
    fn from_lights(lights: &[FxLight]) -> Self {
        let mut pos_radius = [[0.0f32; 4]; MAX_LIGHTS];
        let mut color = [[0.0f32; 4]; MAX_LIGHTS];
        for (i, l) in lights.iter().take(MAX_LIGHTS).enumerate() {
            pos_radius[i] = [l.pos[0], l.pos[1], l.pos[2], l.radius];
            color[i] = [l.rgb[0], l.rgb[1], l.rgb[2], 0.0];
        }
        Self { pos_radius, color }
    }
}

/// The fog values as currently displayed.
#[derive(Copy, Clone, Default, PartialEq)]
struct FogLive {
    linear: bool,
    color: [f32; 3],
    density: f32,
    near: f32,
    far: f32,
}

impl FogLive {
    fn of(p: &FogParams) -> Self {
        FogLive {
            linear: p.is_linear(),
            color: p.color,
            density: p.density,
            near: p.near,
            far: p.far,
        }
    }

    /// 1 = GL_EXP, 2 = GL_LINEAR — the WGSL Camera mode encoding; the off
    /// state is carried by [`FogState::set`], not here.
    fn mode(&self) -> f32 {
        if self.linear { 2.0 } else { 1.0 }
    }
}

/// Global fog from configstring 12, faded toward each new target. Same-mode
/// transitions lerp over the script's fade time; cross-mode changes snap and
/// linear fog clears to the fog colour — RTCW-MP tr_main.c R_SetFog /
/// R_AddFogInterpolation.
#[derive(Default)]
struct FogState {
    set: bool,
    live: FogLive,
    from: FogLive,
    /// target params, transition start (frame seconds), duration (s)
    target: Option<(FogParams, f32, f32)>,
}

impl FogState {
    fn set_target(&mut self, p: FogParams, now: f32) {
        // No fade, first fog, or a mode change snaps like retail.
        let snap = !self.set || p.fade_ms == 0 || self.live.linear != p.is_linear();
        self.from = self.live;
        self.target = Some((p, now, p.fade_ms as f32 / 1000.0));
        self.set = true;
        if snap {
            self.finish();
        }
    }

    fn clear(&mut self) {
        self.set = false;
        self.target = None;
    }

    fn finish(&mut self) {
        if let Some((p, _, _)) = self.target.take() {
            self.live = FogLive::of(&p);
        }
    }

    fn advance(&mut self, now: f32) {
        if self.target.is_none() {
            return;
        }
        let (_, t0, dur) = self.target.unwrap();
        let k = ((now - t0) / dur.max(f32::EPSILON)).clamp(0.0, 1.0);
        let to = match self.target {
            Some((p, _, _)) => FogLive::of(&p),
            None => return,
        };
        let f = &self.from;
        self.live = FogLive {
            linear: to.linear,
            color: [
                f.color[0] + (to.color[0] - f.color[0]) * k,
                f.color[1] + (to.color[1] - f.color[1]) * k,
                f.color[2] + (to.color[2] - f.color[2]) * k,
            ],
            density: f.density + (to.density - f.density) * k,
            near: f.near + (to.near - f.near) * k,
            far: f.far + (to.far - f.far) * k,
        };
        if k >= 1.0 {
            self.target = None;
        }
    }
}

#[cfg(test)]
mod fx_lights_uniform_tests {
    use super::*;

    #[test]
    fn is_256_bytes_std140() {
        assert_eq!(std::mem::size_of::<FxLightsUniform>(), 256);
    }

    #[test]
    fn empty_list_zeroes_every_slot() {
        let u = FxLightsUniform::from_lights(&[]);
        assert_eq!(u.pos_radius, [[0.0; 4]; MAX_LIGHTS]);
        assert_eq!(u.color, [[0.0; 4]; MAX_LIGHTS]);
    }

    #[test]
    fn packs_position_radius_and_color_into_matching_slots() {
        let lights = vec![
            FxLight {
                pos: [1.0, 2.0, 3.0],
                radius: 4.0,
                rgb: [0.5, 0.6, 0.7],
            },
            FxLight {
                pos: [10.0, 20.0, 30.0],
                radius: 40.0,
                rgb: [0.1, 0.2, 0.3],
            },
        ];
        let u = FxLightsUniform::from_lights(&lights);
        assert_eq!(u.pos_radius[0], [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(u.color[0], [0.5, 0.6, 0.7, 0.0]);
        assert_eq!(u.pos_radius[1], [10.0, 20.0, 30.0, 40.0]);
        assert_eq!(u.color[1], [0.1, 0.2, 0.3, 0.0]);
        for i in 2..MAX_LIGHTS {
            assert_eq!(u.pos_radius[i], [0.0; 4]);
        }
    }
}

// ---- shader-script stage machinery (consumed by Tasks 7-8) ----

pub const STAGE_FLAG_VERTEX_RGB: u32 = 1;
pub const STAGE_FLAG_VERTEX_ALPHA: u32 = 2;
pub const STAGE_FLAG_BUNDLE1_LIGHTMAP: u32 = 4;
pub const STAGE_FLAG_BUNDLE0_LIGHTMAP: u32 = 8;
// alphaFunc encoding in bits 16..32: 0 = none, GT0 = 16, LT128 = 32, GE128 = both
pub const STAGE_FLAG_ALPHAFUNC_GT0: u32 = 16;
pub const STAGE_FLAG_ALPHAFUNC_LT128: u32 = 32;
pub const STAGE_FLAG_ALPHAFUNC_GE128: u32 = 48;
pub const STAGE_FLAG_BUNDLE0_VECTOR: u32 = 64;
pub const STAGE_FLAG_BUNDLE1_VECTOR: u32 = 128;
/// Sky dome draws only: add the view origin (StageParams.eye_off) to vertices.
pub const STAGE_FLAG_EYE_OFFSET: u32 = 256;
/// Sky dome draws only: fog applies in exp mode, never in linear mode
/// (RTCW-MP tr_main.c R_SetFog sets drawsky per mode).
pub const STAGE_FLAG_SKY: u32 = 512;
/// `rgbGen vertex` scales the vertex rgb by identityLight; `exactVertex`
/// stays raw (RTCW-MP tr_shade.c CGEN_VERTEX vs CGEN_EXACT_VERTEX).
pub const STAGE_FLAG_VERTEX_RGB_HALF: u32 = 1024;
/// deformVertexes wave: displace vertices along their normals by
/// StageParams.wave = [base, amp, phase + t * rate, spread].
pub const STAGE_FLAG_DEFORM_WAVE: u32 = 2048;
/// The tint rgb scales by identityLight, which the camera uniform carries:
/// `identityLighting`, `constLighting` and `wave` (0x4ffa60).
pub const STAGE_FLAG_TINT_LIGHT: u32 = 4096;
/// `lightingDiffuse`: the entity pass lights the stage from its light set
/// (`dynamic_model.wgsl`); the world draws it as `identityLighting`.
pub const STAGE_FLAG_LIT: u32 = 8192;
/// `tcGen environment` on bundle 0 / 1: UVs from the view vector reflected
/// about the normal in the model's frame (RB_CalcEnvironmentTexCoords,
/// CoDMP.exe 0x514d40), before the bundle's tcMods.
pub const STAGE_FLAG_BUNDLE0_ENV: u32 = 16384;
pub const STAGE_FLAG_BUNDLE1_ENV: u32 = 32768;
/// Bundle 1's texture environment, a `nextbundle`'s `blendFunc`: `add`
/// adds it, `blend` decals it; neither multiplies (shader doc 3).
pub const STAGE_FLAG_ENV1_ADD: u32 = 65536;
pub const STAGE_FLAG_ENV1_DECAL: u32 = 131072;
/// deformVertexes flap: the wave in StageParams.wave scaled by the base
/// `s` (or `t`) coordinate (CoDMP.exe 0x5136e0).
pub const STAGE_FLAG_DEFORM_FLAP_S: u32 = 1 << 18;
pub const STAGE_FLAG_DEFORM_FLAP_T: u32 = 1 << 19;
/// The sprite deforms over vertices `vcod_common::deform` encoded at load.
pub const STAGE_FLAG_AUTOSPRITE: u32 = 1 << 20;
pub const STAGE_FLAG_AUTOSPRITE2: u32 = 1 << 21;
/// The shader says `nofog`.
pub const STAGE_FLAG_NO_FOG: u32 = 1 << 22;

/// Per-stage draw parameters, one dynamic-offset slot per stage batch. WGSL
/// mirror is `StageParams` in shader.wgsl; byte offsets:
/// uv0 @0 and uv1 @24 as mat3x2 (column-major, matching [`bundle_affine`]),
/// turb01 @48, tint @64, flags @80, pad @84..96, then the two tcGen vector
/// bases padded to vec4: vec0_s @96, vec0_t @112, vec1_s @128, vec1_t @144,
/// and the sky dome's view-origin offset @160.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct StageParams {
    pub(crate) uv0: [f32; 6],
    pub(crate) uv1: [f32; 6],
    pub(crate) turb01: [f32; 4],
    pub(crate) tint: [f32; 4],
    pub(crate) flags: u32,
    pub(crate) _pad: [u32; 3],
    pub(crate) vec0_s: [f32; 4],
    pub(crate) vec0_t: [f32; 4],
    pub(crate) vec1_s: [f32; 4],
    pub(crate) vec1_t: [f32; 4],
    pub(crate) eye_off: [f32; 4],
    /// deformVertexes wave: [base, amp, phase + t * rate, spread = 1/div];
    /// the VS displaces along the vertex normal when F_DEFORM_WAVE is set.
    pub(crate) wave: [f32; 4],
}

const _: () = assert!(std::mem::size_of::<StageParams>() == 192);
pub(crate) const STAGE_PARAMS_SIZE: u64 = std::mem::size_of::<StageParams>() as u64;

const UV_AFFINE_IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// Evaluates one stage's draw parameters at time `t`: bundle affines/turb,
/// tint from rgbGen/alphaGen, and the flag bits the WGSL reads. `None` when
/// `idx` is out of range or the stage has no bundles.
pub(crate) fn stage_params(shader: &Shader, idx: usize, t: f32) -> Option<StageParams> {
    let st = shader.stages.get(idx)?;
    let b0 = st.bundles.first()?;
    let b1 = st.bundles.get(1);

    let mut flags = match &st.alpha_func {
        None => 0,
        Some(AlphaFunc::Gt0) => STAGE_FLAG_ALPHAFUNC_GT0,
        Some(AlphaFunc::Lt128) => STAGE_FLAG_ALPHAFUNC_LT128,
        Some(AlphaFunc::Ge128) => STAGE_FLAG_ALPHAFUNC_GE128,
    };
    // the lightmap and the dlight image share the lightmap's coordinates
    if b0.takes_lightmap_coords() {
        flags |= STAGE_FLAG_BUNDLE0_LIGHTMAP;
    }
    if b1.is_some_and(Bundle::takes_lightmap_coords) {
        flags |= STAGE_FLAG_BUNDLE1_LIGHTMAP;
    }
    match b1.map(|b| b.env) {
        Some(TexEnv::Add) => flags |= STAGE_FLAG_ENV1_ADD,
        Some(TexEnv::Decal) => flags |= STAGE_FLAG_ENV1_DECAL,
        _ => {}
    }

    let mut rgb = [1.0f32; 3];
    // identityLight (1 / (1 << overbrightBits)) scales the lit gens in the
    // shader; identity, const and exactVertex pass through unscaled
    // (docs/research/cod11-gamma.md, section 5).
    match &st.rgb_gen {
        RgbGen::Vertex => {
            flags |= STAGE_FLAG_VERTEX_RGB | STAGE_FLAG_VERTEX_RGB_HALF;
        }
        RgbGen::ExactVertex => flags |= STAGE_FLAG_VERTEX_RGB,
        RgbGen::Identity => {}
        RgbGen::IdentityLighting => flags |= STAGE_FLAG_TINT_LIGHT,
        RgbGen::LightingDiffuse => flags |= STAGE_FLAG_TINT_LIGHT | STAGE_FLAG_LIT,
        // the entity's shaderRGBA, which no stock entity material reads
        RgbGen::LightingPrecalc => {}
        RgbGen::Const(c) => rgb = *c,
        RgbGen::ConstLighting(c) => {
            flags |= STAGE_FLAG_TINT_LIGHT;
            rgb = *c;
        }
        RgbGen::Wave(w) => {
            flags |= STAGE_FLAG_TINT_LIGHT;
            rgb = [wave_value(w, t); 3];
        }
    }
    let alpha = match &st.alpha_gen {
        AlphaGen::Vertex => {
            flags |= STAGE_FLAG_VERTEX_ALPHA;
            1.0
        }
        AlphaGen::Const(v) => *v,
        AlphaGen::Wave(w) => wave_value(w, t),
        AlphaGen::Identity => 1.0,
    };

    let mut p = StageParams {
        uv0: bundle_affine(&b0.tcmods, t),
        uv1: b1.map_or(UV_AFFINE_IDENTITY, |b| bundle_affine(&b.tcmods, t)),
        // [amp0, now0, amp1, now1]; the VS adds amp*sin(pos_axis/1024 + now),
        // s on x+z, t on y (RB_CalcTurbulentTexCoords)
        turb01: {
            let tb0 = bundle_turb(&b0.tcmods, t);
            let tb1 = b1.map_or([0.0; 4], |b| bundle_turb(&b.tcmods, t));
            [tb0[0], tb0[1], tb1[0], tb1[1]]
        },
        // Retail writes these as colour bytes (Q3's EvalWaveFormClamped).
        tint: [rgb[0], rgb[1], rgb[2], alpha].map(|c| c.clamp(0.0, 1.0)),
        flags,
        _pad: [0; 3],
        vec0_s: [0.0; 4],
        vec0_t: [0.0; 4],
        vec1_s: [0.0; 4],
        vec1_t: [0.0; 4],
        eye_off: [0.0; 4],
        wave: [0.0; 4],
    };
    match &b0.tc_gen {
        TcGen::Base => {}
        TcGen::Vector(v) => {
            flags |= STAGE_FLAG_BUNDLE0_VECTOR;
            p.vec0_s = [v[0], v[1], v[2], 0.0];
            p.vec0_t = [v[3], v[4], v[5], 0.0];
        }
        TcGen::Environment => flags |= STAGE_FLAG_BUNDLE0_ENV,
    }
    match b1.map(|b| &b.tc_gen) {
        None | Some(TcGen::Base) => {}
        Some(TcGen::Vector(v)) => {
            flags |= STAGE_FLAG_BUNDLE1_VECTOR;
            p.vec1_s = [v[0], v[1], v[2], 0.0];
            p.vec1_t = [v[3], v[4], v[5], 0.0];
        }
        Some(TcGen::Environment) => flags |= STAGE_FLAG_BUNDLE1_ENV,
    }
    // a wave of rate 0 is one phase for every vertex (0x513570, 0x5136e0)
    let deform = |w: &Wave, spread: f32| {
        let spread = if w.freq == 0.0 { 0.0 } else { spread };
        [w.base, w.amp, w.phase + t * w.freq, spread]
    };
    if let Some((w, spread)) = &shader.deform_wave {
        flags |= STAGE_FLAG_DEFORM_WAVE;
        p.wave = deform(w, *spread);
    } else if let Some(f) = &shader.deform_flap {
        flags |= if f.along_t {
            STAGE_FLAG_DEFORM_FLAP_T
        } else {
            STAGE_FLAG_DEFORM_FLAP_S
        };
        p.wave = deform(&f.wave, f.spread);
    }
    if shader.no_fog {
        flags |= STAGE_FLAG_NO_FOG;
    }
    match shader.sprite {
        Some(Sprite::Auto) => flags |= STAGE_FLAG_AUTOSPRITE,
        Some(Sprite::Auto2) => flags |= STAGE_FLAG_AUTOSPRITE2,
        None => {}
    }
    p.flags = flags;
    Some(p)
}

/// A frame-indexed set of bind groups: one per animMap frame, or a single
/// static entry (fps 0 sticks at index 0).
struct AnimFrames {
    fps: f32,
    groups: Vec<wgpu::BindGroup>,
}

impl AnimFrames {
    /// A single, non-animating entry (fps 0 sticks at index 0).
    fn static_(group: wgpu::BindGroup) -> Self {
        Self {
            fps: 0.0,
            groups: vec![group],
        }
    }

    fn bind_group(&self, t: f32) -> &wgpu::BindGroup {
        &self.groups[anim_frame_index(self.fps, self.groups.len(), t)]
    }
}

/// One authored stage expanded over its parent batch's index range. Batches
/// of one material share the params slot per stage.
struct StageBatch {
    /// Dynamic-offset slot of this (material, stage)'s params.
    slot: u32,
    variant: StageVariant,
    two_sided: bool,
    bias: bool,
    /// group(1): bundle-0 image + lightmap page + samplers.
    mat: AnimFrames,
    /// group(2): params UBO slot + bundle-1 image.
    stage: AnimFrames,
}

/// One dlight-pass stage over a parent batch's range, drawn once per light
/// whose gathered range holds that batch.
struct DlightBatch {
    blend: DlightBlend,
    two_sided: bool,
    bias: bool,
    slot: u32,
    mat: AnimFrames,
    stage: AnimFrames,
}

/// One light's draws this frame: its slot in the light uniform, the moved
/// brush model whose camera it draws under (`None` for the world), and the
/// gathered range in `WorldGpu::dlight_index_buf`.
struct DlightRun {
    light: u32,
    moved: Option<usize>,
    draw: DrawRange,
}

/// floor(t * fps) % len, clamped to a valid frame; static or empty anims stick at 0.
fn anim_frame_index(fps: f32, len: usize, t: f32) -> usize {
    if fps <= 0.0 || len == 0 {
        return 0;
    }
    (t.max(0.0) * fps) as usize % len
}

/// Which prebuilt stage pipeline a StageBatch draws with; selected from
/// `ClassedStage` plus the stage's blend pair.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum StageVariant {
    /// Unblended: depth write on, alphafunc discard in fs_stage.
    Opaque,
    /// SrcAlpha/OneMinusSrcAlpha, no depth write.
    Blend,
    /// Blend that kept depth write (explicit depthWrite keyword).
    BlendDepthWrite,
    /// dst factor exactly GL_ONE with src GL_ONE, drawn One/One.
    AdditiveFixed,
    /// Any other additive pair, drawn SrcAlpha/One so source alpha carries.
    AdditiveFaded,
    /// Additive stages that kept depth write (explicit depthWrite keyword).
    AdditiveFixedDepthWrite,
    AdditiveFadedDepthWrite,
}
const STAGE_VARIANTS: usize = 7;

/// Pipeline variant plus the draw facts a StageBatch needs. `None` when
/// `idx` is out of range.
fn stage_draw_facts(sh: &Shader, idx: usize) -> Option<(StageVariant, bool, bool)> {
    let cs = sh.classify_stage(idx)?;
    let st = &sh.stages[idx];
    let variant = match cs.class {
        DrawClass::Opaque => StageVariant::Opaque,
        DrawClass::Blend => {
            if cs.depth_write {
                StageVariant::BlendDepthWrite
            } else {
                StageVariant::Blend
            }
        }
        DrawClass::Additive => {
            let fixed = st.blend == Some((BlendFactor::One, BlendFactor::One));
            match (fixed, cs.depth_write) {
                (true, false) => StageVariant::AdditiveFixed,
                (true, true) => StageVariant::AdditiveFixedDepthWrite,
                (false, true) => StageVariant::AdditiveFadedDepthWrite,
                (false, false) => StageVariant::AdditiveFaded,
            }
        }
    };
    Some((variant, cs.two_sided, cs.bias))
}

/// Whether the stage's params change over time and need per-frame rewrites.
pub(crate) fn stage_animated(sh: &Shader, idx: usize) -> bool {
    let Some(st) = sh.stages.get(idx) else {
        return false;
    };
    sh.deform_wave.is_some()
        || sh.deform_flap.is_some()
        || st.bundles.iter().any(|b| has_animated_tcmods(&b.tcmods))
        || matches!(st.rgb_gen, RgbGen::Wave(_))
        || matches!(st.alpha_gen, AlphaGen::Wave(_))
}

/// Group(1) and group(2) bind groups for one authored stage, shared by the
/// world stage-batch loop and the cloud dome. `lm` is the batch's
/// lightmap page (bundle-0 `$lightmap` samples it through the flag).
#[allow(clippy::too_many_arguments)]
fn stage_bind_groups(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    fs: &Pk3Fs,
    material_layout: &wgpu::BindGroupLayout,
    stage_layout: &wgpu::BindGroupLayout,
    params_buf: &wgpu::Buffer,
    params_size: u64,
    white_view: &wgpu::TextureView,
    lm: &LightmapPage,
    diffuse_sampler: &wgpu::Sampler,
    lightmap_sampler: &wgpu::Sampler,
    bundle_views: &mut BundleViews,
    st: &vcod_common::shader::Stage,
) -> (AnimFrames, AnimFrames) {
    let b0 = &st.bundles[0];
    let b1 = st.bundles.get(1);
    // a clamped bundle needs ClampToEdge; the lightmap sampler is
    // already exactly that
    let clamp = b0.clamp || b1.is_some_and(|b| b.clamp);
    let diff_sampler = if clamp {
        lightmap_sampler
    } else {
        diffuse_sampler
    };
    let mat_group = |b0_view: &wgpu::TextureView| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stage material bind group"),
            layout: material_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(b0_view),
                },
                // bundle-0 lightmaps sample here via the flag
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&lm.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(diff_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(lightmap_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&lm.edges),
                },
            ],
        })
    };
    let stage_group = |b1_view: &wgpu::TextureView| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stage draw bind group"),
            layout: stage_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: params_buf,
                        offset: 0,
                        size: wgpu::BufferSize::new(params_size),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(b1_view),
                },
            ],
        })
    };
    let mut resolve = |p: &str| upload_bundle_view(device, queue, fs, bundle_views, p);
    let mat = match (&b0.anim, &b0.image) {
        (Some(anim), _) => AnimFrames {
            fps: anim.fps,
            groups: anim.paths.iter().map(|p| mat_group(&resolve(p))).collect(),
        },
        (None, ImageRef::Path(p)) => AnimFrames::static_(mat_group(&resolve(p))),
        // $whiteimage draws white; $lightmap samples t_lightmap
        // through the flag, so bundle 0's own slot stays neutral
        (None, _) => AnimFrames::static_(mat_group(white_view)),
    };
    let stage = match b1.map(|b| (&b.anim, &b.image)) {
        Some((Some(anim), _)) => AnimFrames {
            fps: anim.fps,
            groups: anim
                .paths
                .iter()
                .map(|p| stage_group(&resolve(p)))
                .collect(),
        },
        Some((None, ImageRef::Path(p))) => AnimFrames::static_(stage_group(&resolve(p))),
        _ => AnimFrames::static_(stage_group(white_view)),
    };
    (mat, stage)
}

/// World-space fx quads. Two pipelines that differ only in blend state:
/// straight alpha, and `GL_ONE GL_ONE` for [`assets::Shaders::is_additive`]
/// materials. One draw call per contiguous same-shader run.
struct FxPass {
    pipeline: wgpu::RenderPipeline,
    additive_pipeline: wgpu::RenderPipeline,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer, // static quad pattern, MAX_FX_QUADS * 6, u32
    sampler: wgpu::Sampler,
    /// shader name -> bind group; None = failed to resolve, warned already.
    textures: HashMap<String, Option<wgpu::BindGroup>>,
    /// (shader, additive, quad count, material sort) runs in draw order,
    /// by sort first.
    runs: Vec<(String, bool, usize, f32)>,
}

/// Debug-HUD text over an embedded font atlas; drawn last, ignoring depth.
struct HudTextPass {
    pipeline: wgpu::RenderPipeline,
    pipeline_1x: wgpu::RenderPipeline,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

/// On-screen HUD (chat, killfeed, scoreboard, status). Same lazy texture
/// cache and run pattern as [`FxPass`], straight alpha only, no depth test.
struct HudPass {
    pipeline: wgpu::RenderPipeline,
    pipeline_1x: wgpu::RenderPipeline,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    sampler: wgpu::Sampler,
    texture_layout: wgpu::BindGroupLayout,
    /// image name -> bind group; `None` = failed to resolve, warned already.
    textures: HashMap<String, Option<wgpu::BindGroup>>,
    /// (texture, quad count) runs in draw order.
    runs: Vec<(String, usize)>,
    /// For the F3 overlay.
    quad_count: usize,
}

/// Live snapshot entities: xmodels uploaded once with bind pose baked in,
/// drawn with per-instance transforms streamed through one instance-step
/// vertex buffer.
struct DynamicPass {
    instance_buf: wgpu::Buffer,
    bone_layout: wgpu::BindGroupLayout,
    /// Identity block first, then each skinned instance's bone set. Sized
    /// for every instance at the cap.
    bone_buf: wgpu::Buffer,
    bone_bg: wgpu::BindGroup,
    /// Indexed by [`ModelHandle`]; never shrinks.
    models: Vec<DynModel>,
    /// `(model index, packed instance)`; position here is the slot in `instance_buf`.
    instances: Vec<(usize, InstanceRaw)>,
    /// Written to `bone_buf` in [`Renderer::render`].
    bone_mats: Vec<[f32; 16]>,
    /// Per instance, where its lights are picked.
    light_origins: Vec<glam::Vec3>,
    /// One `GpuLightSet` per distinct lighting origin this frame.
    light_buf: wgpu::Buffer,
    /// The map's light grid; `None` with no map.
    lighting: Option<StaticLighting>,
    /// This frame's fx lights, for the entity pick.
    scene_lights: Vec<SceneLight>,
    /// The cgame's own scene lights this frame (`constantLight`,
    /// `projectileDLight`), queued after the fx lights.
    entity_scene_lights: Vec<SceneLight>,
    materials: EntityMaterials,
}

/// A dynamic model and, per drawn surface, its material.
struct DynModel {
    vm: VmModel,
    skins: Vec<Rc<EntSkin>>,
    /// Some surface takes the light grid; an unlit model skips the pick.
    lit: bool,
}

/// Everything built from one map: buffers, material bind groups, batches,
/// visibility. Dropped whole on a map change.
struct WorldGpu {
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    bind_groups: Vec<wgpu::BindGroup>,
    /// Merged CPU indices (world then props); `index_buf` holds the gathered visible subset.
    cpu_indices: Vec<u32>,
    soup_ranges: Vec<Option<IndexRange>>,
    prop_ranges: Vec<(u32, IndexRange)>,
    prop_bounds: Vec<(glam::Vec3, glam::Vec3)>,
    /// Static per-batch draw descriptors; `draws` is rebuilt each frame from them.
    batch_draws: Vec<DrawCall>,
    draws: Vec<DrawRange>,
    /// Authored stages expanded from their parent batches.
    stage_batches: Vec<StageBatch>,
    /// Parent batch id -> indices into `stage_batches`.
    stages_of_batch: Vec<Vec<u32>>,
    /// Animated stages: (shader clone, stage idx, params slot); rewritten each frame.
    animated: Vec<(Shader, usize, u32)>,
    /// Cloud-dome stages: like `animated` but every entry also gets the view
    /// origin stamped into its params each frame.
    sky_stages: Vec<(Shader, usize, u32)>,
    /// Per batch, its material sort and its place in the sorted shader
    /// list (`sorted_places`), the draw list's key.
    batch_keys: Vec<(f32, u32)>,
    /// This frame's emission-ordered draw list.
    frame_draws: Vec<FrameDraw>,
    /// Stage draws emitted last frame, for the F3 overlay.
    stage_draws_last: usize,
    /// Per-stage params UBO (one dynamic-offset slot per (material, stage)).
    /// Group(2) bind groups are per stage batch: bundle 1's image varies.
    stage_params_buf: wgpu::Buffer,
    /// StageParams size rounded up to `min_uniform_buffer_offset_alignment`.
    stage_params_stride: u64,
    vis: WorldVis,
    gathered: Vec<u32>,
    gather_scratch: Vec<Vec<IndexRange>>,
    /// The frozen set and the prop flags it was computed with, so `Locked`
    /// holds props still too.
    locked: Option<(Visible, Vec<bool>)>,
    /// Last frame's mode, so `Locked` and `Off` skip re-uploading indices
    /// that cannot have changed.
    last_cull: Option<CullMode>,
    /// The lump-27 model each soup belongs to, 0 for the world's own.
    soup_model: Vec<u32>,
    /// Per model, whether its soups draw with the world this frame. A
    /// submodel draws only through the entity that carries it
    /// ([`Renderer::set_submodels`]); model 0 always draws.
    static_models: Vec<bool>,
    /// `static_models` changed since the index buffer was last gathered.
    static_dirty: bool,
    /// Per model, its soups, `bsp.models[m]`'s `first_soup..+num_soups`.
    model_soups: Vec<std::ops::Range<usize>>,
    /// The brush models drawn off their spawn pose this frame, with that
    /// pose: their soups gathered after the world's, under their own camera.
    moved: Vec<(usize, glam::Mat4)>,
    /// Per entry of `moved`, its gathered draws and their emission order.
    moved_draws: Vec<(Vec<DrawRange>, Vec<FrameDraw>)>,
    /// Per vertex, the tangent s axis the dlight projection reads
    /// (`dlight::soup_s_axes`; zero for props).
    s_axis_buf: wgpu::Buffer,
    /// Per soup, its spawn-pose bounds, which a light must reach.
    soup_bounds: Vec<(glam::Vec3, glam::Vec3)>,
    /// The dlight pass's stages and, per parent batch, its entries there.
    dlight_batches: Vec<DlightBatch>,
    dlights_of_batch: Vec<Vec<u32>>,
    /// This frame's per-light ranges, indices into `dlight_index_buf`.
    dlight_runs: Vec<DlightRun>,
    dlight_indices: Vec<u32>,
    dlight_index_buf: wgpu::Buffer,
    /// `vc_shadows`' static casters, built the first frame it is on.
    shadow_casters: Option<shadows::Casters>,
}

/// One cloud-dome draw: a stage of the active sky block over the whole dome
/// mesh, through the shared stage pipelines and params slots.
struct DomeStage {
    variant: StageVariant,
    two_sided: bool,
    slot: u32,
    mat: AnimFrames,
    stage: AnimFrames,
}

struct DomeLayer {
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: u32,
    stages: Vec<DomeStage>,
}

/// The farbox cube. Absent from [`SkyRender`] when the env art is missing:
/// retail gates the whole box on `outerbox[0]` having loaded
/// (RB_StageIteratorSky tr_sky.c :994) and draws nothing otherwise.
struct SkyBox {
    pipeline: wgpu::RenderPipeline,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    /// View origin per frame; the cube never rotates (sky.wgsl).
    eye_buf: wgpu::Buffer,
    eye_bg: wgpu::BindGroup,
    /// Per-axis bind groups in [`sky::AXIS_SUFFIX`] order.
    side_bgs: Vec<wgpu::BindGroup>,
    /// Index ranges per axis into `index_buf`.
    sides: [(u32, u32); 6],
}

/// The map's active sky: the farbox cube plus optional cloud dome. Built at
/// load, drawn FIRST every frame with depth-always / write-off / cull-off so
/// the world overdraws it everywhere geometry exists (retail draws at depth
/// range 1.0 before everything; same net effect).
struct SkyRender {
    /// Chosen block name, for the F3 overlay and the load log.
    name: String,
    farbox: Option<SkyBox>,
    dome: Option<DomeLayer>,
    /// `sunfile` glow sprite; needs a resolvable `.sun` sprite image and a
    /// lump-30 sun light.
    sun: Option<SunSprite>,
    /// The `.sun` file's flare and screen washes (`sun_flare.rs`).
    flare: Option<SunFlareGpu>,
}

/// The `sunfile` disc: an additive billboard along `bsp::sun_direction`
/// at farbox distance, rebuilt per frame around the view origin.
struct SunSprite {
    pipeline: wgpu::RenderPipeline,
    vertex_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    dir: glam::Vec3,
    half_extent: f32,
}

/// Encodes the soups of every sprite-deformed map material for `vs_stage`
/// (`vcod_common::deform`), their vertices in `vertices` and their index
/// runs in `indices` in place. Autosprite rewrites a soup's indices as
/// stamps, so a soup whose index count is not six per four vertices keeps
/// its geometry and is warned.
fn encode_world_sprites(
    bsp: &Bsp,
    lib: &ShaderLib,
    soup_ranges: &[Option<IndexRange>],
    vertices: &mut [DrawVert],
    indices: &mut [u32],
) {
    for (soup, range) in bsp.soups.iter().zip(soup_ranges) {
        let Some(r) = range else { continue };
        let name = &bsp.materials[soup.material as usize].name;
        let Some(sprite) = lib.get(name).and_then(|sh| sh.sprite) else {
            continue;
        };
        let first = soup.first_vertex as usize;
        let verts = &mut vertices[first..first + soup.vertex_count as usize];
        let idx = &mut indices[r.first as usize..(r.first + r.count) as usize];
        match sprite {
            Sprite::Auto if verts.len().is_multiple_of(4) && idx.len() == verts.len() / 4 * 6 => {
                idx.copy_from_slice(&deform::encode_autosprite(verts, soup.first_vertex));
            }
            Sprite::Auto => log::warn!("Autosprite shader {name} had odd vertex or index count"),
            Sprite::Auto2 => deform::encode_autosprite2(verts, idx, soup.first_vertex),
        }
    }
}

/// Builds the farbox for the block's env name. Missing non-rt sides draw
/// tr.defaultImage flat white (tr_shader.c:1170), warned per load.
#[allow(clippy::too_many_arguments)]
fn build_sky_box(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    fs: &Pk3Fs,
    camera_layout: &wgpu::BindGroupLayout,
    bundle_views: &mut BundleViews,
    white_view: &wgpu::TextureView,
    env: &str,
    sh_name: &str,
) -> SkyBox {
    let size = sky::box_size(Z_FAR, Z_NEAR);
    let mut verts: Vec<sky::SkyBoxVert> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut sides = [(0u32, 0u32); 6];
    for (axis, side_slot) in sides.iter_mut().enumerate() {
        let side = sky::box_side(axis, size);
        let first = indices.len() as u32;
        indices.extend(side.indices.iter().map(|&i| i + verts.len() as u32));
        *side_slot = (first, indices.len() as u32);
        verts.extend(side.verts);
    }
    let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("sky box vertices"),
        contents: bytemuck::cast_slice(&verts),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("sky box indices"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    // ClampToEdge per ParseSkyParms' GL_CLAMP; linear filtering + mips like
    // every other colour texture.
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("sky box sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("sky side layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let mut side_bgs = Vec::with_capacity(6);
    for suffix in sky::AXIS_SUFFIX {
        let path = format!("{env}_{suffix}");
        let found = assets::resolve_bundle_image(fs, &path).is_some();
        let view = if found {
            upload_bundle_view(device, queue, fs, bundle_views, &path)
        } else {
            // retail binds tr.defaultImage for a side the paks lack
            log::warn!("sky {sh_name}: env image {path} not found, drawing white");
            white_view.clone()
        };
        let label = format!("sky {sh_name}_{suffix}");
        side_bgs.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&label),
            layout: &tex_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        }));
    }

    let eye_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sky eye"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let eye_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("sky eye layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let eye_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sky eye bind group"),
        layout: &eye_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: eye_buf.as_entire_binding(),
        }],
    });

    let module = device.create_shader_module(wgpu::include_wgsl!("sky.wgsl"));
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sky pipeline layout"),
        bind_group_layouts: &[Some(camera_layout), Some(&tex_layout), Some(&eye_layout)],
        immediate_size: 0,
    });
    let box_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sky box pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<sky::SkyBoxVert>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2],
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            // interior faces of a closed box: no culling to second-guess
            front_face: wgpu::FrontFace::Cw,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            // drawn first; the world must overwrite it without a fight
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: MSAA_SAMPLES,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    });

    SkyBox {
        pipeline: box_pipeline,
        vertex_buf,
        index_buf,
        eye_buf,
        eye_bg,
        side_bgs,
        sides,
    }
}

/// The sun disc's half-width per unit of `r_sunsprite_size` at unit
/// distance (CoDMP.exe 0x569024, read by 0x4e7980).
const SUN_SPRITE_SCALE: f32 = 0.001_311_092_9;

/// Builds the sunfile sprite for a parsed `.sun` file along the sun
/// direction. `None` when the sprite image does not resolve.
#[allow(clippy::too_many_arguments)]
fn build_sun_sprite(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    fs: &Pk3Fs,
    camera_layout: &wgpu::BindGroupLayout,
    bundle_views: &mut BundleViews,
    sprite: &str,
    dir: glam::Vec3,
    size: f32,
) -> Option<SunSprite> {
    if assets::resolve_bundle_image(fs, sprite).is_none() {
        log::warn!("sunfile: sprite image {sprite} not found, drawing no sun");
        return None;
    }
    let view = upload_bundle_view(device, queue, fs, bundle_views, sprite);
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("sun sprite sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("sun sprite layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sun sprite bind group"),
        layout: &tex_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });

    let module = device.create_shader_module(wgpu::include_wgsl!("sky.wgsl"));
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sun sprite pipeline layout"),
        bind_group_layouts: &[Some(camera_layout), Some(&tex_layout)],
        immediate_size: 0,
    });
    // Additive like the corpus glow blocks (`blendFunc GL_ONE GL_ONE` with
    // the texture's alpha folded into rgb by fs_sun); depth-always so the
    // disc draws behind everything.
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sun sprite pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_sun"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<sky::SkyBoxVert>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2],
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_sun"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent::REPLACE,
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Cw,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: MSAA_SAMPLES,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    });

    let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sun sprite vertices"),
        size: 6 * std::mem::size_of::<sky::SkyBoxVert>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let dir = dir.normalize_or_zero();
    // 0x4e7980 builds the quad at unit distance with a half-width of
    // `r_sunsprite_size * 0.0013110929`; scaled out to the farbox
    let half_extent = size * SUN_SPRITE_SCALE * sky::box_size(Z_FAR, Z_NEAR);
    Some(SunSprite {
        pipeline,
        vertex_buf,
        bind_group,
        dir,
        half_extent,
    })
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    msaa_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    prop_pipeline: wgpu::RenderPipeline,
    layer_pipeline: wgpu::RenderPipeline,
    overlay_pipeline: wgpu::RenderPipeline,
    /// `[variant][two_sided][bias]`; built at startup so shader.wgsl validates
    /// even mapless.
    stage_pipelines: [[[wgpu::RenderPipeline; 2]; 2]; STAGE_VARIANTS],
    dlight: dlight::DlightPass,
    camera_layout: wgpu::BindGroupLayout,
    camera_buf: wgpu::Buffer,
    camera_bg: wgpu::BindGroup,
    /// One camera per moved brush model drawn this frame, the shared camera
    /// with that model's pose in `model`; grown on demand, never shrunk.
    moved_cams: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
    fx_lights_buf: wgpu::Buffer,
    /// Device-stage state `load_world` builds map bind groups from.
    material_layout: wgpu::BindGroupLayout,
    stage_layout: wgpu::BindGroupLayout,
    diffuse_sampler: wgpu::Sampler,
    lightmap_sampler: wgpu::Sampler,
    white_view: wgpu::TextureView,
    /// Bound for a vertex-lit surface: `white_view` in one block.
    white_lightmap: LightmapPage,
    /// `vc_bicubic`: lightmaps filter B-spline bicubic; rides `time_pad.z`.
    bicubic: bool,
    world: Option<WorldGpu>,
    vis_counts: VisCounts,
    /// Stage draws emitted last frame; stages dropped at the current map's
    /// load; unique ShaderLib warnings. The F3 `shader:` line.
    shader_dropped: usize,
    lib_warn_count: usize,
    vm_pass: VmPass,
    dynamic: DynamicPass,
    fx: FxPass,
    /// Impact marks (`crate::marks`), drawn through the stage pipelines.
    marks: mark_pass::MarkPass,
    /// The map's `corona` entities, drawn as sprites through `marks`.
    coronas: Vec<bsp::Corona>,
    /// `r_flares` is non-zero.
    flares: bool,
    hud: HudTextPass,
    hud_pass: HudPass,
    gamma: GammaPass,
    bloom: crate::bloom::Bloom,
    /// `vc_bloom` and `vc_bloomThreshold`; no bloom passes while the
    /// intensity is 0.
    bloom_intensity: f32,
    bloom_threshold: f32,
    ssao: crate::ssao::SsaoPass,
    /// `vc_sunShafts`, added into the resolved scene before bloom.
    shafts: crate::sun_shafts::SunShafts,
    /// Retail's `identityLight` for the current overbright bits
    /// (`gamma::identity_light`): what lit colours are scaled by.
    identity_light: f32,
    /// The texture gamma table images load through (`gamma::bake_image`),
    /// set before a world load while `r_ignorehwgamma` is 1.
    image_gamma: Option<[u8; 256]>,
    /// Kept past map load so later inline submodels resolve `textures/...`
    /// names the same way the world did.
    shaders: assets::Shaders,
    /// Parsed shader scripts; Task 7 classifies map materials against it.
    shader_lib: ShaderLib,
    /// The map's active sky (farbox + optional cloud dome); `None` without a
    /// matching sky block.
    sky: Option<SkyRender>,
    /// Global fog from configstring 12; the F3 `fog:` line.
    fog: FogState,
    hud_quad_cap_warned: bool,
    /// `vc_shadows`; built when it turns on, dropped when it turns off.
    shadows: Option<shadows::ShadowPass>,
}

impl Renderer {
    pub fn new(window: Arc<winit::window::Window>, fs: &Pk3Fs) -> Result<Renderer> {
        let size = window.inner_size();
        let (width, height) = (size.width.max(1), size.height.max(1));

        // `WGPU_BACKEND=vulkan|gl|dx12|metal` narrows the backend choice.
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance
            .create_surface(window)
            .context("failed to create a surface for the window")?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .context("no GPU adapter found; is a Vulkan (or DX12/Metal) driver installed?")?;

        if !adapter
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
        {
            let info = adapter.get_info();
            bail!(
                "GPU '{}' ({:?}) does not support BC texture compression, \
                 which is required to load CoD's DDS textures",
                info.name,
                info.backend
            );
        }

        // Timestamps feed the F3 `ssao` line's GPU times where the adapter has them.
        let timestamps = adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("vcod device"),
            required_features: wgpu::Features::TEXTURE_COMPRESSION_BC
                | (adapter.features() & wgpu::Features::TIMESTAMP_QUERY),
            // One stage-params UBO must stay reachable through dynamic offsets,
            // past the default 64 KiB uniform binding window.
            required_limits: wgpu::Limits {
                max_uniform_buffer_binding_size: adapter.limits().max_uniform_buffer_binding_size,
                ..Default::default()
            },
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .context("failed to create the wgpu device")?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let msaa_view = create_msaa_view(&device, SCENE_FORMAT, width, height);
        let depth_view = create_depth_view(&device, width, height);
        let ssao =
            crate::ssao::SsaoPass::new(&device, &queue, &depth_view, width, height, timestamps);

        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Fx lights. The fx pass shares this layout without declaring
                // binding 1 in fx.wgsl, which wgpu allows. Props light
                // per vertex from them.
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material layout"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                sampler_entry(2),
                sampler_entry(3),
                // the lightmap page's block runs (lightmap_blocks), read by
                // vc_bicubic's taps
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        // Group 2 of the stage pipelines: the per-stage params slot plus
        // bundle 1's image (white is bound when bundle 1 has none).
        let stage_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("stage params layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(STAGE_PARAMS_SIZE),
                    },
                    count: None,
                },
                texture_entry(1),
            ],
        });

        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera uniform"),
            // proj + time + eye/fog tail + view forward + model, matching
            // Camera in shader.wgsl; the other modules read a prefix
            size: CAMERA_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let fx_lights_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fx lights uniform"),
            size: std::mem::size_of::<FxLightsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera bind group"),
            layout: &camera_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: fx_lights_buf.as_entire_binding(),
                },
            ],
        });

        let diffuse_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("diffuse sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 16,
            ..Default::default()
        });
        let lightmap_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("lightmap sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let white_view = upload_rgba(
            &device,
            &queue,
            "white lightmap",
            1,
            1,
            &[255, 255, 255, 255],
        );

        let white_lightmap = LightmapPage {
            view: white_view.clone(),
            edges: lightmap_blocks::upload_edges(
                &device,
                &queue,
                "white lightmap blocks",
                1,
                &[[0; 4]],
            ),
        };

        let shaders = assets::load_shaders(fs);

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline layout"),
            bind_group_layouts: &[Some(&camera_layout), Some(&material_layout)],
            immediate_size: 0,
        });
        let stage_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("stage pipeline layout"),
                bind_group_layouts: &[
                    Some(&camera_layout),
                    Some(&material_layout),
                    Some(&stage_layout),
                ],
                immediate_size: 0,
            });
        let make_pipeline = |label: &str,
                             layout: &wgpu::PipelineLayout,
                             vs_entry: &str,
                             bias: wgpu::DepthBiasState,
                             fs_entry: &str,
                             alpha_to_coverage: bool,
                             blend: Option<wgpu::BlendState>,
                             depth_write: bool,
                             cull: Option<wgpu::Face>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs_entry),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: 44,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x3, // pos
                            1 => Float32x2, // uv
                            2 => Float32x2, // lm_uv
                            3 => Float32x3, // normal
                            4 => Unorm8x4,  // color
                        ],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs_entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: SCENE_FORMAT,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    // Map triangles are wound clockwise seen from the front.
                    // Cull back faces like the engine; coplanar interior and
                    // exterior faces z-fight otherwise.
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: cull,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias,
                }),
                multisample: wgpu::MultisampleState {
                    count: MSAA_SAMPLES,
                    mask: !0,
                    alpha_to_coverage_enabled: alpha_to_coverage,
                },
                multiview_mask: None,
                cache: None,
            })
        };
        let bias = wgpu::DepthBiasState {
            constant: -2,
            slope_scale: -1.0,
            clamp: 0.0,
        };
        let cull_back = Some(wgpu::Face::Back);
        let pipeline = make_pipeline(
            "map pipeline",
            &pipeline_layout,
            "vs_main",
            Default::default(),
            "fs_main",
            false,
            None,
            true,
            cull_back,
        );
        // Props take the opaque alpha-test path: `*_masked` skins are cutouts
        // on real geometry, not coplanar decals, so no overlay depth bias.
        // Unculled like the viewmodel (xmodel winding), and foliage cross
        // planes need both sides anyway.
        let prop_pipeline = make_pipeline(
            "prop pipeline",
            &pipeline_layout,
            "vs_prop",
            Default::default(),
            "fs_prop",
            false,
            None,
            true,
            None,
        );
        // Blend layers alpha-blend over the base ground by vertex alpha. No
        // depth write: the base wrote depth, and stacked layers must all draw.
        let layer_pipeline = make_pipeline(
            "layer pipeline",
            &pipeline_layout,
            "vs_main",
            bias,
            "fs_layer",
            false,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
            cull_back,
        );
        // Overlays are pulled toward the viewer like the engine's polygon
        // offset (Q3 defaults -1/-2); their alpha drives MSAA coverage.
        let overlay_pipeline = make_pipeline(
            "overlay pipeline",
            &pipeline_layout,
            "vs_main",
            bias,
            "fs_overlay",
            true,
            None,
            true,
            cull_back,
        );
        // One twin pair per variant, [back-culled, two-sided], picked per
        // StageBatch by ClassedStage in Task 7.
        let one_one = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let srcalpha_one = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let variant_states = [
            (StageVariant::Opaque, None, true),
            (
                StageVariant::Blend,
                Some(wgpu::BlendState::ALPHA_BLENDING),
                false,
            ),
            (
                StageVariant::BlendDepthWrite,
                Some(wgpu::BlendState::ALPHA_BLENDING),
                true,
            ),
            (StageVariant::AdditiveFixed, Some(one_one), false),
            (StageVariant::AdditiveFaded, Some(srcalpha_one), false),
            // explicit depthWrite on additive stages
            (StageVariant::AdditiveFixedDepthWrite, Some(one_one), true),
            (
                StageVariant::AdditiveFadedDepthWrite,
                Some(srcalpha_one),
                true,
            ),
        ];
        let stage_pipelines: [[[wgpu::RenderPipeline; 2]; 2]; STAGE_VARIANTS] =
            std::array::from_fn(|v| {
                let (variant, blend, depth_write) = variant_states[v];
                [false, true].map(|two_sided| {
                    [false, true].map(|want_bias| {
                        make_pipeline(
                            &format!("stage {variant:?} twosided={two_sided} bias={want_bias}"),
                            &stage_pipeline_layout,
                            "vs_stage",
                            if want_bias { bias } else { Default::default() },
                            "fs_stage",
                            false,
                            blend,
                            depth_write,
                            if two_sided { None } else { cull_back },
                        )
                    })
                })
            });
        let dlight = dlight::DlightPass::new(
            &device,
            &queue,
            &shader,
            [&camera_layout, &material_layout, &stage_layout],
            &lightmap_sampler,
            bias,
        );
        let vm_pass = create_vm_pass(&device);
        let dynamic =
            create_dynamic_pass(&device, SCENE_FORMAT, &camera_layout, &vm_pass, &white_view);
        let fx = create_fx_pass(&device, SCENE_FORMAT, &camera_layout, &vm_pass.skin_layout);
        let hud = create_hud_text_pass(&device, &queue, SCENE_FORMAT);
        let hud_pass = create_hud_pass(&device, SCENE_FORMAT);
        let gamma = GammaPass::new(&device, format, width, height);
        let bloom = crate::bloom::Bloom::new(&device, gamma.scene_view(), width, height);
        let shafts = crate::sun_shafts::SunShafts::new(
            &device,
            gamma.scene_view(),
            &depth_view,
            width,
            height,
        );

        let lib = ShaderLib::load(fs);
        let marks = mark_pass::MarkPass::new(&device);
        Ok(Renderer {
            surface,
            device,
            queue,
            config,
            msaa_view,
            depth_view,
            pipeline,
            prop_pipeline,
            layer_pipeline,
            overlay_pipeline,
            stage_pipelines,
            dlight,
            camera_layout,
            camera_buf,
            camera_bg,
            moved_cams: Vec::new(),
            fx_lights_buf,
            material_layout,
            stage_layout,
            diffuse_sampler,
            lightmap_sampler,
            white_view,
            white_lightmap,
            bicubic: false,
            world: None,
            vis_counts: VisCounts::default(),
            shader_dropped: 0,
            lib_warn_count: lib.warn_count(),
            vm_pass,
            dynamic,
            fx,
            marks,
            coronas: Vec::new(),
            flares: true,
            hud,
            hud_pass,
            gamma,
            bloom,
            bloom_intensity: 0.0,
            bloom_threshold: 0.0,
            ssao,
            shafts,
            identity_light: 0.5,
            image_gamma: None,
            hud_quad_cap_warned: false,
            shadows: None,
            shaders,
            shader_lib: lib,
            sky: None,
            fog: FogState::default(),
        })
    }

    /// Builds the map's GPU state; a previous map is dropped first.
    pub fn load_world(&mut self, bsp: &Bsp, fs: &Pk3Fs) -> Result<()> {
        self.unload_world();
        let device = &self.device;
        let queue = &self.queue;
        let material_layout = &self.material_layout;
        let diffuse_sampler = &self.diffuse_sampler;
        let lightmap_sampler = &self.lightmap_sampler;
        let white_view = &self.white_view;
        let white_lightmap = &self.white_lightmap;

        // Per-material draw decision: authored blocks classify from their
        // parsed surface bits, implicit names from the gameplay prefixes.
        let kinds: Vec<MaterialKind> = bsp
            .materials
            .iter()
            .map(|m| match self.shader_lib.get(&m.name) {
                Some(sh) => mesh::material_kind(
                    &m.name,
                    true,
                    sh.surface.nodraw,
                    sh.surface.sky || sh.sky.is_some(),
                ),
                None => mesh::material_kind(&m.name, false, false, false),
            })
            .collect();

        // Active sky: authored blocks named by this map's materials, ranked
        // by soup reference count, ties alphabetical.
        let norm = |s: &str| s.to_lowercase().replace('\\', "/");
        let mut material_idx: HashMap<String, u16> = HashMap::new();
        for (i, m) in bsp.materials.iter().enumerate() {
            material_idx.entry(norm(&m.name)).or_insert(i as u16);
        }
        let mut soup_refs: HashMap<u16, u32> = HashMap::new();
        for s in &bsp.soups {
            if kinds[s.material as usize] == MaterialKind::Sky {
                *soup_refs.entry(s.material).or_default() += 1;
            }
        }
        let candidates: Vec<(String, u16)> = self
            .shader_lib
            .sky_blocks()
            .filter_map(|sh| material_idx.get(&sh.name).map(|&mi| (sh.name.clone(), mi)))
            .collect();
        let active_sky = {
            let with_refs: Vec<(String, u32)> = candidates
                .iter()
                .map(|(n, mi)| (n.clone(), soup_refs.get(mi).copied().unwrap_or(0)))
                .collect();
            sky::pick_sky(&with_refs)
                .and_then(|picked| candidates.iter().find(|(n, _)| *n == picked))
                .cloned()
        };

        // Props (misc_model entities) are baked to world space on the CPU in
        // the same vertex format and extend the map's buffers.
        let (mut indices, batches, soup_ranges) = mesh::build_batches(bsp, &kinds);
        let mut soup_model = vec![0u32; bsp.soups.len()];
        for (mi, m) in bsp.models.iter().enumerate().skip(1) {
            let first = m.first_soup as usize;
            for s in soup_model.iter_mut().skip(first).take(m.num_soups as usize) {
                *s = mi as u32;
            }
        }
        if batches.is_empty() {
            bail!("map has no drawable surfaces");
        }
        let mut lighting = StaticLighting::new(bsp);
        let lib = &self.shader_lib;
        let props = props::build(fs, bsp, &mut lighting, &|skin| {
            props::skin_script(skin).and_then(|n| lib.get(&n)?.sprite)
        });
        self.dynamic.lighting = Some(lighting);
        let prop_first_index = indices.len() as u32;
        let prop_first_vertex = bsp.verts.len() as u32;
        indices.extend(props.indices.iter().map(|i| i + prop_first_vertex));
        // prop batches follow the world's in `batch_draws`, prop indices follow
        // the world's in the merged buffer
        let prop_ranges: Vec<(u32, IndexRange)> = props
            .ranges
            .iter()
            .map(|&(p, r)| {
                (
                    p,
                    IndexRange {
                        batch: r.batch + batches.len() as u32,
                        first: r.first + prop_first_index,
                        count: r.count,
                    },
                )
            })
            .collect();
        let mut vertices = bsp.verts.clone();
        encode_world_sprites(
            bsp,
            &self.shader_lib,
            &soup_ranges,
            &mut vertices,
            &mut indices,
        );
        vertices.extend_from_slice(&props.verts);

        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("map vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut s_axes = dlight::soup_s_axes(&bsp.soups, &bsp.verts, &bsp.indices);
        s_axes.resize(vertices.len(), [0.0; 3]);
        let s_axis_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("map tangent s axes"),
            contents: bytemuck::cast_slice(&s_axes),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("map indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });

        let fallback_px = fallback_pixels();
        let shaders = assets::load_shaders(fs);
        let mut material_views: HashMap<u16, wgpu::TextureView> = HashMap::new();
        let (mut loaded, mut fallbacks) = (0usize, 0usize);
        for batch in &batches {
            if material_views.contains_key(&batch.material) {
                continue;
            }
            let name = &bsp.materials[batch.material as usize].name;
            let mut img = assets::load_material_image(fs, &shaders, name);
            if is_fallback(&img, &fallback_px) {
                fallbacks += 1;
            } else {
                loaded += 1;
            }
            crate::gamma::bake_image(&mut img, self.image_gamma.as_ref());
            material_views.insert(batch.material, upload_image(device, queue, name, &img));
        }

        // One RGBA lightmap page per BSP page, plus a white 1x1 for unlit soups.
        let block_edges = lightmap_blocks::edges(bsp);
        let lightmap_views: Vec<LightmapPage> = bsp
            .lightmaps
            .iter()
            .zip(&block_edges)
            .enumerate()
            .map(|(i, (page, edges))| {
                let mut rgba = Vec::with_capacity(page.len() / 3 * 4);
                for px in page.as_chunks::<3>().0 {
                    rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
                }
                let size = bsp::LIGHTMAP_SIZE as u32;
                LightmapPage {
                    view: upload_rgba(device, queue, &format!("lightmap {i}"), size, size, &rgba),
                    edges: lightmap_blocks::upload_edges(
                        device,
                        queue,
                        &format!("lightmap {i} blocks"),
                        size,
                        edges,
                    ),
                }
            })
            .collect();

        self.marks.load(
            crate::marks::MarkWorld::build(bsp, |m| kinds[m as usize] == MaterialKind::Draw),
            lightmap_views.clone(),
        );
        self.coronas = bsp::coronas(&bsp.entities);
        if !self.coronas.is_empty() {
            log::info!("coronas: {}", self.coronas.len());
        }

        let mut bind_groups: Vec<wgpu::BindGroup> = Vec::new();
        let mut cache: HashMap<(u16, u16), usize> = HashMap::new();
        let mut batch_draws = Vec::with_capacity(batches.len());
        for Batch {
            material, lightmap, ..
        } in &batches
        {
            let idx = *cache.entry((*material, *lightmap)).or_insert_with(|| {
                let lm = match *lightmap {
                    bsp::NO_LIGHTMAP => white_lightmap,
                    i => lightmap_views.get(i as usize).unwrap_or(white_lightmap),
                };
                bind_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("material bind group"),
                    layout: material_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&material_views[material]),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&lm.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(diffuse_sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Sampler(lightmap_sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::TextureView(&lm.edges),
                        },
                    ],
                }));
                bind_groups.len() - 1
            });
            let mat = &bsp.materials[*material as usize];
            let pass = if mesh::is_overlay(mat) {
                Pass::Overlay
            } else if shaders.uses_polygon_offset(&mat.name) {
                Pass::Layer
            } else {
                Pass::Opaque
            };
            batch_draws.push(DrawCall {
                bind_group: idx,
                pass,
            });
        }

        // Prop materials are skin filenames, not `textures/...` names, so
        // `load_skin_image` resolves them.
        let mut skin_cache: HashMap<&str, usize> = HashMap::new();
        for batch in &props.batches {
            let idx = *skin_cache.entry(batch.skin.as_str()).or_insert_with(|| {
                let mut img = assets::load_skin_image(fs, &batch.skin);
                crate::gamma::bake_image(&mut img, self.image_gamma.as_ref());
                let view = upload_image(device, queue, &batch.skin, &img);
                bind_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("prop skin bind group"),
                    layout: material_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(white_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(diffuse_sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Sampler(lightmap_sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::TextureView(&white_lightmap.edges),
                        },
                    ],
                }));
                bind_groups.len() - 1
            });
            batch_draws.push(DrawCall {
                bind_group: idx,
                pass: Pass::Prop,
            });
        }

        let count = |p: Pass| batch_draws.iter().filter(|d| d.pass == p).count();
        println!(
            "{} batches ({} opaque, {} prop, {} layer, {} overlay), {} draw indices, {} vertices",
            batch_draws.len(),
            count(Pass::Opaque),
            count(Pass::Prop),
            count(Pass::Layer),
            count(Pass::Overlay),
            indices.len(),
            vertices.len()
        );
        println!(
            "{} textures loaded, {} missing, {} lightmap pages",
            loaded,
            fallbacks,
            lightmap_views.len()
        );

        // One dynamic-offset slot per authored stage of every map material
        // that resolves in the shader lib; batches of one material share the
        // slot per stage. Implicit materials use no slots.
        let mut stage_slots: Vec<(&Shader, usize)> = Vec::new();
        let mut slot_of: HashMap<(u16, usize), u32> = HashMap::new();
        for (mi, mat) in bsp.materials.iter().enumerate() {
            if let Some(sh) = self.shader_lib.get(&mat.name) {
                for idx in 0..sh.stages.len() {
                    slot_of.insert((mi as u16, idx), stage_slots.len() as u32);
                    stage_slots.push((sh, idx));
                }
            }
        }
        // A prop skin with a `skins/<name>` block draws through its stages
        // like a map material, slotted after them (shader doc 8).
        let prop_scripts: Vec<Option<(&Shader, u32)>> = props
            .batches
            .iter()
            .map(|b| {
                let sh = props::skin_script(&b.skin).and_then(|n| self.shader_lib.get(&n))?;
                let first = stage_slots.len() as u32;
                stage_slots.extend((0..sh.stages.len()).map(|idx| (sh, idx)));
                (!sh.stages.is_empty()).then_some((sh, first))
            })
            .collect();
        // The dlight pass's stages (shader doc 3), slotted after the base
        // stages: per scripted material its own as single-stage blocks that
        // keep the material's deform and culling, then the implicit
        // surfaces' copy (texture times the dlight image, rgbGen identity),
        // the same alpha-tested for masked overlays, and `dlightshader`.
        let blob_stage = self
            .shader_lib
            .get("dlightshader")
            .and_then(|sh| sh.stages.first().cloned());
        let mut dl_shaders: Vec<Shader> = Vec::new();
        let mut dl_of_material: HashMap<u16, Vec<u32>> = HashMap::new();
        let single = |sh: Shader, st: Stage, out: &mut Vec<Shader>| {
            out.push(Shader {
                stages: vec![st],
                per_light: Vec::new(),
                ..sh
            });
            out.len() as u32 - 1
        };
        for (mi, mat) in bsp.materials.iter().enumerate() {
            let Some(sh) = self.shader_lib.get(&mat.name) else {
                continue;
            };
            if sh.stages.is_empty() {
                continue;
            }
            let stages = match sh.dlight_draw() {
                None => continue,
                Some(DlightDraw::Stages(st)) => st,
                Some(DlightDraw::Blob) => blob_stage.iter().cloned().collect(),
            };
            for st in stages {
                let i = single(sh.clone(), st, &mut dl_shaders);
                dl_of_material.entry(mi as u16).or_default().push(i);
            }
        }
        let implicit_copy = |alpha_func| Stage {
            bundles: [ImageRef::White, ImageRef::Path(DLIGHT_IMAGE.into())]
                .map(|image| Bundle {
                    image,
                    anim: None,
                    clamp: false,
                    tcmods: Vec::new(),
                    tc_gen: TcGen::Base,
                    env: TexEnv::Modulate,
                })
                .into(),
            blend: Some((BlendFactor::One, BlendFactor::One)),
            depth_write: None,
            alpha_func,
            depth_func: DepthFunc::LessEqual,
            rgb_gen: RgbGen::Identity,
            alpha_gen: AlphaGen::Identity,
        };
        let dl_implicit = [
            Some(implicit_copy(None)),
            Some(implicit_copy(Some(AlphaFunc::Ge128))),
            blob_stage.clone(),
        ]
        .map(|st| st.map(|st| single(Shader::default(), st, &mut dl_shaders)));
        let dl_first_slot = stage_slots.len() as u32;

        let align = device.limits().min_uniform_buffer_offset_alignment as u64;
        let stage_params_stride = STAGE_PARAMS_SIZE.div_ceil(align) * align;
        let stage_params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage params"),
            size: ((stage_slots.len() + dl_shaders.len()) as u64 * stage_params_stride)
                .max(stage_params_stride),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Provisional fill at t = 0 so every slot starts valid (static stages
        // keep these values; animated ones are rewritten each frame).
        let all_slots = stage_slots
            .iter()
            .copied()
            .chain(dl_shaders.iter().map(|sh| (sh, 0)));
        for (i, (sh, idx)) in all_slots.enumerate() {
            if let Some(p) = stage_params(sh, idx, 0.0) {
                queue.write_buffer(
                    &stage_params_buf,
                    i as u64 * stage_params_stride,
                    bytemuck::bytes_of(&p),
                );
            }
        }
        println!(
            "stage params: {} slots x {STAGE_PARAMS_SIZE} B (stride {stage_params_stride})",
            stage_slots.len()
        );

        // Expand authored materials: one StageBatch per parsed stage sharing
        // the parent batch's index range. Implicit materials and blocks with
        // no effective stages keep the legacy fast path.
        let mut stage_batches: Vec<StageBatch> = Vec::new();
        let mut stages_of_batch: Vec<Vec<u32>> = vec![Vec::new(); batch_draws.len()];
        let mut animated: Vec<(Shader, usize, u32)> = Vec::new();
        let mut bundle_views = BundleViews {
            views: HashMap::new(),
            table: self.image_gamma.as_ref(),
        };
        // once per material, not per batch: a material can hold several batches
        let dropped_stages: usize = bsp
            .materials
            .iter()
            .zip(&kinds)
            .filter(|(_, k)| **k == MaterialKind::Draw)
            .filter_map(|(m, _)| self.shader_lib.get(&m.name))
            .map(|sh| sh.dropped_stages)
            .sum();
        for (bi, batch) in batches.iter().enumerate() {
            if kinds[batch.material as usize] != MaterialKind::Draw {
                continue;
            }
            let name = &bsp.materials[batch.material as usize].name;
            let Some(sh) = self.shader_lib.get(name) else {
                continue;
            };
            if sh.stages.is_empty() {
                continue;
            }
            let lm_view = match batch.lightmap {
                bsp::NO_LIGHTMAP => white_lightmap,
                i => lightmap_views.get(i as usize).unwrap_or(white_lightmap),
            };
            for si in 0..sh.stages.len() {
                let Some((variant, two_sided, bias)) = stage_draw_facts(sh, si) else {
                    continue;
                };
                let slot = slot_of[&(batch.material, si)];
                let (mat, stage) = stage_bind_groups(
                    device,
                    queue,
                    fs,
                    material_layout,
                    &self.stage_layout,
                    &stage_params_buf,
                    STAGE_PARAMS_SIZE,
                    white_view,
                    lm_view,
                    diffuse_sampler,
                    lightmap_sampler,
                    &mut bundle_views,
                    &sh.stages[si],
                );
                if stage_animated(sh, si) {
                    animated.push((sh.clone(), si, slot));
                }
                stages_of_batch[bi].push(stage_batches.len() as u32);
                stage_batches.push(StageBatch {
                    slot,
                    variant,
                    two_sided,
                    bias,
                    mat,
                    stage,
                });
            }
        }
        // Scripted prop skins keep the prop path's two-sidedness (xmodel
        // winding) and take no lightmap.
        for (pi, script) in prop_scripts.iter().enumerate() {
            let Some((sh, first)) = *script else { continue };
            for si in 0..sh.stages.len() {
                let Some((variant, _, bias)) = stage_draw_facts(sh, si) else {
                    continue;
                };
                let slot = first + si as u32;
                let (mat, stage) = stage_bind_groups(
                    device,
                    queue,
                    fs,
                    material_layout,
                    &self.stage_layout,
                    &stage_params_buf,
                    STAGE_PARAMS_SIZE,
                    white_view,
                    white_lightmap,
                    diffuse_sampler,
                    lightmap_sampler,
                    &mut bundle_views,
                    &sh.stages[si],
                );
                if stage_animated(sh, si) {
                    animated.push((sh.clone(), si, slot));
                }
                stages_of_batch[batches.len() + pi].push(stage_batches.len() as u32);
                stage_batches.push(StageBatch {
                    slot,
                    variant,
                    two_sided: true,
                    bias,
                    mat,
                    stage,
                });
            }
        }
        for (bi, sbs) in stages_of_batch.iter().enumerate() {
            if !sbs.is_empty() {
                batch_draws[bi].pass = Pass::Stage;
            }
        }

        // The dlight pass per world batch: a scripted material's stages
        // through their own bind groups, an implicit one through its base
        // bind group and a neutral bundle 1.
        let implicit_stage_bg =
            AnimFrames::static_(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("implicit dlight stage bind group"),
                layout: &self.stage_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &stage_params_buf,
                            offset: 0,
                            size: wgpu::BufferSize::new(STAGE_PARAMS_SIZE),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(white_view),
                    },
                ],
            }));
        let mut dlight_batches: Vec<DlightBatch> = Vec::new();
        let mut dlights_of_batch: Vec<Vec<u32>> = vec![Vec::new(); batch_draws.len()];
        for (bi, batch) in batches.iter().enumerate() {
            if kinds[batch.material as usize] != MaterialKind::Draw {
                continue;
            }
            let sh = self
                .shader_lib
                .get(&bsp.materials[batch.material as usize].name);
            if batch_draws[bi].pass == Pass::Stage {
                let Some(list) = dl_of_material.get(&batch.material) else {
                    continue;
                };
                let lm_view = match batch.lightmap {
                    bsp::NO_LIGHTMAP => white_lightmap,
                    i => lightmap_views.get(i as usize).unwrap_or(white_lightmap),
                };
                for &di in list {
                    let syn = &dl_shaders[di as usize];
                    let st = &syn.stages[0];
                    let (Some(blend), Some((_, two_sided, bias))) =
                        (DlightBlend::of(st), stage_draw_facts(syn, 0))
                    else {
                        log::warn!("{}: dlight stage blend {:?} not drawn", syn.name, st.blend);
                        continue;
                    };
                    let slot = dl_first_slot + di;
                    let (mat, stage) = stage_bind_groups(
                        device,
                        queue,
                        fs,
                        material_layout,
                        &self.stage_layout,
                        &stage_params_buf,
                        STAGE_PARAMS_SIZE,
                        white_view,
                        lm_view,
                        diffuse_sampler,
                        lightmap_sampler,
                        &mut bundle_views,
                        st,
                    );
                    if stage_animated(syn, 0) {
                        animated.push((syn.clone(), 0, slot));
                    }
                    dlights_of_batch[bi].push(dlight_batches.len() as u32);
                    dlight_batches.push(DlightBatch {
                        blend,
                        two_sided,
                        bias,
                        slot,
                        mat,
                        stage,
                    });
                }
                continue;
            }
            // retail's implicit shader multiplies the texture by the
            // lightmap (0x4fc440), which the copy takes; a vertex-lit one
            // has no lightmap bundle and gets `dlightshader`
            if sh.is_some_and(|sh| sh.dlight_draw().is_none()) {
                continue;
            }
            let call = &batch_draws[bi];
            let which = match (call.pass, batch.lightmap) {
                (Pass::Opaque | Pass::Overlay, bsp::NO_LIGHTMAP) => 2,
                (Pass::Opaque, _) => 0,
                (Pass::Overlay, _) => 1,
                _ => continue,
            };
            let Some(di) = dl_implicit[which] else {
                continue;
            };
            let blend = DlightBlend::of(&dl_shaders[di as usize].stages[0])
                .expect("the implicit dlight stages blend onto GL_ONE");
            dlights_of_batch[bi].push(dlight_batches.len() as u32);
            dlight_batches.push(DlightBatch {
                blend,
                two_sided: false,
                bias: call.pass == Pass::Overlay,
                slot: dl_first_slot + di,
                mat: AnimFrames::static_(bind_groups[call.bind_group].clone()),
                stage: AnimFrames::static_(implicit_stage_bg.groups[0].clone()),
            });
        }
        println!(
            "dlight pass: {} stage draws over {} batches",
            dlight_batches.len(),
            dlights_of_batch.iter().filter(|d| !d.is_empty()).count()
        );
        self.shader_dropped = dropped_stages;
        println!(
            "shader scripts: {} stage batches, {} dropped stages, {} lib warnings",
            stage_batches.len(),
            dropped_stages,
            self.shader_lib.warn_count()
        );

        // ---- sky pass: farbox now, cloud dome through the stage path ----
        self.sky = None;
        let mut sky_stages: Vec<(Shader, usize, u32)> = Vec::new();
        if let Some((name, mat_idx)) = active_sky {
            let sh = self
                .shader_lib
                .get(&name)
                .expect("sky candidate comes from the lib");
            let parms = sh.sky.as_ref().expect("sky_blocks only yields skyparms");
            log::info!(
                "sky: {name}  env {}  cloud height {}  stages {}",
                parms.env,
                parms.cloud_height,
                sh.stages.len()
            );
            // retail skips the whole box when outerbox[0] fails to load
            let mut farbox = None;
            if parms.env.is_empty() {
                log::info!("sky {name}: skyParms '-' outerbox, drawing no farbox");
            } else {
                let rt = format!("{}_rt", parms.env);
                if assets::resolve_bundle_image(fs, &rt).is_some() {
                    farbox = Some(build_sky_box(
                        device,
                        queue,
                        SCENE_FORMAT,
                        fs,
                        &self.camera_layout,
                        &mut bundle_views,
                        white_view,
                        &parms.env,
                        &name,
                    ));
                } else {
                    log::warn!("sky {name}: env image {rt} not found, drawing no farbox");
                }
            }
            let mut dome = None;
            if !sh.stages.is_empty() {
                let size = sky::box_size(Z_FAR, Z_NEAR);
                let (verts, idx) = sky::dome_mesh(size, parms.cloud_height);
                let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("sky dome vertices"),
                    contents: bytemuck::cast_slice(&verts),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("sky dome indices"),
                    contents: bytemuck::cast_slice(&idx),
                    usage: wgpu::BufferUsages::INDEX,
                });
                let mut dome_stages = Vec::new();
                for si in 0..sh.stages.len() {
                    if sh.stages[si].bundles.is_empty() {
                        continue;
                    }
                    let Some((variant, two_sided, _)) = stage_draw_facts(sh, si) else {
                        continue;
                    };
                    // a sky stage may carry explicit depthWrite; the dome must
                    // still never write depth, it draws first at far depth
                    let variant = match variant {
                        StageVariant::BlendDepthWrite => StageVariant::Blend,
                        StageVariant::AdditiveFixedDepthWrite => StageVariant::AdditiveFixed,
                        StageVariant::AdditiveFadedDepthWrite => StageVariant::AdditiveFaded,
                        other => other,
                    };
                    // the block's slots were allocated with every other material's
                    let slot = slot_of[&(mat_idx, si)];
                    let (mat, stage) = stage_bind_groups(
                        device,
                        queue,
                        fs,
                        material_layout,
                        &self.stage_layout,
                        &stage_params_buf,
                        STAGE_PARAMS_SIZE,
                        white_view,
                        white_lightmap,
                        diffuse_sampler,
                        lightmap_sampler,
                        &mut bundle_views,
                        &sh.stages[si],
                    );
                    // dome params ride the per-frame rewrite with the eye offset
                    sky_stages.push((sh.clone(), si, slot));
                    dome_stages.push(DomeStage {
                        variant,
                        two_sided,
                        slot,
                        mat,
                        stage,
                    });
                }
                if !dome_stages.is_empty() {
                    log::info!(
                        "cloud dome: {} stage(s), {} verts",
                        dome_stages.len(),
                        verts.len()
                    );
                    dome = Some(DomeLayer {
                        vertex_buf,
                        index_buf,
                        index_count: idx.len() as u32,
                        stages: dome_stages,
                    });
                }
            }
            // Retail draws the sun, disc and flare, only on a map with a
            // lump-30 sun light (0x4e83f0 tests `world+0x114`); the disc
            // and the flare's angle test follow `sundirection`, the flare
            // sits on the light's own direction (0x4d9250). A missing `.sun`
            // file draws nothing: the world load zeroes the sun state and
            // only a file that loads refills it (shader doc 8).
            let mut sun = None;
            let mut flare = None;
            let sun_light = bsp
                .lights
                .iter()
                .rev()
                .find(|w| w[0] == 1)
                .map(|w| glam::Vec3::from_array([7, 8, 9].map(|i| f32::from_bits(w[i]))));
            let parsed = sh.sunfile.as_ref().and_then(|sun_name| {
                let sf = fs
                    .read(&format!("scripts/{sun_name}.sun"))
                    .map(|b| SunFile::parse(&String::from_utf8_lossy(&b)));
                if sf.is_none() {
                    log::warn!("sunfile: scripts/{sun_name}.sun not found");
                }
                sf
            });
            if let (Some(sf), Some(light_dir)) = (&parsed, sun_light) {
                let dir = glam::Vec3::from(bsp::sun_direction(&bsp.entities));
                if let Some(sprite) = &sf.sprite {
                    log::info!("sky {name}: sun sprite {sprite} size {}", sf.sprite_size);
                    sun = build_sun_sprite(
                        device,
                        queue,
                        SCENE_FORMAT,
                        fs,
                        &self.camera_layout,
                        &mut bundle_views,
                        sprite,
                        dir,
                        sf.sprite_size,
                    );
                }
                // the flare's image: its block's first stage, or the name
                let image = sf.flare.as_ref().and_then(|f| {
                    let path = self.shader_lib.image(f).unwrap_or(f).to_string();
                    assets::resolve_bundle_image(fs, &path).map(|_| path)
                });
                match image {
                    Some(path) => {
                        log::info!("sky {name}: sun flare {path}");
                        let view = upload_bundle_view(device, queue, fs, &mut bundle_views, &path);
                        flare = Some(SunFlareGpu::new(
                            device,
                            FlareParams::of(sf),
                            light_dir,
                            dir,
                            &view,
                        ));
                    }
                    None => log::info!("sky {name}: no sun flare image, drawing no flare"),
                }
            }
            self.sky = Some(SkyRender {
                name,
                farbox,
                dome,
                sun,
                flare,
            });
        }

        // Retail's sorted shader list over the map's shaders (shader doc 5):
        // one shader per (material, lightmap), registered in batch order,
        // which follows the shader lump's first-use order; images number
        // as they first load, white and the lightmap pages before the
        // textures. vcod's own: props and the implicit overlays keep their
        // slot after every other opaque.
        let batch_keys: Vec<(f32, u32)> = {
            let mut images = ImageNumbers::default();
            images.number("$white");
            let lightmap = |i: u16| format!("$lightmap{i}");
            for i in 0..lightmap_views.len() {
                images.number(&lightmap(i as u16));
            }
            let mut entries: Vec<SortEntry> = Vec::with_capacity(batch_draws.len());
            for (bi, batch) in batches.iter().enumerate() {
                let mat = &bsp.materials[batch.material as usize];
                let sh = self.shader_lib.get(&mat.name);
                let mut image = |r: &ImageRef| match r {
                    ImageRef::Path(p) => images.number(p),
                    ImageRef::Lightmap if batch.lightmap != bsp::NO_LIGHTMAP => {
                        images.number(&lightmap(batch.lightmap))
                    }
                    ImageRef::Lightmap | ImageRef::White => images.number("$white"),
                };
                entries.push(match (batch_draws[bi].pass, sh) {
                    (Pass::Overlay, _) => SortEntry {
                        sort: SORT_OPAQUE,
                        images: Some([u32::MAX, 0]),
                    },
                    (Pass::Stage, Some(sh)) => staged_sort_entry(sh, &mut image),
                    // implicit: the texture, then the lightmap in bundle 1
                    (_, sh) => {
                        let tex = images.number(&mat.name);
                        let lm = match batch.lightmap {
                            bsp::NO_LIGHTMAP => 0,
                            i => images.number(&lightmap(i)),
                        };
                        SortEntry {
                            sort: sh.map_or(SORT_OPAQUE, Shader::sort_value),
                            images: Some([tex, lm]),
                        }
                    }
                });
            }
            for script in &prop_scripts {
                entries.push(match script {
                    Some((sh, _)) => staged_sort_entry(sh, &mut |r: &ImageRef| match r {
                        ImageRef::Path(p) => images.number(p),
                        ImageRef::Lightmap | ImageRef::White => images.number("$white"),
                    }),
                    None => SortEntry {
                        sort: SORT_OPAQUE,
                        images: Some([u32::MAX - 1, 0]),
                    },
                });
            }
            let places = sorted_places(&entries);
            entries
                .iter()
                .zip(places)
                .map(|(e, p)| (e.sort, p))
                .collect()
        };

        self.world = Some(WorldGpu {
            vertex_buf,
            index_buf,
            bind_groups,
            cpu_indices: indices,
            soup_ranges,
            prop_ranges,
            prop_bounds: props.bounds,
            batch_draws,
            draws: Vec::new(),
            stage_batches,
            stages_of_batch,
            animated,
            sky_stages,
            batch_keys,
            frame_draws: Vec::new(),
            stage_draws_last: 0,
            stage_params_buf,
            stage_params_stride,
            vis: WorldVis::build(bsp),
            gathered: Vec::new(),
            gather_scratch: Vec::new(),
            locked: None,
            last_cull: None,
            soup_model,
            static_models: (0..bsp.models.len()).map(|m| m == 0).collect(),
            static_dirty: false,
            model_soups: bsp
                .models
                .iter()
                .map(|m| m.first_soup as usize..(m.first_soup + m.num_soups) as usize)
                .collect(),
            moved: Vec::new(),
            moved_draws: Vec::new(),
            s_axis_buf,
            soup_bounds: dlight::soup_bounds(bsp),
            dlight_batches,
            dlights_of_batch,
            dlight_runs: Vec::new(),
            dlight_indices: Vec::new(),
            dlight_index_buf: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("dlight indices"),
                size: 4,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            shadow_casters: None,
        });
        // the materials were dropped with the old map
        self.resolve_viewmodel_skins(fs);
        Ok(())
    }

    /// The submodels drawn this frame, each with its entity's pose. One at
    /// the identity stands where the map put it and draws with the world;
    /// any other draws its own soups through a camera carrying its pose, so
    /// both keep the baked lightmap, as retail's brush model ref entity
    /// does. A submodel not listed, one whose entity is not in the snapshot,
    /// draws nowhere: retail's world draw holds model 0's surfaces only.
    pub fn set_submodels(&mut self, models: &[(usize, glam::Mat4)]) {
        let Some(world) = &mut self.world else {
            return;
        };
        let mut want: Vec<bool> = (0..world.static_models.len()).map(|m| m == 0).collect();
        world.moved.clear();
        for &(m, pose) in models {
            if m == 0 || m >= want.len() {
                continue;
            }
            if pose == glam::Mat4::IDENTITY {
                want[m] = true;
            } else {
                world.moved.push((m, pose));
            }
        }
        if want != world.static_models {
            world.static_models = want;
            world.static_dirty = true;
        }
    }

    /// A download reopened the search path: reparse the shader scripts the
    /// next `load_world` classifies against, and drop the fx and HUD textures
    /// resolved off the old paks, misses included.
    pub fn reopen(&mut self, fs: &Pk3Fs) {
        self.shaders = assets::load_shaders(fs);
        self.shader_lib = ShaderLib::load(fs);
        self.lib_warn_count = self.shader_lib.warn_count();
        self.fx.textures.clear();
        self.marks.reopen();
        self.hud_pass.textures.clear();
        self.vm_pass.cache.clear();
    }

    /// Drops the map and every model uploaded for it, so no `ModelHandle`
    /// from the old map can index a new upload.
    pub fn unload_world(&mut self) {
        self.world = None;
        self.marks.unload();
        self.coronas.clear();
        self.sky = None;
        self.dynamic.models.clear();
        self.dynamic.instances.clear();
        self.dynamic.bone_mats.clear();
        self.dynamic.light_origins.clear();
        self.dynamic.lighting = None;
        self.dynamic.materials.clear();
        self.vm_pass.skins.clear();
        self.dynamic.entity_scene_lights.clear();
        self.vis_counts = VisCounts::default();
    }

    /// Quads in draw order (already back-to-front). Unseen shader names
    /// resolve lazily; an unresolvable one warns once and is cached as
    /// `None`. Additive materials (see `Shaders::is_additive`) go on the
    /// additive pipeline, everything else on straight alpha.
    pub fn set_fx_quads(&mut self, fs: &Pk3Fs, quads: Vec<FxQuad>) {
        // build_quads is capped by construction; the vertex buffer is sized
        // exactly, so truncate anyway.
        let quads = if quads.len() > MAX_FX_QUADS {
            log::warn!(
                "fx: {} quads exceeds the {MAX_FX_QUADS} cap, truncating",
                quads.len()
            );
            &quads[..MAX_FX_QUADS]
        } else {
            &quads[..]
        };

        // Retail's draw list keys an effect's polygons by material sort
        // first (shader doc 5); within a sort they keep the sim's
        // back-to-front order. An fx material without a script sorts as
        // its blend does.
        let mut sorts: HashMap<&str, f32> = HashMap::new();
        for quad in quads {
            sorts.entry(&quad.shader).or_insert_with(|| {
                self.shader_lib
                    .get(&quad.shader)
                    .map(Shader::sort_value)
                    .unwrap_or(if self.shaders.is_additive(&quad.shader) {
                        SORT_ADDITIVE
                    } else {
                        SORT_BLEND
                    })
            });
        }
        let mut order: Vec<&FxQuad> = quads.iter().collect();
        order.sort_by(|a, b| sorts[a.shader.as_str()].total_cmp(&sorts[b.shader.as_str()]));
        let mut verts: Vec<FxVert> = Vec::with_capacity(quads.len() * 4);
        let mut runs: Vec<(String, bool, usize, f32)> = Vec::new();
        for quad in order {
            if !self.fx.textures.contains_key(&quad.shader) {
                let bg = resolve_fx_texture(
                    &self.device,
                    &self.queue,
                    &self.vm_pass.skin_layout,
                    &self.fx.sampler,
                    fs,
                    self.shaders.image_map(),
                    &quad.shader,
                    self.image_gamma.as_ref(),
                );
                self.fx.textures.insert(quad.shader.clone(), bg);
            }

            let k = colour_scale(self.shader_lib.get(&quad.shader), self.identity_light);
            let [r, g, b, a] = quad.rgba;
            for i in 0..4 {
                verts.push(FxVert {
                    pos: quad.verts[i],
                    uv: quad.uvs[i],
                    rgba: [r * k, g * k, b * k, a],
                });
            }
            match runs.last_mut() {
                Some((name, _, count, _)) if *name == quad.shader => *count += 1,
                _ => {
                    let additive = self.shaders.is_additive(&quad.shader);
                    let sort = sorts[quad.shader.as_str()];
                    runs.push((quad.shader.clone(), additive, 1, sort));
                }
            }
        }

        self.queue
            .write_buffer(&self.fx.vertex_buf, 0, bytemuck::cast_slice(&verts));
        self.fx.runs = runs;
    }

    /// One frame of impact marks: `requests` placed at `now_ms` (the
    /// cgame's clock), the pool expired, faded and uploaded. `enabled` is
    /// `cg_marks`.
    pub fn set_marks(
        &mut self,
        fs: &Pk3Fs,
        requests: &[crate::marks::MarkRequest],
        now_ms: i32,
        time: f32,
        enabled: bool,
    ) {
        let ctx = mark_pass::MarkCtx {
            device: &self.device,
            queue: &self.queue,
            fs,
            shader_lib: &self.shader_lib,
            material_layout: &self.material_layout,
            stage_layout: &self.stage_layout,
            white_view: &self.white_view,
            white_lightmap: &self.white_lightmap,
            diffuse_sampler: &self.diffuse_sampler,
            lightmap_sampler: &self.lightmap_sampler,
            image_gamma: self.image_gamma.as_ref(),
        };
        if !self.coronas.is_empty() {
            self.marks.prepare_sprites(&ctx, crate::corona::SHADER);
        }
        self.marks.update(&ctx, requests, now_ms, time, enabled);
    }

    /// Live marks in the pool, for the F3 `fx` line.
    pub fn mark_count(&self) -> usize {
        self.marks.live()
    }

    /// The cgame's scene lights for this frame (`entities::BuiltScene`);
    /// entity picks see them after the fx lights.
    pub fn set_entity_lights(&mut self, lights: Vec<SceneLight>) {
        self.dynamic.entity_scene_lights = lights;
    }

    /// The frame's scene lights (at most `MAX_SCENE_LIGHTS`), nearest the
    /// camera first: entity picks and the dlight pass see all of them, the
    /// prop vertex shader the first `MAX_LIGHTS`. An empty slice zeroes
    /// every slot.
    pub fn set_fx_lights(&mut self, lights: &[FxLight]) {
        self.dynamic.scene_lights = entity_light::scene_lights(lights);
        let uniform = FxLightsUniform::from_lights(lights);
        self.queue
            .write_buffer(&self.fx_lights_buf, 0, bytemuck::bytes_of(&uniform));
    }

    /// Quads in draw order, pixel coords with origin top-left. Textures
    /// resolve lazily by name; an unresolvable one warns once and is cached
    /// as `None`.
    pub fn set_hud_quads(&mut self, fs: &Pk3Fs, quads: Vec<HudQuad>) {
        let quads = if quads.len() > MAX_HUD_QUADS {
            if !self.hud_quad_cap_warned {
                self.hud_quad_cap_warned = true;
                log::warn!(
                    "hud: {} quads exceeds the {MAX_HUD_QUADS} cap, truncating",
                    quads.len()
                );
            }
            &quads[..MAX_HUD_QUADS]
        } else {
            &quads[..]
        };

        let (w, h) = (self.config.width as f32, self.config.height as f32);
        let mut verts: Vec<HudVert> = Vec::with_capacity(quads.len() * 4);
        let mut runs: Vec<(String, usize)> = Vec::new();
        for quad in quads {
            if !self.hud_pass.textures.contains_key(&quad.texture) {
                let bg = resolve_hud_texture(
                    &self.device,
                    &self.queue,
                    &self.hud_pass.texture_layout,
                    &self.hud_pass.sampler,
                    fs,
                    self.shaders.image_map(),
                    &quad.texture,
                );
                self.hud_pass.textures.insert(quad.texture.clone(), bg);
            }

            // `black` shares `white`'s 1x1 texture; force the rgb here so it
            // draws black regardless of the element colour, alpha untouched.
            let rgba = if quad.texture == "black" {
                [0.0, 0.0, 0.0, quad.rgba[3]]
            } else {
                hud_colour(
                    quad.rgba,
                    colour_scale(self.shader_lib.get(&quad.texture), self.identity_light),
                )
            };

            for i in 0..4 {
                let [px, py] = quad.verts[i];
                verts.push(HudVert {
                    // pixel coords, origin top-left -> clip space
                    pos: [px / w * 2.0 - 1.0, 1.0 - py / h * 2.0],
                    uv: quad.uvs[i],
                    color: rgba,
                });
            }
            match runs.last_mut() {
                Some((name, count)) if *name == quad.texture => *count += 1,
                _ => runs.push((quad.texture.clone(), 1)),
            }
        }

        self.queue
            .write_buffer(&self.hud_pass.vertex_buf, 0, bytemuck::cast_slice(&verts));
        self.hud_pass.quad_count = quads.len();
        self.hud_pass.runs = runs;
    }

    /// Quads drawn last frame, for the F3 overlay.
    pub fn hud_quad_count(&self) -> usize {
        self.hud_pass.quad_count
    }

    /// Slice order is draw order (hands, then the gun). Replaces anything set before.
    pub fn set_viewmodel(&mut self, fs: &Pk3Fs, models: &[xmodel::XModel]) {
        let uploaded: Vec<_> = models
            .iter()
            .filter_map(|m| self.vm_upload(fs, m))
            .collect();
        let surfaces: usize = uploaded.iter().map(|m| m.surfaces.len()).sum();
        println!(
            "viewmodel: {} models, {surfaces} drawn surfaces",
            uploaded.len()
        );
        self.vm_pass.models = uploaded;
        self.resolve_viewmodel_skins(fs);
    }

    /// The drawn viewmodel's materials, from this map's entity materials.
    fn resolve_viewmodel_skins(&mut self, fs: &Pk3Fs) {
        let gamma = self.image_gamma;
        self.vm_pass.skins = self
            .vm_pass
            .models
            .iter()
            .map(|m| {
                m.skins
                    .iter()
                    .map(|skin| {
                        self.dynamic.materials.skin(
                            &self.device,
                            &self.queue,
                            fs,
                            gamma.as_ref(),
                            skin,
                        )
                    })
                    .collect()
            })
            .collect();
    }

    /// Uploads viewmodel parts into the cache without drawing them, so a
    /// later `set_viewmodel` of the same parts uploads nothing.
    pub fn preload_viewmodel(&mut self, fs: &Pk3Fs, models: &[xmodel::XModel]) {
        for m in models {
            self.vm_upload(fs, m);
        }
    }

    /// A viewmodel part's upload, out of the cache after the first time.
    fn vm_upload(&mut self, fs: &Pk3Fs, m: &xmodel::XModel) -> Option<Rc<VmModel>> {
        let key = vm_cache_key(m);
        if let Some(hit) = self.vm_pass.cache.get(&key) {
            return Some(hit.clone());
        }
        let Some(model) = upload_vm_model(
            &self.device,
            &self.queue,
            &self.vm_pass,
            &m.surfaces,
            &m.materials,
            &|skin| {
                let mut img = assets::load_skin_image(fs, skin);
                crate::gamma::bake_image(&mut img, self.image_gamma.as_ref());
                img
            },
        ) else {
            log::warn!("viewmodel {}: no drawable surfaces, skipping it", m.lod);
            return None;
        };
        let model = Rc::new(model);
        self.vm_pass.cache.insert(key, model.clone());
        Some(model)
    }

    /// Same upload as the viewmodel (bind pose baked into the mesh); an
    /// empty model returns `None`.
    pub fn upload_dynamic_model(
        &mut self,
        fs: &Pk3Fs,
        model: &xmodel::XModel,
    ) -> Option<ModelHandle> {
        let vm = &self.vm_pass;
        let uploaded = upload_vm_model(
            &self.device,
            &self.queue,
            vm,
            &model.surfaces,
            &model.materials,
            &|skin| {
                let mut img = assets::load_skin_image(fs, skin);
                crate::gamma::bake_image(&mut img, self.image_gamma.as_ref());
                img
            },
        )?;
        let gamma = self.image_gamma;
        let skins: Vec<_> = uploaded
            .skins
            .iter()
            .map(|skin| {
                self.dynamic
                    .materials
                    .skin(&self.device, &self.queue, fs, gamma.as_ref(), skin)
            })
            .collect();
        let lit = skins.iter().any(|s| s.lit);
        self.dynamic.models.push(DynModel {
            vm: uploaded,
            skins,
            lit,
        });
        Some(ModelHandle(self.dynamic.models.len() - 1))
    }

    /// Instances past `MAX_DYNAMIC_INSTANCES` are dropped (warned once); a
    /// stale handle is skipped. Written to the GPU in [`Self::render`].
    pub fn set_dynamic_models(&mut self, instances: &[DynamicModelInstance]) {
        static OVERFLOW_WARNED: std::sync::Once = std::sync::Once::new();

        let mut model_idxs = Vec::with_capacity(instances.len());
        let mut items = Vec::with_capacity(instances.len());
        let mut origins = Vec::with_capacity(instances.len());
        for inst in instances {
            if model_idxs.len() == MAX_DYNAMIC_INSTANCES {
                OVERFLOW_WARNED.call_once(|| {
                    log::warn!(
                        "more than {MAX_DYNAMIC_INSTANCES} live entities this frame; dropping the rest"
                    );
                });
                break;
            }
            let ModelHandle(idx) = inst.model;
            if idx >= self.dynamic.models.len() {
                continue;
            }
            model_idxs.push(idx);
            origins.push(inst.light_origin);
            items.push((idx, inst.transform.to_cols_array(), inst.bones.as_deref()));
        }
        let (raw, bone_mats) = pack_instances(&items);
        self.dynamic.instances = model_idxs.into_iter().zip(raw).collect();
        self.dynamic.bone_mats = bone_mats;
        self.dynamic.light_origins = origins;
    }

    /// Picks each instance's lights (one pick per distinct origin, so a
    /// player's parts share one) and points its `light_set` at them.
    fn pick_entity_lights(&mut self) -> Vec<GpuLightSet> {
        let d = &mut self.dynamic;
        let Some(lighting) = d.lighting.as_mut() else {
            return vec![GpuLightSet::default()];
        };
        let mut scene = d.scene_lights.clone();
        scene.extend_from_slice(&d.entity_scene_lights);
        scene.truncate(crate::fx::sim::MAX_SCENE_LIGHTS);
        let mut sets = Vec::new();
        let mut seen: HashMap<[u32; 3], u32> = HashMap::new();
        for ((model, raw), origin) in d.instances.iter_mut().zip(&d.light_origins) {
            // retail picks only for a model with a lit surface (0x50e3e0)
            if !d.models[*model].lit {
                raw.light_set = 0;
                continue;
            }
            let key = origin.to_array().map(f32::to_bits);
            raw.light_set = *seen.entry(key).or_insert_with(|| {
                let l = lighting.entity_lights(*origin, &scene);
                sets.push(entity_light::pack(&l, None, self.identity_light));
                sets.len() as u32 - 1
            });
        }
        if sets.is_empty() {
            sets.push(GpuLightSet::default());
        }
        sets
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
        self.msaa_view = create_msaa_view(&self.device, SCENE_FORMAT, w, h);
        self.depth_view = create_depth_view(&self.device, w, h);
        self.gamma.resize(&self.device, w, h);
        self.ssao.resize(&self.device, &self.depth_view, w, h);
        self.bloom
            .resize(&self.device, self.gamma.scene_view(), w, h);
        self.shafts.resize(
            &self.device,
            self.gamma.scene_view(),
            &self.depth_view,
            w,
            h,
        );
    }

    /// `vc_bloom` (0 off) and `vc_bloomThreshold`, applied from the next frame.
    pub fn set_bloom(&mut self, intensity: f32, threshold: f32) {
        self.bloom_intensity = intensity.max(0.0);
        self.bloom_threshold = threshold;
    }

    /// `vc_shadows`: builds the shadow pass on the first frame it is on,
    /// drops it when it goes off.
    pub fn set_shadows(&mut self, on: bool) {
        if on == self.shadows.is_some() {
            return;
        }
        self.shadows = on.then(|| {
            let [vertex, instance] = crate::entity_material::vertex_layouts();
            let mats = &self.dynamic.materials;
            shadows::ShadowPass::new(
                &self.device,
                &self.queue,
                SCENE_FORMAT,
                MSAA_SAMPLES,
                shadows::EntLayouts {
                    skin: &mats.skin_layout,
                    bones: &self.dynamic.bone_layout,
                    params: &mats.params_layout,
                    vertex,
                    instance,
                },
            )
        });
    }

    /// The F3 `shadow:` line while `vc_shadows` is on.
    pub fn shadow_debug(&self) -> Option<String> {
        self.shadows.as_ref().map(shadows::ShadowPass::debug_line)
    }

    /// `r_gamma`, already clamped to retail's range, and retail's overbright
    /// bits (`gamma::overbright_bits`); applied from the next frame.
    pub fn set_gamma(&mut self, gamma: f32, overbright: u32) {
        self.gamma.set_gamma(&self.queue, gamma, overbright);
        self.identity_light = crate::gamma::identity_light(overbright);
    }

    /// `vc_bicubic`, applied from the next frame.
    pub fn set_bicubic(&mut self, on: bool) {
        self.bicubic = on;
    }

    /// `vc_sunShafts` (0 off), applied from the next frame.
    pub fn set_sun_shafts(&mut self, strength: f32) {
        self.shafts.set_strength(strength);
    }

    /// `r_flares`: off, the sun's flare and washes neither step nor draw.
    pub fn set_flares(&mut self, on: bool) {
        self.flares = on;
    }

    /// `vc_ssao`, applied from the next frame; 0 is off.
    pub fn set_ssao(&mut self, strength: f32) {
        self.ssao.set_strength(strength);
    }

    /// The F3 `ssao` line: strength and smoothed GPU ms for the scene
    /// passes and the SSAO passes alone.
    pub fn ssao_line(&self) -> String {
        let s = self.ssao.strength();
        let head = if s > 0.0 {
            format!("ssao {s}")
        } else {
            "ssao off".to_string()
        };
        match self.ssao.timer().map(|t| t.ms()) {
            Some((scene, Some(ao))) => format!("{head}  gpu scene {scene:.2}ms ao {ao:.2}ms"),
            Some((scene, None)) => format!("{head}  gpu scene {scene:.2}ms"),
            None => format!("{head}  (no gpu timestamps)"),
        }
    }

    /// The `r_gamma` textures bake in from the next `load_world` (retail
    /// reloads every image per map), or `None` with device gamma.
    pub fn set_image_gamma(&mut self, gamma: Option<f32>) {
        self.image_gamma = gamma.map(|g| crate::gamma::ramp(g, 0));
    }

    /// For a lost or outdated swapchain.
    pub fn reconfigure(&mut self) {
        let (w, h) = (self.config.width, self.config.height);
        self.resize(w, h);
    }

    /// (world draw calls, dynamic instances, bone matrices) for the debug overlay.
    pub fn debug_counts(&self) -> (usize, usize, usize) {
        (
            self.world.as_ref().map_or(0, |w| w.draws.len()),
            self.dynamic.instances.len(),
            self.dynamic.bone_mats.len(),
        )
    }

    pub fn vis_counts(&self) -> &VisCounts {
        &self.vis_counts
    }

    /// (stage draws emitted last frame, stages dropped at load, unique
    /// shader-lib warnings) for the F3 `shader:` line.
    pub fn shader_stats(&self) -> (usize, usize, usize, usize) {
        (
            self.world.as_ref().map_or(0, |w| w.stage_draws_last),
            self.world.as_ref().map_or(0, |w| {
                w.dlight_runs
                    .iter()
                    .map(|r| w.dlights_of_batch[r.draw.batch as usize].len())
                    .sum()
            }),
            self.shader_dropped,
            self.lib_warn_count,
        )
    }

    /// Name of the map's active sky block, for the F3 `sky` field.
    pub fn sky_name(&self) -> Option<&str> {
        self.sky.as_ref().map(|s| s.name.as_str())
    }

    /// New global fog target from configstring 12; `None` clears it.
    pub fn set_fog(&mut self, fog: Option<FogParams>, now: f32) {
        match fog {
            Some(p) => self.fog.set_target(p, now),
            None => self.fog.clear(),
        }
    }

    /// The displayed fog, for the F3 `fog:` line.
    pub fn fog_debug(&self) -> String {
        if !self.fog.set {
            return "off".to_string();
        }
        let f = &self.fog.live;
        let tail = if self.fog.target.is_some() { " ->" } else { "" };
        if f.linear {
            format!("linear {}..{} {:?}{tail}", f.near, f.far, f.color)
        } else {
            format!("exp den {:.6} {:?}{tail}", f.density, f.color)
        }
    }

    /// `[variant][two_sided][bias]`.
    /// The world's draw sets: the world under the shared camera, then each
    /// moved brush model under its own.
    fn world_sets<'a>(
        &'a self,
        world: &'a WorldGpu,
    ) -> impl Iterator<Item = (&'a Vec<DrawRange>, &'a Vec<FrameDraw>, &'a wgpu::BindGroup)> {
        std::iter::once((&world.draws, &world.frame_draws, &self.camera_bg)).chain(
            world
                .moved_draws
                .iter()
                .zip(&self.moved_cams)
                .map(|((d, o), (_, bg))| (d, o, bg)),
        )
    }

    /// One world draw on its pipeline; `bound` tracks the bound pipeline.
    fn emit_world_draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        world: &WorldGpu,
        draw: &DrawRange,
        kind: DrawRef,
        bound: &mut Option<Bound>,
        time: f32,
    ) {
        match kind {
            DrawRef::Legacy(want) => {
                if *bound != Some(Bound::Legacy(want)) {
                    pass.set_pipeline(match want {
                        Pass::Opaque => &self.pipeline,
                        Pass::Prop => &self.prop_pipeline,
                        Pass::Layer => &self.layer_pipeline,
                        Pass::Overlay => &self.overlay_pipeline,
                        Pass::Stage => return,
                    });
                    *bound = Some(Bound::Legacy(want));
                }
                let call = &world.batch_draws[draw.batch as usize];
                pass.set_bind_group(1, &world.bind_groups[call.bind_group], &[]);
            }
            DrawRef::Stage(sb_idx) => {
                let sb = &world.stage_batches[sb_idx as usize];
                let key = Bound::Stage(sb.variant, sb.two_sided, sb.bias);
                if *bound != Some(key) {
                    pass.set_pipeline(self.stage_pipeline(sb.variant, sb.two_sided, sb.bias));
                    *bound = Some(key);
                }
                // animMap stages pick their frame per draw
                pass.set_bind_group(1, sb.mat.bind_group(time), &[]);
                pass.set_bind_group(
                    2,
                    sb.stage.bind_group(time),
                    &[
                        u32::try_from(u64::from(sb.slot) * world.stage_params_stride)
                            .expect("stage params slot offset fits a dynamic offset"),
                    ],
                );
            }
        }
        pass.draw_indexed(draw.first..draw.first + draw.count, 0, 0..1);
    }

    /// Each light over the opaque surfaces it reaches (`dlight.rs`).
    fn emit_dlights(&self, pass: &mut wgpu::RenderPass<'_>, world: &WorldGpu, time: f32) {
        pass.set_vertex_buffer(0, world.vertex_buf.slice(..));
        pass.set_vertex_buffer(1, world.s_axis_buf.slice(..));
        pass.set_index_buffer(world.dlight_index_buf.slice(..), wgpu::IndexFormat::Uint32);
        let mut bound = None;
        for run in &world.dlight_runs {
            let cam = run
                .moved
                .map_or(&self.camera_bg, |mi| &self.moved_cams[mi].1);
            pass.set_bind_group(0, cam, &[]);
            pass.set_bind_group(
                3,
                &self.dlight.bg,
                &[(u64::from(run.light) * self.dlight.stride) as u32],
            );
            for &db in &world.dlights_of_batch[run.draw.batch as usize] {
                let d = &world.dlight_batches[db as usize];
                let key = (d.blend, d.two_sided, d.bias);
                if bound != Some(key) {
                    pass.set_pipeline(self.dlight.pipeline(d.blend, d.two_sided, d.bias));
                    bound = Some(key);
                }
                pass.set_bind_group(1, d.mat.bind_group(time), &[]);
                pass.set_bind_group(
                    2,
                    d.stage.bind_group(time),
                    &[u32::try_from(u64::from(d.slot) * world.stage_params_stride)
                        .expect("stage params slot offset fits a dynamic offset")],
                );
                pass.draw_indexed(run.draw.first..run.draw.first + run.draw.count, 0, 0..1);
            }
        }
        pass.set_bind_group(0, &self.camera_bg, &[]);
    }

    /// `vc_shadows`' uniforms and caster passes for this frame; false
    /// when it is off or the map has no world or no sun.
    fn encode_shadow_maps(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        frame: &Frame,
        camera: &[f32],
        moved: &[glam::Mat4],
    ) -> bool {
        let (Some(sp), Some(world), Some(sun)) = (
            self.shadows.as_mut(),
            self.world.as_mut(),
            self.dynamic.lighting.as_ref().and_then(StaticLighting::sun),
        ) else {
            return false;
        };
        if world.shadow_casters.is_none() {
            world.shadow_casters = Some(shadow_casters(&self.device, world));
        }
        let world = &*world;
        let Some(casters) = &world.shadow_casters else {
            return false;
        };
        sp.prepare(
            &self.queue,
            &sun,
            casters,
            frame.view_proj,
            frame.eye,
            frame.fwd,
            camera,
            (self.config.width as f32, self.config.height as f32),
            moved,
        );
        // each entity surface casts through its first depth-writing stage
        let dynamic = &self.dynamic;
        let mats = &dynamic.materials;
        let mut ents = Vec::new();
        for (i, (m, _)) in dynamic.instances.iter().enumerate() {
            let model = &dynamic.models[*m];
            for (surf, skin) in model.vm.surfaces.iter().zip(&model.skins) {
                let Some(st) = skin
                    .stages
                    .iter()
                    .find(|st| mats.pipelines[st.pipeline].0.1)
                else {
                    continue;
                };
                ents.push(shadows::EntCaster {
                    vertex_buf: &model.vm.vertex_buf,
                    index_buf: &model.vm.index_buf,
                    instance: i as u32,
                    first_index: surf.first_index,
                    index_count: surf.index_count,
                    base_vertex: surf.base_vertex,
                    image: match st.image {
                        StageImage::Skin => &model.vm.bind_groups[surf.bind_group],
                        StageImage::Own(k) => &mats.images[k],
                    },
                    params_offset: (u64::from(st.slot) * ENT_STAGE_STRIDE) as u32,
                });
            }
        }
        let moved_models: Vec<usize> = world.moved.iter().map(|&(m, _)| m).collect();
        sp.encode_maps(
            encoder,
            &world.vertex_buf,
            casters,
            &moved_models,
            &ents,
            &shadows::EntBindings {
                instance_buf: &dynamic.instance_buf,
                instance_stride: DYNAMIC_INSTANCE_STRIDE,
                bones: &dynamic.bone_bg,
                params: &mats.params_bg,
            },
        );
        true
    }

    fn stage_pipeline(
        &self,
        variant: StageVariant,
        two_sided: bool,
        bias: bool,
    ) -> &wgpu::RenderPipeline {
        &self.stage_pipelines[variant as usize][usize::from(two_sided)][usize::from(bias)]
    }

    pub fn aspect(&self) -> f32 {
        self.config.width as f32 / self.config.height.max(1) as f32
    }

    /// Pixels.
    pub fn screen_size(&self) -> (f32, f32) {
        (self.config.width as f32, self.config.height as f32)
    }

    /// The visible set and the prop flags that go with it. A placement whose
    /// model failed to load has zero bounds and is never drawn or counted.
    fn cull(
        world: &WorldGpu,
        eye: glam::Vec3,
        view_proj: &glam::Mat4,
        frustum: &Frustum,
    ) -> (Visible, Vec<bool>) {
        let v = world.vis.visible(eye, frustum, Some(*view_proj));
        let prop_ok = world
            .prop_bounds
            .iter()
            .map(|&(lo, hi)| {
                (lo, hi) != (glam::Vec3::ZERO, glam::Vec3::ZERO)
                    && world.vis.prop_visible(&v, frustum, lo, hi)
            })
            .collect();
        (v, prop_ok)
    }

    /// `vm` is `None` to draw the world only. Skips the frame if no
    /// swapchain texture can be acquired.
    pub fn render(&mut self, frame: Frame, vm: Option<VmDraw>) {
        // `proj` (64) + `time_pad` (16) + eye/fog tail (48) + view forward
        // (16), matching Camera in the WGSL modules.
        self.fog.advance(frame.time);
        let mut camera = [0.0f32; CAMERA_FLOATS];
        camera[..16].copy_from_slice(&frame.view_proj.to_cols_array());
        camera[16] = frame.time;
        camera[17] = self.identity_light;
        camera[18] = if self.bicubic { 1.0 } else { 0.0 };
        if self.fog.set {
            let f = &self.fog.live;
            camera[20..24].copy_from_slice(&[frame.eye.x, frame.eye.y, frame.eye.z, f.mode()]);
            // GL_FOG_COLOR carries identityLight (0x4d2ad0); the clear doesn't.
            let il = self.identity_light;
            camera[24..28].copy_from_slice(&[
                f.color[0] * il,
                f.color[1] * il,
                f.color[2] * il,
                f.density,
            ]);
            camera[28..32].copy_from_slice(&[f.near, f.far, 0.0, 0.0]);
        } else {
            camera[20..24].copy_from_slice(&[frame.eye.x, frame.eye.y, frame.eye.z, 0.0]);
        }
        camera[32..36].copy_from_slice(&[frame.fwd.x, frame.fwd.y, frame.fwd.z, 0.0]);
        camera[36..52].copy_from_slice(&glam::Mat4::IDENTITY.to_cols_array());
        self.queue
            .write_buffer(&self.camera_buf, 0, bytemuck::cast_slice(&camera));
        let ssao_on = self.ssao.strength() > 0.0 && self.world.is_some();
        if ssao_on {
            self.ssao.prepare(
                &self.queue,
                &crate::ssao::FrameParams {
                    view_proj: frame.view_proj,
                    near: Z_NEAR,
                    far: Z_FAR,
                    fog_mode: camera[23],
                    fog_density: camera[27],
                    fog_near: camera[28],
                    fog_far: camera[29],
                },
            );
        }
        let moved: Vec<glam::Mat4> = self.world.as_ref().map_or_else(Vec::new, |w| {
            w.moved.iter().map(|&(_, pose)| pose).collect()
        });
        while self.moved_cams.len() < moved.len() {
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("moved brush model camera"),
                size: CAMERA_BYTES,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("moved brush model camera bind group"),
                layout: &self.camera_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.fx_lights_buf.as_entire_binding(),
                    },
                ],
            });
            self.moved_cams.push((buf, bg));
        }
        for (pose, (buf, _)) in moved.iter().zip(&self.moved_cams) {
            camera[36..52].copy_from_slice(&pose.to_cols_array());
            self.queue
                .write_buffer(buf, 0, bytemuck::cast_slice(&camera));
        }
        if let Some(sky) = &self.sky
            && let Some(farbox) = &sky.farbox
        {
            self.queue.write_buffer(
                &farbox.eye_buf,
                0,
                bytemuck::bytes_of(&[frame.eye.x, frame.eye.y, frame.eye.z, 0.0f32]),
            );
        }
        let t0 = std::time::Instant::now();
        let frustum = Frustum::from_view_proj(frame.view_proj);
        if let Some(world) = &mut self.world {
            let visible = match frame.cull {
                CullMode::Off => None,
                CullMode::Locked => Some(match world.locked.take() {
                    Some(v) => v,
                    None => {
                        // a fresh freeze: the index buffer holds some other set
                        world.last_cull = None;
                        Self::cull(world, frame.eye, &frame.view_proj, &frustum)
                    }
                }),
                CullMode::On => Some(Self::cull(world, frame.eye, &frame.view_proj, &frustum)),
            };
            // A corona's box goes through the frustum and the visited cells,
            // as a static model's does (0x4e48b8..0x4e4a41).
            let sprites: Vec<crate::corona::Sprite> = self
                .coronas
                .iter()
                .filter(|c| {
                    let (lo, hi) = crate::corona::cull_box(c);
                    match &visible {
                        Some((v, _)) => world.vis.prop_visible(v, &frustum, lo, hi),
                        None => frustum.intersects_aabb(lo, hi),
                    }
                })
                .filter_map(|c| crate::corona::sprite(c, frame.eye, frame.fwd, frame.up))
                .collect();
            self.marks
                .set_sprites(&self.queue, crate::corona::SHADER, &sprites);
            let mut props_drawn = 0usize;
            let shown = |si: usize| world.static_models[world.soup_model[si] as usize];
            let ranges: Vec<IndexRange> = match &visible {
                None => world
                    .soup_ranges
                    .iter()
                    .enumerate()
                    .filter_map(|(si, r)| if shown(si) { *r } else { None })
                    .chain(world.prop_ranges.iter().map(|&(_, r)| r))
                    .collect(),
                Some((v, prop_ok)) => {
                    props_drawn = prop_ok.iter().filter(|&&b| b).count();
                    world
                        .soup_ranges
                        .iter()
                        .zip(&v.soups)
                        .enumerate()
                        .filter_map(|(si, (r, &vis))| if vis && shown(si) { *r } else { None })
                        .chain(
                            world
                                .prop_ranges
                                .iter()
                                .filter(|(p, _)| prop_ok[*p as usize])
                                .map(|&(_, r)| r),
                        )
                        .collect()
                }
            };
            world.draws = gather(
                &world.cpu_indices,
                ranges,
                world.batch_draws.len(),
                &mut world.gathered,
                &mut world.gather_scratch,
            );
            // Animated stages get fresh params; static slots keep the load fill.
            for (sh, si, slot) in &world.animated {
                if let Some(p) = stage_params(sh, *si, frame.time) {
                    self.queue.write_buffer(
                        &world.stage_params_buf,
                        u64::from(*slot) * world.stage_params_stride,
                        bytemuck::bytes_of(&p),
                    );
                }
            }
            // Dome stages always carry the view origin, animated or not, and
            // the sky flag so linear fog skips them.
            for (sh, si, slot) in &world.sky_stages {
                if let Some(mut p) = stage_params(sh, *si, frame.time) {
                    p.flags |= STAGE_FLAG_EYE_OFFSET | STAGE_FLAG_SKY;
                    p.eye_off = [frame.eye.x, frame.eye.y, frame.eye.z, 0.0];
                    self.queue.write_buffer(
                        &world.stage_params_buf,
                        u64::from(*slot) * world.stage_params_stride,
                        bytemuck::bytes_of(&p),
                    );
                }
            }
            let mut frame_draws = std::mem::take(&mut world.frame_draws);
            order_draws(
                &world.batch_keys,
                &world.batch_draws,
                &world.stages_of_batch,
                &world.draws,
                &mut frame_draws,
            );
            world.frame_draws = frame_draws;
            // A moved brush model's soups follow the world's in the index
            // buffer, each set ordered on its own and drawn under its own
            // camera.
            let mut moved_draws = std::mem::take(&mut world.moved_draws);
            moved_draws.clear();
            for &(m, _) in &world.moved {
                let ranges: Vec<IndexRange> = world.model_soups[m]
                    .clone()
                    .filter_map(|si| world.soup_ranges.get(si).copied().flatten())
                    .collect();
                let mut out = Vec::new();
                let mut draws = gather(
                    &world.cpu_indices,
                    ranges,
                    world.batch_draws.len(),
                    &mut out,
                    &mut world.gather_scratch,
                );
                let base = world.gathered.len() as u32;
                for d in &mut draws {
                    d.first += base;
                }
                world.gathered.extend(out);
                let mut order = Vec::new();
                order_draws(
                    &world.batch_keys,
                    &world.batch_draws,
                    &world.stages_of_batch,
                    &draws,
                    &mut order,
                );
                moved_draws.push((draws, order));
            }
            world.moved_draws = moved_draws;
            // The dlight pass: per light, the visible soups it reaches
            // whose batch takes dlights, then each moved brush model's
            // with the light carried into the model's frame (0x4b59f0).
            world.dlight_runs.clear();
            world.dlight_indices.clear();
            let mut scene = self.dynamic.scene_lights.clone();
            scene.extend_from_slice(&self.dynamic.entity_scene_lights);
            let lights = dlight::from_scene(&scene);
            if !lights.is_empty() {
                for (i, l) in lights.iter().enumerate() {
                    self.queue.write_buffer(
                        &self.dlight.buf,
                        i as u64 * self.dlight.stride,
                        bytemuck::bytes_of(&dlight::DlightUniform::of(l)),
                    );
                }
                let takes = |si: usize| {
                    world.soup_ranges[si]
                        .is_some_and(|r| !world.dlights_of_batch[r.batch as usize].is_empty())
                };
                let lit: Vec<usize> = (0..world.soup_ranges.len())
                    .filter(|&si| {
                        takes(si) && shown(si) && visible.as_ref().is_none_or(|(v, _)| v.soups[si])
                    })
                    .collect();
                let mut sets: Vec<(Option<usize>, glam::Mat4, Vec<usize>)> =
                    vec![(None, glam::Mat4::IDENTITY, lit)];
                for (mi, &(m, pose)) in world.moved.iter().enumerate() {
                    let soups = world.model_soups[m]
                        .clone()
                        .filter(|&si| takes(si))
                        .collect();
                    sets.push((Some(mi), pose.inverse(), soups));
                }
                let mut out = Vec::new();
                for (moved, to_model, soups) in &sets {
                    for (li, l) in lights.iter().enumerate() {
                        let origin = to_model.transform_point3(l.origin);
                        let ranges = soups.iter().filter_map(|&si| {
                            let (lo, hi) = world.soup_bounds[si];
                            dlight::reaches(l, origin, lo, hi)
                                .then_some(world.soup_ranges[si])
                                .flatten()
                        });
                        let draws = gather(
                            &world.cpu_indices,
                            ranges,
                            world.batch_draws.len(),
                            &mut out,
                            &mut world.gather_scratch,
                        );
                        let base = world.dlight_indices.len() as u32;
                        world.dlight_indices.extend_from_slice(&out);
                        world
                            .dlight_runs
                            .extend(draws.into_iter().map(|d| DlightRun {
                                light: li as u32,
                                moved: *moved,
                                draw: DrawRange {
                                    first: d.first + base,
                                    ..d
                                },
                            }));
                    }
                }
                let bytes = (world.dlight_indices.len() * 4) as u64;
                if bytes > world.dlight_index_buf.size() {
                    world.dlight_index_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("dlight indices"),
                        size: bytes.next_power_of_two(),
                        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                }
                if bytes > 0 {
                    self.queue.write_buffer(
                        &world.dlight_index_buf,
                        0,
                        bytemuck::cast_slice(&world.dlight_indices),
                    );
                }
            }
            world.stage_draws_last = world
                .frame_draws
                .iter()
                .filter(|fd| matches!(fd.kind, DrawRef::Stage(_)))
                .count();
            // in Locked and Off the gathered set is the same every frame
            let unchanged = frame.cull != CullMode::On
                && world.last_cull == Some(frame.cull)
                && !world.static_dirty
                && world.moved.is_empty();
            world.static_dirty = false;
            if !unchanged {
                self.queue
                    .write_buffer(&world.index_buf, 0, bytemuck::cast_slice(&world.gathered));
            }
            world.last_cull = Some(frame.cull);
            let cell_total = world.vis.cell_count();
            self.vis_counts = VisCounts {
                mode: Some(frame.cull),
                cells: visible.as_ref().map_or((cell_total, cell_total), |(v, _)| {
                    (v.cells.iter().filter(|&&c| c).count(), cell_total)
                }),
                soups: visible.as_ref().map_or(
                    (world.soup_ranges.len(), world.soup_ranges.len()),
                    |(v, _)| (v.stats.soups, world.soup_ranges.len()),
                ),
                tris: (world.gathered.len() / 3, world.cpu_indices.len() / 3),
                props: (
                    if visible.is_some() {
                        props_drawn
                    } else {
                        world.prop_bounds.len()
                    },
                    world.prop_bounds.len(),
                ),
                occluders: visible.as_ref().map_or(0, |(v, _)| v.stats.occluders),
                portals_occluded: visible
                    .as_ref()
                    .map_or(0, |(v, _)| v.stats.portals_occluded),
                gather_ms: t0.elapsed().as_secs_f32() * 1000.0,
            };
            if frame.cull == CullMode::Locked {
                world.locked = visible;
            } else {
                world.locked = None;
            }
        }
        // The glyph cap truncates whole quads, so a cut readout stays legible.
        let hud_quads = if frame.hud_lines.is_empty() {
            0
        } else {
            let mut verts = hud_text::layout_lines(
                &frame.hud_lines,
                self.config.width as f32,
                self.config.height as f32,
                2.0,
            );
            verts.truncate(MAX_HUD_GLYPHS * 2 * 4);
            self.queue
                .write_buffer(&self.hud.vertex_buf, 0, bytemuck::cast_slice(&verts));
            verts.len() / 4
        };
        // Instance i draws from slot i of instance_buf.
        if !self.dynamic.instances.is_empty() {
            let m = &self.dynamic.materials;
            for (sh, si, slot) in &m.animated {
                if let Some(p) = stage_params(sh, *si, frame.time) {
                    self.queue.write_buffer(
                        &m.params_buf,
                        u64::from(*slot) * ENT_STAGE_STRIDE,
                        bytemuck::bytes_of(&p),
                    );
                }
            }
            let sets = self.pick_entity_lights();
            self.queue
                .write_buffer(&self.dynamic.light_buf, 0, bytemuck::cast_slice(&sets));
            let raw: Vec<InstanceRaw> = self.dynamic.instances.iter().map(|(_, r)| *r).collect();
            self.queue
                .write_buffer(&self.dynamic.instance_buf, 0, bytemuck::cast_slice(&raw));
            self.queue.write_buffer(
                &self.dynamic.bone_buf,
                0,
                bytemuck::cast_slice(&self.dynamic.bone_mats),
            );
        }
        let draw_vm = match &vm {
            Some(draw) if !self.vm_pass.models.is_empty() => {
                let to_view = glam::camera::rh::view::look_to_mat4(frame.eye, frame.fwd, frame.up);
                let lights = match self.dynamic.lighting.as_mut() {
                    Some(l) => {
                        let picked = l.entity_lights(draw.light_origin, &self.dynamic.scene_lights);
                        entity_light::pack(&picked, Some(to_view), self.identity_light)
                    }
                    None => GpuLightSet::default(),
                };
                let uniform = vm_uniform(draw.transform, draw.fov_x, self.aspect());
                self.queue.write_buffer(
                    &self.vm_pass.uniform_buf,
                    0,
                    bytemuck::cast_slice(&uniform),
                );
                self.queue.write_buffer(
                    &self.vm_pass.uniform_buf,
                    VM_LIGHTS_OFFSET,
                    bytemuck::bytes_of(&lights),
                );
                self.queue.write_buffer(
                    &self.vm_pass.uniform_buf,
                    VM_MISC_OFFSET,
                    bytemuck::cast_slice(&[self.identity_light, 0.0, 0.0, 0.0]),
                );
                for (i, model) in self.vm_pass.models.iter().enumerate() {
                    let Some(bones) = draw.bone_sets.get(i) else {
                        continue;
                    };
                    if bones.is_empty() {
                        continue;
                    }
                    debug_assert!(
                        bones.len() <= VM_BONE_COUNT,
                        "bone set exceeds the shader's {VM_BONE_COUNT}-matrix uniform"
                    );
                    let cols: Vec<[f32; 16]> =
                        bones.iter().map(glam::Mat4::to_cols_array).collect();
                    self.queue
                        .write_buffer(&model.bone_buf, 0, bytemuck::cast_slice(&cols));
                }
                true
            }
            _ => false,
        };

        let draw_vm = draw_vm && self.vm_pass.skins.len() == self.vm_pass.models.len();
        let vm_skins: &[Vec<Rc<EntSkin>>] = if draw_vm { &self.vm_pass.skins } else { &[] };
        let ent_draws = sorted_ent_draws(
            self.dynamic
                .instances
                .iter()
                .enumerate()
                .flat_map(|(i, (m, _))| {
                    self.dynamic.models[*m]
                        .skins
                        .iter()
                        .enumerate()
                        .map(move |(j, sk)| (i as u32, j as u32, &**sk))
                }),
            vm_skins.iter().enumerate().flat_map(|(i, skins)| {
                skins
                    .iter()
                    .enumerate()
                    .map(move |(j, sk)| (i as u32, j as u32, &**sk))
            }),
        );
        let surface_tex = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(tex)
            | wgpu::CurrentSurfaceTexture::Suboptimal(tex) => tex,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.reconfigure();
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                log::warn!("dropped frame: surface validation error");
                return;
            }
        };
        let view = surface_tex.texture.create_view(&Default::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        let shadow_on = self.encode_shadow_maps(&mut encoder, &frame, &camera, &moved);
        {
            // Linear fog clears to the fog colour (RTCW clearscreen=true);
            // otherwise the fixed steel blue stands in until the sky draws.
            let clear = if self.fog.set && self.fog.live.linear {
                wgpu::Color {
                    r: self.fog.live.color[0] as f64,
                    g: self.fog.live.color[1] as f64,
                    b: self.fog.live.color[2] as f64,
                    a: 1.0,
                }
            } else {
                wgpu::Color {
                    r: 0.35,
                    g: 0.42,
                    b: 0.50,
                    a: 1.0,
                }
            };
            let bloom = self.bloom_intensity > 0.0;
            // Shafts need a sun flare's direction (shader doc 8).
            let shafts = self
                .sky
                .as_ref()
                .and_then(|s| s.flare.as_ref())
                .is_some_and(|f| {
                    self.shafts.prepare(
                        &self.queue,
                        frame.view_proj,
                        f.dir(),
                        (self.config.width as f32, self.config.height as f32),
                        self.identity_light,
                    )
                });
            if let Some(flare) = self
                .sky
                .as_mut()
                .and_then(|s| s.flare.as_mut())
                .filter(|_| self.flares)
            {
                flare.prepare(
                    &self.device,
                    &self.queue,
                    &frame,
                    (self.config.width as f32, self.config.height as f32),
                    self.identity_light,
                );
            }
            // With SSAO or vc_shadows the frame is two passes split after
            // the last opaque or decal draw (`ssao_split`): the first keeps
            // its MSAA colour for the shadow mask and the SSAO multiply, the
            // second loads it and resolves.
            let targets = SceneTargets {
                msaa: &self.msaa_view,
                depth: &self.depth_view,
                resolve: self.gamma.scene_view(),
                timer: self.ssao.timer(),
                clear,
                sun_queries: self
                    .sky
                    .as_ref()
                    .and_then(|s| s.flare.as_ref())
                    .and_then(SunFlareGpu::query_set),
            };
            let mut pass = targets.begin(&mut encoder, true, !(ssao_on || shadow_on));
            pass.set_bind_group(0, &self.camera_bg, &[]);
            // The sky draws first so every world draw overwrites it; the box
            // and dome both ignore depth (cleared 1.0) and write none. CoD
            // keeps the sky visible under linear fog (VERIFIED live on
            // mp_ship, unlike RTCW's drawsky=false).
            if let (Some(world), Some(sky)) = (&self.world, &self.sky) {
                if let Some(farbox) = &sky.farbox {
                    pass.set_pipeline(&farbox.pipeline);
                    pass.set_bind_group(2, &farbox.eye_bg, &[]);
                    pass.set_vertex_buffer(0, farbox.vertex_buf.slice(..));
                    pass.set_index_buffer(farbox.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    for (axis, &(first, end)) in farbox.sides.iter().enumerate() {
                        pass.set_bind_group(1, &farbox.side_bgs[axis], &[]);
                        pass.draw_indexed(first..end, 0, 0..1);
                    }
                }
                // The sunfile disc: additive billboard along the sun
                // direction at farbox distance, camera-facing via world axes.
                if let Some(sun) = &sky.sun {
                    let center = frame.eye + sun.dir * sky::box_size(Z_FAR, Z_NEAR);
                    let up = glam::Vec3::Z;
                    let right = up.cross(sun.dir).normalize_or_zero();
                    let quad_up = sun.dir.cross(right);
                    let r = right * sun.half_extent;
                    let u = quad_up * sun.half_extent;
                    let corner = |dr: f32, du: f32| sky::SkyBoxVert {
                        pos: (center + r * dr + u * du).to_array(),
                        uv: [(dr + 1.0) * 0.5, 1.0 - (du + 1.0) * 0.5],
                    };
                    let verts = [
                        corner(-1.0, -1.0),
                        corner(1.0, -1.0),
                        corner(1.0, 1.0),
                        corner(-1.0, -1.0),
                        corner(1.0, 1.0),
                        corner(-1.0, 1.0),
                    ];
                    self.queue
                        .write_buffer(&sun.vertex_buf, 0, bytemuck::cast_slice(&verts));
                    pass.set_pipeline(&sun.pipeline);
                    pass.set_bind_group(1, &sun.bind_group, &[]);
                    pass.set_vertex_buffer(0, sun.vertex_buf.slice(..));
                    pass.draw(0..6, 0..1);
                }
                if let Some(dome) = &sky.dome {
                    pass.set_vertex_buffer(0, dome.vertex_buf.slice(..));
                    pass.set_index_buffer(dome.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                    let mut bound: Option<(StageVariant, bool)> = None;
                    for ds in &dome.stages {
                        let key = (ds.variant, ds.two_sided);
                        if bound != Some(key) {
                            pass.set_pipeline(self.stage_pipeline(ds.variant, ds.two_sided, false));
                            bound = Some(key);
                        }
                        pass.set_bind_group(1, ds.mat.bind_group(frame.time), &[]);
                        pass.set_bind_group(
                            2,
                            ds.stage.bind_group(frame.time),
                            &[
                                u32::try_from(u64::from(ds.slot) * world.stage_params_stride)
                                    .expect("stage params slot offset fits a dynamic offset"),
                            ],
                        );
                        pass.draw_indexed(0..dome.index_count, 0, 0..1);
                    }
                }
            }
            // The world's draws across the world and every moved brush
            // model, by their place in the sorted shader list; on one
            // shader a moved model, an entity in retail's key, draws before
            // the world (entity 0x3fe, shader doc 5).
            let mut world_list: Vec<(f32, u32, usize, u32)> = Vec::new();
            if let Some(world) = &self.world {
                for (set, (_, frame_draws, _)) in self.world_sets(world).enumerate() {
                    for (i, fd) in frame_draws.iter().enumerate() {
                        world_list.push((fd.sort, fd.rank, set, i as u32));
                    }
                }
            }
            // stable: a batch keeps its stage order
            world_list.sort_by_key(|&(_, rank, set, _)| (rank, set.wrapping_sub(1)));
            // One list, as retail's draw list keys every surface (shader doc
            // 5): the world's draws, the entity and viewmodel surfaces and
            // the fx runs merged by material sort, the world first on a
            // tie, then the models, then the fx. The gun keeps its
            // depth-hack range (renderfx 0x8c: the front fifth), so what
            // draws after it behind it fails the depth test. The flat list
            // mixes pipeline layouts; rebind only on change. An fx run
            // whose texture failed to resolve is skipped.
            let (vw, vh) = (self.config.width as f32, self.config.height as f32);
            let world_sorts: Vec<f32> = world_list.iter().map(|w| w.0).collect();
            let model_sorts: Vec<f32> = ent_draws.iter().map(|d| d.sort).collect();
            let fx_sorts: Vec<f32> = self.fx.runs.iter().map(|r| r.3).collect();
            let fx_firsts: Vec<usize> = self
                .fx
                .runs
                .iter()
                .scan(0, |first, run| {
                    let at = *first;
                    *first += run.2;
                    Some(at)
                })
                .collect();
            let mark_sorts: Vec<f32> = self.marks.runs.iter().map(|r| r.sort).collect();
            let items = merge_by_sort(&world_sorts, &mark_sorts, &model_sorts, &fx_sorts);
            let mut dlight_at = dlight_slot(&items, &world_sorts);
            let split_at = (ssao_on || shadow_on)
                .then(|| ssao_split(&items, &world_sorts, &mark_sorts, &model_sorts, &fx_sorts));
            // the dlights add light, so a shadow must not darken them
            if shadow_on && let Some(at) = split_at {
                dlight_at = dlight_at.max(at);
            }
            let mut emit = Emit::None;
            for k in 0..=items.len() {
                // Before the dlights, so their light is not occluded; the
                // shadow mask before the SSAO multiply.
                if split_at == Some(k) {
                    drop(pass);
                    if shadow_on && let Some(sp) = &self.shadows {
                        sp.encode_mask(
                            &self.device,
                            &mut encoder,
                            &self.msaa_view,
                            &self.depth_view,
                        );
                    }
                    if ssao_on {
                        self.ssao
                            .encode(&mut encoder, &self.msaa_view, &self.depth_view);
                    }
                    pass = targets.begin(&mut encoder, false, true);
                    pass.set_bind_group(0, &self.camera_bg, &[]);
                    emit = Emit::None;
                }
                if let Some(world) = &self.world
                    && dlight_at == k
                    && !world.dlight_runs.is_empty()
                {
                    if matches!(emit, Emit::Gun { .. }) {
                        pass.set_viewport(0.0, 0.0, vw, vh, 0.0, 1.0);
                    }
                    self.emit_dlights(&mut pass, world, frame.time);
                    emit = Emit::None;
                }
                let Some(&item) = items.get(k) else {
                    break;
                };
                match item {
                    SortedItem::World(wi) => {
                        let world = self.world.as_ref().expect("world draws come from a world");
                        let (_, _, set, i) = world_list[wi];
                        let (draws, frame_draws, cam) = self
                            .world_sets(world)
                            .nth(set)
                            .expect("collected from these sets");
                        let mut bound = match emit {
                            Emit::World { set: s, bound } if s == set => bound,
                            Emit::World { bound, .. } => {
                                pass.set_bind_group(0, cam, &[]);
                                bound
                            }
                            other => {
                                if matches!(other, Emit::Gun { .. }) {
                                    pass.set_viewport(0.0, 0.0, vw, vh, 0.0, 1.0);
                                }
                                pass.set_vertex_buffer(0, world.vertex_buf.slice(..));
                                pass.set_index_buffer(
                                    world.index_buf.slice(..),
                                    wgpu::IndexFormat::Uint32,
                                );
                                pass.set_bind_group(0, cam, &[]);
                                None
                            }
                        };
                        let fd = &frame_draws[i as usize];
                        self.emit_world_draw(
                            &mut pass,
                            world,
                            &draws[fd.src as usize],
                            fd.kind,
                            &mut bound,
                            frame.time,
                        );
                        emit = Emit::World { set, bound };
                    }
                    SortedItem::Model(mi) if !ent_draws[mi].gun => {
                        let d = &ent_draws[mi];
                        let dynamic = &self.dynamic;
                        let model = &dynamic.models[dynamic.instances[d.inst as usize].0];
                        let vm = &model.vm;
                        let (same_inst, mut bound) = match emit {
                            Emit::Ent { inst, bound } => (inst == d.inst, bound),
                            other => {
                                if matches!(other, Emit::Gun { .. }) {
                                    pass.set_viewport(0.0, 0.0, vw, vh, 0.0, 1.0);
                                }
                                pass.set_bind_group(0, &self.camera_bg, &[]);
                                pass.set_bind_group(2, &dynamic.bone_bg, &[]);
                                (false, None)
                            }
                        };
                        if !same_inst {
                            let off = u64::from(d.inst) * DYNAMIC_INSTANCE_STRIDE;
                            pass.set_vertex_buffer(0, vm.vertex_buf.slice(..));
                            pass.set_vertex_buffer(
                                1,
                                dynamic
                                    .instance_buf
                                    .slice(off..off + DYNAMIC_INSTANCE_STRIDE),
                            );
                            pass.set_index_buffer(
                                vm.index_buf.slice(..),
                                wgpu::IndexFormat::Uint16,
                            );
                        }
                        let surf = &vm.surfaces[d.surf as usize];
                        draw_ent_stages(
                            &mut pass,
                            &dynamic.materials,
                            EntPass::World,
                            &vm.bind_groups[surf.bind_group],
                            &model.skins[d.surf as usize],
                            surf,
                            &mut bound,
                        );
                        emit = Emit::Ent {
                            inst: d.inst,
                            bound,
                        };
                    }
                    SortedItem::Model(mi) => {
                        let d = &ent_draws[mi];
                        let vmp = &self.vm_pass;
                        let model = &vmp.models[d.inst as usize];
                        let (same_part, mut bound) = match emit {
                            Emit::Gun { part, bound } => (part == d.inst, bound),
                            _ => {
                                pass.set_viewport(0.0, 0.0, vw, vh, 0.0, VM_DEPTH_RANGE);
                                pass.set_bind_group(0, &vmp.uniform_bg, &[]);
                                (false, None)
                            }
                        };
                        if !same_part {
                            pass.set_vertex_buffer(0, model.vertex_buf.slice(..));
                            pass.set_index_buffer(
                                model.index_buf.slice(..),
                                wgpu::IndexFormat::Uint16,
                            );
                            pass.set_bind_group(2, &model.bone_bg, &[]);
                        }
                        let surf = &model.surfaces[d.surf as usize];
                        draw_ent_stages(
                            &mut pass,
                            &self.dynamic.materials,
                            EntPass::View,
                            &model.bind_groups[surf.bind_group],
                            &vmp.skins[d.inst as usize][d.surf as usize],
                            surf,
                            &mut bound,
                        );
                        emit = Emit::Gun {
                            part: d.inst,
                            bound,
                        };
                    }
                    SortedItem::Mark(ri) => {
                        if matches!(emit, Emit::Gun { .. }) {
                            pass.set_viewport(0.0, 0.0, vw, vh, 0.0, 1.0);
                        }
                        pass.set_bind_group(0, &self.camera_bg, &[]);
                        self.marks
                            .draw(&mut pass, ri, &self.stage_pipelines, frame.time);
                        emit = Emit::None;
                    }
                    SortedItem::Fx(fi) => {
                        let fx = &self.fx;
                        let (shader, additive, count, _) = &fx.runs[fi];
                        let Some(Some(bg)) = fx.textures.get(shader) else {
                            continue;
                        };
                        let bound = match emit {
                            Emit::Fx { additive } => Some(additive),
                            other => {
                                if matches!(other, Emit::Gun { .. }) {
                                    pass.set_viewport(0.0, 0.0, vw, vh, 0.0, 1.0);
                                }
                                pass.set_bind_group(0, &self.camera_bg, &[]);
                                pass.set_vertex_buffer(0, fx.vertex_buf.slice(..));
                                pass.set_index_buffer(
                                    fx.index_buf.slice(..),
                                    wgpu::IndexFormat::Uint32,
                                );
                                None
                            }
                        };
                        if bound != Some(*additive) {
                            pass.set_pipeline(if *additive {
                                &fx.additive_pipeline
                            } else {
                                &fx.pipeline
                            });
                        }
                        pass.set_bind_group(1, bg, &[]);
                        let start = (fx_firsts[fi] * 6) as u32;
                        let end = ((fx_firsts[fi] + count) * 6) as u32;
                        pass.draw_indexed(start..end, 0, 0..1);
                        emit = Emit::Fx {
                            additive: *additive,
                        };
                    }
                }
            }
            if matches!(emit, Emit::Gun { .. }) {
                pass.set_viewport(0.0, 0.0, vw, vh, 0.0, 1.0);
            }
            // The sun's flare and washes go over the finished scene, before
            // the 2D (0x4d9770).
            if let Some(flare) = self.sky.as_ref().and_then(|s| s.flare.as_ref())
                && self.flares
            {
                flare.draw(&mut pass);
            }
            pass.set_bind_group(0, &self.camera_bg, &[]);

            // The world is resolved: add the shafts, blur it, add the glow
            // into the resolved scene and draw the HUD there single-sampled,
            // so the HUD never blooms or shafts and the MSAA target is
            // never reloaded.
            let post = bloom || shafts;
            if post {
                drop(pass);
                if shafts {
                    self.shafts.encode(&mut encoder, self.gamma.scene_view());
                }
                if bloom {
                    self.bloom.set_params(
                        &self.queue,
                        self.bloom_intensity,
                        self.bloom_threshold,
                        self.identity_light,
                    );
                    self.bloom.blur(&mut encoder);
                }
                pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("hud pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: self.gamma.scene_view(),
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                if bloom {
                    self.bloom.composite(&mut pass);
                }
            }

            // HUD after the weapon so it sits on top, before the debug
            // overlay so F3 always wins.
            if !self.hud_pass.runs.is_empty() {
                let hud_pass = &self.hud_pass;
                pass.set_pipeline(if post {
                    &hud_pass.pipeline_1x
                } else {
                    &hud_pass.pipeline
                });
                pass.set_vertex_buffer(0, hud_pass.vertex_buf.slice(..));
                pass.set_index_buffer(hud_pass.index_buf.slice(..), wgpu::IndexFormat::Uint16);
                let mut first_quad = 0usize;
                for (texture, count) in &hud_pass.runs {
                    if let Some(Some(bg)) = hud_pass.textures.get(texture) {
                        pass.set_bind_group(0, bg, &[]);
                        let start = (first_quad * 6) as u32;
                        let end = ((first_quad + count) * 6) as u32;
                        pass.draw_indexed(start..end, 0, 0..1);
                    }
                    first_quad += count;
                }
            }

            // Debug HUD text, over everything.
            if hud_quads > 0 {
                let hud = &self.hud;
                pass.set_pipeline(if post {
                    &hud.pipeline_1x
                } else {
                    &hud.pipeline
                });
                pass.set_bind_group(0, &hud.bind_group, &[]);
                pass.set_vertex_buffer(0, hud.vertex_buf.slice(..));
                pass.set_index_buffer(hud.index_buf.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..(hud_quads * 6) as u32, 0, 0..1);
            }
        }
        if shadow_on && let Some(sp) = &mut self.shadows {
            sp.resolve_timer(&mut encoder);
        }
        if let Some(f) = self.sky.as_mut().and_then(|s| s.flare.as_mut()) {
            f.resolve(&mut encoder);
        }
        if let Some(t) = self.ssao.timer_mut() {
            t.resolve(&mut encoder, ssao_on);
        }
        self.gamma.draw(&mut encoder, &view);
        self.queue.submit([encoder.finish()]);
        if let Some(sp) = &mut self.shadows {
            sp.after_submit();
        }
        if let Some(f) = self.sky.as_mut().and_then(|s| s.flare.as_mut()) {
            f.after_submit();
        }
        self.queue.present(surface_tex);
        if let Some(t) = self.ssao.timer_mut() {
            t.collect(&self.device);
        }
    }
}

/// `vc_shadows`' static casters: the soups and props that draw opaque
/// (decals and polygon-offset layers lie on other surfaces).
fn shadow_casters(device: &wgpu::Device, world: &WorldGpu) -> shadows::Casters {
    let casts = |r: &IndexRange| {
        let b = r.batch as usize;
        !matches!(world.batch_draws[b].pass, Pass::Layer | Pass::Overlay)
            && world.batch_keys.get(b).is_none_or(|k| k.0 <= SORT_OPAQUE)
    };
    let range = |r: &IndexRange, bounds| shadows::CasterRange {
        first: r.first,
        count: r.count,
        bounds,
    };
    let mut models: Vec<Vec<shadows::CasterRange>> =
        world.model_soups.iter().map(|_| Vec::new()).collect();
    let mut flat = Vec::new();
    for (si, r) in world.soup_ranges.iter().enumerate() {
        let Some(r) = r.filter(casts) else {
            continue;
        };
        let c = range(&r, world.soup_bounds[si]);
        match world.soup_model[si] as usize {
            0 => flat.push(c),
            m => models[m].push(c),
        }
    }
    for (p, r) in &world.prop_ranges {
        let bounds = world.prop_bounds[*p as usize];
        if casts(r) && bounds.0 != bounds.1 {
            flat.push(range(r, bounds));
        }
    }
    shadows::Casters::build(device, &world.cpu_indices, &flat, &models)
}

fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: MSAA_SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            // the SSAO passes read it between the frame's two halves
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

fn create_msaa_view(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("msaa color"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: MSAA_SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

fn create_fx_pass(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    camera_layout: &wgpu::BindGroupLayout,
    texture_layout: &wgpu::BindGroupLayout,
) -> FxPass {
    // ClampToEdge: repeat would bleed the opposite edge into a sprite's rim.
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("fx sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });

    let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fx vertices"),
        size: (MAX_FX_QUADS * 4 * std::mem::size_of::<FxVert>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    // Quad n spans vertices 4n..4n+4. u32 because MAX_FX_QUADS * 4 can
    // exceed u16::MAX.
    let indices: Vec<u32> = (0..MAX_FX_QUADS)
        .flat_map(|q| {
            let b = (q * 4) as u32;
            [b, b + 1, b + 2, b, b + 2, b + 3]
        })
        .collect();
    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("fx indices"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    let shader = device.create_shader_module(wgpu::include_wgsl!("fx.wgsl"));
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("fx pipeline layout"),
        bind_group_layouts: &[Some(camera_layout), Some(texture_layout)],
        immediate_size: 0,
    });
    // The two pipelines must differ only in blend and fragment entry so runs
    // can interleave within one pass.
    let make = |label, fs_entry, blend| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<FxVert>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3, // pos
                        1 => Float32x2, // uv
                        2 => Float32x4, // rgba
                    ],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(fs_entry),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                // Decal tangent frames flip with the surface normal and
                // billboards face the camera; neither has a meaningful winding.
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                // The world must occlude fx quads, but overlapping quads must all draw.
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: MSAA_SAMPLES,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        })
    };

    let pipeline = make("fx pipeline", "fs_main", wgpu::BlendState::ALPHA_BLENDING);
    // `GL_ONE GL_ONE`, as the effect materials' blendFunc. The fragment
    // writes alpha 0 so additive quads never inflate the target's alpha.
    let additive_pipeline = make(
        "fx additive pipeline",
        "fs_additive",
        wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        },
    );

    FxPass {
        pipeline,
        additive_pipeline,
        vertex_buf,
        index_buf,
        sampler,
        textures: HashMap::new(),
        runs: Vec::new(),
    }
}

/// Resolves an fx shader name to a loadable texture path. `load_path_image`
/// never fails (a missing file becomes the default image), so unresolvable
/// names must be caught here or they draw default-image quads. `.efx` names
/// are usually extensionless material names, probed like
/// `assets::probe_image_path`. A name with a known extension is tried as-is
/// first; if that file is missing the other extensions are probed too, since
/// weapon `killIcon`s say `.tga` while the art ships as `.dds`
/// (docs/research/cod11-hud-protocol.md, section 2).
/// What a quad's vertex colour is scaled by on its way into the
/// framebuffer, from the material's first stage: the lit gens (`vertex`,
/// `identityLighting`, `constLighting`, `wave`) take `identityLight`,
/// `identity`, `exactVertex` and `const` none. A material with no script is
/// implicit 2D or fx, `rgbGen vertex` (0x4fc440 case -4); see
/// docs/research/cod11-gamma.md, section 5.
fn colour_scale(shader: Option<&Shader>, identity_light: f32) -> f32 {
    match shader
        .and_then(|sh| sh.stages.first())
        .map(|st| &st.rgb_gen)
    {
        Some(RgbGen::Identity | RgbGen::ExactVertex | RgbGen::Const(_)) => 1.0,
        _ => identity_light,
    }
}

/// A HUD colour is a display value; `scale` (`colour_scale`) takes it into
/// the framebuffer, and the ramp's overbright doubling brings a lit gen back
/// on display. Alpha is a coverage weight.
fn hud_colour([r, g, b, a]: [f32; 4], scale: f32) -> [f32; 4] {
    [r * scale, g * scale, b * scale, a]
}

fn resolve_fx_path(
    shader_images: &HashMap<String, String>,
    fs: &Pk3Fs,
    name: &str,
) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    let path = shader_images.get(name).map(String::as_str).unwrap_or(name);
    let ext_at = path.rfind('.');
    let has_known_ext = ext_at.is_some_and(|i| {
        assets::IMAGE_EXTS
            .iter()
            .any(|known| *known == path[i..].to_lowercase())
    });
    if has_known_ext {
        if fs.contains(path) {
            return Some(path.to_string());
        }
        let base = &path[..ext_at.unwrap()];
        return assets::probe_image_path(fs, base);
    }
    assets::probe_image_path(fs, path)
}

/// `upload_image` handles both RGBA sprites and BC `.dds` mip chains (many
/// effect textures are DXT5). Unresolvable names warn once; the caller
/// caches the `None`.
#[allow(clippy::too_many_arguments)]
fn resolve_fx_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    fs: &Pk3Fs,
    shader_images: &HashMap<String, String>,
    name: &str,
    table: Option<&[u8; 256]>,
) -> Option<wgpu::BindGroup> {
    let Some(path) = resolve_fx_path(shader_images, fs, name) else {
        log::warn!("fx shader {name:?}: no texture found for it, dropping its quads");
        return None;
    };
    let mut img = assets::load_path_image(fs, &path);
    crate::gamma::bake_image(&mut img, table);
    let view = upload_image(device, queue, name, &img);
    Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(name),
        layout: texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    }))
}

fn create_hud_text_pass(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
) -> HudTextPass {
    // Nearest sampling: glyphs draw at integer scales, filtering would blur them.
    let atlas = hud_text::build_atlas();
    let size = wgpu::Extent3d {
        width: hud_text::ATLAS_W as u32,
        height: hud_text::ATLAS_H as u32,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("hud font atlas"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &atlas,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(hud_text::ATLAS_W as u32),
            rows_per_image: None,
        },
        size,
    );
    let view = texture.create_view(&Default::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("hud font sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });

    let bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("hud text layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("hud text"),
        layout: &bg_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });

    // Two quads per glyph (shadow + text); quad n spans vertices 4n..4n+4.
    const MAX_QUADS: usize = MAX_HUD_GLYPHS * 2;
    let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("hud text vertices"),
        size: (MAX_QUADS * 4 * std::mem::size_of::<HudVert>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let indices: Vec<u16> = (0..MAX_QUADS)
        .flat_map(|q| {
            let b = (q * 4) as u16;
            [b, b + 1, b + 2, b, b + 2, b + 3]
        })
        .collect();
    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("hud text indices"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    let shader = device.create_shader_module(wgpu::include_wgsl!("hud_text.wgsl"));
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("hud text pipeline layout"),
        bind_group_layouts: &[Some(&bg_layout)],
        immediate_size: 0,
    });
    // MSAA in the map pass; single-sample with no depth in the pass after
    // the bloom, which draws straight into the resolved scene.
    let make = |samples: u32| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hud text pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<HudVert>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2, // pos (clip space)
                        1 => Float32x2, // uv
                        2 => Float32x4, // color
                    ],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: (samples > 1).then(|| wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                // depth ignored, draws over everything
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        })
    };
    let (pipeline, pipeline_1x) = (make(MSAA_SAMPLES), make(1));

    HudTextPass {
        pipeline,
        pipeline_1x,
        vertex_buf,
        index_buf,
        bind_group,
    }
}

/// Like `create_hud_text_pass` but with `hud.wgsl` and a linear sampler:
/// font pages and icons draw at scale and would alias under nearest sampling.
fn create_hud_pass(device: &wgpu::Device, format: wgpu::TextureFormat) -> HudPass {
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("hud sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("hud texture layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    // Quad n spans vertices 4n..4n+4.
    let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("hud vertices"),
        size: (MAX_HUD_QUADS * 4 * std::mem::size_of::<HudVert>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let indices: Vec<u16> = (0..MAX_HUD_QUADS)
        .flat_map(|q| {
            let b = (q * 4) as u16;
            [b, b + 1, b + 2, b, b + 2, b + 3]
        })
        .collect();
    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("hud indices"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    let shader = device.create_shader_module(wgpu::include_wgsl!("hud.wgsl"));
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("hud pipeline layout"),
        bind_group_layouts: &[Some(&texture_layout)],
        immediate_size: 0,
    });
    // MSAA in the map pass; single-sample with no depth in the pass after
    // the bloom, which draws straight into the resolved scene.
    let make = |samples: u32| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hud pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<HudVert>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2, // pos (clip space)
                        1 => Float32x2, // uv
                        2 => Float32x4, // color
                    ],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: (samples > 1).then(|| wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                // depth ignored, draws over everything
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        })
    };
    let (pipeline, pipeline_1x) = (make(MSAA_SAMPLES), make(1));

    HudPass {
        pipeline,
        pipeline_1x,
        vertex_buf,
        index_buf,
        sampler,
        texture_layout,
        textures: HashMap::new(),
        runs: Vec::new(),
        quad_count: 0,
    }
}

/// Where a HUD texture name resolves. `white` and `$whiteimage` are the
/// engine built-in; `black` (pak4 `scripts/hud.shader`: `map $whiteimage`,
/// `rgbGen const 0`) shares the same texture, and `set_hud_quads` forces the
/// quad colour to black for it.
#[derive(Debug, PartialEq, Eq)]
enum HudTex {
    White,
    Black,
    Path(String),
    Missing,
}

/// A material name (`hudStanceStand`, `hintHealth`) resolves through the
/// shader-script image map, same as fx; anything else is a bare pk3 path.
/// `ShaderLib::load` folds every material name to lowercase, so the lookup
/// does too before falling back to the name as given.
fn hud_texture_path(name: &str, image_map: &HashMap<String, String>, fs: &Pk3Fs) -> HudTex {
    match name {
        "white" | "$whiteimage" => return HudTex::White,
        "black" => return HudTex::Black,
        _ => {}
    }
    let path = image_map
        .get(name)
        .or_else(|| image_map.get(&name.to_ascii_lowercase()))
        .map(String::as_str)
        .unwrap_or(name);
    match resolve_fx_path(&HashMap::new(), fs, path) {
        Some(p) => HudTex::Path(p),
        None => HudTex::Missing,
    }
}

/// The bind group for a HUD texture name ([`hud_texture_path`]). Unresolvable
/// names warn once; the caller caches the `None`.
fn resolve_hud_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    fs: &Pk3Fs,
    image_map: &HashMap<String, String>,
    name: &str,
) -> Option<wgpu::BindGroup> {
    let img = match hud_texture_path(name, image_map, fs) {
        HudTex::White | HudTex::Black => assets::white_1x1(),
        HudTex::Path(path) => assets::load_path_image(fs, &path),
        HudTex::Missing => {
            log::warn!("hud: no texture found for {name:?}, dropping its quads");
            return None;
        }
    };
    let view = upload_image(device, queue, name, &img);
    Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(name),
        layout: texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    }))
}

/// The viewmodel's uniform, skin and bone layouts and buffers; its
/// pipelines are the entity materials' `EntPass::View` ones, built with the
/// dynamic pass.
fn create_vm_pass(device: &wgpu::Device) -> VmPass {
    let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("viewmodel uniform layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(VM_UNIFORM_SIZE),
            },
            count: None,
        }],
    });
    let skin_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("viewmodel skin layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let bone_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("viewmodel bone layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(VM_BONE_BUF_SIZE),
            },
            count: None,
        }],
    });
    let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("viewmodel uniform"),
        size: VM_UNIFORM_SIZE,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("viewmodel uniform bind group"),
        layout: &uniform_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buf.as_entire_binding(),
        }],
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("viewmodel skin sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        anisotropy_clamp: 16,
        ..Default::default()
    });

    VmPass {
        uniform_layout,
        uniform_buf,
        uniform_bg,
        skin_layout,
        bone_layout,
        sampler,
        models: Vec::new(),
        skins: Vec::new(),
        cache: HashMap::new(),
    }
}

/// Group 0 is the camera, group 1 the viewmodel skin layout (the uploaded
/// models' bind groups are built against it), group 2 the bone storage
/// buffer, group 3 the stage's `StageParams`. Vertex buffer 1 is
/// instance-step `InstanceRaw`. The entity materials also build the
/// viewmodel's pipelines over `vm`'s layouts.
fn create_dynamic_pass(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    camera_layout: &wgpu::BindGroupLayout,
    vm: &VmPass,
    white_view: &wgpu::TextureView,
) -> DynamicPass {
    let instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dynamic instance transforms"),
        size: MAX_DYNAMIC_INSTANCES as u64 * DYNAMIC_INSTANCE_STRIDE,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // Worst case: every instance skinned at the cap, plus the identity block.
    let storage = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    // Binding 1 holds the frame's light sets.
    let bone_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("dynamic bone layout"),
        entries: &[storage(0), storage(1)],
    });
    let bone_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dynamic bone matrices"),
        size: (MAX_DYNAMIC_INSTANCES + 1) as u64 * MAX_INSTANCE_BONES as u64 * 64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let light_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dynamic light sets"),
        size: MAX_DYNAMIC_INSTANCES as u64 * std::mem::size_of::<GpuLightSet>() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bone_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("dynamic bone bind group"),
        layout: &bone_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: bone_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: light_buf.as_entire_binding(),
            },
        ],
    });

    let materials = EntityMaterials::new(
        device,
        format,
        PassLayouts {
            camera: camera_layout,
            bones: &bone_layout,
            view_uniform: &vm.uniform_layout,
            view_bones: &vm.bone_layout,
        },
        &vm.skin_layout,
        &vm.sampler,
        white_view,
    );

    DynamicPass {
        instance_buf,
        bone_layout,
        bone_buf,
        bone_bg,
        models: Vec::new(),
        instances: Vec::new(),
        bone_mats: Vec::new(),
        light_origins: Vec::new(),
        light_buf,
        lighting: None,
        scene_lights: Vec::new(),
        entity_scene_lights: Vec::new(),
        materials,
    }
}

/// The projection and motion transform; the light set follows at 128.
fn vm_uniform(model: glam::Mat4, fov_x: f32, aspect: f32) -> [f32; 32] {
    let proj = crate::camera::perspective(fov_x, aspect, VM_NEAR, VM_FAR);
    let mut out = [0.0f32; 32];
    out[..16].copy_from_slice(&proj.to_cols_array());
    out[16..32].copy_from_slice(&model.to_cols_array());
    out
}

/// `None` when nothing is drawable (empty buffers are invalid to create).
/// `load_image` resolves a material name; the callers name materials
/// differently (skin filenames vs `textures/...`).
fn upload_vm_model(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    vm: &VmPass,
    model_surfaces: &[xmodel::Surface],
    materials: &[String],
    load_image: &dyn Fn(&str) -> Image,
) -> Option<VmModel> {
    let mut verts: Vec<VmVert> = Vec::new();
    let mut indices: Vec<u16> = Vec::new();
    let mut bind_groups = Vec::with_capacity(model_surfaces.len());
    let mut surfaces = Vec::with_capacity(model_surfaces.len());
    let mut skins = Vec::with_capacity(model_surfaces.len());
    for surf in model_surfaces {
        if surf.verts.is_empty() || surf.indices.is_empty() {
            continue;
        }
        let skin = materials
            .get(surf.material)
            .map(String::as_str)
            .unwrap_or_default();
        let img = load_image(skin);
        let view = upload_image(device, queue, skin, &img);
        bind_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewmodel skin bind group"),
            layout: &vm.skin_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&vm.sampler),
                },
            ],
        }));
        skins.push(skin.to_string());
        surfaces.push(VmSurface {
            first_index: indices.len() as u32,
            index_count: surf.indices.len() as u32,
            base_vertex: verts.len() as i32,
            bind_group: bind_groups.len() - 1,
        });
        verts.extend_from_slice(&surf.verts);
        indices.extend_from_slice(&surf.indices);
    }
    if surfaces.is_empty() {
        return None;
    }
    // Identity bones: skinning is a no-op until an animation is sampled.
    let identity: Vec<[f32; 16]> = vec![glam::Mat4::IDENTITY.to_cols_array(); VM_BONE_COUNT];
    let bone_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("viewmodel bone matrices"),
        contents: bytemuck::cast_slice(&identity),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let bone_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("viewmodel bone bind group"),
        layout: &vm.bone_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: bone_buf.as_entire_binding(),
        }],
    });
    Some(VmModel {
        vertex_buf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("viewmodel vertices"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        }),
        index_buf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("viewmodel indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        }),
        bind_groups,
        surfaces,
        skins,
        bone_buf,
        bone_bg,
    })
}

fn wgpu_format(f: assets::TextureFormat) -> wgpu::TextureFormat {
    match f {
        assets::TextureFormat::Bc1RgbaUnorm => wgpu::TextureFormat::Bc1RgbaUnorm,
        assets::TextureFormat::Bc2RgbaUnorm => wgpu::TextureFormat::Bc2RgbaUnorm,
        assets::TextureFormat::Bc3RgbaUnorm => wgpu::TextureFormat::Bc3RgbaUnorm,
    }
}

/// BC mip chain as-is, or a single RGBA level.
pub(crate) fn upload_image(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    img: &Image,
) -> wgpu::TextureView {
    match &img.data {
        ImageData::Rgba8(px) => upload_rgba(device, queue, label, img.width, img.height, px),
        ImageData::Bc { format, mips } => {
            let block_size = format.block_size();
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: img.width,
                    height: img.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: mips.len() as u32,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu_format(*format),
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            for (level, data) in mips.iter().enumerate() {
                let w = (img.width >> level).max(1);
                let h = (img.height >> level).max(1);
                // Copy extents are whole blocks; the 2x2 and 1x1 tail mips
                // still occupy one 4x4 block each.
                let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: level as u32,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(bw * block_size),
                        rows_per_image: Some(bh),
                    },
                    wgpu::Extent3d {
                        width: bw * 4,
                        height: bh * 4,
                        depth_or_array_layers: 1,
                    },
                );
            }
            texture.create_view(&Default::default())
        }
    }
}

fn upload_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    width: u32,
    height: u32,
    px: &[u8],
) -> wgpu::TextureView {
    upload_texel4(
        device,
        queue,
        label,
        width,
        height,
        wgpu::TextureFormat::Rgba8Unorm,
        px,
    )
}

/// One mip of four-byte texels in `format`.
pub(crate) fn upload_texel4(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    px: &[u8],
) -> wgpu::TextureView {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        px,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        size,
    );
    texture.create_view(&Default::default())
}

/// Resolves a bundle image path through the asset probe chain; a missing
/// file warns and lands in the default image. Views are deduped per load.
fn upload_bundle_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    fs: &Pk3Fs,
    cache: &mut BundleViews,
    path: &str,
) -> wgpu::TextureView {
    if let Some(v) = cache.views.get(path) {
        return v.clone();
    }
    let mut img = if path == "$dlight" {
        // engine-generated light blob, never a file on disk (research doc §8)
        assets::dlight_blob()
    } else if path == "textures/battleship/deckflag_np.tga" {
        // ships in no stock pak; retail binds its default image there too
        assets::white_1x1()
    } else {
        match assets::resolve_bundle_image(fs, path) {
            Some(full) => assets::load_path_image(fs, &full),
            None => {
                log::warn!("shader image {path} not found, using the default image");
                assets::default_image()
            }
        }
    };
    // Retail generates the blob unmipped at its native size, which skips
    // the texture gamma (docs/research/cod11-gamma.md, section 3).
    if path != "$dlight" {
        crate::gamma::bake_image(&mut img, cache.table);
    }
    let v = upload_image(device, queue, path, &img);
    cache.views.insert(path.to_string(), v.clone());
    v
}

/// Shader-stage images uploaded during one world load, and the texture
/// gamma they load through (`r_ignorehwgamma 1`, `gamma::bake_image`).
struct BundleViews<'a> {
    views: HashMap<String, wgpu::TextureView>,
    table: Option<&'a [u8; 256]>,
}

fn fallback_pixels() -> Vec<u8> {
    match assets::default_image().data {
        ImageData::Rgba8(px) => px,
        _ => Vec::new(),
    }
}

/// True for `load_material_image`'s default-image placeholder.
fn is_fallback(img: &Image, fallback_px: &[u8]) -> bool {
    img.width == 16
        && img.height == 16
        && matches!(&img.data, ImageData::Rgba8(px) if px == fallback_px)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Mat4;
    use vcod_common::shader::{BlendFactor, Bundle, DepthFunc, Stage, TcMod, Wave, WaveForm};

    #[test]
    fn gather_copies_visible_ranges_batch_by_batch() {
        use vcod_common::mesh::IndexRange;
        let src: Vec<u32> = (0..12).collect();
        let ranges = [
            IndexRange {
                batch: 1,
                first: 6,
                count: 3,
            },
            IndexRange {
                batch: 0,
                first: 0,
                count: 3,
            },
            IndexRange {
                batch: 1,
                first: 9,
                count: 3,
            },
        ];
        let mut out = Vec::new();
        let mut scratch = Vec::new();
        let draws = gather(&src, ranges, 3, &mut out, &mut scratch);
        assert_eq!(out, vec![0, 1, 2, 6, 7, 8, 9, 10, 11]);
        assert_eq!(
            draws,
            vec![
                DrawRange {
                    batch: 0,
                    first: 0,
                    count: 3
                },
                DrawRange {
                    batch: 1,
                    first: 3,
                    count: 6
                },
            ]
        );
        let draws = gather(&src, [], 3, &mut out, &mut scratch);
        assert!(draws.is_empty() && out.is_empty());
    }

    #[test]
    fn packs_identity_block_and_bone_bases() {
        let id = Mat4::IDENTITY.to_cols_array();
        let bones_a = vec![Mat4::from_translation(glam::Vec3::X); 3];
        let items = [
            (0usize, id, None),                // static: identity block
            (1, id, Some(bones_a.as_slice())), // skinned, 3 bones
            (0, id, None),
        ];
        let (raw, mats) = pack_instances(&items);
        assert_eq!(raw.len(), 3);
        assert_eq!(raw[0].bone_base, 0);
        assert_eq!(raw[2].bone_base, 0);
        assert_eq!(raw[1].bone_base, MAX_INSTANCE_BONES as u32);
        assert_eq!(mats.len(), MAX_INSTANCE_BONES + 3);
        assert_eq!(mats[0], Mat4::IDENTITY.to_cols_array());
        assert_eq!(mats[MAX_INSTANCE_BONES], bones_a[0].to_cols_array());
    }

    #[test]
    fn hud_colour_scales_rgb_by_identity_light_and_keeps_alpha() {
        assert_eq!(hud_colour([0.0, 1.0, 0.5, 0.6], 0.5), [0.0, 0.5, 0.25, 0.6]);
        assert_eq!(hud_colour([0.15; 4], 1.0), [0.15; 4]);
    }

    /// Lit gens and implicit materials take identityLight; identity,
    /// exactVertex and const draw their colour as it is (0x4ffa60).
    #[test]
    fn colour_scale_follows_the_first_stage_gen() {
        let with = |rgb_gen: RgbGen| Shader {
            name: "t".into(),
            stages: vec![Stage {
                bundles: vec![],
                blend: None,
                depth_write: None,
                alpha_func: None,
                depth_func: DepthFunc::LessEqual,
                rgb_gen,
                alpha_gen: AlphaGen::Identity,
            }],
            ..Default::default()
        };
        assert_eq!(colour_scale(None, 0.5), 0.5);
        assert_eq!(colour_scale(Some(&with(RgbGen::Vertex)), 0.5), 0.5);
        assert_eq!(colour_scale(Some(&with(RgbGen::ExactVertex)), 0.5), 1.0);
        assert_eq!(colour_scale(Some(&with(RgbGen::Identity)), 0.5), 1.0);
        assert_eq!(
            colour_scale(Some(&with(RgbGen::IdentityLighting)), 1.0),
            1.0
        );
    }

    #[test]
    fn resolve_fx_path_drops_empty_and_missing_names_but_keeps_a_known_texture() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let no_mapping = HashMap::new();

        // An emitter with no `shaders` key hands over an empty string.
        assert_eq!(resolve_fx_path(&no_mapping, &fs, ""), None);

        assert_eq!(
            resolve_fx_path(&no_mapping, &fs, "fx/does/not/exist.tga"),
            None
        );

        assert_eq!(
            resolve_fx_path(&no_mapping, &fs, "gfx/impact/bullethole1.tga"),
            Some("gfx/impact/bullethole1.tga".to_string())
        );

        // Extensionless material name, as fx::sim hands over.
        assert_eq!(
            resolve_fx_path(&no_mapping, &fs, "gfx/impact/bullethole1"),
            Some("gfx/impact/bullethole1.tga".to_string())
        );

        assert_eq!(resolve_fx_path(&no_mapping, &fs, "fx/does/not/exist"), None);

        let mut mapped = HashMap::new();
        mapped.insert(
            "some_material".to_string(),
            "gfx/impact/bullethole1.tga".to_string(),
        );
        assert_eq!(
            resolve_fx_path(&mapped, &fs, "some_material"),
            Some("gfx/impact/bullethole1.tga".to_string())
        );
    }

    #[test]
    fn resolve_fx_path_resolves_default_hit_efx_shader_names() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let no_mapping = HashMap::new();
        for name in [
            "gfx/impact/bullethole1",
            "gfx/impact/dustlayer1",
            "gfx/impact/sparkflash",
            "gfx/impact/stone_piece1",
            "gfx/impact/stone_piece2",
        ] {
            assert!(
                resolve_fx_path(&no_mapping, &fs, name).is_some(),
                "{name} should resolve to a real texture, not drop its quads"
            );
        }
    }

    /// Weapon `killIcon`s name `.tga` while the art ships as `.dds`.
    #[test]
    fn resolve_fx_path_falls_back_to_the_real_extension_when_the_named_one_is_missing() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let no_mapping = HashMap::new();
        assert_eq!(
            resolve_fx_path(&no_mapping, &fs, "gfx/hud/hud@death_kar98.tga"),
            Some("gfx/hud/hud@death_kar98.dds".to_string())
        );
    }

    /// `gfx/misc/tracer`'s image lives under a different path and
    /// `gfx/effects/whitesmoke`'s is written with a leading slash; both only
    /// resolve through the fxshaders material map.
    #[test]
    fn resolve_fx_path_uses_fxshaders_materials() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let shaders = vcod_common::assets::load_shaders(&fs);
        assert_eq!(
            resolve_fx_path(shaders.image_map(), &fs, "gfx/misc/tracer"),
            Some("textures/sfx/tracer.jpg".to_string())
        );
        assert_eq!(
            resolve_fx_path(shaders.image_map(), &fs, "gfx/effects/whitesmoke"),
            Some("gfx/effects/whitesmoke.tga".to_string())
        );
        for name in [
            "gfx/effects/muzflash2",
            "gfx/effects/muzflash2a",
            "gfx/effects/mg42a",
            "gfx/misc/tracer",
        ] {
            assert!(
                resolve_fx_path(shaders.image_map(), &fs, name).is_some(),
                "{name} should resolve"
            );
            assert!(shaders.is_additive(name), "{name} should be additive");
        }
    }

    #[test]
    fn resolve_fx_path_resolves_bc_compressed_effect_textures() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let no_mapping = HashMap::new();
        for name in ["gfx/effects/pjsmoke", "gfx/impact/cratered"] {
            let path = resolve_fx_path(&no_mapping, &fs, name);
            assert!(path.is_some(), "{name} should resolve to a real texture");
            let img = vcod_common::assets::load_path_image(&fs, path.as_ref().unwrap());
            assert!(
                matches!(img.data, ImageData::Bc { .. }),
                "{name} should decode as BC-compressed, exercising the upload_image path"
            );
        }
    }

    /// `white` and `$whiteimage` are the engine built-in; `black` shares its
    /// texture (pak4 `scripts/hud.shader`: both stage `map $whiteimage`,
    /// `black` differs only by `rgbGen const 0`, which `set_hud_quads`
    /// applies to the quad colour).
    #[test]
    fn hud_texture_path_resolves_the_white_and_black_built_ins() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let no_mapping = HashMap::new();
        assert_eq!(hud_texture_path("white", &no_mapping, &fs), HudTex::White);
        assert_eq!(
            hud_texture_path("$whiteimage", &no_mapping, &fs),
            HudTex::White
        );
        assert_eq!(hud_texture_path("black", &no_mapping, &fs), HudTex::Black);
    }

    #[test]
    fn hud_texture_path_resolves_a_material_a_plain_path_and_a_miss() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let shaders = vcod_common::assets::load_shaders(&fs);
        assert_eq!(
            hud_texture_path("hudStanceStand", shaders.image_map(), &fs),
            HudTex::Path("gfx/hud/stance_stand.dds".to_string())
        );
        assert_eq!(
            hud_texture_path("gfx/hud/hud@compassback", &HashMap::new(), &fs),
            HudTex::Path("gfx/hud/hud@compassback.tga".to_string())
        );
        assert_eq!(
            hud_texture_path("gfx/hud/does_not_exist", &HashMap::new(), &fs),
            HudTex::Missing
        );
    }

    #[test]
    fn over_cap_bone_set_truncates() {
        let id = Mat4::IDENTITY.to_cols_array();
        let big = vec![Mat4::IDENTITY; MAX_INSTANCE_BONES + 40];
        let (raw, mats) = pack_instances(&[(0, id, Some(big.as_slice()))]);
        assert_eq!(raw[0].bone_base, MAX_INSTANCE_BONES as u32);
        assert_eq!(mats.len(), 2 * MAX_INSTANCE_BONES); // identity block + truncated set
    }

    // ---- StageParams ----

    #[test]
    fn stage_params_is_std140_shaped_and_pod_roundtrips() {
        assert_eq!(std::mem::size_of::<StageParams>(), 192);
        let p = StageParams {
            uv0: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            uv1: [7.0, 8.0, 9.0, 10.0, 11.0, 12.0],
            turb01: [13.0, 14.0, 15.0, 16.0],
            tint: [17.0, 18.0, 19.0, 20.0],
            flags: STAGE_FLAG_VERTEX_RGB | STAGE_FLAG_VERTEX_ALPHA,
            _pad: [0; 3],
            vec0_s: [1.0, 0.0, 0.0, 0.0],
            vec0_t: [0.0, 1.0, 0.0, 0.0],
            vec1_s: [0.0, 0.0, 1.0, 0.0],
            vec1_t: [0.0, 0.0, 0.0, 1.0],
            eye_off: [9.0, 8.0, 7.0, 0.0],
            wave: [1.0, 2.0, 3.0, 4.0],
        };
        // slot-shaped write/read, like the UBO dynamic-offset path
        let mut slot = [0u8; std::mem::size_of::<StageParams>()];
        slot[..bytemuck::bytes_of(&p).len()].copy_from_slice(bytemuck::bytes_of(&p));
        let q: &StageParams = bytemuck::try_from_bytes(&slot).unwrap();
        assert_eq!(*q, p);
    }

    /// Sky dome stages ride the same UBO slots with the eye-offset flag set.
    #[test]
    fn stage_params_eye_offset_flag_marks_dome_draws() {
        let sh = Shader {
            name: "test/skydome".into(),
            stages: vec![Stage {
                bundles: vec![Bundle {
                    image: ImageRef::Path("clouds".into()),
                    anim: None,
                    clamp: false,
                    tcmods: vec![TcMod::Scroll(0.004, 0.004), TcMod::Scale(3.0, 3.0)],
                    tc_gen: TcGen::Base,
                    env: TexEnv::Modulate,
                }],
                blend: Some((BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha)),
                depth_write: None,
                alpha_func: None,
                depth_func: DepthFunc::LessEqual,
                rgb_gen: RgbGen::Identity,
                alpha_gen: AlphaGen::Identity,
            }],
            ..Default::default()
        };
        let mut p = stage_params(&sh, 0, 5.0).unwrap();
        p.flags |= STAGE_FLAG_EYE_OFFSET;
        p.eye_off = [100.0, 200.0, 300.0, 0.0];
        assert_eq!(p.flags & STAGE_FLAG_EYE_OFFSET, STAGE_FLAG_EYE_OFFSET);
        // the affine still carries scroll+scale for the dome's raw-radian uvs
        assert_eq!(
            p.uv0,
            bundle_affine(&[TcMod::Scroll(0.004, 0.004), TcMod::Scale(3.0, 3.0)], 5.0)
        );
        // world draws never set the flag; the default params stay put
        let world_p = stage_params(&sh, 0, 5.0).unwrap();
        assert_eq!(world_p.flags & STAGE_FLAG_EYE_OFFSET, 0);
        assert_eq!(world_p.eye_off, [0.0; 4]);
    }

    #[test]
    fn stage_params_encodes_const_gens_lightmap_and_alphafunc() {
        let sh = Shader {
            name: "test/mat".into(),
            stages: vec![Stage {
                bundles: vec![
                    Bundle {
                        image: ImageRef::Path("a".into()),
                        anim: None,
                        clamp: false,
                        tcmods: vec![TcMod::Scroll(0.25, 0.0)],
                        tc_gen: TcGen::Base,
                        env: TexEnv::Modulate,
                    },
                    Bundle {
                        image: ImageRef::Lightmap,
                        anim: None,
                        clamp: false,
                        tcmods: vec![],
                        tc_gen: TcGen::Base,
                        env: TexEnv::Modulate,
                    },
                ],
                blend: Some((BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha)),
                depth_write: None,
                alpha_func: Some(AlphaFunc::Ge128),
                depth_func: DepthFunc::LessEqual,
                rgb_gen: RgbGen::Const([0.5, 0.25, 0.125]),
                alpha_gen: AlphaGen::Const(0.75),
            }],
            ..Default::default()
        };
        let p = stage_params(&sh, 0, 2.0).unwrap();
        assert_eq!(
            p.flags,
            STAGE_FLAG_BUNDLE1_LIGHTMAP | STAGE_FLAG_ALPHAFUNC_GE128
        );
        assert_eq!(p.uv0, bundle_affine(&[TcMod::Scroll(0.25, 0.0)], 2.0));
        assert_eq!(p.uv1, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        assert_eq!(p.tint, [0.5, 0.25, 0.125, 0.75]);
        assert_eq!(p.turb01, [0.0; 4]);
    }

    #[test]
    fn stage_params_vector_bundle_sets_flag_and_carries_basis() {
        let sh = Shader {
            name: "test/vec".into(),
            stages: vec![Stage {
                bundles: vec![Bundle {
                    image: ImageRef::White,
                    anim: None,
                    clamp: false,
                    tcmods: vec![],
                    tc_gen: TcGen::Vector([1.0, 0.0, 0.0, 0.0, 1.0, 0.0]),
                    env: TexEnv::Modulate,
                }],
                blend: None,
                depth_write: None,
                alpha_func: None,
                depth_func: DepthFunc::LessEqual,
                rgb_gen: RgbGen::Vertex,
                alpha_gen: AlphaGen::Vertex,
            }],
            ..Default::default()
        };
        let p = stage_params(&sh, 0, 0.0).unwrap();
        assert_eq!(
            p.flags,
            STAGE_FLAG_BUNDLE0_VECTOR
                | STAGE_FLAG_VERTEX_RGB
                | STAGE_FLAG_VERTEX_RGB_HALF
                | STAGE_FLAG_VERTEX_ALPHA
        );
        assert_eq!(p.vec0_s, [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(p.vec0_t, [0.0, 1.0, 0.0, 0.0]);
        assert_eq!(p.vec1_s, [0.0; 4]);
        // vertex gens leave tint white; the flags carry the instruction
        assert_eq!(p.tint, [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn stage_params_wave_gens_evaluate_at_time() {
        let rgb_wave = Wave {
            form: WaveForm::Sin,
            base: 1.0,
            amp: 0.5,
            phase: 0.25,
            freq: 0.0,
        };
        let a_wave = Wave {
            form: WaveForm::Sawtooth,
            base: 0.0,
            amp: 1.0,
            phase: 0.25,
            freq: 0.0,
        };
        let sh = Shader {
            name: "test/wave".into(),
            stages: vec![Stage {
                bundles: vec![Bundle {
                    image: ImageRef::Path("a".into()),
                    anim: None,
                    clamp: false,
                    tcmods: vec![],
                    tc_gen: TcGen::Base,
                    env: TexEnv::Modulate,
                }],
                blend: None,
                depth_write: None,
                alpha_func: None,
                depth_func: DepthFunc::LessEqual,
                rgb_gen: RgbGen::Wave(rgb_wave.clone()),
                alpha_gen: AlphaGen::Wave(a_wave.clone()),
            }],
            ..Default::default()
        };
        let p = stage_params(&sh, 0, 123.0).unwrap();
        let rv = vcod_common::shader::wave_value(&rgb_wave, 123.0);
        let av = vcod_common::shader::wave_value(&a_wave, 123.0);
        // the rgb wave clamps to a colour byte and scales by identityLight
        // in the shader; alpha waves do not
        assert!((rv - 1.5).abs() < 1e-3, "{rv}");
        assert_eq!(p.tint[..3], [1.0; 3]);
        assert!((p.tint[3] - av).abs() < 1e-6 && (av - 0.25).abs() < 1e-6);
        assert_eq!(p.flags, STAGE_FLAG_TINT_LIGHT);
    }

    #[test]
    fn stage_params_identity_gens_stay_white_without_flags() {
        for rgb in [RgbGen::Identity, RgbGen::ExactVertex] {
            let sh = Shader {
                name: "test/id".into(),
                stages: vec![Stage {
                    bundles: vec![Bundle {
                        image: ImageRef::Path("a".into()),
                        anim: None,
                        clamp: false,
                        tcmods: vec![],
                        tc_gen: TcGen::Base,
                        env: TexEnv::Modulate,
                    }],
                    blend: None,
                    depth_write: None,
                    alpha_func: None,
                    depth_func: DepthFunc::LessEqual,
                    rgb_gen: rgb.clone(),
                    alpha_gen: AlphaGen::Identity,
                }],
                ..Default::default()
            };
            let p = stage_params(&sh, 0, 9.0).unwrap();
            assert_eq!(p.tint, [1.0, 1.0, 1.0, 1.0], "{rgb:?}");
            // ExactVertex routes through the vertex-colour flag like Vertex
            assert_eq!(
                p.flags,
                u32::from(rgb == RgbGen::ExactVertex) * STAGE_FLAG_VERTEX_RGB
            );
        }
    }

    /// identityLighting and constLighting take identityLight in the
    /// shader; const passes through (0x4ffa60's colour switch).
    #[test]
    fn stage_params_identity_light_flags_the_lighting_gens() {
        let cases: &[(RgbGen, [f32; 3], u32)] = &[
            (RgbGen::IdentityLighting, [1.0; 3], STAGE_FLAG_TINT_LIGHT),
            (
                RgbGen::ConstLighting([0.6, 0.65, 0.7]),
                [0.6, 0.65, 0.7],
                STAGE_FLAG_TINT_LIGHT,
            ),
            (RgbGen::Const([0.6, 0.65, 0.7]), [0.6, 0.65, 0.7], 0),
        ];
        for (rgb, want, flags) in cases {
            let sh = Shader {
                name: "test/il".into(),
                stages: vec![Stage {
                    bundles: vec![Bundle {
                        image: ImageRef::Path("a".into()),
                        anim: None,
                        clamp: false,
                        tcmods: vec![],
                        tc_gen: TcGen::Base,
                        env: TexEnv::Modulate,
                    }],
                    blend: None,
                    depth_write: None,
                    alpha_func: None,
                    depth_func: DepthFunc::LessEqual,
                    rgb_gen: rgb.clone(),
                    alpha_gen: AlphaGen::Identity,
                }],
                ..Default::default()
            };
            let p = stage_params(&sh, 0, 9.0).unwrap();
            assert_eq!(p.tint[..3], want[..], "{rgb:?}");
            assert_eq!(p.flags, *flags, "{rgb:?}");
        }
    }

    /// Every WGSL module parses and validates, so a shader edit fails here
    /// instead of at pipeline creation.
    #[test]
    fn wgsl_modules_validate() {
        use wgpu::naga;
        for (name, src) in [
            ("shader", include_str!("shader.wgsl")),
            ("dynamic_model", include_str!("dynamic_model.wgsl")),
            ("viewmodel", include_str!("viewmodel.wgsl")),
            ("fx", include_str!("fx.wgsl")),
            ("sky", include_str!("sky.wgsl")),
            ("gamma", include_str!("gamma.wgsl")),
            ("hud", include_str!("hud.wgsl")),
            ("hud_text", include_str!("hud_text.wgsl")),
            ("bloom", include_str!("bloom.wgsl")),
            ("ssao", include_str!("ssao.wgsl")),
            ("sun_flare", include_str!("sun_flare.wgsl")),
            ("sun_shafts", include_str!("sun_shafts.wgsl")),
        ] {
            let module = naga::front::wgsl::parse_str(src)
                .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(src)));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::default(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(src)));
        }
    }

    #[test]
    fn stage_params_single_bundle_leaves_slot_one_neutral() {
        let sh = Shader {
            name: "test/one".into(),
            stages: vec![Stage {
                bundles: vec![Bundle {
                    image: ImageRef::Path("a".into()),
                    anim: None,
                    clamp: false,
                    tcmods: vec![TcMod::Scale(2.0, 3.0)],
                    tc_gen: TcGen::Base,
                    env: TexEnv::Modulate,
                }],
                blend: None,
                depth_write: None,
                alpha_func: Some(AlphaFunc::Lt128),
                depth_func: DepthFunc::LessEqual,
                rgb_gen: RgbGen::Identity,
                alpha_gen: AlphaGen::Identity,
            }],
            ..Default::default()
        };
        let p = stage_params(&sh, 0, 1.0).unwrap();
        assert_eq!(p.flags, STAGE_FLAG_ALPHAFUNC_LT128);
        assert_eq!(p.uv1, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        assert_eq!(p.turb01[2..], [0.0, 0.0]);
        assert_eq!(p.vec1_s, [0.0; 4]);
    }

    #[test]
    fn stage_params_turb_rides_separate_from_affine() {
        let tcmods = vec![
            TcMod::Scroll(0.5, 0.5),
            TcMod::Turb {
                amp: 3.0,
                phase: 0.1,
                freq: 0.5,
            },
        ];
        let sh = Shader {
            name: "test/turb".into(),
            stages: vec![Stage {
                bundles: vec![Bundle {
                    image: ImageRef::Path("a".into()),
                    anim: None,
                    clamp: false,
                    tcmods: tcmods.clone(),
                    tc_gen: TcGen::Base,
                    env: TexEnv::Modulate,
                }],
                blend: None,
                depth_write: None,
                alpha_func: None,
                depth_func: DepthFunc::LessEqual,
                rgb_gen: RgbGen::Identity,
                alpha_gen: AlphaGen::Identity,
            }],
            ..Default::default()
        };
        let p = stage_params(&sh, 0, 4.0).unwrap();
        assert_eq!(p.uv0, bundle_affine(&tcmods, 4.0));
        assert_eq!(p.turb01, [3.0, 0.1 + 0.5 * 4.0, 0.0, 0.0]);
    }

    #[test]
    fn stage_params_flag_flap_and_sprites() {
        // skins/shipflag_np: flap s, the wave riding StageParams.wave
        let parse = |body: &str| {
            let (name, body) = vcod_common::shader::split_blocks(body)
                .into_iter()
                .next()
                .expect("one block");
            vcod_common::shader::parse_shader(
                &name,
                &body,
                &mut vcod_common::shader::WarnSet::new(),
            )
        };
        let flag = parse("f { cull none deformVertexes flap s 32 sin 4 8 0 .75 { map a.tga } }");
        let p = stage_params(&flag, 0, 2.0).expect("one stage");
        assert_eq!(p.flags & STAGE_FLAG_DEFORM_FLAP_S, STAGE_FLAG_DEFORM_FLAP_S);
        assert_eq!(
            p.flags & (STAGE_FLAG_DEFORM_WAVE | STAGE_FLAG_DEFORM_FLAP_T),
            0
        );
        assert_eq!(p.wave, [4.0, 8.0, 1.5, 1.0 / 32.0]);
        assert!(stage_animated(&flag, 0));
        // a rate of 0 drops the spread (one phase for every vertex)
        let still = parse("f { deformVertexes flap t 32 sin 4 8 .5 0 { map a.tga } }");
        let p = stage_params(&still, 0, 2.0).expect("one stage");
        assert_eq!(p.flags & STAGE_FLAG_DEFORM_FLAP_T, STAGE_FLAG_DEFORM_FLAP_T);
        assert_eq!(p.wave, [4.0, 8.0, 0.5, 0.0]);
        // skins/light_sprite: autosprite, and its lightingDiffuse-free stage
        let sprite = parse(
            "s { deformvertexes autosprite { map a.tga blendfunc add rgbGen identityLighting } }",
        );
        let p = stage_params(&sprite, 0, 0.0).expect("one stage");
        assert_eq!(p.flags & STAGE_FLAG_AUTOSPRITE, STAGE_FLAG_AUTOSPRITE);
        assert!(!stage_animated(&sprite, 0));
        let beam = parse("b { deformVertexes autosprite2 { map a.tga } }");
        let p = stage_params(&beam, 0, 0.0).expect("one stage");
        assert_eq!(
            p.flags & (STAGE_FLAG_AUTOSPRITE | STAGE_FLAG_AUTOSPRITE2),
            STAGE_FLAG_AUTOSPRITE2
        );
    }

    #[test]
    fn stage_params_out_of_range_stage_is_none() {
        let sh = Shader::default();
        assert_eq!(stage_params(&sh, 0, 0.0), None);
    }

    #[test]
    fn anim_frame_index_floors_wraps_and_clamps() {
        assert_eq!(anim_frame_index(5.0, 3, 0.0), 0);
        assert_eq!(anim_frame_index(5.0, 3, 0.21), 1);
        assert_eq!(anim_frame_index(5.0, 3, 0.999), 1); // floor(4.995)=4, 4%3
        assert_eq!(anim_frame_index(5.0, 3, 10.0), 2); // 50 % 3
        assert_eq!(anim_frame_index(0.0, 3, 5.0), 0);
        assert_eq!(anim_frame_index(-1.0, 3, 5.0), 0);
        assert_eq!(anim_frame_index(5.0, 0, 5.0), 0);
        assert_eq!(anim_frame_index(5.0, 3, -1.0), 0);
    }

    // ---- StageBatch classification ----

    use vcod_common::shader::{SkyParms, SurfaceBits};

    /// One-bundle shader from blend pair plus top-level knobs.
    fn script(
        blend: Option<(BlendFactor, BlendFactor)>,
        sort: Option<f32>,
        offset: bool,
    ) -> Shader {
        Shader {
            name: "test/script".into(),
            stages: vec![Stage {
                bundles: vec![Bundle {
                    image: ImageRef::Path("a".into()),
                    anim: None,
                    clamp: false,
                    tcmods: vec![],
                    tc_gen: TcGen::Base,
                    env: TexEnv::Modulate,
                }],
                blend,
                depth_write: None,
                alpha_func: None,
                depth_func: DepthFunc::LessEqual,
                rgb_gen: RgbGen::Identity,
                alpha_gen: AlphaGen::Identity,
            }],
            sort,
            polygon_offset: offset,
            ..Default::default()
        }
    }

    #[test]
    fn opaque_and_blend_variants_follow_class_and_depth_write() {
        let (v, _, _) = stage_draw_facts(&script(None, None, false), 0).unwrap();
        assert_eq!(v, StageVariant::Opaque);

        let alpha = Some((BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha));
        let (v, _, _) = stage_draw_facts(&script(alpha.clone(), None, false), 0).unwrap();
        assert_eq!(v, StageVariant::Blend);

        // explicit depthWrite keeps writing
        let mut sh = script(alpha, None, false);
        sh.stages[0].depth_write = Some(true);
        assert_eq!(
            stage_draw_facts(&sh, 0).unwrap().0,
            StageVariant::BlendDepthWrite
        );

        // polygonOffset biases the stage
        assert!(stage_draw_facts(&script(None, None, true), 0).unwrap().2);
    }

    /// Additive depthWrite must reach the pipeline (Task-6 carry-over): the
    /// variants exist instead of silently drawing without depth writes.
    #[test]
    fn additive_depth_write_selects_a_depth_writing_variant() {
        let one_one = Some((BlendFactor::One, BlendFactor::One));
        let src_one = Some((BlendFactor::SrcAlpha, BlendFactor::One));

        let mut sh = script(one_one, None, false);
        assert_eq!(
            stage_draw_facts(&sh, 0).unwrap().0,
            StageVariant::AdditiveFixed
        );
        sh.stages[0].depth_write = Some(true);
        assert_eq!(
            stage_draw_facts(&sh, 0).unwrap().0,
            StageVariant::AdditiveFixedDepthWrite
        );

        let mut sh = script(src_one, None, false);
        assert_eq!(
            stage_draw_facts(&sh, 0).unwrap().0,
            StageVariant::AdditiveFaded
        );
        sh.stages[0].depth_write = Some(true);
        assert_eq!(
            stage_draw_facts(&sh, 0).unwrap().0,
            StageVariant::AdditiveFadedDepthWrite
        );
    }

    #[test]
    fn animated_params_track_tcmods_and_wave_gens() {
        let mut sh = script(None, None, false);
        assert!(!stage_animated(&sh, 0));
        sh.stages[0].bundles[0].tcmods.push(TcMod::Scroll(0.1, 0.0));
        assert!(stage_animated(&sh, 0));

        let mut sh = script(None, None, false);
        sh.stages[0].rgb_gen = RgbGen::Wave(Wave {
            form: WaveForm::Sin,
            base: 1.0,
            amp: 0.0,
            phase: 0.0,
            freq: 1.0,
        });
        assert!(stage_animated(&sh, 0));

        // scale/transform are static affines; no rewrite needed
        let mut sh = script(None, None, false);
        sh.stages[0].bundles[0].tcmods.push(TcMod::Scale(2.0, 2.0));
        assert!(!stage_animated(&sh, 0));
    }

    /// Sky materials classify through their block's surface bits so the sky
    /// pass can pick them up.
    #[test]
    fn surface_bits_carry_sky_and_nodraw_classification() {
        let mut bits = SurfaceBits::default();
        assert!(!bits.sky && !bits.nodraw);
        bits.sky = true;
        let sh = Shader {
            name: "test/sky".into(),
            stages: vec![],
            sky: Some(SkyParms {
                env: "textures/skies/x".into(),
                cloud_height: 512.0,
            }),
            surface: bits.clone(),
            ..Default::default()
        };
        // either signal routes to Sky at load time
        assert!(sh.surface.sky || sh.sky.is_some());
        assert!(mesh::material_kind("textures/skies/x", true, false, true) == MaterialKind::Sky);
    }

    #[test]
    fn one_list_merges_world_marks_models_and_fx_by_sort_in_that_order_on_ties() {
        use SortedItem::{Fx, Mark, Model, World};
        use vcod_common::shader::SORT_DECAL;
        let world = [SORT_OPAQUE, SORT_DECAL, SORT_BLEND, SORT_ADDITIVE];
        // marks sort decal (polygonOffset2): after the opaques, before blends
        let marks = [SORT_DECAL];
        let models = [SORT_OPAQUE, SORT_BLEND];
        // an opaque effect material draws among the opaques, not after
        // every world blend
        let fx = [SORT_OPAQUE, SORT_BLEND, SORT_ADDITIVE, 22.0];
        assert_eq!(
            merge_by_sort(&world, &marks, &models, &fx),
            [
                World(0),
                Model(0),
                Fx(0),
                World(1),
                Mark(0),
                World(2),
                Model(1),
                Fx(1),
                World(3),
                Fx(2),
                Fx(3)
            ]
        );
        assert_eq!(merge_by_sort(&[], &[], &[], &[SORT_BLEND]), [Fx(0)]);
        assert_eq!(merge_by_sort(&[SORT_BLEND], &[], &[], &[]), [World(0)]);
    }

    #[test]
    fn shaders_of_one_sort_order_by_their_first_stage_images() {
        let e = |sort, images| SortEntry { sort, images };
        let places = sorted_places(&[
            // registered first, but blend sorts after every opaque
            e(SORT_BLEND, Some([1, 0])),
            e(SORT_OPAQUE, Some([9, 3])),
            e(SORT_OPAQUE, Some([4, 3])),
            // the same texture on a lower lightmap goes before it
            e(SORT_OPAQUE, Some([4, 2])),
            // equal images: after the earlier one
            e(SORT_OPAQUE, Some([4, 3])),
            e(vcod_common::shader::SORT_OCEAN, Some([20, 0])),
        ]);
        assert_eq!(places, [5, 4, 2, 1, 3, 0]);
        // a shader without bundle 1 does not hold back a newcomer with the
        // same bundle 0, nor does a stageless one of the same sort
        let places = sorted_places(&[
            e(SORT_OPAQUE, Some([4, 0])),
            e(SORT_OPAQUE, None),
            e(SORT_OPAQUE, Some([4, 7])),
        ]);
        assert_eq!(places, [2, 1, 0]);
    }

    #[test]
    fn image_numbers_follow_first_load_and_ignore_case_and_extension() {
        let mut n = ImageNumbers::default();
        assert_eq!(n.number("$white"), 1);
        assert_eq!(n.number("textures/a/Wall.tga"), 2);
        assert_eq!(n.number("textures/a/wall"), 2);
        assert_eq!(n.number("textures/a/floor.jpg"), 3);
    }

    #[test]
    fn world_draws_follow_the_sorted_list_with_a_shaders_stages_together() {
        let call = |pass| DrawCall {
            bind_group: 0,
            pass,
        };
        // batch 0: implicit opaque; 1: an opaque base with an additive glow
        // stage; 2: an alphaFunc cutout scripted `sort blend`; 3: a plain
        // opaque scripted shader whose texture loaded before batch 0's
        let calls = [
            call(Pass::Opaque),
            call(Pass::Stage),
            call(Pass::Stage),
            call(Pass::Stage),
        ];
        let stages = vec![vec![], vec![0, 1], vec![2], vec![3]];
        let keys = [
            (SORT_OPAQUE, 1),
            (SORT_OPAQUE, 2),
            (SORT_BLEND, 3),
            (SORT_OPAQUE, 0),
        ];
        let draws: Vec<DrawRange> = (0..4)
            .map(|batch| DrawRange {
                batch,
                first: 0,
                count: 3,
            })
            .collect();
        let mut out = Vec::new();
        order_draws(&keys, &calls, &stages, &draws, &mut out);
        let got: Vec<_> = out
            .iter()
            .map(|fd| match fd.kind {
                DrawRef::Legacy(_) => (fd.src, None, fd.sort),
                DrawRef::Stage(sb) => (fd.src, Some(sb), fd.sort),
            })
            .collect();
        assert_eq!(
            got,
            [
                (3, Some(3), SORT_OPAQUE),
                (0, None, SORT_OPAQUE),
                // the glow stage right after its base, at the shader's sort
                (1, Some(0), SORT_OPAQUE),
                (1, Some(1), SORT_OPAQUE),
                // the cutout at its own sort, after an entity of sort 5
                (2, Some(2), SORT_BLEND),
            ]
        );
        let world: Vec<f32> = out.iter().map(|fd| fd.sort).collect();
        let items = merge_by_sort(&world, &[], &[SORT_OPAQUE, SORT_BLEND], &[]);
        use SortedItem::{Model, World};
        assert_eq!(
            items,
            [
                World(0),
                World(1),
                World(2),
                World(3),
                Model(0),
                World(4),
                Model(1)
            ]
        );
    }

    #[test]
    fn entity_draws_sort_by_material_sort_then_load_order_then_entity() {
        let skin = |sort, order| EntSkin {
            stages: Vec::new(),
            lit: true,
            sort,
            order,
        };
        let (blend, opaque_late, opaque_early) = (skin(15.0, 0), skin(5.0, 2), skin(5.0, 1));
        let surfs = [
            (0, 0, &blend),
            (0, 1, &opaque_late),
            (1, 0, &opaque_early),
            (2, 0, &opaque_late),
        ];
        let got: Vec<_> = sorted_ent_draws(surfs.into_iter(), std::iter::empty())
            .iter()
            .map(|d| (d.inst, d.surf))
            .collect();
        assert_eq!(got, [(1, 0), (0, 1), (2, 0), (0, 0)]);
    }

    #[test]
    fn the_gun_interleaves_with_the_world_and_entities_by_material() {
        let skin = |sort, order| EntSkin {
            stages: Vec::new(),
            lit: true,
            sort,
            order,
        };
        let (flesh, rifle, glass, smoke) = (
            skin(SORT_OPAQUE, 0),
            skin(SORT_OPAQUE, 1),
            skin(SORT_BLEND, 2),
            skin(16.0, 3),
        );
        // a player in the rifle's material and a blend2 smoke card
        let ents = [(0, 0, &flesh), (0, 1, &rifle), (1, 0, &smoke)];
        // the gun: hands, the same rifle material, a blended scope lens
        let gun = [(0, 0, &glass), (0, 1, &rifle), (1, 0, &flesh)];
        let draws = sorted_ent_draws(ents.into_iter(), gun.into_iter());
        let got: Vec<_> = draws.iter().map(|d| (d.gun, d.inst, d.surf)).collect();
        assert_eq!(
            got,
            [
                // one material: the entities, then the gun (a later number)
                (false, 0, 0),
                (true, 1, 0),
                (false, 0, 1),
                (true, 0, 1),
                (true, 0, 0),
                (false, 1, 0),
            ]
        );

        // World ocean and opaque, then the models of sort 5, then an
        // opaque-class see-through cutout; the dlights after the last opaque
        // world draw, ahead of the models. The gun's lens (blend) draws
        // before a world blend2 and the smoke (blend2), not after every
        // world translucent.
        use SortedItem::{Model, World};
        let world = [3.0, SORT_OPAQUE, SORT_OPAQUE, 7.0, SORT_BLEND, 16.0];
        let sorts: Vec<f32> = draws.iter().map(|d| d.sort).collect();
        let items = merge_by_sort(&world, &[], &sorts, &[]);
        assert_eq!(
            items,
            [
                World(0),
                World(1),
                World(2),
                Model(0),
                Model(1),
                Model(2),
                Model(3),
                World(3),
                World(4),
                Model(4),
                World(5),
                Model(5)
            ]
        );
        assert_eq!(dlight_slot(&items, &world), 3);
        assert_eq!(dlight_slot(&[], &[]), 0);
    }

    /// The SSAO split follows the last draw of any list sorting up to
    /// decal, past an opaque effect, a mark run and a decal model, before every blend.
    #[test]
    fn ssao_splits_after_the_last_opaque_or_decal_draw() {
        let world = [SORT_OPAQUE, SORT_DECAL, SORT_BLEND];
        let models = [SORT_OPAQUE, SORT_DECAL, SORT_ADDITIVE];
        let fx = [SORT_OPAQUE, SORT_BLEND];
        let marks = [SORT_DECAL];
        let items = merge_by_sort(&world, &marks, &models, &fx);
        // world 0, model 0, fx 0, world 1, mark 0, model 1 | world 2, fx 1, model 2
        assert_eq!(ssao_split(&items, &world, &marks, &models, &fx), 6);
        assert_eq!(ssao_split(&[], &[], &[], &[], &[]), 0);
        let blends = [SORT_BLEND];
        let only = merge_by_sort(&blends, &[], &[], &[]);
        assert_eq!(ssao_split(&only, &blends, &[], &[], &[]), 0);
    }
}
