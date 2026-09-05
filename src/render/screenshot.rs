//! Asynchronous screenshot readback with three reusable GPU staging slots.
//!
//! The render thread only reserves a slot and records a texture copy. Mapping and
//! PNG encoding are intentionally separate operations so callers can run them on
//! a short-lived worker. A fourth request is reported as [`ScreenshotRequest::Dropped`]
//! and never grows the queue or panics.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use image::ImageFormat;

/// The fixed number of reusable screenshot readback slots.
pub const SCREENSHOT_SLOT_COUNT: usize = 3;

fn padded_buffer_size(width: u32, height: u32) -> u64 {
    let row = u64::from(width) * 4;
    row.next_multiple_of(u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)) * u64::from(height)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotState {
    Free,
    CopyQueued,
    ReadbackMapped,
}

struct Slot {
    generation: u64,
    state: SlotState,
    width: u32,
    height: u32,
    padded_row: u32,
    path: PathBuf,
    buffer: Option<wgpu::Buffer>,
}

impl Default for Slot {
    fn default() -> Self {
        Self {
            generation: 0,
            state: SlotState::Free,
            width: 0,
            height: 0,
            padded_row: 0,
            path: PathBuf::new(),
            buffer: None,
        }
    }
}

/// Opaque identity for one reserved screenshot slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ScreenshotTicket {
    slot: u8,
    generation: u64,
}

/// Result of an F2 request. `Dropped` is expected backpressure when all slots
/// are still being copied or encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenshotRequest {
    Accepted(ScreenshotTicket),
    Dropped,
}

impl ScreenshotRequest {
    pub fn ticket(self) -> Option<ScreenshotTicket> {
        match self {
            Self::Accepted(ticket) => Some(ticket),
            Self::Dropped => None,
        }
    }
}

/// The CPU result of a completed GPU readback. Encode it outside the render
/// loop with [`ScreenshotQueue::write_png`].
pub struct ScreenshotPixels {
    ticket: ScreenshotTicket,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// Encode pixels without touching the GPU queue; intended for a worker thread.
pub fn encode_png(path: &Path, pixels: &ScreenshotPixels) -> anyhow::Result<()> {
    if pixels.pixels.len() != pixels.width as usize * pixels.height as usize * 4 {
        anyhow::bail!("screenshot pixel buffer has the wrong length");
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    image::save_buffer_with_format(
        path,
        &pixels.pixels,
        pixels.width,
        pixels.height,
        image::ColorType::Rgba8,
        ImageFormat::Png,
    )?;
    Ok(())
}

impl ScreenshotPixels {
    pub fn ticket(&self) -> ScreenshotTicket {
        self.ticket
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn as_rgba8(&self) -> &[u8] {
        &self.pixels
    }

    /// Convert a BGRA surface readback into the RGBA layout expected by PNG.
    pub fn swizzle_bgra_to_rgba(&mut self) {
        for pixel in self.pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
}

/// A bounded, three-slot screenshot queue.
pub struct ScreenshotQueue {
    slots: [Slot; SCREENSHOT_SLOT_COUNT],
    next_generation: u64,
    output_dir: PathBuf,
    dropped: u64,
}

impl ScreenshotQueue {
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self {
            slots: std::array::from_fn(|_| Slot::default()),
            next_generation: 1,
            output_dir: output_dir.into(),
            dropped: 0,
        }
    }

    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    pub fn path_for(&self, ticket: ScreenshotTicket) -> anyhow::Result<PathBuf> {
        Ok(self.valid_slot(ticket)?.path.clone())
    }

    pub fn dropped_count(&self) -> u64 {
        self.dropped
    }

    pub fn available_slots(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.state == SlotState::Free)
            .count()
    }

    /// Reserve a slot for a future RGBA8 texture copy.
    pub fn request(
        &mut self,
        width: u32,
        height: u32,
        file_name: impl AsRef<Path>,
    ) -> anyhow::Result<ScreenshotRequest> {
        if width == 0 || height == 0 {
            anyhow::bail!("screenshot dimensions must be non-zero");
        }
        let Some((index, slot)) = self
            .slots
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.state == SlotState::Free)
        else {
            self.dropped = self.dropped.saturating_add(1);
            log::warn!("screenshot queue full; dropping request");
            return Ok(ScreenshotRequest::Dropped);
        };

        let file_name = file_name.as_ref();
        if file_name.is_absolute()
            || file_name
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            anyhow::bail!("screenshot file name must stay inside output directory");
        }
        let generation = self.next_generation;
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        let padded_row = padded_buffer_size(width, 1);
        slot.generation = generation;
        slot.state = SlotState::CopyQueued;
        slot.width = width;
        slot.height = height;
        slot.padded_row = padded_row as u32;
        slot.path = self.output_dir.join(file_name);
        Ok(ScreenshotRequest::Accepted(ScreenshotTicket {
            slot: index as u8,
            generation,
        }))
    }

