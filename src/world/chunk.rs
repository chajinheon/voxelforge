use super::block::{AIR, BlockId};
use super::coords::chunk_index;
use glam::UVec3;

pub const CHUNK_VOLUME: usize = 32 * 32 * 32;
pub const PADDED: usize = 34;
pub const LIGHT_MAX: u8 = 15;
pub const BLOCK_LIGHT_MASK: u8 = 0x0f;
pub const SKY_LIGHT_MASK: u8 = 0xf0;

pub fn pack_light(block: u8, sky: u8) -> u8 {
    block.min(LIGHT_MAX) | (sky.min(LIGHT_MAX) << 4)
}

pub fn block_light(packed: u8) -> u8 {
    packed & BLOCK_LIGHT_MASK
}

pub fn sky_light(packed: u8) -> u8 {
    (packed & SKY_LIGHT_MASK) >> 4
}

pub struct Chunk {
    pub(crate) blocks: Box<[BlockId; CHUNK_VOLUME]>,
    pub(crate) non_air: u32,
    pub(crate) light: Box<[u8; CHUNK_VOLUME]>,
    light_initialized: bool,
}

impl Chunk {
    pub fn new_air() -> Self {
        Self {
            blocks: Box::new([AIR; CHUNK_VOLUME]),
            non_air: 0,
            light: Box::new([0; CHUNK_VOLUME]),
            light_initialized: false,
        }
    }
    pub fn get(&self, l: UVec3) -> BlockId {
        self.blocks[chunk_index(l)]
    }
    pub fn set(&mut self, l: UVec3, id: BlockId) {
        let slot = &mut self.blocks[chunk_index(l)];
        if *slot == AIR && id != AIR {
            self.non_air += 1;
        } else if *slot != AIR && id == AIR {
            self.non_air -= 1;
        }
        *slot = id;
    }
    pub fn is_empty(&self) -> bool {
        self.non_air == 0
    }
    pub fn get_light(&self, l: UVec3) -> u8 {
        self.light[chunk_index(l)]
    }
    pub fn set_light(&mut self, l: UVec3, packed: u8) {
        self.light[chunk_index(l)] = packed;
    }
    pub fn light_initialized(&self) -> bool {
        self.light_initialized
    }
    pub(crate) fn set_light_initialized(&mut self, value: bool) {
        self.light_initialized = value;
    }
}

pub struct PaddedChunk {
    pub(crate) blocks: Box<[BlockId; PADDED * PADDED * PADDED]>,
    pub(crate) light: Box<[u8; PADDED * PADDED * PADDED]>,
}

impl PaddedChunk {
    pub fn new() -> Self {
        Self {
            blocks: Box::new([AIR; PADDED * PADDED * PADDED]),
            light: Box::new([0; PADDED * PADDED * PADDED]),
        }
    }
    pub fn set(&mut self, x: i32, y: i32, z: i32, id: BlockId) {
        let i = ((x + 1) + PADDED as i32 * ((z + 1) + PADDED as i32 * (y + 1))) as usize;
        self.blocks[i] = id;
    }
    pub fn get(&self, x: i32, y: i32, z: i32) -> BlockId {
        debug_assert!((-1..=32).contains(&x) && (-1..=32).contains(&y) && (-1..=32).contains(&z));
        let i = ((x + 1) + PADDED as i32 * ((z + 1) + PADDED as i32 * (y + 1))) as usize;
        self.blocks[i]
    }
    pub fn set_light(&mut self, x: i32, y: i32, z: i32, packed: u8) {
        let i = ((x + 1) + PADDED as i32 * ((z + 1) + PADDED as i32 * (y + 1))) as usize;
        self.light[i] = packed;
    }
    pub fn get_light(&self, x: i32, y: i32, z: i32) -> u8 {
        debug_assert!((-1..=32).contains(&x) && (-1..=32).contains(&y) && (-1..=32).contains(&z));
        let i = ((x + 1) + PADDED as i32 * ((z + 1) + PADDED as i32 * (y + 1))) as usize;
        self.light[i]
    }
}

impl Default for PaddedChunk {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::STONE;
    #[test]
    fn chunk_set_get_and_non_air() {
        let mut chunk = Chunk::new_air();
        let p = UVec3::new(3, 4, 5);
        assert_eq!(chunk.get(p), AIR);
        assert!(chunk.is_empty());
        chunk.set(p, STONE);
        assert_eq!(chunk.get(p), STONE);
        assert!(!chunk.is_empty());
        chunk.set(p, STONE);
        assert!(!chunk.is_empty());
        chunk.set(p, AIR);
        assert!(chunk.is_empty());
    }
}
