//! Deterministic, column-local flood-fill lighting.

use super::block::{BlockId, def};
use super::chunk::{block_light, pack_light, sky_light};
use std::collections::VecDeque;

pub const LIGHT_COLUMN_CELLS: usize = 32 * 256 * 32;
pub const LIGHT_BOUNDARY_CELLS: usize = 32 * 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LightColumn {
    pub x: i32,
    pub z: i32,
}

pub struct LightColumnSnapshot {
    pub column: LightColumn,
    pub epoch: u64,
    pub blocks: Box<[BlockId; LIGHT_COLUMN_CELLS]>,
    /// Faces in the order -X, +X, -Z, +Z; each is indexed u + 32*y.
    pub incoming: [Box<[u8; LIGHT_BOUNDARY_CELLS]>; 4],
}

pub struct LightColumnResult {
    pub column: LightColumn,
    pub epoch: u64,
    pub light: Box<[u8; LIGHT_COLUMN_CELLS]>,
    pub boundary: [Box<[u8; LIGHT_BOUNDARY_CELLS]>; 4],
}

#[derive(Debug, Default)]
pub struct LightApplyResult {
    pub changed_chunks: Vec<glam::IVec3>,
    pub changed_boundaries: [bool; 4],
}

impl super::world::World {
    pub fn column_loaded(&self, c: LightColumn) -> bool {
        (0..super::coords::WORLD_CHUNKS_Y)
            .all(|y| self.chunks.contains_key(&glam::IVec3::new(c.x, y, c.z)))
    }
    pub fn light_column_initialized(&self, c: LightColumn) -> bool {
        self.column_loaded(c)
            && (0..super::coords::WORLD_CHUNKS_Y)
                .all(|y| self.chunks[&glam::IVec3::new(c.x, y, c.z)].light_initialized())
    }
    pub fn column_light_initialized(&self, c: LightColumn) -> bool {
        self.light_column_initialized(c)
    }
    pub(crate) fn light_epoch(&self, c: LightColumn) -> u64 {
        self.light_epochs.get(&c).copied().unwrap_or(0)
    }
    pub fn take_light_dirty(&mut self) -> Vec<LightColumn> {
        let mut v: Vec<_> = self.light_dirty.drain().collect();
        v.sort_by_key(|c| (c.x, c.z));
        v
    }
    pub fn light_column_snapshot(&self, c: LightColumn, epoch: u64) -> Option<LightColumnSnapshot> {
        if !self.column_loaded(c) {
            return None;
        }
        let mut blocks = Box::new([super::block::AIR; LIGHT_COLUMN_CELLS]);
        for cy in 0..super::coords::WORLD_CHUNKS_Y as usize {
            let ch = &self.chunks[&glam::IVec3::new(c.x, cy as i32, c.z)];
            let offset = cy * super::chunk::CHUNK_VOLUME;
            blocks[offset..offset + super::chunk::CHUNK_VOLUME].copy_from_slice(&ch.blocks[..]);
        }
        let mut incoming =
            std::array::from_fn(|_| Box::new([pack_light(0, 15); LIGHT_BOUNDARY_CELLS]));
        for (f, n) in [
            (0, LightColumn { x: c.x - 1, z: c.z }),
            (1, LightColumn { x: c.x + 1, z: c.z }),
            (2, LightColumn { x: c.x, z: c.z - 1 }),
            (3, LightColumn { x: c.x, z: c.z + 1 }),
        ] {
            if !self.column_loaded(n) {
                continue;
            }
            if !self.light_column_initialized(n) {
                incoming[f] = self.provisional_boundary(n, f);
                continue;
            }
            for y in 0..256 {
                let ch = &self.chunks[&glam::IVec3::new(n.x, y / 32, n.z)];
                for u in 0..32 {
                    let l = glam::UVec3::new(
                        if f < 2 {
                            if f == 0 { 31 } else { 0 }
                        } else {
                            u
                        } as u32,
                        (y % 32) as u32,
                        if f < 2 {
                            u
                        } else {
                            if f == 2 { 31 } else { 0 }
                        } as u32,
                    );
                    incoming[f][u as usize + 32 * y as usize] = ch.get_light(l);
                }
            }
        }
        Some(LightColumnSnapshot {
            column: c,
            epoch,
            blocks,
            incoming,
        })
    }

