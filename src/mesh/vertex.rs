use bytemuck::{Pod, Zeroable};

/// Compact chunk-local vertex consumed directly by the chunk shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq, Eq)]
pub struct ChunkVertex {
    pub a: u32,
    pub b: u32,
}

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
}
