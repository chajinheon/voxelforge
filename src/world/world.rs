use super::block::{AIR, BlockId, STONE};
use super::chunk::{Chunk, PaddedChunk};
use super::coords::{CHUNK_SIZE, WORLD_CHUNKS_Y, chunk_of, local_of};
use super::r#gen::WorldGen;
use glam::IVec3;
use std::collections::{HashMap, HashSet};

pub struct World {
    pub(crate) chunks: HashMap<IVec3, Chunk>,
    pub(crate) generator: WorldGen,
    dirty: HashSet<IVec3>,
}

impl World {
    pub fn new(seed: u64) -> Self {
        Self {
            chunks: HashMap::new(),
            generator: WorldGen::new(seed),
            dirty: HashSet::new(),
        }
    }

    pub fn get_block(&self, bp: IVec3) -> BlockId {
        if bp.y < 0 {
            return STONE;
        }
        if bp.y >= CHUNK_SIZE * WORLD_CHUNKS_Y {
            return AIR;
        }
        let cp = chunk_of(bp);
        self.chunks.get(&cp).map_or(AIR, |c| c.get(local_of(bp)))
    }

    pub fn set_block(&mut self, bp: IVec3, id: BlockId) -> bool {
        if bp.y < 0 || bp.y >= CHUNK_SIZE * WORLD_CHUNKS_Y {
            return false;
        }
        let cp = chunk_of(bp);
        let Some(chunk) = self.chunks.get_mut(&cp) else {
            return false;
        };
        chunk.set(local_of(bp), id);
        self.dirty.insert(cp);
        let local = local_of(bp);
        for (axis, edge) in [
            (0, local.x == 0 || local.x == 31),
            (1, local.y == 0 || local.y == 31),
            (2, local.z == 0 || local.z == 31),
        ] {
            if edge {
                for sign in [-1, 1] {
                    let mut n = cp;
                    match axis {
                        0 => n.x += sign,
                        1 => n.y += sign,
                        _ => n.z += sign,
                    }
                    if self.chunks.contains_key(&n) {
                        self.dirty.insert(n);
                    }
                }
            }
        }
        true
    }

    pub fn chunk(&self, cp: IVec3) -> Option<&Chunk> {
        self.chunks.get(&cp)
    }

    pub fn padded(&self, cp: IVec3) -> PaddedChunk {
        let mut padded = PaddedChunk::new();
        for y in -1..=CHUNK_SIZE {
            for z in -1..=CHUNK_SIZE {
                for x in -1..=CHUNK_SIZE {
                    let bp = cp * CHUNK_SIZE + IVec3::new(x, y, z);
                    padded.set(x, y, z, self.get_block(bp));
                }
            }
        }
        padded
    }

    pub fn ensure_loaded(&mut self, cp: IVec3) -> bool {
        if cp.y < 0 || cp.y >= WORLD_CHUNKS_Y {
            return false;
        }
        if self.chunks.contains_key(&cp) {
            return false;
        }
        let chunk = self.generator.generate(cp);
        self.chunks.insert(cp, chunk);
        self.dirty.insert(cp);
        for axis in 0..3 {
            for sign in [-1, 1] {
                let mut n = cp;
                match axis {
                    0 => n.x += sign,
                    1 => n.y += sign,
                    _ => n.z += sign,
                }
                if self.chunks.contains_key(&n) {
                    self.dirty.insert(n);
                }
            }
        }
        true
    }

    pub fn unload_outside(&mut self, center: IVec3, radius: i32) {
        self.chunks.retain(|cp, _| {
            let keep = (cp.x - center.x).abs() <= radius && (cp.z - center.z).abs() <= radius;
            if !keep {
                self.dirty.remove(cp);
            }
            keep
        });
    }

    pub fn take_dirty(&mut self) -> Vec<IVec3> {
        self.dirty.drain().collect()
    }
}
