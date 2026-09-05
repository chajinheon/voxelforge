//! Shape-aware first-person geometry.
//!
//! The world owns the canonical 1/16 occupancy templates.  The viewmodel keeps
//! a compact ShapeTemplate cache. Live rendering consumes the sibling GPU mesh
//! cache; the CPU projection helpers remain for deterministic unit coverage.
#![allow(dead_code)]

use super::UiVertex;
use crate::ui::{HandTransform, ViewModelState};
use crate::world::block;
use crate::world::catalog;
use crate::world::raycast::shape_kind;
use crate::world::shape::{self, Face, ShapeKind};
#[path = "viewmodel_gpu.rs"]
mod viewmodel_gpu;
pub(super) use viewmodel_gpu::GpuViewModelMeshes;
pub(super) use viewmodel_gpu::{GpuParams, GpuVertex};

#[derive(Clone, Copy)]
struct CachedTriangle {
    points: [[f32; 3]; 3],
    face: Face,
}

#[derive(Clone)]
struct ViewMesh {
    triangles: Vec<CachedTriangle>,
    scale: f32,
    translucent: bool,
}

/// All catalog item shapes, built once with the canonical world templates.
pub(super) struct ViewModelMeshCache {
    meshes: Vec<ViewMesh>,
    hand: ViewMesh,
}

impl ViewModelMeshCache {
    pub(super) fn new() -> Self {
        let meshes = (0..=122).map(|id| mesh_for_item(id as u16)).collect();
        Self {
            meshes,
            hand: hand_mesh(),
        }
    }

    pub(super) fn memory_bytes(&self) -> usize {
        self.meshes
            .iter()
            .map(|mesh| mesh.triangles.len() * std::mem::size_of::<CachedTriangle>())
            .sum::<usize>()
            + self.hand.triangles.len() * std::mem::size_of::<CachedTriangle>()
    }

    pub(super) fn draw(
        &self,
        vertices: &mut Vec<UiVertex>,
        output: (u32, u32),
        state: &ViewModelState,
    ) {
        let transform = state.transform();
        let delta_translation = sub3(transform.translation, HandTransform::BASE.translation);
        let delta_rotation = sub3(
            transform.rotation_degrees,
            HandTransform::BASE.rotation_degrees,
        );
        let mut projected = Vec::new();

        let mut arm_origin = add3(HandTransform::BASE.translation, delta_translation);
        // The pivot is camera-space top-center; lift the presentation so the
        // complete 0.72-unit arm remains inside the native HUD crop.
        arm_origin[1] += 0.24;
        project_mesh(
            &mut projected,
            &self.hand,
            Projection {
                origin: arm_origin,
                rotation: transform.rotation_degrees,
                scale: 0.35,
                base_color: [0.70, 0.50, 0.40, 0.95],
                output,
                hand: true,
            },
        );

        let item_id = state.displayed_item();
        let mesh = self
            .meshes
            .get(item_id as usize)
            .or_else(|| self.meshes.first());
        let Some(mesh) = mesh else {
            return;
        };
        let item_origin = [
            0.38 + delta_translation[0],
            -0.34 + delta_translation[1],
            -0.66 + delta_translation[2],
        ];
        let item_rotation = add3([18.0, -38.0, 8.0], delta_rotation);
        project_mesh(
            &mut projected,
            mesh,
            Projection {
                origin: item_origin,
                rotation: item_rotation,
                // The contract scale is stored per shape; this compact camera
                // projection keeps the native-LDR hand crop usable at 68°.
                scale: mesh.scale * 0.22,
                base_color: item_color(item_id, mesh.translucent),
                output,
                hand: false,
            },
        );

        projected.sort_by(|a, b| a.depth.total_cmp(&b.depth));
        for triangle in projected {
            let vertex = |index: usize, point: [f32; 2]| UiVertex {
                position: [
                    point[0] / output.0 as f32 * 2.0 - 1.0,
                    1.0 - point[1] / output.1 as f32 * 2.0,
                ],
                color: triangle.color,
                icon_uv: triangle.uvs[index],
                icon_layer: if triangle.hand { -2.0 } else { -1.0 },
            };
            vertices.extend([
                vertex(0, triangle.points[0]),
                vertex(1, triangle.points[1]),
                vertex(2, triangle.points[2]),
            ]);
        }
    }
}

struct ProjectedTriangle {
    points: [[f32; 2]; 3],
    uvs: [[f32; 2]; 3],
    depth: f32,
    color: [f32; 4],
    hand: bool,
}

struct Projection {
    origin: [f32; 3],
    rotation: [f32; 3],
    scale: f32,
    base_color: [f32; 4],
    output: (u32, u32),
    hand: bool,
}

fn project_mesh(output_mesh: &mut Vec<ProjectedTriangle>, mesh: &ViewMesh, projection: Projection) {
    let aspect = projection.output.0 as f32 / projection.output.1.max(1) as f32;
    let tangent = (34.0_f32.to_radians() * 0.5).tan();
    for triangle in &mesh.triangles {
        let mut screen = [[0.0; 2]; 3];
        let mut depth = 0.0;
        let mut uvs = [[0.0; 2]; 3];
        for (index, point) in triangle.points.into_iter().enumerate() {
            let rotated = rotate_euler(mul3(point, projection.scale), projection.rotation);
            let camera = add3(rotated, projection.origin);
            let z = camera[2].min(-0.05);
            let ndc_x = camera[0] / (-z * tangent * aspect);
            let ndc_y = camera[1] / (-z * tangent);
            screen[index] = [
                (ndc_x * 0.5 + 0.5) * projection.output.0 as f32,
                (0.5 - ndc_y * 0.5) * projection.output.1 as f32,
            ];
            if projection.hand {
                uvs[index] = [
                    (point[0] / 0.26 + 0.5).clamp(0.0, 1.0),
                    (-point[1] / 0.72).clamp(0.0, 1.0),
                ];
            }
            depth += z;
        }
        let shade = face_shade(triangle.face);
        output_mesh.push(ProjectedTriangle {
            points: screen,
            uvs,
            depth: depth / 3.0,
            color: [
                projection.base_color[0] * shade,
                projection.base_color[1] * shade,
                projection.base_color[2] * shade,
                projection.base_color[3],
            ],
            hand: projection.hand,
        });
    }
}

