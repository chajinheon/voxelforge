//! Native-resolution fullscreen present pass.

use super::deferred::DeferredDebug;
use wgpu::util::DeviceExt;

pub fn debug_for_view(view: super::renderer::RenderView) -> DeferredDebug {
    match view {
        super::renderer::RenderView::Albedo => DeferredDebug::Albedo,
        super::renderer::RenderView::Normal => DeferredDebug::Normal,
        super::renderer::RenderView::Depth => DeferredDebug::Depth,
        super::renderer::RenderView::Material => DeferredDebug::Material,
        super::renderer::RenderView::Motion => DeferredDebug::Motion,
        super::renderer::RenderView::Reactive => DeferredDebug::Reactive,
        _ => DeferredDebug::Final,
    }
}

pub struct FinalPass {
    pipeline: wgpu::RenderPipeline,
    bind_groups: Vec<wgpu::BindGroup>,
    sampler: wgpu::Sampler,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
}

impl FinalPass {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_source(
            device,
            format,
            include_str!("../../assets/shaders/present.wgsl"),
        )
    }

    pub(crate) fn new_source(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        source: &str,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m7/present/shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vf/m7/present/layout"),
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
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vf/m7/present/pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vf/m7/present/pipeline"),
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
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vf/m7/present/sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vf/m7/present/uniform"),
            contents: bytemuck::cast_slice(&[1.0_f32, 1.0, 1.0, 0.18]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            pipeline,
            bind_groups: Vec::with_capacity(2),
            sampler,
            layout,
            uniform,
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: &wgpu::TextureView,
        source_size: (u32, u32),
        exposure: f32,
        sharpen: f32,
    ) {
        let values = [
            1.0 / source_size.0.max(1) as f32,
            1.0 / source_size.1.max(1) as f32,
            exposure.max(0.0001),
            sharpen.max(0.0),
        ];
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&values));
        self.bind_groups.clear();
        self.bind_groups.push(self.make_bind_group(device, source));
    }

    pub fn prepare_pair(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sources: [&wgpu::TextureView; 2],
        source_size: (u32, u32),
        exposure: f32,
        sharpen: f32,
    ) {
        let values = [
            1.0 / source_size.0.max(1) as f32,
            1.0 / source_size.1.max(1) as f32,
            exposure.max(0.0001),
            sharpen.max(0.0),
        ];
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&values));
        self.bind_groups.clear();
        for source in sources {
            self.bind_groups.push(self.make_bind_group(device, source));
        }
    }

    fn make_bind_group(
        &self,
        device: &wgpu::Device,
        source: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m7/present/bind-group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.uniform.as_entire_binding(),
                },
            ],
        })
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        self.draw_index(pass, 0);
    }

    pub fn draw_index<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, index: usize) {
        if let Some(bind_group) = self.bind_groups.get(index) {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
