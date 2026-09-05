//! WATER-only wave, reflection, refraction, and foam references.

use glam::{Vec2, Vec3};

pub const WATER_MAX_DISPLACEMENT: f32 = 0.149;
pub const SSR_COARSE_STEPS: usize = 40;
pub const SSR_BINARY_REFINEMENT: usize = 5;
pub const SSR_MAX_DISTANCE: f32 = 48.0;
pub const UNDERWATER_MAX_DISTANCE: f32 = 64.0;
pub const SURFACE_ABSORPTION: Vec3 = Vec3::new(0.150, 0.055, 0.025);
pub const SURFACE_SCATTERING: Vec3 = Vec3::new(0.020, 0.160, 0.220);
pub const UNDERWATER_ABSORPTION: Vec3 = Vec3::new(0.160, 0.065, 0.028);
pub const UNDERWATER_SCATTERING: Vec3 = Vec3::new(0.015, 0.120, 0.180);
pub const UNDERWATER_DISTORTION: f32 = 0.0025;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GerstnerWave {
    pub direction: Vec2,
    pub amplitude: f32,
    pub wavelength: f32,
    pub speed: f32,
    pub steepness: f32,
}

pub const WAVES: [GerstnerWave; 4] = [
    GerstnerWave {
        direction: Vec2::new(1.0, 0.2),
        amplitude: 0.075,
        wavelength: 18.0,
        speed: 1.20,
        steepness: 0.35,
    },
    GerstnerWave {
        direction: Vec2::new(-0.6, 0.8),
        amplitude: 0.040,
        wavelength: 9.0,
        speed: 1.00,
        steepness: 0.30,
    },
    GerstnerWave {
        direction: Vec2::new(0.3, -1.0),
        amplitude: 0.022,
        wavelength: 4.5,
        speed: 0.80,
        steepness: 0.22,
    },
    GerstnerWave {
        direction: Vec2::new(-0.8, -0.3),
        amplitude: 0.012,
        wavelength: 2.25,
        speed: 0.60,
        steepness: 0.15,
    },
];

