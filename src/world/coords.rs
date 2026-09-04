use glam::{IVec3, UVec3};

pub const CHUNK_SIZE: i32 = 32;
pub const CHUNK_BITS: i32 = 5;
pub const WORLD_CHUNKS_Y: i32 = 8;

pub const FACE_NORMALS: [IVec3; 6] = [
    IVec3::X,
    IVec3::NEG_X,
    IVec3::Y,
    IVec3::NEG_Y,
    IVec3::Z,
    IVec3::NEG_Z,
];

pub fn chunk_of(bp: IVec3) -> IVec3 {
    bp >> CHUNK_BITS
}

pub fn local_of(bp: IVec3) -> UVec3 {
    (bp & IVec3::splat(CHUNK_SIZE - 1)).as_uvec3()
}

pub fn chunk_index(l: UVec3) -> usize {
    (l.x + CHUNK_SIZE as u32 * (l.z + CHUNK_SIZE as u32 * l.y)) as usize
}

pub fn face_corners(face: usize, bp: IVec3) -> [IVec3; 4] {
    let x = IVec3::X;
    let y = IVec3::Y;
    let z = IVec3::Z;

    match face {
        0 => [bp + x, bp + x + y, bp + x + y + z, bp + x + z],
        1 => [bp, bp + z, bp + y + z, bp + y],
        2 => [bp + y, bp + y + z, bp + x + y + z, bp + x + y],
        3 => [bp, bp + x, bp + x + z, bp + z],
        4 => [bp + z, bp + x + z, bp + x + y + z, bp + y + z],
        5 => [bp, bp + y, bp + x + y, bp + x],
        _ => panic!("face index out of range: {face}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coords_floor_div_negative() {
        let cases = [
            (
                IVec3::new(-1, -32, -33),
                IVec3::new(-1, -1, -2),
                UVec3::new(31, 0, 31),
            ),
            (
                IVec3::new(0, 31, 32),
                IVec3::new(0, 0, 1),
                UVec3::new(0, 31, 0),
            ),
        ];

        for (block, expected_chunk, expected_local) in cases {
            assert_eq!(chunk_of(block), expected_chunk);
            assert_eq!(local_of(block), expected_local);
        }
    }

    #[test]
    fn chunk_index_roundtrip() {
        for y in 0..CHUNK_SIZE as u32 {
            for z in 0..CHUNK_SIZE as u32 {
                for x in 0..CHUNK_SIZE as u32 {
                    let local = UVec3::new(x, y, z);
                    let index = chunk_index(local);
                    assert_eq!(index, (x + 32 * (z + 32 * y)) as usize);
                    assert_eq!(index % 32, x as usize);
                    assert_eq!((index / 32) % 32, z as usize);
                    assert_eq!(index / (32 * 32), y as usize);
                }
            }
        }
    }
}
