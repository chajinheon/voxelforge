//! Deterministic atmosphere LUT contracts and a small CPU reference generator.

use std::f32::consts::{PI, TAU};

pub const TRANSMITTANCE_SIZE: (u32, u32) = (256, 64);
pub const MULTISCATTER_SIZE: (u32, u32) = (32, 32);
pub const SKY_VIEW_SIZE: (u32, u32) = (192, 108);
pub const CUBEMAP_SIZE: u32 = 64;
pub const CUBEMAP_FACES: u32 = 6;
pub const CUBEMAP_MIPS: u32 = 7;
pub const TRANSMITTANCE_STEPS: usize = 40;
pub const MULTISCATTER_DIRECTIONS: usize = 16;
pub const MULTISCATTER_STEPS: usize = 8;
pub const SKY_VIEW_STEPS: usize = 24;
pub const SKY_SUN_STEPS: usize = 8;
pub const SKY_UPDATE_CADENCE: u64 = 8;
pub const SKY_PHASE_THRESHOLD: f32 = 0.0005;
pub const ATMOSPHERE_WGSL: &str = include_str!("../../assets/shaders/atmosphere.wgsl");

pub const R_GROUND: f32 = 6_360_000.0;
pub const R_ATMOSPHERE: f32 = 6_460_000.0;
pub const BETA_R: [f32; 3] = [5.8e-6, 13.5e-6, 33.1e-6];
pub const BETA_M: [f32; 3] = [21.0e-6; 3];
pub const HEIGHT_SCALE_R: f32 = 8_000.0;
pub const HEIGHT_SCALE_M: f32 = 1_200.0;
pub const MIE_G: f32 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LutDescriptor {
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    pub mip_levels: u32,
    pub format: &'static str,
}

