//! Optional pass-level timestamp queries with reusable resolve/readback buffers.

use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::mpsc;

/// The stable order used by the timing JSON contract and the GPU query set.
pub const TIMED_PASS_COUNT: usize = 20;
const QUERY_COUNT: u32 = (TIMED_PASS_COUNT * 2) as u32;
const QUERY_BYTES: u64 = (TIMED_PASS_COUNT * 2 * std::mem::size_of::<u64>()) as u64;
const READBACK_RING_SIZE: usize = 3;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimedPass {
    Shadow = 0,
    GBuffer = 1,
    DepthPyramid = 2,
    Gtao = 3,
    Atmosphere = 4,
    Deferred = 5,
    GiTrace = 6,
    GiTemporal = 7,
    GiDenoise = 8,
    Volumetric = 9,
    Clouds = 10,
    CloudShadow = 11,
    Glass = 12,
    Water = 13,
    Bloom = 14,
    Exposure = 15,
    TaauTonemap = 16,
    Viewmodel = 17,
    Ui = 18,
    Present = 19,
}

impl TimedPass {
    /// Legacy name retained for the M6 renderer; it resolves to the glass pass.
    #[allow(non_upper_case_globals)]
    pub const Forward: Self = Self::Glass;

    pub const ALL: [Self; TIMED_PASS_COUNT] = [
        Self::Shadow,
        Self::GBuffer,
        Self::DepthPyramid,
        Self::Gtao,
        Self::Atmosphere,
        Self::Deferred,
        Self::GiTrace,
        Self::GiTemporal,
        Self::GiDenoise,
        Self::Volumetric,
        Self::Clouds,
        Self::CloudShadow,
        Self::Glass,
        Self::Water,
        Self::Bloom,
        Self::Exposure,
        Self::TaauTonemap,
        Self::Viewmodel,
        Self::Ui,
        Self::Present,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Shadow => "shadow",
            Self::GBuffer => "gbuffer",
            Self::DepthPyramid => "depth_pyramid",
            Self::Gtao => "gtao",
            Self::Atmosphere => "atmosphere",
            Self::Deferred => "deferred",
            Self::GiTrace => "gi_trace",
            Self::GiTemporal => "gi_temporal",
            Self::GiDenoise => "gi_denoise",
            Self::Volumetric => "volumetric",
            Self::Clouds => "clouds",
            Self::CloudShadow => "cloud_shadow",
            Self::Glass => "glass",
            Self::Water => "water",
            Self::Bloom => "bloom",
            Self::Exposure => "exposure",
            Self::TaauTonemap => "taau_tonemap",
            Self::Viewmodel => "viewmodel",
            Self::Ui => "ui",
            Self::Present => "present",
        }
    }

    const fn query_indices(self) -> (u32, u32) {
        let start = self as u32 * 2;
        (start, start + 1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GpuFrameTimings {
    pub gbuffer_ms: f64,
    pub deferred_ms: f64,
    pub forward_ms: f64,
    pub present_ms: f64,
    passes_ms: [f64; TIMED_PASS_COUNT],
    valid_mask: u32,
    rendered_frame_index: u64,
}

impl Default for GpuFrameTimings {
    fn default() -> Self {
        Self {
            gbuffer_ms: 0.0,
            deferred_ms: 0.0,
            forward_ms: 0.0,
            present_ms: 0.0,
            passes_ms: [0.0; TIMED_PASS_COUNT],
            valid_mask: 0,
            rendered_frame_index: 0,
        }
    }
}

impl GpuFrameTimings {
    #[cfg(test)]
    fn from_passes(passes_ms: [f64; TIMED_PASS_COUNT], valid_mask: u32) -> Self {
        Self::from_passes_at(passes_ms, valid_mask, 0)
    }

    fn from_passes_at(
        passes_ms: [f64; TIMED_PASS_COUNT],
        valid_mask: u32,
        rendered_frame_index: u64,
    ) -> Self {
        let mut passes_ms = passes_ms;
        for (index, value) in passes_ms.iter_mut().enumerate() {
            if valid_mask & (1_u32 << index) == 0 {
                *value = 0.0;
            }
        }
        Self {
            gbuffer_ms: passes_ms[TimedPass::GBuffer.index()],
            deferred_ms: passes_ms[TimedPass::Deferred.index()],
            forward_ms: passes_ms[TimedPass::Glass.index()] + passes_ms[TimedPass::Water.index()],
            present_ms: passes_ms[TimedPass::Present.index()],
            passes_ms,
            valid_mask,
            rendered_frame_index,
        }
    }

    pub fn rendered_frame_index(self) -> u64 {
        self.rendered_frame_index
    }

    pub fn ms(self, pass: TimedPass) -> f64 {
        if self.valid_mask != 0 {
            self.passes_ms[pass.index()]
        } else {
            match pass {
                TimedPass::GBuffer => self.gbuffer_ms,
                TimedPass::Deferred => self.deferred_ms,
                TimedPass::Glass => self.forward_ms,
                TimedPass::Present => self.present_ms,
                _ => 0.0,
            }
        }
    }

    pub fn has_sample(self, pass: TimedPass) -> bool {
        self.valid_mask & (1_u32 << pass.index()) != 0
    }

    pub fn iter(self) -> impl Iterator<Item = (&'static str, f64)> {
        TimedPass::ALL
            .into_iter()
            .map(move |pass| (pass.label(), self.ms(pass)))
    }

    pub fn total_ms(self) -> f64 {
        if self.valid_mask != 0 {
            self.passes_ms.iter().sum()
        } else {
            self.gbuffer_ms + self.deferred_ms + self.forward_ms + self.present_ms
        }
    }
}

struct TimingReadbackSlot {
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    pending: bool,
    pending_mask: u32,
    pending_frame_index: u64,
    map_receiver: Option<mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>>,
}

pub struct GpuTiming {
    query_set: Option<wgpu::QuerySet>,
    readbacks: Vec<TimingReadbackSlot>,
    enabled: bool,
    written: Cell<u32>,
    next_frame_index: u64,
    completed: VecDeque<GpuFrameTimings>,
    pub last_frame: Option<GpuFrameTimings>,
}

impl GpuTiming {
    pub fn new(device: &wgpu::Device) -> Self {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return Self {
                query_set: None,
                readbacks: Vec::new(),
                enabled: false,
                written: Cell::new(0),
                next_frame_index: 0,
                completed: VecDeque::new(),
                last_frame: None,
            };
        }
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("vf/m7/timing/query-set"),
            ty: wgpu::QueryType::Timestamp,
            count: QUERY_COUNT,
        });
        let readbacks = (0..READBACK_RING_SIZE)
            .map(|slot| {
                let resolve = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("vf/m7/timing/resolve/{slot}")),
                    size: QUERY_BYTES,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                });
                let readback = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("vf/m7/timing/readback/{slot}")),
                    size: QUERY_BYTES,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                TimingReadbackSlot {
                    resolve,
                    readback,
                    pending: false,
                    pending_mask: 0,
                    pending_frame_index: 0,
                    map_receiver: None,
                }
            })
            .collect();
        Self {
            query_set: Some(query_set),
            readbacks,
            enabled: false,
            written: Cell::new(0),
            next_frame_index: 0,
            completed: VecDeque::new(),
            last_frame: None,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled && self.query_set.is_some();
        self.written.set(0);
        self.next_frame_index = 0;
        if !self.enabled {
            self.completed.clear();
            self.last_frame = None;
        }
    }

    pub fn supported(&self) -> bool {
        self.query_set.is_some()
    }

    pub fn active(&self) -> bool {
        self.enabled && self.supported()
    }

    pub fn writes(&self, pass: TimedPass) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let (start, end) = pass.query_indices();
        self.query_set
            .as_ref()
            .filter(|_| self.active())
            .map(|query_set| {
                self.mark_written(pass);
                wgpu::RenderPassTimestampWrites {
                    query_set,
                    beginning_of_pass_write_index: Some(start),
                    end_of_pass_write_index: Some(end),
                }
            })
    }

    pub fn compute_writes(&self, pass: TimedPass) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        let (start, end) = pass.query_indices();
        self.query_set
            .as_ref()
            .filter(|_| self.active())
            .map(|query_set| {
                self.mark_written(pass);
                wgpu::ComputePassTimestampWrites {
                    query_set,
                    beginning_of_pass_write_index: Some(start),
                    end_of_pass_write_index: Some(end),
                }
            })
    }

    /// Delimit work encoded directly on a command encoder.  This is used for
    /// multi-pass effects (LUT generation, bloom, exposure, and reconstruction)
    /// whose full span cannot be represented by one render/compute pass.  The
    /// caller must not also attach `writes`/`compute_writes` for the same pass.
    pub fn begin_encoder_span(&self, encoder: &mut wgpu::CommandEncoder, pass: TimedPass) {
        let Some(query_set) = self.query_set.as_ref().filter(|_| self.active()) else {
            return;
        };
        self.mark_written(pass);
        encoder.write_timestamp(query_set, pass.query_indices().0);
    }

    pub fn end_encoder_span(&self, encoder: &mut wgpu::CommandEncoder, pass: TimedPass) {
        let Some(query_set) = self.query_set.as_ref().filter(|_| self.active()) else {
            return;
        };
        encoder.write_timestamp(query_set, pass.query_indices().1);
    }

    fn mark_written(&self, pass: TimedPass) {
        self.written
            .set(self.written.get() | (1_u32 << pass.index()));
    }

    pub fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if !self.active() {
            return;
        }
        let rendered_frame_index = self.next_frame_index;
        self.next_frame_index = self.next_frame_index.wrapping_add(1);
        let mask = self.written.get();
        let Some(slot) = self.readbacks.iter_mut().find(|slot| !slot.pending) else {
            self.written.set(0);
            return;
        };
        if let Some(query_set) = &self.query_set {
            encoder.resolve_query_set(query_set, 0..QUERY_COUNT, &slot.resolve, 0);
            encoder.copy_buffer_to_buffer(&slot.resolve, 0, &slot.readback, 0, QUERY_BYTES);
        }
        slot.pending = true;
        slot.pending_mask = mask;
        slot.pending_frame_index = rendered_frame_index;
        self.written.set(0);
    }

    pub fn finish(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if !self.active() {
            return;
        }
        for slot in &mut self.readbacks {
            if slot.pending && slot.map_receiver.is_none() {
                let slice = slot.readback.slice(..);
                let (sender, receiver) = mpsc::channel();
                slice.map_async(wgpu::MapMode::Read, move |result| {
                    let _ = sender.send(result);
                });
                slot.map_receiver = Some(receiver);
            }
        }
        if let Err(error) = device.poll(wgpu::PollType::Poll) {
            log::debug!("GPU timing poll failed: {error}");
            return;
        }
        let period_ns = f64::from(queue.get_timestamp_period());
        for slot in &mut self.readbacks {
            let Some(receiver) = slot.map_receiver.as_ref() else {
                continue;
            };
            match receiver.try_recv() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    log::warn!("GPU timing map failed: {error}");
                    slot.map_receiver = None;
                    slot.pending = false;
                    continue;
                }
                Err(mpsc::TryRecvError::Empty) => continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    slot.map_receiver = None;
                    slot.pending = false;
                    continue;
                }
            }
            slot.map_receiver = None;
            let slice = slot.readback.slice(..);
            let Ok(mapped) = slice.get_mapped_range() else {
                slot.readback.unmap();
                slot.pending = false;
                continue;
            };
            let mut ticks = [0_u64; TIMED_PASS_COUNT * 2];
            for (tick, bytes) in ticks.iter_mut().zip(mapped.chunks_exact(8)) {
                let mut raw = [0_u8; 8];
                raw.copy_from_slice(bytes);
                *tick = u64::from_ne_bytes(raw);
            }
            drop(mapped);
            slot.readback.unmap();
            let mut passes_ms = [0.0; TIMED_PASS_COUNT];
            for pass in TimedPass::ALL {
                if slot.pending_mask & (1_u32 << pass.index()) == 0 {
                    continue;
                }
                let (start, end) = pass.query_indices();
                passes_ms[pass.index()] =
                    ticks[end as usize].saturating_sub(ticks[start as usize]) as f64 * period_ns
                        / 1_000_000.0;
            }
            let frame = GpuFrameTimings::from_passes_at(
                passes_ms,
                slot.pending_mask,
                slot.pending_frame_index,
            );
            if self.completed.len() == READBACK_RING_SIZE {
                self.completed.pop_front();
            }
            self.completed.push_back(frame);
            self.last_frame = Some(frame);
            slot.pending = false;
        }
    }

    pub fn take_last_frame(&mut self) -> Option<GpuFrameTimings> {
        self.completed.pop_front()
    }
}

#[cfg(test)]
mod tests;
