//! Command-line parsing for the snapshot renderer.

use std::path::PathBuf;

use super::fixtures::{Fixture, View};
use super::{
    AIR, DEFAULT_HEIGHT, DEFAULT_OUT, DEFAULT_PITCH, DEFAULT_RADIUS, DEFAULT_SEED, DEFAULT_WIDTH,
    DEFAULT_YAW, Edit, MesherMode, Options, TORCH,
};
use anyhow::Context;
use glam::{IVec3, Vec3};
use voxelforge::mesh::{ChunkMeshes, mesh_chunk_all, mesh_chunk_greedy_all};
use voxelforge::render::RenderPreset;
use voxelforge::world::block::BlockId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UiMode {
    None,
    Inventory,
    Hud,
    Pause,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GiMode {
    Off,
    On,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum HandActionSpec {
    Idle,
    Break { elapsed: f32 },
    Place { elapsed: f32 },
    Switch { elapsed: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InventoryCategory {
    All,
    Terrain,
    Masonry,
    Wood,
    Color,
    Glass,
    Detail,
    Shapes,
}

pub(super) fn parse_args<I>(args: I) -> anyhow::Result<Options>
where
    I: IntoIterator<Item = String>,
{
    let mut options = Options {
        seed: DEFAULT_SEED,
        pos: None,
        yaw: DEFAULT_YAW,
        pitch: DEFAULT_PITCH,
        radius: DEFAULT_RADIUS,
        width: DEFAULT_WIDTH,
        height: DEFAULT_HEIGHT,
        out: PathBuf::from(DEFAULT_OUT),
        edits: Vec::new(),
        mesher: MesherMode::Greedy,
        fixture: Fixture::Terrain,
        view: View::Final,
        day_phase: None,
        world_time: None,
        preset: RenderPreset::default(),
        render_scale: None,
        fixed_exposure: None,
        warmup: 0,
        frames: 1,
        timings: None,
        ui: UiMode::None,
        inventory_category: InventoryCategory::All,
        inventory_query: String::new(),
        hand_action: HandActionSpec::Idle,
        held_item: 101,
        gi: GiMode::Off,
    };
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| anyhow::anyhow!("missing value for {flag}"))
        };
        match flag.as_str() {
            "--seed" => options.seed = value()?.parse().context("--seed must be an integer")?,
            "--pos" => options.pos = Some(parse_vec3(&value()?)?),
            "--yaw" => options.yaw = value()?.parse().context("--yaw must be a number")?,
            "--pitch" => options.pitch = value()?.parse().context("--pitch must be a number")?,
            "--radius" => {
                options.radius = value()?.parse().context("--radius must be an integer")?;
                if options.radius < 0 {
                    anyhow::bail!("--radius must be non-negative");
                }
            }
            "--size" => {
                let (width, height) = parse_size(&value()?)?;
                options.width = width;
                options.height = height;
            }
            "--out" => options.out = PathBuf::from(value()?),
            "--edits" => options.edits = parse_edits(&value()?)?,
            "--mesher" => {
                options.mesher = match value()?.as_str() {
                    "culled" => MesherMode::Culled,
                    "greedy" => MesherMode::Greedy,
                    other => anyhow::bail!("--mesher must be culled or greedy, got {other:?}"),
                };
            }
            "--fixture" => options.fixture = Fixture::parse(&value()?)?,
            "--view" => options.view = View::parse(&value()?)?,
            "--day-phase" => {
                let phase: f32 = value()?.parse().context("--day-phase must be a number")?;
                if !(0.0..=1.0).contains(&phase) {
                    anyhow::bail!("--day-phase must be between 0 and 1");
                }
                options.day_phase = Some(phase);
            }
            "--world-time" => {
                let time: f32 = value()?.parse().context("--world-time must be a number")?;
                if !time.is_finite() || time < 0.0 {
                    anyhow::bail!("--world-time must be a non-negative finite number");
                }
                options.world_time = Some(time);
            }
            "--preset" => options.preset = RenderPreset::parse(&value()?)?,
            "--render-scale" => {
                let scale: f32 = value()?
                    .parse()
                    .context("--render-scale must be a number")?;
                if !(0.5..=1.0).contains(&scale) {
                    anyhow::bail!("--render-scale must be between 0.5 and 1.0");
                }
                options.render_scale = Some(scale);
            }
            "--fixed-exposure" => {
                let exposure: f32 = value()?
                    .parse()
                    .context("--fixed-exposure must be a number")?;
                if !exposure.is_finite() || exposure <= 0.0 {
                    anyhow::bail!("--fixed-exposure must be finite and positive");
                }
                options.fixed_exposure = Some(exposure)
            }
            "--warmup" => {
                options.warmup = value()?.parse().context("--warmup must be an integer")?
            }
            "--frames" => {
                options.frames = value()?.parse().context("--frames must be an integer")?
            }
            "--timings" => options.timings = Some(PathBuf::from(value()?)),
            "--gi" => {
                options.gi = match value()?.as_str() {
                    "on" => GiMode::On,
                    "off" => GiMode::Off,
                    other => anyhow::bail!("--gi must be on or off, got {other:?}"),
                };
            }
            "--ui" => {
                options.ui = match value()?.as_str() {
                    "inventory" => UiMode::Inventory,
                    "hud" => UiMode::Hud,
                    "pause" => UiMode::Pause,
                    "none" => UiMode::None,
                    other => {
                        anyhow::bail!("--ui must be inventory, hud, pause, or none, got {other:?}")
                    }
                };
            }
            "--inventory-category" => {
                options.inventory_category = parse_inventory_category(&value()?)?;
            }
            "--inventory-query" => {
                options.inventory_query = value()?;
                if options.inventory_query.len() > 32 {
                    anyhow::bail!("--inventory-query must be at most 32 bytes");
                }
            }
            "--hand-action" => options.hand_action = parse_hand_action(&value()?)?,
            "--held-item" => {
                options.held_item = value()?.parse().context("--held-item must be an integer")?;
                if options.held_item > 122 {
                    anyhow::bail!("--held-item must be between 0 and 122");
                }
            }
            "--help" | "-h" => {
                println!(
                    "snapshot [--fixture terrain|m6-light-room|m6-wind|m7-materials|m7-taau|m7-passes|m8-water|m8-godrays|m8-clouds|m8-lod|m9-gi-room|m9-clipmap|m10-shapes|m10-inventory|m10-viewmodel] [--view final|light|albedo|normal|depth|material|motion|reactive|water|volumetric|cloud|lod|gi|clipmap] [--gi on|off] [--ui inventory|hud|pause] [--inventory-category all|terrain|masonry|wood|color|glass|detail|shapes] [--inventory-query TEXT] [--hand-action idle|break:SECONDS|place:SECONDS|switch:SECONDS] [--held-item ITEM_ID] [--preset performance|balanced|m5_air_high|cinematic] [--render-scale 0.5..1] [--fixed-exposure N] [--warmup N] [--frames N] [--timings PATH] [--day-phase 0..1|--world-time SEC] [--seed N] [--pos X,Y,Z] [--yaw R] [--pitch R] [--radius N] [--size WxH] [--out PATH] [--edits \"set X,Y,Z,ID; ...\"] [--mesher culled|greedy]"
                );
                std::process::exit(0);
            }
            _ => anyhow::bail!("unknown argument: {flag}"),
        }
    }
    if options.day_phase.is_some() && options.world_time.is_some() {
        anyhow::bail!("--day-phase and --world-time are mutually exclusive");
    }
    Ok(options)
}

fn parse_inventory_category(value: &str) -> anyhow::Result<InventoryCategory> {
    match value.to_ascii_lowercase().as_str() {
        "all" => Ok(InventoryCategory::All),
        "terrain" => Ok(InventoryCategory::Terrain),
        "masonry" => Ok(InventoryCategory::Masonry),
        "wood" | "woodnature" | "wood_nature" => Ok(InventoryCategory::Wood),
        "color" => Ok(InventoryCategory::Color),
        "glass" | "glasslight" | "glass_light" => Ok(InventoryCategory::Glass),
        "detail" | "detailutility" | "detail_utility" => Ok(InventoryCategory::Detail),
        "shapes" => Ok(InventoryCategory::Shapes),
        other => anyhow::bail!("unknown inventory category: {other:?}"),
    }
}

fn parse_hand_action(value: &str) -> anyhow::Result<HandActionSpec> {
    if value == "idle" {
        return Ok(HandActionSpec::Idle);
    }
    let (kind, elapsed) = value.split_once(':').ok_or_else(|| {
        anyhow::anyhow!(
            "--hand-action must be idle, break:SECONDS, place:SECONDS, or switch:SECONDS"
        )
    })?;
    let elapsed: f32 = elapsed
        .parse()
        .context("--hand-action elapsed time must be a number")?;
    if !elapsed.is_finite() || elapsed < 0.0 {
        anyhow::bail!("--hand-action elapsed time must be finite and non-negative");
    }
    match kind {
        "break" => Ok(HandActionSpec::Break { elapsed }),
        "place" => Ok(HandActionSpec::Place { elapsed }),
        "switch" => Ok(HandActionSpec::Switch { elapsed }),
        other => anyhow::bail!("unknown hand action: {other:?}"),
    }
}

impl MesherMode {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Culled => "culled",
            Self::Greedy => "greedy",
        }
    }

    pub(super) fn mesh(self, padded: &voxelforge::world::chunk::PaddedChunk) -> ChunkMeshes {
        match self {
            Self::Culled => mesh_chunk_all(padded),
            Self::Greedy => mesh_chunk_greedy_all(padded),
        }
    }
}

