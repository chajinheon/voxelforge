//! Deterministic, purely functional tree placement.
//!
//! A tree is anchored by the surface column `(x, z)`.  The generator is
//! intentionally independent of chunks: callers may ask about the two
//! columns on either side of a chunk border and receive the same tree.

use super::block::{BlockId, LEAVES, LOG};
use glam::IVec3;

/// Salt reserved for tree placement and shape hashing.
pub const TREE_SALT: u64 = 0x5452_4545_5f56_4631;

/// A tree's anchor and generated dimensions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Tree {
    pub x: i32,
    pub z: i32,
    /// Surface height at the anchor column.
    pub h: i32,
    pub trunk_h: i32,
    /// First block of the trunk (`h + 1`).
    pub base_y: i32,
}

/// Stable integer-to-unit hash used by world generation.
///
/// The wrapping arithmetic is deliberate: it defines the result for all
/// signed world coordinates without relying on a platform hash implementation.
pub fn hash01(seed: u64, x: i32, z: i32, salt: u64) -> f32 {
    let mut v = seed ^ salt;
    v = v.wrapping_add((x as i64 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    v = v.wrapping_add((z as i64 as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9));
    // SplitMix64 finalizer.
    v = (v ^ (v >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    v = (v ^ (v >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    v ^= v >> 31;
    ((v >> 40) as f32) / ((1_u32 << 24) as f32)
}

/// Return the tree anchored at a surface column, if this column receives one.
pub fn tree_at(seed: u64, x: i32, z: i32, h: i32) -> Option<Tree> {
    if h <= super::r#gen::SEA_LEVEL + 1 || hash01(seed, x, z, TREE_SALT) >= 0.004 {
        return None;
    }

    let shape = hash01(seed, x, z, TREE_SALT.wrapping_add(1));
    let trunk_h = 4 + (shape * 3.0).floor() as i32;
    Some(Tree {
        x,
        z,
        h,
        trunk_h: trunk_h.clamp(4, 6),
        base_y: h + 1,
    })
}

/// Return all blocks belonging to `tree` as `(world_position, block_id)` pairs.
///
/// Leaves are emitted first and logs last, making the LOG-over-LEAVES rule
/// explicit for callers that fold this list into a chunk.
pub fn tree_blocks(tree: &Tree) -> Vec<(IVec3, BlockId)> {
    let mut blocks = Vec::with_capacity(tree.trunk_h as usize + 56);
    let leaf_layers = [tree.trunk_h - 2, tree.trunk_h - 1];
    for &dy in &leaf_layers {
        for dx in -2_i32..=2 {
            for dz in -2_i32..=2 {
                if dx.abs() == 2 && dz.abs() == 2 {
                    continue;
                }
                blocks.push((
                    IVec3::new(tree.x + dx, tree.base_y + dy, tree.z + dz),
                    LEAVES,
                ));
            }
        }
    }
    for dx in -1..=1 {
        for dz in -1..=1 {
            blocks.push((
                IVec3::new(tree.x + dx, tree.base_y + tree.trunk_h, tree.z + dz),
                LEAVES,
            ));
        }
    }
    for (dx, dz) in [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)] {
        blocks.push((
            IVec3::new(tree.x + dx, tree.base_y + tree.trunk_h + 1, tree.z + dz),
            LEAVES,
        ));
    }
    for i in 0..tree.trunk_h {
        blocks.push((IVec3::new(tree.x, tree.base_y + i, tree.z), LOG));
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_for_negative_coordinates() {
        let negative = hash01(7, -12, 31, TREE_SALT);
        assert!((0.0..1.0).contains(&negative));
        assert_ne!(negative, hash01(7, 12, 31, TREE_SALT));
    }

    #[test]
    fn tree_shape_has_expected_layers_and_log_precedence() {
        let tree = Tree {
            x: 3,
            z: -4,
            h: 70,
            trunk_h: 5,
            base_y: 71,
        };
        let blocks = tree_blocks(&tree);
        assert_eq!(blocks.len(), 5 + 21 * 2 + 9 + 5);
        let top_log = IVec3::new(3, 71 + 4, -4);
        let last = blocks.iter().rev().find(|(p, _)| *p == top_log);
        assert_eq!(last.map(|(_, id)| *id), Some(LOG));
    }

    #[test]
    fn placement_requires_high_land_and_is_rare() {
        assert!(tree_at(1, 0, 0, 62).is_none());
        let placed = (0..200_000)
            .find_map(|x| tree_at(99, x, 0, 80))
            .expect("a deterministic sample should contain a tree");
        assert!((4..=6).contains(&placed.trunk_h));
    }
}
