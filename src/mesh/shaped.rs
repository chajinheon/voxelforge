//! Meshing for the concrete 1/16-block state ranges.
use glam::IVec3;

use crate::world::{
    block::{self, AIR, BlockId},
    chunk::PaddedChunk,
    coords::FACE_NORMALS,
    shape::{self, Face, Facing, ShapeKind, ShapeTemplate},
};

use crate::mesh::mesher::{ChunkMesh, corner_light, face_ao};
use crate::mesh::vertex::{pack_with_fraction, with_lowered};

/// Resolve a concrete state ID to its geometric shape. Ordinary block IDs
/// remain cubes in the legacy mesher.
pub fn shape_kind(id: BlockId) -> Option<ShapeKind> {
    let kind = shape::shape_kind(id);
    if matches!(kind, ShapeKind::Cube) {
        None
    } else {
        Some(kind)
    }
}

fn face_index(face: Face) -> usize {
    match face {
        Face::PosY => 0,
        Face::NegY => 1,
        Face::PosX => 2,
        Face::NegX => 3,
        Face::PosZ => 4,
        Face::NegZ => 5,
    }
}

fn opposite(face: Face) -> Face {
    match face {
        Face::NegX => Face::PosX,
        Face::PosX => Face::NegX,
        Face::NegY => Face::PosY,
        Face::PosY => Face::NegY,
        Face::NegZ => Face::PosZ,
        Face::PosZ => Face::NegZ,
    }
}

fn neighbor_template(padded: &PaddedChunk, bp: IVec3, face: Face) -> Option<ShapeTemplate> {
    let n = bp + FACE_NORMALS[face_index(opposite(face))];
    let id = padded.get(n.x, n.y, n.z);
    if id == AIR {
        None
    } else {
        shape_kind(id).map(shape::shape_template).or_else(|| {
            block::def(id)
                .light_blocking
                .then(|| shape::shape_template(ShapeKind::Cube))
        })
    }
}

fn boundary_index(face: Face) -> usize {
    match face {
        Face::NegZ => 0,
        Face::PosZ => 1,
        Face::NegX => 2,
        Face::PosX => 3,
        Face::NegY => 4,
        Face::PosY => 5,
    }
}

fn quad_corners(q: &shape::TemplateQuad) -> [([u32; 3], [u32; 3]); 4] {
    let a = q.min;
    let b = q.max;
    match q.face {
        Face::NegX | Face::PosX => [
            (
                [a[0] as u32, a[1] as u32, a[2] as u32],
                [b[0] as u32, b[1] as u32, b[2] as u32],
            ),
            (
                [a[0] as u32, b[1] as u32, a[2] as u32],
                [b[0] as u32, b[1] as u32, b[2] as u32],
            ),
            (
                [a[0] as u32, b[1] as u32, b[2] as u32],
                [b[0] as u32, b[1] as u32, b[2] as u32],
            ),
            (
                [a[0] as u32, a[1] as u32, b[2] as u32],
                [b[0] as u32, b[1] as u32, b[2] as u32],
            ),
        ],
        Face::NegY | Face::PosY => [
            (
                [a[0] as u32, a[1] as u32, a[2] as u32],
                [b[0] as u32, b[1] as u32, b[2] as u32],
            ),
            (
                [b[0] as u32, a[1] as u32, a[2] as u32],
                [b[0] as u32, b[1] as u32, b[2] as u32],
            ),
            (
                [b[0] as u32, a[1] as u32, b[2] as u32],
                [b[0] as u32, b[1] as u32, b[2] as u32],
            ),
            (
                [a[0] as u32, a[1] as u32, b[2] as u32],
                [b[0] as u32, b[1] as u32, b[2] as u32],
            ),
        ],
        Face::NegZ | Face::PosZ => [
            (
                [a[0] as u32, a[1] as u32, a[2] as u32],
                [b[0] as u32, b[1] as u32, b[2] as u32],
            ),
            (
                [b[0] as u32, a[1] as u32, a[2] as u32],
                [b[0] as u32, b[1] as u32, b[2] as u32],
            ),
            (
                [b[0] as u32, b[1] as u32, a[2] as u32],
                [b[0] as u32, b[1] as u32, b[2] as u32],
            ),
            (
                [a[0] as u32, b[1] as u32, a[2] as u32],
                [b[0] as u32, b[1] as u32, b[2] as u32],
            ),
        ],
    }
}