impl GerstnerWave {
    fn normalized_direction(self) -> Vec2 {
        self.direction.normalize_or_zero()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterSurface {
    pub displacement: Vec3,
    pub normal: Vec3,
    pub height_delta: f32,
}

pub fn evaluate_waves(world_xz: Vec2, time: f32) -> WaterSurface {
    let mut horizontal = Vec2::ZERO;
    let mut vertical = 0.0;
    let mut tangent_x = Vec3::X;
    let mut tangent_z = Vec3::Z;
    for wave in WAVES {
        let direction = wave.normalized_direction();
        let k = std::f32::consts::TAU / wave.wavelength;
        let omega = std::f32::consts::TAU * wave.speed / wave.wavelength;
        let phase = k * direction.dot(world_xz) - omega * time;
        let sine = phase.sin();
        let cosine = phase.cos();
        horizontal += wave.steepness * wave.amplitude * direction * cosine;
        vertical += wave.amplitude * sine;
        tangent_x += Vec3::new(
            -wave.steepness * wave.amplitude * direction.x * direction.x * k * sine,
            wave.amplitude * direction.x * k * cosine,
            -wave.steepness * wave.amplitude * direction.x * direction.y * k * sine,
        );
        tangent_z += Vec3::new(
            -wave.steepness * wave.amplitude * direction.x * direction.y * k * sine,
            wave.amplitude * direction.y * k * cosine,
            -wave.steepness * wave.amplitude * direction.y * direction.y * k * sine,
        );
    }
    let normal = tangent_z.cross(tangent_x).normalize_or_zero();
    WaterSurface {
        displacement: Vec3::new(horizontal.x, vertical, horizontal.y),
        normal,
        height_delta: vertical,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SsrHit {
    pub color: Vec3,
    pub uv: Vec2,
    pub distance: f32,
    pub residual: f32,
    pub confidence: f32,
}

pub fn ssr_confidence(uv: Vec2, distance: f32, normal_facing: f32, residual: f32) -> f32 {
    let edge = smoothstep(0.0, 0.08, uv.x.min(uv.y).min(1.0 - uv.x).min(1.0 - uv.y));
    let facing = normal_facing.clamp(0.0, 1.0);
    let distance_fade = 1.0 - smoothstep(0.70, 1.00, distance / SSR_MAX_DISTANCE);
    let residual_fade = (1.0 - residual.max(0.0)).clamp(0.0, 1.0);
    (edge * facing * distance_fade * residual_fade).clamp(0.0, 1.0)
}

pub fn accept_ssr_hit(uv: Vec2, distance: f32, thickness: f32, residual: f32) -> bool {
    uv.x >= 0.0
        && uv.x <= 1.0
        && uv.y >= 0.0
        && uv.y <= 1.0
        && (0.0..=SSR_MAX_DISTANCE).contains(&distance)
        && residual <= thickness
}

pub fn fresnel_schlick(n_dot_v: f32) -> f32 {
    let cosine = n_dot_v.clamp(0.0, 1.0);
    0.02 + 0.98 * (1.0 - cosine).powi(5)
}

pub fn beer_lambert_transmittance(thickness: f32, absorption: Vec3) -> Vec3 {
    (-absorption.max(Vec3::ZERO) * thickness.max(0.0)).exp()
}

pub fn refraction_uv(
    screen_uv: Vec2,
    normal_view_xy: Vec2,
    water_depth: f32,
    opaque_depth: f32,
) -> Vec2 {
    let mut offset = normal_view_xy * 0.015 * (4.0 / water_depth.max(1.0)).min(1.0);
    if opaque_depth < water_depth - 0.1 {
        offset = Vec2::ZERO;
    }
    screen_uv + offset
}

pub fn shoreline_foam(water_thickness: f32, height_delta: f32, noise: f32) -> f32 {
    let shore = 1.0 - smoothstep(0.15, 1.25, water_thickness.max(0.0));
    let crest = smoothstep(0.06, 0.13, height_delta.abs());
    (shore * 0.85 + crest * 0.45).clamp(0.0, 1.0) * smoothstep(0.45, 0.70, noise)
}

pub fn caustic(world_xz: Vec2, time: f32, thickness: f32) -> f32 {
    let a = (world_xz.dot(Vec2::new(1.7, 1.1)) + time * 1.6).sin();
    let b = (world_xz.dot(Vec2::new(-1.3, 1.9)) - time * 1.2).sin();
    let c = (1.0 - (a + b).abs() * 0.5).clamp(0.0, 1.0).powi(6);
    1.0 + 0.18 * c * (-0.12 * thickness.max(0.0)).exp()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterParams {
    pub time: f32,
    pub underwater: f32,
    pub max_distance: f32,
    pub _padding: f32,
    pub absorption: Vec3,
    pub scattering: Vec3,
}

impl Default for WaterParams {
    fn default() -> Self {
        Self {
            time: 0.0,
            underwater: 0.0,
            max_distance: SSR_MAX_DISTANCE,
            _padding: 0.0,
            absorption: SURFACE_ABSORPTION,
            scattering: SURFACE_SCATTERING,
        }
    }
}

/// Std140-compatible representation for the forward water uniform group.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WaterGpuParams {
    pub time: f32,
    pub underwater: f32,
    pub max_distance: f32,
    pub _padding: f32,
    pub absorption: [f32; 3],
    pub _pad0: f32,
    pub scattering: [f32; 3],
    pub _pad1: f32,
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gerstner_displacement_stays_inside_contract() {
        let mut max_height: f32 = 0.0;
        for i in 0..64 {
            max_height = max_height.max(
                evaluate_waves(Vec2::new(i as f32, 0.37), 2.3)
                    .height_delta
                    .abs(),
            );
        }
        assert!(max_height <= WATER_MAX_DISPLACEMENT + 0.000001);
    }

    #[test]
    fn gerstner_includes_horizontal_displacement() {
        let displacement = evaluate_waves(Vec2::new(3.25, -1.75), 0.8).displacement;
        assert!(displacement.x.abs() > 1.0e-5 || displacement.z.abs() > 1.0e-5);
    }

    #[test]
    fn analytic_normal_is_unit_length() {
        let normal = evaluate_waves(Vec2::new(4.0, -3.0), 1.2).normal;
        assert!((normal.length() - 1.0).abs() < 1.0e-4);
    }

    #[test]
    fn adjacent_chunk_water_is_continuous() {
        let left = evaluate_waves(Vec2::new(15.99999, 4.0), 7.0).displacement;
        let right = evaluate_waves(Vec2::new(16.00001, 4.0), 7.0).displacement;
        assert!((left - right).length() < 1.0e-5);
    }

    #[test]
    fn refraction_reverts_to_original_uv_for_foreground_geometry() {
        let uv = Vec2::new(0.25, 0.75);
        assert_eq!(refraction_uv(uv, Vec2::splat(1.0), 5.0, 4.8), uv);
    }

    #[test]
    fn thickness_increases_absorption_monotonically() {
        let absorption = Vec3::new(0.15, 0.055, 0.025);
        let near = beer_lambert_transmittance(1.0, absorption);
        let far = beer_lambert_transmittance(2.0, absorption);
        assert!(far.x < near.x && far.y < near.y && far.z < near.z);
    }

    #[test]
    fn caustic_intensity_is_bounded() {
        for i in 0..64 {
            let value = caustic(
                Vec2::new(i as f32 * 0.37, i as f32 * -0.19),
                i as f32 * 0.11,
                4.0,
            );
            assert!((1.0..=1.180001).contains(&value));
        }
    }

    #[test]
    fn ssr_confidence_is_bounded_and_residual_can_miss() {
        assert!((0.0..=1.0).contains(&ssr_confidence(Vec2::splat(0.5), 2.0, 1.0, 0.0)));
        assert!(!accept_ssr_hit(Vec2::splat(0.5), 2.0, 0.18, 0.19));
    }
}
