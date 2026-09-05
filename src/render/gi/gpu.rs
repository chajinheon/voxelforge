//! GPU-resident voxel GI clipmap and compute stages.

use super::GiCapabilities;
use bytemuck::{Pod, Zeroable};

mod memory;
mod resources;
mod runtime;
mod upload;
use resources::{make_group, storage_binding, storage_binding_format, texture_binding};

const LEVELS: usize = 4;
const VOXELS: u32 = 128;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuParams {
    origins: [[i32; 4]; LEVELS],
    rings: [[u32; 4]; LEVELS],
    params: [f32; 4],
    inverse_view_proj: [[f32; 4]; 4],
    previous_view_proj: [[f32; 4]; 4],
    camera_position: [f32; 4],
    sun_direction: [f32; 4],
    sky_color: [f32; 4],
}

pub struct GpuGi {
    material: Vec<wgpu::Texture>,
    light: Vec<wgpu::Texture>,
    _material_views: Vec<wgpu::TextureView>,
    _light_views: Vec<wgpu::TextureView>,
    _output: [wgpu::Texture; 2],
    output_views: [wgpu::TextureView; 2],
    _moments: [wgpu::Texture; 2],
    _moments_views: [wgpu::TextureView; 2],
    _history: [wgpu::Texture; 2],
    _history_views: [wgpu::TextureView; 2],
    _surface_history: [wgpu::Texture; 2],
    _surface_history_views: [wgpu::TextureView; 2],
    _fallback_gbuffer: [wgpu::Texture; 3],
    params: wgpu::Buffer,
    _layout: wgpu::BindGroupLayout,
    groups: Vec<wgpu::BindGroup>,
    temporal_groups: [wgpu::BindGroup; 2],
    trace: wgpu::ComputePipeline,
    temporal: wgpu::ComputePipeline,
    denoise: [wgpu::ComputePipeline; 3],
    width: u32,
    height: u32,
    enabled: bool,
    history_valid: bool,
    temporal_parity: bool,
    final_output: usize,
    previous_view_proj: [[f32; 4]; 4],
    previous_camera: [f32; 3],
    has_previous_camera: bool,
    previous_revisions: [u32; LEVELS],
    previous_enabled: bool,
    previous_phase: f32,
    has_previous_phase: bool,
}

