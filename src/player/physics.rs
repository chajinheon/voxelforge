//! A small, deterministic swept-AABB controller for the player body.

use glam::{IVec3, Vec3};
use std::sync::OnceLock;

use crate::world::block::{BlockId, WATER, def};
use crate::world::coords::chunk_of;
use crate::world::r#gen::{SEA_LEVEL, WorldGen};
use crate::world::raycast::shape_kind;
use crate::world::shape::{Aabb, ShapeKind, SmallAabbList, collision_boxes, shape_mask};
use crate::world::world::World;

use super::camera::{PLAYER_H, PLAYER_HALF_W};

pub const GRAVITY: f32 = -28.0;
pub const TERMINAL_VELOCITY: f32 = -78.0;
pub const JUMP_SPEED: f32 = 8.5;
pub const WALK_SPEED: f32 = 4.3;
pub const SPRINT_SPEED: f32 = 5.6;
pub const FLY_SPEED: f32 = 12.0;
const COLLISION_EPSILON: f32 = 1.0e-4;
const SUBSTEP: f32 = 1.0 / 120.0;

const fn full_collision() -> SmallAabbList {
    let mut collision = SmallAabbList::EMPTY;
    collision.boxes[0] = Aabb {
        min: [0, 0, 0],
        max: [16, 16, 16],
    };
    collision.len = 1;
    collision
}

const FULL_COLLISION: SmallAabbList = full_collision();

// Shape state IDs are stable and bounded by the block registry. Build their
// collision boxes once, so a moving player does not rebuild 1/16-block masks on
// every substep. Cube IDs use the same single box as the original controller.
static SHAPED_COLLISION: OnceLock<[SmallAabbList; 960]> = OnceLock::new();

fn collision_boxes_for(id: BlockId) -> &'static SmallAabbList {
    if matches!(shape_kind(id), ShapeKind::Cube) {
        return &FULL_COLLISION;
    }
    let cache = SHAPED_COLLISION.get_or_init(|| {
        std::array::from_fn(|raw| {
            let state = raw as BlockId;
            let kind = shape_kind(state);
            if matches!(kind, ShapeKind::Cube) {
                FULL_COLLISION
            } else {
                collision_boxes(kind, &shape_mask(kind))
            }
        })
    });
    cache.get(id as usize).unwrap_or(&FULL_COLLISION)
}

