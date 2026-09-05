use super::super::catalog::ITEM_ICON_LAYERS;
use super::{hand_mesh, quad_points};
use crate::render::ui::ViewModelLighting;
use crate::ui::{HandTransform, ViewModelState};
use crate::world::raycast::shape_kind;
use crate::world::shape::{Face, ShapeKind};
use crate::world::{block, catalog, shape};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub layer: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuParams {
    pub model_view_projection: [[f32; 4]; 4],
    pub normal_matrix: [[f32; 4]; 4],
    pub color: [f32; 4],
    pub material: [f32; 4],
    pub lighting: [f32; 4],
    pub sky_color: [f32; 4],
    pub sun_dir: [f32; 4],
    pub emission: [f32; 4],
}

/// Persistent GPU mesh cache used by the native-depth viewmodel pass.
pub(crate) struct GpuViewModelMeshes {
    pub(super) vertex_buffer: wgpu::Buffer,
    pub(super) params_buffer: wgpu::Buffer,
    pub(super) params_bind_group: wgpu::BindGroup,
    pub(super) item_ranges: Vec<std::ops::Range<u32>>,
    pub(super) hand_range: std::ops::Range<u32>,
    pub(super) uniform_stride: u32,
}

impl GpuViewModelMeshes {
    pub(crate) fn new(device: &wgpu::Device, params_layout: &wgpu::BindGroupLayout) -> Self {
        let (vertices, item_ranges, hand_range) = gpu_mesh_data();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vf/m10/viewmodel/gpu-meshes"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let stride = device.limits().min_uniform_buffer_offset_alignment.max(256);
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vf/m10/viewmodel/params"),
            size: u64::from(stride) * 2,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m10/viewmodel/params-bind"),
            layout: params_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            }],
        });
        Self {
            vertex_buffer,
            params_buffer,
            params_bind_group,
            item_ranges,
            hand_range,
            uniform_stride: stride,
        }
    }

    pub(crate) fn memory_bytes(&self) -> usize {
        self.vertex_buffer.size() as usize + self.params_buffer.size() as usize
    }

    pub(crate) fn update(
        &self,
        queue: &wgpu::Queue,
        state: &ViewModelState,
        aspect: f32,
        view_lighting: ViewModelLighting,
    ) {
        let transform = state.transform();
        #[allow(deprecated)]
        let projection =
            glam::Mat4::perspective_rh(68.0_f32.to_radians(), aspect.max(0.1), 0.01, 10.0);
        let hand_model = model_matrix(
            transform.translation,
            transform.rotation_degrees,
            glam::Vec3::ONE,
        );
        let item_translation = [
            0.38 + transform.translation[0] - HandTransform::BASE.translation[0],
            -0.34 + transform.translation[1] - HandTransform::BASE.translation[1],
            -0.66 + transform.translation[2] - HandTransform::BASE.translation[2],
        ];
        let item_rotation = [
            18.0 + transform.rotation_degrees[0] - HandTransform::BASE.rotation_degrees[0],
            -38.0 + transform.rotation_degrees[1] - HandTransform::BASE.rotation_degrees[1],
            8.0 + transform.rotation_degrees[2] - HandTransform::BASE.rotation_degrees[2],
        ];
        let item_model = model_matrix(
            item_translation,
            item_rotation,
            glam::Vec3::splat(item_scale(state.displayed_item())),
        );
        let curves = view_lighting.curves();
        let lighting = [curves[0], curves[1], view_lighting.sun_factor, 0.0];
        let sky_color = [
            view_lighting.sky_color[0],
            view_lighting.sky_color[1],
            view_lighting.sky_color[2],
            0.0,
        ];
        let sun_dir = [
            view_lighting.sun_dir[0],
            view_lighting.sun_dir[1],
            view_lighting.sun_dir[2],
            0.0,
        ];
        let hand = GpuParams {
            model_view_projection: (projection * hand_model).to_cols_array_2d(),
            normal_matrix: hand_model.inverse().transpose().to_cols_array_2d(),
            color: [1.0; 4],
            material: [0.72, 0.0, 0.95, 0.0],
            lighting,
            sky_color,
            sun_dir,
            emission: [0.0; 4],
        };
        let (material, lighting, emission) = material_params(state.displayed_item());
        let item = GpuParams {
            model_view_projection: (projection * item_model).to_cols_array_2d(),
            normal_matrix: item_model.inverse().transpose().to_cols_array_2d(),
            color: [1.0; 4],
            material,
            lighting,
            sky_color,
            sun_dir,
            emission,
        };
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&hand));
        queue.write_buffer(
            &self.params_buffer,
            u64::from(self.uniform_stride),
            bytemuck::bytes_of(&item),
        );
    }

    pub(crate) fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        pipeline: &'a wgpu::RenderPipeline,
        icon_bind_group: &'a wgpu::BindGroup,
        hand_bind_group: &'a wgpu::BindGroup,
        item: u16,
    ) {
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, icon_bind_group, &[]);
        pass.set_bind_group(1, hand_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_bind_group(2, &self.params_bind_group, &[0]);
        pass.draw(self.hand_range.clone(), 0..1);
        if let Some(range) = self.item_ranges.get(item.saturating_sub(1) as usize) {
            pass.set_bind_group(2, &self.params_bind_group, &[self.uniform_stride]);
            pass.draw(range.clone(), 0..1);
        }
    }
}

