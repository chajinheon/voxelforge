//! Alpha-blended chunk pipeline for water and glass.

use std::path::Path;

use glam::IVec3;

use crate::mesh::mesher::ChunkMesh;

use super::chunk_pipeline::{ChunkPipeline, GpuChunk};
use super::frustum::Frustum;
use super::globals::Globals;
use super::scene_bindings::SceneBindings;
use super::textures::BlockTextures;

/// Transparent pass resources. It shares the chunk shader and texture format
/// with the opaque pipeline, but disables depth writes and face culling.
pub struct TranslucentPipeline {
    pipeline: ChunkPipeline,
}

impl TranslucentPipeline {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        textures: &BlockTextures,
        shader_path: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            pipeline: ChunkPipeline::new_translucent(
                device,
                queue,
                color_format,
                textures,
                shader_path,
            )?,
        })
    }

    pub(crate) fn new_shared(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        shader_path: impl AsRef<Path>,
        bindings: SceneBindings,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            pipeline: ChunkPipeline::new_shared(device, color_format, shader_path, bindings, true)?,
        })
    }

    pub(crate) fn new_shared_source(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        source: &str,
        shader_path: impl AsRef<Path>,
        bindings: SceneBindings,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            pipeline: ChunkPipeline::new_shared_source(
                device,
                color_format,
                source,
                shader_path,
                bindings,
                true,
            )?,
        })
    }

    pub fn upload_chunk(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &ChunkMesh,
        origin: IVec3,
    ) -> anyhow::Result<GpuChunk> {
        self.pipeline.upload_chunk(device, queue, mesh, origin)
    }

    pub fn remove_chunk(&mut self, chunk: GpuChunk) {
        self.pipeline.remove_chunk(chunk);
    }

    pub(crate) fn upload_chunk_with_slot(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &ChunkMesh,
        origin: IVec3,
        slot: u32,
    ) -> anyhow::Result<GpuChunk> {
        self.pipeline
            .upload_chunk_with_slot(device, queue, mesh, origin, slot)
    }

    pub fn update_globals(&self, queue: &wgpu::Queue, globals: &Globals) {
        self.pipeline.update_globals(queue, globals);
    }

    pub fn reload_shader(
        &mut self,
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
    ) -> bool {
        self.pipeline.reload_shader(device, color_format)
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        chunks: &[&'a GpuChunk],
        frustum: &Frustum,
    ) -> usize {
        self.pipeline.draw_visible(pass, chunks, frustum)
    }

    pub fn draw_light<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        chunks: &[&'a GpuChunk],
        frustum: &Frustum,
    ) -> usize {
        self.pipeline.draw_visible_light(pass, chunks, frustum)
    }

    pub(crate) fn draw_mesh_index<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        meshes: &'a [super::chunk_pipeline::GpuChunkMeshes],
        index: usize,
        water: bool,
    ) -> bool {
        self.pipeline.set_state(pass);
        let chunk = meshes.get(index).and_then(|mesh| {
            if water {
                mesh.water.as_ref()
            } else {
                mesh.translucent.as_ref()
            }
        });
        let Some(chunk) = chunk else {
            return false;
        };
        self.pipeline.draw_unchecked(pass, chunk);
        true
    }
}
