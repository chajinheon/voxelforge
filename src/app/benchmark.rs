//! Deterministic autopilot and benchmark reporting for the windowed app.

use glam::{IVec3, Vec3};
use voxelforge::player::camera::EYE_HEIGHT;
use voxelforge::render::RenderView;
use voxelforge::ui::ActionRequest;
use voxelforge::world::block::{AIR, FENCE_BASE, PANE_BASE, SLAB_BASE, STAIR_BASE, STONE};
use voxelforge::world::coords::chunk_of;
use winit::event_loop::ActiveEventLoop;

use super::App;
use super::memory::MemoryAccounting;

const AUTOPILOT_PHASE_COUNT: usize = 6;
const FIRST_MEASURED_PHASE: usize = 1;
const LAST_MEASURED_PHASE: usize = AUTOPILOT_PHASE_COUNT - 1;
const INVENTORY_PHASE: usize = 4;
const HUD_GPU_P95_LIMIT_MS: f64 = 15.40;
const INVENTORY_GPU_P95_LIMIT_MS: f64 = 15.75;

/// Return the exact 6x600 autopilot phase for a rendered frame index.
/// Phase zero is the prescribed warmup and is intentionally not measured.
pub(super) const fn benchmark_phase(frame_index: u64) -> Option<usize> {
    let phase = (frame_index / 600) as usize;
    if phase >= FIRST_MEASURED_PHASE && phase <= LAST_MEASURED_PHASE {
        Some(phase)
    } else {
        None
    }
}

fn phase_label(phase: usize) -> &'static str {
    match phase {
        0 => "warmup",
        1 => "move-fly-stream",
        2 => "place-break-shapes",
        3 => "water-cloud-gi",
        4 => "inventory-search-stair",
        5 => "viewmodel-settled",
        _ => "unknown",
    }
}

fn percentile(values: &[f64], p: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    Some(sorted[((sorted.len() - 1) as f64 * p).round() as usize])
}

impl App {
    pub(super) fn autopilot_step(&mut self) {
        if !self.autopilot {
            return;
        }
        let phase = self.frames / 600;
        if phase != self.autopilot_phase {
            self.autopilot_phase = phase;
            log::info!(
                "autopilot: phase={} {}",
                phase,
                match phase {
                    0 => "warmup",
                    1 => "move-fly-stream",
                    2 => "place-break-cube-slab-stair-pane-fence",
                    3 => "water-cloud-gi-camera-route",
                    4 => "inventory-search-stair-hotbar-close",
                    _ => "viewmodel-settled",
                }
            );
        }
        match phase {
            1 => {
                self.body.fly = true;
                let t = self.frames as f32 * 0.025;
                // Cross at least one 32-block boundary so this phase exercises
                // generation, lighting and mesh streaming instead of orbiting
                // inside the bootstrap column.
                self.body.pos.x += 0.060;
                self.body.pos.z += t.sin() * 0.012;
                self.camera.pos = self.body.pos + Vec3::Y * EYE_HEIGHT;
                self.camera.yaw = 0.6 + t * 0.015;
            }
            2 => {
                let base = IVec3::new(
                    self.body.pos.x.floor() as i32 + 2,
                    self.body.pos.y.floor() as i32,
                    self.body.pos.z.floor() as i32 - 4,
                );
                let ids = [STONE, SLAB_BASE, STAIR_BASE, PANE_BASE, FENCE_BASE];
                let offset = (self.frames - 1200) / 120;
                if self.frames.is_multiple_of(120) {
                    let p = base + IVec3::new(offset as i32 % 5, 0, offset as i32 / 5);
                    let id = ids[(offset as usize) % ids.len()];
                    self.world.set_block(p, id);
                    self.streamer.mark_urgent(chunk_of(p));
                    log::info!("autopilot: edit action=place item={} block={:?}", id, p);
                }
                if self.frames % 300 == 299 {
                    let p = base + IVec3::new((offset as i32) % 5, 0, (offset as i32) / 5);
                    self.world.set_block(p, AIR);
                    self.streamer.mark_urgent(chunk_of(p));
                    log::info!("autopilot: edit action=break block={:?}", p);
                }
            }
            3 => {
                let t = (self.frames - 1800) as f32 * 0.02;
                self.camera.yaw = t.sin() * 0.8;
                self.body.fly = true;
                self.body.pos.x += t.cos() * 0.018;
                self.body.pos.z += t.sin() * 0.018;
                self.body.pos.y += t.sin() * 0.004;
                self.render_view = match self.frames % 300 {
                    0..=99 => RenderView::Water,
                    100..=199 => RenderView::Cloud,
                    _ => RenderView::Gi,
                };
            }
            4 => {
                if self.frames == 2400 {
                    self.toggle_inventory();
                }
                if self.frames == 2460 {
                    let _ = self.inventory.set_query_text("stair");
                    log::info!("autopilot: inventory query=stair");
                }
                if self.frames == 2520
                    && let Some(item) = self.inventory.visible_items(&self.ui_records).first()
                {
                    let id = item.id;
                    let _ = self.inventory.assign_item(id);
                    log::info!("autopilot: inventory hotbar_assign item={}", id);
                }
                if self.frames == 2700 {
                    self.toggle_inventory();
                }
            }
            _ => {
                match self.frames % 540 {
                    0 => self.viewmodel.start_action(ActionRequest::Place),
                    180 => self.viewmodel.start_action(ActionRequest::Break),
                    360 => self.viewmodel.start_action(ActionRequest::Switch {
                        from: self.inventory.hotbar[0],
                        to: self.inventory.hotbar[1],
                    }),
                    _ => {}
                }
                self.render_view = RenderView::Final;
            }
        }
    }

