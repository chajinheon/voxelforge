//! Deterministic coarse-grid meshing, including transition skirts.

use glam::IVec3;

use crate::{
    lod::grid::{GRID_SIZE, LodGrid},
    world::block::{AIR, BlockId},
    world::chunk::{block_light, sky_light},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LodVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub block: BlockId,
    pub block_light: u8,
    pub sky_light: u8,
    pub skirt: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LodMesh {
    pub vertices: Vec<LodVertex>,
    pub indices: Vec<u32>,
}

/// Build a coarse visible-surface mesh in world coordinates.
#[allow(clippy::needless_range_loop)]
pub fn mesh_lod_grid(grid: &LodGrid) -> LodMesh {
    mesh_lod_grid_with_neighbors(grid, |_| AIR)
}

/// Build a grid while consulting adjacent cached grids for border occupancy.
/// The callback receives a local coordinate that may be one cell outside the
/// 32^3 grid; returning AIR keeps the standalone/offline behavior unchanged.
#[allow(clippy::needless_range_loop)]
pub fn mesh_lod_grid_with_neighbors<F>(grid: &LodGrid, sample_neighbor: F) -> LodMesh
where
    F: Fn(IVec3) -> BlockId,
{
    let mut mesh = LodMesh::default();
    let scale = grid.key.cell_size() as f32;
    for face in 0..6 {
        for layer in 0..GRID_SIZE {
            let mut mask = [[None; GRID_SIZE]; GRID_SIZE];
            for v in 0..GRID_SIZE {
                for u in 0..GRID_SIZE {
                    let local = face_cell(face, layer, u, v);
                    let block = grid.get(local.as_uvec3());
                    let neighbor_pos = local + FACE_NORMALS[face];
                    let neighbor_block = if inside(neighbor_pos) {
                        grid.get(neighbor_pos.as_uvec3())
                    } else {
                        sample_neighbor(neighbor_pos)
                    };
                    if block != AIR && neighbor_block == AIR {
                        mask[v][u] = Some((block, grid.get_light(local.as_uvec3())));
                    }
                }
            }
            let mut v = 0;
            while v < GRID_SIZE {
                let mut u = 0;
                while u < GRID_SIZE {
                    let Some(key) = mask[v][u] else {
                        u += 1;
                        continue;
                    };
                    let mut width = 1;
                    while u + width < GRID_SIZE && mask[v][u + width] == Some(key) {
                        width += 1;
                    }
                    let mut height = 1;
                    'grow: while v + height < GRID_SIZE {
                        for item in &mask[v + height][u..u + width] {
                            if *item != Some(key) {
                                break 'grow;
                            }
                        }
                        height += 1;
                    }
                    for row in &mut mask[v..v + height] {
                        row[u..u + width].fill(None);
                    }
                    let local = face_cell(face, layer, u, v);
                    let base = grid.key.world_origin() + local * grid.key.cell_size();
                    append_face(
                        &mut mesh,
                        face_rect_vertices(base, scale, face, width, height),
                        FACE_NORMALS[face],
                        key.0,
                        key.1,
                        false,
                    );
                    u += width;
                }
                v += 1;
            }
        }
    }
    append_transition_skirt(grid, &mut mesh);
    mesh
}

pub fn mesh_coarse_grid(grid: &LodGrid) -> LodMesh {
    mesh_lod_grid(grid)
}

const FACE_BASIS: [(usize, usize, usize); 6] = [
    (0, 1, 2),
    (0, 2, 1),
    (1, 2, 0),
    (1, 0, 2),
    (2, 0, 1),
    (2, 1, 0),
];

fn face_cell(face: usize, layer: usize, u: usize, v: usize) -> IVec3 {
    let (normal_axis, u_axis, v_axis) = FACE_BASIS[face];
    let mut cell = IVec3::ZERO;
    cell[normal_axis] = layer as i32;
    cell[u_axis] = u as i32;
    cell[v_axis] = v as i32;
    cell
}

fn face_rect_vertices(
    base: IVec3,
    scale: f32,
    face: usize,
    width: usize,
    height: usize,
) -> [[f32; 3]; 4] {
    let (normal_axis, u_axis, v_axis) = FACE_BASIS[face];
    let offset = if FACE_NORMALS[face][normal_axis] > 0 {
        FACE_NORMALS[face] * scale as i32
    } else {
        IVec3::ZERO
    };
    let plane = base + offset;
    let mut u = IVec3::ZERO;
    let mut v = IVec3::ZERO;
    u[u_axis] = (width as f32 * scale) as i32;
    v[v_axis] = (height as f32 * scale) as i32;
    let origin = plane.as_vec3();
    let a = origin.to_array();
    let b = (plane + u).as_vec3().to_array();
    let c = (plane + u + v).as_vec3().to_array();
    let d = (plane + v).as_vec3().to_array();
    [a, b, c, d]
}

