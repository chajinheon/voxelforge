//! Windowed Voxelforge entrypoint for the M2 free-camera and streaming milestone.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use glam::{IVec3, Vec3};
use voxelforge::mesh::mesh_chunk;
use voxelforge::player::camera::EYE_HEIGHT;
use voxelforge::player::{Camera, Controller};
use voxelforge::render::{Globals, Gpu, GpuChunk, Renderer};
use voxelforge::world::coords::{CHUNK_SIZE, WORLD_CHUNKS_Y, chunk_of};
use voxelforge::world::r#gen::WorldGen;
use voxelforge::world::world::World;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

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

struct WindowGpu {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
}

impl WindowGpu {
    fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let gpu = Gpu::new()?;
        let surface = gpu.instance.create_surface(window.clone())?;
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&gpu.adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| anyhow::anyhow!("surface is not supported by adapter"))?;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&gpu.device, &config);
        log::info!(
            "surface: {:?} {}x{} (scale {})",
            config.format,
            config.width,
            config.height,
            window.scale_factor()
        );
        let (depth, depth_view) = create_depth(&gpu.device, config.width, config.height);
        Ok(Self {
            window,
            surface,
            device: gpu.device,
            queue: gpu.queue,
            config,
            depth,
            depth_view,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        (self.depth, self.depth_view) = create_depth(&self.device, width, height);
    }

    fn render(&self, renderer: &mut Renderer, globals: &Globals, chunks: &[GpuChunk]) -> bool {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return false;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return false;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                log::error!("surface validation error while acquiring frame");
                return false;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        renderer.render(&view, &self.depth_view, globals, chunks);
        self.window.pre_present_notify();
        self.queue.present(frame);
        true
    }
}

fn create_depth(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("window-depth"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    (depth, view)
}

struct App {
    gpu: Option<WindowGpu>,
    world: World,
    camera: Camera,
    controller: Controller,
    renderer: Option<Renderer>,
    chunks: Vec<GpuChunk>,
    mesh_queue: HashSet<IVec3>,
    last_frame: Instant,
    started: Instant,
    title_at: Instant,
    title_frames: u32,
    title_max_ms: f64,
    cursor_locked: bool,
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
            mesh_queue: HashSet::new(),
            last_frame: now,
            started: now,
            title_at: now,
            title_frames: 0,
            title_max_ms: 0.0,
            cursor_locked: false,
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
        self.mesh_queue.extend(self.world.take_dirty());
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
        let mut pending: Vec<IVec3> = self.mesh_queue.iter().copied().collect();
        let center = self.center_chunk();
        pending.sort_by_key(|cp| {
            let dx = cp.x - center.x;
            let dy = cp.y - center.y;
            let dz = cp.z - center.z;
            dx * dx + dy * dy + dz * dz
        });
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        for cp in pending.into_iter().take(MESH_BUDGET) {
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
        if let Some(gpu) = self.gpu.as_ref() {
            gpu.window.set_title(&format!(
                "voxelforge | {:.0} fps | max {:.1}ms | pos {:.1} {:.1} {:.1} | chunks {} drawn {} | meshq {}",
                fps,
                self.title_max_ms,
                self.camera.pos.x,
                self.camera.pos.y,
                self.camera.pos.z,
                self.world.chunk_count(),
                self.chunks.len(),
                self.mesh_queue.len()
            ));
        }
        self.title_at = Instant::now();
        self.title_frames = 0;
        self.title_max_ms = 0.0;
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
        let globals = self.globals();
        let rendered = match (self.gpu.as_ref(), self.renderer.as_mut()) {
            (Some(gpu), Some(renderer)) => gpu.render(renderer, &globals, &self.chunks),
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

impl Default for App {
    fn default() -> Self {
        Self::new(None)
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
                ..
            } => self.lock_cursor(),
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
