//! Reusable internal frame targets. Geometry never renders directly to the surface.

pub struct FrameTargets {
    pub width: u32,
    pub height: u32,
    pub color: wgpu::Texture,
    pub color_view: wgpu::TextureView,
    pub ui_depth: wgpu::Texture,
    pub ui_depth_view: wgpu::TextureView,
}

impl FrameTargets {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vf/m7/frame-targets/hdr-color"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor {
            label: Some("vf/m7/frame-targets/color-view"),
            ..Default::default()
        });
        let ui_depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vf/m10/viewmodel/native-depth32"),
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
        let ui_depth_view = ui_depth.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            width: width.max(1),
            height: height.max(1),
            color,
            color_view,
            ui_depth,
            ui_depth_view,
        }
    }

    pub fn texture_memory_bytes(&self) -> usize {
        crate::render::memory::texture_bytes(&self.color)
            + crate::render::memory::texture_bytes(&self.ui_depth)
    }
}
