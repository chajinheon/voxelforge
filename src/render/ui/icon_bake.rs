use super::catalog::ITEM_ICON_LAYERS;
use crate::world::{
    block, catalog,
    raycast::shape_kind,
    shape::{self, Face},
};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec3};
use std::ops::Range;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BakeVertex {
    position: [f32; 3],
    normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BakeParams {
    model: [[f32; 4]; 4],
    color: [f32; 4],
    emission: [f32; 4],
}

struct MeshData {
    vertices: Vec<BakeVertex>,
    ranges: Vec<Range<u32>>,
    colors: Vec<[f32; 4]>,
    emissions: Vec<[f32; 4]>,
    models: Vec<[[f32; 4]; 4]>,
}

pub(super) struct IconBake {
    atlas: wgpu::Texture,
    atlas_view: wgpu::TextureView,
    native_view: Option<wgpu::TextureView>,
    depth_view: Option<wgpu::TextureView>,
    vertex: wgpu::Buffer,
    params: wgpu::Buffer,
    bake_bind: wgpu::BindGroup,
    reduce_bind: Option<wgpu::BindGroup>,
    bake_pipeline: wgpu::RenderPipeline,
    reduce_pipeline: wgpu::RenderPipeline,
    ranges: Vec<Range<u32>>,
    uniform_stride: u32,
    pending: bool,
    bake_wall_ms: u64,
}
impl IconBake {
    pub(super) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vf/m10/item-icons-122x64"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: ITEM_ICON_LAYERS as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_view = atlas.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            array_layer_count: Some(ITEM_ICON_LAYERS as u32),
            ..Default::default()
        });
        let native = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vf/m10/item-icon-bake-128-rgba16f"),
            size: wgpu::Extent3d {
                width: 128,
                height: 128,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vf/m10/item-icon-bake-depth32"),
            size: wgpu::Extent3d {
                width: 128,
                height: 128,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let native_view = native.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        let MeshData {
            vertices,
            ranges,
            colors,
            emissions,
            models,
        } = mesh_data();
        let vertex = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vf/m10/item-icon-bake/vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let align = device.limits().min_uniform_buffer_offset_alignment.max(256);
        let params_size = align as usize * ITEM_ICON_LAYERS;
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vf/m10/item-icon-bake/params"),
            size: params_size as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        for (index, ((color, emission), model)) in
            colors.into_iter().zip(emissions).zip(models).enumerate()
        {
            queue.write_buffer(
                &params,
                index as u64 * align as u64,
                bytemuck::bytes_of(&BakeParams {
                    model,
                    color: [color[0], color[1], color[2], color[3]],
                    emission,
                }),
            );
        }
        let bake_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m10/item-icon-bake/shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/item_icon_bake.wgsl").into(),
            ),
        });
        let bake_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vf/m10/item-icon-bake/layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<BakeParams>() as u64
                    ),
                },
                count: None,
            }],
        });
        let bake_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m10/item-icon-bake/bind"),
            layout: &bake_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &params,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<BakeParams>() as u64),
                }),
            }],
        });
        let bake_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vf/m10/item-icon-bake/pipeline-layout"),
            bind_group_layouts: &[Some(&bake_layout)],
            immediate_size: 0,
        });
        let bake_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vf/m10/item-icon-bake/pipeline"),
            layout: Some(&bake_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &bake_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<BakeVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &bake_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let reduce_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m10/item-icon-reduce/shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/item_icon_reduce.wgsl").into(),
            ),
        });
        let reduce_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vf/m10/item-icon-reduce/layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let reduce_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m10/item-icon-reduce/bind"),
            layout: &reduce_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&native_view),
            }],
        });
        let reduce_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("vf/m10/item-icon-reduce/pipeline-layout"),
                bind_group_layouts: &[Some(&reduce_layout)],
                immediate_size: 0,
            });
        let reduce_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vf/m10/item-icon-reduce/pipeline"),
            layout: Some(&reduce_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &reduce_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &reduce_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            atlas,
            atlas_view,
            native_view: Some(native_view),
            depth_view: Some(depth_view),
            vertex,
            params,
            bake_bind,
            reduce_bind: Some(reduce_bind),
            bake_pipeline,
            reduce_pipeline,
            ranges,
            uniform_stride: align,
            pending: true,
            bake_wall_ms: 0,
        }
    }

    pub(super) fn atlas_view(&self) -> &wgpu::TextureView {
        &self.atlas_view
    }
    pub(super) fn memory_bytes(&self) -> usize {
        ITEM_ICON_LAYERS * 64 * 64 * 4
            + self.native_view.as_ref().map_or(0, |_| 128 * 128 * 8)
            + self.depth_view.as_ref().map_or(0, |_| 128 * 128 * 4)
            + self.vertex.size() as usize
            + self.params.size() as usize
    }
    pub(super) fn bake_ms(&self) -> u64 {
        self.bake_wall_ms
    }
    pub(super) fn encode(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if !self.pending {
            return;
        }
        let started = std::time::Instant::now();
        {
            let (Some(native_view), Some(depth_view), Some(reduce_bind)) = (
                self.native_view.as_ref(),
                self.depth_view.as_ref(),
                self.reduce_bind.as_ref(),
            ) else {
                self.pending = false;
                return;
            };
            for (layer, range) in self.ranges.iter().enumerate() {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("vf/m10/item-icon-bake/native"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: native_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
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
                pass.set_pipeline(&self.bake_pipeline);
                pass.set_bind_group(0, &self.bake_bind, &[layer as u32 * self.uniform_stride]);
                pass.set_vertex_buffer(0, self.vertex.slice(..));
                pass.draw(range.clone(), 0..1);
                drop(pass);
                let view = self.atlas.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: layer as u32,
                    array_layer_count: Some(1),
                    ..Default::default()
                });
                let mut reduce = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("vf/m10/item-icon-reduce/layer"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                reduce.set_pipeline(&self.reduce_pipeline);
                reduce.set_bind_group(0, reduce_bind, &[]);
                reduce.draw(0..3, 0..1);
            }
        }
        self.native_view = None;
        self.depth_view = None;
        self.reduce_bind = None;
        self.pending = false;
        self.bake_wall_ms = started.elapsed().as_millis() as u64;
    }
}

