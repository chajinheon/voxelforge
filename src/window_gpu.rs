use std::sync::Arc;

use voxelforge::render::{Globals, Gpu, GpuChunk, Renderer};
use winit::window::Window;

pub(crate) struct WindowGpu {
    pub(crate) window: Arc<Window>,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) config: wgpu::SurfaceConfiguration,
    surface: wgpu::Surface<'static>,
    depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
}

impl WindowGpu {
    pub(crate) fn new(window: Arc<Window>) -> anyhow::Result<Self> {
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
            device: gpu.device,
            queue: gpu.queue,
            config,
            surface,
            depth,
            depth_view,
        })
    }

    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        (self.depth, self.depth_view) = create_depth(&self.device, width, height);
    }

    pub(crate) fn render(
        &self,
        renderer: &mut Renderer,
        globals: &Globals,
        chunks: &[GpuChunk],
        outline: Option<glam::IVec3>,
    ) -> bool {
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
        renderer.render_with_outline(&view, &self.depth_view, globals, chunks, outline);
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
