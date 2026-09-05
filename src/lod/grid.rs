//! Fixed-size hierarchical voxel grids used for distant terrain.

use glam::{IVec3, UVec3};

use crate::world::{
    block::{AIR, BlockId, WATER, def, lod_equivalent},
    chunk::Chunk,
};

use super::light::{apply_cell_blocking, downsample_light_step};

pub const GRID_SIZE: usize = 32;
pub const GRID_VOLUME: usize = GRID_SIZE * GRID_SIZE * GRID_SIZE;

/// A grid coordinate is measured in grids at the key's level.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LodKey {
    pub level: u8,
    pub coord: IVec3,
}

impl LodKey {
    pub const fn new(level: u8, coord: IVec3) -> Self {
        Self { level, coord }
    }

    pub fn cell_size(self) -> i32 {
        1_i32.checked_shl(self.level as u32).unwrap_or(i32::MAX)
    }

    pub fn world_origin(self) -> IVec3 {
        self.coord * (GRID_SIZE as i32 * self.cell_size())
    }

    pub fn world_position(self, local: UVec3) -> [f32; 3] {
        let p = self.world_origin() + local.as_ivec3() * self.cell_size();
        [p.x as f32, p.y as f32, p.z as f32]
    }
}

/// The CPU representation is deliberately fixed at 32³ cells for every level.
#[derive(Clone)]
pub struct LodGrid {
    pub key: LodKey,
    pub blocks: Box<[BlockId; GRID_VOLUME]>,
    pub light: Box<[u8; GRID_VOLUME]>,
}

impl LodGrid {
    pub fn empty(key: LodKey) -> Self {
        Self {
            key,
            blocks: Box::new([AIR; GRID_VOLUME]),
            light: Box::new([0; GRID_VOLUME]),
        }
    }

    pub fn index(local: UVec3) -> usize {
        local.x as usize + GRID_SIZE * (local.z as usize + GRID_SIZE * local.y as usize)
    }

    pub fn get(&self, local: UVec3) -> BlockId {
        self.blocks[Self::index(local)]
    }

    pub fn get_light(&self, local: UVec3) -> u8 {
        self.light[Self::index(local)]
    }

    pub fn byte_size(&self) -> usize {
        GRID_VOLUME * (std::mem::size_of::<BlockId>() + std::mem::size_of::<u8>())
    }
}

/// Select the stable mode of eight voxel IDs. Count dominates; the remaining
/// rules are the deterministic tie-break contract from BLUEPRINT §17.2.10.
pub fn modal_block(ids: [BlockId; 8]) -> BlockId {
    let ids = ids.map(lod_equivalent);
    let mut best = ids[0];
    let mut best_count = 0usize;
    for candidate in ids {
        let count = ids.iter().filter(|&&id| id == candidate).count();
        if count > best_count || (count == best_count && better_tie(candidate, best)) {
            best = candidate;
            best_count = count;
        }
    }
    best
}

fn better_tie(candidate: BlockId, incumbent: BlockId) -> bool {
    let candidate_non_air = candidate != AIR;
    let incumbent_non_air = incumbent != AIR;
    if candidate_non_air != incumbent_non_air {
        return candidate_non_air;
    }
    let candidate_opaque = def(candidate).opaque;
    let incumbent_opaque = def(incumbent).opaque;
    if candidate_opaque != incumbent_opaque {
        return candidate_opaque;
    }
    // WATER is translucent and therefore loses an opaque tie. This explicit
    // branch documents the strict-majority rule and protects future metadata.
    if (candidate == WATER) != (incumbent == WATER) {
        return incumbent == WATER;
    }
    candidate < incumbent
}

fn child_index(x: usize, y: usize, z: usize) -> usize {
    (x / 16) | ((y / 16) << 1) | ((z / 16) << 2)
}

fn child_local(axis: usize) -> u32 {
    ((axis * 2) % GRID_SIZE) as u32
}

