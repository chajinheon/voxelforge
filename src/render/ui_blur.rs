//! Persistent native-LDR backdrop used by modal UI.

use wgpu::util::DeviceExt;

pub struct UiBlur {
    width: u32,
    height: u32,
    _scene: wgpu::Texture,
    scene_view: wgpu::TextureView,
    _quarter: [wgpu::Texture; 2],
    quarter_views: [wgpu::TextureView; 2],
    _params: [wgpu::Buffer; 3],
    groups: [wgpu::BindGroup; 3],
    blur_pipeline: wgpu::RenderPipeline,
    copy_pipeline: wgpu::RenderPipeline,
}

impl UiBlur {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let quarter_width = width.div_ceil(2);
        let quarter_height = height.div_ceil(2);
        let texture = |label, width, height| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let scene = texture("vf/m10/ui-blur/scene", width, height);
        let quarter = [
            texture(
                "vf/m10/ui-blur/quarter-horizontal",
                quarter_width,
                quarter_height,
            ),
            texture(
                "vf/m10/ui-blur/quarter-vertical",
                quarter_width,
                quarter_height,
            ),
        ];
        let scene_view = scene.create_view(&Default::default());
        let quarter_views = [
            quarter[0].create_view(&Default::default()),
            quarter[1].create_view(&Default::default()),
        ];
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vf/m10/ui-blur/layout"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m10/ui-blur/shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../assets/shaders/ui_blur.wgsl").into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vf/m10/ui-blur/pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let make_pipeline = |label, entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vf/m10/ui-blur/sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let params = [
            uniform(device, [1.0 / width as f32, 0.0, 0.0, 0.0]),
            uniform(device, [0.0, 1.0 / quarter_height as f32, 0.0, 0.0]),
            uniform(device, [0.0; 4]),
        ];
        let group = |label, view: &wgpu::TextureView, params: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: params.as_entire_binding(),
                    },
                ],
            })
        };
        let groups = [
            group("vf/m10/ui-blur/scene-bind", &scene_view, &params[0]),
            group(
                "vf/m10/ui-blur/horizontal-bind",
                &quarter_views[0],
                &params[1],
            ),
            group(
                "vf/m10/ui-blur/vertical-bind",
                &quarter_views[1],
                &params[2],
            ),
        ];
        Self {
            width,
            height,
            _scene: scene,
            scene_view,
            _quarter: quarter,
            quarter_views,
            _params: params,
            groups,
            blur_pipeline: make_pipeline("vf/m10/ui-blur/blur", "fs_blur"),
            copy_pipeline: make_pipeline("vf/m10/ui-blur/copy", "fs_copy"),
        }
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn scene_view(&self) -> &wgpu::TextureView {
        &self.scene_view
    }

    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        for (label, view, pipeline, group) in [
            (
                "vf/m10/ui-blur/horizontal",
                &self.quarter_views[0],
                &self.blur_pipeline,
                &self.groups[0],
            ),
            (
                "vf/m10/ui-blur/vertical",
                &self.quarter_views[1],
                &self.blur_pipeline,
                &self.groups[1],
            ),
            (
                "vf/m10/ui-blur/composite",
                target,
                &self.copy_pipeline,
                &self.groups[2],
            ),
        ] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    pub fn texture_memory_bytes(&self) -> usize {
        crate::render::memory::texture_bytes(&self._scene)
            + self
                ._quarter
                .iter()
                .map(crate::render::memory::texture_bytes)
                .sum::<usize>()
    }
}

fn uniform(device: &wgpu::Device, values: [f32; 4]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vf/m10/ui-blur/params"),
        contents: bytemuck::cast_slice(&values),
        usage: wgpu::BufferUsages::UNIFORM,
    })
}
