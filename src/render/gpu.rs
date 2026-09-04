//! Device and queue setup shared by the window and snapshot front ends.

/// The renderer's device context.  It deliberately does not contain a
/// `Surface`, `Window`, or any winit type.
pub struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    /// Request a high-performance adapter without requiring a presentation
    /// surface. This works for both the windowed and offscreen entry points.
    pub fn new() -> anyhow::Result<Self> {
        pollster::block_on(Self::new_async())
    }

    /// Async counterpart for callers that already own an async runtime.
    pub async fn new_async() -> anyhow::Result<Self> {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
                apply_limit_buckets: false,
            })
            .await?;
        let info = adapter.get_info();
        log::info!(
            "adapter: {} | backend: {:?} | driver: {} {}",
            info.name,
            info.backend,
            info.driver,
            info.driver_info
        );
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("voxelforge-device"),
                ..Default::default()
            })
            .await?;
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
        })
    }
}
