//! M7 opaque G-buffer target allocation.

use super::{
    chunk_pipeline::GpuChunk, frustum::Frustum, scene_bindings::SceneBindings,
    shader_source::compose,
};

/// The five color attachments written by the opaque G-buffer pass.
pub const GBUFFER_FORMATS: [wgpu::TextureFormat; 5] = [
    wgpu::TextureFormat::Rgba8UnormSrgb,
    wgpu::TextureFormat::Rgba16Float,
    wgpu::TextureFormat::Rgba8Unorm,
    wgpu::TextureFormat::Rg16Float,
    wgpu::TextureFormat::R8Unorm,
];

pub struct GBuffer {
    pub albedo: wgpu::Texture,
    pub normal: wgpu::Texture,
    pub light: wgpu::Texture,
    pub motion: wgpu::Texture,
    pub reactive: wgpu::Texture,
    pub depth: wgpu::Texture,
    pub albedo_view: wgpu::TextureView,
    pub normal_view: wgpu::TextureView,
    pub light_view: wgpu::TextureView,
    pub motion_view: wgpu::TextureView,
    pub reactive_view: wgpu::TextureView,
    pub depth_view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

/// Opaque geometry pipeline that writes all five M7 G-buffer attachments.
pub struct GBufferPipeline {
    pub pipeline: wgpu::RenderPipeline,
    #[allow(dead_code)]
    shader_module: wgpu::ShaderModule,
    bindings: SceneBindings,
}

impl GBufferPipeline {
    pub fn new(device: &wgpu::Device, bindings: SceneBindings) -> anyhow::Result<Self> {
        let source = compose(include_str!("../../assets/shaders/gbuffer.wgsl"));
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m7/gbuffer/shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vf/m7/gbuffer/layout"),
            bind_group_layouts: &[
                Some(&bindings.globals_layout),
                Some(&bindings.pbr_layout),
                Some(&bindings.chunk_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vf/m7/gbuffer/pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<crate::mesh::vertex::ChunkVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Uint32x2],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &GBUFFER_FORMATS.map(|format| {
                    Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })
                }),
            }),
            multiview_mask: None,
            cache: None,
        });
        if let Some(error) = pollster::block_on(scope.pop()) {
            anyhow::bail!("gbuffer shader validation failed: {error}");
        }
        Ok(Self {
            pipeline,
            shader_module,
            bindings,
        })
    }

    pub fn draw_visible<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        chunks: &[&'a GpuChunk],
        frustum: &Frustum,
    ) -> usize {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bindings.globals_bind_group, &[]);
        pass.set_bind_group(1, &self.bindings.pbr_bind_group, &[]);
        let mut drawn = 0;
        for chunk in chunks {
            let min = chunk.origin.as_vec3();
            let max = min + glam::Vec3::splat(crate::world::coords::CHUNK_SIZE as f32);
            if !frustum.intersects_aabb(min, max) {
                continue;
            }
            pass.set_bind_group(2, &self.bindings.chunk_bind_group, &[chunk.uniform_offset]);
            pass.set_vertex_buffer(0, chunk.vertex_buffer.slice(..));
            pass.set_index_buffer(chunk.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..chunk.index_count, 0, 0..1);
            drawn += 1;
        }
        drawn
    }

    pub fn draw_mesh_indices<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        meshes: &'a [super::chunk_pipeline::GpuChunkMeshes],
        indices: &[usize],
    ) -> usize {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bindings.globals_bind_group, &[]);
        pass.set_bind_group(1, &self.bindings.pbr_bind_group, &[]);
        let mut drawn = 0;
        for &index in indices {
            let Some(chunk) = meshes.get(index).and_then(|mesh| mesh.opaque.as_ref()) else {
                continue;
            };
            pass.set_bind_group(2, &self.bindings.chunk_bind_group, &[chunk.uniform_offset]);
            pass.set_vertex_buffer(0, chunk.vertex_buffer.slice(..));
            pass.set_index_buffer(chunk.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..chunk.index_count, 0, 0..1);
            drawn += 1;
        }
        drawn
    }
}

impl GBuffer {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let make = |format, label| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let albedo = make(GBUFFER_FORMATS[0], "vf/m7/gbuffer/albedo");
        let normal = make(GBUFFER_FORMATS[1], "vf/m7/gbuffer/normal");
        let light = make(GBUFFER_FORMATS[2], "vf/m7/gbuffer/light");
        let motion = make(GBUFFER_FORMATS[3], "vf/m7/gbuffer/motion");
        let reactive = make(GBUFFER_FORMATS[4], "vf/m7/gbuffer/reactive");
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vf/m7/gbuffer/depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view =
            |texture: &wgpu::Texture| texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            albedo_view: view(&albedo),
            normal_view: view(&normal),
            light_view: view(&light),
            motion_view: view(&motion),
            reactive_view: view(&reactive),
            depth_view: view(&depth),
            albedo,
            normal,
            light,
            motion,
            reactive,
            depth,
            width,
            height,
        }
    }

    pub fn color_attachments(&self) -> [Option<wgpu::RenderPassColorAttachment<'_>>; 5] {
        [
            Some(attachment(&self.albedo_view)),
            Some(attachment(&self.normal_view)),
            Some(attachment(&self.light_view)),
            Some(attachment(&self.motion_view)),
            Some(attachment(&self.reactive_view)),
        ]
    }

    /// Byte estimate derived from each live attachment's descriptor.
    pub fn texture_memory_bytes(&self) -> usize {
        [
            &self.albedo,
            &self.normal,
            &self.light,
            &self.motion,
            &self.reactive,
            &self.depth,
        ]
        .into_iter()
        .map(crate::render::memory::texture_bytes)
        .sum()
    }
}

fn attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            store: wgpu::StoreOp::Store,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contract_has_five_color_targets_and_depth() {
        assert_eq!(GBUFFER_FORMATS.len(), 5);
        assert_eq!(GBUFFER_FORMATS[0], wgpu::TextureFormat::Rgba8UnormSrgb);
        assert_eq!(GBUFFER_FORMATS[3], wgpu::TextureFormat::Rg16Float);
    }

    #[test]
    fn motion_shader_reprojects_unjittered_current_and_previous_positions() {
        let source = include_str!("../../assets/shaders/gbuffer.wgsl");
        assert!(source.contains("globals.unjittered_view_proj"));
        assert!(source.contains("globals.previous_view_proj"));
        assert!(source.contains("out.motion = select"));
        assert!(source.contains("(previous_uv.y - current_uv.y)"));
    }

    #[test]
    fn renderer_keeps_legacy_pipeline_after_invalid_reload() -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!("vf-m7-reload-{}", std::process::id()));
        let shaders = root.join("shaders");
        std::fs::create_dir_all(&shaders)?;
        let chunk_path = shaders.join("chunk.wgsl");
        const CHUNK: &str = include_str!("../../assets/shaders/chunk.wgsl");
        std::fs::write(&chunk_path, CHUNK)?;
        let gpu = crate::render::Gpu::new()?;
        let textures = crate::render::BlockTextures::from_registry(
            &gpu.device,
            &gpu.queue,
            &crate::assets::dir(),
        )?;
        let bindings = crate::render::SceneBindings::new(&gpu.device, &gpu.queue, &textures);
        let mut pipeline = crate::render::ChunkPipeline::new_shared(
            &gpu.device,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            &chunk_path,
            bindings,
            false,
        )?;
        std::fs::write(&chunk_path, "invalid wgsl")?;
        assert!(!pipeline.reload_shader(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb));
        std::fs::write(&chunk_path, CHUNK)?;
        assert!(pipeline.reload_shader(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb));
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
