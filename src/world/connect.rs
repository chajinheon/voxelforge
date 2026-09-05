//! Deterministic pane/fence connection state updates.
use super::block::{self, BlockId};
use super::coords::{CHUNK_SIZE, chunk_of, local_of};
use super::light::LightColumn;
use super::world::World;
use glam::IVec3;
use std::collections::HashMap;

fn pane(id: BlockId) -> bool {
    (block::PANE_BASE..=863).contains(&id)
}
fn fence(id: BlockId) -> bool {
    (block::FENCE_BASE..=959).contains(&id)
}
fn connects(id: BlockId, family: bool) -> bool {
    if family {
        fence(id) || (block::def(id).opaque && !(block::SLAB_BASE..=959).contains(&id))
    } else {
        pane(id) || (block::def(id).opaque && !(block::SLAB_BASE..=959).contains(&id))
    }
}

pub fn connection_mask(world: &World, at: IVec3, family_fence: bool) -> u8 {
    connection_mask_override(world, at, family_fence, None)
}

fn connection_mask_override(
    world: &World,
    at: IVec3,
    family_fence: bool,
    override_cell: Option<(IVec3, BlockId)>,
) -> u8 {
    let dirs = [
        IVec3::new(0, 0, -1),
        IVec3::new(1, 0, 0),
        IVec3::new(0, 0, 1),
        IVec3::new(-1, 0, 0),
    ];
    dirs.iter().enumerate().fold(0, |mask, (i, d)| {
        let neighbor = at + *d;
        let id = override_cell
            .filter(|(position, _)| *position == neighbor)
            .map_or_else(|| world.get_block(neighbor), |(_, id)| id);
        mask | ((connects(id, family_fence) as u8) << i)
    })
}

pub fn update_connections(world: &mut World, center: IVec3) -> usize {
    let cells = [
        center,
        center + IVec3::new(0, 0, -1),
        center + IVec3::new(1, 0, 0),
        center + IVec3::new(0, 0, 1),
        center + IVec3::new(-1, 0, 0),
    ];
    let mut edits = Vec::new();
    for at in cells {
        let old = world.get_block(at);
        let (base, family) = if pane(old) {
            (block::PANE_BASE, false)
        } else if fence(old) {
            (block::FENCE_BASE, true)
        } else {
            continue;
        };
        let material = (old - base) / 16;
        let next = base + material * 16 + connection_mask(world, at, family) as u16;
        if next != old {
            edits.push((at, old, next));
        }
    }

    // A single user edit may rewrite several state cells. Apply one normal
    // World::set_block per chunk, then write the remaining state changes
    // without incrementing that chunk's content version again. This keeps
    // save/version semantics transactional while retaining dirty/modified
    // bookkeeping for every changed cell.
    apply_edits(world, edits)
}

/// Place a pane or fence and update the five affected cells as one edit.
/// Keeping the new cell as a virtual override while calculating masks avoids
/// charging the same user action two content-version increments.
pub fn place_with_connections(world: &mut World, at: IVec3, id: BlockId) -> bool {
    let family_fence = if fence(id) {
        true
    } else if pane(id) {
        false
    } else {
        return world.set_block(at, id);
    };
    if world.get_block(at) != block::AIR {
        return false;
    }

    let cells = [
        at,
        at + IVec3::new(0, 0, -1),
        at + IVec3::new(1, 0, 0),
        at + IVec3::new(0, 0, 1),
        at + IVec3::new(-1, 0, 0),
    ];
    let mut edits = Vec::new();
    for cell in cells {
        let old = world.get_block(cell);
        let next = if cell == at {
            id
        } else if (family_fence && fence(old)) || (!family_fence && pane(old)) {
            let base = if family_fence {
                block::FENCE_BASE
            } else {
                block::PANE_BASE
            };
            let material = (old - base) / 16;
            base + material * 16
                + connection_mask_override(world, cell, family_fence, Some((at, id))) as u16
        } else {
            continue;
        };
        if next != old {
            edits.push((cell, old, next));
        }
    }
    apply_edits(world, edits) != 0
}

fn apply_edits(world: &mut World, edits: Vec<(IVec3, BlockId, BlockId)>) -> usize {
    let mut by_chunk: HashMap<IVec3, Vec<(IVec3, BlockId, BlockId)>> = HashMap::new();
    for edit in edits {
        by_chunk.entry(chunk_of(edit.0)).or_default().push(edit);
    }
    let mut changed = 0;
    for (cp, chunk_edits) in by_chunk {
        let Some(first) = chunk_edits.first().copied() else {
            continue;
        };
        if !world.set_block(first.0, first.2) {
            continue;
        }
        changed += 1;
        for (at, old, next) in chunk_edits.into_iter().skip(1) {
            let local = local_of(at);
            let Some(chunk) = world.chunks.get_mut(&cp) else {
                continue;
            };
            if chunk.get(local) != old {
                continue;
            }
            chunk.set(local, next);
            world.dirty.insert(cp);
            world.modified.insert(cp);
            world.mark_light_dirty(LightColumn { x: cp.x, z: cp.z });
            mark_border_neighbor_dirty(world, cp, local);
            changed += 1;
        }
    }
    changed
}

