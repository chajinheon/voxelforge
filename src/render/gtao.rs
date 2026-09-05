//! CPU GTAO references and GPU-pass contracts for M7.2.

pub const HALF_RESOLUTION: u32 = 2;
pub const RADIUS_BLOCKS: f32 = 1.5;
pub const FALLOFF_START: f32 = 0.6 * RADIUS_BLOCKS;
pub const THICKNESS: f32 = 0.20;
pub const DIRECTIONS: usize = 8;
pub const STEPS: usize = 4;
pub const HISTORY_STATIC_WEIGHT: f32 = 0.90;
pub const HISTORY_MOTION_WEIGHT: f32 = 0.75;
pub const DEPTH_EDGE_ABSOLUTE: f32 = 0.5;
pub const DEPTH_EDGE_RELATIVE: f32 = 0.02;
pub const NORMAL_REJECT_DOT: f32 = 0.85;
pub const BILATERAL_DEPTH_SIGMA: f32 = 0.75;
pub const BILATERAL_NORMAL_POWER: f32 = 32.0;
pub const GTAO_WGSL: &str = include_str!("../../assets/shaders/gtao.wgsl");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GtaoConfig {
    pub directions: usize,
    pub steps: usize,
    pub temporal: bool,
}

impl Default for GtaoConfig {
    fn default() -> Self {
        Self {
            directions: DIRECTIONS,
            steps: STEPS,
            temporal: true,
        }
    }
}

/// A ray's horizon contribution in cosine space.  `horizon_cos = 1` is an
/// unobstructed ray; a lower value represents a nearby occluding horizon.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HorizonSample {
    pub horizon_cos: f32,
    pub distance: f32,
}

/// Deterministic horizon integration used by tests and useful as a CPU fallback.
/// The result follows the shader convention: 1 is fully unoccluded.
pub fn horizon_factor(samples: &[HorizonSample]) -> f32 {
    if samples.is_empty() {
        return 1.0;
    }
    let mut occlusion = 0.0;
    for sample in samples {
        let distance = sample.distance.max(0.0);
        let distance_weight = if distance <= FALLOFF_START {
            1.0
        } else {
            ((RADIUS_BLOCKS - distance) / (RADIUS_BLOCKS - FALLOFF_START)).clamp(0.0, 1.0)
        };
        let horizon = (1.0 - sample.horizon_cos.clamp(0.0, 1.0)) * distance_weight;
        occlusion += horizon;
    }
    (1.0 - occlusion / samples.len() as f32).clamp(0.0, 1.0)
}

pub fn flat_plane_probe() -> [HorizonSample; DIRECTIONS] {
    [HorizonSample {
        horizon_cos: 1.0,
        distance: RADIUS_BLOCKS,
    }; DIRECTIONS]
}

pub fn right_angle_corner_probe() -> [HorizonSample; DIRECTIONS] {
    let mut samples = flat_plane_probe();
    for sample in &mut samples[..DIRECTIONS / 2] {
        sample.horizon_cos = 0.0;
        sample.distance = 0.9;
    }
    samples
}

pub fn temporal_weight(motion_pixels: f32) -> f32 {
    let t = (motion_pixels.max(0.0) / 1.0).clamp(0.0, 1.0);
    HISTORY_STATIC_WEIGHT + (HISTORY_MOTION_WEIGHT - HISTORY_STATIC_WEIGHT) * t
}

pub fn history_rejected(
    depth_delta: f32,
    center_depth: f32,
    normal_dot: f32,
    motion_pixels: f32,
) -> bool {
    depth_delta.abs() > DEPTH_EDGE_ABSOLUTE
        || depth_delta.abs() > center_depth.abs().max(0.001) * DEPTH_EDGE_RELATIVE
        || normal_dot < NORMAL_REJECT_DOT
        || motion_pixels.is_nan()
}

pub fn bilateral_weight(depth_delta: f32, normal_dot: f32) -> f32 {
    let depth = (-0.5 * (depth_delta / BILATERAL_DEPTH_SIGMA).powi(2)).exp();
    depth * normal_dot.clamp(0.0, 1.0).powf(BILATERAL_NORMAL_POWER)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GtaoTargetContract {
    pub output_format: &'static str,
    pub half_resolution: bool,
    pub max_directions: usize,
    pub max_steps: usize,
}

impl Default for GtaoTargetContract {
    fn default() -> Self {
        Self {
            output_format: "R8Unorm",
            half_resolution: true,
            max_directions: DIRECTIONS,
            max_steps: STEPS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gtao_flat_plane_is_unoccluded() {
        assert!(horizon_factor(&flat_plane_probe()) >= 0.92);
    }

    #[test]
    fn gtao_corner_is_occluded() {
        assert!(horizon_factor(&right_angle_corner_probe()) <= 0.62);
    }

    #[test]
    fn gtao_depth_edge_rejects_history() {
        assert!(history_rejected(0.51, 4.0, 1.0, 0.0));
        assert!(history_rejected(0.1, 4.0, 0.8, 0.0));
        assert!(!history_rejected(0.01, 4.0, 0.99, 0.0));
    }
}