    pub(super) fn finish_benchmark(&mut self, event_loop: &ActiveEventLoop) {
        if self.frame_times_ms.is_empty() {
            log::error!("benchmark failed: no CPU frame samples after warmup");
            std::process::exit(1);
        }
        let mut frame = self.frame_times_ms.clone();
        frame.sort_by(f64::total_cmp);
        let pick =
            |values: &[f64], p: f64| values[((values.len() - 1) as f64 * p).round() as usize];
        let pick_u32 =
            |values: &[u32], p: f64| values[((values.len() - 1) as f64 * p).round() as usize];
        let mut gpu = self.gpu_times_ms.clone();
        gpu.sort_by(f64::total_cmp);
        let gpu_p95 = if gpu.is_empty() {
            0.0
        } else {
            pick(&gpu, 0.95)
        };
        let gpu_max = gpu.last().copied().unwrap_or(0.0);
        let phase_gpu_p95: [Option<f64>; AUTOPILOT_PHASE_COUNT] =
            std::array::from_fn(|phase| percentile(&self.gpu_phase_times_ms[phase], 0.95));
        let phase_contract_ok = if self.autopilot && self.smoke_frames == Some(3600) {
            (FIRST_MEASURED_PHASE..=LAST_MEASURED_PHASE)
                .all(|phase| self.frame_phase_times_ms[phase].len() == 600)
        } else {
            true
        };
        let phase_gpu_ok = (FIRST_MEASURED_PHASE..=LAST_MEASURED_PHASE).all(|phase| {
            let Some(p95) = phase_gpu_p95[phase] else {
                return false;
            };
            let limit = if phase == INVENTORY_PHASE {
                INVENTORY_GPU_P95_LIMIT_MS
            } else {
                HUD_GPU_P95_LIMIT_MS
            };
            p95 <= limit
        });
        for phase in FIRST_MEASURED_PHASE..=LAST_MEASURED_PHASE {
            let cpu = &self.frame_phase_times_ms[phase];
            let gpu = &self.gpu_phase_times_ms[phase];
            log::info!(
                "benchmark: phase={} label={} frames={} gpu_samples={} cpu_p95_ms={:?} gpu_p95_ms={:?} gpu_limit_ms={:.2}",
                phase,
                phase_label(phase),
                cpu.len(),
                gpu.len(),
                percentile(cpu, 0.95),
                phase_gpu_p95[phase],
                if phase == INVENTORY_PHASE {
                    INVENTORY_GPU_P95_LIMIT_MS
                } else {
                    HUD_GPU_P95_LIMIT_MS
                },
            );
        }
        let memory = MemoryAccounting::measure(
            &self.world,
            &self.streamer,
            self.renderer.as_ref(),
            &self.chunks,
        );
        let (allocation_median, allocation_p95) =
            if self.allocation_stats_enabled && !self.allocation_samples.is_empty() {
                let mut allocations = self.allocation_samples.clone();
                allocations.sort_unstable();
                (
                    Some(pick_u32(&allocations, 0.5)),
                    Some(pick_u32(&allocations, 0.95)),
                )
            } else {
                (None, None)
            };
        log::info!(
            "benchmark: frames={} frame_median_ms={:.3} frame_p95_ms={:.3} frame_max_ms={:.3} gpu_p95_ms={:.3} gpu_max_ms={:.3} settled_seconds={:?} memory_chunks={} allocation_median={} allocation_p95={} phase_contract_ok={} phase_gpu_ok={} {}",
            frame.len(),
            pick(&frame, 0.5),
            pick(&frame, 0.95),
            frame[frame.len() - 1],
            gpu_p95,
            gpu_max,
            self.settled_seconds,
            self.world.chunk_count(),
            allocation_median.map_or_else(|| "unavailable".to_owned(), |value| value.to_string()),
            allocation_p95.map_or_else(|| "unavailable".to_owned(), |value| value.to_string()),
            phase_contract_ok,
            phase_gpu_ok,
            memory.log_fields(),
        );
        let settled_ok = self.settled_seconds.is_some_and(|seconds| seconds < 5.0);
        let gpu_samples_ok = !gpu.is_empty();
        let gpu_ok = gpu_samples_ok && phase_contract_ok && phase_gpu_ok && gpu_max < 25.0;
        let frame_ok = frame[frame.len() - 1] < 25.0 && pick(&frame, 0.95) <= 16.6;
        let memory_ok = memory.total_bytes() < 1_350_000_000;
        let allocation_ok = !self.allocation_stats_enabled
            || allocation_median == Some(0) && allocation_p95 == Some(0);
        let passed = settled_ok && gpu_ok && frame_ok && memory_ok && allocation_ok;
        if !passed {
            log::error!(
                "benchmark failed: settled_ok={} gpu_samples_ok={} gpu_ok={} frame_ok={} memory_ok={} allocation_ok={} phase_contract_ok={} phase_gpu_ok={}",
                settled_ok,
                gpu_samples_ok,
                gpu_ok,
                frame_ok,
                memory_ok,
                allocation_ok,
                phase_contract_ok,
                phase_gpu_ok,
            );
        }
        self.save();
        event_loop.exit();
        if !passed {
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{benchmark_phase, percentile};

    #[test]
    fn benchmark_phase_has_exact_warmup_and_boundaries() {
        assert_eq!(benchmark_phase(0), None);
        assert_eq!(benchmark_phase(599), None);
        assert_eq!(benchmark_phase(600), Some(1));
        assert_eq!(benchmark_phase(1199), Some(1));
        assert_eq!(benchmark_phase(1200), Some(2));
        assert_eq!(benchmark_phase(2999), Some(4));
        assert_eq!(benchmark_phase(3000), Some(5));
        assert_eq!(benchmark_phase(3599), Some(5));
        assert_eq!(benchmark_phase(3600), None);
    }

    #[test]
    fn percentile_is_empty_safe_and_deterministic() {
        assert_eq!(percentile(&[], 0.95), None);
        assert_eq!(percentile(&[4.0, 1.0, 3.0, 2.0], 0.5), Some(3.0));
        assert_eq!(percentile(&[4.0, 1.0, 3.0, 2.0], 0.95), Some(4.0));
    }
}
