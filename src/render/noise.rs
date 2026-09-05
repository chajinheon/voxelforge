//! Deterministic periodic noise and blue-noise rank generation.

pub const BLUE_NOISE_SIZE: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlueNoiseRank {
    ranks: Vec<u16>,
}

impl BlueNoiseRank {
    pub fn generate(seed: u32) -> Self {
        let count = BLUE_NOISE_SIZE * BLUE_NOISE_SIZE;
        let mut energy = vec![0.0_f32; count];
        let mut selected = vec![false; count];
        let mut ranks = vec![0_u16; count];
        let first = (0x9e37_79b9_u32 ^ seed) as usize % count;
        for rank in 0..count {
            let index = if rank == 0 {
                first
            } else {
                (0..count)
                    .filter(|&candidate| !selected[candidate])
                    .min_by(|&a, &b| {
                        energy[a].total_cmp(&energy[b]).then_with(|| {
                            tie_break(a as u32 ^ seed).cmp(&tie_break(b as u32 ^ seed))
                        })
                    })
                    .unwrap_or(0)
            };
            selected[index] = true;
            ranks[index] = rank as u16;
            let x = index % BLUE_NOISE_SIZE;
            let y = index / BLUE_NOISE_SIZE;
            for dy in -6..=6 {
                for dx in -6..=6 {
                    let radius_sq = (dx * dx + dy * dy) as f32;
                    let kernel = (-radius_sq / (2.0 * 1.9 * 1.9)).exp();
                    let xx = (x as isize + dx).rem_euclid(BLUE_NOISE_SIZE as isize) as usize;
                    let yy = (y as isize + dy).rem_euclid(BLUE_NOISE_SIZE as isize) as usize;
                    energy[yy * BLUE_NOISE_SIZE + xx] += kernel;
                }
            }
        }
        Self { ranks }
    }

    pub fn default_seed() -> Self {
        Self::generate(0)
    }

    pub fn ranks(&self) -> &[u16] {
        &self.ranks
    }

    pub fn rank_at(&self, x: usize, y: usize) -> u16 {
        self.ranks[(y % BLUE_NOISE_SIZE) * BLUE_NOISE_SIZE + (x % BLUE_NOISE_SIZE)]
    }

    pub fn byte_at(&self, x: usize, y: usize) -> u8 {
        ((255.0 * self.rank_at(x, y) as f32 / 4095.0).round() as u32).min(255) as u8
    }
}

fn tie_break(mut value: u32) -> u32 {
    value ^= value << 13;
    value ^= value >> 17;
    value ^ (value << 5)
}

#[derive(Clone, Debug, PartialEq)]
pub struct PeriodicNoise {
    pub size: usize,
    pub values: Vec<f32>,
}

impl PeriodicNoise {
    pub fn generate(size: usize, seed: u32, octaves: usize) -> Self {
        assert!(size > 0);
        let mut values = Vec::with_capacity(size * size * size);
        for z in 0..size {
            for y in 0..size {
                for x in 0..size {
                    let p = [
                        x as f32 / size as f32,
                        y as f32 / size as f32,
                        z as f32 / size as f32,
                    ];
                    values.push(perlin_fbm(p, size, seed, octaves));
                }
            }
        }
        Self { size, values }
    }

    pub fn sample(&self, position: [f32; 3]) -> f32 {
        trilinear(&self.values, self.size, position)
    }

    pub fn at(&self, x: usize, y: usize, z: usize) -> f32 {
        self.values[(z % self.size * self.size + y % self.size) * self.size + x % self.size]
    }
}

pub fn perlin_fbm(position: [f32; 3], period: usize, seed: u32, octaves: usize) -> f32 {
    let mut total = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut normalization = 0.0;
    for octave in 0..octaves.max(1) {
        total += amplitude
            * perlin_periodic(
                position,
                period,
                seed.wrapping_add(octave as u32),
                frequency,
            );
        normalization += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    (total / normalization.max(f32::EPSILON) * 0.5 + 0.5).clamp(0.0, 1.0)
}

pub fn worley_f1(position: [f32; 3], period: usize, seed: u32) -> f32 {
    let cell = period as f32;
    let px = position[0].rem_euclid(1.0) * cell;
    let py = position[1].rem_euclid(1.0) * cell;
    let pz = position[2].rem_euclid(1.0) * cell;
    let base = [
        px.floor() as isize,
        py.floor() as isize,
        pz.floor() as isize,
    ];
    let mut nearest = f32::INFINITY;
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let cx = (base[0] + dx).rem_euclid(period as isize) as usize;
                let cy = (base[1] + dy).rem_euclid(period as isize) as usize;
                let cz = (base[2] + dz).rem_euclid(period as isize) as usize;
                let hash = lattice_hash(cx, cy, cz, seed);
                let point = [
                    cx as f32 + unit_hash(hash),
                    cy as f32 + unit_hash(hash.rotate_left(11)),
                    cz as f32 + unit_hash(hash.rotate_left(22)),
                ];
                let mut distance = [px - point[0], py - point[1], pz - point[2]];
                distance[0] -= (distance[0] / cell).round() * cell;
                distance[1] -= (distance[1] / cell).round() * cell;
                distance[2] -= (distance[2] / cell).round() * cell;
                nearest = nearest.min(
                    (distance[0] * distance[0]
                        + distance[1] * distance[1]
                        + distance[2] * distance[2])
                        .sqrt()
                        / cell,
                );
            }
        }
    }
    nearest.clamp(0.0, 1.0)
}