fn emit_q(
    mesh: &mut ChunkMesh,
    padded: &PaddedChunk,
    bp: IVec3,
    id: BlockId,
    q: &shape::TemplateQuad,
) {
    let face = face_index(q.face);
    let corners = quad_corners(q);
    let base = mesh.vertices.len() as u32;
    for (coords, _) in corners {
        let p = IVec3::new(
            coords[0] as i32 / 16,
            coords[1] as i32 / 16,
            coords[2] as i32 / 16,
        ) + bp;
        let fx = coords[0] % 16;
        let fy = coords[1] % 16;
        let fz = coords[2] % 16;
        let corner = p;
        let (light, sky) = corner_light(padded, corner, face, corner, id);
        let ao = face_ao(padded, corner, face, corner);
        mesh.vertices.push(with_lowered(
            pack_with_fraction(
                p.x as u32,
                p.y as u32,
                p.z as u32,
                fx,
                fy,
                fz,
                face as u32,
                ao,
                block::def(id).textures[face] as u32,
                light,
                sky,
            ),
            false,
        ));
    }
    mesh.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// Emit a shaped block, subtracting a neighboring shape's boundary coverage.
pub fn append_shaped(
    mesh: &mut ChunkMesh,
    padded: &PaddedChunk,
    bp: IVec3,
    id: BlockId,
    template: &ShapeTemplate,
) {
    for q in &template.quads {
        if let Some(neighbor) = neighbor_template(padded, bp, q.face) {
            let axis = q.face as usize / 2;
            let at_boundary = match q.face {
                Face::NegX => q.min[axis] == 0,
                Face::PosX => q.max[axis] == 16,
                Face::NegY => q.min[axis] == 0,
                Face::PosY => q.max[axis] == 16,
                Face::NegZ => q.min[axis] == 0,
                Face::PosZ => q.max[axis] == 16,
            };
            if at_boundary {
                for uncovered in
                    subtract_boundary(q, &neighbor.boundary[boundary_index(opposite(q.face))])
                {
                    emit_q(mesh, padded, bp, id, &uncovered);
                }
                continue;
            }
        } else if block::def(padded.get(
            (bp + FACE_NORMALS[face_index(q.face)]).x,
            (bp + FACE_NORMALS[face_index(q.face)]).y,
            (bp + FACE_NORMALS[face_index(q.face)]).z,
        ))
        .opaque
            && matches!(
                q.face,
                Face::NegX | Face::PosX | Face::NegY | Face::PosY | Face::NegZ | Face::PosZ
            )
        {
            continue;
        }
        emit_q(mesh, padded, bp, id, q);
    }
}

/// Subtract a neighbor's 16×16 face coverage from one template quad. The
/// result is a deterministic 2-D greedy tiling; no covered or uncovered area
/// is rounded to an entire quad.
fn subtract_boundary(q: &shape::TemplateQuad, coverage: &[u16; 16]) -> Vec<shape::TemplateQuad> {
    let (u_axis, v_axis) = match q.face {
        Face::NegX | Face::PosX => (2, 1),
        Face::NegY | Face::PosY => (0, 2),
        Face::NegZ | Face::PosZ => (0, 1),
    };
    let width = (q.max[u_axis] - q.min[u_axis]) as usize;
    let height = (q.max[v_axis] - q.min[v_axis]) as usize;
    let mut open = vec![vec![true; width]; height];
    for v in 0..height {
        for u in 0..width {
            let row = q.min[v_axis] as usize + v;
            let col = q.min[u_axis] as usize + u;
            open[v][u] = coverage[row] & (1 << col) == 0;
        }
    }
    let mut out = Vec::new();
    for v in 0..height {
        for u in 0..width {
            if !open[v][u] {
                continue;
            }
            let mut w = 1;
            while u + w < width && open[v][u + w] {
                w += 1;
            }
            let mut h = 1;
            'grow: while v + h < height {
                for x in u..u + w {
                    if !open[v + h][x] {
                        break 'grow;
                    }
                }
                h += 1;
            }
            for yy in v..v + h {
                for xx in u..u + w {
                    open[yy][xx] = false;
                }
            }
            let mut min = q.min;
            let mut max = q.max;
            min[u_axis] = q.min[u_axis] + u as u8;
            max[u_axis] = min[u_axis] + w as u8;
            min[v_axis] = q.min[v_axis] + v as u8;
            max[v_axis] = min[v_axis] + h as u8;
            out.push(shape::TemplateQuad {
                face: q.face,
                min,
                max,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shaped_slab_emits_fractional_vertices() {
        let mut p = PaddedChunk::new();
        p.set(0, 0, 0, 256);
        let mut m = ChunkMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
        append_shaped(
            &mut m,
            &p,
            IVec3::ZERO,
            256,
            &shape::shape_template(ShapeKind::Slab { top: false }),
        );
        assert!(!m.vertices.is_empty());
        assert!(
            m.vertices
                .iter()
                .any(|v| crate::mesh::vertex::unpack_fraction(*v).1 != 0)
        );
    }

    #[test]
    fn state_ids_select_expected_templates() {
        assert_eq!(shape_kind(256), Some(ShapeKind::Slab { top: false }));
        assert_eq!(shape_kind(257), Some(ShapeKind::Slab { top: true }));
        assert_eq!(
            shape_kind(512),
            Some(ShapeKind::Stair {
                upside_down: false,
                facing: Facing::North
            })
        );
        assert_eq!(
            shape_kind(516),
            Some(ShapeKind::Stair {
                upside_down: true,
                facing: Facing::North
            })
        );
        assert_eq!(
            shape_kind(128),
            Some(ShapeKind::Log {
                axis: shape::Axis::X
            })
        );
        assert_eq!(
            shape_kind(129),
            Some(ShapeKind::Log {
                axis: shape::Axis::Z
            })
        );
    }

    #[test]
    fn partial_neighbor_does_not_remove_uncovered_slab_face() {
        let mut p = PaddedChunk::new();
        p.set(0, 0, 0, 256);
        p.set(1, 0, 0, 256);
        let mut m = ChunkMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
        append_shaped(
            &mut m,
            &p,
            IVec3::ZERO,
            256,
            &shape::shape_template(ShapeKind::Slab { top: false }),
        );
        assert!(!m.indices.is_empty());
    }
}