impl LutDescriptor {
    pub const fn transmittance() -> Self {
        Self {
            width: 256,
            height: 64,
            layers: 1,
            mip_levels: 1,
            format: "Rgba16Float",
        }
    }
    pub const fn multiscatter() -> Self {
        Self {
            width: 32,
            height: 32,
            layers: 1,
            mip_levels: 1,
            format: "Rgba16Float",
        }
    }
    pub const fn sky_view() -> Self {
        Self {
            width: 192,
            height: 108,
            layers: 1,
            mip_levels: 1,
            format: "Rgba16Float",
        }
    }
    pub const fn sky_cubemap() -> Self {
        Self {
            width: 64,
            height: 64,
            layers: 6,
            mip_levels: 7,
            format: "Rgba16Float",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtmosphereConstants {
    pub ground_radius: f32,
    pub atmosphere_radius: f32,
    pub beta_rayleigh: [f32; 3],
    pub beta_mie: [f32; 3],
    pub rayleigh_height: f32,
    pub mie_height: f32,
    pub mie_g: f32,
}

impl Default for AtmosphereConstants {
    fn default() -> Self {
        Self {
            ground_radius: R_GROUND,
            atmosphere_radius: R_ATMOSPHERE,
            beta_rayleigh: BETA_R,
            beta_mie: BETA_M,
            rayleigh_height: HEIGHT_SCALE_R,
            mie_height: HEIGHT_SCALE_M,
            mie_g: MIE_G,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AtmosphereLuts {
    pub transmittance: Vec<[f32; 4]>,
    pub multiscatter: Vec<[f32; 4]>,
    pub sky_view: Vec<[f32; 4]>,
}

impl AtmosphereLuts {
    /// Generate finite, non-negative reference LUT data.  The renderer may
    /// replace this with the compute implementation; keeping this fallback
    /// deterministic makes headless validation and device-less startup safe.
    pub fn generate(constants: AtmosphereConstants) -> Self {
        let transmittance = generate_2d(TRANSMITTANCE_SIZE, |u, v| {
            let mu = u * 2.0 - 1.0;
            let altitude = v * v;
            let path = (1.0 - mu.abs()).max(0.08) * (1.0 - altitude * 0.85);
            let mut out = [0.0; 4];
            for (index, value) in out[..3].iter_mut().enumerate() {
                *value = (-constants.beta_rayleigh[index] * 100_000.0 * path)
                    .exp()
                    .clamp(0.0, 1.0);
            }
            out[3] = 1.0;
            out
        });
        let multiscatter = generate_2d(MULTISCATTER_SIZE, |u, v| {
            let sun_mu = u * 2.0 - 1.0;
            let altitude = v * v;
            let phase = 0.5 + 0.5 * sun_mu;
            let mut out = [0.0; 4];
            for (index, value) in out[..3].iter_mut().enumerate() {
                *value =
                    (phase * (1.0 - altitude) * constants.beta_rayleigh[index] * 1.0e5).max(0.0);
            }
            out[3] = 1.0;
            out
        });
        let sky_view = generate_2d(SKY_VIEW_SIZE, |u, v| {
            let elevation = (0.5 - v).mul_add(PI, 0.0).sin().max(0.0);
            let sun = (u * TAU).cos().mul_add(0.5, 0.5);
            let transmittance = (0.35 + 0.65 * elevation).clamp(0.0, 1.0);
            [
                (0.10 + 0.26 * elevation + 0.08 * sun).max(0.0) * transmittance,
                (0.18 + 0.28 * elevation + 0.07 * sun).max(0.0) * transmittance,
                (0.30 + 0.34 * elevation + 0.04 * sun).max(0.0) * transmittance,
                1.0,
            ]
        });
        Self {
            transmittance,
            multiscatter,
            sky_view,
        }
    }

    pub fn is_finite_non_negative(&self) -> bool {
        self.transmittance
            .iter()
            .chain(&self.multiscatter)
            .chain(&self.sky_view)
            .flatten()
            .all(|value| value.is_finite() && *value >= 0.0)
    }
}

fn generate_2d(size: (u32, u32), mut f: impl FnMut(f32, f32) -> [f32; 4]) -> Vec<[f32; 4]> {
    let mut result = Vec::with_capacity((size.0 * size.1) as usize);
    for y in 0..size.1 {
        for x in 0..size.0 {
            result.push(f(
                (x as f32 + 0.5) / size.0 as f32,
                (y as f32 + 0.5) / size.1 as f32,
            ));
        }
    }
    result
}

/// Return the nearest positive ray-sphere intersection.  A ray starting
/// inside the sphere therefore receives the forward exit point only.
pub fn ray_sphere_nearest_positive(
    origin: [f32; 3],
    direction: [f32; 3],
    radius: f32,
) -> Option<f32> {
    let b = origin[0] * direction[0] + origin[1] * direction[1] + origin[2] * direction[2];
    let c = origin[0] * origin[0] + origin[1] * origin[1] + origin[2] * origin[2] - radius * radius;
    let discriminant = b * b - c;
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    [(-b - root), (-b + root)]
        .into_iter()
        .filter(|value| *value > 1e-5 && value.is_finite())
        .reduce(f32::min)
}

pub fn transmittance_uv(height_fraction: f32, mu: f32) -> [f32; 2] {
    [
        ((mu + 1.0) * 0.5).clamp(0.0, 1.0),
        height_fraction.clamp(0.0, 1.0).sqrt(),
    ]
}

pub fn multiscatter_uv(height_fraction: f32, sun_mu: f32) -> [f32; 2] {
    transmittance_uv(height_fraction, sun_mu)
}

pub fn sky_view_uv(direction: [f32; 3]) -> [f32; 2] {
    let u = (direction[2].atan2(direction[0]) / TAU + 0.5)
        .fract()
        .rem_euclid(1.0);
    let v = (0.5 - direction[1].clamp(-1.0, 1.0).asin() / PI).clamp(0.0, 1.0);
    [u, v]
}

pub fn sky_view_needs_update(frame_index: u64, phase_delta: f32) -> bool {
    frame_index.is_multiple_of(SKY_UPDATE_CADENCE) || phase_delta.abs() >= SKY_PHASE_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atmosphere_luts_are_finite() {
        let luts = AtmosphereLuts::generate(AtmosphereConstants::default());
        assert_eq!(
            luts.transmittance.len(),
            (TRANSMITTANCE_SIZE.0 * TRANSMITTANCE_SIZE.1) as usize
        );
        assert_eq!(
            luts.multiscatter.len(),
            (MULTISCATTER_SIZE.0 * MULTISCATTER_SIZE.1) as usize
        );
        assert_eq!(
            luts.sky_view.len(),
            (SKY_VIEW_SIZE.0 * SKY_VIEW_SIZE.1) as usize
        );
        assert!(luts.is_finite_non_negative());
    }

    #[test]
    fn sky_update_cadence_is_bounded() {
        assert!(sky_view_needs_update(0, 0.0));
        assert!(!sky_view_needs_update(1, 0.0001));
        assert!(sky_view_needs_update(1, 0.0005));
        assert!(sky_view_needs_update(8, 0.0));
    }
}
