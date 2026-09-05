use glam::IVec3;

use crate::world::{
    block::{AIR, def},
    chunk::PaddedChunk,
    coords::{FACE_NORMALS, face_corners},
};

use super::mesher::{
    ChunkMesh, ChunkMeshes, append_quad_with_lowered, corner_light, face_ao, lowered_pattern,
};
use crate::world::block::{GRASS, LEAVES};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FaceKey {
    pub tex: u16,
    pub ao: [u8; 4],
    pub block_light: [u8; 4],
    pub sky_light: [u8; 4],
    pub lowered: [bool; 4],
}

// For each face, the two axes point from corner 0 to corners 1 and 3.
const BASIS: [(usize, i32, usize, i32, usize, i32); 6] = [
    (0, 1, 1, 1, 2, 1), // +X: layer X, u=+Y, v=+Z
    (0, 1, 2, 1, 1, 1), // -X: layer X, u=+Z, v=+Y
    (1, 1, 2, 1, 0, 1), // +Y: layer Y, u=+Z, v=+X
    (1, 1, 0, 1, 2, 1), // -Y: layer Y, u=+X, v=+Z
    (2, 1, 0, 1, 1, 1), // +Z: layer Z, u=+X, v=+Y
    (2, 1, 1, 1, 0, 1), // -Z: layer Z, u=+Y, v=+X
];

fn coord(mut p: IVec3, axis: usize, value: i32) -> IVec3 {
    p[axis] = value;
    p
}

fn cell_bp(face: usize, layer: usize, u: usize, v: usize) -> IVec3 {
    let (na, ns, ua, us, va, vs) = BASIS[face];
    let mut p = IVec3::ZERO;
    p = coord(p, na, layer as i32 * ns);
    p = coord(p, ua, u as i32 * us);
    coord(p, va, v as i32 * vs)
}

fn face_cell(
    padded: &PaddedChunk,
    face: usize,
    bp: IVec3,
    translucent_pass: bool,
) -> Option<(FaceKey, bool)> {
    let id = padded.get(bp.x, bp.y, bp.z);
    let block = def(id);
    if id == AIR || block.name == "air" || block.translucent != translucent_pass {
        return None;
    }
    if translucent_pass && id == crate::world::block::WATER {
        return None;
    }
    let neighbor = bp + FACE_NORMALS[face];
    let neighbor_id = padded.get(neighbor.x, neighbor.y, neighbor.z);
    if def(neighbor_id).opaque || (block.translucent && neighbor_id == id) {
        return None;
    }
    let corners = face_corners(face, bp);
    let mut block_light = [0u8; 4];
    let mut sky_light = [0u8; 4];
    for (i, corner) in corners.into_iter().enumerate() {
        let (block, sky) = corner_light(padded, bp, face, corner, id);
        block_light[i] = block as u8;
        sky_light[i] = sky as u8;
    }
    Some((
        FaceKey {
            ao: [
                face_ao(padded, bp, face, corners[0]),
                face_ao(padded, bp, face, corners[1]),
                face_ao(padded, bp, face, corners[2]),
                face_ao(padded, bp, face, corners[3]),
            ]
            .map(|value| value as u8),
            tex: block.textures[face],
            block_light,
            sky_light,
            lowered: lowered_pattern(
                id == crate::world::block::WATER && padded.get(bp.x, bp.y + 1, bp.z) != id,
                face,
            ),
        },
        (id == LEAVES) || (id == GRASS && face == 2),
    ))
}

/// Build a Lysenko greedy mesh. Cells merge only when texture and all four AO
/// values match exactly; this deliberately keeps the key small until lighting.
pub fn mesh_chunk_greedy(padded: &PaddedChunk) -> ChunkMesh {
    mesh_chunk_greedy_pass(padded, false)
}

pub fn mesh_chunk_greedy_all(padded: &PaddedChunk) -> ChunkMeshes {
    ChunkMeshes {
        opaque: mesh_chunk_greedy_pass(padded, false),
        translucent: mesh_chunk_greedy_pass(padded, true),
        water: mesh_water_greedy(padded),
    }
}

