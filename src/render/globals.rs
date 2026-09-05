//! Per-frame global uniform data.

use super::day_cycle::DayState;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

/// The group(0) uniform. Its layout mirrors the `Globals` WGSL structure.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Globals {
    pub view_proj: [[f32; 4]; 4],
    pub cam_pos: [f32; 4],
    pub sun_dir: [f32; 4],
    pub time_res: [f32; 4],
    pub sky_color: [f32; 4],
    pub unjittered_view_proj: [[f32; 4]; 4],
    pub previous_view_proj: [[f32; 4]; 4],
}

impl Globals {
    /// Build globals from the camera matrix and canonical day-cycle state.
    pub fn from_day(
        view_proj: Mat4,
        cam_pos: Vec3,
        time: f32,
        width: f32,
        height: f32,
        day: DayState,
    ) -> Self {
        Self {
            view_proj: view_proj.to_cols_array_2d(),
            unjittered_view_proj: view_proj.to_cols_array_2d(),
            previous_view_proj: view_proj.to_cols_array_2d(),
            cam_pos: [cam_pos.x, cam_pos.y, cam_pos.z, 1.0],
            sun_dir: [day.sun_dir.x, day.sun_dir.y, day.sun_dir.z, day.sun_factor],
            time_res: [time, width, height, 0.0],
            sky_color: [day.sky_color.x, day.sky_color.y, day.sky_color.z, day.phase],
        }
    }

    /// Upload this frame's values to an existing uniform buffer.
    pub fn write(&self, queue: &wgpu::Queue, buffer: &wgpu::Buffer) {
        queue.write_buffer(buffer, 0, bytemuck::bytes_of(self));
    }
}

pub const GLOBALS_SIZE: u64 = std::mem::size_of::<Globals>() as u64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globals_are_exactly_256_bytes() {
        assert_eq!(std::mem::size_of::<Globals>(), 256);
        assert_eq!(GLOBALS_SIZE, 256);
    }
}
