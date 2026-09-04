use super::block::{AIR, BlockId};
use super::coords::chunk_index;
use glam::UVec3;

pub const CHUNK_VOLUME: usize = 32 * 32 * 32;
pub const PADDED: usize = 34;

pub struct Chunk {
    pub(crate) blocks: Box<[BlockId; CHUNK_VOLUME]>,
    pub(crate) non_air: u32,
}

impl Chunk {
    pub fn new_air() -> Self {
        Self {
            blocks: Box::new([AIR; CHUNK_VOLUME]),
            non_air: 0,
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
}

pub struct PaddedChunk {
    pub(crate) blocks: Box<[BlockId; PADDED * PADDED * PADDED]>,
}

impl PaddedChunk {
    pub fn new() -> Self {
        Self {
            blocks: Box::new([AIR; PADDED * PADDED * PADDED]),
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
