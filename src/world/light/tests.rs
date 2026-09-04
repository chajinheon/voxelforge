use super::*;
use crate::world::block::{AIR, BlockId, STONE, TORCH, def};
use crate::world::chunk::{block_light, pack_light, sky_light};
fn input(blocks: Box<[BlockId; LIGHT_COLUMN_CELLS]>) -> LightColumnSnapshot {
    LightColumnSnapshot {
        column: LightColumn { x: 0, z: 0 },
        epoch: 1,
        blocks,
        incoming: std::array::from_fn(|_| Box::new([0; LIGHT_BOUNDARY_CELLS])),
    }
}
#[test]
fn light_pack_roundtrip() {
    for b in 0..=15 {
        for s in 0..=15 {
            let p = pack_light(b, s);
            assert_eq!((block_light(p), sky_light(p)), (b, s));
        }
    }
    assert_eq!(pack_light(99, 99), 255);
}
#[test]
fn skylight_descends_without_attenuation() {
    let r = solve_column(&input(Box::new([AIR; LIGHT_COLUMN_CELLS])));
    assert_eq!(sky_light(r.light[index(3, 0, 4)]), 15);
}
#[test]
fn skylight_spreads_sideways_with_unit_falloff() {
    let mut b = Box::new([STONE; LIGHT_COLUMN_CELLS]);
    b[index(0, 255, 1)] = AIR;
    b[index(0, 254, 1)] = AIR;
    b[index(1, 254, 1)] = AIR;
    b[index(2, 254, 1)] = AIR;
    b[index(15, 254, 1)] = AIR;
    let r = solve_column(&input(b));
    for (distance, expected) in [(0, 15), (1, 14), (2, 13), (15, 0)] {
        assert_eq!(sky_light(r.light[index(distance, 254, 1)]), expected);
    }

    // Horizontal incoming light reaches the cell above with one attenuation.
    let mut snapshot = input(Box::new([STONE; LIGHT_COLUMN_CELLS]));
    snapshot.blocks[index(0, 100, 5)] = AIR;
    snapshot.blocks[index(1, 100, 5)] = AIR;
    snapshot.blocks[index(1, 101, 5)] = AIR;
    snapshot.incoming[0][5 + 32 * 100] = pack_light(0, 15);
    let r = solve_column(&snapshot);
    assert_eq!(sky_light(r.light[index(1, 101, 5)]), 12);
}
#[test]
fn block_light_falls_off_one_per_block() {
    let mut b = Box::new([AIR; LIGHT_COLUMN_CELLS]);
    b[index(10, 20, 10)] = TORCH;
    let r = solve_column(&input(b));
    for (distance, expected) in [(0, 14), (1, 13), (2, 12), (14, 0), (15, 0)] {
        assert_eq!(block_light(r.light[index(10 + distance, 20, 10)]), expected);
    }
}
#[test]
fn opaque_block_stops_both_light_channels() {
    let mut b = Box::new([AIR; LIGHT_COLUMN_CELLS]);
    // A full-height wall prevents the torch from routing around it.
    for y in 0..256 {
        for z in 0..32 {
            b[index(2, y, z)] = STONE;
        }
    }
    b[index(1, 128, 2)] = TORCH;
    let r = solve_column(&input(b));
    assert_eq!(sky_light(r.light[index(2, 200, 2)]), 0);
    assert_eq!(block_light(r.light[index(3, 128, 2)]), 0);
}
#[test]
fn torch_emission_is_fourteen() {
    assert_eq!(def(TORCH).emission, 14);
}

#[test]
fn solve_column_is_deterministic() {
    let mut b = Box::new([AIR; LIGHT_COLUMN_CELLS]);
    b[index(10, 120, 10)] = TORCH;
    b[index(10, 121, 10)] = STONE;
    let first = solve_column(&input(b.clone()));
    let second = solve_column(&input(b));
    assert_eq!(first.light, second.light);
    assert_eq!(first.boundary, second.boundary);
}

#[test]
fn light_types_are_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<LightColumnSnapshot>();
    assert_send_sync::<LightColumnResult>();
}

fn settle(w: &mut crate::world::world::World) {
    let mut n = std::collections::HashMap::new();
    loop {
        let ds = w.take_light_dirty();
        if ds.is_empty() {
            break;
        }
        for c in ds {
            let e = w.light_epoch(c);
            if let Some(s) = w.light_column_snapshot(c, e) {
                *n.entry(c).or_insert(0usize) += 1;
                assert!(n[&c] <= 16);
                w.apply_light_column(solve_column(&s));
            }
        }
    }
}
fn load(w: &mut crate::world::world::World, x: i32, z: i32) {
    for y in 0..crate::world::coords::WORLD_CHUNKS_Y {
        w.insert_generated(
            glam::IVec3::new(x, y, z),
            crate::world::chunk::Chunk::new_air(),
        );
    }
}
#[test]
fn light_crosses_column_boundary() {
    let mut w = crate::world::world::World::new(0);
    load(&mut w, 0, 0);
    load(&mut w, 1, 0);
    w.set_block(glam::IVec3::new(31, 128, 0), TORCH);
    settle(&mut w);
    assert_eq!(
        block_light(
            w.chunk(glam::IVec3::new(1, 4, 0))
                .unwrap()
                .get_light(glam::UVec3::new(0, 0, 0))
        ),
        13
    );
}
#[test]
fn removing_torch_converges_to_zero() {
    let mut w = crate::world::world::World::new(0);
    load(&mut w, 0, 0);
    load(&mut w, 1, 0);
    w.set_block(glam::IVec3::new(31, 128, 0), TORCH);
    settle(&mut w);
    w.set_block(glam::IVec3::new(31, 128, 0), AIR);
    settle(&mut w);
    for x in 0..=1 {
        assert!(
            w.chunk(glam::IVec3::new(x, 4, 0))
                .unwrap()
                .light
                .iter()
                .all(|p| block_light(*p) == 0)
        );
    }
}

#[test]
fn provisional_boundary_respects_opaque_blocker() {
    let mut w = crate::world::world::World::new(0);
    for y in 0..crate::world::coords::WORLD_CHUNKS_Y {
        w.insert_generated(
            glam::IVec3::new(0, y, 0),
            crate::world::chunk::Chunk::new_air(),
        );
        w.insert_generated(
            glam::IVec3::new(1, y, 0),
            crate::world::chunk::Chunk::new_air(),
        );
    }
    w.set_block(glam::IVec3::new(32, 100, 0), STONE);
    let s = w
        .light_column_snapshot(
            LightColumn { x: 0, z: 0 },
            w.light_epoch(LightColumn { x: 0, z: 0 }),
        )
        .unwrap();
    assert_eq!(sky_light(s.incoming[1][32 * 99]), 0);
    assert_eq!(sky_light(s.incoming[1][32 * 101]), 15);
    let s = w
        .light_column_snapshot(
            LightColumn { x: 1, z: 0 },
            w.light_epoch(LightColumn { x: 1, z: 0 }),
        )
        .unwrap();
    assert_eq!(sky_light(s.incoming[1][32 * 101]), 15);
}
#[test]
fn light_apply_does_not_mark_chunk_modified() {
    let mut w = crate::world::world::World::new(0);
    load(&mut w, 0, 0);
    let v = w.chunk_version(glam::IVec3::new(0, 0, 0));
    let m = w.modified.clone();
    settle(&mut w);
    assert_eq!(v, w.chunk_version(glam::IVec3::new(0, 0, 0)));
    assert_eq!(m, w.modified);
}
