//! Shared temporal rejection and quarter-resolution checkerboard references.

use glam::{Vec2, Vec3};

pub const VOLUMETRIC_HISTORY_WEIGHT: f32 = 0.93;
pub const VOLUMETRIC_MOTION_WEIGHT: f32 = 0.80;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CheckerboardCoord {
    pub x_parity: u32,
    pub y_parity: u32,
}

impl CheckerboardCoord {
    pub fn for_frame(frame_index: u64) -> Self {
        Self {
            x_parity: (frame_index as u32) & 1,
            y_parity: ((frame_index as u32) >> 1) & 1,
        }
    }

    pub fn is_active(self, x: u32, y: u32) -> bool {
        (x & 1) == self.x_parity && (y & 1) == self.y_parity
    }

    pub fn active_pixels(self, width: u32, height: u32) -> impl Iterator<Item = (u32, u32)> {
        (0..height).flat_map(move |y| {
            (0..width).filter_map(move |x| self.is_active(x, y).then_some((x, y)))
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RejectReason {
    #[default]
    Accepted,
    HistoryReset,
    PreviousUvOutOfBounds,
    RelativeDepth,
    AbsoluteDepth,
    NormalDiscontinuity,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TemporalInput {
    pub current_uv: Vec2,
    pub previous_uv: Vec2,
    pub current_depth: f32,
    pub previous_depth: f32,
    pub current_normal: Vec3,
    pub previous_normal: Vec3,
    pub history_reset: bool,
}

pub fn reject(input: TemporalInput) -> RejectReason {
    if input.history_reset {
        return RejectReason::HistoryReset;
    }
    if input.previous_uv.x < 0.0
        || input.previous_uv.x > 1.0
        || input.previous_uv.y < 0.0
        || input.previous_uv.y > 1.0
    {
        return RejectReason::PreviousUvOutOfBounds;
    }
    let depth_delta = (input.previous_depth - input.current_depth).abs();
    if depth_delta > 0.5 {
        return RejectReason::AbsoluteDepth;
    }
    if depth_delta > input.current_depth.abs().max(1.0e-4) * 0.02 {
        return RejectReason::RelativeDepth;
    }
    if input
        .current_normal
        .normalize_or_zero()
        .dot(input.previous_normal.normalize_or_zero())
        < 0.90
    {
        return RejectReason::NormalDiscontinuity;
    }
    RejectReason::Accepted
}

pub fn history_weight(motion_pixels: f32, reason: RejectReason) -> f32 {
    if reason != RejectReason::Accepted {
        return 0.0;
    }
    if motion_pixels.abs() > 2.0 {
        VOLUMETRIC_MOTION_WEIGHT
    } else {
        VOLUMETRIC_HISTORY_WEIGHT
    }
}

pub fn blend(current: Vec3, history: Vec3, motion_pixels: f32, input: TemporalInput) -> Vec3 {
    let reason = reject(input);
    current.lerp(history, history_weight(motion_pixels, reason))
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HistoryKey {
    pub width: u32,
    pub height: u32,
    pub render_scale: f32,
    pub camera_position: Vec3,
    pub yaw_degrees: f32,
}

impl HistoryKey {
    pub fn needs_reset(self, next: Self, shaderpack_changed: bool) -> bool {
        self.width != next.width
            || self.height != next.height
            || (self.render_scale - next.render_scale).abs() > f32::EPSILON
            || self.camera_position.distance(next.camera_position) > 8.0
            || (self.yaw_degrees - next.yaw_degrees).abs() > 30.0
            || shaderpack_changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_frame_checkerboard_visits_every_parity() {
        let mut seen = [(false, false); 4];
        for frame in 0..4 {
            let c = CheckerboardCoord::for_frame(frame);
            seen[(c.x_parity + c.y_parity * 2) as usize] = (true, true);
        }
        assert!(seen.into_iter().all(|value| value == (true, true)));
    }

    #[test]
    fn disocclusion_history_weight_is_zero() {
        let mut input = TemporalInput {
            current_uv: Vec2::splat(0.5),
            previous_uv: Vec2::splat(0.5),
            current_depth: 10.0,
            previous_depth: 10.0,
            current_normal: Vec3::Z,
            previous_normal: Vec3::Z,
            history_reset: false,
        };
        assert_eq!(history_weight(0.0, reject(input)), 0.93);
        input.previous_depth = 11.0;
        assert_eq!(history_weight(0.0, reject(input)), 0.0);
    }

    #[test]
    fn reset_thresholds_match_contract() {
        let base = HistoryKey {
            width: 100,
            height: 100,
            render_scale: 0.72,
            camera_position: Vec3::ZERO,
            yaw_degrees: 0.0,
        };
        let mut next = base;
        next.camera_position.x = 8.01;
        assert!(base.needs_reset(next, false));
        next = base;
        next.yaw_degrees = 30.01;
        assert!(base.needs_reset(next, false));
    }
}
