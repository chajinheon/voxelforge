//! Windowed Voxelforge entrypoint for the M3 building milestone.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use glam::{IVec3, Vec3};
use voxelforge::mesh::mesh_chunk;
use voxelforge::player::camera::EYE_HEIGHT;
use voxelforge::player::{Camera, Controller, place_rejected_inside_player_aabb};
use voxelforge::render::{Globals, GpuChunk, Renderer};
use voxelforge::world::block::{AIR, HOTBAR, WATER, def};
use voxelforge::world::coords::{CHUNK_SIZE, WORLD_CHUNKS_Y, chunk_of};
use voxelforge::world::r#gen::WorldGen;
use voxelforge::world::raycast::{RayHit, raycast};
use voxelforge::world::world::World;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

mod window_gpu;
use window_gpu::WindowGpu;

const WINDOW_WIDTH: f64 = 1280.0;
const WINDOW_HEIGHT: f64 = 720.0;
const STREAM_RADIUS: i32 = 6;
const UNLOAD_RADIUS: i32 = STREAM_RADIUS + 2;
const LOAD_BUDGET: usize = 4;
const MESH_BUDGET: usize = 8;
const DEFAULT_SEED: u64 = 1;
const DEFAULT_YAW: f32 = 0.6;
const DEFAULT_PITCH: f32 = -0.3;
const FOV_Y: f32 = 60.0_f32.to_radians();
const NEAR: f32 = 0.05;
const FAR: f32 = 1000.0;

struct App {
    gpu: Option<WindowGpu>,
    world: World,
    camera: Camera,
    controller: Controller,
    renderer: Option<Renderer>,
    chunks: Vec<GpuChunk>,
    mesh_queue: HashMap<IVec3, bool>,
    last_frame: Instant,
    started: Instant,
    title_at: Instant,
    title_frames: u32,
    title_max_ms: f64,
    cursor_locked: bool,
    selected_hotbar: usize,
    ray_hit: Option<RayHit>,
    debug_stats: bool,
    wheel_remainder: f64,
    frames: u32,
    smoke_frames: Option<u32>,
}

impl App {
    fn new(smoke_frames: Option<u32>) -> Self {
        let world = World::new(DEFAULT_SEED);
        let generator = WorldGen::new(DEFAULT_SEED);
        let camera = Camera {
            pos: Vec3::new(
                0.0,
                generator.height_at(0, 0) as f32 + EYE_HEIGHT + 1.0,
                0.0,
            ),
            yaw: DEFAULT_YAW,
            pitch: DEFAULT_PITCH,
            fov_y: FOV_Y,
            near: NEAR,
            far: FAR,
        };
        let now = Instant::now();
        Self {
            gpu: None,
            world,
            camera,
            controller: Controller::new(),
            renderer: None,
            chunks: Vec::new(),
            mesh_queue: HashMap::new(),
            last_frame: now,
            started: now,
            title_at: now,
            title_frames: 0,
            title_max_ms: 0.0,
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
        let mut desired =
            Vec::with_capacity(((STREAM_RADIUS * 2 + 1).pow(2) * WORLD_CHUNKS_Y) as usize);
        for z in -STREAM_RADIUS..=STREAM_RADIUS {
            for x in -STREAM_RADIUS..=STREAM_RADIUS {
                for y in 0..WORLD_CHUNKS_Y {
                    desired.push(IVec3::new(center.x + x, y, center.z + z));
                }
            }
        }
        desired.sort_by_key(|cp| {
            let dx = cp.x - center.x;
            let dz = cp.z - center.z;
            (dx * dx + dz * dz, (cp.y - center.y).abs(), cp.y)
        });
        let mut loaded = 0;
        for cp in desired {
            if loaded == LOAD_BUDGET {
                break;
            }
            if self.world.ensure_loaded(cp) {
                loaded += 1;
            }
        }
        self.world.unload_outside(center, UNLOAD_RADIUS);
        self.remove_unloaded_gpu_chunks();
        for cp in self.world.take_dirty() {
            self.mesh_queue.entry(cp).or_insert(false);
        }
    }

    fn remove_unloaded_gpu_chunks(&mut self) {
        let mut kept = Vec::with_capacity(self.chunks.len());
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        for chunk in self.chunks.drain(..) {
            let origin = chunk.origin;
            let cp = IVec3::new(
                origin.x / CHUNK_SIZE,
                origin.y / CHUNK_SIZE,
                origin.z / CHUNK_SIZE,
            );
            if self.world.chunk(cp).is_some() {
                kept.push(chunk);
            } else {
                renderer.remove_chunk(chunk);
            }
        }
        self.chunks = kept;
    }

    fn mesh_some(&mut self) {
        let mut pending: Vec<(IVec3, bool)> = self
            .mesh_queue
            .iter()
            .map(|(&cp, &urgent)| (cp, urgent))
            .collect();
        let center = self.center_chunk();
        pending.sort_by_key(|(cp, urgent)| {
            let dx = cp.x - center.x;
            let dy = cp.y - center.y;
            let dz = cp.z - center.z;
            (!urgent, dx * dx + dy * dy + dz * dz)
        });
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        for (cp, _) in pending.into_iter().take(MESH_BUDGET) {
            self.mesh_queue.remove(&cp);
            if let Some(index) = self
                .chunks
                .iter()
                .position(|chunk| chunk.origin == cp * CHUNK_SIZE)
            {
                let old = self.chunks.swap_remove(index);
                renderer.remove_chunk(old);
            }
            if self.world.chunk(cp).is_none() {
                continue;
            }
            let mesh = mesh_chunk(&self.world.padded(cp));
            if mesh.vertices.is_empty() || mesh.indices.is_empty() {
                continue;
            }
            match renderer.upload_chunk(&mesh, cp * CHUNK_SIZE) {
                Ok(chunk) => self.chunks.push(chunk),
                Err(error) => log::error!("upload chunk {cp:?} failed: {error:#}"),
            }
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
            self.chunks.len(),
            self.mesh_queue.len(),
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
                self.mesh_queue.insert(cp, true);
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
        self.controller.update(&mut self.camera, dt);
        self.stream();
        self.mesh_some();
        self.ray_hit = raycast(&self.world, self.camera.pos, self.camera.forward(), 6.0);
        let globals = self.globals();
        let rendered = match (self.gpu.as_ref(), self.renderer.as_mut()) {
            (Some(gpu), Some(renderer)) => gpu.render(
                renderer,
                &globals,
                &self.chunks,
                self.ray_hit.map(|hit| hit.block),
            ),
            _ => false,
        };
        let frame_ms = now.elapsed().as_secs_f64() * 1000.0;
        self.title_max_ms = self.title_max_ms.max(frame_ms);
        if rendered {
            self.title_frames = self.title_frames.saturating_add(1);
            self.frames = self.frames.saturating_add(1);
            if self.smoke_frames.is_some_and(|limit| self.frames >= limit) {
                self.smoke_frames = None;
                log::info!("smoke: rendered {} frames, exiting", self.frames);
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
        self.last_frame = Instant::now();
        if let Some(gpu) = self.gpu.as_ref() {
            gpu.window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
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
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(smoke_frames);
    event_loop.run_app(&mut app)?;
    Ok(())
}
