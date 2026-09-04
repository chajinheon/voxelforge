use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use glam::IVec3;
use rayon::{ThreadPool, ThreadPoolBuilder};

use crate::mesh::{ChunkMeshes, mesh_chunk_greedy_all};
use crate::render::{GpuChunkMeshes, Renderer};
use crate::world::chunk::PaddedChunk;
use crate::world::coords::WORLD_CHUNKS_Y;
use crate::world::r#gen::WorldGen;
use crate::world::save::SaveDir;
use crate::world::world::World;

mod persistence;
mod rendering;

const DEFAULT_RADIUS: i32 = 10;
const MIN_RADIUS: i32 = 4;
const MAX_RADIUS: i32 = 16;
const URGENT_BUDGET: usize = 3;
const MAIN_BUDGET: Duration = Duration::from_millis(6);
const GEN_PUMP_BUDGET: Duration = Duration::from_millis(4);

enum Job {
    Gen {
        cp: IVec3,
        generator: Arc<WorldGen>,
    },
    Load {
        cp: IVec3,
        save: SaveDir,
    },
    Mesh {
        cp: IVec3,
        padded: Box<PaddedChunk>,
        version: u64,
        epoch: u64,
    },
}

enum ResultMessage {
    Gen {
        cp: IVec3,
        chunk: crate::world::chunk::Chunk,
    },
    Load {
        cp: IVec3,
        result: Result<Option<crate::world::chunk::Chunk>, String>,
    },
    Mesh {
        cp: IVec3,
        version: u64,
        epoch: u64,
        mesh: ChunkMeshes,
    },
}

pub struct Streamer {
    pool: ThreadPool,
    sender: Sender<ResultMessage>,
    receiver: Receiver<ResultMessage>,
    gen_inflight: HashSet<IVec3>,
    load_inflight: HashSet<IVec3>,
    mesh_inflight: HashSet<IVec3>,
    mesh_epochs: HashMap<IVec3, u64>,
    next_mesh_epoch: u64,
    dirty: HashMap<IVec3, bool>,
    completed_meshes: VecDeque<(IVec3, u64, u64, ChunkMeshes)>,
    radius: i32,
    started: Instant,
    settled: bool,
    save_dir: Option<SaveDir>,
    last_periodic_save: Instant,
}

impl Streamer {
    pub fn new() -> Self {
        let workers = thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(2)
            .saturating_sub(2)
            .clamp(2, 8);
        let pool = ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .expect("stream worker pool must build");
        let (sender, receiver) = mpsc::channel();
        Self {
            pool,
            sender,
            receiver,
            gen_inflight: HashSet::new(),
            load_inflight: HashSet::new(),
            mesh_inflight: HashSet::new(),
            mesh_epochs: HashMap::new(),
            next_mesh_epoch: 0,
            dirty: HashMap::new(),
            completed_meshes: VecDeque::new(),
            radius: configured_radius(),
            started: Instant::now(),
            settled: false,
            save_dir: None,
            last_periodic_save: Instant::now(),
        }
    }

    pub fn radius(&self) -> i32 {
        self.radius
    }

    pub fn queue_len(&self) -> usize {
        self.dirty.len()
            + self.gen_inflight.len()
            + self.load_inflight.len()
            + self.mesh_inflight.len()
            + self.completed_meshes.len()
    }

    pub fn mark_urgent(&mut self, cp: IVec3) {
        self.bump_mesh_epoch(cp);
        self.dirty.insert(cp, true);
    }

    pub fn bootstrap(
        &mut self,
        world: &mut World,
        renderer: &mut Renderer,
        gpu_chunks: &mut Vec<GpuChunkMeshes>,
        center: IVec3,
    ) {
        let bootstrap = self.desired(center, 1);
        for cp in &bootstrap {
            if !world.is_loaded(*cp) {
                self.load_or_generate(world, *cp);
            }
        }
        world.take_dirty();
        self.dirty.clear();
        for cp in bootstrap {
            self.mesh_and_upload(world, renderer, gpu_chunks, cp);
        }
    }

    pub fn update(
        &mut self,
        world: &mut World,
        renderer: &mut Renderer,
        gpu_chunks: &mut Vec<GpuChunkMeshes>,
        center: IVec3,
        visible_drawn: usize,
    ) {
        let desired = self.desired_set(center);
        self.drain_results(world, &desired);
        self.collect_dirty(world);
        self.pump_generation(world, &desired, center);
        let budget_start = Instant::now();
        // Edited chunks are synchronous and take priority over queued worker
        // results and new regular mesh copies, so an edit is visible this frame.
        self.mesh_urgent(world, renderer, gpu_chunks, &desired, center, budget_start);
        self.upload_completed(world, renderer, gpu_chunks, &desired, budget_start);
        self.issue_mesh(world, renderer, gpu_chunks, &desired, center, budget_start);
        if self.last_periodic_save.elapsed() >= Duration::from_secs(30) {
            self.save_now(world);
        }
        // Save before unloading so modified chunks always have their in-memory
        // bytes available to the persistence pass.
        if self.save_modified_before_unload(world) {
            world.unload_outside(center, self.radius + 2);
        }
        self.dirty
            .retain(|cp, _| desired.contains(cp) && world.is_loaded(*cp));
        self.remove_unloaded_gpu_chunks(world, renderer, gpu_chunks);
        self.check_settled(world, &desired, visible_drawn);
    }