    fn provisional_boundary(&self, c: LightColumn, face: usize) -> Box<[u8; LIGHT_BOUNDARY_CELLS]> {
        let mut out = Box::new([pack_light(0, 15); LIGHT_BOUNDARY_CELLS]);
        for u in 0..32 {
            let mut beam = 15;
            for y in (0..256).rev() {
                let cp = glam::IVec3::new(c.x, y / 32, c.z);
                let ch = &self.chunks[&cp];
                let l = glam::UVec3::new(
                    if face < 2 {
                        if face == 0 { 31 } else { 0 }
                    } else {
                        u
                    } as u32,
                    (y % 32) as u32,
                    if face < 2 {
                        u
                    } else {
                        if face == 2 { 31 } else { 0 }
                    } as u32,
                );
                let block = ch.get(l);
                let sky = if super::block::def(block).light_blocking {
                    beam = 0;
                    0
                } else {
                    beam
                };
                out[u as usize + 32 * y as usize] =
                    pack_light(super::block::def(block).emission, sky);
            }
        }
        out
    }
    pub fn apply_light_column(&mut self, r: LightColumnResult) -> LightApplyResult {
        if self.light_epoch(r.column) != r.epoch || !self.column_loaded(r.column) {
            return LightApplyResult::default();
        }
        let old = self.light_boundaries(r.column);
        let mut changed_chunks = Vec::new();
        for cy in 0..super::coords::WORLD_CHUNKS_Y {
            let cp = glam::IVec3::new(r.column.x, cy, r.column.z);
            let ch = self.chunks.get_mut(&cp).unwrap();
            let offset = cy as usize * super::chunk::CHUNK_VOLUME;
            let incoming = &r.light[offset..offset + super::chunk::CHUNK_VOLUME];
            let changed = ch.light[..] != incoming[..];
            if changed {
                ch.light.copy_from_slice(incoming);
            }
            ch.set_light_initialized(true);
            if changed {
                self.dirty.insert(cp);
                changed_chunks.push(cp);
            }
        }
        let mut cb = [false; 4];
        for f in 0..4 {
            cb[f] = old[f] != r.boundary[f];
            if cb[f] {
                let n = match f {
                    0 => LightColumn {
                        x: r.column.x - 1,
                        z: r.column.z,
                    },
                    1 => LightColumn {
                        x: r.column.x + 1,
                        z: r.column.z,
                    },
                    2 => LightColumn {
                        x: r.column.x,
                        z: r.column.z - 1,
                    },
                    _ => LightColumn {
                        x: r.column.x,
                        z: r.column.z + 1,
                    },
                };
                // A changed neighboring boundary is the next fixed-point wave,
                // not a content mutation. Keep the content epoch stable so
                // other results from the same wave remain applicable.
                self.mark_light_boundary_dirty(n);
            }
        }
        LightApplyResult {
            changed_chunks,
            changed_boundaries: cb,
        }
    }
    fn light_boundaries(&self, c: LightColumn) -> [Box<[u8; LIGHT_BOUNDARY_CELLS]>; 4] {
        std::array::from_fn(|f| {
            let mut b = Box::new([0; LIGHT_BOUNDARY_CELLS]);
            for y in 0..256 {
                let ch = &self.chunks[&glam::IVec3::new(c.x, y / 32, c.z)];
                for u in 0..32 {
                    let l = glam::UVec3::new(
                        if f < 2 {
                            if f == 0 { 0 } else { 31 }
                        } else {
                            u
                        } as u32,
                        (y % 32) as u32,
                        if f < 2 {
                            u
                        } else {
                            if f == 2 { 0 } else { 31 }
                        } as u32,
                    );
                    b[u as usize + 32 * y as usize] = ch.get_light(l);
                }
            }
            b
        })
    }
}

#[inline]
fn index(x: usize, y: usize, z: usize) -> usize {
    x + 32 * (z + 32 * y)
}
#[inline]
fn face_index(u: usize, y: usize) -> usize {
    u + 32 * y
}

