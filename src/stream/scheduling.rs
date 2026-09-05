//! Scheduling and frame-budget coordination for streamed chunks.

use std::collections::{HashSet, VecDeque};
use std::thread;
use std::time::Instant;

use glam::IVec3;

use crate::render::{GpuChunkMeshes, Renderer};
use crate::world::coords::WORLD_CHUNKS_Y;
use crate::world::light::LightColumn;
use crate::world::world::World;

use super::jobs::{self, Job, ResultMessage};
use super::{GEN_PUMP_BUDGET, MAIN_BUDGET, Streamer, URGENT_BUDGET, distance_key};

impl Streamer {
    pub(super) fn desired(&self, center: IVec3, radius: i32) -> Vec<IVec3> {
        let mut desired = Vec::with_capacity(((radius * 2 + 1).pow(2) * WORLD_CHUNKS_Y) as usize);
        for z in -radius..=radius {
            for x in -radius..=radius {
                for y in 0..WORLD_CHUNKS_Y {
                    desired.push(IVec3::new(center.x + x, y, center.z + z));
                }
            }
        }
        desired.sort_by_key(|cp| {
            let dx = cp.x - center.x;
            let dy = cp.y - center.y;
            let dz = cp.z - center.z;
            (dx * dx + dz * dz, dy.abs(), cp.y)
        });
        desired
    }

    pub(super) fn desired_set(&self, center: IVec3) -> HashSet<IVec3> {
        self.desired(center, self.radius).into_iter().collect()
    }

    pub(super) fn drain_results(&mut self, world: &mut World, desired: &HashSet<IVec3>) {
        while let Ok(result) = self.receiver.try_recv() {
            match result {
                ResultMessage::Gen { cp, chunk } => {
                    self.gen_inflight.remove(&cp);
                    if desired.contains(&cp) && !world.is_loaded(cp) {
                        world.insert_generated(cp, chunk);
                    }
                }
                ResultMessage::Load { cp, result } => {
                    self.handle_load_result(cp, result, world, desired);
                }
                ResultMessage::Mesh {
                    cp,
                    version,
                    epoch,
                    mesh,
                } => {
                    self.mesh_inflight.remove(&cp);
                    if desired.contains(&cp) {
                        self.completed_meshes.push_back((cp, version, epoch, mesh));
                    } else if world.is_loaded(cp) {
                        self.dirty.entry(cp).or_insert(false);
                    }
                }
                // M6.1 defines the worker result now; scheduling and applying
                // lighting remain M6.2 responsibilities.
                ResultMessage::Light { result, elapsed } => {
                    self.light_inflight.remove(&result.column);
                    self.completed_lights.push_back((result, elapsed));
                }
            }
        }
    }

    pub(super) fn upload_completed(
        &mut self,
        world: &mut World,
        renderer: &mut Renderer,
        gpu_chunks: &mut Vec<GpuChunkMeshes>,
        desired: &HashSet<IVec3>,
        budget_start: Instant,
    ) {
        while budget_start.elapsed() < MAIN_BUDGET {
            let Some((cp, version, epoch, mesh)) = self.completed_meshes.pop_front() else {
                break;
            };
            if desired.contains(&cp)
                && world.chunk_version(cp) == Some(version)
                && self.mesh_epochs.get(&cp).copied() == Some(epoch)
            {
                self.upload_mesh(world, renderer, gpu_chunks, cp, mesh);
            } else if world.is_loaded(cp) && desired.contains(&cp) {
                self.dirty.entry(cp).or_insert(false);
            }
        }
    }

    pub(super) fn collect_dirty(&mut self, world: &mut World) {
        for cp in world.take_dirty() {
            if world.is_loaded(cp) {
                self.gi_invalidated = true;
                self.lod_cache.invalidate_base_edit(cp);
                self.bump_mesh_epoch(cp);
                self.dirty.entry(cp).or_insert(false);
            }
        }
    }