fn perlin_periodic(position: [f32; 3], period: usize, seed: u32, frequency: f32) -> f32 {
    let scale = period as f32 * frequency;
    let p = [
        position[0] * scale,
        position[1] * scale,
        position[2] * scale,
    ];
    let base = [
        p[0].floor() as isize,
        p[1].floor() as isize,
        p[2].floor() as isize,
    ];
    let fraction = [p[0].fract(), p[1].fract(), p[2].fract()];
    let fade = fraction.map(|value| value * value * (3.0 - 2.0 * value));
    let mut value = 0.0;
    for dz in 0..=1 {
        for dy in 0..=1 {
            for dx in 0..=1 {
                let weight = if dx == 0 { 1.0 - fade[0] } else { fade[0] }
                    * if dy == 0 { 1.0 - fade[1] } else { fade[1] }
                    * if dz == 0 { 1.0 - fade[2] } else { fade[2] };
                let gx = (base[0] + dx).rem_euclid(period as isize) as usize;
                let gy = (base[1] + dy).rem_euclid(period as isize) as usize;
                let gz = (base[2] + dz).rem_euclid(period as isize) as usize;
                let hash = lattice_hash(gx, gy, gz, seed);
                let gradient = gradient(hash);
                let delta = [
                    fraction[0] - dx as f32,
                    fraction[1] - dy as f32,
                    fraction[2] - dz as f32,
                ];
                value += weight
                    * (gradient[0] * delta[0] + gradient[1] * delta[1] + gradient[2] * delta[2]);
            }
        }
    }
    value.clamp(-1.0, 1.0)
}

fn trilinear(values: &[f32], size: usize, position: [f32; 3]) -> f32 {
    let scaled = [
        position[0].rem_euclid(1.0) * size as f32,
        position[1].rem_euclid(1.0) * size as f32,
        position[2].rem_euclid(1.0) * size as f32,
    ];
    let base = scaled.map(|value| value.floor() as usize % size);
    let fraction = scaled.map(f32::fract);
    let mut value = 0.0;
    for dz in 0..=1 {
        for dy in 0..=1 {
            for dx in 0..=1 {
                let weight = if dx == 0 {
                    1.0 - fraction[0]
                } else {
                    fraction[0]
                } * if dy == 0 {
                    1.0 - fraction[1]
                } else {
                    fraction[1]
                } * if dz == 0 {
                    1.0 - fraction[2]
                } else {
                    fraction[2]
                };
                value += values[(((base[2] + dz) % size) * size + (base[1] + dy) % size) * size
                    + (base[0] + dx) % size]
                    * weight;
            }
        }
    }
    value
}

fn lattice_hash(x: usize, y: usize, z: usize, seed: u32) -> u32 {
    let mut value = seed
        ^ (x as u32).wrapping_mul(0x9e37_79b9)
        ^ (y as u32).wrapping_mul(0x85eb_ca6b)
        ^ (z as u32).wrapping_mul(0xc2b2_ae35);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^ (value >> 15)
}

fn unit_hash(value: u32) -> f32 {
    (value as f32 / u32::MAX as f32).clamp(0.0, 1.0)
}

fn gradient(hash: u32) -> [f32; 3] {
    const GRADIENTS: [[f32; 3]; 12] = [
        [1.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0],
        [1.0, -1.0, 0.0],
        [-1.0, -1.0, 0.0],
        [1.0, 0.0, 1.0],
        [-1.0, 0.0, 1.0],
        [1.0, 0.0, -1.0],
        [-1.0, 0.0, -1.0],
        [0.0, 1.0, 1.0],
        [0.0, -1.0, 1.0],
        [0.0, 1.0, -1.0],
        [0.0, -1.0, -1.0],
    ];
    GRADIENTS[(hash as usize) % GRADIENTS.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blue_noise_is_a_deterministic_rank_permutation() {
        let a = BlueNoiseRank::generate(0);
        let b = BlueNoiseRank::generate(0);
        assert_eq!(a, b);
        let mut ranks = a.ranks().to_vec();
        ranks.sort_unstable();
        assert_eq!(
            ranks,
            (0..4096).map(|value| value as u16).collect::<Vec<_>>()
        );
    }

    #[test]
    fn periodic_noise_opposite_faces_match() {
        let noise = PeriodicNoise::generate(8, 42, 3);
        for y in 0..8 {
            for x in 0..8 {
                assert!(
                    (noise.sample([x as f32 / 8.0, y as f32 / 8.0, 0.0])
                        - noise.sample([x as f32 / 8.0, y as f32 / 8.0, 1.0]))
                    .abs()
                        < 1.0e-6
                );
            }
        }
    }
}
