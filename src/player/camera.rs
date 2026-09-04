use glam::{Mat4, Vec3};

/// Height of the camera above the player's feet, in blocks.
pub const EYE_HEIGHT: f32 = 1.62;
/// Half-width of the player's axis-aligned footprint, in blocks.
pub const PLAYER_HALF_W: f32 = 0.3;
/// Player height, in blocks.
pub const PLAYER_H: f32 = 1.8;

/// A right-handed, Y-up flying camera.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    /// Returns the unit direction the camera is facing.
    pub fn forward(&self) -> Vec3 {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        Vec3::new(sin_yaw * cos_pitch, sin_pitch, -cos_yaw * cos_pitch)
    }

    /// Builds the right-handed view matrix specified by the voxel coordinate
    /// convention.
    #[allow(deprecated)]
    pub fn view(&self) -> Mat4 {
        Mat4::look_to_rh(self.pos, self.forward(), Vec3::Y)
    }

    /// Builds a depth-0..1 right-handed perspective projection.
    #[allow(deprecated)]
    pub fn proj(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov_y, aspect, self.near, self.far)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera(yaw: f32, pitch: f32) -> Camera {
        Camera {
            pos: Vec3::ZERO,
            yaw,
            pitch,
            fov_y: 60.0_f32.to_radians(),
            near: 0.05,
            far: 1_000.0,
        }
    }

    #[test]
    fn forward_zero_angles_points_down_negative_z() {
        let forward = camera(0.0, 0.0).forward();
        assert!((forward - Vec3::NEG_Z).length() < 1e-6);
    }

    #[test]
    fn forward_pitch_points_up() {
        let forward = camera(0.0, std::f32::consts::FRAC_PI_2).forward();
        assert!((forward - Vec3::Y).length() < 1e-6);
    }
}
