//! Per-frame global uniform data.

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
}

impl Globals {
    /// Build globals from the camera matrix and frame values.
    pub fn new(
        view_proj: Mat4,
        cam_pos: Vec3,
        sun_dir: Vec3,
        time: f32,
        width: f32,
        height: f32,
    ) -> Self {
        Self {
            view_proj: view_proj.to_cols_array_2d(),
            cam_pos: [cam_pos.x, cam_pos.y, cam_pos.z, 1.0],
            sun_dir: [sun_dir.x, sun_dir.y, sun_dir.z, 0.0],
            time_res: [time, width, height, 0.0],
        }
    }

    /// Upload this frame's values to an existing uniform buffer.
    pub fn write(&self, queue: &wgpu::Queue, buffer: &wgpu::Buffer) {
        queue.write_buffer(buffer, 0, bytemuck::bytes_of(self));
    }
}

pub const GLOBALS_SIZE: u64 = std::mem::size_of::<Globals>() as u64;