fn parse_edits(value: &str) -> anyhow::Result<Vec<Edit>> {
    let mut edits = Vec::new();
    for command in value.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let fields = command
            .strip_prefix("set ")
            .ok_or_else(|| anyhow::anyhow!("edit must use `set X,Y,Z,ID`: {command:?}"))?
            .split(',')
            .collect::<Vec<_>>();
        if fields.len() != 4 {
            anyhow::bail!("edit must use `set X,Y,Z,ID`: {command:?}");
        }
        let parse_coord = |field: &str| {
            field
                .parse::<i32>()
                .with_context(|| format!("edit coordinate must be an integer: {field:?}"))
        };
        let position = IVec3::new(
            parse_coord(fields[0])?,
            parse_coord(fields[1])?,
            parse_coord(fields[2])?,
        );
        let id = fields[3]
            .parse::<BlockId>()
            .with_context(|| format!("edit block id must be an integer: {:?}", fields[3]))?;
        if id > TORCH {
            anyhow::bail!("edit block id must be between {AIR} and {TORCH}: {id}");
        }
        edits.push(Edit { position, id });
    }
    if edits.is_empty() {
        anyhow::bail!("--edits requires at least one `set X,Y,Z,ID` command");
    }
    Ok(edits)
}

fn parse_vec3(value: &str) -> anyhow::Result<Vec3> {
    let values: Vec<f32> = value
        .split(',')
        .map(|part| part.parse().context("--pos components must be numbers"))
        .collect::<anyhow::Result<_>>()?;
    match values.as_slice() {
        [x, y, z] => Ok(Vec3::new(*x, *y, *z)),
        _ => anyhow::bail!("--pos must be X,Y,Z"),
    }
}

fn parse_size(value: &str) -> anyhow::Result<(u32, u32)> {
    let (width, height) = value
        .split_once('x')
        .ok_or_else(|| anyhow::anyhow!("--size must be WIDTHxHEIGHT"))?;
    let width: u32 = width.parse().context("--size width must be an integer")?;
    let height: u32 = height.parse().context("--size height must be an integer")?;
    if width == 0 || height == 0 {
        anyhow::bail!("--size dimensions must be non-zero");
    }
    Ok((width, height))
}
