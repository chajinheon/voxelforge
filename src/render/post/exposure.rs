//! Frame-rate independent logarithmic exposure adaptation.

pub const LOG_LUMINANCE_FLOOR: f32 = 1.0e-4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExposureConfig {
    pub min_exposure: f32,
    pub max_exposure: f32,
    pub middle_gray: f32,
    pub adaptation_speed: f32,
}

impl Default for ExposureConfig {
    fn default() -> Self {
        Self {
            min_exposure: 0.25,
            max_exposure: 8.0,
            middle_gray: 0.18,
            adaptation_speed: 3.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExposureState {
    pub exposure: f32,
    pub target: f32,
    pub config: ExposureConfig,
}

impl ExposureState {
    pub fn new(config: ExposureConfig) -> Self {
        let initial = config
            .middle_gray
            .clamp(config.min_exposure, config.max_exposure);
        Self {
            exposure: initial,
            target: initial,
            config,
        }
    }

    pub fn update(&mut self, average_log_luminance: f32, delta_seconds: f32) -> f32 {
        let average_log_luminance = if average_log_luminance.is_finite() {
            average_log_luminance
        } else {
            0.0
        };
        self.target = (self.config.middle_gray / 2.0_f32.powf(average_log_luminance))
            .clamp(self.config.min_exposure, self.config.max_exposure);
        let dt = delta_seconds.max(0.0);
        let alpha = 1.0 - (-self.config.adaptation_speed.max(0.0) * dt).exp();
        self.exposure = self.exposure + (self.target - self.exposure) * alpha;
        self.exposure = self
            .exposure
            .clamp(self.config.min_exposure, self.config.max_exposure);
        self.exposure
    }
}

pub fn luminance(rgb: [f32; 3]) -> f32 {
    (rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722).max(LOG_LUMINANCE_FLOOR)
}

pub trait LuminanceSample {
    fn rgb(self) -> [f32; 3];
}

impl LuminanceSample for [f32; 3] {
    fn rgb(self) -> [f32; 3] {
        self
    }
}

impl LuminanceSample for glam::Vec3 {
    fn rgb(self) -> [f32; 3] {
        self.to_array()
    }
}

pub fn average_log_luminance<T: LuminanceSample + Copy>(samples: &[T]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    samples
        .iter()
        .map(|sample| luminance((*sample).rgb()).log2())
        .sum::<f32>()
        / samples.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposure_reduction_matches_cpu_log_average() {
        let values = [[1.0, 1.0, 1.0], [0.5, 0.25, 0.125], [4.0, 2.0, 1.0]];
        let expected = values.iter().map(|v| luminance(*v).log2()).sum::<f32>() / 3.0;
        assert!((average_log_luminance(&values) - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn exposure_adaptation_is_frame_rate_independent() {
        let config = ExposureConfig::default();
        let mut at_30 = ExposureState::new(config);
        let mut at_120 = ExposureState::new(config);
        for _ in 0..60 {
            at_30.update(2.0, 1.0 / 30.0);
        }
        for _ in 0..240 {
            at_120.update(2.0, 1.0 / 120.0);
        }
        let relative = (at_30.exposure - at_120.exposure).abs() / at_120.exposure;
        assert!(relative < 0.005, "relative error {relative}");
    }
}
