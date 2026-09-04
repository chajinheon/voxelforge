use crate::mesh::ChunkMeshes;
use crate::render::{GpuChunkMeshes, Renderer};
use crate::world::coords::{CHUNK_SIZE, chunk_of};
use crate::world::world::World;

use super::Streamer;

impl Streamer {
    pub(super) fn mesh_and_upload(
        &mut self,
        world: &World,
        renderer: &mut Renderer,
        gpu_chunks: &mut Vec<GpuChunkMeshes>,
        cp: glam::IVec3,
    ) {
        if world.chunk(cp).is_some_and(|chunk| chunk.is_empty()) {
            self.remove_gpu_chunk(renderer, gpu_chunks, cp);
            return;
        }
        self.upload_mesh(
            world,
            renderer,
            gpu_chunks,
            cp,
            crate::mesh::mesh_chunk_greedy_all(&world.padded(cp)),
        );
    }

    pub(super) fn upload_mesh(
        &mut self,
        world: &World,
        renderer: &mut Renderer,
        gpu_chunks: &mut Vec<GpuChunkMeshes>,
        cp: glam::IVec3,
        mesh: ChunkMeshes,
    ) {
        self.remove_gpu_chunk(renderer, gpu_chunks, cp);
        if !world.is_loaded(cp) {
            return;
        }
        match renderer.upload_chunk_meshes(&mesh, cp * CHUNK_SIZE) {
            Ok(chunk) => gpu_chunks.push(chunk),
            Err(error) => log::error!("upload chunk {cp:?} failed: {error:#}"),
        }
    }

    pub(super) fn remove_gpu_chunk(
        &self,
        renderer: &mut Renderer,
        gpu_chunks: &mut Vec<GpuChunkMeshes>,
        cp: glam::IVec3,
    ) {
        if let Some(index) = gpu_chunks
            .iter()
            .position(|chunk| chunk.origin == cp * CHUNK_SIZE)
        {
            renderer.remove_chunk_meshes(gpu_chunks.swap_remove(index));
        }
    }

    pub(super) fn remove_unloaded_gpu_chunks(
        &mut self,
        world: &World,
        renderer: &mut Renderer,
        gpu_chunks: &mut Vec<GpuChunkMeshes>,
    ) {
        let mut kept = Vec::with_capacity(gpu_chunks.len());
        for chunk in gpu_chunks.drain(..) {
            let cp = chunk_of(chunk.origin);
            if world.is_loaded(cp) {
                kept.push(chunk);
            } else {
                self.mesh_epochs.remove(&cp);
                renderer.remove_chunk_meshes(chunk);
            }
        }
        *gpu_chunks = kept;
        self.mesh_epochs.retain(|cp, _| world.is_loaded(*cp));
    }
}
