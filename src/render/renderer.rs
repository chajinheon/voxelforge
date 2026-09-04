//! Surface-independent frame renderer.

use std::path::Path;

use super::chunk_pipeline::{ChunkPipeline, GpuChunk};
use super::globals::Globals;
use super::shader_watch::ShaderWatcher;
use super::textures::BlockTextures;

const SKY: wgpu::Color = wgpu::Color {
    r: 0.53,
    g: 0.81,
    b: 0.92,
    a: 1.0,
};

/// Owns render resources but accepts views supplied by either a window or an
/// offscreen target. No `Surface` or `Window` is part of this type.
pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub textures: BlockTextures,
    pub chunks: ChunkPipeline,
    watcher: ShaderWatcher,
    color_format: wgpu::TextureFormat,
}

impl Renderer {
    /// Initialize from an existing surface-free device context.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
    ) -> anyhow::Result<Self> {
        Self::with_assets(device, queue, color_format, &crate::assets::dir())
    }

    /// Initialize with an explicit asset root, useful for tests and tools.
    pub fn with_assets(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        asset_root: &Path,
    ) -> anyhow::Result<Self> {
        let textures = BlockTextures::from_registry(device, queue, asset_root)?;
        let shader_path = asset_root.join("shaders/chunk.wgsl");
        let chunks = ChunkPipeline::new(device, queue, color_format, &textures, &shader_path)?;
        Ok(Self {
            device: device.clone(),
            queue: queue.clone(),
            textures,
            chunks,
            watcher: ShaderWatcher::new(shader_path),
            color_format,
        })
    }

    pub fn upload_chunk(
        &mut self,
        mesh: &crate::mesh::mesher::ChunkMesh,
        origin: glam::IVec3,
    ) -> anyhow::Result<GpuChunk> {
        self.chunks
            .upload_chunk(&self.device, &self.queue, mesh, origin)
    }

    pub fn remove_chunk(&mut self, chunk: GpuChunk) {
        self.chunks.remove_chunk(chunk);
    }

    /// Force a shader recompile on the next frame.
    pub fn force_shader_reload(&mut self) {
        self.watcher.force();
    }

    /// Render one opaque frame into caller-owned color/depth views.
    pub fn render(
        &mut self,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        globals: &Globals,
        chunks: &[GpuChunk],
    ) {
        if self.watcher.poll() {
            let _ = self.chunks.reload_shader(&self.device, self.color_format);
        }
        self.chunks.update_globals(&self.queue, globals);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("voxelforge-frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("voxelforge-opaque-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(SKY),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.chunks.draw(&mut pass, chunks);
        }
        self.queue.submit([encoder.finish()]);
    }
}
