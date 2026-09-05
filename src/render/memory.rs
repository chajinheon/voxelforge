//! Byte accounting for resources owned by the live renderer.
//!
//! These helpers deliberately use the descriptors retained by wgpu objects
//! instead of reproducing dimensions or formats in a second table.  The
//! result is therefore an allocation estimate for the resources that exist,
//! not a theoretical worst-case budget.

use super::chunk_pipeline::{GpuChunk, GpuChunkMeshes};

/// Estimate the texel storage represented by a live texture descriptor.
pub fn texture_bytes(texture: &wgpu::Texture) -> usize {
    let format = texture.format();
    let Some(block_bytes) = format.block_copy_size(None) else {
        return 0;
    };
    let (block_width, block_height) = format.block_dimensions();
    let mut width = texture.width();
    let mut height = texture.height();
    let mut bytes = 0usize;
    for _ in 0..texture.mip_level_count() {
        let blocks_x = width.div_ceil(block_width) as usize;
        let blocks_y = height.div_ceil(block_height) as usize;
        bytes = bytes.saturating_add(
            blocks_x
                .saturating_mul(blocks_y)
                .saturating_mul(texture.depth_or_array_layers() as usize)
                .saturating_mul(block_bytes as usize),
        );
        width = width.div_ceil(2).max(1);
        height = height.div_ceil(2).max(1);
    }
    bytes.saturating_mul(texture.sample_count() as usize)
}

impl GpuChunk {
    /// Actual sizes requested from wgpu for this chunk's vertex and index
    /// buffers. Empty sentinel buffers are included because they are live GPU
    /// allocations too.
    pub fn buffer_memory_bytes(&self) -> usize {
        self.vertex_buffer
            .size()
            .saturating_add(self.index_buffer.size()) as usize
    }
}

impl GpuChunkMeshes {
    /// Sum of the live opaque, translucent, and water buffer allocations.
    pub fn buffer_memory_bytes(&self) -> usize {
        self.opaque
            .as_ref()
            .map_or(0, GpuChunk::buffer_memory_bytes)
            .saturating_add(
                self.translucent
                    .as_ref()
                    .map_or(0, GpuChunk::buffer_memory_bytes),
            )
            .saturating_add(self.water.as_ref().map_or(0, GpuChunk::buffer_memory_bytes))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn saturating_sum_keeps_accounting_finite() {
        assert_eq!(usize::MAX.saturating_add(1), usize::MAX);
        assert_eq!(3usize.saturating_add(4), 7);
    }
}
