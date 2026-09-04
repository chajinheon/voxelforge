//! Wireframe pipeline for the currently targeted block.

use std::path::{Path, PathBuf};

use bytemuck::{Pod, Zeroable};
use glam::IVec3;

const OUTLINE_VERTEX_COUNT: u32 = 24;
const OUTLINE_SCALE: f32 = 1.002;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct OutlineVertex {
    position: [f32; 3],
}

/// GPU resources for the selected block's 12-edge wireframe.
pub struct OutlinePipeline {
    pub pipeline: wgpu::RenderPipeline,
    shader_module: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    globals_bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    shader_path: PathBuf,
}

impl OutlinePipeline {
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        globals_layout: &wgpu::BindGroupLayout,
        globals_bind_group: &wgpu::BindGroup,
        shader_path: impl AsRef<Path>,
    ) -> anyhow::Result<Self> {
        let shader_path = shader_path.as_ref().to_owned();
        let source = std::fs::read_to_string(&shader_path)
            .map_err(|error| anyhow::anyhow!("{}: {error}", shader_path.display()))?;
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("outline-pipeline-layout"),
            bind_group_layouts: &[Some(globals_layout)],
            immediate_size: 0,
        });
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("outline-vertices"),
            size: (OUTLINE_VERTEX_COUNT as usize * std::mem::size_of::<OutlineVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("outline-shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let pipeline = create_pipeline(device, &pipeline_layout, &shader_module, color_format);
        if let Some(error) = pollster::block_on(scope.pop()) {
            anyhow::bail!("outline shader validation failed: {error}");
        }
        Ok(Self {
            pipeline,
            shader_module,
            pipeline_layout,
            globals_bind_group: globals_bind_group.clone(),
            vertex_buffer,
            shader_path,
        })
    }

    pub fn shader_path(&self) -> &Path {
        &self.shader_path
    }

    /// Replace the selected block's 24 line-list vertices.
    pub fn update(&self, queue: &wgpu::Queue, block: IVec3) {
        queue.write_buffer(
            &self.vertex_buffer,
            0,
            bytemuck::cast_slice(&vertices(block)),
        );
    }

    /// Recompile the outline shader, retaining the previous pipeline on error.
    pub fn reload_shader(
        &mut self,
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
    ) -> bool {
        let source = match std::fs::read_to_string(&self.shader_path) {
            Ok(source) => source,
            Err(error) => {
                log::error!("shader reload {}: {error}", self.shader_path.display());
                return false;
            }
        };
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("outline-shader-reloaded"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let pipeline = create_pipeline(device, &self.pipeline_layout, &module, color_format);
        if let Some(error) = pollster::block_on(scope.pop()) {
            log::error!("outline shader reload validation failed: {error}");
            return false;
        }
        self.shader_module = module;
        self.pipeline = pipeline;
        log::info!("shader reloaded: {}", self.shader_path.display());
        true
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.globals_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..OUTLINE_VERTEX_COUNT, 0..1);
    }
}

fn vertices(block: IVec3) -> [OutlineVertex; OUTLINE_VERTEX_COUNT as usize] {
    let center = block.as_vec3() + glam::Vec3::splat(0.5);
    let half = glam::Vec3::splat(0.5 * OUTLINE_SCALE);
    let min = center - half;
    let max = center + half;
    let corners = [
        [min.x, min.y, min.z],
        [max.x, min.y, min.z],
        [max.x, max.y, min.z],
        [min.x, max.y, min.z],
        [min.x, min.y, max.z],
        [max.x, min.y, max.z],
        [max.x, max.y, max.z],
        [min.x, max.y, max.z],
    ];
    const EDGES: [(usize, usize); 12] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    let mut result = [OutlineVertex { position: [0.0; 3] }; OUTLINE_VERTEX_COUNT as usize];
    for (edge_index, (a, b)) in EDGES.into_iter().enumerate() {
        result[edge_index * 2] = OutlineVertex {
            position: corners[a],
        };
        result[edge_index * 2 + 1] = OutlineVertex {
            position: corners[b],
        };
    }
    result
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    color_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    const ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("outline-render-pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<OutlineVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &ATTRIBUTES,
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::LineList,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::vertices;
    use glam::IVec3;

    #[test]
    fn outline_vertices_are_expanded_about_block_center() {
        let vertices = vertices(IVec3::new(2, 3, -4));
        assert_eq!(vertices.len(), 24);
        let min = [1.999, 2.999, -4.001];
        let max = [3.001, 4.001, -2.999];
        let mut degree = [0_u8; 8];

        for edge in vertices.chunks_exact(2) {
            let mut corner_ids = [0_usize; 2];
            for (endpoint_index, endpoint) in edge.iter().enumerate() {
                for axis in 0..3 {
                    let value = endpoint.position[axis];
                    if (value - max[axis]).abs() < 1e-5 {
                        corner_ids[endpoint_index] |= 1 << axis;
                    } else {
                        assert!((value - min[axis]).abs() < 1e-5);
                    }
                }
                degree[corner_ids[endpoint_index]] += 1;
            }
            assert_eq!((corner_ids[0] ^ corner_ids[1]).count_ones(), 1);
        }
        assert_eq!(degree, [3; 8]);
    }
}
