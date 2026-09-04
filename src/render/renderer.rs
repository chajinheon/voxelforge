//! Surface-independent frame renderer.

use std::path::Path;

use super::chunk_pipeline::{ChunkPipeline, GpuChunk, GpuChunkMeshes};
use super::frustum::Frustum;
use super::globals::Globals;
use super::outline::OutlinePipeline;
use super::shader_watch::ShaderWatcher;
use super::textures::BlockTextures;
use super::translucent::TranslucentPipeline;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderView {
    #[default]
    Final,
    Light,
}

/// Owns render resources but accepts views supplied by either a window or an
/// offscreen target. No `Surface` or `Window` is part of this type.
pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub textures: BlockTextures,
    pub chunks: ChunkPipeline,
    pub translucent: TranslucentPipeline,
    pub outline: OutlinePipeline,
    watcher: ShaderWatcher,
    outline_watcher: ShaderWatcher,
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
        let translucent =
            TranslucentPipeline::new(device, queue, color_format, &textures, &shader_path)?;
        let outline_path = asset_root.join("shaders/outline.wgsl");
        let outline = OutlinePipeline::new(
            device,
            color_format,
            chunks.globals_layout(),
            chunks.globals_bind_group(),
            &outline_path,
        )?;
        Ok(Self {
            device: device.clone(),
            queue: queue.clone(),
            textures,
            chunks,
            translucent,
            outline,
            watcher: ShaderWatcher::new(shader_path),
            outline_watcher: ShaderWatcher::new(outline_path),
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

    /// Upload both render passes for one chunk. If either pass fails, the
    /// already-uploaded pass is returned to its arena before reporting error.
    pub fn upload_chunk_meshes(
        &mut self,
        meshes: &crate::mesh::mesher::ChunkMeshes,
        origin: glam::IVec3,
    ) -> anyhow::Result<GpuChunkMeshes> {
        let opaque = if meshes.opaque.vertices.is_empty() || meshes.opaque.indices.is_empty() {
            None
        } else {
            Some(
                self.chunks
                    .upload_chunk(&self.device, &self.queue, &meshes.opaque, origin)?,
            )
        };
        let translucent = match if meshes.translucent.vertices.is_empty()
            || meshes.translucent.indices.is_empty()
        {
            Ok(None)
        } else {
            self.translucent
                .upload_chunk(&self.device, &self.queue, &meshes.translucent, origin)
                .map(Some)
        } {
            Ok(chunk) => chunk,
            Err(error) => {
                if let Some(chunk) = opaque {
                    self.chunks.remove_chunk(chunk);
                }
                return Err(error);
            }
        };
        Ok(GpuChunkMeshes {
            opaque,
            translucent,
            origin,
        })
    }

    pub fn remove_chunk_meshes(&mut self, meshes: GpuChunkMeshes) {
        if let Some(chunk) = meshes.opaque {
            self.chunks.remove_chunk(chunk);
        }
        if let Some(chunk) = meshes.translucent {
            self.translucent.remove_chunk(chunk);
        }
    }

    /// Force a shader recompile on the next frame.
    pub fn force_shader_reload(&mut self) {
        self.watcher.force();
        self.outline_watcher.force();
    }

    /// Render one opaque frame into caller-owned color/depth views.
    pub fn render(
        &mut self,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        globals: &Globals,
        chunks: &[GpuChunkMeshes],
    ) -> usize {
        self.render_internal(
            color_view,
            depth_view,
            globals,
            chunks,
            None,
            RenderView::Final,
        )
    }

    pub fn render_view(
        &mut self,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        globals: &Globals,
        chunks: &[GpuChunkMeshes],
        view: RenderView,
    ) -> usize {
        self.render_internal(color_view, depth_view, globals, chunks, None, view)
    }

    /// Render an opaque frame and optionally draw the selected block outline.
    pub fn render_with_outline(
        &mut self,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        globals: &Globals,
        chunks: &[GpuChunkMeshes],
        selected_block: Option<glam::IVec3>,
    ) -> usize {
        self.render_internal(
            color_view,
            depth_view,
            globals,
            chunks,
            selected_block,
            RenderView::Final,
        )
    }

    fn render_internal(
        &mut self,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        globals: &Globals,
        chunks: &[GpuChunkMeshes],
        selected_block: Option<glam::IVec3>,
        view: RenderView,
    ) -> usize {
        if self.watcher.poll() {
            let _ = self.chunks.reload_shader(&self.device, self.color_format);
            let _ = self
                .translucent
                .reload_shader(&self.device, self.color_format);
        }
        if self.outline_watcher.poll() {
            let _ = self.outline.reload_shader(&self.device, self.color_format);
        }
        self.chunks.update_globals(&self.queue, globals);
        self.translucent.update_globals(&self.queue, globals);
        if let Some(block) = selected_block {
            self.outline.update(&self.queue, block);
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("voxelforge-frame"),
            });
        let frustum = Frustum::from_view_proj(glam::Mat4::from_cols_array_2d(&globals.view_proj));
        let visible_chunks = chunks
            .iter()
            .filter(|chunk| {
                let min = chunk.origin.as_vec3();
                let max = min + glam::Vec3::splat(crate::world::coords::CHUNK_SIZE as f32);
                frustum.intersects_aabb(min, max)
                    && (chunk.opaque.is_some() || chunk.translucent.is_some())
            })
            .count();
        let opaque: Vec<&GpuChunk> = chunks
            .iter()
            .filter_map(|chunk| chunk.opaque.as_ref())
            .collect();
        let mut translucent: Vec<&GpuChunk> = chunks
            .iter()
            .filter_map(|chunk| chunk.translucent.as_ref())
            .collect();
        let camera =
            glam::Vec3::from_array([globals.cam_pos[0], globals.cam_pos[1], globals.cam_pos[2]]);
        translucent.sort_by(|a, b| {
            let distance = |chunk: &GpuChunk| {
                let center = chunk.origin.as_vec3()
                    + glam::Vec3::splat(crate::world::coords::CHUNK_SIZE as f32 * 0.5);
                center.distance_squared(camera)
            };
            distance(b)
                .partial_cmp(&distance(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("voxelforge-opaque-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: globals.sky_color[0] as f64,
                            g: globals.sky_color[1] as f64,
                            b: globals.sky_color[2] as f64,
                            a: 1.0,
                        }),
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
            match view {
                RenderView::Final => {
                    self.chunks.draw_visible(&mut pass, &opaque, &frustum);
                    if selected_block.is_some() {
                        self.outline.draw(&mut pass);
                    }
                    self.translucent.draw(&mut pass, &translucent, &frustum);
                }
                RenderView::Light => {
                    self.chunks.draw_visible_light(&mut pass, &opaque, &frustum);
                    self.translucent
                        .draw_light(&mut pass, &translucent, &frustum);
                }
            }
        }
        self.queue.submit([encoder.finish()]);
        visible_chunks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{Gpu, OffscreenTarget, day_state};
    use glam::{Mat4, Vec3};

    #[test]
    fn shader_hotreload_preserves_final_and_light_pipelines_on_error() -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!(
            "voxelforge-hotreload-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let shaders = root.join("shaders");
        std::fs::create_dir_all(&shaders)?;
        let chunk_path = shaders.join("chunk.wgsl");
        let outline_path = shaders.join("outline.wgsl");
        const CHUNK_SHADER: &str = include_str!("../../assets/shaders/chunk.wgsl");
        const OUTLINE_SHADER: &str = include_str!("../../assets/shaders/outline.wgsl");
        std::fs::write(&chunk_path, CHUNK_SHADER)?;
        std::fs::write(&outline_path, OUTLINE_SHADER)?;

        let gpu = Gpu::new()?;
        let mut renderer = Renderer::with_assets(
            &gpu.device,
            &gpu.queue,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            &root,
        )?;
        let mut padded = crate::world::chunk::PaddedChunk::new();
        padded.set(0, 0, 0, crate::world::block::STONE);
        let mesh = crate::mesh::mesh_chunk_greedy_all(&padded);
        let chunks = vec![renderer.upload_chunk_meshes(&mesh, glam::IVec3::ZERO)?];
        std::fs::write(&chunk_path, "this is not valid wgsl")?;
        assert!(
            !renderer
                .chunks
                .reload_shader(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb)
        );

        let target = OffscreenTarget::new(&gpu.device, 8, 8)?;
        let globals = Globals::from_day(Mat4::IDENTITY, Vec3::ZERO, 0.0, 8.0, 8.0, day_state(0.0));
        renderer.render_view(
            &target.color_view,
            &target.depth_view,
            &globals,
            &chunks,
            RenderView::Final,
        );
        renderer.render_view(
            &target.color_view,
            &target.depth_view,
            &globals,
            &chunks,
            RenderView::Light,
        );
        target.read_pixels(&gpu.device, &gpu.queue)?;

        std::fs::write(&chunk_path, CHUNK_SHADER)?;
        assert!(
            renderer
                .chunks
                .reload_shader(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb)
        );
        assert!(
            renderer
                .translucent
                .reload_shader(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb)
        );
        assert!(
            renderer
                .outline
                .reload_shader(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb)
        );
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