    pub(super) fn pump_generation(
        &mut self,
        world: &mut World,
        desired: &HashSet<IVec3>,
        center: IVec3,
    ) {
        let limit = self.worker_limit();
        let mut missing: Vec<_> = desired
            .iter()
            .filter(|&&cp| {
                !world.is_loaded(cp)
                    && !self.gen_inflight.contains(&cp)
                    && !self.load_inflight.contains(&cp)
            })
            .copied()
            .collect();
        missing.sort_by_key(|cp| distance_key(*cp, center));
        let mut missing = VecDeque::from(missing);
        let started = Instant::now();

        loop {
            while self.total_inflight() < limit {
                let Some(cp) = missing.pop_front() else {
                    break;
                };
                if world.is_loaded(cp) {
                    continue;
                }
                if let Some(save) = self.saved_chunk(cp) {
                    if self.load_inflight.insert(cp) {
                        self.spawn(Job::Load { cp, save });
                    }
                } else if self.gen_inflight.insert(cp) {
                    self.spawn(Job::Gen {
                        cp,
                        generator: world.generator().clone(),
                    });
                }
            }
            if missing.is_empty()
                || started.elapsed() >= GEN_PUMP_BUDGET
                || self.gen_inflight.is_empty()
            {
                break;
            }

            // Fast empty-chunk jobs can finish well inside one display frame.
            // Reap and refill here so the 2w in-flight cap does not become an
            // accidental 2w-per-frame throughput cap.
            thread::yield_now();
            self.drain_results(world, desired);
            self.collect_dirty(world);
        }
    }

    pub(super) fn issue_mesh(
        &mut self,
        world: &mut World,
        renderer: &mut Renderer,
        gpu_chunks: &mut Vec<GpuChunkMeshes>,
        desired: &HashSet<IVec3>,
        center: IVec3,
        budget_start: Instant,
    ) {
        if self.lighting_busy() {
            return;
        }
        let available = self.total_limit().saturating_sub(self.total_inflight());
        if available == 0 {
            return;
        }
        let mut pending: Vec<_> = self
            .dirty
            .iter()
            .filter(|(cp, urgent)| {
                desired.contains(cp)
                    && world.light_column_initialized(LightColumn { x: cp.x, z: cp.z })
                    && !**urgent
                    && !self.mesh_inflight.contains(cp)
                    && !self
                        .completed_meshes
                        .iter()
                        .any(|(queued, _, _, _)| queued == *cp)
            })
            .map(|(&cp, _)| cp)
            .collect();
        pending.sort_by_key(|&cp| distance_key(cp, center));
        let mut spawned = 0usize;
        for cp in pending {
            if budget_start.elapsed() >= MAIN_BUDGET {
                break;
            }
            self.dirty.remove(&cp);
            if world.chunk(cp).is_none() {
                continue;
            }
            if world.chunk(cp).is_some_and(|chunk| chunk.is_empty()) {
                self.remove_gpu_chunk(renderer, gpu_chunks, cp);
                continue;
            }
            let Some(version) = world.chunk_version(cp) else {
                continue;
            };
            let epoch = self.mesh_epochs.get(&cp).copied().unwrap_or(0);
            let padded = Box::new(world.padded(cp));
            self.mesh_inflight.insert(cp);
            self.spawn(Job::Mesh {
                cp,
                padded,
                version,
                epoch,
            });
            spawned += 1;
            if spawned == available {
                break;
            }
        }
    }

    pub(super) fn mesh_urgent(
        &mut self,
        world: &mut World,
        renderer: &mut Renderer,
        gpu_chunks: &mut Vec<GpuChunkMeshes>,
        desired: &HashSet<IVec3>,
        center: IVec3,
        budget_start: Instant,
    ) {
        let mut urgent: Vec<_> = self
            .dirty
            .iter()
            .filter(|(cp, is_urgent)| desired.contains(cp) && **is_urgent)
            .map(|(&cp, _)| cp)
            .collect();
        urgent.sort_by_key(|&cp| distance_key(cp, center));
        for cp in urgent.into_iter().take(URGENT_BUDGET) {
            if budget_start.elapsed() >= MAIN_BUDGET {
                break;
            }
            if !world.light_column_initialized(LightColumn { x: cp.x, z: cp.z }) {
                continue;
            }
            self.dirty.remove(&cp);
            if world.chunk(cp).is_some_and(|chunk| chunk.is_empty()) {
                self.remove_gpu_chunk(renderer, gpu_chunks, cp);
            } else if world.is_loaded(cp)
                && world.light_column_initialized(LightColumn { x: cp.x, z: cp.z })
            {
                self.mesh_and_upload(world, renderer, gpu_chunks, cp);
            }
        }
    }

    pub(super) fn spawn(&self, job: Job) {
        let sender = self.sender.clone();
        self.pool.spawn(move || jobs::execute(job, sender));
    }
}
