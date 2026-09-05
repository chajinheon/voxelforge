//! Hierarchical distant-terrain data and streaming primitives.

pub mod grid;
pub mod light;
pub mod streamer;

pub use grid::{
    GRID_SIZE, GRID_VOLUME, LodGrid, LodKey, downsample_base, downsample_lod, modal_block,
};
pub use light::{
    LightDirection, apply_cell_blocking, downsample_light, downsample_light_step, neighbor_light,
    propagate,
};
pub use streamer::{
    CPU_LOD_MEMORY_CAP, DEFAULT_FULL_DETAIL_RADIUS, GPU_LOD_MEMORY_CAP, LodCache, LodRange,
    LodSource, ancestor_keys, choose_source, lod_ancestor_keys, lod_for_distance, lod_ring_ranges,
    lod_ring_ranges_default, resolve_source, ring_ranges, select_lod,
};
