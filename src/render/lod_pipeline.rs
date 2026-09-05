//! Dedicated distant-terrain pipeline. LOD meshes use world-space positions,
//! so they cannot be packed into the six-bit full-detail chunk vertex format.

use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Vec3Swizzles};

use crate::mesh::lod::LodMesh;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuLodVertex {
    position: [f32; 3],
    block: u32,
    light: f32,
    level: u32,
}

pub struct GpuLodMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub key: crate::lod::LodKey,
    pub origin: IVec3,
}

impl GpuLodMesh {
    pub fn buffer_memory_bytes(&self) -> usize {
        self.vertex_buffer
            .size()
            .saturating_add(self.index_buffer.size()) as usize
    }
}

pub struct LodPipeline {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::BindGroup,
}

impl LodPipeline {
    pub fn new(
        device: &wgpu::Device,
        globals_layout: &wgpu::BindGroupLayout,
        globals: &wgpu::BindGroup,
        format: wgpu::TextureFormat,
    ) -> anyhow::Result<Self> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m7/lod/shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../assets/shaders/lod.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vf/m7/lod/layout"),
            bind_group_layouts: &[Some(globals_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vf/m7/lod/pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuLodVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Uint32, 2 => Float32, 3 => Uint32],
                })],
            },
            primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Ok(Self {
            pipeline,
            globals: globals.clone(),
        })
    }

    pub fn upload(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: crate::lod::LodKey,
        mesh: &LodMesh,
    ) -> GpuLodMesh {
        let vertices: Vec<_> = mesh
            .vertices
            .iter()
            .map(|vertex| GpuLodVertex {
                position: vertex.position,
                block: vertex.block as u32,
                light: f32::from(vertex.sky_light.max(vertex.block_light)) / 15.0,
                level: key.level as u32,
            })
            .collect();
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vf/m7/lod/vertices"),
            size: (vertices.len().max(1) * std::mem::size_of::<GpuLodVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !vertices.is_empty() {
            queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        }
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vf/m7/lod/indices"),
            size: (mesh.indices.len().max(1) * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !mesh.indices.is_empty() {
            queue.write_buffer(&index_buffer, 0, bytemuck::cast_slice(&mesh.indices));
        }
        GpuLodMesh {
            vertex_buffer,
            index_buffer,
            index_count: mesh.indices.len() as u32,
            key,
            origin: key.world_origin(),
        }
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, meshes: &'a [GpuLodMesh]) -> usize {
        self.draw_selected(pass, meshes, glam::Vec3::ZERO)
    }

    pub fn draw_selected<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        meshes: &'a [GpuLodMesh],
        camera: glam::Vec3,
    ) -> usize {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.globals, &[]);
        let mut drawn = 0;
        for mesh in meshes {
            let center =
                mesh.origin.as_vec3() + glam::Vec3::splat(16.0 * mesh.key.cell_size() as f32);
            let distance = (center.xz() - camera.xz()).length() as i32;
            let target =
                crate::lod::lod_for_distance(distance, crate::lod::DEFAULT_FULL_DETAIL_RADIUS);
            let ranges = crate::lod::lod_ring_ranges_default();
            let in_overlap = mesh.key.level > 0
                && distance >= ranges[mesh.key.level as usize].start
                && distance <= ranges[mesh.key.level as usize].end
                && target != mesh.key.level;
            if target != mesh.key.level && !in_overlap {
                continue;
            }
            if mesh.index_count == 0 {
                continue;
            }
            pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            drawn += 1;
        }
        drawn
    }
}
