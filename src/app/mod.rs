//! Windowed Voxelforge entrypoint for the M3 building milestone.
use super::app_config::RuntimeOverrides;
use crate::window_gpu::WindowGpu;
use glam::{IVec3, Vec3};
use std::path::PathBuf;
use std::time::Instant;
use voxelforge::audio::{AudioOutput, MovementAudio, event_seed, generate};
use voxelforge::config::Settings;
use voxelforge::game_clock::GameClock;
use voxelforge::player::camera::EYE_HEIGHT;
use voxelforge::player::place::HoldRepeat;
use voxelforge::player::{Body, Camera, Controller, safe_spawn};
use voxelforge::render::RenderView;
use voxelforge::render::ui::catalog_records;
use voxelforge::render::{Globals, GpuChunkMeshes, Renderer, day_state};
use voxelforge::shaderpack::ShaderPack;
use voxelforge::stream::Streamer;
use voxelforge::ui::{CreativeInventory, ItemRecord, ViewModelState};
use voxelforge::world::catalog::{self, DEFAULT_HOTBAR_ITEMS};
use voxelforge::world::coords::{WORLD_CHUNKS_Y, chunk_of};
use voxelforge::world::raycast::RayHit;
use voxelforge::world::save::{PlayerMeta, SaveDir, WorldMeta};
use voxelforge::world::world::World;
mod benchmark;
mod events;
mod events_loop;
mod input;
mod inventory;
mod memory;
const WINDOW_WIDTH: f64 = 1280.0;
const WINDOW_HEIGHT: f64 = 720.0;
const DEFAULT_SEED: u64 = 1;
const DEFAULT_YAW: f32 = 0.6;
const DEFAULT_PITCH: f32 = -0.3;
const FOV_Y: f32 = 60.0_f32.to_radians();
const NEAR: f32 = 0.05;
const FAR: f32 = 1000.0;
pub struct App {
    gpu: Option<WindowGpu>,
    world: World,
    body: Body,
    camera: Camera,
    controller: Controller,
    renderer: Option<Renderer>,
    chunks: Vec<GpuChunkMeshes>,
    streamer: Streamer,
    last_frame: Instant,
    title_at: Instant,
    title_frames: u32,
    title_max_ms: f64,
    drawn_chunks: usize,
    cursor_locked: bool,
    inventory: CreativeInventory,
    settings: Settings,
    settings_path: Option<PathBuf>,
    transient_overrides: RuntimeOverrides,
    clock: GameClock,
    paused: bool,
    settings_open: bool,
    settings_row: usize,
    audio: AudioOutput,
    hold_repeat: HoldRepeat,
    lmb_down: bool,
    rmb_down: bool,
    render_view: RenderView,
    ray_hit: Option<RayHit>,
    debug_stats: bool,
    wheel_remainder: f64,
    frames: u32,
    smoke_frames: Option<u32>,
    autopilot: bool,
    viewmodel: ViewModelState,
    ui_records: Vec<ItemRecord>,
    cursor_position: (f64, f64),
    shader_pack: Option<ShaderPack>,
    benchmark: bool,
    frame_times_ms: Vec<f64>,
    gpu_times_ms: Vec<f64>,
    frame_phase_times_ms: [Vec<f64>; 6],
    gpu_phase_times_ms: [Vec<f64>; 6],
    allocation_stats_enabled: bool,
    allocation_samples: Vec<u32>,
    settled_seconds: Option<f64>,
    autopilot_phase: u32,
    interaction_id: u64,
    movement_audio: MovementAudio,
    control_down: bool,
}
impl App {
    pub fn new(
        smoke_frames: Option<u32>,
        save_dir: SaveDir,
        mut settings: Settings,
        settings_path: Option<PathBuf>,
        shader_pack: Option<ShaderPack>,
        transient_overrides: RuntimeOverrides,
    ) -> Self {
        settings.clamp_and_warn();
        let saved_meta = match save_dir.read_meta() {
            Ok(Some(meta)) if meta.version == 1 => Some(meta),
            Ok(Some(meta)) => {
                log::warn!(
                    "unsupported world metadata version {}; using defaults",
                    meta.version
                );
                None
            }
            Ok(None) => None,
            Err(error) => {
                log::warn!("read world metadata failed ({error:#}); using defaults");
                None
            }
        };
        let seed = saved_meta.as_ref().map_or(DEFAULT_SEED, |meta| meta.seed);
        let world = World::new(seed);
        let generator = world.generator().clone();
        let spawn = safe_spawn(&generator);
        let (body_pos, yaw, pitch, fly) =
            saved_meta
                .as_ref()
                .map_or((spawn, DEFAULT_YAW, DEFAULT_PITCH, false), |meta| {
                    (
                        Vec3::from_array(meta.player.pos),
                        meta.player.yaw,
                        meta.player.pitch,
                        meta.player.fly,
                    )
                });
        let camera = Camera {
            pos: body_pos + Vec3::Y * EYE_HEIGHT,
            yaw,
            pitch,
            fov_y: FOV_Y,
            near: NEAR,
            far: FAR,
        };
        let now = Instant::now();
        let mut audio = AudioOutput::new();
        audio.set_master_volume(settings.audio.master * settings.audio.effects);
        let benchmark = std::env::var("VF_BENCH_FRAMES")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .is_some();
        let benchmark_samples = smoke_frames.unwrap_or_default().saturating_sub(600) as usize;
        let allocation_stats_enabled =
            std::env::var("VF_ALLOC_STATS").is_ok_and(|value| value == "1");
        Self {
            gpu: None,
            world,
            body: Body {
                pos: body_pos,
                fly,
                ..Body::default()
            },
            camera,
            controller: {
                let mut controller = Controller::new();
                controller.configure_look(
                    settings.controls.mouse_sensitivity,
                    settings.controls.invert_y,
                );
                controller
            },
            renderer: None,
            chunks: Vec::new(),
            streamer: Streamer::new_with_save_and_radius(save_dir, settings.video.view_radius),
            last_frame: now,
            title_at: now,
            title_frames: 0,
            title_max_ms: 0.0,
            drawn_chunks: 0,
            cursor_locked: false,
            inventory: CreativeInventory {
                hotbar: if settings.creative.hotbar == [0; 9] {
                    DEFAULT_HOTBAR_ITEMS
                } else {
                    settings.creative.hotbar
                },
                selected_slot: settings.creative.selected_slot,
                ..CreativeInventory::default()
            },
            settings,
            settings_path,
            transient_overrides,
            clock: GameClock::new(),
            paused: false,
            settings_open: false,
            settings_row: 0,
            audio,
            hold_repeat: HoldRepeat::default(),
            lmb_down: false,
            rmb_down: false,
            render_view: RenderView::Final,
            ray_hit: None,
            debug_stats: smoke_frames.is_some(),
            wheel_remainder: 0.0,
            frames: 0,
            smoke_frames,
            autopilot: std::env::var("VF_AUTOPILOT").is_ok(),
            viewmodel: ViewModelState::new(DEFAULT_HOTBAR_ITEMS[0]),
            ui_records: catalog_records(),
            cursor_position: (0.0, 0.0),
            shader_pack,
            benchmark,
            frame_times_ms: Vec::with_capacity(benchmark_samples),
            gpu_times_ms: Vec::with_capacity(benchmark_samples),
            frame_phase_times_ms: std::array::from_fn(|_| {
                Vec::with_capacity(benchmark_samples / 5)
            }),
            gpu_phase_times_ms: std::array::from_fn(|_| Vec::with_capacity(benchmark_samples / 5)),
            allocation_stats_enabled,
            allocation_samples: Vec::with_capacity(benchmark_samples),
            settled_seconds: None,
            autopilot_phase: u32::MAX,
            interaction_id: 0,
            movement_audio: MovementAudio::new(),
            control_down: false,
        }
    }
    fn center_chunk(&self) -> IVec3 {
        let block = IVec3::new(
            self.camera.pos.x.floor() as i32,
            self.camera.pos.y.floor() as i32,
            self.camera.pos.z.floor() as i32,
        );
        let mut center = chunk_of(block);
        center.y = center.y.clamp(0, WORLD_CHUNKS_Y - 1);
        center
    }
    fn stream(&mut self) {
        let center = self.center_chunk();
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        self.streamer.update(
            &mut self.world,
            renderer,
            &mut self.chunks,
            center,
            self.camera.pos,
            self.drawn_chunks,
        );
    }
    fn save(&mut self) {
        self.streamer.save_now(&mut self.world);
        let Some(save) = self.streamer.save_dir() else {
            return;
        };
        let meta = WorldMeta {
            version: 1,
            seed: self.world.seed(),
            player: PlayerMeta {
                pos: self.body.pos.to_array(),
                yaw: self.camera.yaw,
                pitch: self.camera.pitch,
                fly: self.body.fly,
            },
        };
        if let Err(error) = save.write_meta(&meta) {
            log::error!("save world metadata failed: {error:#}");
        }
        self.settings.creative.hotbar = self.inventory.hotbar;
        self.settings.creative.selected_slot = self.inventory.selected_slot;
        if let Some(path) = self.settings_path.as_deref() {
            let mut persisted = self.settings.clone();
            self.transient_overrides.restore_persisted(&mut persisted);
            if let Err(error) = persisted.write_atomic(path) {
                log::warn!("write settings failed: {error:#}");
            }
        }
    }

