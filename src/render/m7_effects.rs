//! Persistent M7 HDR post-processing graph.

mod atmosphere;
mod graph;
mod gtao;
mod gtao_gpu;
mod resources;

pub use graph::M7Effects;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct M7EffectParams {
    pub exposure: f32,
    pub ao: f32,
    pub atmosphere: f32,
    pub history: f32,
    pub jitter: [f32; 2],
    pub frame: f32,
    pub _pad: f32,
    pub auto_exposure: f32,
    pub delta_seconds: f32,
    pub phase: f32,
    pub sun_mu: f32,
}

impl Default for M7EffectParams {
    fn default() -> Self {
        Self {
            exposure: 1.0,
            ao: 1.0,
            atmosphere: 1.0,
            history: 0.98,
            jitter: [0.0; 2],
            frame: 0.0,
            _pad: 0.0,
            auto_exposure: 0.0,
            delta_seconds: 1.0 / 60.0,
            phase: 0.0,
            sun_mu: 0.65,
        }
    }
}

pub struct M7EffectInputs<'a> {
    pub hdr: &'a wgpu::TextureView,
    pub depth: &'a wgpu::TextureView,
    pub normal: &'a wgpu::TextureView,
    pub material: &'a wgpu::TextureView,
    pub motion: &'a wgpu::TextureView,
    pub reactive: &'a wgpu::TextureView,
    pub sky: &'a wgpu::TextureView,
    pub inverse_view_proj: [[f32; 4]; 4],
    pub camera_pos: [f32; 3],
    pub viewport: [f32; 2],
}
