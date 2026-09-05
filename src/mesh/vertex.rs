use bytemuck::{Pod, Zeroable};

/// Compact chunk-local vertex consumed directly by the chunk shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq, Eq)]
pub struct ChunkVertex {
    pub a: u32,
    pub b: u32,
}

/// Reserved vertex bit used to lower the top surface of fluid blocks.
pub const LOWERED_BIT: u32 = 1 << 23;

const FRAC_X_SHIFT: u32 = 24;
const FRAC_Y_SHIFT: u32 = 28;
const FRAC_Z_SHIFT: u32 = 24;

/// Pack a vertex using the layout in BLUEPRINT §4.
#[allow(clippy::too_many_arguments)] // The eight-argument signature is a fixed data contract.
pub fn pack(
    x: u32,
    y: u32,
    z: u32,
    face: u32,
    ao: u32,
    tex: u32,
    light: u32,
    sky: u32,
) -> ChunkVertex {
    let a = (x & 0x3f)
        | ((y & 0x3f) << 6)
        | ((z & 0x3f) << 12)
        | ((face & 0x7) << 18)
        | ((ao & 0x3) << 21);
    let b = (tex & 0xffff) | ((light & 0xf) << 16) | ((sky & 0xf) << 20);
    ChunkVertex { a, b }
}

/// Pack a vertex with 1/16-block subvoxel coordinates in the previously
/// reserved bits. Integer coordinates remain the legacy six-bit fields.
#[allow(clippy::too_many_arguments)]
pub fn pack_with_fraction(
    x: u32,
    y: u32,
    z: u32,
    frac_x: u32,
    frac_y: u32,
    frac_z: u32,
    face: u32,
    ao: u32,
    tex: u32,
    light: u32,
    sky: u32,
) -> ChunkVertex {
    assert!(frac_x < 16 && frac_y < 16 && frac_z < 16);
    assert!((x < 32 || frac_x == 0) && (y < 32 || frac_y == 0) && (z < 32 || frac_z == 0));
    let mut vertex = pack(x, y, z, face, ao, tex, light, sky);
    vertex.a |= (frac_x & 0xf) << FRAC_X_SHIFT;
    vertex.a |= (frac_y & 0xf) << FRAC_Y_SHIFT;
    vertex.b |= (frac_z & 0xf) << FRAC_Z_SHIFT;
    vertex
}

pub fn unpack_fraction(v: ChunkVertex) -> (u32, u32, u32) {
    (
        (v.a >> FRAC_X_SHIFT) & 0xf,
        (v.a >> FRAC_Y_SHIFT) & 0xf,
        (v.b >> FRAC_Z_SHIFT) & 0xf,
    )
}

pub fn with_lowered(mut vertex: ChunkVertex, lowered: bool) -> ChunkVertex {
    if lowered {
        vertex.a |= LOWERED_BIT;
    } else {
        vertex.a &= !LOWERED_BIT;
    }
    vertex
}

pub fn is_lowered(vertex: ChunkVertex) -> bool {
    vertex.a & LOWERED_BIT != 0
}

/// Unpack all defined fields. Reserved bits are intentionally omitted.
pub fn unpack(v: ChunkVertex) -> (u32, u32, u32, u32, u32, u32, u32, u32) {
    (
        v.a & 0x3f,
        (v.a >> 6) & 0x3f,
        (v.a >> 12) & 0x3f,
        (v.a >> 18) & 0x7,
        (v.a >> 21) & 0x3,
        v.b & 0xffff,
        (v.b >> 16) & 0xf,
        (v.b >> 20) & 0xf,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_pack_roundtrip() {
        let input = (31, 32, 0, 5, 2, 0x1234, 0, 15);
        assert_eq!(
            unpack(pack(
                input.0, input.1, input.2, input.3, input.4, input.5, input.6, input.7
            )),
            input
        );
    }

    #[test]
    fn vertex_fraction_pack_roundtrip() {
        let v = pack_with_fraction(7, 8, 9, 1, 7, 15, 2, 3, 4, 5, 6);
        assert_eq!(unpack(v), (7, 8, 9, 2, 3, 4, 5, 6));
        assert_eq!(unpack_fraction(v), (1, 7, 15));
    }

    #[test]
    fn cube_vertices_keep_zero_fraction() {
        let v = pack(31, 31, 31, 5, 2, 3, 4, 5);
        assert_eq!(unpack_fraction(v), (0, 0, 0));
    }
}
