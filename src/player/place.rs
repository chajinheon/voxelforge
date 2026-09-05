//! Shape-aware placement rules. World mutation is deliberately kept here so
//! rendering and input layers can share the same deterministic decision.
use crate::world::{
    block,
    catalog::{self, PlacementKind},
    connect,
    raycast::RayHit,
    world::World,
};
use glam::Vec3;

pub fn place_item(
    world: &mut World,
    hit: RayHit,
    item_id: catalog::ItemId,
    camera_forward: Vec3,
) -> Option<block::BlockId> {
    let placement = catalog::item(item_id)?.placement;
    let target = hit.block + hit.normal;
    let id = match placement {
        PlacementKind::Block(id) => id,
        PlacementKind::AxisLog { x, y, z } => match hit.normal.abs() {
            v if v.x == 1 => x,
            v if v.y == 1 => y,
            _ => z,
        },
        PlacementKind::Slab { material, full } => {
            let bottom = block::SLAB_BASE + material as u16 * 2;
            let top = bottom + 1;
            if hit.block_id == bottom && (hit.normal.y == 1 || hit.local_hit.y > 0.5) {
                world.set_block(hit.block, full);
                return Some(full);
            }
            if hit.block_id == top && (hit.normal.y == -1 || hit.local_hit.y < 0.5) {
                world.set_block(hit.block, full);
                return Some(full);
            }
            if hit.normal.y == -1 || (hit.normal.y == 0 && hit.local_hit.y > 0.5) {
                top
            } else {
                bottom
            }
        }
        PlacementKind::Stair { material } => {
            let axis = if camera_forward.x.abs() >= camera_forward.z.abs() {
                if camera_forward.x >= 0.0 { 3 } else { 1 }
            } else if camera_forward.z >= 0.0 {
                0
            } else {
                2
            };
            let upside = hit.normal.y == -1 || (hit.normal.y == 0 && hit.local_hit.y > 0.5);
            block::STAIR_BASE + material as u16 * 8 + axis + if upside { 4 } else { 0 }
        }
        PlacementKind::Pane { material } => block::PANE_BASE + material as u16 * 16,
        PlacementKind::Fence { material } => block::FENCE_BASE + material as u16 * 16,
    };
    if matches!(
        placement,
        PlacementKind::Pane { .. } | PlacementKind::Fence { .. }
    ) {
        return connect::place_with_connections(world, target, id).then_some(id);
    }
    if world.get_block(target) == block::AIR && world.set_block(target, id) {
        Some(id)
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditAction {
    Place,
    Break,
}
#[derive(Clone, Copy, Debug)]
pub struct HoldRepeat {
    lmb: f32,
    rmb: f32,
    was_lmb: bool,
    was_rmb: bool,
}
impl Default for HoldRepeat {
    fn default() -> Self {
        Self {
            lmb: 0.0,
            rmb: 0.0,
            was_lmb: false,
            was_rmb: false,
        }
    }
}
impl HoldRepeat {
    pub fn tick(&mut self, dt: f32, lmb: bool, rmb: bool) -> Option<EditAction> {
        let dt = dt.max(0.0);
        let lpress = lmb && !self.was_lmb;
        let rpress = rmb && !self.was_rmb;
        self.lmb = if lmb { self.lmb + dt } else { 0.0 };
        self.rmb = if rmb { self.rmb + dt } else { 0.0 };
        self.was_lmb = lmb;
        self.was_rmb = rmb;
        if rmb && (rpress || self.rmb >= 0.25) {
            if !rpress {
                self.rmb -= 0.12;
            }
            return Some(EditAction::Place);
        }
        if lmb && (lpress || self.lmb >= 0.22) {
            if !lpress {
                self.lmb -= 0.10;
            }
            return Some(EditAction::Break);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::{self, STONE};
    use crate::world::raycast::RayHit;
    use crate::world::world::World;
    use glam::{IVec3, Vec3};

    fn loaded_world() -> World {
        let mut world = World::new(0);
        world.ensure_loaded(IVec3::new(0, 6, 0));
        world
    }

    fn support_hit(world: &mut World, at: IVec3, normal: IVec3) -> RayHit {
        assert!(world.set_block(at, STONE));
        RayHit {
            block: at,
            normal,
            distance: 1.0,
            local_hit: Vec3::splat(0.5),
            block_id: STONE,
        }
    }

    #[test]
    fn axis_log_uses_clicked_axis() {
        for (normal, expected) in [
            (IVec3::X, block::OAK_LOG_X),
            (IVec3::Y, block::OAK_LOG_Y),
            (IVec3::Z, block::OAK_LOG_Z),
        ] {
            let mut world = loaded_world();
            let hit = support_hit(&mut world, IVec3::new(2, 200, 2), normal);
            assert_eq!(place_item(&mut world, hit, 31, Vec3::Z), Some(expected));
            assert_eq!(world.get_block(hit.block + normal), expected);
        }
    }

    #[test]
    fn slab_pair_merges_to_full_block() {
        let mut world = loaded_world();
        let at = IVec3::new(2, 200, 2);
        let bottom = block::SLAB_BASE;
        assert!(world.set_block(at, bottom));
        let hit = RayHit {
            block: at,
            normal: IVec3::Y,
            distance: 1.0,
            local_hit: Vec3::new(0.5, 1.0, 0.5),
            block_id: bottom,
        };

        assert_eq!(place_item(&mut world, hit, 89, Vec3::Z), Some(STONE));
        assert_eq!(world.get_block(at), STONE);
    }

    #[test]
    fn stair_facing_is_opposite_camera_forward() {
        let mut world = loaded_world();
        let hit = support_hit(&mut world, IVec3::new(2, 200, 2), IVec3::Y);
        assert_eq!(
            place_item(&mut world, hit, 101, Vec3::X),
            Some(block::STAIR_BASE + 3)
        );
    }

    #[test]
    fn connected_placement_increments_chunk_version_once() {
        let mut world = loaded_world();
        let hit = support_hit(&mut world, IVec3::new(2, 200, 2), IVec3::X);
        let before = world
            .chunk_version(IVec3::new(0, 6, 0))
            .expect("loaded version");
        assert!(place_item(&mut world, hit, 113, Vec3::Z).is_some());
        assert_eq!(world.chunk_version(IVec3::new(0, 6, 0)), Some(before + 1));
    }
}