pub fn solve_column(input: &LightColumnSnapshot) -> LightColumnResult {
    let mut light = Box::new([0_u8; LIGHT_COLUMN_CELLS]);
    let mut sky_queue = VecDeque::new();
    let mut block_queue = VecDeque::new();
    // Fixed neighbor order: -X,+X,-Z,+Z,-Y,+Y.
    const DIRS: [(isize, isize, isize); 6] = [
        (-1, 0, 0),
        (1, 0, 0),
        (0, 0, -1),
        (0, 0, 1),
        (0, -1, 0),
        (0, 1, 0),
    ];

    // Direct skylight beam, top down, with no attenuation downwards.
    for z in 0..32 {
        for x in 0..32 {
            let mut beam = 15_u8;
            for y in (0..256).rev() {
                let i = index(x, y, z);
                if def(input.blocks[i]).light_blocking {
                    beam = 0;
                } else if beam > sky_light(light[i]) {
                    light[i] = pack_light(block_light(light[i]), beam);
                }
            }
        }
    }

    // Seed incoming boundary light after crossing one horizontal edge.
    for y in 0..256 {
        for u in 0..32 {
            let fi = face_index(u, y);
            for (face, (x, z)) in [(0, (0, u)), (1, (31, u)), (2, (u, 0)), (3, (u, 31))] {
                let i = index(x, y, z);
                if def(input.blocks[i]).light_blocking {
                    continue;
                }
                let incoming = input.incoming[face][fi];
                let s = sky_light(incoming).saturating_sub(1);
                if s > sky_light(light[i]) {
                    light[i] = pack_light(block_light(light[i]), s);
                    sky_queue.push_back(i as u32);
                }
                let b = block_light(incoming).saturating_sub(1);
                if b > block_light(light[i]) {
                    light[i] = pack_light(b, sky_light(light[i]));
                    block_queue.push_back(i as u32);
                }
            }
        }
    }

    // Opaque emitters seed themselves before the block flood fill.
    for i in 0..LIGHT_COLUMN_CELLS {
        let e = def(input.blocks[i]).emission;
        if e > block_light(light[i]) {
            light[i] = pack_light(e, sky_light(light[i]));
            block_queue.push_back(i as u32);
        }
    }

    // Direct sunlight only enters the flood fill when it can improve a neighbor.
    for i in 0..LIGHT_COLUMN_CELLS {
        let current = sky_light(light[i]);
        if current == 0 {
            continue;
        }
        let x = i % 32;
        let z = (i / 32) % 32;
        let y = i / (32 * 32);
        if DIRS.iter().any(|&(dx, dy, dz)| {
            let (nx, ny, nz) = (x as isize + dx, y as isize + dy, z as isize + dz);
            if !(0..32).contains(&nx) || !(0..256).contains(&ny) || !(0..32).contains(&nz) {
                return false;
            }
            let ni = index(nx as usize, ny as usize, nz as usize);
            let candidate = if dy == -1 {
                current
            } else {
                current.saturating_sub(1)
            };
            !def(input.blocks[ni]).light_blocking && candidate > sky_light(light[ni])
        }) {
            sky_queue.push_back(i as u32);
        }
    }

    while let Some(raw) = sky_queue.pop_front() {
        let i = raw as usize;
        let x = i % 32;
        let z = (i / 32) % 32;
        let y = i / (32 * 32);
        let current = sky_light(light[i]);
        for (dx, dy, dz) in DIRS {
            let (nx, ny, nz) = (x as isize + dx, y as isize + dy, z as isize + dz);
            if !(0..32).contains(&nx) || !(0..256).contains(&ny) || !(0..32).contains(&nz) {
                continue;
            }
            let ni = index(nx as usize, ny as usize, nz as usize);
            if def(input.blocks[ni]).light_blocking {
                continue;
            }
            let candidate = if dy == -1 {
                current
            } else {
                current.saturating_sub(1)
            };
            if candidate > sky_light(light[ni]) {
                light[ni] = pack_light(block_light(light[ni]), candidate);
                sky_queue.push_back(ni as u32);
            }
        }
    }
    while let Some(raw) = block_queue.pop_front() {
        let i = raw as usize;
        let x = i % 32;
        let z = (i / 32) % 32;
        let y = i / (32 * 32);
        let current = block_light(light[i]);
        for (dx, dy, dz) in DIRS {
            let (nx, ny, nz) = (x as isize + dx, y as isize + dy, z as isize + dz);
            if !(0..32).contains(&nx) || !(0..256).contains(&ny) || !(0..32).contains(&nz) {
                continue;
            }
            let ni = index(nx as usize, ny as usize, nz as usize);
            if def(input.blocks[ni]).light_blocking {
                continue;
            }
            let candidate = current.saturating_sub(1);
            if candidate > block_light(light[ni]) {
                light[ni] = pack_light(candidate, sky_light(light[ni]));
                block_queue.push_back(ni as u32);
            }
        }
    }

    let mut boundary = std::array::from_fn(|_| Box::new([0_u8; LIGHT_BOUNDARY_CELLS]));
    for y in 0..256 {
        for u in 0..32 {
            boundary[0][face_index(u, y)] = light[index(0, y, u)];
            boundary[1][face_index(u, y)] = light[index(31, y, u)];
            boundary[2][face_index(u, y)] = light[index(u, y, 0)];
            boundary[3][face_index(u, y)] = light[index(u, y, 31)];
        }
    }
    LightColumnResult {
        column: input.column,
        epoch: input.epoch,
        light,
        boundary,
    }
}

#[cfg(test)]
mod tests;
