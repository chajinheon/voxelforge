//! Deterministic worlds used by the M6 snapshot probes.

use glam::{IVec3, Vec3};
use serde::Serialize;
use voxelforge::world::block::{BRICK, COBBLE, LOG, PLANKS, SAND, STONE, TORCH, WATER};
use voxelforge::world::chunk::Chunk;
use voxelforge::world::coords::WORLD_CHUNKS_Y;
use voxelforge::world::world::World;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fixture {
    Terrain,
    LightRoom,
    Wind,
    Materials,
    Taau,
    Passes,
    Water,
    Godrays,
    Clouds,
    Lod,
    GiRoom,
    Clipmap,
    Shapes,
    Inventory,
    ViewModel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum View {
    Final,
    Light,
    Albedo,
    Normal,
    Depth,
    Material,
    Motion,
    Reactive,
    Water,
    Volumetric,
    Cloud,
    Lod,
    Gi,
    Clipmap,
}

impl Fixture {
    pub(crate) fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "terrain" => Ok(Self::Terrain),
            "m6-light-room" => Ok(Self::LightRoom),
            "m6-wind" => Ok(Self::Wind),
            "m7-materials" => Ok(Self::Materials),
            "m7-taau" => Ok(Self::Taau),
            "m7-passes" => Ok(Self::Passes),
            "m8-water" => Ok(Self::Water),
            "m8-godrays" => Ok(Self::Godrays),
            "m8-clouds" => Ok(Self::Clouds),
            "m8-lod" => Ok(Self::Lod),
            "m9-gi-room" => Ok(Self::GiRoom),
            "m9-clipmap" => Ok(Self::Clipmap),
            "m10-shapes" => Ok(Self::Shapes),
            "m10-inventory" => Ok(Self::Inventory),
            "m10-viewmodel" => Ok(Self::ViewModel),
            _ => anyhow::bail!("unknown snapshot fixture: {value:?}"),
        }
    }
}

impl View {
    pub(crate) fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "final" => Ok(Self::Final),
            "light" => Ok(Self::Light),
            "albedo" => Ok(Self::Albedo),
            "normal" => Ok(Self::Normal),
            "depth" => Ok(Self::Depth),
            "material" => Ok(Self::Material),
            "motion" => Ok(Self::Motion),
            "reactive" => Ok(Self::Reactive),
            "water" => Ok(Self::Water),
            "volumetric" => Ok(Self::Volumetric),
            "cloud" => Ok(Self::Cloud),
            "lod" => Ok(Self::Lod),
            "gi" => Ok(Self::Gi),
            "clipmap" => Ok(Self::Clipmap),
            _ => anyhow::bail!("unknown snapshot view: {value:?}"),
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
        Fixture::Materials => (-2, 2, -1, 1),
        Fixture::Taau | Fixture::Passes | Fixture::Lod => (-3, 3, -3, 3),
        Fixture::Clipmap => (1, 7, -3, 3),
        Fixture::Water | Fixture::Godrays | Fixture::Clouds | Fixture::GiRoom => (-2, 2, -2, 2),
        Fixture::Shapes | Fixture::Inventory | Fixture::ViewModel => (-2, 2, -2, 2),
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
        Fixture::Materials => build_materials(&mut world),
        Fixture::Taau | Fixture::Passes | Fixture::Lod => build_terrain_gallery(&mut world),
        Fixture::Clipmap => build_clipmap(&mut world),
        Fixture::Water => build_water(&mut world),
        Fixture::Godrays => build_godrays(&mut world),
        Fixture::Clouds => build_clouds(&mut world),
        Fixture::GiRoom => build_gi_room(&mut world),
        Fixture::Shapes => build_shapes(&mut world),
        Fixture::Inventory | Fixture::ViewModel => build_terrain_gallery(&mut world),
    }
    world
}

fn build_materials(world: &mut World) {
    use voxelforge::world::block::{BRICK, GLOW_PANEL, GOLD_BLOCK, METAL_PANEL};
    let ids = [STONE, BRICK, PLANKS, METAL_PANEL, GOLD_BLOCK, GLOW_PANEL];
    for (panel, id) in ids.into_iter().enumerate() {
        for x in (panel as i32 * 10 - 25)..(panel as i32 * 10 - 17) {
            for y in 8..20 {
                let _ = world.set_block(IVec3::new(x, y, -8), id);
            }
        }
    }
}

fn build_terrain_gallery(world: &mut World) {
    for x in -24..25 {
        for z in -24..25 {
            let _ = world.set_block(IVec3::new(x, 8, z), STONE);
            if (x + z).rem_euclid(11) == 0 {
                let _ = world.set_block(IVec3::new(x, 9, z), voxelforge::world::block::GRASS);
            }
        }
    }
    for x in -12..13 {
        for y in 9..13 {
            let _ = world.set_block(IVec3::new(x, y, -12), BRICK);
        }
    }
}

fn build_clipmap(world: &mut World) {
    // Center authored geometry on the x=64 clipmap boundary used by the seam
    // probe. The elevated chamber makes the exact y=72 capture exercise real
    // material and light voxels instead of comparing two empty sky images.
    for x in 32..97 {
        for z in -48..17 {
            let _ = world.set_block(IVec3::new(x, 66, z), STONE);
        }
    }
    for x in 38..91 {
        for y in 67..79 {
            if !(-3..=3).contains(&(x - 64)) {
                let _ = world.set_block(IVec3::new(x, y, -30), BRICK);
            }
        }
    }
    for x in [48, 64, 80] {
        for y in 67..73 {
            let _ = world.set_block(IVec3::new(x, y, -18), PLANKS);
        }
        let _ = world.set_block(IVec3::new(x, 73, -18), TORCH);
    }
}

