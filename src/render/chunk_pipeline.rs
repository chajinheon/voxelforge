//! Chunk vertex/index buffers, bind groups, and the opaque voxel pipeline.

use std::path::Path;

use bytemuck::{Pod, Zeroable};
use glam::IVec3;

use crate::mesh::mesher::ChunkMesh;
use crate::mesh::vertex::ChunkVertex;
use crate::world::coords::CHUNK_SIZE;

use super::frustum::Frustum;
use super::globals::Globals;
use super::scene_bindings::SceneBindings;
use super::textures::BlockTextures;

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
    pub(crate) slot: u32,
}

/// The GPU passes belonging to one world chunk. Uploads and removals are
/// performed as one unit by the renderer/streamer even when one pass is empty.
pub struct GpuChunkMeshes {
    pub opaque: Option<GpuChunk>,
    pub translucent: Option<GpuChunk>,
    pub water: Option<GpuChunk>,
    pub origin: IVec3,
    pub slot: Option<ChunkSlot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkSlot {
    pub uniform_offset: u32,
    pub(crate) slot: u32,
}

/// Opaque chunk pipeline and its dynamic-offset uniform arena.
pub struct ChunkPipeline {
    pub pipeline: wgpu::RenderPipeline,
    light_pipeline: wgpu::RenderPipeline,
    shader_module: wgpu::ShaderModule,
    pub(crate) bindings: SceneBindings,
    shader_path: std::path::PathBuf,
    blend: bool,
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
        Self::new_with_bindings(
            device,
            color_format,
            shader_path,
            SceneBindings::new(device, queue, textures),
            false,
        )
    }

    pub(crate) fn new_translucent(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        textures: &BlockTextures,
        shader_path: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        Self::new_with_bindings(
            device,
            color_format,
            shader_path,
            SceneBindings::new(device, queue, textures),
            true,
        )
    }

    pub(crate) fn new_shared(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        shader_path: impl AsRef<Path>,
        bindings: SceneBindings,
        translucent: bool,
    ) -> anyhow::Result<Self> {
        Self::new_with_bindings(device, color_format, shader_path, bindings, translucent)
    }

    pub(crate) fn new_shared_source(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        source: &str,
        shader_path: impl AsRef<Path>,
        bindings: SceneBindings,
        translucent: bool,
    ) -> anyhow::Result<Self> {
        Self::new_with_bindings_source(
            device,
            color_format,
            source.to_owned(),
            shader_path,
            bindings,
            translucent,
        )
    }

    fn new_with_bindings(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        shader_path: impl AsRef<Path>,
        bindings: SceneBindings,
        translucent: bool,
    ) -> anyhow::Result<Self> {
        let shader_path = shader_path.as_ref().to_owned();
        let source = std::fs::read_to_string(&shader_path)
            .map_err(|error| anyhow::anyhow!("{}: {error}", shader_path.display()))?;
        Self::new_with_bindings_source(
            device,
            color_format,
            source,
            shader_path,
            bindings,
            translucent,
        )
    }

