//! Deterministic worlds used by the M6 snapshot probes.

use glam::{IVec3, Vec3};
use voxelforge::world::block::{SAND, STONE, TORCH};
use voxelforge::world::chunk::Chunk;
use voxelforge::world::coords::WORLD_CHUNKS_Y;
use voxelforge::world::world::World;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fixture {
    Terrain,
    LightRoom,
    Wind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum View {
    Final,
    Light,
}

impl Fixture {
    pub(crate) fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "terrain" => Ok(Self::Terrain),
            "m6-light-room" => Ok(Self::LightRoom),
            "m6-wind" => Ok(Self::Wind),
            _ => anyhow::bail!("--fixture must be terrain, m6-light-room, or m6-wind"),
        }
    }
}

impl View {
    pub(crate) fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "final" => Ok(Self::Final),
            "light" => Ok(Self::Light),
            _ => anyhow::bail!("--view must be final or light"),
        }
    }
}

/// Load all eight vertical chunks around a fixture and place its authored geometry.
pub(crate) fn build(seed: u64, fixture: Fixture) -> World {
    let mut world = World::new(seed);
    let (min_x, max_x, min_z, max_z) = match fixture {
        Fixture::Terrain => (-4, 4, -4, 4),
        Fixture::LightRoom => (-2, 2, -2, 2),
        Fixture::Wind => (-1, 1, -1, 1),
    };
    for z in min_z..=max_z {
        for x in min_x..=max_x {
            for y in 0..WORLD_CHUNKS_Y {
                if matches!(fixture, Fixture::Terrain) {
                    world.ensure_loaded(IVec3::new(x, y, z));
                } else {
                    world.insert_generated(IVec3::new(x, y, z), Chunk::new_air());
                }
            }
        }
    }
    match fixture {
        Fixture::Terrain => {}
        Fixture::LightRoom => build_light_room(&mut world),
        Fixture::Wind => build_wind(&mut world),
    }
    world
}

fn build_light_room(world: &mut World) {
    // A fully sealed stone chamber makes sky light irrelevant to the probes.
    for x in -48..49 {
        for y in 4..29 {
            for z in -56..25 {
                let shell = x == -48 || x == 48 || y == 4 || y == 28 || z == -56 || z == 24;
                if shell {
                    let _ = world.set_block(IVec3::new(x, y, z), STONE);
                }
            }
        }
    }
    // Source offsets compensate for the mesher's four-cell corner average so
    // the four panel-center fragments resolve to block levels 13, 9, 5, and 1.
    for x in [-26, -9, 8, 26] {
        for panel_x in x - 1..=x + 1 {
            for y in 5..28 {
                let _ = world.set_block(IVec3::new(panel_x, y, -30), SAND);
            }
        }
    }
    for torch_x in [-27, -25, -5, 0, 38] {
        let _ = world.set_block(IVec3::new(torch_x, 12, -29), TORCH);
    }
}

fn build_wind(world: &mut World) {
    // Keep the two crops in one sealed, stable volume so the camera and light
    // framing remain independent of terrain noise.
    for x in -16..24 {
        for z in -4..5 {
            let _ = world.set_block(IVec3::new(x, 8, z), STONE);
        }
    }
    for x in -12..4 {
        for y in 9..15 {
            for z in -1..2 {
                let _ = world.set_block(IVec3::new(x, y, z), voxelforge::world::block::LEAVES);
            }
        }
    }
    for x in 14..23 {
        for y in 9..15 {
            for z in -1..2 {
                let _ = world.set_block(IVec3::new(x, y, z), STONE);
            }
        }
    }
}

pub(crate) fn camera(fixture: Fixture) -> (Vec3, f32, f32) {
    match fixture {
        Fixture::Terrain => (Vec3::new(0.0, 18.0, 28.0), 0.0, -0.25),
        Fixture::LightRoom => (Vec3::new(0.0, 12.0, 20.0), 0.0, 0.0),
        Fixture::Wind => (Vec3::new(0.0, 12.0, 24.0), 0.0, -0.04),
    }
}
