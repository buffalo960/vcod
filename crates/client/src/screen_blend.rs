//! The shellshock screen blend, retail's render commands 9 and 10
//! (CoDMP.exe `0x4d8620`, `0x4d86b0`; docs/research/cod11-shellshock.md,
//! "Screen blend"): each frame of a shock draws the last capture of the
//! finished 3D view over this one, then captures the result, so the view
//! trails. Both run on the gamma scene after the world's post effects and
//! before the HUD, in retail's byte space.

use crate::renderer::SCENE_FORMAT;

pub struct ScreenBlend {
    blend: wgpu::RenderPipeline,
    copy: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    capture: wgpu::TextureView,
    /// Samples the capture, for the blend.
    from_capture: wgpu::BindGroup,
    /// Samples the scene, for the capture.
    from_scene: wgpu::BindGroup,
    /// The capture holds a frame (render command 9 has run since the last
    /// resize).
    held: bool,
}

impl ScreenBlend {
    pub fn new(device: &wgpu::Device, scene: &wgpu::TextureView, width: u32, height: u32) -> Self {
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("screen blend layout"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    1,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
            ],
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen blend params"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("screen_blend.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("screen blend pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: SCENE_FORMAT,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let blend = pipeline("fs_blend", Some(wgpu::BlendState::ALPHA_BLENDING));
        let copy = pipeline("fs_copy", None);
        let capture = create_capture(device, width, height);
        let from_capture = bind(device, &layout, &capture, &params);
        let from_scene = bind(device, &layout, scene, &params);
        ScreenBlend {
            blend,
            copy,
            layout,
            params,
            capture,
            from_capture,
            from_scene,
            held: false,
        }
    }

    /// The scene target was rebuilt; the old capture is gone with it.
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        scene: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) {
        self.capture = create_capture(device, width, height);
        self.from_capture = bind(device, &self.layout, &self.capture, &self.params);
        self.from_scene = bind(device, &self.layout, scene, &self.params);
        self.held = false;
    }

    /// Forgets the capture: a frame without the blend.
    pub fn drop_capture(&mut self) {
        self.held = false;
    }

    /// Render command 10 into `pass` (on the scene) at `alpha` (0..1),
    /// skipped with no capture or a zero alpha.
    pub fn blend(&self, queue: &wgpu::Queue, pass: &mut wgpu::RenderPass<'_>, alpha: f32) {
        if !self.held || alpha <= 0.0 {
            return;
        }
        queue.write_buffer(
            &self.params,
            0,
            bytemuck::cast_slice(&[alpha, 0.0, 0.0, 0.0]),
        );
        pass.set_pipeline(&self.blend);
        pass.set_bind_group(0, &self.from_capture, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Render command 9: the scene as it stands into the capture.
    pub fn capture(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("screen blend capture"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.capture,
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
        pass.set_pipeline(&self.copy);
        pass.set_bind_group(0, &self.from_scene, &[]);
        pass.draw(0..3, 0..1);
        self.held = true;
    }
}

/// Render command 10's alpha (CoDMP.exe `0x4d86b0`): `0.01` raised to the
/// wall time since the capture over the decay, capped at 0.99 and rounded
/// to a byte; nothing once the capture is older than the decay.
pub fn alpha(since_capture_ms: i32, decay_ms: i32) -> f32 {
    if decay_ms <= 0 || since_capture_ms >= decay_ms {
        return 0.0;
    }
    let a = 0.01f64
        .powf(f64::from(since_capture_ms) / f64::from(decay_ms))
        .min(f64::from(0.99f32));
    let byte = (a as f32 * 255.0 + 9.313226e-10).round_ties_even();
    byte / 255.0
}

fn create_capture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("screen blend capture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SCENE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

fn bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    src: &wgpu::TextureView,
    params: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("screen blend bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(src),
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

    /// One 60 Hz frame under the 400 ms decay keeps 83% of the last frame;
    /// a capture the same millisecond keeps 0.99's byte.
    #[test]
    fn alpha_decays_to_a_hundredth_over_the_decay() {
        assert_eq!(alpha(0, 400), 252.0 / 255.0);
        let a = alpha(16, 400);
        assert_eq!(a, (0.01f64.powf(0.04) as f32 * 255.0).round() / 255.0);
        assert_eq!(alpha(400, 400), 0.0);
        assert_eq!(alpha(10, 0), 0.0);
    }
}