    fn globals(&self) -> Globals {
        let gpu = self.gpu.as_ref();
        let (width, height) = gpu
            .map(|gpu| (gpu.config.width, gpu.config.height))
            .unwrap_or((1, 1));
        let world_time = self.clock.seconds();
        Globals::from_day(
            self.camera.proj(width.max(1) as f32 / height.max(1) as f32) * self.camera.view(),
            self.camera.pos,
            world_time,
            width as f32,
            height as f32,
            day_state(world_time),
        )
    }

    fn update_title(&mut self) {
        let elapsed = self.title_at.elapsed();
        if elapsed.as_secs_f32() < 1.0 {
            return;
        }
        let fps = self.title_frames as f64 / elapsed.as_secs_f64();
        let selected_name =
            catalog::item(self.inventory.hotbar[self.inventory.selected_slot as usize])
                .map_or("empty", |item| item.name);
        let stats = format!(
            "{fps:.0} fps | max {:.1}ms | pos {:.1} {:.1} {:.1} | chunks {} drawn {} | meshq {} | selected {selected_name}",
            self.title_max_ms,
            self.camera.pos.x,
            self.camera.pos.y,
            self.camera.pos.z,
            self.world.chunk_count(),
            self.drawn_chunks,
            self.streamer.queue_len(),
        );
        if let Some(gpu) = self.gpu.as_ref() {
            gpu.window.set_title(&format!("voxelforge | {stats}"));
        }
        if self.debug_stats {
            log::info!("stats: {stats}");
        }
        self.title_at = Instant::now();
        self.title_frames = 0;
        self.title_max_ms = 0.0;
    }

    fn emit_movement_audio(&mut self, previous_pos: Vec3, sprinting: bool) {
        let delta = self.body.pos - previous_pos;
        let horizontal_distance = Vec3::new(delta.x, 0.0, delta.z).length();
        let ground_block = self
            .world
            .get_block((self.body.pos - Vec3::Y * 0.05).floor().as_ivec3());
        let cues = self.movement_audio.update(
            horizontal_distance,
            self.body.on_ground,
            self.body.fly,
            self.body.in_water,
            ground_block,
            sprinting,
            self.paused || self.inventory.open,
        );
        for cue in cues {
            self.interaction_id = self.interaction_id.wrapping_add(1).max(1);
            let pcm = generate(
                cue.effect,
                event_seed(self.world.seed(), self.interaction_id, cue.block_id),
            );
            let _ = self
                .audio
                .play_with_gain(&pcm, 0.0, cue.kind, self.frames as u64, cue.gain);
        }
    }
}
