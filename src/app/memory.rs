//! Live memory accounting used by the bounded benchmark report.

use voxelforge::render::ui::item_icon_memory_bytes;
use voxelforge::render::{GpuChunkMeshes, Renderer};
use voxelforge::stream::Streamer;
use voxelforge::world::world::World;

/// Memory components reported by the benchmark. Every field is either derived
/// from a live object or explicitly zero when that GPU resource is not owned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct MemoryAccounting {
    pub world_blocks_light_bytes: usize,
    pub uploaded_buffers_bytes: usize,
    pub render_targets_bytes: usize,
    pub lod_cache_bytes: usize,
    pub gi_clipmap_bytes: usize,
    pub icons_bytes: usize,
}

impl MemoryAccounting {
    pub(super) fn measure(
        world: &World,
        streamer: &Streamer,
        renderer: Option<&Renderer>,
        chunks: &[GpuChunkMeshes],
    ) -> Self {
        let (uploaded_buffers_bytes, render_targets_bytes, gi_clipmap_bytes) = renderer
            .map(|renderer| {
                (
                    renderer.uploaded_buffer_memory_bytes(chunks),
                    renderer.render_target_memory_bytes(),
                    renderer.gi_clipmap_memory_bytes(),
                )
            })
            .unwrap_or_default();
        Self {
            world_blocks_light_bytes: world.block_light_memory_bytes(),
            uploaded_buffers_bytes,
            render_targets_bytes,
            lod_cache_bytes: streamer.lod_cache_memory_bytes(),
            gi_clipmap_bytes,
            icons_bytes: item_icon_memory_bytes(),
        }
    }

    pub(super) fn total_bytes(self) -> usize {
        self.world_blocks_light_bytes
            .saturating_add(self.uploaded_buffers_bytes)
            .saturating_add(self.render_targets_bytes)
            .saturating_add(self.lod_cache_bytes)
            .saturating_add(self.gi_clipmap_bytes)
            .saturating_add(self.icons_bytes)
    }

    /// Stable key/value fields for the single benchmark summary line.
    pub(super) fn log_fields(self) -> String {
        format!(
            "memory_world_blocks_light_bytes={} memory_uploaded_buffers_bytes={} memory_render_targets_bytes={} memory_lod_cache_bytes={} memory_gi_clipmap_bytes={} memory_icons_bytes={} memory_total_bytes={}",
            self.world_blocks_light_bytes,
            self.uploaded_buffers_bytes,
            self.render_targets_bytes,
            self.lod_cache_bytes,
            self.gi_clipmap_bytes,
            self.icons_bytes,
            self.total_bytes(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::MemoryAccounting;

    #[test]
    fn total_includes_all_components_without_overflow() {
        let report = MemoryAccounting {
            world_blocks_light_bytes: 1,
            uploaded_buffers_bytes: 2,
            render_targets_bytes: 4,
            lod_cache_bytes: 8,
            gi_clipmap_bytes: 16,
            icons_bytes: 32,
        };
        assert_eq!(report.total_bytes(), 63);
        assert!(report.log_fields().contains("memory_total_bytes=63"));
    }

    #[test]
    fn total_saturates() {
        let report = MemoryAccounting {
            world_blocks_light_bytes: usize::MAX,
            uploaded_buffers_bytes: 1,
            ..MemoryAccounting::default()
        };
        assert_eq!(report.total_bytes(), usize::MAX);
    }
}