    fn desired(&self, center: IVec3, radius: i32) -> Vec<IVec3> {
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

    fn desired_set(&self, center: IVec3) -> HashSet<IVec3> {
        self.desired(center, self.radius).into_iter().collect()
    }

    fn drain_results(&mut self, world: &mut World, desired: &HashSet<IVec3>) {
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
            }
        }
    }

    fn upload_completed(
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

    fn collect_dirty(&mut self, world: &mut World) {
        for cp in world.take_dirty() {
            if world.is_loaded(cp) {
                self.bump_mesh_epoch(cp);
                self.dirty.entry(cp).or_insert(false);
            }
        }
    }

    fn pump_generation(&mut self, world: &mut World, desired: &HashSet<IVec3>, center: IVec3) {
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
            while self.gen_inflight.len() + self.load_inflight.len() < limit {
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

    fn issue_mesh(
        &mut self,
        world: &mut World,
        renderer: &mut Renderer,
        gpu_chunks: &mut Vec<GpuChunkMeshes>,
        desired: &HashSet<IVec3>,
        center: IVec3,
        budget_start: Instant,
    ) {
        let limit = self.worker_limit();
        let available = limit.saturating_sub(self.mesh_inflight.len());
        if available == 0 {
            return;
        }
        let mut pending: Vec<_> = self
            .dirty
            .iter()
            .filter(|(cp, urgent)| {
                desired.contains(cp)
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

    fn mesh_urgent(
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
            self.dirty.remove(&cp);
            if world.chunk(cp).is_some_and(|chunk| chunk.is_empty()) {
                self.remove_gpu_chunk(renderer, gpu_chunks, cp);
            } else if world.is_loaded(cp) {
                self.mesh_and_upload(world, renderer, gpu_chunks, cp);
            }
        }
    }

    fn spawn(&self, job: Job) {
        let sender = self.sender.clone();
        self.pool.spawn(move || match job {
            Job::Gen { cp, generator } => {
                let chunk = generator.generate(cp);
                let _ = sender.send(ResultMessage::Gen { cp, chunk });
            }
            Job::Load { cp, save } => {
                let result = save.read_chunk(cp).map_err(|error| format!("{error:#}"));
                let _ = sender.send(ResultMessage::Load { cp, result });
            }
            Job::Mesh {
                cp,
                padded,
                version,
                epoch,
            } => {
                let mesh = mesh_chunk_greedy_all(&padded);
                let _ = sender.send(ResultMessage::Mesh {
                    cp,
                    version,
                    epoch,
                    mesh,
                });
            }
        });
    }

    fn bump_mesh_epoch(&mut self, cp: IVec3) {
        self.next_mesh_epoch = self.next_mesh_epoch.wrapping_add(1);
        self.mesh_epochs.insert(cp, self.next_mesh_epoch);
    }

    fn check_settled(&mut self, world: &World, desired: &HashSet<IVec3>, visible_drawn: usize) {
        if self.settled
            || !self.gen_inflight.is_empty()
            || !self.load_inflight.is_empty()
            || !self.mesh_inflight.is_empty()
            || !self.completed_meshes.is_empty()
            || !self.dirty.is_empty()
            || !desired.iter().all(|&cp| world.is_loaded(cp))
        {
            return;
        }
        self.settled = true;
        log::info!(
            "stream: settled in {:.2}s ({} chunks, {} drawn)",
            self.started.elapsed().as_secs_f64(),
            desired.len(),
            visible_drawn
        );
    }

    fn worker_limit(&self) -> usize {
        self.pool.current_num_threads() * 2
    }
}

impl Default for Streamer {
    fn default() -> Self {
        Self::new()
    }
}

fn configured_radius() -> i32 {
    std::env::var("VF_RADIUS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|&radius: &i32| (MIN_RADIUS..=MAX_RADIUS).contains(&radius))
        .unwrap_or(DEFAULT_RADIUS)
}

fn distance_key(cp: IVec3, center: IVec3) -> (i32, i32, i32) {
    let dx = cp.x - center.x;
    let dy = cp.y - center.y;
    let dz = cp.z - center.z;
    (dx * dx + dz * dz, dy.abs(), cp.y)
}
