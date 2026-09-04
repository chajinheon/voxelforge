//! Offscreen color/depth targets and synchronous RGBA readback for snapshots.

use std::sync::mpsc;

/// A renderable color target paired with a standard-Z depth target.
pub struct OffscreenTarget {
    pub width: u32,
    pub height: u32,
    pub color: wgpu::Texture,
    pub color_view: wgpu::TextureView,
    pub depth: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
}

impl OffscreenTarget {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> anyhow::Result<Self> {
        if width == 0 || height == 0 {
            anyhow::bail!("offscreen target dimensions must be non-zero");
        }
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen-color"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen-depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        Ok(Self {
            width,
            height,
            color,
            color_view,
            depth,
            depth_view,
        })
    }

    pub fn read_pixels(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> anyhow::Result<Vec<u8>> {
        read_pixels(device, queue, &self.color, self.width, self.height)
    }
}

/// Copy an RGBA texture into CPU memory, removing wgpu's 256-byte row padding.
pub fn read_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> anyhow::Result<Vec<u8>> {
    if width == 0 || height == 0 {
        anyhow::bail!("readback dimensions must be non-zero");
    }
    let unpadded_row = width as u64 * 4;
    let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as u64;
    let padded_row = unpadded_row.next_multiple_of(alignment);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("offscreen-readback"),
        size: padded_row * height as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("offscreen-readback-copy"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row as u32),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;
    receiver
        .recv()
        .map_err(|error| anyhow::anyhow!("readback callback dropped: {error}"))??;
    let mapped = buffer.slice(..).get_mapped_range()?;
    let mut output = vec![0_u8; (unpadded_row * height as u64) as usize];
    for row in 0..height as usize {
        let src_start = row * padded_row as usize;
        let dst_start = row * unpadded_row as usize;
        output[dst_start..dst_start + unpadded_row as usize]
            .copy_from_slice(&mapped[src_start..src_start + unpadded_row as usize]);
    }
    drop(mapped);
    buffer.unmap();
    Ok(output)
}
