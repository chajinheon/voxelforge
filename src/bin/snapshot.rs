//! Surface-free M1 terrain snapshot renderer.

use std::path::PathBuf;
use std::time::Instant;

use glam::{IVec3, Vec3};
use voxelforge::render::{
    DAY_LENGTH_SECONDS, Globals, Gpu, GpuChunkMeshes, OffscreenTarget, RenderView, Renderer,
    day_state,
};
use voxelforge::stream::Streamer;
use voxelforge::world::block::{AIR, BlockId, TORCH};
use voxelforge::world::coords::{CHUNK_SIZE, WORLD_CHUNKS_Y, chunk_of};
use voxelforge::world::r#gen::WorldGen;
use voxelforge::world::world::World;

#[path = "snapshot/args.rs"]
mod args;
#[path = "snapshot/fixtures.rs"]
mod fixtures;
#[path = "snapshot/ui.rs"]
mod snapshot_ui;
use args::parse_args;
use fixtures::{Fixture, View};

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
    fixture: Fixture,
    view: View,
    day_phase: Option<f32>,
    world_time: Option<f32>,
    preset: voxelforge::render::RenderPreset,
    render_scale: Option<f32>,
    fixed_exposure: Option<f32>,
    warmup: u32,
    frames: u32,
    timings: Option<PathBuf>,
    ui: args::UiMode,
    inventory_category: args::InventoryCategory,
    inventory_query: String,
    hand_action: args::HandActionSpec,
    held_item: u16,
    gi: args::GiMode,
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_core=warn,naga=warn"),
    )
    .init();

    let options = parse_args(std::env::args().skip(1))?;
    let gpu = Gpu::new()?;
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8UnormSrgb)?;
    renderer.configure_gpu_gi(&gpu.adapter, options.width, options.height);
    renderer.configure_preset(options.preset, options.render_scale);
    renderer.configure_exposure(options.fixed_exposure);
    renderer.configure_gpu_timing(options.timings.is_some());
    renderer.configure_gi(matches!(options.gi, args::GiMode::On));

    let generator = WorldGen::new(options.seed);
    let (fixture_position, fixture_yaw, fixture_pitch) = fixtures::camera(options.fixture);
    let position = options.pos.unwrap_or_else(|| {
        if matches!(options.fixture, Fixture::Terrain) {
            Vec3::new(0.0, generator.height_at(0, 0) as f32 + 3.0, 0.0)
        } else {
            fixture_position
        }
    });
    let custom_fixture = !matches!(options.fixture, Fixture::Terrain);
    let yaw = if custom_fixture && options.pos.is_none() {
        fixture_yaw
    } else {
        options.yaw
    };
    let pitch = if custom_fixture && options.pos.is_none() {
        fixture_pitch
    } else {
        options.pitch
    };
    let target = OffscreenTarget::new(&gpu.device, options.width, options.height)?;
    log::info!("snapshot: mesher={}", options.mesher.name());
    let chunks = build_terrain(
        &mut renderer,
        options.seed,
        position,
        options.radius,
        &options.edits,
        options.mesher,
        options.fixture,
        matches!(options.gi, args::GiMode::On),
    )?;
    let world_time = options
        .world_time
        .or_else(|| options.day_phase.map(|phase| phase * DAY_LENGTH_SECONDS))
        .unwrap_or({
            if matches!(options.fixture, Fixture::Materials) {
                900.0
            } else {
                0.0
            }
        });
    let globals = make_globals(
        position,
        yaw,
        pitch,
        options.width,
        options.height,
        world_time,
    );
    let snapshot_ui = snapshot_ui::SnapshotUi::new(&options);
    let render_view = match options.view {
        View::Final => RenderView::Final,
        View::Light => RenderView::Light,
        View::Albedo => RenderView::Albedo,
        View::Normal => RenderView::Normal,
        View::Depth => RenderView::Depth,
        View::Material => RenderView::Material,
        View::Motion => RenderView::Motion,
        View::Reactive => RenderView::Reactive,
        View::Water => RenderView::Water,
        View::Volumetric => RenderView::Volumetric,
        View::Cloud => RenderView::Cloud,
        View::Lod => RenderView::Lod,
        View::Gi => RenderView::Gi,
        View::Clipmap => RenderView::Clipmap,
    };
    let mut timings = Vec::with_capacity(options.warmup as usize + options.frames as usize);
    let mut gpu_timings = Vec::with_capacity(options.frames as usize);
    let total_frames = options.warmup.saturating_add(options.frames).max(1);
    for frame in 0..total_frames {
        let started = Instant::now();
        if let Some(ui) = snapshot_ui.frame(options.width, options.height) {
            renderer.render_view_with_ui(
                &target.color_view,
                &target.depth_view,
                &globals,
                &chunks,
                render_view,
                &ui,
            );
        } else {
            renderer.render_view(
                &target.color_view,
                &target.depth_view,
                &globals,
                &chunks,
                render_view,
            );
        }
        timings.push(started.elapsed().as_secs_f64() * 1000.0);
        if options.timings.is_some() {
            gpu.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })?;
            renderer.poll_gpu_timings();
        }
        while let Some(timing) = renderer.take_last_gpu_timings() {
            // Readback completion lags rendering by an adapter-dependent
            // number of frames, so retain the rendered frame index instead
            // of treating this sparse stream as a dense frame array.
            gpu_timings.push((timing.rendered_frame_index() as usize, timing));
        }
        if frame + 1 == total_frames {
            log::debug!("snapshot: last frame rendered");
        }
    }
    let pixels = target.read_pixels(&gpu.device, &gpu.queue)?;
    image::save_buffer_with_format(
        &options.out,
        &pixels,
        options.width,
        options.height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )?;
    if options.fixture == Fixture::Shapes && options.view == View::Final {
        fixtures::write_shape_probes(&pixels, options.width, options.height)?;
    }
    if let Some(path) = options.timings {
        write_timings(
            &path,
            options.warmup,
            &timings,
            &gpu_timings,
            renderer.gpu_timing_supported(),
        )?;
    }
    log::info!(
        "snapshot: wrote {} ({}x{}, {} chunks)",
        options.out.display(),
        options.width,
        options.height,
        chunks.len()
    );
    Ok(())
}