fn mark_border_neighbor_dirty(world: &mut World, cp: IVec3, local: glam::UVec3) {
    let mut neighbors = Vec::new();
    if local.x == 0 {
        neighbors.push(cp + IVec3::new(-1, 0, 0));
    }
    if local.x == (CHUNK_SIZE - 1) as u32 {
        neighbors.push(cp + IVec3::new(1, 0, 0));
    }
    if local.y == 0 {
        neighbors.push(cp + IVec3::new(0, -1, 0));
    }
    if local.y == (CHUNK_SIZE - 1) as u32 {
        neighbors.push(cp + IVec3::new(0, 1, 0));
    }
    if local.z == 0 {
        neighbors.push(cp + IVec3::new(0, 0, -1));
    }
    if local.z == (CHUNK_SIZE - 1) as u32 {
        neighbors.push(cp + IVec3::new(0, 0, 1));
    }
    for neighbor in neighbors {
        if world.chunks.contains_key(&neighbor) {
            world.dirty.insert(neighbor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::{FENCE_BASE, PANE_BASE, STONE};

    fn loaded_world() -> World {
        let mut world = World::new(0);
        world.ensure_loaded(IVec3::new(0, 6, 0));
        world
    }

    #[test]
    fn connection_masks_use_four_cardinal_cells() {
        let mut w = loaded_world();
        let p = IVec3::new(2, 200, 2);
        w.set_block(p, PANE_BASE);
        w.set_block(p + IVec3::new(0, 0, -1), STONE);
        assert_eq!(connection_mask(&w, p, false), 1);
    }

    #[test]
    fn pane_connections_update_five_cells() {
        let mut world = loaded_world();
        let center = IVec3::new(2, 200, 2);
        let neighbors = [
            center + IVec3::new(0, 0, -1),
            center + IVec3::new(1, 0, 0),
            center + IVec3::new(0, 0, 1),
            center + IVec3::new(-1, 0, 0),
        ];
        assert!(world.set_block(center, PANE_BASE));
        for at in neighbors {
            assert!(world.set_block(at, PANE_BASE));
        }

        assert_eq!(update_connections(&mut world, center), 5);
        assert_eq!(world.get_block(center), PANE_BASE + 15);
        assert_eq!(world.get_block(neighbors[0]), PANE_BASE + 4);
        assert_eq!(world.get_block(neighbors[1]), PANE_BASE + 8);
        assert_eq!(world.get_block(neighbors[2]), PANE_BASE + 1);
        assert_eq!(world.get_block(neighbors[3]), PANE_BASE + 2);
    }

    #[test]
    fn fence_connections_update_five_cells() {
        let mut world = loaded_world();
        let center = IVec3::new(2, 200, 2);
        let neighbors = [
            center + IVec3::new(0, 0, -1),
            center + IVec3::new(1, 0, 0),
            center + IVec3::new(0, 0, 1),
            center + IVec3::new(-1, 0, 0),
        ];
        assert!(world.set_block(center, FENCE_BASE));
        for at in neighbors {
            assert!(world.set_block(at, FENCE_BASE));
        }

        assert_eq!(update_connections(&mut world, center), 5);
        assert_eq!(world.get_block(center), FENCE_BASE + 15);
        assert_eq!(world.get_block(neighbors[0]), FENCE_BASE + 4);
        assert_eq!(world.get_block(neighbors[1]), FENCE_BASE + 8);
        assert_eq!(world.get_block(neighbors[2]), FENCE_BASE + 1);
        assert_eq!(world.get_block(neighbors[3]), FENCE_BASE + 2);
    }

    #[test]
    fn connection_update_increments_chunk_version_once() {
        let mut world = loaded_world();
        let center = IVec3::new(2, 200, 2);
        for at in [
            center,
            center + IVec3::new(0, 0, -1),
            center + IVec3::new(1, 0, 0),
            center + IVec3::new(0, 0, 1),
            center + IVec3::new(-1, 0, 0),
        ] {
            assert!(world.set_block(at, PANE_BASE));
        }
        let cp = glam::IVec3::new(0, 6, 0);
        let before = world.chunk_version(cp).expect("loaded chunk version");
        assert_eq!(update_connections(&mut world, center), 5);
        assert_eq!(world.chunk_version(cp), Some(before + 1));
    }
}
