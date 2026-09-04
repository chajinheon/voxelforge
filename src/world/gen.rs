use super::block::{AIR, DIRT, GRASS, SAND, STONE, WATER};
use super::chunk::Chunk;
use super::coords::{CHUNK_SIZE, chunk_index};
use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};
use glam::{IVec3, UVec3};

pub const SEA_LEVEL: i32 = 62;

pub struct WorldGen {
    seed: u64,
    noise: FastNoiseLite,
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
        Self { seed, noise }
    }
    pub fn height_at(&self, x: i32, z: i32) -> i32 {
        64 + (24.0 * self.noise.get_noise_2d(x as f32, z as f32)).round() as i32
    }
    pub fn generate(&self, cp: IVec3) -> Chunk {
        let mut chunk = Chunk::new_air();
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let wx = cp.x * CHUNK_SIZE + x;
                    let wz = cp.z * CHUNK_SIZE + z;
                    let h = self.height_at(wx, wz);
                    let id = if y + cp.y * CHUNK_SIZE < 1 || y + cp.y * CHUNK_SIZE <= h - 4 {
                        STONE
                    } else if y + cp.y * CHUNK_SIZE < h {
                        DIRT
                    } else if y + cp.y * CHUNK_SIZE == h {
                        if h > SEA_LEVEL { GRASS } else { SAND }
                    } else if y + cp.y * CHUNK_SIZE <= SEA_LEVEL {
                        WATER
                    } else {
                        AIR
                    };
                    if id != AIR {
                        chunk.blocks[chunk_index(UVec3::new(x as u32, y as u32, z as u32))] = id;
                        chunk.non_air += 1;
                    }
                }
            }
        }
        let _ = self.seed;
        chunk
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
