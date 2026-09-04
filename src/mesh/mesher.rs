use glam::IVec3;

use crate::world::{
    block::{AIR, def},
    chunk::PaddedChunk,
    coords::{FACE_NORMALS, face_corners},
};

use super::vertex::{ChunkVertex, pack};

pub struct ChunkMesh {
    pub vertices: Vec<ChunkVertex>,
    pub indices: Vec<u32>,
}

const FACE_TANGENTS: [(IVec3, IVec3); 6] = [
    (IVec3::Y, IVec3::Z),
    (IVec3::Y, IVec3::Z),
    (IVec3::X, IVec3::Z),
    (IVec3::X, IVec3::Z),
    (IVec3::X, IVec3::Y),
    (IVec3::X, IVec3::Y),
];

/// Build a face-culled mesh for one 32³ chunk and its one-block padding.
pub fn mesh_chunk(padded: &PaddedChunk) -> ChunkMesh {
    let mut mesh = ChunkMesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    };

    for y in 0..32 {
        for z in 0..32 {
            for x in 0..32 {
                let bp = IVec3::new(x, y, z);
                let id = padded.get(x, y, z);
                let block = def(id);
                if id == AIR || block.name == "air" {
                    continue;
                }

                for face in 0..6 {
                    let neighbor = bp + FACE_NORMALS[face];
                    if def(padded.get(neighbor.x, neighbor.y, neighbor.z)).opaque {
                        continue;
                    }

                    let base = mesh.vertices.len() as u32;
                    let corners = face_corners(face, bp);
                    for position in corners {
                        let (tangent_1, tangent_2) = FACE_TANGENTS[face];
                        let s1 = corner_side(bp, position, tangent_1);
                        let s2 = corner_side(bp, position, tangent_2);
                        // Occluders live in the layer *in front of* the face
                        // (BLUEPRINT §4). Blocks in the block's own layer share
                        // the face plane and never occlude it.
                        let side1 = opaque(padded, neighbor + s1);
                        let side2 = opaque(padded, neighbor + s2);
                        let diagonal = opaque(padded, neighbor + s1 + s2);
                        let ao = if side1 && side2 {
                            0
                        } else {
                            3 - (side1 as u32 + side2 as u32 + diagonal as u32)
                        };
                        mesh.vertices.push(pack(
                            position.x as u32,
                            position.y as u32,
                            position.z as u32,
                            face as u32,
                            ao,
                            block.textures[face] as u32,
                            0,
                            15,
                        ));
                    }
                    mesh.indices.extend_from_slice(&[
                        base,
                        base + 1,
                        base + 2,
                        base,
                        base + 2,
                        base + 3,
                    ]);
                }
            }
        }
    }
    mesh
}

fn opaque(padded: &PaddedChunk, position: IVec3) -> bool {
    def(padded.get(position.x, position.y, position.z)).opaque
}

fn corner_side(block: IVec3, corner: IVec3, axis: IVec3) -> IVec3 {
    let is_low = (axis.x != 0 && corner.x == block.x)
        || (axis.y != 0 && corner.y == block.y)
        || (axis.z != 0 && corner.z == block.z);
    if is_low { -axis } else { axis }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{block::STONE, chunk::PaddedChunk};

    fn chunk_with(blocks: &[(i32, i32, i32)]) -> PaddedChunk {
        let mut padded = PaddedChunk::new();
        for &(x, y, z) in blocks {
            padded.set(x, y, z, STONE);
        }
        padded
    }

    fn ao_of(mesh: &ChunkMesh, range: std::ops::Range<usize>) -> Vec<u32> {
        mesh.vertices[range]
            .iter()
            .map(|vertex| crate::mesh::vertex::unpack(*vertex).4)
            .collect()
    }

    #[test]
    fn mesher_lone_block_has_6_faces() {
        let mesh = mesh_chunk(&chunk_with(&[(0, 0, 0)]));
        assert_eq!(mesh.vertices.len(), 24);
        assert_eq!(mesh.indices.len(), 36);
        // Nothing around the block: every vertex is fully open.
        assert!(ao_of(&mesh, 0..24).iter().all(|&ao| ao == 3));
    }

    #[test]
    fn mesher_ao_reads_layer_in_front_of_face() {
        // A block directly below shares the +X face plane: coplanar neighbours
        // must not darken that face (face 0 is emitted first for block A).
        let stacked = mesh_chunk(&chunk_with(&[(0, 0, 0), (0, -1, 0)]));
        assert_eq!(ao_of(&stacked, 0..4), [3, 3, 3, 3]);

        // A block diagonally up (+X,+Y) protrudes in front of A's top face and
        // beside A's +X face; only the corners touching it are occluded.
        let diagonal = mesh_chunk(&chunk_with(&[(0, 0, 0), (1, 1, 0)]));
        assert_eq!(ao_of(&diagonal, 0..4), [3, 2, 2, 3]); // +X face: corners at y+1
        assert_eq!(ao_of(&diagonal, 8..12), [3, 3, 2, 2]); // +Y face: corners at x+1

        // Flat ground: a 3x3 slab, centre block's top face is fully open.
        let mut slab = Vec::new();
        for z in -1..=1 {
            for x in -1..=1 {
                slab.push((x, 0, z));
            }
        }
        let flat = mesh_chunk(&chunk_with(&slab));
        // Centre block (0,0,0) is the first meshed block at (x=0,z=0) only if
        // negative coordinates are padding; they are, so it is emitted first.
        // Its +X/-X/+Z/-Z faces are culled by neighbours; +Y is face index 0
        // of the emitted list, -Y second.
        assert_eq!(ao_of(&flat, 0..4), [3, 3, 3, 3]);
    }

    #[test]
    fn mesher_enclosed_block_has_0_faces() {
        let mut blocks = Vec::with_capacity(32 * 32 * 32);
        for y in -1..=32 {
            for z in -1..=32 {
                for x in -1..=32 {
                    blocks.push((x, y, z));
                }
            }
        }
        let mesh = mesh_chunk(&chunk_with(&blocks));
        assert!(mesh.vertices.is_empty());
        assert!(mesh.indices.is_empty());
    }

    #[test]
    fn mesher_two_adjacent_blocks_10_faces() {
        let mesh = mesh_chunk(&chunk_with(&[(0, 0, 0), (1, 0, 0)]));
        assert_eq!(mesh.vertices.len(), 40);
        assert_eq!(mesh.indices.len(), 60);
    }

    #[test]
    fn mesher_border_face_culled_by_padding() {
        let mesh = mesh_chunk(&chunk_with(&[(31, 0, 0), (32, 0, 0)]));
        assert_eq!(mesh.vertices.len(), 20);
        assert_eq!(mesh.indices.len(), 30);
    }
}
