//! Windowed Voxelforge entrypoint for the M3 building milestone.

use std::sync::Arc;
use std::time::Instant;

use glam::{IVec3, Vec3};
use voxelforge::player::camera::EYE_HEIGHT;
use voxelforge::player::{
    Body, Camera, Controller, place_rejected_inside_player_aabb, safe_spawn, step,
};
use voxelforge::render::{Globals, GpuChunkMeshes, Renderer};
use voxelforge::stream::Streamer;
use voxelforge::world::block::{AIR, HOTBAR, WATER, def};
use voxelforge::world::coords::{WORLD_CHUNKS_Y, chunk_of};
use voxelforge::world::raycast::{RayHit, raycast};
use voxelforge::world::save::{PlayerMeta, SaveDir, WorldMeta};
use voxelforge::world::world::World;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

mod app_config;
mod window_gpu;
use app_config::save_name;
use window_gpu::WindowGpu;

const WINDOW_WIDTH: f64 = 1280.0;
const WINDOW_HEIGHT: f64 = 720.0;
const DEFAULT_SEED: u64 = 1;
const DEFAULT_YAW: f32 = 0.6;
const DEFAULT_PITCH: f32 = -0.3;
const FOV_Y: f32 = 60.0_f32.to_radians();
const NEAR: f32 = 0.05;
const FAR: f32 = 1000.0;

struct App {
    gpu: Option<WindowGpu>,
    world: World,
    body: Body,
    camera: Camera,
    controller: Controller,
    renderer: Option<Renderer>,
    chunks: Vec<GpuChunkMeshes>,
    streamer: Streamer,
    last_frame: Instant,
    started: Instant,
    title_at: Instant,
    title_frames: u32,
    title_max_ms: f64,
    drawn_chunks: usize,
    cursor_locked: bool,
    selected_hotbar: usize,
    ray_hit: Option<RayHit>,
    debug_stats: bool,
    wheel_remainder: f64,
    frames: u32,
    smoke_frames: Option<u32>,
}

