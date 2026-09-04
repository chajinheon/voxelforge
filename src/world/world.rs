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
        let local = local_of(bp);
        chunk.set(local, id);
        self.dirty.insert(cp);
        for (axis, sign) in [
            (
                0,
                (local.x == 0)
                    .then_some(-1)
                    .or_else(|| (local.x == 31).then_some(1)),
            ),
            (
                1,
                (local.y == 0)
                    .then_some(-1)
                    .or_else(|| (local.y == 31).then_some(1)),
            ),
            (
                2,
                (local.z == 0)
                    .then_some(-1)
                    .or_else(|| (local.z == 31).then_some(1)),
            ),
        ] {
            if let Some(sign) = sign {
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

    pub fn chunk(&self, cp: IVec3) -> Option<&Chunk> {
        self.chunks.get(&cp)
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn padded(&self, cp: IVec3) -> PaddedChunk {
        let mut chunks = [[[None; 3]; 3]; 3];
        for dy in -1..=1 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let neighbor = cp + IVec3::new(dx, dy, dz);
                    if (0..WORLD_CHUNKS_Y).contains(&neighbor.y) {
                        chunks[(dy + 1) as usize][(dz + 1) as usize][(dx + 1) as usize] =
                            self.chunks.get(&neighbor);
                    }
                }
            }
        }

        let mut padded = PaddedChunk::new();
        for y in -1..=CHUNK_SIZE {
            for z in -1..=CHUNK_SIZE {
                for x in -1..=CHUNK_SIZE {
                    let bp = cp * CHUNK_SIZE + IVec3::new(x, y, z);
                    let id = if bp.y < 0 {
                        STONE
                    } else if bp.y >= CHUNK_SIZE * WORLD_CHUNKS_Y {
                        AIR
                    } else {
                        let dx = (x == CHUNK_SIZE) as i32 - (x == -1) as i32;
                        let dy = (y == CHUNK_SIZE) as i32 - (y == -1) as i32;
                        let dz = (z == CHUNK_SIZE) as i32 - (z == -1) as i32;
                        let local = IVec3::new(
                            if dx == -1 {
                                CHUNK_SIZE - 1
                            } else {
                                x & (CHUNK_SIZE - 1)
                            },
                            if dy == -1 {
                                CHUNK_SIZE - 1
                            } else {
                                y & (CHUNK_SIZE - 1)
                            },
                            if dz == -1 {
                                CHUNK_SIZE - 1
                            } else {
                                z & (CHUNK_SIZE - 1)
                            },
                        )
                        .as_uvec3();
                        chunks[(dy + 1) as usize][(dz + 1) as usize][(dx + 1) as usize]
                            .map_or(AIR, |chunk| chunk.get(local))
                    };
                    padded.set(x, y, z, id);
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
        let removed: Vec<IVec3> = self
            .chunks
            .keys()
            .copied()
            .filter(|cp| (cp.x - center.x).abs() > radius || (cp.z - center.z).abs() > radius)
            .collect();

        for cp in &removed {
            self.chunks.remove(cp);
            self.dirty.remove(cp);
        }

        for cp in removed {
            for axis in 0..3 {
                for sign in [-1, 1] {
                    let mut neighbor = cp;
                    match axis {
                        0 => neighbor.x += sign,
                        1 => neighbor.y += sign,
                        _ => neighbor.z += sign,
                    }
                    if self.chunks.contains_key(&neighbor) {
                        self.dirty.insert(neighbor);
                    }
                }
            }
        }
    }

    pub fn take_dirty(&mut self) -> Vec<IVec3> {
        self.dirty.drain().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::World;
    use crate::world::block::STONE;
    use crate::world::coords::{CHUNK_SIZE, WORLD_CHUNKS_Y};
    use glam::IVec3;

    #[test]
    fn unload_marks_kept_face_neighbor_dirty_without_duplicate_loads() {
        let mut world = World::new(0);
        let kept = IVec3::new(0, 0, 0);
        let removed = IVec3::new(1, 0, 0);

        assert!(world.ensure_loaded(kept));
        assert!(world.ensure_loaded(removed));
        assert_eq!(world.chunk_count(), 2);
        world.take_dirty();

        world.unload_outside(kept, 0);

        assert_eq!(world.chunk_count(), 1);
        assert!(world.chunk(kept).is_some());
        assert!(world.chunk(removed).is_none());
        assert_eq!(world.take_dirty(), vec![kept]);

        assert!(world.ensure_loaded(removed));
        assert_eq!(world.chunk_count(), 2);
        assert!(!world.ensure_loaded(removed));
        assert_eq!(world.chunk_count(), 2);
        assert!(world.chunk(removed).is_some());
        assert!(!world.ensure_loaded(IVec3::new(0, WORLD_CHUNKS_Y, 0)));
        assert_eq!(world.chunk_count(), 2);
    }

    #[test]
    fn set_block_marks_neighbor_dirty_on_border() {
        let mut world = World::new(0);
        let edited = IVec3::new(0, 0, 0);
        let touched_neighbor = IVec3::new(-1, 0, 0);
        let opposite_neighbor = IVec3::new(1, 0, 0);

        assert!(world.ensure_loaded(edited));
        assert!(world.ensure_loaded(touched_neighbor));
        assert!(world.ensure_loaded(opposite_neighbor));
        world.take_dirty();

        assert!(world.set_block(edited * CHUNK_SIZE, STONE));

        let dirty = world.take_dirty();
        assert!(dirty.contains(&edited));
        assert!(dirty.contains(&touched_neighbor));
        assert!(!dirty.contains(&opposite_neighbor));
    }

    #[test]
    fn padded_matches_world_lookup_at_vertical_boundaries() {
        let mut world = World::new(0);
        let cases = [IVec3::new(0, 0, 0), IVec3::new(0, WORLD_CHUNKS_Y - 1, 0)];

        for cp in cases {
            assert!(world.ensure_loaded(cp));
            let padded = world.padded(cp);
            for y in -1..=CHUNK_SIZE {
                for z in -1..=CHUNK_SIZE {
                    for x in -1..=CHUNK_SIZE {
                        let bp = cp * CHUNK_SIZE + IVec3::new(x, y, z);
                        assert_eq!(padded.get(x, y, z), world.get_block(bp));
                    }
                }
            }
        }
    }
}
