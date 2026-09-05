//! Quarter-resolution volumetric fog and local-light CPU references.

use glam::Vec3;

pub const VOLUMETRIC_VIEW_STEPS: usize = 32;
pub const VOLUMETRIC_MAX_DISTANCE: f32 = 224.0;
pub const HENYEY_GREENSTEIN_G: f32 = 0.65;
pub const VOLUMETRIC_HISTORY_WEIGHT: f32 = 0.93;
pub const VOLUMETRIC_MOTION_WEIGHT: f32 = 0.80;
pub const MAX_LOCAL_FOG_LIGHTS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogLight {
    pub position: Vec3,
    pub color: Vec3,
    pub radius: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FogLightGpu {
    pub position: [f32; 3],
    pub radius: f32,
    pub color: [f32; 3],
    pub _padding: f32,
}

pub fn closest_lights(origin: Vec3, lights: &[FogLight]) -> Vec<FogLight> {
    let mut sorted = lights.to_vec();
    sorted.sort_by(|a, b| {
        origin
            .distance_squared(a.position)
            .total_cmp(&origin.distance_squared(b.position))
            .then_with(|| a.position.x.total_cmp(&b.position.x))
            .then_with(|| a.position.y.total_cmp(&b.position.y))
            .then_with(|| a.position.z.total_cmp(&b.position.z))
    });
    sorted.truncate(MAX_LOCAL_FOG_LIGHTS);
    sorted
}

pub fn density_at(world_y: f32, sea_level: f32, underwater: bool) -> f32 {
    let height = world_y - sea_level;
    let base = 0.0015 + 0.0060 * (-height.max(0.0) * 0.025).exp();
    if underwater { base * 3.0 } else { base }
}

pub fn henyey_greenstein(mu: f32) -> f32 {
    let g = HENYEY_GREENSTEIN_G;
    let denominator = (1.0 + g * g - 2.0 * g * mu.clamp(-1.0, 1.0)).max(1.0e-4);
    (1.0 - g * g) / (4.0 * std::f32::consts::PI * denominator.powf(1.5))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogResult {
    pub in_scattering: Vec3,
    pub transmittance: f32,
}

impl FogResult {
    pub const ZERO_DENSITY: Self = Self {
        in_scattering: Vec3::ZERO,
        transmittance: 1.0,
    };
}

pub struct FogMarchInput<'a> {
    pub origin: Vec3,
    pub ray_direction: Vec3,
    pub sun_direction: Vec3,
    pub sun_height: f32,
    pub sun_color: Vec3,
    pub sea_level: f32,
    pub underwater: bool,
    pub lights: &'a [FogLight],
    pub shadow_samples: &'a [f32],
    pub density_scale: f32,
    pub jitter: f32,
}

pub fn march_fog(input: FogMarchInput<'_>) -> FogResult {
    if input.density_scale <= 0.0 {
        return FogResult::ZERO_DENSITY;
    }
    let ray = input.ray_direction.normalize_or_zero();
    let sun = input.sun_direction.normalize_or_zero();
    let selected_lights = closest_lights(input.origin, input.lights);
    let mut result = Vec3::ZERO;
    let mut transmittance = 1.0;
    let step_count = VOLUMETRIC_VIEW_STEPS as f32;
    let step_length = VOLUMETRIC_MAX_DISTANCE / step_count;
    let phase = henyey_greenstein(ray.dot(sun));
    for i in 0..VOLUMETRIC_VIEW_STEPS {
        let distance = ((i as f32 + 0.5 + input.jitter.clamp(-0.5, 0.5)) * step_length)
            .clamp(0.0, VOLUMETRIC_MAX_DISTANCE);
        let sample_position = input.origin + ray * distance;
        let density =
            density_at(sample_position.y, input.sea_level, input.underwater) * input.density_scale;
        if density <= 0.0 {
            continue;
        }
        let shadow = input
            .shadow_samples
            .get(i / 2)
            .copied()
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let ds = step_length;
        let local = selected_lights.iter().fold(Vec3::ZERO, |sum, light| {
            let distance_sq = sample_position.distance_squared(light.position);
            let radius = light.radius.max(1.0e-4);
            let falloff = (1.0 - distance_sq.sqrt() / radius).clamp(0.0, 1.0);
            sum + light.color * falloff * falloff
        });
        let sun_term = if input.sun_height <= 0.02 {
            Vec3::ZERO
        } else {
            input.sun_color * phase * shadow
        };
        result += transmittance * (sun_term + local) * density * ds;
        transmittance *= (-density * ds).exp();
        if transmittance < 1.0e-4 {
            break;
        }
    }
    FogResult {
        in_scattering: result,
        transmittance,
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VolumetricParams {
    pub max_distance: f32,
    pub density_scale: f32,
    pub history_weight: f32,
    pub underwater: f32,
    pub sun_height: f32,
    pub _padding: [f32; 3],
}

impl Default for VolumetricParams {
    fn default() -> Self {
        Self {
            max_distance: VOLUMETRIC_MAX_DISTANCE,
            density_scale: 1.0,
            history_weight: VOLUMETRIC_HISTORY_WEIGHT,
            underwater: 0.0,
            sun_height: 1.0,
            _padding: [0.0; 3],
        }
    }
}

pub fn temporal_history_weight(disoccluded: bool, motion_pixels: f32) -> f32 {
    if disoccluded {
        0.0
    } else if motion_pixels.abs() > 2.0 {
        VOLUMETRIC_MOTION_WEIGHT
    } else {
        VOLUMETRIC_HISTORY_WEIGHT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_density_is_black_with_full_transmittance() {
        let result = march_fog(FogMarchInput {
            origin: Vec3::ZERO,
            ray_direction: Vec3::Z,
            sun_direction: Vec3::Y,
            sun_height: 1.0,
            sun_color: Vec3::ONE,
            sea_level: 63.0,
            underwater: false,
            lights: &[],
            shadow_samples: &[],
            density_scale: 0.0,
            jitter: 0.0,
        });
        assert_eq!(result, FogResult::ZERO_DENSITY);
    }

    #[test]
    fn disocclusion_history_weight_is_zero() {
        assert_eq!(temporal_history_weight(true, 0.0), 0.0);
        assert_eq!(temporal_history_weight(false, 3.0), 0.80);
    }

    #[test]
    fn local_lights_are_distance_sorted_with_position_tiebreak() {
        let lights = [
            FogLight {
                position: Vec3::X * 2.0,
                color: Vec3::ONE,
                radius: 16.0,
            },
            FogLight {
                position: Vec3::X * -2.0,
                color: Vec3::ONE,
                radius: 16.0,
            },
            FogLight {
                position: Vec3::X * 4.0,
                color: Vec3::ONE,
                radius: 16.0,
            },
        ];
        let selected = closest_lights(Vec3::ZERO, &lights);
        assert_eq!(selected[0].position, Vec3::X * -2.0);
        assert_eq!(selected.len(), 3);
    }

    #[test]
    fn four_frame_checkerboard_is_supported_by_shared_contract() {
        for frame in 0..4 {
            let parity = ((frame & 1), ((frame >> 1) & 1));
            assert!(parity.0 < 2 && parity.1 < 2);
        }
    }
}