fn build_water(world: &mut World) {
    for x in -24..25 {
        for z in -24..25 {
            let _ = world.set_block(IVec3::new(x, 7, z), STONE);
            let _ = world.set_block(IVec3::new(x, 8, z), WATER);
        }
    }
    for x in -8..9 {
        for z in -8..9 {
            let _ = world.set_block(IVec3::new(x, 9, z), WATER);
        }
    }
}

fn build_godrays(world: &mut World) {
    build_light_room(world);
    for x in -16..17 {
        for z in -55..-48 {
            let _ = world.set_block(IVec3::new(x, 18, z), voxelforge::world::block::AIR);
        }
    }
}

fn build_clouds(world: &mut World) {
    build_terrain_gallery(world);
    for x in -18..19 {
        for z in -10..11 {
            let wave = (x * x + z * 3_i32).rem_euclid(17);
            if wave < 8 {
                let _ = world.set_block(
                    IVec3::new(x, 28 + wave / 4, z),
                    voxelforge::world::block::LEAVES,
                );
            }
        }
    }
}

fn build_gi_room(world: &mut World) {
    build_light_room(world);
    for x in -28..29 {
        for y in 5..20 {
            let _ = world.set_block(IVec3::new(x, y, -25), BRICK);
        }
    }
    // Center the warm source over the floor probe. Multiples of four keep the
    // source represented when the distant primary ray selects clipmap L2.
    for x in [-4, 0, 4] {
        let _ = world.set_block(IVec3::new(x, 8, -8), TORCH);
    }
}

fn build_shapes(world: &mut World) {
    for x in -24..25 {
        for z in -14..7 {
            let _ = world.set_block(IVec3::new(x, 6, z), STONE);
        }
    }
    // Stable state IDs keep this gallery useful to both shape-aware and cube
    // fallback meshers while the snapshot remains surface independent.
    for (index, id) in [256, 257, 512, 520, 528, 536, 768, 773, 783, 896, 901, 911]
        .into_iter()
        .enumerate()
    {
        let x = index as i32 * 4 - 22;
        let _ = world.set_block(IVec3::new(x, 7, -4), id);
    }
    for x in -18..19 {
        let _ = world.set_block(IVec3::new(x, 7, 4), PLANKS);
    }
    let _ = world.set_block(IVec3::new(0, 8, 0), LOG);
    let _ = world.set_block(IVec3::new(4, 8, 0), COBBLE);
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
        Fixture::Materials => (Vec3::new(0.0, 14.0, 26.0), 0.0, 0.0),
        Fixture::Taau | Fixture::Passes | Fixture::Lod => (Vec3::new(0.0, 17.0, 28.0), 0.0, -0.2),
        Fixture::Water => (Vec3::new(0.0, 13.0, 25.0), 0.0, -0.1),
        Fixture::Godrays => (Vec3::new(0.0, 13.0, 20.0), 0.0, 0.0),
        Fixture::Clouds => (Vec3::new(0.0, 19.0, 30.0), 0.0, -0.18),
        Fixture::GiRoom => (Vec3::new(0.0, 13.0, 20.0), 0.0, 0.0),
        Fixture::Clipmap => (Vec3::new(64.0, 72.0, 0.0), 0.0, -0.15),
        Fixture::Shapes => (Vec3::new(0.0, 12.0, 24.0), 0.0, -0.05),
        Fixture::Inventory | Fixture::ViewModel => (Vec3::new(0.0, 17.0, 28.0), 0.0, -0.2),
    }
}

#[derive(Debug, Serialize, serde::Deserialize)]
struct SnapshotProbe {
    name: String,
    x: u32,
    y: u32,
    expected_linear_rgb: [f32; 3],
    epsilon: f32,
}

/// Emit and immediately validate the shape gallery's deterministic readback probes.
pub(crate) fn write_shape_probes(pixels: &[u8], width: u32, height: u32) -> anyhow::Result<()> {
    // These values are the checked-in readback contract for the 1280x720,
    // render-scale 1.0 shape gallery.  Keep them independent of the current
    // frame: deriving the reference from `pixels` would make this probe
    // tautological and unable to catch a regression.
    let points = [
        ("slab_bottom", 150, 430, [0.08228271, 0.11953843, 0.1746474]),
        (
            "stair_north",
            640,
            430,
            [0.09758735, 0.12477182, 0.15292615],
        ),
        ("pane_mask5", 900, 430, [0.23074005, 0.28744084, 0.3515326]),
    ];
    let probes: Vec<_> = points
        .into_iter()
        .map(|(name, x, y, expected_linear_rgb)| {
            let x = x.min(width.saturating_sub(1));
            let y = y.min(height.saturating_sub(1));
            SnapshotProbe {
                name: name.to_owned(),
                x,
                y,
                expected_linear_rgb,
                epsilon: 1.0 / 255.0,
            }
        })
        .collect();
    let path = std::path::Path::new("/tmp/vf_m10_shapes.probes.json");
    std::fs::write(path, serde_json::to_vec_pretty(&probes)?)?;
    let data: Vec<SnapshotProbe> = serde_json::from_slice(&std::fs::read(path)?)?;
    for probe in data {
        let i = ((probe.y * width + probe.x) * 4) as usize;
        for channel in 0..3 {
            let actual = srgb_to_linear(pixels[i + channel]);
            if (actual - probe.expected_linear_rgb[channel]).abs() > probe.epsilon {
                anyhow::bail!(
                    "shape probe {} failed at ({}, {})",
                    probe.name,
                    probe.x,
                    probe.y
                );
            }
        }
    }
    Ok(())
}

fn srgb_to_linear(value: u8) -> f32 {
    let value = f32::from(value) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
