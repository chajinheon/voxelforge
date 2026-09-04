//! Chunk vertex/index buffers, bind groups, and the opaque voxel pipeline.

use std::num::NonZeroU64;
use std::path::Path;

use bytemuck::{Pod, Zeroable};
use glam::IVec3;

use crate::mesh::mesher::ChunkMesh;
use crate::mesh::vertex::ChunkVertex;

use super::globals::{GLOBALS_SIZE, Globals};
use super::textures::BlockTextures;

const CHUNK_UNIFORM_SIZE: u64 = 16;
const DEFAULT_SLOTS: u32 = 4096;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ChunkUniform {
    origin: [i32; 4],
}

/// GPU-owned resources for one non-empty chunk mesh.
pub struct GpuChunk {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub origin: IVec3,
    pub uniform_offset: u32,
    slot: u32,
}

/// Opaque chunk pipeline and its dynamic-offset uniform arena.
pub struct ChunkPipeline {
    pub pipeline: wgpu::RenderPipeline,
    shader_module: wgpu::ShaderModule,
    pub globals_buffer: wgpu::Buffer,
    pub globals_bind_group: wgpu::BindGroup,
    pub texture_bind_group: wgpu::BindGroup,
    globals_layout: wgpu::BindGroupLayout,
    texture_layout: wgpu::BindGroupLayout,
    chunk_bind_group: wgpu::BindGroup,
    chunk_uniforms: wgpu::Buffer,
    chunk_layout: wgpu::BindGroupLayout,
    slot_size: u64,
    free_slots: Vec<u32>,
    shader_path: std::path::PathBuf,
}

impl ChunkPipeline {
    /// Create the pipeline with explicit target format, usually the surface
    /// format (or `Rgba8UnormSrgb` for snapshots).
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        textures: &BlockTextures,
        shader_path: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(GLOBALS_SIZE),
                },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("block-texture-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
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
        let chunk_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("chunk-dynamic-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(CHUNK_UNIFORM_SIZE),
                },
                count: None,
            }],
        });

        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals-uniform"),
            size: GLOBALS_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&globals_buffer, 0, bytemuck::bytes_of(&Globals::zeroed()));
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals-bind-group"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });
        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("block-texture-bind-group"),
            layout: &texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&textures.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&textures.sampler),
                },
            ],
        });

        let alignment = device.limits().min_uniform_buffer_offset_alignment as u64;
        let slot_size = CHUNK_UNIFORM_SIZE
            .max(256)
            .next_multiple_of(alignment.max(1));
        let uniform_size = slot_size * DEFAULT_SLOTS as u64;
        let chunk_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chunk-uniform-arena"),
            size: uniform_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let chunk_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("chunk-dynamic-bind-group"),
            layout: &chunk_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &chunk_uniforms,
                    offset: 0,
                    size: NonZeroU64::new(CHUNK_UNIFORM_SIZE),
                }),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("chunk-pipeline-layout"),
            bind_group_layouts: &[
                Some(&globals_layout),
                Some(&texture_layout),
                Some(&chunk_layout),
            ],
            immediate_size: 0,
        });

        let shader_path = shader_path.as_ref().to_owned();
        let source = std::fs::read_to_string(&shader_path)
            .map_err(|error| anyhow::anyhow!("{}: {error}", shader_path.display()))?;
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("chunk-shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let pipeline = create_pipeline(device, &layout, &shader_module, color_format);
        if let Some(error) = pollster::block_on(scope.pop()) {
            anyhow::bail!("chunk shader validation failed: {error}");
        }

        Ok(Self {
            pipeline,
            shader_module,
            globals_buffer,
            globals_bind_group,
            texture_bind_group,
            globals_layout,
            texture_layout,
            chunk_bind_group,
            chunk_uniforms,
            chunk_layout,
            slot_size,
            free_slots: (0..DEFAULT_SLOTS).rev().collect(),
            shader_path,
        })
    }

    pub fn shader_path(&self) -> &Path {
        &self.shader_path
    }

    pub(crate) fn globals_layout(&self) -> &wgpu::BindGroupLayout {
        &self.globals_layout
    }

    pub(crate) fn globals_bind_group(&self) -> &wgpu::BindGroup {
        &self.globals_bind_group
    }

    pub fn update_globals(&self, queue: &wgpu::Queue, globals: &Globals) {
        globals.write(queue, &self.globals_buffer);
    }

    /// Upload a mesh and reserve its dynamic uniform slot.
    pub fn upload_chunk(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &ChunkMesh,
        origin: IVec3,
    ) -> anyhow::Result<GpuChunk> {
        if mesh.vertices.is_empty() || mesh.indices.is_empty() {
            anyhow::bail!("cannot upload an empty chunk mesh");
        }
        let slot = self
            .free_slots
            .pop()
            .ok_or_else(|| anyhow::anyhow!("chunk uniform arena exhausted"))?;
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chunk-vertices"),
            size: (mesh.vertices.len() * std::mem::size_of::<ChunkVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&mesh.vertices));
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chunk-indices"),
            size: (mesh.indices.len() * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&index_buffer, 0, bytemuck::cast_slice(&mesh.indices));
        let uniform = ChunkUniform {
            origin: [origin.x, origin.y, origin.z, 0],
        };
        queue.write_buffer(
            &self.chunk_uniforms,
            slot as u64 * self.slot_size,
            bytemuck::bytes_of(&uniform),
        );
        Ok(GpuChunk {
            vertex_buffer,
            index_buffer,
            index_count: mesh.indices.len() as u32,
            origin,
            uniform_offset: (slot as u64 * self.slot_size) as u32,
            slot,
        })
    }

    /// Return a dynamic slot when a chunk is removed from the world.
    pub fn remove_chunk(&mut self, chunk: GpuChunk) {
        self.free_slots.push(chunk.slot);
    }

    /// Recompile and replace the pipeline. On validation failure, the old
    /// module and pipeline remain live and the error is logged.
    pub fn reload_shader(
        &mut self,
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
    ) -> bool {
        let source = match std::fs::read_to_string(&self.shader_path) {
            Ok(source) => source,
            Err(error) => {
                log::error!("shader reload {}: {error}", self.shader_path.display());
                return false;
            }
        };
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("chunk-shader-reloaded"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        // The pipeline layout is intentionally rebuilt from the existing
        // layouts below; this keeps resource bind groups compatible.
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("chunk-pipeline-layout-reloaded"),
            bind_group_layouts: &[
                Some(&self.globals_layout),
                Some(&self.texture_layout),
                Some(&self.chunk_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = create_pipeline(device, &pipeline_layout, &module, color_format);
        if let Some(error) = pollster::block_on(scope.pop()) {
            log::error!("shader reload validation failed: {error}");
            return false;
        }
        self.shader_module = module;
        self.pipeline = pipeline;
        log::info!("shader reloaded: {}", self.shader_path.display());
        true
    }

    /// Draw all non-empty chunks into an active render pass.
    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, chunks: &'a [GpuChunk]) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.globals_bind_group, &[]);
        pass.set_bind_group(1, &self.texture_bind_group, &[]);
        for chunk in chunks {
            pass.set_bind_group(2, &self.chunk_bind_group, &[chunk.uniform_offset]);
            pass.set_vertex_buffer(0, chunk.vertex_buffer.slice(..));
            pass.set_index_buffer(chunk.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..chunk.index_count, 0, 0..1);
        }
    }
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    color_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    const ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Uint32x2];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("chunk-render-pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<ChunkVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &ATTRIBUTES,
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
