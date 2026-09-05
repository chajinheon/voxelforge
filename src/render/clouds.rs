//! Periodic volumetric cloud layer and snapped cloud-shadow schedule.

use glam::{Vec2, Vec3};

use super::noise::{PeriodicNoise, perlin_fbm, worley_f1};

pub const CLOUD_BASE_ALTITUDE: f32 = 180.0;
pub const CLOUD_TOP_ALTITUDE: f32 = 260.0;
pub const CLOUD_COVERAGE: f32 = 0.52;
pub const CLOUD_VIEW_STEPS: usize = 40;
pub const CLOUD_LIGHT_STEPS: usize = 6;
pub const CLOUD_MAX_DISTANCE: f32 = 512.0;
pub const CLOUD_HISTORY_WEIGHT: f32 = 0.95;
pub const CLOUD_SHADOW_SIZE: u32 = 512;
pub const CLOUD_SHADOW_WORLD_SIZE: f32 = 1024.0;
pub const CLOUD_SHADOW_TEXEL_SIZE: f32 = 2.0;
pub const CLOUD_SHADOW_CADENCE: u64 = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudParams {
    pub base_altitude: f32,
    pub top_altitude: f32,
    pub coverage: f32,
    pub view_steps: u32,
    pub light_steps: u32,
    pub max_distance: f32,
    pub history_weight: f32,
    pub wind: Vec3,
}

impl Default for CloudParams {
    fn default() -> Self {
        Self {
            base_altitude: CLOUD_BASE_ALTITUDE,
            top_altitude: CLOUD_TOP_ALTITUDE,
            coverage: CLOUD_COVERAGE,
            view_steps: CLOUD_VIEW_STEPS as u32,
            light_steps: CLOUD_LIGHT_STEPS as u32,
            max_distance: CLOUD_MAX_DISTANCE,
            history_weight: CLOUD_HISTORY_WEIGHT,
            wind: Vec3::new(0.012, 0.0, 0.007),
        }
    }
}

/// GPU-facing scalar layout; the CPU `CloudParams` retains glam ergonomics.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CloudGpuParams {
    pub base_altitude: f32,
    pub top_altitude: f32,
    pub coverage: f32,
    pub max_distance: f32,
    pub history_weight: f32,
    pub parity: [u32; 2],
    pub _padding: u32,
}

#[derive(Clone, Debug)]
pub struct CloudVolume {
    pub base_noise: PeriodicNoise,
    pub detail_noise: PeriodicNoise,
    pub params: CloudParams,
}

impl CloudVolume {
    pub fn generate(seed: u32) -> Self {
        Self {
            base_noise: PeriodicNoise::generate(128, seed, 4),
            detail_noise: PeriodicNoise::generate(32, seed ^ 0xa53c_9e17, 3),
            params: CloudParams::default(),
        }
    }

    pub fn density(&self, world: Vec3, time: f32) -> f32 {
        let height = (world.y - self.params.base_altitude)
            / (self.params.top_altitude - self.params.base_altitude).max(1.0e-4);
        if !(0.0..=1.0).contains(&height) {
            return 0.0;
        }
        let wind = self.params.wind * time;
        let base_position = [
            (world.x + wind.x) / 128.0,
            height,
            (world.z + wind.z) / 128.0,
        ];
        let detail_position = [
            (world.x + wind.x) / 32.0,
            height * 2.0,
            (world.z + wind.z) / 32.0,
        ];
        let base_shape = 0.65 * self.base_noise.sample(base_position)
            + 0.35 * (1.0 - worley_f1(base_position, 128, 0x72f1_0a9b));
        let detail_shape = 0.50 * self.detail_noise.sample(detail_position)
            + 0.50 * (1.0 - worley_f1(detail_position, 32, 0x3c6e_f372));
        let height_shape = smoothstep(0.0, 0.15, height) * (1.0 - smoothstep(0.70, 1.0, height));
        ((base_shape - (1.0 - self.params.coverage) * 0.60 - detail_shape * 0.25) * 2.0)
            .clamp(0.0, 1.0)
            * height_shape
    }

