use super::block::{AIR, BlockId, STONE};
use super::chunk::{CHUNK_VOLUME, Chunk, PaddedChunk, pack_light};
use super::coords::{CHUNK_SIZE, WORLD_CHUNKS_Y, chunk_of, local_of};
use super::r#gen::WorldGen;
use super::light::LightColumn;
use super::save::SaveDir;
use anyhow::{Context, Result};
use glam::IVec3;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub struct World {
    pub(crate) chunks: HashMap<IVec3, Chunk>,
    pub(crate) generator: Arc<WorldGen>,
    seed: u64,
    versions: HashMap<IVec3, u64>,
    pub(crate) dirty: HashSet<IVec3>,
    pub(crate) modified: HashSet<IVec3>,
    pub(crate) saved_version: HashMap<IVec3, u64>,
    pub(crate) light_dirty: HashSet<LightColumn>,
    pub(crate) light_epochs: HashMap<LightColumn, u64>,
}

impl World {
    pub fn new(seed: u64) -> Self {
        Self {
            chunks: HashMap::new(),
            generator: Arc::new(WorldGen::new(seed)),
            seed,
            versions: HashMap::new(),
            dirty: HashSet::new(),
            modified: HashSet::new(),
            saved_version: HashMap::new(),
            light_dirty: HashSet::new(),
            light_epochs: HashMap::new(),
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

    pub fn get_light(&self, bp: IVec3) -> u8 {
        let cp = chunk_of(bp);
        self.chunks
            .get(&cp)
            .map_or(pack_light(0, 15), |c| c.get_light(local_of(bp)))
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
        if chunk.get(local) == id {
            return false;
        }
        chunk.set(local, id);
        *self.versions.entry(cp).or_insert(1) += 1;
        self.dirty.insert(cp);
        self.modified.insert(cp);
        self.mark_light_dirty_edit(cp);
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

    /// Bytes occupied by the canonical block (u16) plus light (u8) arrays.
    /// This is derived from the live loaded-chunk count, not the stream radius.
    pub fn block_light_memory_bytes(&self) -> usize {
        self.chunk_count()
            .saturating_mul(CHUNK_VOLUME)
            .saturating_mul(3)
    }

    pub fn is_loaded(&self, cp: IVec3) -> bool {
        self.chunks.contains_key(&cp)
    }

    pub fn insert_generated(&mut self, cp: IVec3, chunk: Chunk) {
        if !(0..WORLD_CHUNKS_Y).contains(&cp.y) || self.chunks.contains_key(&cp) {
            return;
        }
        self.chunks.insert(cp, chunk);
        self.versions.insert(cp, 1);
        self.dirty.insert(cp);
        self.mark_light_dirty_insert(cp);
        self.mark_mesh_neighbors_dirty(cp);
    }

    /// Insert a chunk read from disk and retain its modified identity so later
    /// edits are persisted without treating the loaded bytes as a new change.
    pub fn insert_loaded(&mut self, cp: IVec3, chunk: Chunk) {
        if !(0..WORLD_CHUNKS_Y).contains(&cp.y) || self.chunks.contains_key(&cp) {
            return;
        }
        self.chunks.insert(cp, chunk);
        self.versions.insert(cp, 1);
        self.saved_version.insert(cp, 1);
        self.modified.insert(cp);
        self.dirty.insert(cp);
        self.mark_light_dirty_insert(cp);
        self.mark_mesh_neighbors_dirty(cp);
    }

    pub fn chunk_version(&self, cp: IVec3) -> Option<u64> {
        self.versions.get(&cp).copied()
    }

    pub fn generator(&self) -> &Arc<WorldGen> {
        &self.generator
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Persist changed loaded chunks and return the number of files written.
    pub fn save_modified(&mut self, save: &SaveDir) -> Result<usize> {
        let mut targets: Vec<_> = self.modified.iter().copied().collect();
        targets.sort_by_key(|cp| (cp.x, cp.y, cp.z));
        let mut written = 0;
        for cp in targets {
            let Some(version) = self.versions.get(&cp).copied() else {
                continue;
            };
            if self.saved_version.get(&cp) == Some(&version) {
                continue;
            }
            let Some(chunk) = self.chunks.get(&cp) else {
                log::warn!("cannot save unloaded modified chunk {cp:?}");
                continue;
            };
            save.write_chunk(cp, chunk)
                .with_context(|| format!("save modified chunk {cp:?}"))?;
            self.saved_version.insert(cp, version);
            written += 1;
        }
        Ok(written)
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
                    let light = if bp.y < 0 {
                        pack_light(0, 0)
                    } else if bp.y >= CHUNK_SIZE * WORLD_CHUNKS_Y {
                        pack_light(0, 15)
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
                            .map_or(pack_light(0, 15), |chunk| chunk.get_light(local))
                    };
                    padded.set_light(x, y, z, light);
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
        self.insert_generated(cp, chunk);
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
            self.versions.remove(cp);
            self.dirty.remove(cp);
        }

        for cp in removed {
            self.mark_mesh_neighbors_dirty(cp);
            for neighbor in [
                LightColumn {
                    x: cp.x - 1,
                    z: cp.z,
                },
                LightColumn {
                    x: cp.x + 1,
                    z: cp.z,
                },
                LightColumn {
                    x: cp.x,
                    z: cp.z - 1,
                },
                LightColumn {
                    x: cp.x,
                    z: cp.z + 1,
                },
            ] {
                self.mark_light_dirty(neighbor);
            }
            for neighbor in [
                IVec3::new(cp.x - 1, cp.y, cp.z),
                IVec3::new(cp.x + 1, cp.y, cp.z),
                IVec3::new(cp.x, cp.y, cp.z - 1),
                IVec3::new(cp.x, cp.y, cp.z + 1),
            ] {
                if self.chunks.contains_key(&neighbor) {
                    self.dirty.insert(neighbor);
                }
            }
        }
    }

    pub fn take_dirty(&mut self) -> Vec<IVec3> {
        self.dirty.drain().collect()
    }

    fn mark_light_dirty_edit(&mut self, cp: IVec3) {
        let c = LightColumn { x: cp.x, z: cp.z };
        self.mark_light_dirty(c);
        for n in [
            LightColumn { x: c.x - 1, z: c.z },
            LightColumn { x: c.x + 1, z: c.z },
            LightColumn { x: c.x, z: c.z - 1 },
            LightColumn { x: c.x, z: c.z + 1 },
        ] {
            self.mark_light_dirty(n);
        }
    }

    fn mark_light_dirty_insert(&mut self, cp: IVec3) {
        let c = LightColumn { x: cp.x, z: cp.z };
        self.mark_light_dirty(c);
        if self.column_loaded(c) {
            for n in [
                LightColumn { x: c.x - 1, z: c.z },
                LightColumn { x: c.x + 1, z: c.z },
                LightColumn { x: c.x, z: c.z - 1 },
                LightColumn { x: c.x, z: c.z + 1 },
            ] {
                self.mark_light_dirty(n);
            }
        }
    }

    fn mark_mesh_neighbors_dirty(&mut self, cp: IVec3) {
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
    }

    pub(crate) fn mark_light_dirty(&mut self, column: LightColumn) {
        *self.light_epochs.entry(column).or_insert(0) += 1;
        self.light_dirty.insert(column);
    }

    pub(crate) fn mark_light_boundary_dirty(&mut self, column: LightColumn) {
        self.light_dirty.insert(column);
    }
}

#[cfg(test)]
#[path = "world_tests.rs"]
mod tests;
