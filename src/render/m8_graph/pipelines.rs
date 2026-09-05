use super::{CloudShadowGpuParams, PeriodicNoise, VOLUME_FORMAT, VolumeTargets};
use crate::render::noise::BlueNoiseRank;

#[path = "helpers.rs"]
mod helpers;
use helpers::*;

pub(super) fn buffer(device: &wgpu::Device, label: &str, size: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

pub(super) fn effect_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("vf/m8/effect-layout"),
        entries: &[
            uniform(0, 256),
            uniform(1, 40),
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            uniform(4, 144),
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 6,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
        ],
    })
}

pub(super) fn cloud_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    effect_layout(device)
}

pub(super) fn noise_volume(device: &wgpu::Device, size: u32, label: &str) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: size,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

pub(super) fn upload_volume(queue: &wgpu::Queue, texture: &wgpu::Texture, noise: &PeriodicNoise) {
    let size = noise.size;
    let stride = 256usize;
    let mut bytes = vec![0u8; stride * size * size];
    for z in 0..size {
        for y in 0..size {
            for x in 0..size {
                bytes[z * stride * size + y * stride + x] =
                    (noise.at(x, y, z) * 255.0).round() as u8;
            }
        }
    }
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(stride as u32),
            rows_per_image: Some(size as u32),
        },
        wgpu::Extent3d {
            width: size as u32,
            height: size as u32,
            depth_or_array_layers: size as u32,
        },
    );
}

pub(super) fn blue_texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vf/m8/blue-noise-64"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

pub(super) fn upload_blue(queue: &wgpu::Queue, texture: &wgpu::Texture, noise: &BlueNoiseRank) {
    let mut bytes = vec![0u8; 256 * 64];
    for (index, rank) in noise.ranks().iter().enumerate() {
        bytes[(index / 64) * 256 + index % 64] = (255 * u32::from(*rank) / 4095) as u8;
    }
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256),
            rows_per_image: Some(64),
        },
        wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
    );
}

pub(super) fn view3(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D3),
        ..Default::default()
    })
}
pub(super) fn noise_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("vf/m8/noise-repeat"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    })
}
pub(super) fn depth_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("vf/m8/depth-nearest"),
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        ..Default::default()
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn make_volumetric(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    targets: &VolumeTargets,
    blue: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    normal: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    depth_sampler: &wgpu::Sampler,
) -> (wgpu::ComputePipeline, [wgpu::BindGroup; 2]) {
    let output_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("vf/m8/volumetric-output"),
        entries: &[
            tex(0),
            samp(1),
            storage(2, VOLUME_FORMAT),
            tex_depth_compute(3),
            tex(4),
            tex(5),
            samp_compute_nearest(6),
        ],
    });
    let module = shader(
        device,
        "vf/m8/volumetric",
        include_str!("../../../assets/shaders/volumetric.wgsl"),
    );
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("vf/m8/volumetric-layout"),
        bind_group_layouts: &[Some(layout), Some(&output_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("vf/m8/volumetric-pipeline"),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some("cs_main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let groups = [0, 1].map(|index| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m8/volumetric-output-group"),
            layout: &output_layout,
            entries: &[
                entry_tex(0, blue),
                entry_samp(1, sampler),
                entry_tex(2, &targets.views[index]),
                entry_tex(3, depth),
                entry_tex(4, normal),
                entry_tex(5, &targets.views[index ^ 1]),
                entry_samp(6, depth_sampler),
            ],
        })
    });
    (pipeline, groups)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn make_cloud(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    targets: &VolumeTargets,
    base: &wgpu::TextureView,
    detail: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    normal: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    depth_sampler: &wgpu::Sampler,
) -> (wgpu::ComputePipeline, [wgpu::BindGroup; 2]) {
    let output_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("vf/m8/cloud-output"),
        entries: &[
            tex3(0),
            tex3(1),
            samp(2),
            storage(3, VOLUME_FORMAT),
            tex_depth_compute(4),
            tex(5),
            tex(6),
            samp_compute_nearest(7),
        ],
    });
    let module = shader(
        device,
        "vf/m8/clouds",
        include_str!("../../../assets/shaders/clouds.wgsl"),
    );
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("vf/m8/cloud-layout"),
        bind_group_layouts: &[Some(layout), Some(&output_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("vf/m8/cloud-pipeline"),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some("cs_main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let groups = [0, 1].map(|index| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m8/cloud-output-group"),
            layout: &output_layout,
            entries: &[
                entry_tex(0, base),
                entry_tex(1, detail),
                entry_samp(2, sampler),
                entry_tex(3, &targets.views[index]),
                entry_tex(4, depth),
                entry_tex(5, normal),
                entry_tex(6, &targets.views[index ^ 1]),
                entry_samp(7, depth_sampler),
            ],
        })
    });
    (pipeline, groups)
}

pub(super) fn make_shadow(
    device: &wgpu::Device,
    params: &wgpu::Buffer,
    base: &wgpu::TextureView,
    detail: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> (wgpu::RenderPipeline, [wgpu::BindGroup; 2]) {
    let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("vf/m8/cloud-shadow-params"),
        entries: &[uniform_f(
            0,
            std::mem::size_of::<CloudShadowGpuParams>() as u64,
        )],
    });
    let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("vf/m8/cloud-shadow-textures"),
        entries: &[tex3_f(0), tex3_f(1), samp_f(2)],
    });
    let module = shader(
        device,
        "vf/m8/cloud-shadow",
        include_str!("../../../assets/shaders/cloud_shadow.wgsl"),
    );
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("vf/m8/cloud-shadow-layout"),
        bind_group_layouts: &[Some(&params_layout), Some(&texture_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("vf/m8/cloud-shadow-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::R8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::RED,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let params_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("vf/m8/cloud-shadow-params-group"),
        layout: &params_layout,
        entries: &[entry_buffer(0, params)],
    });
    let textures = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("vf/m8/cloud-shadow-texture-group"),
        layout: &texture_layout,
        entries: &[
            entry_tex(0, base),
            entry_tex(1, detail),
            entry_samp(2, sampler),
        ],
    });
    (pipeline, [params_group, textures])
}

pub(super) fn make_composite(
    device: &wgpu::Device,
    scene: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    normal: &wgpu::TextureView,
    fog: &[wgpu::TextureView; 2],
    cloud: &[wgpu::TextureView; 2],
    params: &wgpu::Buffer,
) -> (wgpu::RenderPipeline, [wgpu::BindGroup; 2]) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("vf/m8/composite-layout"),
        entries: &[
            tex_f(0),
            tex_depth(1),
            tex_f(2),
            tex_f(3),
            tex_f(4),
            samp_f(5),
            samp_nearest(6),
            uniform_f(7, 32),
        ],
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("vf/m8/composite-sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let depth_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("vf/m8/composite-depth-sampler"),
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    let module = shader(
        device,
        "vf/m8/volumetric-composite",
        include_str!("../../../assets/shaders/volumetric.wgsl"),
    );
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("vf/m8/composite-pipeline-layout"),
        bind_group_layouts: &[None, None, Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("vf/m8/composite-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_composite"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_composite"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: VOLUME_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let groups = [0, 1].map(|index| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m8/composite-group"),
            layout: &layout,
            entries: &[
                entry_tex(0, scene),
                entry_tex(1, depth),
                entry_tex(2, normal),
                entry_tex(3, &fog[index]),
                entry_tex(4, &cloud[index]),
                entry_samp(5, &sampler),
                entry_samp(6, &depth_sampler),
                entry_buffer(7, params),
            ],
        })
    });
    (pipeline, groups)
}
