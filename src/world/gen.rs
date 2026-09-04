use super::block::{AIR, DIRT, GRASS, LEAVES, LOG, SAND, STONE, WATER};
use super::chunk::Chunk;
use super::coords::CHUNK_SIZE;
use super::trees::{tree_at, tree_blocks};
use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};
use glam::IVec3;

pub const SEA_LEVEL: i32 = 62;
pub const MAX_GEN_CHUNK_Y: i32 = 4;

pub struct WorldGen {
    seed: u64,
    noise: FastNoiseLite,
    cave_noise: FastNoiseLite,
}

impl WorldGen {
    pub fn new(seed: u64) -> Self {
        let mut noise = FastNoiseLite::with_seed(seed as i32);
        noise.set_noise_type(Some(NoiseType::OpenSimplex2));
        noise.set_fractal_type(Some(FractalType::FBm));
        noise.set_fractal_octaves(Some(4));
        noise.set_fractal_lacunarity(Some(2.0));
        noise.set_fractal_gain(Some(0.5));
        noise.set_frequency(Some(0.008));
        let mut cave_noise = FastNoiseLite::with_seed(seed.wrapping_add(1) as i32);
        cave_noise.set_noise_type(Some(NoiseType::OpenSimplex2));
        cave_noise.set_fractal_type(Some(FractalType::None));
        cave_noise.set_frequency(Some(0.045));
        Self {
            seed,
            noise,
            cave_noise,
        }
    }
    pub fn height_at(&self, x: i32, z: i32) -> i32 {
        64 + (24.0 * self.noise.get_noise_2d(x as f32, z as f32)).round() as i32
    }
    pub fn generate(&self, cp: IVec3) -> Chunk {
        let mut chunk = Chunk::new_air();
        if cp.y >= MAX_GEN_CHUNK_Y {
            return chunk;
        }
        let mut heights = [0; (CHUNK_SIZE * CHUNK_SIZE) as usize];
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let wx = cp.x * CHUNK_SIZE + x;
                let wz = cp.z * CHUNK_SIZE + z;
                heights[(x + CHUNK_SIZE * z) as usize] = self.height_at(wx, wz);
            }
        }
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let h = heights[(x + CHUNK_SIZE * z) as usize];
                    let world_y = y + cp.y * CHUNK_SIZE;
                    let mut id = if world_y < 1 || world_y <= h - 4 {
                        STONE
                    } else if world_y < h {
                        DIRT
                    } else if world_y == h {
                        if h > SEA_LEVEL { GRASS } else { SAND }
                    } else if world_y <= SEA_LEVEL {
                        WATER
                    } else {
                        AIR
                    };
                    let world_x = cp.x * CHUNK_SIZE + x;
                    let world_z = cp.z * CHUNK_SIZE + z;
                    if world_y >= 8
                        && world_y <= h - 6
                        && self.cave_noise.get_noise_3d(
                            world_x as f32,
                            world_y as f32,
                            world_z as f32,
                        ) > 0.62
                    {
                        id = AIR;
                    }
                    if id != AIR {
                        chunk.set(glam::UVec3::new(x as u32, y as u32, z as u32), id);
                    }
                }
            }
        }
        self.add_trees(cp, &mut chunk);
        chunk
    }

    fn add_trees(&self, cp: IVec3, chunk: &mut Chunk) {
        let origin = cp * CHUNK_SIZE;
        for z in origin.z - 2..origin.z + CHUNK_SIZE + 2 {
            for x in origin.x - 2..origin.x + CHUNK_SIZE + 2 {
                let height = self.height_at(x, z);
                let Some(tree) = tree_at(self.seed, x, z, height) else {
                    continue;
                };
                for (position, id) in tree_blocks(&tree) {
                    let local = position - origin;
                    if !(0..CHUNK_SIZE).contains(&local.x)
                        || !(0..CHUNK_SIZE).contains(&local.y)
                        || !(0..CHUNK_SIZE).contains(&local.z)
                    {
                        continue;
                    }
                    let local = local.as_uvec3();
                    let current = chunk.get(local);
                    if current == AIR || (current == LEAVES && id == LOG) {
                        chunk.set(local, id);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::UVec3;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn worldgen_is_send_sync() {
        assert_send_sync::<WorldGen>();
    }

    #[test]
    fn gen_is_deterministic_for_seed() {
        let g = WorldGen::new(7);
        let a = g.generate(IVec3::new(0, 0, 0));
        let b = g.generate(IVec3::new(0, 0, 0));
        assert_eq!(a.blocks, b.blocks);
    }
    #[test]
    fn gen_layers_match_height() {
        let g = WorldGen::new(1);
        let h = g.height_at(0, 0);
        let cp = IVec3::new(0, h.div_euclid(32), 0);
        let c = g.generate(cp);
        let ly = h.rem_euclid(32) as u32;
        let top = c.get(UVec3::new(0, ly, 0));
        assert_eq!(top, if h > SEA_LEVEL { GRASS } else { SAND });
        let below = if ly == 0 {
            g.generate(cp - IVec3::Y).get(UVec3::new(0, 31, 0))
        } else {
            c.get(UVec3::new(0, ly - 1, 0))
        };
        assert_eq!(below, DIRT);
    }

    #[test]
    fn caves_do_not_break_surface() {
        let generator = WorldGen::new(17);
        let chunks = [
            generator.generate(IVec3::new(0, 1, 0)),
            generator.generate(IVec3::new(0, 2, 0)),
        ];
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let height = generator.height_at(x, z);
                for y in height - 5..=height {
                    let cp_y = y.div_euclid(CHUNK_SIZE);
                    let local_y = y.rem_euclid(CHUNK_SIZE) as u32;
                    assert_ne!(
                        chunks[(cp_y - 1) as usize].get(UVec3::new(x as u32, local_y, z as u32)),
                        AIR,
                        "surface opened at ({x}, {y}, {z})"
                    );
                }
            }
        }
    }

    #[test]
    fn trees_agree_across_chunk_borders() {
        let generator = WorldGen::new(23);
        let tree = (-20_000..=20_000)
            .find_map(|z| {
                let height = generator.height_at(31, z);
                tree_at(generator.seed, 31, z, height)
            })
            .expect("fixed seed should place a tree on the x=31 border");
        let blocks = tree_blocks(&tree);
        let z_chunk = tree.z.div_euclid(CHUNK_SIZE);
        let mut generated = std::collections::HashMap::new();
        for cp_x in [0, 1] {
            for cp_z in z_chunk - 1..=z_chunk + 1 {
                for cp_y in 1..=3 {
                    let cp = IVec3::new(cp_x, cp_y, cp_z);
                    generated.insert(cp, generator.generate(cp));
                }
            }
        }
        let mut crossed = [false; 2];
        for (position, expected) in blocks {
            let cp = IVec3::new(
                position.x.div_euclid(CHUNK_SIZE),
                position.y.div_euclid(CHUNK_SIZE),
                position.z.div_euclid(CHUNK_SIZE),
            );
            if cp.x != 0 && cp.x != 1 {
                continue;
            }
            let surface = generator.height_at(position.x, position.z);
            if position.y <= surface {
                continue;
            }
            let actual = generated[&cp].get(glam::UVec3::new(
                position.x.rem_euclid(CHUNK_SIZE) as u32,
                position.y.rem_euclid(CHUNK_SIZE) as u32,
                position.z.rem_euclid(CHUNK_SIZE) as u32,
            ));
            if expected == LOG {
                assert_eq!(actual, LOG);
            } else {
                assert!(actual == LEAVES || actual == LOG);
            }
            crossed[cp.x as usize] = true;
        }
        assert!(crossed.into_iter().all(|value| value));
    }

    #[test]
    fn max_gen_chunk_y_covers_generated_content() {
        assert_eq!(MAX_GEN_CHUNK_Y, 4);
        let generator = WorldGen::new(1);
        for cp_y in 4..crate::world::coords::WORLD_CHUNKS_Y {
            let chunk = generator.generate(IVec3::new(17, cp_y, -9));
            assert!(
                chunk.blocks.iter().all(|&id| id == AIR),
                "generated chunk at y={cp_y} contains non-air content"
            );
        }
    }
}