    /// Record a GPU copy into the ticket's persistent MAP_READ staging buffer.
    /// The caller submits the finished encoder and later calls [`map_copy`].
    pub fn copy_texture_to_slot(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture,
        ticket: ScreenshotTicket,
    ) -> anyhow::Result<()> {
        let slot = self.valid_slot_mut(ticket)?;
        let buffer_size = padded_buffer_size(slot.width, slot.height);
        if slot
            .buffer
            .as_ref()
            .is_none_or(|buffer| buffer.size() != buffer_size)
        {
            slot.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("vf/m10/screenshot/readback"),
                size: buffer_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }));
        }
        let Some(buffer) = slot.buffer.as_ref() else {
            anyhow::bail!("screenshot staging buffer was not created");
        };
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: source,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(slot.padded_row),
                    rows_per_image: Some(slot.height),
                },
            },
            wgpu::Extent3d {
                width: slot.width,
                height: slot.height,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    /// Begin asynchronous mapping after the command buffer containing the copy
    /// has been submitted. The caller should poll the device before completing.
    pub fn map_copy(
        &mut self,
        ticket: ScreenshotTicket,
    ) -> anyhow::Result<mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>> {
        let slot = self.valid_slot_mut(ticket)?;
        let Some(buffer) = slot.buffer.as_ref() else {
            anyhow::bail!("screenshot copy was not recorded");
        };
        let (sender, receiver) = mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        slot.state = SlotState::ReadbackMapped;
        Ok(receiver)
    }

    /// Finish a mapped readback, stripping the required 256-byte row padding.
    pub fn finish_copy(
        &mut self,
        ticket: ScreenshotTicket,
        receiver: mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    ) -> anyhow::Result<ScreenshotPixels> {
        receiver
            .recv()
            .map_err(|error| anyhow::anyhow!("screenshot map callback dropped: {error}"))??;
        self.finish_copy_mapped(ticket)
    }

    /// Finish after the caller has already observed a successful map callback.
    pub fn finish_copy_mapped(
        &mut self,
        ticket: ScreenshotTicket,
    ) -> anyhow::Result<ScreenshotPixels> {
        let slot = self.valid_slot_mut(ticket)?;
        let Some(buffer) = slot.buffer.as_ref() else {
            anyhow::bail!("screenshot staging buffer was not created");
        };
        let mapped = buffer.slice(..).get_mapped_range()?;
        let row_size = slot.width as usize * 4;
        let mut pixels = vec![0_u8; row_size * slot.height as usize];
        for row in 0..slot.height as usize {
            let source_start = row * slot.padded_row as usize;
            let destination_start = row * row_size;
            pixels[destination_start..destination_start + row_size]
                .copy_from_slice(&mapped[source_start..source_start + row_size]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(ScreenshotPixels {
            ticket,
            width: slot.width,
            height: slot.height,
            pixels,
        })
    }

    /// Encode the completed pixels and release its slot. This should run on the
    /// screenshot encoder worker, never in the render loop.
    pub fn write_png(&mut self, pixels: ScreenshotPixels) -> anyhow::Result<PathBuf> {
        let path = self.valid_slot(pixels.ticket)?.path.clone();
        if pixels.pixels.len() != pixels.width as usize * pixels.height as usize * 4 {
            anyhow::bail!("screenshot pixel buffer has the wrong length");
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        image::save_buffer_with_format(
            &path,
            &pixels.pixels,
            pixels.width,
            pixels.height,
            image::ColorType::Rgba8,
            ImageFormat::Png,
        )?;
        self.release(pixels.ticket)?;
        Ok(path)
    }

    pub fn release(&mut self, ticket: ScreenshotTicket) -> anyhow::Result<()> {
        let slot = self.valid_slot_mut(ticket)?;
        slot.state = SlotState::Free;
        slot.width = 0;
        slot.height = 0;
        slot.padded_row = 0;
        slot.path.clear();
        Ok(())
    }

    fn valid_slot(&self, ticket: ScreenshotTicket) -> anyhow::Result<&Slot> {
        let slot = self
            .slots
            .get(usize::from(ticket.slot))
            .ok_or_else(|| anyhow::anyhow!("invalid screenshot ticket slot"))?;
        if slot.generation != ticket.generation || slot.state == SlotState::Free {
            anyhow::bail!("stale screenshot ticket");
        }
        Ok(slot)
    }

    fn valid_slot_mut(&mut self, ticket: ScreenshotTicket) -> anyhow::Result<&mut Slot> {
        let slot = self
            .slots
            .get_mut(usize::from(ticket.slot))
            .ok_or_else(|| anyhow::anyhow!("invalid screenshot ticket slot"))?;
        if slot.generation != ticket.generation || slot.state == SlotState::Free {
            anyhow::bail!("stale screenshot ticket");
        }
        Ok(slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_queue_drops_when_full_without_panic() {
        let mut queue = ScreenshotQueue::new("/tmp/voxelforge-screenshot-test");
        let mut tickets = Vec::new();
        for index in 0..SCREENSHOT_SLOT_COUNT {
            let request = queue
                .request(16, 16, format!("{index}.png"))
                .expect("valid request");
            tickets.push(request.ticket().expect("slot should be available"));
        }
        assert_eq!(queue.available_slots(), 0);
        assert_eq!(
            queue
                .request(16, 16, "dropped.png")
                .expect("drop is not an error"),
            ScreenshotRequest::Dropped
        );
        assert_eq!(queue.dropped_count(), 1);
        queue.release(tickets[0]).expect("ticket is valid");
        assert_eq!(queue.available_slots(), 1);
        assert!(matches!(
            queue.request(16, 16, "reused.png").expect("slot reused"),
            ScreenshotRequest::Accepted(_)
        ));
    }

    #[test]
    fn screenshot_rejects_absolute_and_parent_paths() {
        let mut queue = ScreenshotQueue::new("/tmp/voxelforge-screenshot-test");
        assert!(queue.request(4, 4, "/tmp/out.png").is_err());
        assert!(queue.request(4, 4, "../out.png").is_err());
    }

    #[test]
    fn screenshot_requires_non_zero_dimensions() {
        let mut queue = ScreenshotQueue::new("/tmp/voxelforge-screenshot-test");
        assert!(queue.request(0, 1, "zero.png").is_err());
        assert!(queue.request(1, 0, "zero.png").is_err());
    }

    #[test]
    fn staging_size_grows_with_resize_and_keeps_row_alignment() {
        let small = padded_buffer_size(127, 32);
        let large = padded_buffer_size(1024, 720);
        assert_eq!(small % u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT), 0);
        assert!(large > small);
        assert_eq!(large, 1024 * 4 * 720);
    }
}