/// Greedy WATER mesh.  Water remains a separate draw list because its shader
/// is a replacement pass; surface and upper side quads are deliberately kept
/// at most 2x2 so Gerstner displacement cannot span a visibly flat patch.
fn mesh_water_greedy(padded: &PaddedChunk) -> ChunkMesh {
    let mut mesh = ChunkMesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    for (face, face_normal) in FACE_NORMALS.iter().copied().enumerate() {
        for layer in 0..32 {
            let mut mask = [[None; 32]; 32];
            for (v, row) in mask.iter_mut().enumerate() {
                for (u, cell) in row.iter_mut().enumerate() {
                    let bp = cell_bp(face, layer, u, v);
                    if padded.get(bp.x, bp.y, bp.z) != crate::world::block::WATER {
                        continue;
                    }
                    let neighbor = bp + face_normal;
                    let neighbor_id = padded.get(neighbor.x, neighbor.y, neighbor.z);
                    if def(neighbor_id).opaque || neighbor_id == crate::world::block::WATER {
                        continue;
                    }
                    let corners = face_corners(face, bp);
                    let mut block_light = [0u8; 4];
                    let mut sky_light = [0u8; 4];
                    for (i, corner) in corners.into_iter().enumerate() {
                        let (block, sky) =
                            corner_light(padded, bp, face, corner, crate::world::block::WATER);
                        block_light[i] = block as u8;
                        sky_light[i] = sky as u8;
                    }
                    *cell = Some(FaceKey {
                        ao: corners.map(|corner| face_ao(padded, bp, face, corner) as u8),
                        tex: def(crate::world::block::WATER).textures[face],
                        block_light,
                        sky_light,
                        lowered: lowered_pattern(
                            padded.get(bp.x, bp.y + 1, bp.z) != crate::world::block::WATER,
                            face,
                        ),
                    });
                }
            }
            let mut v = 0;
            while v < 32 {
                let mut u = 0;
                while u < 32 {
                    let Some(key) = mask[v][u] else {
                        u += 1;
                        continue;
                    };
                    let cap = if face == 2 || key.lowered.iter().any(|&value| value) {
                        2
                    } else {
                        32
                    };
                    let mut width = 1;
                    while u + width < 32 && width < cap && mask[v][u + width] == Some(key) {
                        width += 1;
                    }
                    let mut height = 1;
                    'grow: while v + height < 32 && height < cap {
                        for cell in &mask[v + height][u..u + width] {
                            if *cell != Some(key) {
                                break 'grow;
                            }
                        }
                        height += 1;
                    }
                    let bp = cell_bp(face, layer, u, v);
                    let corners = face_corners(face, bp);
                    let e1 = corners[1] - corners[0];
                    let e2 = corners[3] - corners[0];
                    let quad = [
                        corners[0],
                        corners[0] + e1 * width as i32,
                        corners[0] + e1 * width as i32 + e2 * height as i32,
                        corners[0] + e2 * height as i32,
                    ];
                    append_quad_with_lowered(
                        &mut mesh,
                        quad,
                        face,
                        key.ao.map(u32::from),
                        key.tex as u32,
                        key.block_light.map(u32::from),
                        key.sky_light.map(u32::from),
                        key.lowered,
                    );
                    for row in &mut mask[v..v + height] {
                        for cell in &mut row[u..u + width] {
                            *cell = None;
                        }
                    }
                    u += width;
                }
                v += 1;
            }
        }
    }
    mesh
}