#[allow(deprecated)]
fn mesh_data() -> MeshData {
    let mut vertices = Vec::new();
    let mut ranges = Vec::with_capacity(ITEM_ICON_LAYERS);
    let mut colors = Vec::with_capacity(ITEM_ICON_LAYERS);
    let mut emissions = Vec::with_capacity(ITEM_ICON_LAYERS);
    let mut models = Vec::with_capacity(ITEM_ICON_LAYERS);
    const ICON_YAW_DEGREES: f32 = -45.0;
    const ICON_PITCH_DEGREES: f32 = 30.0;
    const ICON_SCALE: f32 = 1.25;
    let view = Mat4::look_at_rh(Vec3::new(-2.8, 2.0, 2.8), Vec3::ZERO, Vec3::Y);
    let projection = Mat4::orthographic_rh(-1.1, 1.1, -1.1, 1.1, 0.1, 10.0);
    for id in 1..=ITEM_ICON_LAYERS as u16 {
        let block_id = catalog::item(id).map_or(block::STONE, |item| item.icon_block);
        let kind = shape_kind(block_id);
        let start = vertices.len() as u32;
        for quad in shape::shape_template(kind).quads {
            let [a, b, c, d] = quad_points(&quad);
            let n = normal(quad.face);
            vertices.extend([a, b, c, a, c, d].into_iter().map(|position| BakeVertex {
                position,
                normal: n,
            }));
        }
        ranges.push(start..vertices.len() as u32);
        colors.push(color(id, block::def(block_id).translucent));
        let emission = block::def(block_id).emission_rgb;
        emissions.push([emission[0], emission[1], emission[2], 0.0]);
        let model = projection
            * view
            * Mat4::from_scale_rotation_translation(
                Vec3::splat(ICON_SCALE),
                Quat::from_euler(
                    glam::EulerRot::XYZ,
                    ICON_PITCH_DEGREES.to_radians(),
                    ICON_YAW_DEGREES.to_radians(),
                    0.0,
                ),
                Vec3::ZERO,
            );
        models.push(model.to_cols_array_2d());
    }
    MeshData {
        vertices,
        ranges,
        colors,
        emissions,
        models,
    }
}

fn quad_points(quad: &shape::TemplateQuad) -> [[f32; 3]; 4] {
    let a = quad.min.map(|v| v as f32 / 16.0 - 0.5);
    let b = quad.max.map(|v| v as f32 / 16.0 - 0.5);
    match quad.face {
        Face::NegX => [
            [a[0], a[1], a[2]],
            [a[0], b[1], a[2]],
            [a[0], b[1], b[2]],
            [a[0], a[1], b[2]],
        ],
        Face::PosX => [
            [b[0], a[1], a[2]],
            [b[0], a[1], b[2]],
            [b[0], b[1], b[2]],
            [b[0], b[1], a[2]],
        ],
        Face::NegY => [
            [a[0], a[1], a[2]],
            [b[0], a[1], a[2]],
            [b[0], a[1], b[2]],
            [a[0], a[1], b[2]],
        ],
        Face::PosY => [
            [a[0], b[1], a[2]],
            [a[0], b[1], b[2]],
            [b[0], b[1], b[2]],
            [b[0], b[1], a[2]],
        ],
        Face::NegZ => [
            [a[0], a[1], a[2]],
            [b[0], a[1], a[2]],
            [b[0], b[1], a[2]],
            [a[0], b[1], a[2]],
        ],
        Face::PosZ => [
            [b[0], a[1], b[2]],
            [a[0], a[1], b[2]],
            [a[0], b[1], b[2]],
            [b[0], b[1], b[2]],
        ],
    }
}
fn normal(face: Face) -> [f32; 3] {
    match face {
        Face::NegX => [-1.0, 0.0, 0.0],
        Face::PosX => [1.0, 0.0, 0.0],
        Face::NegY => [0.0, -1.0, 0.0],
        Face::PosY => [0.0, 1.0, 0.0],
        Face::NegZ => [0.0, 0.0, -1.0],
        Face::PosZ => [0.0, 0.0, 1.0],
    }
}
fn color(id: u16, translucent: bool) -> [f32; 4] {
    let block_id = catalog::item(id).map_or(block::STONE, |item| item.icon_block);
    let def = block::def(block_id);
    let recipe = crate::world::material::recipe(def.name);
    let base = recipe.map_or([128, 128, 128], |value| value.base_srgb);
    [
        srgb_to_linear(base[0]),
        srgb_to_linear(base[1]),
        srgb_to_linear(base[2]),
        if translucent { 0.74 } else { 1.0 },
    ]
}

fn srgb_to_linear(value: u8) -> f32 {
    let value = f32::from(value) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
