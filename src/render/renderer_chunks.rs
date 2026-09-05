//! Chunk upload and shared-slot lifetime operations for [`Renderer`].

use super::chunk_pipeline::{ChunkSlot, GpuChunk, GpuChunkMeshes};
use super::renderer::Renderer;

impl Renderer {
    pub fn sync_lod_meshes(&mut self, cache: &crate::lod::LodCache) {
        self.lod_meshes.retain(|mesh| cache.has_gpu_mesh(mesh.key));
    }

    pub fn upload_lod_mesh(&mut self, key: crate::lod::LodKey, mesh: &crate::mesh::lod::LodMesh) {
        self.lod_meshes.retain(|existing| existing.key != key);
        let gpu = self
            .lod_pipeline
            .upload(&self.device, &self.queue, key, mesh);
        self.lod_meshes.push(gpu);
    }

    pub fn remove_lod_mesh(&mut self, key: crate::lod::LodKey) {
        self.lod_meshes.retain(|mesh| mesh.key != key);
    }

    pub fn upload_chunk(
        &mut self,
        mesh: &crate::mesh::mesher::ChunkMesh,
        origin: glam::IVec3,
    ) -> anyhow::Result<GpuChunk> {
        self.chunks
            .upload_chunk(&self.device, &self.queue, mesh, origin)
    }

    pub fn remove_chunk(&mut self, chunk: GpuChunk) {
        self.chunks.remove_chunk(chunk);
    }

    pub fn upload_chunk_meshes(
        &mut self,
        meshes: &crate::mesh::mesher::ChunkMeshes,
        origin: glam::IVec3,
    ) -> anyhow::Result<GpuChunkMeshes> {
        let opaque_empty = meshes.opaque.vertices.is_empty() || meshes.opaque.indices.is_empty();
        let translucent_empty =
            meshes.translucent.vertices.is_empty() || meshes.translucent.indices.is_empty();
        let water_empty = meshes.water.vertices.is_empty() || meshes.water.indices.is_empty();
        if opaque_empty && translucent_empty && water_empty {
            return Ok(GpuChunkMeshes {
                opaque: None,
                translucent: None,
                water: None,
                origin,
                slot: None,
            });
        }
        let slot = self
            .chunks
            .bindings
            .reserve_slot()
            .ok_or_else(|| anyhow::anyhow!("chunk uniform arena exhausted"))?;
        let opaque = if opaque_empty {
            None
        } else {
            match self.chunks.upload_chunk_with_slot(
                &self.device,
                &self.queue,
                &meshes.opaque,
                origin,
                slot,
            ) {
                Ok(chunk) => Some(chunk),
                Err(error) => {
                    self.chunks.bindings.release_slot(slot);
                    return Err(error);
                }
            }
        };
        let translucent = if translucent_empty {
            None
        } else {
            match self.translucent.upload_chunk_with_slot(
                &self.device,
                &self.queue,
                &meshes.translucent,
                origin,
                slot,
            ) {
                Ok(chunk) => Some(chunk),
                Err(error) => {
                    self.chunks.bindings.release_slot(slot);
                    return Err(error);
                }
            }
        };
        let water = if water_empty {
            None
        } else {
            match self.water.upload_chunk_with_slot(
                &self.device,
                &self.queue,
                &meshes.water,
                origin,
                slot,
            ) {
                Ok(chunk) => Some(chunk),
                Err(error) => {
                    self.chunks.bindings.release_slot(slot);
                    return Err(error);
                }
            }
        };
        Ok(GpuChunkMeshes {
            opaque,
            translucent,
            water,
            origin,
            slot: Some(ChunkSlot {
                uniform_offset: (slot as u64 * self.chunks.bindings.slot_size) as u32,
                slot,
            }),
        })
    }

    pub fn remove_chunk_meshes(&mut self, meshes: GpuChunkMeshes) {
        if let Some(slot) = meshes.slot {
            self.chunks.bindings.release_slot(slot.slot);
        }
    }
}
