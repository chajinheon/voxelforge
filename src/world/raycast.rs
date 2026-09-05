use super::block::{self, BlockId, def};
use super::shape::{Axis, Facing, ShapeKind, shape_template};
use super::world::World;
use glam::{IVec3, Vec3};

/// A solid block intersected by a ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub block: IVec3,
    pub normal: IVec3,
    pub distance: f32,
    pub local_hit: Vec3,
    pub block_id: BlockId,
}

pub fn shape_kind(id: BlockId) -> ShapeKind {
    match id {
        block::OAK_LOG_X | block::BIRCH_LOG_X | block::SPRUCE_LOG_X | block::DARK_OAK_LOG_X => {
            ShapeKind::Log { axis: Axis::X }
        }
        block::OAK_LOG_Z | block::BIRCH_LOG_Z | block::SPRUCE_LOG_Z | block::DARK_OAK_LOG_Z => {
            ShapeKind::Log { axis: Axis::Z }
        }
        block::SLAB_BASE..=279 => ShapeKind::Slab {
            top: !id.is_multiple_of(2),
        },
        block::STAIR_BASE..=607 => {
            let s = (id - block::STAIR_BASE) % 8;
            ShapeKind::Stair {
                upside_down: s >= 4,
                facing: [Facing::North, Facing::East, Facing::South, Facing::West]
                    [(s % 4) as usize],
            }
        }
        block::PANE_BASE..=863 => ShapeKind::Pane {
            connections: ((id - block::PANE_BASE) & 15) as u8,
        },
        block::FENCE_BASE..=959 => ShapeKind::Fence {
            connections: ((id - block::FENCE_BASE) & 15) as u8,
        },
        _ => ShapeKind::Cube,
    }
}