/// Player position is the centre of the feet (not the eye position).
#[derive(Clone, Copy, Debug, Default)]
pub struct Body {
    pub pos: Vec3,
    pub vel: Vec3,
    pub on_ground: bool,
    pub fly: bool,
    pub in_water: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MoveInput {
    /// Horizontal wish direction in world space, normally already normalized.
    pub wish: Vec3,
    pub jump: bool,
    pub sprint: bool,
    pub up: bool,
    pub down: bool,
}

/// Pick dry land near the origin and return a feet-centred spawn position.
pub fn safe_spawn(generator: &WorldGen) -> Vec3 {
    for radius in 0_i32..=64 {
        for z in -radius..=radius {
            for x in -radius..=radius {
                if x.abs().max(z.abs()) != radius {
                    continue;
                }
                let height = generator.height_at(x, z);
                if height > SEA_LEVEL {
                    return Vec3::new(x as f32 + 0.5, height as f32 + 1.0, z as f32 + 0.5);
                }
            }
        }
    }
    let height = generator.height_at(0, 0);
    Vec3::new(0.5, height as f32 + 1.0, 0.5)
}

/// Sweep one axis of the player's AABB against solid blocks.
///
/// `min` and `max` are the current (unswept) AABB bounds. The returned
/// distance is the largest collision-free part of `delta`.
pub fn sweep_axis(world: &World, min: Vec3, max: Vec3, delta: f32, axis: usize) -> (f32, bool) {
    assert!(axis < 3, "axis must be 0, 1, or 2");
    if delta == 0.0 {
        return (0.0, false);
    }

    let swept_min = min[axis].min(min[axis] + delta);
    let swept_max = max[axis].max(max[axis] + delta);
    let mut best = delta;
    let mut hit = false;

    let range =
        |lo: f32, hi: f32| ((lo.floor() as i32 - 1)..=(hi.floor() as i32 + 1)).collect::<Vec<_>>();
    let ranges = [
        range(swept_min, swept_max),
        range(min[(axis + 1) % 3], max[(axis + 1) % 3]),
        range(min[(axis + 2) % 3], max[(axis + 2) % 3]),
    ];

    for a in &ranges[0] {
        for b in &ranges[1] {
            for c in &ranges[2] {
                let mut p = [0i32; 3];
                p[axis] = *a;
                p[(axis + 1) % 3] = *b;
                p[(axis + 2) % 3] = *c;
                let bp = IVec3::new(p[0], p[1], p[2]);
                let id: BlockId = world.get_block(bp);
                if !def(id).solid {
                    continue;
                }

                let other0 = (axis + 1) % 3;
                let other1 = (axis + 2) % 3;
                let collision = collision_boxes_for(id);
                for shape_box in collision.boxes.iter().take(collision.len as usize) {
                    let box_min = bp.as_vec3()
                        + Vec3::new(
                            shape_box.min[0] as f32,
                            shape_box.min[1] as f32,
                            shape_box.min[2] as f32,
                        ) / 16.0;
                    let box_max = bp.as_vec3()
                        + Vec3::new(
                            shape_box.max[0] as f32,
                            shape_box.max[1] as f32,
                            shape_box.max[2] as f32,
                        ) / 16.0;
                    if max[other0] <= box_min[other0] + COLLISION_EPSILON
                        || min[other0] >= box_max[other0] - COLLISION_EPSILON
                        || max[other1] <= box_min[other1] + COLLISION_EPSILON
                        || min[other1] >= box_max[other1] - COLLISION_EPSILON
                    {
                        continue;
                    }

                    let block_min = box_min[axis];
                    let block_max = box_max[axis];
                    if delta > 0.0 {
                        if max[axis] <= block_min + COLLISION_EPSILON
                            && max[axis] + delta >= block_min
                        {
                            let allowed = block_min - max[axis];
                            if allowed < best {
                                best = allowed.max(0.0);
                                hit = true;
                            }
                        }
                    } else if min[axis] >= block_max - COLLISION_EPSILON
                        && min[axis] + delta <= block_max
                    {
                        let allowed = block_max - min[axis];
                        if allowed > best {
                            best = allowed.min(0.0);
                            hit = true;
                        }
                    }
                }
            }
        }
    }
    (best, hit)
}

/// Advance a body using fixed-size substeps and axis-separated collision sweeps.
pub fn step(world: &World, body: &mut Body, input: MoveInput, dt: f32) {
    let player_chunk = chunk_of(body.pos.floor().as_ivec3());
    if world.chunk(player_chunk).is_none() {
        return;
    }
    let dt = dt.max(0.0);
    if dt == 0.0 {
        return;
    }
    let steps = ((dt / SUBSTEP).ceil() as usize).max(1);
    let h = dt / steps as f32;
    body.in_water = is_in_water(world, body.pos);
    let can_jump = body.on_ground;
    body.on_ground = false;

    for substep in 0..steps {
        if body.fly {
            let vertical = input.up as i8 as f32 - input.down as i8 as f32;
            let direction = Vec3::new(input.wish.x, vertical, input.wish.z).normalize_or_zero();
            body.vel = direction * FLY_SPEED * if input.sprint { 3.0 } else { 1.0 };
            body.on_ground = false;
        } else if body.in_water {
            if input.up {
                body.vel.y = 4.0;
            } else {
                body.vel.y += GRAVITY * 0.4 * h;
            }
            body.vel.y = body.vel.y.clamp(-4.0, 4.0);
        } else {
            if input.jump && substep == 0 && can_jump {
                body.vel.y = JUMP_SPEED;
                body.on_ground = false;
            }
            body.vel.y = (body.vel.y + GRAVITY * h).max(TERMINAL_VELOCITY);
        }

        if !body.fly {
            let speed = if input.sprint {
                SPRINT_SPEED
            } else {
                WALK_SPEED
            } * if body.in_water { 0.6 } else { 1.0 };
            body.vel.x = input.wish.x * speed;
            body.vel.z = input.wish.z * speed;
        }

        for axis in [1usize, 0, 2] {
            let min = body.pos + Vec3::new(-PLAYER_HALF_W, 0.0, -PLAYER_HALF_W);
            let max = body.pos + Vec3::new(PLAYER_HALF_W, PLAYER_H, PLAYER_HALF_W);
            let (moved, hit) = sweep_axis(world, min, max, body.vel[axis] * h, axis);
            body.pos[axis] += moved;
            if hit {
                body.vel[axis] = 0.0;
                if axis == 1 && moved < 0.0 {
                    body.on_ground = true;
                }
            }
        }
        body.in_water = is_in_water(world, body.pos);
    }
}

fn is_in_water(world: &World, pos: Vec3) -> bool {
    let base = pos.floor().as_ivec3();
    [base, base + IVec3::Y, base + IVec3::Y * 2]
        .into_iter()
        .any(|bp| world.get_block(bp) == WATER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::STONE;

    fn world_with_floor() -> World {
        let mut world = World::new(0);
        world.ensure_loaded(IVec3::ZERO);
        for x in -4..=4 {
            for z in -4..=4 {
                for y in 1..=70 {
                    world.set_block(IVec3::new(x, y, z), crate::world::block::AIR);
                }
            }
        }
        for x in -4..=4 {
            for z in -4..=4 {
                world.set_block(IVec3::new(x, 0, z), STONE);
            }
        }
        world
    }

    #[test]
    fn physics_falls_and_lands_on_ground() {
        let world = world_with_floor();
        let mut body = Body {
            pos: Vec3::new(0.5, 5.0, 0.5),
            ..Default::default()
        };
        step(&world, &mut body, MoveInput::default(), 2.0);
        assert!(body.on_ground);
        assert!((body.pos.y - 1.0).abs() < 1e-3);
    }

    #[test]
    fn physics_jump_height_about_1_25() {
        let world = world_with_floor();
        let mut body = Body {
            pos: Vec3::new(0.5, 1.0, 0.5),
            on_ground: true,
            ..Default::default()
        };
        let mut highest = body.pos.y;
        for i in 0..120 {
            step(
                &world,
                &mut body,
                MoveInput {
                    jump: i == 0,
                    ..Default::default()
                },
                1.0 / 120.0,
            );
            highest = highest.max(body.pos.y);
        }
        assert!((highest - 2.29).abs() < 0.12);
    }

    #[test]
    fn physics_wall_blocks_horizontal_motion() {
        let mut world = world_with_floor();
        for y in 1..=3 {
            world.set_block(IVec3::new(2, y, 0), STONE);
        }
        let mut body = Body {
            pos: Vec3::new(0.5, 1.0, 0.5),
            ..Default::default()
        };
        step(
            &world,
            &mut body,
            MoveInput {
                wish: Vec3::X,
                ..Default::default()
            },
            1.0,
        );
        assert!((body.pos.x - 1.7).abs() < 1e-3);
        assert!(body.pos.x + PLAYER_HALF_W <= 2.0 + 1e-4);
    }

    #[test]
    fn physics_no_tunneling_at_low_fps() {
        let world = world_with_floor();
        let mut body = Body {
            pos: Vec3::new(0.5, 5.0, 0.5),
            vel: Vec3::new(0.0, -40.0, 0.0),
            ..Default::default()
        };
        step(&world, &mut body, MoveInput::default(), 0.25);
        assert!(body.on_ground && body.pos.y >= 1.0 - 1e-3);
    }

    #[test]
    fn physics_fly_ignores_gravity() {
        let world = world_with_floor();
        let mut body = Body {
            pos: Vec3::new(0.5, 5.0, 0.5),
            fly: true,
            ..Default::default()
        };
        step(&world, &mut body, MoveInput::default(), 1.0);
        assert!((body.pos.y - 5.0).abs() < 1e-3);
    }

    #[test]
    fn physics_water_slows_and_space_swims_up() {
        let mut world = world_with_floor();
        world.set_block(IVec3::new(0, 1, 0), WATER);
        world.set_block(IVec3::new(0, 2, 0), WATER);
        let mut body = Body {
            pos: Vec3::new(0.5, 1.0, 0.5),
            ..Default::default()
        };
        step(
            &world,
            &mut body,
            MoveInput {
                wish: Vec3::X,
                up: true,
                ..Default::default()
            },
            1.0 / 120.0,
        );
        assert!(body.in_water);
        assert!((body.vel.x - WALK_SPEED * 0.6).abs() < 1e-5);
        assert!((body.vel.y - 4.0).abs() < 1e-5);
    }

    #[test]
    fn shape_collision_respects_half_slab_height() {
        let mut world = world_with_floor();
        world.set_block(IVec3::new(2, 1, 0), crate::world::block::SLAB_BASE);

        // The lower slab occupies y=1.0..1.5. A body starting above it must
        // pass through its empty upper half while moving horizontally.
        let min = Vec3::new(0.2, 1.6, 0.2);
        let max = Vec3::new(0.8, 2.8, 0.8);
        let (moved, hit) = sweep_axis(&world, min, max, 3.0, 0);
        assert!(!hit);
        assert!((moved - 3.0).abs() < 1e-6);
    }

    #[test]
    fn shape_collision_uses_stair_orientation() {
        let mut world = world_with_floor();
        world.set_block(IVec3::new(2, 1, 0), crate::world::block::STAIR_BASE);
        let min = Vec3::new(0.2, 1.6, 0.45);
        let max = Vec3::new(0.8, 2.8, 1.05);
        let (north_moved, north_hit) = sweep_axis(&world, min, max, 3.0, 0);
        assert!(north_hit);
        assert!((north_moved - 1.2).abs() < 1e-4);

        world.set_block(IVec3::new(2, 1, 0), crate::world::block::STAIR_BASE + 1);
        let (east_moved, east_hit) = sweep_axis(&world, min, max, 3.0, 0);
        assert!(east_hit);
        assert!((east_moved - 1.7).abs() < 1e-4);
    }

    #[test]
    fn shape_collision_keeps_fence_height_one_and_a_half() {
        let mut world = world_with_floor();
        world.set_block(IVec3::new(2, 1, 0), crate::world::block::FENCE_BASE);
        let min = Vec3::new(0.2, 2.2, 0.2);
        let max = Vec3::new(0.8, 3.4, 0.8);
        let (moved, hit) = sweep_axis(&world, min, max, 3.0, 0);
        assert!(hit);
        assert!((moved - 1.575).abs() < 1e-4);
    }

    #[test]
    fn shape_collision_uses_pane_connections_and_thickness() {
        let mut world = world_with_floor();
        world.set_block(IVec3::new(2, 1, 0), crate::world::block::PANE_BASE);

        let min = Vec3::new(0.2, 1.0, 0.2);
        let max = Vec3::new(0.8, 2.2, 0.8);
        let (center_moved, center_hit) = sweep_axis(&world, min, max, 3.0, 0);
        assert!(center_hit);
        assert!((center_moved - 1.6375).abs() < 1e-4);

        // A disconnected pane has no arm on the far side of the block.
        let far_min = Vec3::new(0.2, 1.0, 0.8);
        let far_max = Vec3::new(0.8, 2.2, 1.4);
        let (far_moved, far_hit) = sweep_axis(&world, far_min, far_max, 3.0, 0);
        assert!(!far_hit);
        assert!((far_moved - 3.0).abs() < 1e-6);
    }
}
