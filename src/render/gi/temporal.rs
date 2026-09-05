use glam::{Vec2, Vec3, Vec4};

pub const HISTORY_WEIGHT: f32 = 0.90;
pub const RELATIVE_DEPTH_REJECT: f32 = 0.02;
pub const ABSOLUTE_DEPTH_REJECT: f32 = 0.50;
pub const NORMAL_REJECT_DOT: f32 = 0.90;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TemporalSample {
    pub current: Vec4,
    pub history: Vec4,
    pub current_depth: f32,
    pub previous_depth: f32,
    pub current_normal: Vec3,
    pub previous_normal: Vec3,
    pub current_material: u32,
    pub previous_material: u32,
    pub previous_uv: Vec2,
}

pub fn history_accepts(sample: TemporalSample) -> bool {
    if !sample.previous_uv.x.is_finite()
        || !sample.previous_uv.y.is_finite()
        || sample.previous_uv.x < 0.0
        || sample.previous_uv.x > 1.0
        || sample.previous_uv.y < 0.0
        || sample.previous_uv.y > 1.0
    {
        return false;
    }
    let delta = (sample.current_depth - sample.previous_depth).abs();
    let relative = delta / sample.current_depth.abs().max(1e-4);
    relative <= RELATIVE_DEPTH_REJECT
        && delta <= ABSOLUTE_DEPTH_REJECT
        && sample
            .current_normal
            .normalize_or_zero()
            .dot(sample.previous_normal.normalize_or_zero())
            >= NORMAL_REJECT_DOT
        && sample.current_material == sample.previous_material
}

pub fn temporal_resolve(
    sample: TemporalSample,
    neighborhood_min: Vec4,
    neighborhood_max: Vec4,
) -> Vec4 {
    if !history_accepts(sample) {
        return sample.current;
    }
    let lo = neighborhood_min.min(neighborhood_max);
    let hi = neighborhood_min.max(neighborhood_max);
    let span = (hi - lo) * 0.10;
    let clamped = sample.history.clamp(lo - span, hi + span);
    sample.current.lerp(clamped, HISTORY_WEIGHT)
}

#[derive(Clone, Debug, Default)]
pub struct TemporalHistory {
    pub valid: bool,
    pub width: u32,
    pub height: u32,
    pub camera_position: Vec3,
    pub clipmap_revision: u32,
}

impl TemporalHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.valid = false;
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            self.reset();
        }
    }

    pub fn update_camera(&mut self, position: Vec3) {
        if self.valid && position.distance(self.camera_position) > 8.0 {
            self.reset();
        }
        self.camera_position = position;
    }

    pub fn update_clipmap_revision(&mut self, revision: u32, rebuild_ratio: f32) {
        if self.valid && (revision != self.clipmap_revision || rebuild_ratio >= 0.10) {
            self.reset();
        }
        self.clipmap_revision = revision;
    }

    pub fn commit(&mut self) {
        self.valid = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(depth: f32, previous_depth: f32, normal: Vec3) -> TemporalSample {
        TemporalSample {
            current: Vec4::new(1.0, 2.0, 3.0, 1.0),
            history: Vec4::splat(4.0),
            current_depth: depth,
            previous_depth,
            current_normal: Vec3::Z,
            previous_normal: normal,
            current_material: 1,
            previous_material: 1,
            previous_uv: Vec2::splat(0.5),
        }
    }

    #[test]
    fn temporal_gi_rejects_depth_disocclusion() {
        let input = sample(10.0, 10.5, Vec3::Z);
        assert_eq!(
            temporal_resolve(input, Vec4::ZERO, Vec4::ONE),
            input.current
        );
    }

    #[test]
    fn temporal_gi_rejects_normal_disocclusion() {
        let input = sample(10.0, 10.0, -Vec3::Z);
        assert_eq!(
            temporal_resolve(input, Vec4::ZERO, Vec4::ONE),
            input.current
        );
    }
}
