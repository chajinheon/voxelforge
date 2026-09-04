//! Windowed Voxelforge entrypoint for the M1 static-terrain milestone.

use std::sync::Arc;

use glam::{IVec3, Mat4, Vec3};
use voxelforge::mesh::mesh_chunk;
use voxelforge::render::{Globals, Gpu, GpuChunk, Renderer};
use voxelforge::world::coords::WORLD_CHUNKS_Y;
use voxelforge::world::r#gen::WorldGen;
use voxelforge::world::world::World;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const WINDOW_WIDTH: f64 = 1280.0;
const WINDOW_HEIGHT: f64 = 720.0;
const TERRAIN_RADIUS: i32 = 4;
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
    renderer: Renderer,
    chunks: Vec<GpuChunk>,
    globals: Globals,
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
        let mut renderer = Renderer::new(&gpu.device, &gpu.queue, config.format)?;
        let (chunks, globals) = build_terrain(
            &mut renderer,
            DEFAULT_SEED,
            TERRAIN_RADIUS,
            config.width,
            config.height,
        )?;

        // Keep the device and queue owned by this state. The surface remains
        // valid because the Arc<Window> is retained alongside it.
        Ok(Self {
            window,
            surface,
            device: gpu.device,
            queue: gpu.queue,
            config,
            depth,
            depth_view,
            renderer,
            chunks,
            globals,
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
        self.globals = make_globals(DEFAULT_SEED, DEFAULT_YAW, DEFAULT_PITCH, width, height);
    }

    /// Acquire and present one frame. `false` means that no frame was
    /// presented (for example while the window is occluded or outdated).
    fn render(&mut self) -> bool {
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
        self.renderer
            .render(&view, &self.depth_view, &self.globals, &self.chunks);
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

fn build_terrain(
    renderer: &mut Renderer,
    seed: u64,
    radius: i32,
    width: u32,
    height: u32,
) -> anyhow::Result<(Vec<GpuChunk>, Globals)> {
    let mut world = World::new(seed);
    for z in -radius..=radius {
        for x in -radius..=radius {
            for y in 0..WORLD_CHUNKS_Y {
                world.ensure_loaded(IVec3::new(x, y, z));
            }
        }
    }

    let mut dirty = world.take_dirty();
    dirty.sort_by_key(|cp| cp.x * cp.x + cp.z * cp.z + cp.y * cp.y);
    let mut gpu_chunks = Vec::with_capacity(dirty.len());
    for cp in dirty {
        let mesh = mesh_chunk(&world.padded(cp));
        if !mesh.vertices.is_empty() && !mesh.indices.is_empty() {
            gpu_chunks.push(renderer.upload_chunk(&mesh, cp * 32)?);
        }
    }
    let globals = make_globals(seed, DEFAULT_YAW, DEFAULT_PITCH, width, height);
    Ok((gpu_chunks, globals))
}

#[allow(deprecated)] // BLUEPRINT D4 fixes these exact glam constructors.
fn make_globals(seed: u64, yaw: f32, pitch: f32, width: u32, height: u32) -> Globals {
    let generator = WorldGen::new(seed);
    let eye_y = generator.height_at(0, 0) as f32 + 3.0;
    let position = Vec3::new(0.0, eye_y, 0.0);
    let view = Mat4::look_to_rh(position, forward(yaw, pitch), Vec3::Y);
    let projection =
        Mat4::perspective_rh(FOV_Y, width.max(1) as f32 / height.max(1) as f32, NEAR, FAR);
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

#[derive(Default)]
struct App {
    gpu: Option<WindowGpu>,
    frames: u32,
    smoke_frames: Option<u32>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("voxelforge — M1 terrain")
            .with_inner_size(winit::dpi::LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                log::error!("create window failed: {error}");
                event_loop.exit();
                return;
            }
        };
        match WindowGpu::new(window) {
            Ok(gpu) => self.gpu = Some(gpu),
            Err(error) => {
                log::error!("initialize renderer failed: {error:#}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => gpu.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                if gpu.render() {
                    self.frames = self.frames.saturating_add(1);
                    if self.smoke_frames.is_some_and(|limit| self.frames >= limit) {
                        log::info!("smoke: rendered {} frames, exiting", self.frames);
                        event_loop.exit();
                        return;
                    }
                }
                gpu.window.request_redraw();
            }
            _ => {}
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
    let mut app = App {
        smoke_frames,
        ..Default::default()
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}
