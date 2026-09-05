use glam::Vec3;
use winit::keyboard::KeyCode;

use super::camera::Camera;
use super::physics::MoveInput;
const DEFAULT_MOUSE_SENSITIVITY: f32 = 0.002;
const MAX_PITCH: f32 = 89.0_f32.to_radians();

/// State of the keyboard and mouse input used to fly the camera.
#[derive(Clone, Copy, Debug, Default)]
pub struct Controller {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    sprint: bool,
    mouse_dx: f64,
    mouse_dy: f64,
    mouse_sensitivity: f32,
    invert_y: bool,
}

impl Controller {
    pub fn new() -> Self {
        Self {
            mouse_sensitivity: DEFAULT_MOUSE_SENSITIVITY,
            ..Self::default()
        }
    }

    pub fn configure_look(&mut self, sensitivity: f32, invert_y: bool) {
        if sensitivity.is_finite() && sensitivity > 0.0 {
            self.mouse_sensitivity = sensitivity;
        }
        self.invert_y = invert_y;
    }

    /// Sets the pressed state of a movement key.
    pub fn set_key(&mut self, key: KeyCode, pressed: bool) {
        match key {
            KeyCode::KeyW => self.forward = pressed,
            KeyCode::KeyS => self.backward = pressed,
            KeyCode::KeyA => self.left = pressed,
            KeyCode::KeyD => self.right = pressed,
            KeyCode::Space => self.up = pressed,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => self.down = pressed,
            KeyCode::ControlLeft | KeyCode::ControlRight => self.sprint = pressed,
            _ => {}
        }
    }

    /// Adds raw mouse motion to be applied on the next update.
    pub fn mouse_motion(&mut self, dx: f64, dy: f64) {
        self.mouse_dx += dx;
        self.mouse_dy += dy;
    }

    /// Applies pending look input and returns a world-space movement wish.
    pub fn update(&mut self, camera: &mut Camera) -> MoveInput {
        camera.yaw += self.mouse_dx as f32 * self.mouse_sensitivity;
        let pitch_sign = if self.invert_y { 1.0 } else { -1.0 };
        camera.pitch += self.mouse_dy as f32 * self.mouse_sensitivity * pitch_sign;
        camera.pitch = camera.pitch.clamp(-MAX_PITCH, MAX_PITCH);
        self.mouse_dx = 0.0;
        self.mouse_dy = 0.0;

        let mut horizontal = Vec3::ZERO;
        let (sin_yaw, cos_yaw) = camera.yaw.sin_cos();
        let forward = Vec3::new(sin_yaw, 0.0, -cos_yaw);
        let right = Vec3::new(cos_yaw, 0.0, sin_yaw);
        if self.forward {
            horizontal += forward;
        }
        if self.backward {
            horizontal -= forward;
        }
        if self.right {
            horizontal += right;
        }
        if self.left {
            horizontal -= right;
        }
        if horizontal != Vec3::ZERO {
            horizontal = horizontal.normalize();
        }

        MoveInput {
            wish: horizontal,
            jump: self.up,
            sprint: self.sprint,
            up: self.up,
            down: self.down,
        }
    }

    /// Clears all held input and pending mouse motion.
    pub fn clear(&mut self) {
        let sensitivity = self.mouse_sensitivity;
        let invert_y = self.invert_y;
        *self = Self {
            mouse_sensitivity: sensitivity,
            invert_y,
            ..Self::default()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn camera() -> Camera {
        Camera {
            pos: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            fov_y: 60.0_f32.to_radians(),
            near: 0.05,
            far: 1_000.0,
        }
    }

    #[test]
    fn diagonal_input_is_normalized() {
        let mut controller = Controller::new();
        let mut camera = camera();
        controller.set_key(KeyCode::KeyW, true);
        controller.set_key(KeyCode::KeyD, true);
        let input = controller.update(&mut camera);
        assert!((input.wish.length() - 1.0).abs() < 1e-5);
        assert!(input.wish.x > 0.0 && input.wish.z < 0.0);
    }

    #[test]
    fn mouse_signs_and_pitch_clamp_are_correct() {
        let mut controller = Controller::new();
        let mut camera = camera();
        controller.mouse_motion(10.0, 10.0);
        controller.update(&mut camera);
        assert!((camera.yaw - 0.02).abs() < 1e-6);
        assert!((camera.pitch + 0.02).abs() < 1e-6);
        controller.mouse_motion(0.0, -100_000.0);
        controller.update(&mut camera);
        assert!((camera.pitch - MAX_PITCH).abs() < 1e-6);
    }

    #[test]
    fn sprint_is_reported_in_move_input() {
        let mut controller = Controller::new();
        let mut camera = camera();
        controller.set_key(KeyCode::KeyW, true);
        controller.set_key(KeyCode::ControlLeft, true);
        let input = controller.update(&mut camera);
        assert!(input.sprint);
    }

    #[test]
    fn vertical_input_is_separate_from_horizontal_wish() {
        let mut controller = Controller::new();
        let mut camera = camera();
        controller.set_key(KeyCode::KeyW, true);
        controller.set_key(KeyCode::Space, true);
        let input = controller.update(&mut camera);
        assert_eq!(input.wish, Vec3::NEG_Z);
        assert!(input.jump && input.up);
    }
}