fn percentile(values: &[f64], numerator: usize, denominator: usize) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted
        .get((sorted.len().saturating_sub(1) * numerator) / denominator.max(1))
        .copied()
        .unwrap_or(0.0)
}

fn timing_value(values: &[f64]) -> serde_json::Value {
    if values.is_empty() {
        return serde_json::json!({
            "samples": 0,
            "median_ms": null,
            "p95_ms": null,
            "max_ms": null,
        });
    }
    serde_json::json!({
        "samples": values.len(),
        "median_ms": percentile(values, 50, 100),
        "p95_ms": percentile(values, 95, 100),
        "max_ms": values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    })
}

fn post_warmup_gpu_frames(
    warmup: u32,
    gpu_frames: &[(usize, voxelforge::render::gpu_timing::GpuFrameTimings)],
) -> Vec<voxelforge::render::gpu_timing::GpuFrameTimings> {
    gpu_frames
        .iter()
        .filter(|(frame, _)| *frame >= warmup as usize)
        .map(|(_, timing)| *timing)
        .collect()
}

fn write_timings(
    path: &std::path::Path,
    warmup: u32,
    cpu_frames: &[f64],
    gpu_frames: &[(usize, voxelforge::render::gpu_timing::GpuFrameTimings)],
    gpu_supported: bool,
) -> anyhow::Result<()> {
    let start = warmup.min(cpu_frames.len() as u32) as usize;
    let cpu = &cpu_frames[start..];
    let gpu = post_warmup_gpu_frames(warmup, gpu_frames);
    let mut object = serde_json::Map::new();
    object.insert("frames".into(), serde_json::json!(cpu.len()));
    object.insert("warmup".into(), serde_json::json!(warmup));
    object.insert("gpu_supported".into(), serde_json::json!(gpu_supported));
    object.insert("cpu_frame".into(), timing_value(cpu));
    for pass in voxelforge::render::gpu_timing::TimedPass::ALL {
        let values = gpu
            .iter()
            .filter_map(|frame| frame.has_sample(pass).then_some(frame.ms(pass)))
            .collect::<Vec<_>>();
        object.insert(pass.label().into(), timing_value(&values));
    }
    let total = gpu.iter().map(|frame| frame.total_ms()).collect::<Vec<_>>();
    object.insert("total_gpu".into(), timing_value(&total));
    let json = serde_json::Value::Object(object);
    std::fs::write(path, serde_json::to_vec_pretty(&json)?)?;
    Ok(())
}

