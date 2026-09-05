use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use voxelforge::render::screenshot::{ScreenshotQueue, ScreenshotRequest, encode_png};
use voxelforge::render::ui::UiFrame;
use voxelforge::render::{Globals, Gpu, GpuChunkMeshes, RenderView, Renderer};
use winit::window::Window;

pub(crate) struct WindowGpu {
    pub(crate) window: Arc<Window>,
    pub(crate) adapter: wgpu::Adapter,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) config: wgpu::SurfaceConfiguration,
    surface: wgpu::Surface<'static>,
    depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    screenshots: ScreenshotQueue,
    screenshot_requested: bool,
    screenshot_copy_supported: bool,
    pending_readbacks: Vec<(
        voxelforge::render::screenshot::ScreenshotTicket,
        mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    )>,
    encoding_jobs: Vec<(
        voxelforge::render::screenshot::ScreenshotTicket,
        std::thread::JoinHandle<anyhow::Result<()>>,
    )>,
}

impl WindowGpu {
    pub(crate) fn new(window: Arc<Window>, vsync: bool) -> anyhow::Result<Self> {
        let gpu = Gpu::new()?;
        let surface = gpu.instance.create_surface(window.clone())?;
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&gpu.adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| anyhow::anyhow!("surface is not supported by adapter"))?;
        config.present_mode = if vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };
        let capabilities = surface.get_capabilities(&gpu.adapter);
        let copy_supported = capabilities.usages.contains(wgpu::TextureUsages::COPY_SRC);
        if copy_supported {
            config.usage |= wgpu::TextureUsages::COPY_SRC;
        } else {
            log::warn!("surface does not support COPY_SRC; F2 screenshots disabled");
        }
        let mut screenshot_copy_supported =
            copy_supported && screenshot_format_supported(config.format);
        surface.configure(&gpu.device, &config);
        log::info!(
            "surface: {:?} {}x{} (scale {})",
            config.format,
            config.width,
            config.height,
            window.scale_factor()
        );
        let (depth, depth_view) = create_depth(&gpu.device, config.width, config.height);
        let output_dir = std::env::var_os("VF_SCREENSHOTS")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join("Pictures/Voxelforge"))
            })
            .unwrap_or_else(|| {
                screenshot_copy_supported = false;
                log::warn!("HOME is unavailable; F2 screenshots disabled");
                PathBuf::new()
            });
        Ok(Self {
            window,
            adapter: gpu.adapter,
            device: gpu.device,
            queue: gpu.queue,
            config,
            surface,
            depth,
            depth_view,
            screenshots: ScreenshotQueue::new(output_dir),
            screenshot_requested: false,
            screenshot_copy_supported,
            pending_readbacks: Vec::new(),
            encoding_jobs: Vec::new(),
        })
    }

    pub(crate) fn request_screenshot(&mut self) {
        if !self.screenshot_copy_supported {
            log::warn!("screenshot requested but surface readback is unavailable");
            return;
        }
        if self.screenshot_requested {
            log::debug!("screenshot request already pending");
        } else {
            self.screenshot_requested = true;
        }
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
        &mut self,
        renderer: &mut Renderer,
        globals: &Globals,
        chunks: &[GpuChunkMeshes],
        outline: Option<glam::IVec3>,
        render_view: RenderView,
        ui: &UiFrame<'_>,
    ) -> Option<usize> {
        self.service_screenshots();
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return None;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return None;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                log::error!("surface validation error while acquiring frame");
                return None;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let drawn = renderer.render_app_frame(
            &view,
            &self.depth_view,
            globals,
            chunks,
            outline,
            render_view,
            ui,
        );
        if self.screenshot_requested {
            self.screenshot_requested = false;
            self.capture_frame(&frame.texture);
        }
        self.window.pre_present_notify();
        self.queue.present(frame);
        self.service_screenshots();
        Some(drawn)
    }

    fn service_screenshots(&mut self) {
        let _ = self.device.poll(wgpu::PollType::Poll);
        let mut ready = Vec::new();
        for (index, (_ticket, receiver)) in self.pending_readbacks.iter().enumerate() {
            match receiver.try_recv() {
                Ok(Ok(())) => ready.push((index, true)),
                Ok(Err(error)) => {
                    log::warn!("screenshot readback map failed: {error}");
                    ready.push((index, false));
                }
                Err(mpsc::TryRecvError::Disconnected) => ready.push((index, false)),
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        for (index, mapped) in ready.into_iter().rev() {
            let (ticket, _receiver) = self.pending_readbacks.remove(index);
            if mapped {
                self.finish_screenshot(ticket);
            } else {
                let _ = self.screenshots.release(ticket);
            }
        }
        let finished: Vec<usize> = self
            .encoding_jobs
            .iter()
            .enumerate()
            .filter_map(|(index, (_, job))| job.is_finished().then_some(index))
            .collect();
        for index in finished.into_iter().rev() {
            let (ticket, job) = self.encoding_jobs.remove(index);
            let result = job
                .join()
                .unwrap_or_else(|_| Err(anyhow::anyhow!("screenshot encoder panicked")));
            if let Err(error) = result {
                log::warn!("screenshot PNG write failed: {error:#}");
            }
            let _ = self.screenshots.release(ticket);
        }
    }

    fn finish_screenshot(&mut self, ticket: voxelforge::render::screenshot::ScreenshotTicket) {
        let mut pixels = match self.screenshots.finish_copy_mapped(ticket) {
            Ok(pixels) => pixels,
            Err(error) => {
                log::warn!("screenshot readback failed: {error:#}");
                let _ = self.screenshots.release(ticket);
                return;
            }
        };
        if is_bgra_format(self.config.format) {
            pixels.swizzle_bgra_to_rgba();
        }
        let Ok(path) = self.screenshots.path_for(ticket) else {
            let _ = self.screenshots.release(ticket);
            return;
        };
        self.encoding_jobs.push((
            ticket,
            std::thread::spawn(move || encode_png(&path, &pixels)),
        ));
    }

    fn capture_frame(&mut self, source: &wgpu::Texture) {
        let timestamp = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration,
            Err(error) => {
                log::error!("screenshot timestamp failed: {error}");
                return;
            }
        };
        let file_name = screenshot_filename(timestamp);
        let request =
            match self
                .screenshots
                .request(self.config.width, self.config.height, &file_name)
            {
                Ok(request) => request,
                Err(error) => {
                    log::error!("screenshot request failed: {error:#}");
                    return;
                }
            };
        let ticket = match request {
            ScreenshotRequest::Accepted(ticket) => ticket,
            ScreenshotRequest::Dropped => return,
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("vf/m10/screenshot-copy"),
            });
        if let Err(error) =
            self.screenshots
                .copy_texture_to_slot(&self.device, &mut encoder, source, ticket)
        {
            log::error!("screenshot copy recording failed: {error:#}");
            if let Err(release_error) = self.screenshots.release(ticket) {
                log::error!("screenshot slot release failed: {release_error:#}");
            }
            return;
        }
        self.queue.submit([encoder.finish()]);
        let receiver = match self.screenshots.map_copy(ticket) {
            Ok(receiver) => receiver,
            Err(error) => {
                log::error!("screenshot map request failed: {error:#}");
                if let Err(release_error) = self.screenshots.release(ticket) {
                    log::error!("screenshot slot release failed: {release_error:#}");
                }
                return;
            }
        };
        self.pending_readbacks.push((ticket, receiver));
    }
}

