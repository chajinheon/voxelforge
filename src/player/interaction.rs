use glam::{IVec3, Vec3};

use super::camera::{EYE_HEIGHT, PLAYER_H, PLAYER_HALF_W};

const AABB_EPSILON: f32 = 1e-6;

/// Returns whether placing a unit block would overlap the player's AABB.
pub fn place_rejected_inside_player_aabb(block: IVec3, eye_pos: Vec3) -> bool {
    let feet = eye_pos.y - EYE_HEIGHT;
    let player_min = Vec3::new(eye_pos.x - PLAYER_HALF_W, feet, eye_pos.z - PLAYER_HALF_W);
    let player_max = Vec3::new(
        eye_pos.x + PLAYER_HALF_W,
        feet + PLAYER_H,
        eye_pos.z + PLAYER_HALF_W,
    );
    let block_min = block.as_vec3();
    let block_max = block_min + Vec3::ONE;

    // Strict comparisons make touching faces/edges valid while rejecting only
    // positive-volume intersection.
    block_min.x + AABB_EPSILON < player_max.x
        && block_max.x > player_min.x + AABB_EPSILON
        && block_min.y + AABB_EPSILON < player_max.y
        && block_max.y > player_min.y + AABB_EPSILON
        && block_min.z + AABB_EPSILON < player_max.z
        && block_max.z > player_min.z + AABB_EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;

    const EYE: Vec3 = Vec3::new(1.3, EYE_HEIGHT + 1.0, 0.5);

    #[test]
    fn place_rejected_inside_player_aabb() {
        // A block inside the footprint and one intersecting the player's head.
        assert!(super::place_rejected_inside_player_aabb(
            IVec3::new(1, 1, 0),
            EYE
        ));
        assert!(super::place_rejected_inside_player_aabb(
            IVec3::new(1, 2, 0),
            EYE
        ));
        // The block below the feet and the block at the side only touch.
        assert!(!super::place_rejected_inside_player_aabb(
            IVec3::new(1, 0, 0),
            EYE
        ));
        assert!(!super::place_rejected_inside_player_aabb(
            IVec3::new(0, 1, 0),
            EYE
        ));
        // A distant block is valid as well.
        assert!(!super::place_rejected_inside_player_aabb(
            IVec3::new(10, 10, 10),
            EYE
        ));
    }
}
