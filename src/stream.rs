use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use glam::{IVec3, Vec3};
use rayon::{ThreadPool, ThreadPoolBuilder};

use crate::lod::LodCache;
use crate::mesh::ChunkMeshes;
use crate::render::gi::{SlabRegion, UPLOAD_BUDGET_BYTES, VoxelUpload};
use crate::render::{GpuChunkMeshes, Renderer};
use crate::world::block::{AIR, DIRT, GRASS, RenderClass, SAND, STONE, WATER, def};
use crate::world::coords::chunk_of;
use crate::world::r#gen::SEA_LEVEL;
use crate::world::light::LightColumn;
use crate::world::save::SaveDir;
use crate::world::world::World;

use self::jobs::{Job, ResultMessage};

mod jobs;
mod lighting;
mod lod;
mod persistence;
mod rendering;
mod scheduling;

const DEFAULT_RADIUS: i32 = 10;
const MIN_RADIUS: i32 = 4;
const MAX_RADIUS: i32 = 16;
const URGENT_BUDGET: usize = 3;
const MAIN_BUDGET: Duration = Duration::from_millis(6);
const GEN_PUMP_BUDGET: Duration = Duration::from_millis(4);

struct PendingGiUpload {
    level: usize,
    revision: u32,
    region: SlabRegion,
    next_z: u32,
}

fn fallback_block(world: &World, bp: IVec3) -> u16 {
    if bp.y < 0 {
        return STONE;
    }
    if bp.y >= crate::world::coords::CHUNK_SIZE * crate::world::coords::WORLD_CHUNKS_Y {
        return AIR;
    }
    let height = world.generator().height_at(bp.x, bp.z);
    if bp.y < 1 || bp.y <= height - 4 {
        STONE
    } else if bp.y < height {
        DIRT
    } else if bp.y == height {
        if height > SEA_LEVEL { GRASS } else { SAND }
    } else if bp.y <= SEA_LEVEL {
        WATER
    } else {
        AIR
    }
}

fn upload_slice(
    world: &World,
    renderer: &mut Renderer,
    pending: &PendingGiUpload,
    z0: u32,
    depth: u32,
    material: &mut Vec<u8>,
    light: &mut Vec<u8>,
) {
    let region = pending.region;
    let level = pending.level;
    let cells_per_slice = region.extent.x as usize * region.extent.y as usize;
    let cells = cells_per_slice * depth as usize;
    material.clear();
    material.resize(cells * 4, 0);
    light.clear();
    light.resize(cells * 4, 0);
    for z in 0..depth {
        for y in 0..region.extent.y {
            for x in 0..region.extent.x {
                let index = ((z * region.extent.y + y) * region.extent.x + x) as usize;
                let cell = region.logical_start
                    + glam::IVec3::new(x as i32, y as i32, z0 as i32 + z as i32);
                let world_cell = cell * (1_i32 << level);
                let block = if world.is_loaded(chunk_of(world_cell)) {
                    world.get_block(world_cell)
                } else {
                    fallback_block(world, world_cell)
                };
                let packed = if world.is_loaded(chunk_of(world_cell)) {
                    world.get_light(world_cell)
                } else {
                    0xf0
                };
                let (material_cell, light_cell) = encode_gi_cell(block, packed);
                material[index * 4..index * 4 + 4].copy_from_slice(&material_cell);
                light[index * 4..index * 4 + 4].copy_from_slice(&light_cell);
            }
        }
    }
    let upload = VoxelUpload {
        level: level as u8,
        logical_origin: region.logical_start + glam::IVec3::new(0, 0, z0 as i32),
        extent: glam::UVec3::new(region.extent.x, region.extent.y, depth),
        material: std::mem::take(material),
        light: std::mem::take(light),
        revision: pending.revision,
    };
    renderer.upload_gi(&upload);
    *material = upload.material;
    *light = upload.light;
}

