use super::World;
use crate::world::block::AIR;
use crate::world::chunk::Chunk;
use crate::world::coords::{CHUNK_SIZE, WORLD_CHUNKS_Y};
use crate::world::light::LightColumn;
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

    let neighbor_version = world.chunk_version(touched_neighbor).unwrap();
    assert!(world.set_block(edited * CHUNK_SIZE, AIR));

    let dirty = world.take_dirty();
    assert!(dirty.contains(&edited));
    assert!(dirty.contains(&touched_neighbor));
    assert!(!dirty.contains(&opposite_neighbor));
    assert_eq!(
        world.chunk_version(touched_neighbor),
        Some(neighbor_version)
    );
}

#[test]
fn loading_adjacent_chunks_does_not_change_content_versions() {
    let mut world = World::new(0);
    let first = IVec3::new(0, 0, 0);
    let second = IVec3::new(1, 0, 0);

    world.insert_loaded(first, crate::world::chunk::Chunk::new_air());
    assert_eq!(world.chunk_version(first), Some(1));
    world.insert_loaded(second, crate::world::chunk::Chunk::new_air());

    assert_eq!(world.chunk_version(first), Some(1));
    assert_eq!(world.chunk_version(second), Some(1));
    assert_eq!(world.saved_version.get(&first), Some(&1));
    assert_eq!(world.saved_version.get(&second), Some(&1));
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

#[test]
fn incomplete_column_insert_does_not_dirty_loaded_adjacent_column() {
    let mut world = World::new(0);
    for y in 0..WORLD_CHUNKS_Y {
        world.insert_generated(IVec3::new(1, y, 0), Chunk::new_air());
    }
    world.take_light_dirty();
    for y in 0..WORLD_CHUNKS_Y - 1 {
        world.insert_generated(IVec3::new(0, y, 0), Chunk::new_air());
    }
    let dirty = world.take_light_dirty();
    assert!(!dirty.contains(&LightColumn { x: 1, z: 0 }));
    world.insert_generated(IVec3::new(0, WORLD_CHUNKS_Y - 1, 0), Chunk::new_air());
    assert!(
        world
            .take_light_dirty()
            .contains(&LightColumn { x: 1, z: 0 })
    );
}

#[test]
fn block_light_memory_uses_loaded_chunk_count_and_three_bytes_per_cell() {
    let mut world = World::new(0);
    assert_eq!(world.block_light_memory_bytes(), 0);
    world.ensure_loaded(IVec3::ZERO);
    world.ensure_loaded(IVec3::new(1, 0, 0));
    assert_eq!(
        world.block_light_memory_bytes(),
        2 * crate::world::chunk::CHUNK_VOLUME * 3
    );
}
