//! Deterministic M6 sun and sky cycle.

use glam::{Vec2, Vec3};

pub const DAY_LENGTH_SECONDS: f32 = 1200.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DayState {
    pub phase: f32,
    pub sun_dir: Vec3,
    pub sun_factor: f32,
    pub sky_color: Vec3,
}

pub fn day_state(world_time_seconds: f32) -> DayState {
    let phase = (world_time_seconds / DAY_LENGTH_SECONDS).rem_euclid(1.0);
    let theta = std::f32::consts::TAU * phase;
    let sun_dir =
        Vec3::new(0.25 * theta.sin(), -theta.cos(), 0.968_245_8 * theta.sin()).normalize();
    let sun_height = (-sun_dir.y).clamp(-1.0, 1.0);
    let day_weight = smoothstep(-0.08, 0.12, sun_height);
    let sun_factor = 0.08 + (1.0 - 0.08) * day_weight;
    let sky_color = sky_at(phase);
    DayState {
        phase,
        sun_dir,
        sun_factor,
        sky_color,
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn sky_at(phase: f32) -> Vec3 {
    const P: [f32; 9] = [0.0, 0.18, 0.25, 0.32, 0.50, 0.68, 0.75, 0.82, 1.0];
    const C: [[f32; 3]; 9] = [
        [0.530, 0.810, 0.920],
        [0.420, 0.580, 0.720],
        [0.320, 0.120, 0.055],
        [0.035, 0.055, 0.120],
        [0.004, 0.008, 0.025],
        [0.035, 0.055, 0.120],
        [0.320, 0.120, 0.055],
        [0.420, 0.580, 0.720],
        [0.530, 0.810, 0.920],
    ];
    for i in 0..P.len() - 1 {
        if phase <= P[i + 1] {
            let t = smoothstep(0.0, 1.0, (phase - P[i]) / (P[i + 1] - P[i]));
            return Vec3::from_array(C[i]).lerp(Vec3::from_array(C[i + 1]), t);
        }
    }
    Vec3::from_array(C[0])
}

/// Shared deterministic XZ wind displacement used by leaves and grass.
pub fn wind_offset(world_pos: Vec3, time: f32, amplitude: f32) -> Vec2 {
    let phase = world_pos.x * 0.173 + world_pos.z * 0.127 + time * 1.7;
    let gust = (phase.sin() + (phase * 2.17 + 1.3).sin() * 0.5) / 1.5;
    Vec2::new(0.8, 0.6).normalize() * (gust * amplitude)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_noon_and_midnight_are_opposites() {
        assert!(day_state(0.0).sun_dir.y < -0.99);
        assert!(day_state(600.0).sun_dir.y > 0.99);
        assert!(day_state(0.0).sun_factor > 0.99);
        assert!((day_state(600.0).sun_factor - 0.08).abs() < 1e-6);
    }

    #[test]
    fn wind_offset_matches_at_shared_world_vertex() {
        let left_local = Vec3::new(32.0, 17.0, 28.0);
        let left_origin = Vec3::new(0.0, 0.0, -32.0);
        let right_local = Vec3::new(0.0, 17.0, 28.0);
        let right_origin = Vec3::new(32.0, 0.0, -32.0);
        let a = wind_offset(left_origin + left_local, 4.5, 0.045);
        let b = wind_offset(right_origin + right_local, 4.5, 0.045);
        assert!((a - b).length() < 1e-6);
        assert!((a - Vec2::new(-0.003_464_42, -0.002_598_315)).length() < 1e-6);
    }
}