/// Traverse the voxel grid with an Amanatides--Woo DDA.
pub fn raycast(world: &World, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<RayHit> {
    if !dir.is_finite() || dir.length_squared() == 0.0 || !max_dist.is_finite() || max_dist < 0.0 {
        return None;
    }

    let direction = dir.normalize();
    let mut block = origin.floor().as_ivec3();

    // The ray cannot select the block it starts inside.
    if def(world.get_block(block)).solid {
        return None;
    }

    let step = IVec3::new(
        if direction.x > 0.0 {
            1
        } else if direction.x < 0.0 {
            -1
        } else {
            0
        },
        if direction.y > 0.0 {
            1
        } else if direction.y < 0.0 {
            -1
        } else {
            0
        },
        if direction.z > 0.0 {
            1
        } else if direction.z < 0.0 {
            -1
        } else {
            0
        },
    );
    let delta = Vec3::new(
        if step.x == 0 {
            f32::INFINITY
        } else {
            1.0 / direction.x.abs()
        },
        if step.y == 0 {
            f32::INFINITY
        } else {
            1.0 / direction.y.abs()
        },
        if step.z == 0 {
            f32::INFINITY
        } else {
            1.0 / direction.z.abs()
        },
    );
    let cell_min = block.as_vec3();
    let next_boundary = Vec3::new(
        if step.x > 0 {
            cell_min.x + 1.0
        } else {
            cell_min.x
        },
        if step.y > 0 {
            cell_min.y + 1.0
        } else {
            cell_min.y
        },
        if step.z > 0 {
            cell_min.z + 1.0
        } else {
            cell_min.z
        },
    );
    let mut t_max = Vec3::new(
        if step.x == 0 {
            f32::INFINITY
        } else {
            (next_boundary.x - origin.x) / direction.x
        },
        if step.y == 0 {
            f32::INFINITY
        } else {
            (next_boundary.y - origin.y) / direction.y
        },
        if step.z == 0 {
            f32::INFINITY
        } else {
            (next_boundary.z - origin.z) / direction.z
        },
    );

    let mut cell_entry = 0.0;
    loop {
        let (axis, distance) = if t_max.x <= t_max.y && t_max.x <= t_max.z {
            (0, t_max.x)
        } else if t_max.y <= t_max.z {
            (1, t_max.y)
        } else {
            (2, t_max.z)
        };
        if distance > max_dist {
            return None;
        }

        match axis {
            0 => {
                block.x += step.x;
                t_max.x += delta.x;
            }
            1 => {
                block.y += step.y;
                t_max.y += delta.y;
            }
            _ => {
                block.z += step.z;
                t_max.z += delta.z;
            }
        }

        let id = world.get_block(block);
        if id != block::AIR && id != block::WATER {
            let template = shape_template(shape_kind(id));
            let mut best: Option<(f32, Vec3, IVec3)> = None;
            for a in template
                .collision
                .boxes
                .iter()
                .take(template.collision.len as usize)
            {
                let min = block.as_vec3()
                    + Vec3::new(a.min[0] as f32, a.min[1] as f32, a.min[2] as f32) / 16.0;
                let max = block.as_vec3()
                    + Vec3::new(a.max[0] as f32, a.max[1] as f32, a.max[2] as f32) / 16.0;
                if let Some(hit) = ray_aabb(
                    origin,
                    direction,
                    min,
                    max,
                    cell_entry,
                    distance.min(max_dist),
                ) && best.as_ref().is_none_or(|best_hit| hit.0 < best_hit.0)
                {
                    best = Some(hit);
                }
            }
            if let Some((distance, point, hit_normal)) = best {
                return Some(RayHit {
                    block,
                    normal: hit_normal,
                    distance,
                    local_hit: point - block.as_vec3(),
                    block_id: id,
                });
            }
        }
        cell_entry = distance;
    }
}

fn ray_aabb(
    o: Vec3,
    d: Vec3,
    min: Vec3,
    max: Vec3,
    mut near: f32,
    mut far: f32,
) -> Option<(f32, Vec3, IVec3)> {
    let mut axis = 0;
    for i in 0..3 {
        if d[i].abs() < f32::EPSILON {
            if o[i] < min[i] || o[i] > max[i] {
                return None;
            }
        } else {
            let (mut a, mut b) = ((min[i] - o[i]) / d[i], (max[i] - o[i]) / d[i]);
            if a > b {
                std::mem::swap(&mut a, &mut b);
            }
            if a > near {
                near = a;
                axis = i;
            }
            far = far.min(b);
            if near > far {
                return None;
            }
        }
    }
    if near < 0.0 {
        return None;
    }
    let mut n = IVec3::ZERO;
    n[axis] = if d[axis] > 0.0 { -1 } else { 1 };
    Some((near, o + d * near, n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::{AIR, PANE_BASE, STAIR_BASE, STONE, WATER};

    fn loaded_world() -> World {
        let mut world = World::new(0);
        world.ensure_loaded(IVec3::new(0, 6, 0));
        world
    }

    #[test]
    fn raycast_hits_block_in_front() {
        let mut world = loaded_world();
        let target = IVec3::new(3, 200, 0);
        assert!(world.set_block(target, STONE));

        let hit = raycast(&world, Vec3::new(0.5, 200.5, 0.5), Vec3::X, 6.0).unwrap();
        assert_eq!(hit.block, target);
        assert!((hit.distance - 2.5).abs() < f32::EPSILON);
    }

    #[test]
    fn raycast_face_normal_is_toward_origin() {
        let mut world = loaded_world();
        let target = IVec3::new(3, 200, 0);
        assert!(world.set_block(target, STONE));

        let hit = raycast(&world, Vec3::new(0.5, 200.5, 0.5), Vec3::X, 6.0).unwrap();
        assert_eq!(hit.normal, IVec3::new(-1, 0, 0));
    }

    #[test]
    fn raycast_misses_when_only_air() {
        let world = loaded_world();
        assert_eq!(world.get_block(IVec3::new(3, 200, 0)), AIR);
        assert!(raycast(&world, Vec3::new(0.5, 200.5, 0.5), Vec3::X, 6.0).is_none());
    }

    #[test]
    fn raycast_passes_through_water() {
        let mut world = loaded_world();
        assert!(world.set_block(IVec3::new(1, 200, 0), WATER));
        let target = IVec3::new(3, 200, 0);
        assert!(world.set_block(target, STONE));

        let hit = raycast(&world, Vec3::new(0.5, 200.5, 0.5), Vec3::X, 6.0).unwrap();
        assert_eq!(hit.block, target);
    }

    #[test]
    fn raycast_passes_through_empty_pane_region() {
        let mut world = loaded_world();
        let pane = IVec3::new(1, 200, 0);
        let target = IVec3::new(3, 200, 0);
        assert!(world.set_block(pane, PANE_BASE));
        assert!(world.set_block(target, STONE));

        // The ray is outside the pane's 2/16-wide central column, so the
        // empty region must not consume the hit before the solid behind it.
        let hit = raycast(&world, Vec3::new(0.5, 200.5, 0.1), Vec3::X, 6.0)
            .expect("solid behind empty pane region");
        assert_eq!(hit.block, target);
    }

    #[test]
    fn raycast_hits_stair_step() {
        let mut world = loaded_world();
        let stair = IVec3::new(2, 200, 0);
        assert!(world.set_block(stair, STAIR_BASE));

        let hit =
            raycast(&world, Vec3::new(0.5, 200.75, 0.5), Vec3::X, 5.0).expect("upper stair step");
        assert_eq!(hit.block, stair);
        assert!(hit.local_hit.y >= 0.5);
    }
}
