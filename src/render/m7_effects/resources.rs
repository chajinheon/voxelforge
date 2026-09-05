pub(super) struct Targets {
    pub(super) ao_history: [wgpu::TextureView; 2],
    pub(super) exposure: wgpu::TextureView,
    pub(super) atmosphere: wgpu::TextureView,
    pub(super) bloom: wgpu::TextureView,
    pub(super) history: [wgpu::TextureView; 2],
    pub(super) history_depth: [wgpu::TextureView; 2],
    pub(super) history_normal: [wgpu::TextureView; 2],
    pub(super) history_material: [wgpu::TextureView; 2],
    pub(super) bloom_down: Vec<wgpu::TextureView>,
    pub(super) bloom_up: Vec<wgpu::TextureView>,
    pub(super) _textures: Vec<wgpu::Texture>,
}

pub(super) use super::atmosphere::AtmosphereGpu;
pub(super) fn history_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("vf/m7/history/pl"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("vf/m7/history/pipeline"),
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("history_pass"),
            compilation_options: Default::default(),
            targets: &[
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R32Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                }),
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rg16Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                }),
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                }),
            ],
        }),
        multiview_mask: None,
        cache: None,
    })
}
pub(super) fn history_pass(
    e: &mut wgpu::CommandEncoder,
    depth: &wgpu::TextureView,
    normal: &wgpu::TextureView,
    material: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
) {
    let mut p = e.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("vf/m7/history-update"),
        color_attachments: &[
            Some(wgpu::RenderPassColorAttachment {
                view: depth,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            }),
            Some(wgpu::RenderPassColorAttachment {
                view: normal,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            }),
            Some(wgpu::RenderPassColorAttachment {
                view: material,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            }),
        ],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    p.set_pipeline(pipeline);
    p.set_bind_group(0, group, &[]);
    p.draw(0..3, 0..1);
}
pub(super) fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
    entry: &str,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("vf/m7/post/pl"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(entry),
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
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
}
pub(super) fn compute_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
    entry: &str,
) -> wgpu::ComputePipeline {
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("vf/m7/exposure/pl"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(entry),
        layout: Some(&pl),
        module: shader,
        entry_point: Some(entry),
        compilation_options: Default::default(),
        cache: None,
    })
}
pub(super) fn pass(
    e: &mut wgpu::CommandEncoder,
    label: &str,
    target: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
    t: Option<wgpu::RenderPassTimestampWrites<'_>>,
) {
    let mut p = e.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: t,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    p.set_pipeline(pipeline);
    p.set_bind_group(0, group, &[]);
    p.draw(0..3, 0..1);
}
pub(super) fn tex(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

pub(super) fn tex_cube(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::Cube,
            multisampled: false,
        },
        count: None,
    }
}
pub(super) fn tex_unfilterable(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}
pub(super) fn depth(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Depth,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}
pub(super) fn sampler(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}
pub(super) fn uniform(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
pub(super) fn storage(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
pub(super) fn storage_rw(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

pub(super) fn texture_entry<'a>(
    binding: u32,
    view: &'a wgpu::TextureView,
) -> wgpu::BindGroupEntry<'a> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}
pub(super) fn sampler_entry<'a>(
    binding: u32,
    sampler: &'a wgpu::Sampler,
) -> wgpu::BindGroupEntry<'a> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::Sampler(sampler),
    }
}
pub(super) fn buffer_entry<'a>(
    binding: u32,
    buffer: wgpu::BindingResource<'a>,
) -> wgpu::BindGroupEntry<'a> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer,
    }
}
pub(super) fn compute_tex(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}
pub(super) fn compute_uniform(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
pub(super) fn halton(mut i: u32, b: u32) -> f32 {
    let mut r = 0.;
    let mut f = 1. / b as f32;
    while i != 0 {
        r += f * (i % b) as f32;
        i /= b;
        f /= b as f32;
    }
    r
}
impl Targets {
    pub(super) fn new(
        device: &wgpu::Device,
        internal: (u32, u32),
        native: (u32, u32),
        format: wgpu::TextureFormat,
    ) -> Self {
        let mut textures = Vec::new();
        let make = |size: (u32, u32), label: &str, f, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size.0.max(1),
                    height: size.1.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: f,
                usage,
                view_formats: &[],
            })
        };
        let common = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC;
        let ao_size = (internal.0.div_ceil(2), internal.1.div_ceil(2));
        let ah0 = make(
            ao_size,
            "vf/m7/ao-history-0",
            wgpu::TextureFormat::R8Unorm,
            common,
        );
        let ah1 = make(
            ao_size,
            "vf/m7/ao-history-1",
            wgpu::TextureFormat::R8Unorm,
            common,
        );
        let e = make(internal, "vf/m7/exposure", format, common);
        let x = make(internal, "vf/m7/atmosphere", format, common);
        let b = make(internal, "vf/m7/bloom", format, common);
        let h0 = make(native, "vf/m7/history-0", format, common);
        let h1 = make(native, "vf/m7/history-1", format, common);
        let hd0 = make(
            native,
            "vf/m7/history-depth-0",
            wgpu::TextureFormat::R32Float,
            common,
        );
        let hd1 = make(
            native,
            "vf/m7/history-depth-1",
            wgpu::TextureFormat::R32Float,
            common,
        );
        let hn0 = make(
            native,
            "vf/m7/history-normal-0",
            wgpu::TextureFormat::Rg16Float,
            common,
        );
        let hn1 = make(
            native,
            "vf/m7/history-normal-1",
            wgpu::TextureFormat::Rg16Float,
            common,
        );
        let hm0 = make(
            native,
            "vf/m7/history-material-0",
            wgpu::TextureFormat::R8Unorm,
            common,
        );
        let hm1 = make(
            native,
            "vf/m7/history-material-1",
            wgpu::TextureFormat::R8Unorm,
            common,
        );
        let mut down = Vec::new();
        let mut up = Vec::new();
        let mut size = internal;
        for i in 0..6 {
            down.push(make(size, &format!("vf/m7/bloom/down-{i}"), format, common));
            up.push(make(size, &format!("vf/m7/bloom/up-{i}"), format, common));
            size = (size.0.div_ceil(2).max(1), size.1.div_ceil(2).max(1));
        }
        let view = |t: &wgpu::Texture| t.create_view(&Default::default());
        let ahv = [view(&ah0), view(&ah1)];
        let ev = view(&e);
        let xv = view(&x);
        let bv = view(&b);
        let hv = [view(&h0), view(&h1)];
        let hdv = [view(&hd0), view(&hd1)];
        let hnv = [view(&hn0), view(&hn1)];
        let hmv = [view(&hm0), view(&hm1)];
        let dv = down.iter().map(view).collect();
        let uv = up.iter().map(view).collect();
        textures.extend([ah0, ah1, e, x, b, h0, h1, hd0, hd1, hn0, hn1, hm0, hm1]);
        textures.extend(down);
        textures.extend(up);
        Self {
            ao_history: ahv,
            exposure: ev,
            atmosphere: xv,
            bloom: bv,
            history: hv,
            history_depth: hdv,
            history_normal: hnv,
            history_material: hmv,
            bloom_down: dv,
            bloom_up: uv,
            _textures: textures,
        }
    }
}