fn model_matrix(translation: [f32; 3], rotation: [f32; 3], scale: glam::Vec3) -> glam::Mat4 {
    glam::Mat4::from_scale_rotation_translation(
        scale,
        glam::Quat::from_euler(
            glam::EulerRot::XYZ,
            rotation[0].to_radians(),
            rotation[1].to_radians(),
            rotation[2].to_radians(),
        ),
        glam::Vec3::from_array(translation),
    )
}

fn item_scale(id: u16) -> f32 {
    let block_id = catalog::item(id).map_or(block::STONE, |item| item.icon_block);
    match shape_kind(block_id) {
        ShapeKind::Slab { .. } | ShapeKind::Stair { .. } => 0.38,
        ShapeKind::Pane { .. } | ShapeKind::Fence { .. } => 0.46,
        _ => 0.34,
    }
}

fn face_normal(face: Face) -> [f32; 3] {
    match face {
        Face::NegX => [-1.0, 0.0, 0.0],
        Face::PosX => [1.0, 0.0, 0.0],
        Face::NegY => [0.0, -1.0, 0.0],
        Face::PosY => [0.0, 1.0, 0.0],
        Face::NegZ => [0.0, 0.0, -1.0],
        Face::PosZ => [0.0, 0.0, 1.0],
    }
}

fn gpu_mesh_data() -> (
    Vec<GpuVertex>,
    Vec<std::ops::Range<u32>>,
    std::ops::Range<u32>,
) {
    let mut vertices = Vec::new();
    let mut ranges = Vec::with_capacity(ITEM_ICON_LAYERS);
    for id in 1..=ITEM_ICON_LAYERS as u16 {
        let block_id = catalog::item(id).map_or(block::STONE, |item| item.icon_block);
        let start = vertices.len() as u32;
        let color = material_color(block_id);
        for quad in shape::shape_template(shape_kind(block_id)).quads {
            let [a, b, c, d] = quad_points(&quad);
            let normal = face_normal(quad.face);
            for position in [a, b, c, a, c, d] {
                vertices.push(GpuVertex {
                    position,
                    normal,
                    uv: [0.0, 0.0],
                    color,
                    layer: id.saturating_sub(1) as f32,
                });
            }
        }
        ranges.push(start..vertices.len() as u32);
    }
    let hand_start = vertices.len() as u32;
    for triangle in hand_mesh().triangles {
        for point in triangle.points {
            vertices.push(GpuVertex {
                position: point,
                normal: face_normal(triangle.face),
                uv: [
                    (point[0] / 0.26 + 0.5).clamp(0.0, 1.0),
                    (-point[1] / 0.72).clamp(0.0, 1.0),
                ],
                color: [1.0; 4],
                layer: -2.0,
            });
        }
    }
    let hand_end = vertices.len() as u32;
    (vertices, ranges, hand_start..hand_end)
}

fn material_color(block_id: block::BlockId) -> [f32; 4] {
    let def = block::def(block_id);
    let srgb =
        crate::world::material::recipe(def.name).map_or([128, 128, 128], |recipe| recipe.base_srgb);
    [
        srgb_to_linear(srgb[0]),
        srgb_to_linear(srgb[1]),
        srgb_to_linear(srgb[2]),
        if def.translucent { 0.74 } else { 1.0 },
    ]
}

fn material_params(id: u16) -> ([f32; 4], [f32; 4], [f32; 4]) {
    let block_id = catalog::item(id).map_or(block::STONE, |item| item.icon_block);
    let def = block::def(block_id);
    let recipe = crate::world::material::recipe(def.name);
    let roughness = recipe.map_or(0.72, |value| f32::from(value.roughness) / 255.0);
    let metallic = recipe.map_or(0.0, |value| f32::from(value.metallic) / 255.0);
    let alpha = if def.translucent { 0.74 } else { 1.0 };
    (
        [roughness, metallic, alpha, 0.0],
        [0.32, 0.45, 0.25, 0.0],
        [
            def.emission_rgb[0],
            def.emission_rgb[1],
            def.emission_rgb[2],
            0.0,
        ],
    )
}

fn srgb_to_linear(value: u8) -> f32 {
    let value = f32::from(value) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