pub fn downsample_base(key: LodKey, children: [&Chunk; 8]) -> LodGrid {
    downsample_from(key, |x, y, z| {
        let index = child_index(x, y, z);
        let local = UVec3::new(child_local(x), child_local(y), child_local(z));
        let child = children[index];
        let ids = [
            child.get(local),
            child.get(local + UVec3::X),
            child.get(local + UVec3::Y),
            child.get(local + UVec3::Z),
            child.get(local + UVec3::new(1, 1, 0)),
            child.get(local + UVec3::new(1, 0, 1)),
            child.get(local + UVec3::new(0, 1, 1)),
            child.get(local + UVec3::splat(1)),
        ];
        let lights = [
            child.get_light(local),
            child.get_light(local + UVec3::X),
            child.get_light(local + UVec3::Y),
            child.get_light(local + UVec3::Z),
            child.get_light(local + UVec3::new(1, 1, 0)),
            child.get_light(local + UVec3::new(1, 0, 1)),
            child.get_light(local + UVec3::new(0, 1, 1)),
            child.get_light(local + UVec3::splat(1)),
        ];
        let block = modal_block(ids);
        (
            block,
            apply_cell_blocking(downsample_light_step(lights, key.cell_size() as u8), block),
        )
    })
}

pub fn downsample_lod(key: LodKey, children: [&LodGrid; 8]) -> LodGrid {
    debug_assert!(key.level == children[0].key.level.saturating_add(1));
    downsample_from(key, |x, y, z| {
        let index = child_index(x, y, z);
        let child = children[index];
        let local = UVec3::new(child_local(x), child_local(y), child_local(z));
        let ids = [
            child.get(local),
            child.get(local + UVec3::X),
            child.get(local + UVec3::Y),
            child.get(local + UVec3::Z),
            child.get(local + UVec3::new(1, 1, 0)),
            child.get(local + UVec3::new(1, 0, 1)),
            child.get(local + UVec3::new(0, 1, 1)),
            child.get(local + UVec3::splat(1)),
        ];
        let lights = [
            child.get_light(local),
            child.get_light(local + UVec3::X),
            child.get_light(local + UVec3::Y),
            child.get_light(local + UVec3::Z),
            child.get_light(local + UVec3::new(1, 1, 0)),
            child.get_light(local + UVec3::new(1, 0, 1)),
            child.get_light(local + UVec3::new(0, 1, 1)),
            child.get_light(local + UVec3::splat(1)),
        ];
        let block = modal_block(ids);
        (
            block,
            apply_cell_blocking(downsample_light_step(lights, key.cell_size() as u8), block),
        )
    })
}

fn downsample_from<F>(key: LodKey, mut sample: F) -> LodGrid
where
    F: FnMut(usize, usize, usize) -> (BlockId, u8),
{
    let mut result = LodGrid::empty(key);
    for y in 0..GRID_SIZE {
        for z in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                let (block, light) = sample(x, y, z);
                let index = x + GRID_SIZE * (z + GRID_SIZE * y);
                result.blocks[index] = block;
                result.light[index] = light;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::{BRICK, STONE};

    #[test]
    fn lod_mode_tie_break_is_stable() {
        assert_eq!(
            modal_block([AIR, STONE, AIR, STONE, STONE, AIR, STONE, AIR]),
            STONE
        );
        assert_eq!(
            modal_block([BRICK, STONE, BRICK, STONE, BRICK, STONE, AIR, AIR]),
            STONE
        );
        assert_eq!(
            modal_block([WATER, STONE, WATER, STONE, WATER, STONE, AIR, AIR]),
            STONE
        );
        assert_eq!(
            modal_block([BRICK, STONE, BRICK, STONE, BRICK, BRICK, AIR, AIR]),
            BRICK
        );
    }

    #[test]
    fn lod_recursive_downsample_is_deterministic() {
        let children: [LodGrid; 8] = std::array::from_fn(|i| {
            let mut grid = LodGrid::empty(LodKey::new(1, IVec3::new(i as i32, 0, 0)));
            grid.blocks.fill(if i % 2 == 0 { STONE } else { BRICK });
            grid
        });
        let refs: [&LodGrid; 8] = std::array::from_fn(|i| &children[i]);
        let first = downsample_lod(LodKey::new(2, IVec3::ZERO), refs);
        let second = downsample_lod(LodKey::new(2, IVec3::ZERO), refs);
        assert_eq!(first.blocks, second.blocks);
        assert_eq!(first.light, second.light);
    }

    #[test]
    fn lod_chunk_uniform_scale_preserves_world_position() {
        let key = LodKey::new(0, IVec3::new(-3, 2, 4));
        for level in 0..=3 {
            let key = LodKey::new(level, key.coord);
            let local = UVec3::new(7, 11, 19);
            let expected = key.world_origin() + local.as_ivec3() * key.cell_size();
            assert_eq!(
                key.world_position(local),
                [expected.x as f32, expected.y as f32, expected.z as f32]
            );
        }
    }
}