/// Add a two-coarse-cell-deep skirt around the four horizontal grid borders.
/// The skirt repeats the contour block and light, so it does not introduce a
/// new material or a seam-visible lighting discontinuity.
pub fn append_transition_skirt(grid: &LodGrid, mesh: &mut LodMesh) {
    let scale = grid.key.cell_size() as f32;
    let drop = scale * 2.0;
    let last = (GRID_SIZE - 1) as i32;
    for y in 0..GRID_SIZE {
        for &(x, z, face) in &[
            (0, y as i32, 1usize),
            (last, y as i32, 0usize),
            (y as i32, 0, 5usize),
            (y as i32, last, 4usize),
        ] {
            let local = IVec3::new(x, y as i32, z);
            let block = grid.get(local.as_uvec3());
            if block == AIR
                || (inside(local + FACE_NORMALS[face])
                    && grid.get((local + FACE_NORMALS[face]).as_uvec3()) != AIR)
            {
                continue;
            }
            let origin = grid.key.world_origin() + local * grid.key.cell_size();
            // Use the border's outward vertical face.  The previous
            // implementation used the top face for every edge, creating a
            // horizontal cap instead of a two-cell transition skirt.
            let top = face_vertices(origin, scale, face);
            let lower = top.map(|p| [p[0], p[1] - drop, p[2]]);
            let light = grid.get_light(local.as_uvec3());
            let normal = FACE_NORMALS[face];
            append_face_pair(mesh, top, lower, normal, block, light);
        }
    }
}

fn append_face_pair(
    mesh: &mut LodMesh,
    top: [[f32; 3]; 4],
    bottom: [[f32; 3]; 4],
    normal: IVec3,
    block: BlockId,
    light: u8,
) {
    let start = mesh.vertices.len() as u32;
    for position in top.into_iter().chain(bottom) {
        mesh.vertices.push(LodVertex {
            position,
            normal: [normal.x as f32, normal.y as f32, normal.z as f32],
            block,
            block_light: block_light(light),
            sky_light: sky_light(light),
            skirt: true,
        });
    }
    mesh.indices.extend_from_slice(&[
        start,
        start + 1,
        start + 5,
        start,
        start + 5,
        start + 4,
        start + 1,
        start + 2,
        start + 6,
        start + 1,
        start + 6,
        start + 5,
        start + 2,
        start + 3,
        start + 7,
        start + 2,
        start + 7,
        start + 6,
        start + 3,
        start,
        start + 4,
        start + 3,
        start + 4,
        start + 7,
    ]);
}

fn append_face(
    mesh: &mut LodMesh,
    corners: [[f32; 3]; 4],
    normal: IVec3,
    block: BlockId,
    light: u8,
    skirt: bool,
) {
    let start = mesh.vertices.len() as u32;
    for position in corners {
        mesh.vertices.push(LodVertex {
            position,
            normal: [normal.x as f32, normal.y as f32, normal.z as f32],
            block,
            block_light: block_light(light),
            sky_light: sky_light(light),
            skirt,
        });
    }
    mesh.indices
        .extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
}

const FACE_NORMALS: [IVec3; 6] = [
    IVec3::X,
    IVec3::NEG_X,
    IVec3::Y,
    IVec3::NEG_Y,
    IVec3::Z,
    IVec3::NEG_Z,
];

fn inside(p: IVec3) -> bool {
    (0..GRID_SIZE as i32).contains(&p.x)
        && (0..GRID_SIZE as i32).contains(&p.y)
        && (0..GRID_SIZE as i32).contains(&p.z)
}

fn face_vertices(base: IVec3, scale: f32, face: usize) -> [[f32; 3]; 4] {
    let p = [base.x as f32, base.y as f32, base.z as f32];
    let s = scale;
    match face {
        0 => [
            [p[0] + s, p[1], p[2]],
            [p[0] + s, p[1] + s, p[2]],
            [p[0] + s, p[1] + s, p[2] + s],
            [p[0] + s, p[1], p[2] + s],
        ],
        1 => [
            p,
            [p[0], p[1], p[2] + s],
            [p[0], p[1] + s, p[2] + s],
            [p[0], p[1] + s, p[2]],
        ],
        2 => [
            [p[0], p[1] + s, p[2]],
            [p[0], p[1] + s, p[2] + s],
            [p[0] + s, p[1] + s, p[2] + s],
            [p[0] + s, p[1] + s, p[2]],
        ],
        3 => [
            p,
            [p[0] + s, p[1], p[2]],
            [p[0] + s, p[1], p[2] + s],
            [p[0], p[1], p[2] + s],
        ],
        4 => [
            [p[0], p[1], p[2] + s],
            [p[0] + s, p[1], p[2] + s],
            [p[0] + s, p[1] + s, p[2] + s],
            [p[0], p[1] + s, p[2] + s],
        ],
        _ => [
            p,
            [p[0], p[1] + s, p[2]],
            [p[0] + s, p[1] + s, p[2]],
            [p[0] + s, p[1], p[2]],
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lod::grid::{LodGrid, LodKey};

    #[test]
    fn lod_mesh_uses_cell_scale() {
        let mut grid = LodGrid::empty(LodKey::new(2, IVec3::new(1, 0, 0)));
        grid.blocks[0] = 1;
        let mesh = mesh_lod_grid(&grid);
        assert!(mesh.vertices.iter().any(|v| v.position[0] == 128.0));
        assert!(mesh.vertices.iter().all(|v| v.position[0] >= 128.0));
    }

    #[test]
    fn lod_mesh_contains_transition_skirt() {
        let mut grid = LodGrid::empty(LodKey::new(1, IVec3::ZERO));
        grid.blocks[0] = 1;
        let mesh = mesh_lod_grid(&grid);
        assert!(mesh.vertices.iter().any(|v| v.skirt));
        assert!(mesh.vertices.iter().any(|v| v.position[1] == -4.0));
    }
}