fn mesh_chunk_greedy_pass(padded: &PaddedChunk, translucent_pass: bool) -> ChunkMesh {
    let mut mesh = ChunkMesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    for face in 0..6 {
        for layer in 0..32 {
            let mut mask = [[None; 32]; 32];
            for (v, row) in mask.iter_mut().enumerate() {
                for (u, cell) in row.iter_mut().enumerate() {
                    if let Some((key, eligible)) =
                        face_cell(padded, face, cell_bp(face, layer, u, v), translucent_pass)
                    {
                        *cell = Some((key, eligible));
                    }
                }
            }

            let mut v = 0;
            while v < 32 {
                let mut u = 0;
                while u < 32 {
                    let Some(key) = mask[v][u] else {
                        u += 1;
                        continue;
                    };
                    let mut width = 1;
                    let cap = if key.1 { 4 } else { 32 };
                    while u + width < 32 && width < cap && mask[v][u + width] == Some(key) {
                        width += 1;
                    }
                    let mut height = 1;
                    'grow: while v + height < 32 && height < cap {
                        for cell in &mask[v + height][u..u + width] {
                            if *cell != Some(key) {
                                break 'grow;
                            }
                        }
                        height += 1;
                    }

                    let bp = cell_bp(face, layer, u, v);
                    let corners = face_corners(face, bp);
                    let e1 = corners[1] - corners[0];
                    let e2 = corners[3] - corners[0];
                    let quad = [
                        corners[0],
                        corners[0] + e1 * width as i32,
                        corners[0] + e1 * width as i32 + e2 * height as i32,
                        corners[0] + e2 * height as i32,
                    ];
                    let face_key = key.0;
                    append_quad_with_lowered(
                        &mut mesh,
                        quad,
                        face,
                        face_key.ao.map(u32::from),
                        face_key.tex as u32,
                        face_key.block_light.map(u32::from),
                        face_key.sky_light.map(u32::from),
                        face_key.lowered,
                    );
                    for row in &mut mask[v..v + height] {
                        for cell in &mut row[u..u + width] {
                            *cell = None;
                        }
                    }
                    u += width;
                }
                v += 1;
            }
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::{mesh_chunk, unpack};
    use crate::world::block::{DIRT, STONE, WATER};

    fn area(mesh: &ChunkMesh) -> usize {
        mesh.vertices
            .chunks_exact(4)
            .map(|q| {
                // Every face is axis aligned; recover the rectangle area from edges.
                let p = |v| IVec3::new(unpack(v).0 as i32, unpack(v).1 as i32, unpack(v).2 as i32);
                let u = p(q[1]) - p(q[0]);
                let v = p(q[3]) - p(q[0]);
                (u.abs().max_element() * v.abs().max_element()) as usize
            })
            .sum()
    }

    #[test]
    fn greedy_quad_area_equals_culled_face_count() {
        for seed in 0..3u32 {
            let mut p = PaddedChunk::new();
            let mut x = seed.wrapping_mul(747796405).wrapping_add(2891336453);
            for y in 0..32 {
                for z in 0..32 {
                    for xx in 0..32 {
                        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
                        if x % 13 == 0 {
                            p.set(xx, y, z, STONE);
                        }
                    }
                }
            }
            assert_eq!(
                area(&mesh_chunk_greedy(&p)),
                mesh_chunk(&p).indices.len() / 6
            );
        }
    }

    #[test]
    fn greedy_never_merges_different_keys() {
        let mut p = PaddedChunk::new();
        for x in 0..32 {
            p.set(x, 0, 0, if x < 16 { STONE } else { DIRT });
        }
        let mesh = mesh_chunk_greedy(&p);
        let top: Vec<_> = mesh
            .vertices
            .iter()
            .filter(|vertex| unpack(**vertex).3 == 2)
            .collect();
        assert_eq!(top.len(), 8, "two top-face keys must produce two quads");
        let mut textures: Vec<_> = top.iter().map(|vertex| unpack(**vertex).5).collect();
        textures.sort_unstable();
        textures.dedup();
        assert_eq!(textures.len(), 2);
    }

    #[test]
    fn greedy_never_merges_different_light_keys() {
        let mut p = PaddedChunk::new();
        p.set(0, 0, 0, STONE);
        p.set(1, 0, 0, STONE);
        // Both +Y faces are otherwise identical, but their front corner light differs.
        p.set_light(0, 1, 0, crate::world::chunk::pack_light(3, 7));
        p.set_light(1, 1, 0, crate::world::chunk::pack_light(9, 7));
        let mesh = mesh_chunk_greedy(&p);
        let top = mesh.vertices.iter().filter(|v| unpack(**v).3 == 2).count();
        assert_eq!(top, 8, "different corner light keys must produce two quads");
    }

    #[test]
    fn greedy_flat_slab_top_is_one_quad() {
        let mut p = PaddedChunk::new();
        for z in 0..32 {
            for x in 0..32 {
                p.set(x, 0, z, STONE);
            }
        }
        let mesh = mesh_chunk_greedy(&p);
        let top = mesh.vertices.iter().filter(|v| unpack(**v).3 == 2).count();
        assert_eq!(top, 4);
    }

    #[test]
    fn water_surface_greedy_quads_are_capped_at_two_cells() {
        let mut p = PaddedChunk::new();
        for z in 0..32 {
            for x in 0..32 {
                p.set(x, 0, z, WATER);
            }
        }
        let mesh = mesh_chunk_greedy_all(&p).water;
        let top = mesh
            .vertices
            .chunks_exact(4)
            .filter(|quad| unpack(quad[0]).3 == 2)
            .collect::<Vec<_>>();
        assert_eq!(top.len(), 256);
        assert!(top.iter().all(|quad| {
            let p0 = IVec3::new(
                unpack(quad[0]).0 as i32,
                unpack(quad[0]).1 as i32,
                unpack(quad[0]).2 as i32,
            );
            let p1 = IVec3::new(
                unpack(quad[1]).0 as i32,
                unpack(quad[1]).1 as i32,
                unpack(quad[1]).2 as i32,
            );
            let p3 = IVec3::new(
                unpack(quad[3]).0 as i32,
                unpack(quad[3]).1 as i32,
                unpack(quad[3]).2 as i32,
            );
            (p1 - p0).abs().max_element() <= 2 && (p3 - p0).abs().max_element() <= 2
        }));
    }
}
