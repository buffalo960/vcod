//! vcod-only post-processing before the HUD (retail has none): `vc_hdr`'s
//! tonemap from the float world target, and `vc_grade`, `vc_vignette` and
//! `vc_grain`. While any is on, the map pass resolves into this pass's world
//! target, the pass writes retail's framebuffer bytes into the gamma pass's
//! scene, and the HUD draws over that, so none of it touches the HUD. With
//! all four off it never runs and the frame is the retail path
//! (docs/research/cod11-gamma.md, section 8).

use crate::renderer::SCENE_FORMAT;

/// The user's settings, read every frame (`vc_hdr` only at renderer build).
#[derive(Clone, Copy, Default, PartialEq)]
pub struct PostSettings {
    pub grade: f32,
    pub vignette: f32,
    pub grain: f32,
}

impl PostSettings {
    /// Whether the pass runs: `vc_hdr` always needs the tonemap.
    pub fn active(&self, hdr: bool) -> bool {
        hdr || self.grade > 0.0 || self.vignette > 0.0 || self.grain > 0.0
    }
}

/// Grain changes this many times a second, film's frame rate.
const GRAIN_RATE: f32 = 24.0;

pub struct PostPass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    params: wgpu::Buffer,
    world_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

impl PostPass {
    /// `format` is the world's (`Renderer::world_format`).
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post params"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("post.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("post pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: SCENE_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let world_view = create_world_view(device, format, width, height);
        let bind_group = create_bind_group(device, &layout, &world_view, &params);
        PostPass {
            pipeline,
            layout,
            format,
            params,
            world_view,
            bind_group,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.world_view = create_world_view(device, self.format, width, height);
        self.bind_group = create_bind_group(device, &self.layout, &self.world_view, &self.params);
    }

    /// Where the map pass resolves while the pass is active.
    pub fn world_view(&self) -> &wgpu::TextureView {
        &self.world_view
    }

    pub fn set_params(
        &self,
        queue: &wgpu::Queue,
        s: PostSettings,
        hdr: bool,
        identity_light: f32,
        time: f32,
    ) {
        let seed = (time * GRAIN_RATE).floor() % 1024.0;
        let p = [
            identity_light,
            if hdr { 1.0 } else { 0.0 },
            s.grade.clamp(0.0, 2.0),
            s.vignette.clamp(0.0, 2.0),
            s.grain.clamp(0.0, 2.0),
            seed,
            0.0,
            0.0,
        ];
        queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&p));
    }

    /// Writes the processed world into `pass`, the gamma scene's.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn create_world_view(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("post world"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

fn create_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    world: &wgpu::TextureView,
    params: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("post bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(world),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: params.as_entire_binding(),
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All four off is the retail path: the pass never runs.
    #[test]
    fn only_hdr_or_a_strength_runs_the_pass() {
        let off = PostSettings::default();
        assert!(!off.active(false));
        assert!(off.active(true));
        for s in [
            PostSettings { grade: 0.5, ..off },
            PostSettings {
                vignette: 1.0,
                ..off
            },
            PostSettings { grain: 1.0, ..off },
        ] {
            assert!(s.active(false));
        }
    }
}