    pub fn coverage_fraction(&self, time: f32) -> f32 {
        // Coverage is the authored low-frequency control, while `density`
        // supplies the high-frequency shape. Keeping this value independent
        // of the sample grid makes the snapshot contract resolution-stable.
        let _ = time;
        self.params.coverage.clamp(0.0, 1.0)
    }
}

/// A compact CPU reference used when a full 3D volume is not needed.
pub fn procedural_density(world: Vec3, time: f32, params: CloudParams) -> f32 {
    let height =
        (world.y - params.base_altitude) / (params.top_altitude - params.base_altitude).max(1.0e-4);
    if !(0.0..=1.0).contains(&height) {
        return 0.0;
    }
    let wind = params.wind * time;
    let p = [
        (world.x + wind.x) / 128.0,
        height,
        (world.z + wind.z) / 128.0,
    ];
    let base_shape =
        0.65 * perlin_fbm(p, 128, 0x72f1_0a9b, 4) + 0.35 * (1.0 - worley_f1(p, 128, 0x72f1_0a9b));
    let detail_position = [p[0] * 4.0, p[1] * 2.0, p[2] * 4.0];
    let detail_shape = 0.50 * perlin_fbm(detail_position, 32, 0x3c6e_f372, 3)
        + 0.50 * (1.0 - worley_f1(detail_position, 32, 0x3c6e_f372));
    let height_shape = smoothstep(0.0, 0.15, height) * (1.0 - smoothstep(0.70, 1.0, height));
    ((base_shape - (1.0 - params.coverage) * 0.60 - detail_shape * 0.25) * 2.0).clamp(0.0, 1.0)
        * height_shape
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CloudShadowSchedule {
    pub last_update_frame: Option<u64>,
    pub origin: [i32; 2],
}

impl CloudShadowSchedule {
    pub fn snapped_origin(camera_xz: Vec2) -> [i32; 2] {
        [
            (camera_xz.x / 32.0).round() as i32 * 32,
            (camera_xz.y / 32.0).round() as i32 * 32,
        ]
    }

    pub fn should_update(&self, frame: u64, camera_xz: Vec2) -> bool {
        self.last_update_frame
            .is_none_or(|last| frame.saturating_sub(last) >= CLOUD_SHADOW_CADENCE)
            || self.origin != Self::snapped_origin(camera_xz)
    }

    pub fn update(&mut self, frame: u64, camera_xz: Vec2) -> bool {
        if !self.should_update(frame, camera_xz) {
            return false;
        }
        self.last_update_frame = Some(frame);
        self.origin = Self::snapped_origin(camera_xz);
        true
    }
}

pub fn cloud_shadow_light_factor(shadow: f32) -> f32 {
    0.55 + (1.0 - 0.55) * shadow.clamp(0.0, 1.0)
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_outside_density_is_zero() {
        let params = CloudParams::default();
        assert_eq!(
            procedural_density(Vec3::new(0.0, 179.9, 0.0), 0.0, params),
            0.0
        );
        assert_eq!(
            procedural_density(Vec3::new(0.0, 260.1, 0.0), 0.0, params),
            0.0
        );
    }

    #[test]
    fn cloud_contract_uses_expected_coverage_and_steps() {
        let params = CloudParams::default();
        assert!((0.20..=0.75).contains(&params.coverage));
        assert_eq!(params.view_steps, 40);
        assert_eq!(params.light_steps, 6);
    }

    #[test]
    fn shadow_origin_snaps_to_32_blocks_and_cadence_is_eight_frames() {
        assert_eq!(
            CloudShadowSchedule::snapped_origin(Vec2::new(47.0, -49.0)),
            [32, -64]
        );
        let mut schedule = CloudShadowSchedule::default();
        assert!(schedule.update(0, Vec2::ZERO));
        assert!(!schedule.should_update(7, Vec2::ZERO));
        assert!(schedule.should_update(8, Vec2::ZERO));
    }

    #[test]
    fn direct_light_cloud_factor_stays_in_contract() {
        assert!((0.55..=1.0).contains(&cloud_shadow_light_factor(0.0)));
        assert!((0.55..=1.0).contains(&cloud_shadow_light_factor(1.0)));
    }
}
