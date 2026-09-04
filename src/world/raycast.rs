use super::block::def;
use super::world::World;
use glam::{IVec3, Vec3};

/// A solid block intersected by a ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub block: IVec3,
    pub normal: IVec3,
    pub distance: f32,
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

        let normal = match axis {
            0 => IVec3::new(-step.x, 0, 0),
            1 => IVec3::new(0, -step.y, 0),
            _ => IVec3::new(0, 0, -step.z),
        };
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

        if def(world.get_block(block)).solid {
            return Some(RayHit {
                block,
                normal,
                distance,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::{AIR, STONE, WATER};

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
}
