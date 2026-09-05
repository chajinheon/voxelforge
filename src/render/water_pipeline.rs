//! Dedicated forward water pipeline.
//!
//! Water is kept separate from the legacy translucent chunk shader so the
//! forward pass can read the immutable opaque HDR scene and the linear depth
//! pyramid while retaining the packed chunk vertex contract.

use std::num::NonZeroU64;
use std::path::Path;

use glam::IVec3;
use wgpu::util::DeviceExt;

use crate::mesh::mesher::ChunkMesh;

use super::chunk_pipeline::{GpuChunk, GpuChunkMeshes};
use super::globals::Globals;
use super::scene_bindings::SceneBindings;

const WATER_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub struct WaterPipeline {
    pipeline: wgpu::RenderPipeline,
    bindings: SceneBindings,
    params: wgpu::Buffer,
    sampler: wgpu::Sampler,
    depth_sampler: wgpu::Sampler,
    scene_layout: wgpu::BindGroupLayout,
    scene_group: Option<wgpu::BindGroup>,
    depth_copy: Option<wgpu::Texture>,
    depth_copy_view: Option<wgpu::TextureView>,
    shader_path: std::path::PathBuf,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WaterParams {
    time: f32,
    underwater: f32,
    max_distance: f32,
    _padding0: f32,
    absorption: [f32; 3],
    _padding1: f32,
    scattering: [f32; 3],
    _padding_scattering: f32,
    internal_size: [f32; 2],
    ssr_steps: f32,
    _padding2: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ChunkUniform {
    origin: [i32; 4],
}

impl WaterPipeline {
    pub fn new(
        device: &wgpu::Device,
        bindings: SceneBindings,
        shader_path: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        let shader_path = shader_path.as_ref().to_owned();
        let source = std::fs::read_to_string(&shader_path)?;
        Self::new_source(device, bindings, source, shader_path)
    }

    pub(crate) fn new_source(
        device: &wgpu::Device,
        bindings: SceneBindings,
        source: String,
        shader_path: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        let shader_path = shader_path.as_ref().to_owned();
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m8/water-shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let scene_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("vf/m8/water-scene-layout"),
                entries: &[
                    texture_entry(0, true),
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    texture_entry(2, false),
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(
                                std::mem::size_of::<WaterParams>() as u64
                            ),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                        count: None,
                    },
                ],
            });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vf/m8/water-layout"),
            bind_group_layouts: &[
                Some(&bindings.globals_layout),
                Some(&scene_layout),
                Some(&bindings.chunk_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vf/m8/water-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<crate::mesh::vertex::ChunkVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Uint32x2],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: WATER_FORMAT,
                    // Water computes its final HDR color from the immutable
                    // scene copy; blending would double-apply the background.
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        if let Some(error) = pollster::block_on(scope.pop()) {
            anyhow::bail!("water shader validation failed: {error}");
        }
        let defaults = WaterParams {
            time: 0.0,
            underwater: 0.0,
            max_distance: super::water::SSR_MAX_DISTANCE,
            _padding0: 0.0,
            absorption: super::water::SURFACE_ABSORPTION.to_array(),
            _padding1: 0.0,
            scattering: super::water::SURFACE_SCATTERING.to_array(),
            _padding_scattering: 0.0,
            internal_size: [1.0, 1.0],
            ssr_steps: super::water::SSR_COARSE_STEPS as f32,
            _padding2: 0.0,
        };
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vf/m8/water-params"),
            contents: bytemuck::bytes_of(&defaults),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vf/m8/water-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let depth_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vf/m8/water-depth-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Ok(Self {
            pipeline,
            bindings,
            params,
            sampler,
            depth_sampler,
            scene_layout,
            scene_group: None,
            depth_copy: None,
            depth_copy_view: None,
            shader_path,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_scene(
        &mut self,
        device: &wgpu::Device,
        scene_color: &wgpu::TextureView,
        _scene_depth: &wgpu::TextureView,
        scene_depth_texture: &wgpu::Texture,
        dimensions: (u32, u32),
        depth_pyramid: &wgpu::TextureView,
        _sampler: &wgpu::Sampler,
    ) {
        let depth_copy = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vf/m8/water-depth-copy"),
            size: wgpu::Extent3d {
                width: dimensions.0,
                height: dimensions.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth_copy_view = depth_copy.create_view(&Default::default());
        self.depth_copy = Some(depth_copy);
        self.depth_copy_view = Some(depth_copy_view);
        let Some(depth_view) = self.depth_copy_view.as_ref() else {
            return;
        };
        self.scene_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m8/water-scene"),
            layout: &self.scene_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(scene_color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(depth_pyramid),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.depth_sampler),
                },
            ],
        }));
        let _ = scene_depth_texture;
    }

    pub fn copy_depth(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture,
        dimensions: (u32, u32),
    ) {
        let Some(destination) = self.depth_copy.as_ref() else {
            return;
        };
        encoder.copy_texture_to_texture(
            source.as_image_copy(),
            destination.as_image_copy(),
            wgpu::Extent3d {
                width: dimensions.0,
                height: dimensions.1,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn update(
        &self,
        queue: &wgpu::Queue,
        globals: &Globals,
        time: f32,
        underwater: bool,
        internal_size: (u32, u32),
        ssr_steps: u32,
    ) {
        globals.write(queue, &self.bindings.globals_buffer);
        let params = WaterParams {
            time,
            underwater: f32::from(underwater),
            max_distance: if underwater {
                super::water::UNDERWATER_MAX_DISTANCE
            } else {
                super::water::SSR_MAX_DISTANCE
            },
            _padding0: 0.0,
            absorption: if underwater {
                super::water::UNDERWATER_ABSORPTION.to_array()
            } else {
                super::water::SURFACE_ABSORPTION.to_array()
            },
            _padding1: 0.0,
            scattering: if underwater {
                super::water::UNDERWATER_SCATTERING.to_array()
            } else {
                super::water::SURFACE_SCATTERING.to_array()
            },
            _padding_scattering: 0.0,
            internal_size: [internal_size.0 as f32, internal_size.1 as f32],
            ssr_steps: ssr_steps.clamp(16, 64) as f32,
            _padding2: 0.0,
        };
        queue.write_buffer(&self.params, 0, bytemuck::bytes_of(&params));
    }

    pub fn upload_chunk(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &ChunkMesh,
        origin: IVec3,
        slot: u32,
    ) -> anyhow::Result<GpuChunk> {
        // The regular chunk pipeline owns the shared slot allocator and
        // uploads. This entry point is retained for future water-only reloads.
        let _ = (device, queue, mesh, origin, slot);
        anyhow::bail!("water uploads are owned by the shared chunk pipeline")
    }

    pub fn upload_chunk_with_slot(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &ChunkMesh,
        origin: IVec3,
        slot: u32,
    ) -> anyhow::Result<GpuChunk> {
        if mesh.vertices.is_empty() || mesh.indices.is_empty() {
            anyhow::bail!("cannot upload an empty water mesh");
        }
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vf/m8/water-vertices"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vf/m8/water-indices"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });
        queue.write_buffer(
            &self.bindings.chunk_uniforms,
            slot as u64 * self.bindings.slot_size,
            bytemuck::bytes_of(&ChunkUniform {
                origin: [origin.x, origin.y, origin.z, 0],
            }),
        );
        Ok(GpuChunk {
            vertex_buffer,
            index_buffer,
            index_count: mesh.indices.len() as u32,
            origin,
            uniform_offset: (slot as u64 * self.bindings.slot_size) as u32,
            slot,
        })
    }

    pub fn update_globals(&self, queue: &wgpu::Queue, globals: &Globals) {
        globals.write(queue, &self.bindings.globals_buffer);
    }

    pub fn reload_shader(&mut self, device: &wgpu::Device, _format: wgpu::TextureFormat) -> bool {
        let Ok(source) = std::fs::read_to_string(&self.shader_path) else {
            return false;
        };
        let Ok(candidate) = Self::new_source(
            device,
            self.bindings.clone(),
            source,
            self.shader_path.clone(),
        ) else {
            log::warn!(
                "water shader reload rejected: {}",
                self.shader_path.display()
            );
            return false;
        };
        *self = candidate;
        true
    }

    pub fn draw_mesh_index<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        meshes: &'a [GpuChunkMeshes],
        index: usize,
    ) -> bool {
        let Some(scene_group) = self.scene_group.as_ref() else {
            return false;
        };
        let Some(chunk) = meshes.get(index).and_then(|mesh| mesh.water.as_ref()) else {
            return false;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bindings.globals_bind_group, &[]);
        pass.set_bind_group(1, scene_group, &[]);
        pass.set_bind_group(2, &self.bindings.chunk_bind_group, &[chunk.uniform_offset]);
        pass.set_vertex_buffer(0, chunk.vertex_buffer.slice(..));
        pass.set_index_buffer(chunk.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..chunk.index_count, 0, 0..1);
        true
    }

    pub fn shader_path(&self) -> &Path {
        &self.shader_path
    }
}

fn texture_entry(binding: u32, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}
