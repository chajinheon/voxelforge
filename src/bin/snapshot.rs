//! Surface-free M1 terrain snapshot renderer.

use std::path::PathBuf;

use anyhow::Context;
use glam::{IVec3, Vec3};
use voxelforge::mesh::{ChunkMeshes, mesh_chunk_all, mesh_chunk_greedy_all};
use voxelforge::render::{Globals, Gpu, GpuChunkMeshes, OffscreenTarget, Renderer};
use voxelforge::world::block::{AIR, BlockId, COBBLE};
use voxelforge::world::coords::{WORLD_CHUNKS_Y, chunk_of};
use voxelforge::world::r#gen::WorldGen;
use voxelforge::world::world::World;

const DEFAULT_SEED: u64 = 1;
const DEFAULT_YAW: f32 = 0.6;
const DEFAULT_PITCH: f32 = -0.3;
const DEFAULT_RADIUS: i32 = 4;
const DEFAULT_WIDTH: u32 = 1280;
const DEFAULT_HEIGHT: u32 = 720;
const DEFAULT_OUT: &str = "/tmp/vf.png";
const FOV_Y: f32 = 60.0_f32.to_radians();
const NEAR: f32 = 0.05;
const FAR: f32 = 1000.0;

#[derive(Clone, Copy, Debug)]
enum MesherMode {
    Culled,
    Greedy,
}

#[derive(Clone, Copy, Debug)]
struct Edit {
    position: IVec3,
    id: BlockId,
}

struct Options {
    seed: u64,
    pos: Option<Vec3>,
    yaw: f32,
    pitch: f32,
    radius: i32,
    width: u32,
    height: u32,
    out: PathBuf,
    edits: Vec<Edit>,
    mesher: MesherMode,
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn"),
    )
    .init();

    let options = parse_args(std::env::args().skip(1))?;
    let gpu = Gpu::new()?;
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8UnormSrgb)?;

    let generator = WorldGen::new(options.seed);
    let position = options
        .pos
        .unwrap_or_else(|| Vec3::new(0.0, generator.height_at(0, 0) as f32 + 3.0, 0.0));
    let target = OffscreenTarget::new(&gpu.device, options.width, options.height)?;
    log::info!("snapshot: mesher={}", options.mesher.name());
    let chunks = build_terrain(
        &mut renderer,
        options.seed,
        position,
        options.radius,
        &options.edits,
        options.mesher,
    )?;
    let globals = make_globals(
        position,
        options.yaw,
        options.pitch,
        options.width,
        options.height,
    );
    renderer.render(&target.color_view, &target.depth_view, &globals, &chunks);
    let pixels = target.read_pixels(&gpu.device, &gpu.queue)?;
    image::save_buffer_with_format(
        &options.out,
        &pixels,
        options.width,
        options.height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )?;
    log::info!(
        "snapshot: wrote {} ({}x{}, {} chunks)",
        options.out.display(),
        options.width,
        options.height,
        chunks.len()
    );
    Ok(())
}

fn build_terrain(
    renderer: &mut Renderer,
    seed: u64,
    position: Vec3,
    radius: i32,
    edits: &[Edit],
    mesher: MesherMode,
) -> anyhow::Result<Vec<GpuChunkMeshes>> {
    let center = chunk_of(IVec3::new(
        position.x.floor() as i32,
        position.y.floor() as i32,
        position.z.floor() as i32,
    ));
    let mut world = World::new(seed);
    for z in center.z - radius..=center.z + radius {
        for x in center.x - radius..=center.x + radius {
            for y in 0..WORLD_CHUNKS_Y {
                world.ensure_loaded(IVec3::new(x, y, z));
            }
        }
    }

    for edit in edits {
        if !world.set_block(edit.position, edit.id) {
            anyhow::bail!(
                "edit position {:?} is outside the loaded world or vertical bounds",
                edit.position
            );
        }
    }

    let mut dirty = world.take_dirty();
    dirty.sort_by_key(|cp| {
        (
            (cp.x - center.x).pow(2) + (cp.z - center.z).pow(2) + cp.y.pow(2),
            cp.x,
            cp.y,
            cp.z,
        )
    });
    let mut gpu_chunks = Vec::with_capacity(dirty.len());
    let mut vertex_count = 0usize;
    let mut index_count = 0usize;
    for cp in dirty {
        let padded = world.padded(cp);
        let meshes = mesher.mesh(&padded);
        vertex_count += meshes.opaque.vertices.len() + meshes.translucent.vertices.len();
        index_count += meshes.opaque.indices.len() + meshes.translucent.indices.len();
        if !(meshes.opaque.vertices.is_empty() && meshes.translucent.vertices.is_empty()) {
            gpu_chunks.push(renderer.upload_chunk_meshes(&meshes, cp * 32)?);
        }
    }
    log::info!(
        "snapshot: mesher={} vertices {vertex_count} indices {index_count}",
        mesher.name()
    );
    Ok(gpu_chunks)
}

fn make_globals(position: Vec3, yaw: f32, pitch: f32, width: u32, height: u32) -> Globals {
    let view = glam::camera::rh::view::look_to_mat4(position, forward(yaw, pitch), Vec3::Y);
    let projection = glam::camera::rh::proj::directx::perspective(
        FOV_Y,
        width.max(1) as f32 / height.max(1) as f32,
        NEAR,
        FAR,
    );
    Globals::new(
        projection * view,
        position,
        Vec3::new(-0.4, -1.0, -0.3).normalize(),
        0.0,
        width as f32,
        height as f32,
    )
}

fn forward(yaw: f32, pitch: f32) -> Vec3 {
    Vec3::new(
        yaw.sin() * pitch.cos(),
        pitch.sin(),
        -yaw.cos() * pitch.cos(),
    )
}

fn parse_args<I>(args: I) -> anyhow::Result<Options>
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
            "--help" | "-h" => {
                println!(
                    "snapshot [--seed N] [--pos X,Y,Z] [--yaw R] [--pitch R] [--radius N] [--size WxH] [--out PATH] [--edits \"set X,Y,Z,ID; ...\"] [--mesher culled|greedy]"
                );
                std::process::exit(0);
            }
            _ => anyhow::bail!("unknown argument: {flag}"),
        }
    }
    Ok(options)
}

impl MesherMode {
    fn name(self) -> &'static str {
        match self {
            Self::Culled => "culled",
            Self::Greedy => "greedy",
        }
    }

    fn mesh(self, padded: &voxelforge::world::chunk::PaddedChunk) -> ChunkMeshes {
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
        if id > COBBLE {
            anyhow::bail!("edit block id must be between {AIR} and {COBBLE}: {id}");
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
