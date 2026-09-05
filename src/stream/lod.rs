use rayon::prelude::*;

use crate::lod::{
    GRID_SIZE, LodGrid, LodKey, apply_cell_blocking, downsample_light_step, modal_block,
};
use crate::render::Renderer;
use crate::world::block::{AIR, DIRT, GRASS, SAND, STONE, WATER};
use crate::world::coords::CHUNK_SIZE;
use crate::world::r#gen::SEA_LEVEL;
use crate::world::world::World;

pub(super) fn generated_block(y: i32, height: i32) -> u16 {
    if y < 0 {
        STONE
    } else if y >= CHUNK_SIZE * crate::world::coords::WORLD_CHUNKS_Y {
        AIR
    } else if y < 1 || y <= height - 4 {
        STONE
    } else if y < height {
        DIRT
    } else if y == height {
        if height > SEA_LEVEL { GRASS } else { SAND }
    } else if y <= SEA_LEVEL {
        WATER
    } else {
        AIR
    }
}

impl super::Streamer {
    pub(super) fn generate_procedural_lod_parallel(
        &mut self,
        world: &World,
        renderer: &mut Renderer,
    ) {
        if self.save_dir.is_some() {
            return;
        }
        let mut procedural = Vec::new();
        let mut remaining = std::collections::VecDeque::new();
        while let Some(key) = self.lod_pending.pop_front() {
            if procedural.len() >= 32 {
                remaining.push_back(key);
                remaining.extend(self.lod_pending.drain(..));
                break;
            }
            let resident = key.level == 1
                && (0..8).any(|index| {
                    let cp = key.coord * 2
                        + glam::IVec3::new(index & 1, (index >> 1) & 1, (index >> 2) & 1);
                    world.chunk(cp).is_some()
                });
            if key.level == 1 && !resident && self.lod_cache.grid(key).is_none() {
                procedural.push(key);
            } else {
                remaining.push_back(key);
            }
        }
        self.lod_pending = remaining;
        let grids: Vec<_> = procedural
            .par_iter()
            .map(|&key| generated_lod_grid(world, key))
            .collect();
        for grid in grids {
            self.insert_lod_grid(renderer, grid);
        }
    }
}

#[allow(clippy::needless_range_loop)]
pub(super) fn generated_lod_grid(world: &World, key: LodKey) -> LodGrid {
    let mut result = LodGrid::empty(key);
    let origin = key.world_origin();
    let mut heights = [[[[0_i32; 2]; 2]; GRID_SIZE]; GRID_SIZE];
    for z in 0..GRID_SIZE {
        for x in 0..GRID_SIZE {
            for dz in 0..2 {
                for dx in 0..2 {
                    heights[z][x][dz][dx] = world.generator().height_at(
                        origin.x + x as i32 * 2 + dx as i32,
                        origin.z + z as i32 * 2 + dz as i32,
                    );
                }
            }
        }
    }
    for y in 0..GRID_SIZE {
        for z in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                let mut ids = [AIR; 8];
                for sample in 0..8 {
                    let fy = origin.y + y as i32 * 2 + ((sample >> 1) & 1) as i32;
                    let dx = sample & 1;
                    let dz = (sample >> 2) & 1;
                    ids[sample] = generated_block(fy, heights[z][x][dz][dx]);
                }
                let index = LodGrid::index(glam::UVec3::new(x as u32, y as u32, z as u32));
                result.blocks[index] = modal_block(ids);
                result.light[index] = apply_cell_blocking(
                    downsample_light_step([0xf0; 8], key.cell_size() as u8),
                    result.blocks[index],
                );
            }
        }
    }
    result
}