    fn new_with_bindings_source(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        source: String,
        shader_path: impl AsRef<Path>,
        bindings: SceneBindings,
        translucent: bool,
    ) -> anyhow::Result<Self> {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("chunk-pipeline-layout"),
            bind_group_layouts: &[
                Some(&bindings.globals_layout),
                Some(&bindings.texture_layout),
                Some(&bindings.chunk_layout),
            ],
            immediate_size: 0,
        });

        let shader_path = shader_path.as_ref().to_owned();
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("chunk-shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let pipeline = create_pipeline(
            device,
            &layout,
            &shader_module,
            color_format,
            translucent,
            "fs_main",
        );
        let light_pipeline = create_pipeline(
            device,
            &layout,
            &shader_module,
            color_format,
            translucent,
            "fs_light",
        );
        if let Some(error) = pollster::block_on(scope.pop()) {
            anyhow::bail!("chunk shader validation failed: {error}");
        }

        Ok(Self {
            pipeline,
            light_pipeline,
            shader_module,
            bindings,
            shader_path,
            blend: translucent,
        })
    }

    pub fn shader_path(&self) -> &Path {
        &self.shader_path
    }

    pub(crate) fn globals_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bindings.globals_layout
    }

    pub(crate) fn globals_bind_group(&self) -> &wgpu::BindGroup {
        &self.bindings.globals_bind_group
    }

    pub(crate) fn scene_bindings(&self) -> SceneBindings {
        self.bindings.clone()
    }

    pub fn update_globals(&self, queue: &wgpu::Queue, globals: &Globals) {
        globals.write(queue, &self.bindings.globals_buffer);
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
            .bindings
            .reserve_slot()
            .ok_or_else(|| anyhow::anyhow!("chunk uniform arena exhausted"))?;
        self.upload_chunk_with_slot(device, queue, mesh, origin, slot)
    }

    pub(crate) fn upload_chunk_with_slot(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &ChunkMesh,
        origin: IVec3,
        slot: u32,
    ) -> anyhow::Result<GpuChunk> {
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
            &self.bindings.chunk_uniforms,
            slot as u64 * self.bindings.slot_size,
            bytemuck::bytes_of(&uniform),
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

    /// Return a dynamic slot when a chunk is removed from the world.
    pub fn remove_chunk(&mut self, chunk: GpuChunk) {
        self.bindings.release_slot(chunk.slot);
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
                Some(&self.bindings.globals_layout),
                Some(&self.bindings.texture_layout),
                Some(&self.bindings.chunk_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = create_pipeline(
            device,
            &pipeline_layout,
            &module,
            color_format,
            self.is_translucent(),
            "fs_main",
        );
        let light_pipeline = create_pipeline(
            device,
            &pipeline_layout,
            &module,
            color_format,
            self.is_translucent(),
            "fs_light",
        );
        if let Some(error) = pollster::block_on(scope.pop()) {
            log::error!("shader reload validation failed: {error}");
            return false;
        }
        self.shader_module = module;
        self.pipeline = pipeline;
        self.light_pipeline = light_pipeline;
        log::info!("shader reloaded: {}", self.shader_path.display());
        true
    }

    /// Draw all non-empty chunks into an active render pass.
    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, chunks: &'a [GpuChunk]) {
        self.set_state(pass);
        for chunk in chunks {
            self.draw_unchecked(pass, chunk);
        }
    }

    pub(crate) fn draw_visible<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        chunks: &[&'a GpuChunk],
        frustum: &Frustum,
    ) -> usize {
        self.set_state(pass);
        let mut drawn = 0;
        for chunk in chunks {
            let min = chunk.origin.as_vec3();
            let max = min + glam::Vec3::splat(CHUNK_SIZE as f32);
            if !frustum.intersects_aabb(min, max) {
                continue;
            }
            self.draw_unchecked(pass, chunk);
            drawn += 1;
        }
        drawn
    }

    pub(crate) fn draw_visible_light<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        chunks: &[&'a GpuChunk],
        frustum: &Frustum,
    ) -> usize {
        pass.set_pipeline(&self.light_pipeline);
        pass.set_bind_group(0, &self.bindings.globals_bind_group, &[]);
        pass.set_bind_group(1, &self.bindings.texture_bind_group, &[]);
        let mut drawn = 0;
        for chunk in chunks {
            let min = chunk.origin.as_vec3();
            let max = min + glam::Vec3::splat(CHUNK_SIZE as f32);
            if frustum.intersects_aabb(min, max) {
                self.draw_unchecked(pass, chunk);
                drawn += 1;
            }
        }
        drawn
    }

    pub(crate) fn draw_mesh_indices<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        meshes: &'a [GpuChunkMeshes],
        indices: &[usize],
        translucent: bool,
        light: bool,
    ) -> usize {
        if light {
            pass.set_pipeline(&self.light_pipeline);
            pass.set_bind_group(0, &self.bindings.globals_bind_group, &[]);
            pass.set_bind_group(1, &self.bindings.texture_bind_group, &[]);
        } else {
            self.set_state(pass);
        }
        let mut drawn = 0;
        for &index in indices {
            let chunk = if translucent {
                meshes.get(index).and_then(|mesh| mesh.translucent.as_ref())
            } else {
                meshes.get(index).and_then(|mesh| mesh.opaque.as_ref())
            };
            if let Some(chunk) = chunk {
                self.draw_unchecked(pass, chunk);
                drawn += 1;
            }
        }
        drawn
    }

    pub(crate) fn set_state<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bindings.globals_bind_group, &[]);
        pass.set_bind_group(1, &self.bindings.texture_bind_group, &[]);
    }

    pub(crate) fn draw_unchecked<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        chunk: &'a GpuChunk,
    ) {
        pass.set_bind_group(2, &self.bindings.chunk_bind_group, &[chunk.uniform_offset]);
        pass.set_vertex_buffer(0, chunk.vertex_buffer.slice(..));
        pass.set_index_buffer(chunk.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..chunk.index_count, 0, 0..1);
    }

    fn is_translucent(&self) -> bool {
        self.blend
    }
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    color_format: wgpu::TextureFormat,
    translucent: bool,
    fragment_entry: &'static str,
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
            cull_mode: (!translucent).then_some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(!translucent),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: translucent.then_some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