impl App {
    fn new(smoke_frames: Option<u32>, save_dir: SaveDir) -> Self {
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
        Self {
            gpu: None,
            world,
            body: Body {
                pos: body_pos,
                fly,
                ..Body::default()
            },
            camera,
            controller: Controller::new(),
            renderer: None,
            chunks: Vec::new(),
            streamer: Streamer::new_with_save(save_dir),
            last_frame: now,
            started: now,
            title_at: now,
            title_frames: 0,
            title_max_ms: 0.0,
            drawn_chunks: 0,
            cursor_locked: false,
            selected_hotbar: 0,
            ray_hit: None,
            debug_stats: smoke_frames.is_some(),
            wheel_remainder: 0.0,
            frames: 0,
            smoke_frames,
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
    }

    fn globals(&self) -> Globals {
        let gpu = self.gpu.as_ref();
        let (width, height) = gpu
            .map(|gpu| (gpu.config.width, gpu.config.height))
            .unwrap_or((1, 1));
        Globals::new(
            self.camera.proj(width.max(1) as f32 / height.max(1) as f32) * self.camera.view(),
            self.camera.pos,
            Vec3::new(-0.4, -1.0, -0.3).normalize(),
            self.started.elapsed().as_secs_f32(),
            width as f32,
            height as f32,
        )
    }

    fn update_title(&mut self) {
        let elapsed = self.title_at.elapsed();
        if elapsed.as_secs_f32() < 1.0 {
            return;
        }
        let fps = self.title_frames as f64 / elapsed.as_secs_f64();
        let selected_name = def(HOTBAR[self.selected_hotbar]).name;
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

    fn cycle_hotbar(&mut self, steps: i32) {
        let count = HOTBAR.len() as i32;
        self.selected_hotbar = (self.selected_hotbar as i32 + steps).rem_euclid(count) as usize;
    }

    fn scroll_hotbar(&mut self, delta: MouseScrollDelta) {
        let amount = match delta {
            MouseScrollDelta::LineDelta(_, y) => f64::from(y),
            MouseScrollDelta::PixelDelta(position) => position.y / 40.0,
        };
        self.wheel_remainder += amount;
        let steps = self.wheel_remainder.trunc() as i32;
        if steps != 0 {
            self.wheel_remainder -= f64::from(steps);
            self.cycle_hotbar(steps);
        }
    }

    fn edit_with_button(&mut self, button: MouseButton) {
        let Some(hit) = self.ray_hit else {
            return;
        };
        let changed = match button {
            MouseButton::Left => self.world.set_block(hit.block, AIR),
            MouseButton::Right => {
                let target = hit.block + hit.normal;
                let target_id = self.world.get_block(target);
                if (target_id == AIR || target_id == WATER)
                    && !place_rejected_inside_player_aabb(target, self.camera.pos)
                {
                    self.world.set_block(target, HOTBAR[self.selected_hotbar])
                } else {
                    false
                }
            }
            _ => false,
        };
        if changed {
            for cp in self.world.take_dirty() {
                self.streamer.mark_urgent(cp);
            }
        }
    }

    fn unlock_cursor(&mut self) {
        if let Some(gpu) = self.gpu.as_ref() {
            if let Err(error) = gpu.window.set_cursor_grab(CursorGrabMode::None) {
                log::warn!("release cursor failed: {error}");
            }
            gpu.window.set_cursor_visible(true);
        }
        self.cursor_locked = false;
        self.controller.clear();
    }

    fn lock_cursor(&mut self) {
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        let result = gpu
            .window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| gpu.window.set_cursor_grab(CursorGrabMode::Confined));
        match result {
            Ok(()) => {
                gpu.window.set_cursor_visible(false);
                self.cursor_locked = true;
            }
            Err(error) => log::warn!("grab cursor failed: {error}"),
        }
    }

    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        let input = self.controller.update(&mut self.camera);
        step(&self.world, &mut self.body, input, dt);
        self.camera.pos = self.body.pos + Vec3::Y * EYE_HEIGHT;
        self.stream();
        self.ray_hit = raycast(&self.world, self.camera.pos, self.camera.forward(), 6.0);
        let globals = self.globals();
        let rendered = match (self.gpu.as_ref(), self.renderer.as_mut()) {
            (Some(gpu), Some(renderer)) => gpu.render(
                renderer,
                &globals,
                &self.chunks,
                self.ray_hit.map(|hit| hit.block),
            ),
            _ => None,
        };
        let frame_ms = now.elapsed().as_secs_f64() * 1000.0;
        self.title_max_ms = self.title_max_ms.max(frame_ms);
        if let Some(drawn) = rendered {
            self.drawn_chunks = drawn;
            self.title_frames = self.title_frames.saturating_add(1);
            self.frames = self.frames.saturating_add(1);
            if self.smoke_frames.is_some_and(|limit| self.frames >= limit) {
                self.smoke_frames = None;
                log::info!("smoke: rendered {} frames, exiting", self.frames);
                self.save();
                event_loop.exit();
                return;
            }
        }
        self.update_title();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("voxelforge | starting")
            .with_inner_size(winit::dpi::LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                log::error!("create window failed: {error}");
                event_loop.exit();
                return;
            }
        };
        let gpu = match WindowGpu::new(window) {
            Ok(gpu) => gpu,
            Err(error) => {
                log::error!("initialize renderer failed: {error:#}");
                event_loop.exit();
                return;
            }
        };
        let renderer = match Renderer::new(&gpu.device, &gpu.queue, gpu.config.format) {
            Ok(renderer) => renderer,
            Err(error) => {
                log::error!("initialize renderer failed: {error:#}");
                event_loop.exit();
                return;
            }
        };
        self.gpu = Some(gpu);
        self.renderer = Some(renderer);
        let center = self.center_chunk();
        self.streamer.bootstrap(
            &mut self.world,
            self.renderer.as_mut().expect("renderer initialized"),
            &mut self.chunks,
            center,
        );
        self.last_frame = Instant::now();
        if let Some(gpu) = self.gpu.as_ref() {
            gpu.window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.save();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(size.width, size.height);
                }
            }
            WindowEvent::Focused(false) => self.unlock_cursor(),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => {
                if !self.cursor_locked {
                    self.lock_cursor();
                } else {
                    self.edit_with_button(button);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => self.scroll_hotbar(delta),
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let pressed = event.state == ElementState::Pressed;
                if code == KeyCode::Escape && pressed && !event.repeat {
                    if self.cursor_locked {
                        self.unlock_cursor();
                    } else {
                        self.save();
                        event_loop.exit();
                    }
                    return;
                }
                if pressed && !event.repeat {
                    match code {
                        KeyCode::Digit1 => self.selected_hotbar = 0,
                        KeyCode::Digit2 => self.selected_hotbar = 1,
                        KeyCode::Digit3 => self.selected_hotbar = 2,
                        KeyCode::Digit4 => self.selected_hotbar = 3,
                        KeyCode::Digit5 => self.selected_hotbar = 4,
                        KeyCode::Digit6 => self.selected_hotbar = 5,
                        KeyCode::Digit7 => self.selected_hotbar = 6,
                        KeyCode::Digit8 => self.selected_hotbar = 7,
                        KeyCode::Digit9 => self.selected_hotbar = 8,
                        KeyCode::F3 => {
                            self.debug_stats = !self.debug_stats;
                            log::info!(
                                "console stats: {}",
                                if self.debug_stats { "on" } else { "off" }
                            );
                        }
                        KeyCode::KeyF => {
                            self.body.fly = !self.body.fly;
                            if self.body.fly {
                                self.body.vel.y = 0.0;
                            }
                            log::info!(
                                "movement mode: {}",
                                if self.body.fly { "fly" } else { "walk" }
                            );
                        }
                        _ => {}
                    }
                }
                if code == KeyCode::KeyR
                    && pressed
                    && !event.repeat
                    && let Some(renderer) = self.renderer.as_mut()
                {
                    renderer.force_shader_reload();
                }
                self.controller.set_key(code, pressed);
            }
            WindowEvent::RedrawRequested => self.frame(event_loop),
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if self.cursor_locked
            && let DeviceEvent::MouseMotion { delta: (dx, dy) } = event
        {
            self.controller.mouse_motion(dx, dy);
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(gpu) = self.gpu.as_ref() {
            gpu.window.request_redraw();
        }
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn"),
    )
    .init();
    let smoke_frames = std::env::var("VF_SMOKE_FRAMES")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0);
    let save_dir = SaveDir::open(&save_name()?)?;
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(smoke_frames, save_dir);
    event_loop.run_app(&mut app)?;
    Ok(())
}