fn screenshot_format_supported(format: wgpu::TextureFormat) -> bool {
    matches!(
        format,
        wgpu::TextureFormat::Rgba8Unorm
            | wgpu::TextureFormat::Rgba8UnormSrgb
            | wgpu::TextureFormat::Bgra8Unorm
            | wgpu::TextureFormat::Bgra8UnormSrgb
    )
}

fn is_bgra_format(format: wgpu::TextureFormat) -> bool {
    matches!(
        format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    )
}

/// Produce the stable `vf_YYYYMMDD_HHMMSS_mmm.png` contract without adding a
/// calendar dependency to the render executable. The timestamp is UTC; the
/// filename contract deliberately does not encode a timezone.
fn screenshot_filename(timestamp: Duration) -> String {
    let seconds = timestamp.as_secs();
    let days = (seconds / 86_400) as i64;
    let second_of_day = seconds % 86_400;
    let hour = second_of_day / 3_600;
    let minute = second_of_day % 3_600 / 60;
    let second = second_of_day % 60;
    let millis = timestamp.subsec_millis();
    let (year, month, day) = civil_date_from_unix_days(days);
    format!("vf_{year:04}{month:02}{day:02}_{hour:02}{minute:02}{second:02}_{millis:03}.png")
}

fn civil_date_from_unix_days(days: i64) -> (i64, i64, i64) {
    // Howard Hinnant's proleptic-Gregorian civil-from-days transform. `days`
    // is measured from 1970-01-01 and all SystemTime values here are >= epoch.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
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

#[cfg(test)]
mod tests {
    use super::screenshot_filename;
    use std::time::Duration;

    #[test]
    fn screenshot_name_matches_contract_at_epoch() {
        assert_eq!(
            screenshot_filename(Duration::from_millis(123)),
            "vf_19700101_000000_123.png"
        );
    }

    #[test]
    fn screenshot_name_handles_leap_day() {
        // 2024-02-29 12:34:56.789 UTC.
        assert_eq!(
            screenshot_filename(Duration::from_millis(1_709_210_096_789)),
            "vf_20240229_123456_789.png"
        );
    }
}