fn build_terrain(
    renderer: &mut Renderer,
    seed: u64,
    position: Vec3,
    radius: i32,
    edits: &[Edit],
    mesher: MesherMode,
    fixture: Fixture,
    populate_gi: bool,
) -> anyhow::Result<Vec<GpuChunkMeshes>> {
    let center = chunk_of(IVec3::new(
        position.x.floor() as i32,
        position.y.floor() as i32,
        position.z.floor() as i32,
    ));
    let mut world = if matches!(fixture, Fixture::Terrain) {
        let mut world = World::new(seed);
        for z in center.z - radius..=center.z + radius {
            for x in center.x - radius..=center.x + radius {
                for y in 0..WORLD_CHUNKS_Y {
                    world.ensure_loaded(IVec3::new(x, y, z));
                }
            }
        }
        world
    } else {
        fixtures::build(seed, fixture)
    };

    for edit in edits {
        if let EditOutcome::Noop(warning) = apply_edit(&mut world, *edit)? {
            log::warn!("{warning}");
        }
    }

    let mut streamer = Streamer::new();
    streamer
        .solve_lighting_sync(&mut world)
        .map_err(anyhow::Error::msg)?;
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
        vertex_count += meshes.opaque.vertices.len()
            + meshes.translucent.vertices.len()
            + meshes.water.vertices.len();
        index_count += meshes.opaque.indices.len()
            + meshes.translucent.indices.len()
            + meshes.water.indices.len();
        if !(meshes.opaque.vertices.is_empty()
            && meshes.translucent.vertices.is_empty()
            && meshes.water.vertices.is_empty())
        {
            gpu_chunks.push(renderer.upload_chunk_meshes(&meshes, cp * 32)?);
        }
    }
    log::info!(
        "snapshot: mesher={} vertices {vertex_count} indices {index_count}",
        mesher.name()
    );
    if populate_gi {
        streamer.populate_gi_offline(&world, renderer, position);
    }
    let mut local_lights = Vec::new();
    let extent = (radius.max(1) + 1) * CHUNK_SIZE;
    for z in position.z.floor() as i32 - extent..=position.z.floor() as i32 + extent {
        for y in 0..CHUNK_SIZE * WORLD_CHUNKS_Y {
            for x in position.x.floor() as i32 - extent..=position.x.floor() as i32 + extent {
                let id = world.get_block(IVec3::new(x, y, z));
                let emission = voxelforge::world::block::def(id).emission_rgb;
                if emission.iter().any(|value| *value > 0.0) {
                    local_lights.push(voxelforge::render::volumetric::FogLight {
                        position: glam::Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5),
                        color: glam::Vec3::from_array(emission),
                        radius: 14.0,
                    });
                }
            }
        }
    }
    let selected = voxelforge::render::volumetric::closest_lights(position, &local_lights);
    renderer.set_m8_local_lights(&selected);
    if matches!(fixture, Fixture::Lod) {
        streamer.populate_lod_rings_offline(&world, renderer, position);
    }
    Ok(gpu_chunks)
}

#[derive(Debug, PartialEq, Eq)]
enum EditOutcome {
    Changed,
    Noop(String),
}

fn apply_edit(world: &mut World, edit: Edit) -> anyhow::Result<EditOutcome> {
    if edit.position.y < 0 || edit.position.y >= CHUNK_SIZE * WORLD_CHUNKS_Y {
        anyhow::bail!(
            "edit position {:?} is outside the loaded world or vertical bounds",
            edit.position
        );
    }
    let cp = chunk_of(edit.position);
    if !world.is_loaded(cp) {
        anyhow::bail!(
            "edit position {:?} is outside the loaded world or vertical bounds",
            edit.position
        );
    }
    let existing = world.get_block(edit.position);
    if existing == edit.id {
        return Ok(EditOutcome::Noop(format!(
            "snapshot edit at {:?} is a no-op; existing id {}",
            edit.position, existing
        )));
    }
    if !world.set_block(edit.position, edit.id) {
        anyhow::bail!(
            "edit position {:?} is outside the loaded world or vertical bounds",
            edit.position
        );
    }
    Ok(EditOutcome::Changed)
}

fn make_globals(
    position: Vec3,
    yaw: f32,
    pitch: f32,
    width: u32,
    height: u32,
    world_time: f32,
) -> Globals {
    let view = glam::camera::rh::view::look_to_mat4(position, forward(yaw, pitch), Vec3::Y);
    let projection = glam::camera::rh::proj::directx::perspective(
        FOV_Y,
        width.max(1) as f32 / height.max(1) as f32,
        NEAR,
        FAR,
    );
    Globals::from_day(
        projection * view,
        position,
        world_time,
        width as f32,
        height as f32,
        day_state(world_time),
    )
}

fn forward(yaw: f32, pitch: f32) -> Vec3 {
    Vec3::new(
        yaw.sin() * pitch.cos(),
        pitch.sin(),
        -yaw.cos() * pitch.cos(),
    )
}

#[cfg(test)]
#[path = "snapshot/tests.rs"]
mod tests;
