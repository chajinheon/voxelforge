use glam::IVec3;

use crate::world::{
    block::{AIR, def},
    chunk::PaddedChunk,
    coords::{FACE_NORMALS, face_corners},
};

use super::mesher::{ChunkMesh, ChunkMeshes, append_quad_with_lowered, face_ao, lowered_pattern};

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
) -> Option<([u32; 4], u32, [bool; 4])> {
    let id = padded.get(bp.x, bp.y, bp.z);
    let block = def(id);
    if id == AIR || block.name == "air" || block.translucent != translucent_pass {
        return None;
    }
    let neighbor = bp + FACE_NORMALS[face];
    let neighbor_id = padded.get(neighbor.x, neighbor.y, neighbor.z);
    if def(neighbor_id).opaque || (block.translucent && neighbor_id == id) {
        return None;
    }
    let corners = face_corners(face, bp);
    Some((
        [
            face_ao(padded, bp, face, corners[0]),
            face_ao(padded, bp, face, corners[1]),
            face_ao(padded, bp, face, corners[2]),
            face_ao(padded, bp, face, corners[3]),
        ],
        block.textures[face] as u32,
        lowered_pattern(
            id == crate::world::block::WATER && padded.get(bp.x, bp.y + 1, bp.z) != id,
            face,
        ),
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
    }
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
                    if let Some((ao, tex, lowered)) =
                        face_cell(padded, face, cell_bp(face, layer, u, v), translucent_pass)
                    {
                        *cell = Some((ao, tex, lowered));
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
                    while u + width < 32 && mask[v][u + width] == Some(key) {
                        width += 1;
                    }
                    let mut height = 1;
                    'grow: while v + height < 32 {
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
                    let ao = [key.0[0], key.0[1], key.0[2], key.0[3]];
                    append_quad_with_lowered(&mut mesh, quad, face, ao, key.1, key.2);
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
    use crate::world::block::{DIRT, STONE};

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
}