impl GpuGi {
    /// Allocate all 8 clipmap textures before publishing the renderer.
    pub fn try_new(
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        let capabilities = GiCapabilities::from_adapter(adapter);
        if !capabilities.supported() {
            return Err(capabilities.unsupported_reason().into());
        }
        let width = width.max(1).div_ceil(4);
        let height = height.max(1).div_ceil(4);
        let texture_desc = |label: &'static str| wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: VOXELS,
                height: VOXELS,
                depth_or_array_layers: VOXELS,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };
        let material: Vec<_> = [
            "vf/m9/material-0",
            "vf/m9/material-1",
            "vf/m9/material-2",
            "vf/m9/material-3",
        ]
        .into_iter()
        .map(|label| device.create_texture(&texture_desc(label)))
        .collect();
        let light: Vec<_> = [
            "vf/m9/light-0",
            "vf/m9/light-1",
            "vf/m9/light-2",
            "vf/m9/light-3",
        ]
        .into_iter()
        .map(|label| device.create_texture(&texture_desc(label)))
        .collect();
        let material_views: Vec<_> = material
            .iter()
            .map(|t| t.create_view(&Default::default()))
            .collect();
        let light_views: Vec<_> = light
            .iter()
            .map(|t| t.create_view(&Default::default()))
            .collect();
        let output_desc = |label| wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        };
        let output = [
            device.create_texture(&output_desc("vf/m9/gi-current")),
            device.create_texture(&output_desc("vf/m9/gi-history")),
        ];
        let output_views = [
            output[0].create_view(&Default::default()),
            output[1].create_view(&Default::default()),
        ];
        let moments = [
            device.create_texture(&output_desc("vf/m9/gi-moments-a")),
            device.create_texture(&output_desc("vf/m9/gi-moments-b")),
        ];
        let moments_views = [
            moments[0].create_view(&Default::default()),
            moments[1].create_view(&Default::default()),
        ];
        let history = [
            device.create_texture(&output_desc("vf/m9/gi-history-a")),
            device.create_texture(&output_desc("vf/m9/gi-history-b")),
        ];
        let history_views = [
            history[0].create_view(&Default::default()),
            history[1].create_view(&Default::default()),
        ];
        let surface = |label, format| {
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
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
                view_formats: &[],
            })
        };
        // Packed logical previous depth, oct normal.xy and material class.
        let surface_history = [
            surface("vf/m9/previous-surface-a", wgpu::TextureFormat::Rgba16Float),
            surface("vf/m9/previous-surface-b", wgpu::TextureFormat::Rgba16Float),
        ];
        let surface_history_views = [
            surface_history[0].create_view(&Default::default()),
            surface_history[1].create_view(&Default::default()),
        ];
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vf/m9/gi-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(
                            std::num::NonZeroU64::new(std::mem::size_of::<GpuParams>() as u64)
                                .expect("nonzero"),
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 9,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                texture_binding(10),
                texture_binding(11),
                texture_binding(12),
                storage_binding(13),
                storage_binding(14),
                texture_binding(15),
                texture_binding(16),
                texture_binding(17),
                texture_binding(18),
                storage_binding_format(19, wgpu::TextureFormat::Rgba16Float),
            ],
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vf/m9/gi-params"),
            size: std::mem::size_of::<GpuParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m9/gi-compute"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/gi_gpu.wgsl").into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vf/m9/gi-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let make = |label, entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let trace = make("vf/m9/gi-trace", "main");
        let temporal = make("vf/m9/gi-temporal", "temporal");
        let denoise = [
            make("vf/m9/gi-atrous-1", "denoise_1"),
            make("vf/m9/gi-atrous-2", "denoise_2"),
            make("vf/m9/gi-atrous-4", "denoise_4"),
        ];
        let fallback_gbuffer = [
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("vf/m9/gbuffer-depth-fallback"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            }),
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("vf/m9/gbuffer-normal-fallback"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            }),
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("vf/m9/gbuffer-material-fallback"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            }),
        ];
        let fallback_views = [
            fallback_gbuffer[0].create_view(&Default::default()),
            fallback_gbuffer[1].create_view(&Default::default()),
            fallback_gbuffer[2].create_view(&Default::default()),
        ];
        let groups = vec![
            make_group(
                device,
                &layout,
                &material_views,
                &light_views,
                &params,
                &output_views[0],
                &output_views[1],
                &history_views[0],
                &moments_views[0],
                &history_views[1],
                &moments_views[1],
                &fallback_views[0],
                &fallback_views[1],
                &fallback_views[2],
                &surface_history_views[0],
                &surface_history_views[1],
            ),
            make_group(
                device,
                &layout,
                &material_views,
                &light_views,
                &params,
                &output_views[1],
                &output_views[0],
                &history_views[1],
                &moments_views[1],
                &history_views[0],
                &moments_views[0],
                &fallback_views[0],
                &fallback_views[1],
                &fallback_views[2],
                &surface_history_views[1],
                &surface_history_views[0],
            ),
        ];
        let temporal_groups = [
            make_group(
                device,
                &layout,
                &material_views,
                &light_views,
                &params,
                &output_views[1],
                &output_views[0],
                &history_views[0],
                &moments_views[0],
                &history_views[1],
                &moments_views[1],
                &fallback_views[0],
                &fallback_views[1],
                &fallback_views[2],
                &surface_history_views[0],
                &surface_history_views[1],
            ),
            make_group(
                device,
                &layout,
                &material_views,
                &light_views,
                &params,
                &output_views[1],
                &output_views[0],
                &history_views[1],
                &moments_views[1],
                &history_views[0],
                &moments_views[0],
                &fallback_views[0],
                &fallback_views[1],
                &fallback_views[2],
                &surface_history_views[1],
                &surface_history_views[0],
            ),
        ];
        Ok(Self {
            material,
            light,
            _material_views: material_views,
            _light_views: light_views,
            _output: output,
            output_views,
            _moments: moments,
            _moments_views: moments_views,
            _history: history,
            _history_views: history_views,
            _fallback_gbuffer: fallback_gbuffer,
            _surface_history: surface_history,
            _surface_history_views: surface_history_views,
            params,
            _layout: layout,
            groups,
            temporal_groups,
            trace,
            temporal,
            denoise,
            width,
            height,
            enabled: true,
            history_valid: false,
            temporal_parity: false,
            final_output: 0,
            previous_view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
            previous_camera: [0.0; 3],
            has_previous_camera: false,
            previous_revisions: [0; LEVELS],
            previous_enabled: true,
            previous_phase: 0.0,
            has_previous_phase: false,
        })
    }
}