fn mesh_for_item(id: u16) -> ViewMesh {
    let icon_block = catalog::item(id)
        .map(|item| item.icon_block)
        .unwrap_or(block::STONE);
    let kind = shape_kind(icon_block);
    let template = shape::shape_template(kind);
    let triangles = template
        .quads
        .iter()
        .flat_map(|quad| {
            let [a, b, c, d] = quad_points(quad);
            [
                CachedTriangle {
                    points: [a, b, c],
                    face: quad.face,
                },
                CachedTriangle {
                    points: [a, c, d],
                    face: quad.face,
                },
            ]
        })
        .collect();
    let scale = match kind {
        ShapeKind::Slab { .. } | ShapeKind::Stair { .. } => 0.38,
        ShapeKind::Pane { .. } | ShapeKind::Fence { .. } => 0.46,
        _ => 0.34,
    };
    ViewMesh {
        triangles,
        scale,
        translucent: block::def(icon_block).translucent,
    }
}

fn hand_mesh() -> ViewMesh {
    let template = shape::shape_template(ShapeKind::Cube);
    let mut triangles = Vec::with_capacity(template.quads.len() * 2);
    for quad in &template.quads {
        let [a, b, c, d] = quad_points(quad);
        triangles.extend([
            CachedTriangle {
                points: [a, b, c],
                face: quad.face,
            },
            CachedTriangle {
                points: [a, c, d],
                face: quad.face,
            },
        ]);
    }
    // The local cube is stretched to the contract's (0.26, 0.72, 0.26)
    // cuboid and translated so its top center is the animation pivot.
    for triangle in &mut triangles {
        for point in &mut triangle.points {
            point[0] *= 0.26;
            point[1] = point[1] * 0.72 - 0.72;
            point[2] *= 0.26;
        }
    }
    ViewMesh {
        triangles,
        scale: 1.0,
        translucent: false,
    }
}

fn quad_points(quad: &shape::TemplateQuad) -> [[f32; 3]; 4] {
    let a = quad.min.map(|value| value as f32 / 16.0 - 0.5);
    let b = quad.max.map(|value| value as f32 / 16.0 - 0.5);
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

fn item_color(id: u16, translucent: bool) -> [f32; 4] {
    let seed = u32::from(id).wrapping_mul(37).wrapping_add(19);
    let alpha = if translucent { 0.64 } else { 1.0 };
    [
        0.30 + (seed % 120) as f32 / 255.0,
        0.34 + ((seed / 3) % 105) as f32 / 255.0,
        0.40 + ((seed / 7) % 90) as f32 / 255.0,
        alpha,
    ]
}

fn face_shade(face: Face) -> f32 {
    match face {
        Face::PosY => 1.0,
        Face::NegY => 0.48,
        Face::PosX => 0.84,
        Face::NegX => 0.72,
        Face::PosZ => 0.90,
        Face::NegZ => 0.60,
    }
}

fn rotate_euler(mut point: [f32; 3], degrees: [f32; 3]) -> [f32; 3] {
    for (axis, angle) in degrees.into_iter().enumerate() {
        let (sin, cos) = angle.to_radians().sin_cos();
        point = match axis {
            0 => [
                point[0],
                point[1] * cos - point[2] * sin,
                point[1] * sin + point[2] * cos,
            ],
            1 => [
                point[0] * cos + point[2] * sin,
                point[1],
                -point[0] * sin + point[2] * cos,
            ],
            _ => [
                point[0] * cos - point[1] * sin,
                point[0] * sin + point[1] * cos,
                point[2],
            ],
        };
    }
    point
}

fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn mul3(a: [f32; 3], scale: f32) -> [f32; 3] {
    [a[0] * scale, a[1] * scale, a[2] * scale]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_contains_distinct_cube_and_shape_geometry() {
        let cache = ViewModelMeshCache::new();
        assert!(cache.meshes[4].triangles.len() >= 12);
        assert_ne!(
            cache.meshes[101].triangles.len(),
            cache.meshes[4].triangles.len()
        );
        assert!(cache.memory_bytes() < 4 * 1024 * 1024);
    }

    #[test]
    fn shape_scales_follow_viewmodel_contract() {
        let cache = ViewModelMeshCache::new();
        assert_eq!(cache.meshes[101].scale, 0.38);
        assert_eq!(cache.meshes[113].scale, 0.46);
        assert_eq!(cache.meshes[4].scale, 0.34);
    }

    #[test]
    fn draw_uses_projected_shape_faces_instead_of_icon_layers() {
        let cache = ViewModelMeshCache::new();
        let state = ViewModelState::new(101);
        let mut vertices = Vec::new();
        cache.draw(&mut vertices, (1280, 720), &state);
        assert!(vertices.len() > 24);
        assert!(vertices.iter().all(|vertex| vertex.icon_layer < 0.0));
    }
}