fn encode_gi_cell(block: u16, packed_light: u8) -> ([u8; 4], [u8; 4]) {
    let block_def = def(block);
    let albedo = crate::world::material::recipe(block_def.name)
        .map_or([128, 128, 128], |recipe| recipe.base_srgb);
    let opacity = match block_def.render_class {
        RenderClass::Opaque => u8::from(block != AIR) * 255,
        RenderClass::Cutout => 128,
        RenderClass::Translucent | RenderClass::Water => 0,
    };
    let block_radiance = f32::from(packed_light & 0x0f) / 15.0;
    let mut encoded_emission = [0_u8; 3];
    let block_light_rgb = [block_radiance, block_radiance * 0.42, block_radiance * 0.12];
    for ((encoded, emission), propagated) in encoded_emission
        .iter_mut()
        .zip(block_def.emission_rgb)
        .zip(block_light_rgb)
    {
        *encoded = ((emission + propagated) / 8.0 * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8;
    }
    (
        [albedo[0], albedo[1], albedo[2], opacity],
        [
            encoded_emission[0],
            encoded_emission[1],
            encoded_emission[2],
            (packed_light >> 4) * 17,
        ],
    )
}

pub struct Streamer {
    pool: ThreadPool,
    sender: Sender<ResultMessage>,
    receiver: Receiver<ResultMessage>,
    gen_inflight: HashSet<IVec3>,
    load_inflight: HashSet<IVec3>,
    mesh_inflight: HashSet<IVec3>,
    pub(crate) light_pending: VecDeque<LightColumn>,
    pub(crate) light_next_pending: HashSet<LightColumn>,
    pub(crate) light_inflight: HashSet<LightColumn>,
    light_urgent: HashSet<LightColumn>,
    light_solve_counts: HashMap<(LightColumn, u64), u32>,
    light_columns_seen: HashSet<LightColumn>,
    light_solves: usize,
    light_times: Vec<Duration>,
    max_light_requeues: u32,
    mesh_epochs: HashMap<IVec3, u64>,
    next_mesh_epoch: u64,
    dirty: HashMap<IVec3, bool>,
    completed_meshes: VecDeque<(IVec3, u64, u64, ChunkMeshes)>,
    pub(crate) completed_lights: VecDeque<(crate::world::light::LightColumnResult, Duration)>,
    radius: i32,
    started: Instant,
    settled: bool,
    save_dir: Option<SaveDir>,
    last_periodic_save: Instant,
    lod_cache: LodCache,
    gi_pending: VecDeque<PendingGiUpload>,
    gi_material_scratch: Vec<u8>,
    gi_light_scratch: Vec<u8>,
    gi_invalidated: bool,
    pub(crate) lod_pending: VecDeque<crate::lod::LodKey>,
}

impl Streamer {
    pub fn new() -> Self {
        Self::new_with_radius(configured_radius() as u32)
    }

    /// Creates a streamer with the persisted video view radius.
    pub fn new_with_radius(view_radius: u32) -> Self {
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
            light_pending: VecDeque::new(),
            light_next_pending: HashSet::new(),
            light_inflight: HashSet::new(),
            light_urgent: HashSet::new(),
            light_solve_counts: HashMap::new(),
            light_columns_seen: HashSet::new(),
            light_solves: 0,
            light_times: Vec::new(),
            max_light_requeues: 0,
            mesh_epochs: HashMap::new(),
            next_mesh_epoch: 0,
            dirty: HashMap::new(),
            completed_meshes: VecDeque::new(),
            completed_lights: VecDeque::new(),
            radius: view_radius.clamp(MIN_RADIUS as u32, MAX_RADIUS as u32) as i32,
            started: Instant::now(),
            settled: false,
            save_dir: None,
            last_periodic_save: Instant::now(),
            lod_cache: LodCache::new(),
            gi_pending: VecDeque::new(),
            gi_material_scratch: Vec::new(),
            gi_light_scratch: Vec::new(),
            gi_invalidated: false,
            lod_pending: VecDeque::new(),
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
            + self.light_pending.len()
            + self.light_next_pending.len()
            + self.light_inflight.len()
            + self.completed_lights.len()
            + self.completed_meshes.len()
            + self.gi_pending.len()
            + self.lod_pending.len()
    }

    /// CPU/GPU-budgeted hierarchical terrain cache owned by the streamer.
    pub fn lod_cache(&self) -> &LodCache {
        &self.lod_cache
    }

    pub fn lod_memory_bytes(&self) -> (usize, usize) {
        (self.lod_cache.cpu_bytes(), self.lod_cache.gpu_bytes())
    }

    /// Total bytes currently retained by the CPU and GPU LOD cache metadata.
    pub fn lod_cache_memory_bytes(&self) -> usize {
        let (cpu, gpu) = self.lod_memory_bytes();
        cpu.saturating_add(gpu)
    }

    pub fn mark_urgent(&mut self, cp: IVec3) {
        self.bump_mesh_epoch(cp);
        self.dirty.insert(cp, true);
        self.mark_light_urgent(cp);
    }

    pub fn bootstrap(
        &mut self,
        world: &mut World,
        renderer: &mut Renderer,
        gpu_chunks: &mut Vec<GpuChunkMeshes>,
        center: IVec3,
        camera_world: Vec3,
    ) {
        let bootstrap = self.desired(center, 1);
        for cp in &bootstrap {
            if !world.is_loaded(*cp) {
                self.load_or_generate(world, *cp);
            }
        }
        if let Err(error) = self.solve_lighting_sync(world) {
            log::error!("bootstrap lighting failed: {error}");
        }
        world.take_dirty();
        self.dirty.clear();
        for cp in bootstrap {
            self.mesh_and_upload(world, renderer, gpu_chunks, cp);
        }
        self.update_gi_clipmap(renderer, camera_world);
        self.populate_lod_rings(world, renderer, camera_world);
        while !self.gi_pending.is_empty() {
            self.drain_gi_uploads(world, renderer, usize::MAX);
        }
        self.lod_cache.set_active_ring_protection(camera_world);
    }

    pub fn update(
        &mut self,
        world: &mut World,
        renderer: &mut Renderer,
        gpu_chunks: &mut Vec<GpuChunkMeshes>,
        center: IVec3,
        camera_world: Vec3,
        visible_drawn: usize,
    ) {
        let desired = self.desired_set(center);
        self.drain_results(world, &desired);
        let budget_start = Instant::now();
        self.collect_dirty(world);
        self.pump_generation(world, &desired, center);
        self.pump_lighting(world, &desired, center, budget_start);
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
        if self.gi_invalidated {
            renderer.invalidate_gi_clipmap();
            self.gi_invalidated = false;
        }
        self.update_gi_clipmap(renderer, camera_world);
        self.populate_lod_rings(world, renderer, camera_world);
        self.drain_gi_uploads(world, renderer, UPLOAD_BUDGET_BYTES);
        self.lod_cache.set_active_ring_protection(camera_world);
        self.check_settled(world, &desired, visible_drawn);
    }

    fn update_gi_clipmap(&mut self, renderer: &mut Renderer, camera_world: Vec3) {
        for update in renderer.update_gi_clipmap(camera_world) {
            self.gi_pending
                .retain(|pending| pending.level != update.level);
            for region in update.regions {
                self.gi_pending.push_back(PendingGiUpload {
                    level: update.level,
                    revision: update.revision,
                    region,
                    next_z: 0,
                });
            }
        }
    }

    /// Populate every initial clipmap level before an offscreen capture. This
    /// deliberately bypasses the interactive per-frame budget.
    pub fn populate_gi_offline(
        &mut self,
        world: &World,
        renderer: &mut Renderer,
        camera_world: Vec3,
    ) {
        self.update_gi_clipmap(renderer, camera_world);
        while !self.gi_pending.is_empty() {
            self.drain_gi_uploads(world, renderer, usize::MAX);
        }
    }

    fn drain_gi_uploads(&mut self, world: &World, renderer: &mut Renderer, budget: usize) {
        let mut remaining = budget;
        let mut completed = Vec::new();
        while remaining >= 8 && !self.gi_pending.is_empty() {
            let mut pending = self
                .gi_pending
                .pop_front()
                .expect("GI queue checked non-empty");
            let cells_per_slice =
                pending.region.extent.x as usize * pending.region.extent.y as usize;
            let bytes_per_slice = cells_per_slice.saturating_mul(8);
            if bytes_per_slice == 0 || remaining < bytes_per_slice {
                self.gi_pending.push_front(pending);
                break;
            }
            let max_depth = (remaining / bytes_per_slice) as u32;
            let depth = max_depth.min(pending.region.extent.z - pending.next_z);
            upload_slice(
                world,
                renderer,
                &pending,
                pending.next_z,
                depth,
                &mut self.gi_material_scratch,
                &mut self.gi_light_scratch,
            );
            remaining = remaining.saturating_sub(cells_per_slice * depth as usize * 8);
            pending.next_z += depth;
            if pending.next_z < pending.region.extent.z {
                self.gi_pending.push_front(pending);
            } else {
                completed.push((pending.level, pending.revision));
            }
        }
        for (level, revision) in completed {
            if !self
                .gi_pending
                .iter()
                .any(|p| p.level == level && p.revision == revision)
            {
                renderer.mark_gi_level_ready(level, revision);
            }
        }
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
            || !self.light_pending.is_empty()
            || !self.light_inflight.is_empty()
            || !self.completed_lights.is_empty()
            || !self.completed_meshes.is_empty()
            || !self.dirty.is_empty()
            || !desired.iter().all(|&cp| {
                world.is_loaded(cp)
                    && world.light_column_initialized(LightColumn { x: cp.x, z: cp.z })
            })
        {
            return;
        }
        self.settled = true;
        self.log_lighting_stats();
        log::info!(
            "stream: settled in {:.2}s ({} chunks, {} drawn)",
            self.started.elapsed().as_secs_f64(),
            desired.len(),
            visible_drawn
        );
    }

    fn worker_limit(&self) -> usize {
        self.total_limit()
    }

    fn worker_threads(&self) -> usize {
        self.pool.current_num_threads()
    }
    fn total_limit(&self) -> usize {
        self.worker_threads() * 2
    }
    fn total_inflight(&self) -> usize {
        self.gen_inflight.len()
            + self.load_inflight.len()
            + self.light_inflight.len()
            + self.mesh_inflight.len()
    }

    fn lighting_busy(&self) -> bool {
        !self.light_pending.is_empty()
            || !self.light_next_pending.is_empty()
            || !self.light_inflight.is_empty()
            || !self.completed_lights.is_empty()
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
