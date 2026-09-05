use crate::lod::{
    GRID_SIZE, LodGrid, LodKey, apply_cell_blocking, downsample_base, downsample_light_step,
    downsample_lod, lod_ring_ranges_default, modal_block,
};
use crate::mesh::ChunkMeshes;
use crate::mesh::lod::mesh_lod_grid_with_neighbors;
use crate::render::{GpuChunkMeshes, Renderer};
use crate::world::chunk::Chunk;
use crate::world::coords::{CHUNK_SIZE, chunk_of};
use crate::world::light::LightColumn;
use crate::world::world::World;
use glam::Vec3Swizzles;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::Streamer;
use super::lod::generated_block;

impl Streamer {
    /// Offline capture path: drain the same persistent queue without the
    /// interactive 10ms frame budget. This is intentionally not called by the
    /// live update loop, where the budget is part of the settle contract.
    pub fn populate_lod_rings_offline(
        &mut self,
        world: &World,
        renderer: &mut Renderer,
        camera: glam::Vec3,
    ) {
        // A surface-free fixture only needs a deterministic dependency cone
        // for proving all four debug colours.  The live streamer still uses
        // the complete R10 annuli; keeping this capture cone small prevents a
        // screenshot from synthesizing hundreds of off-camera grids.
        if self.lod_pending.is_empty() {
            self.enqueue_lod_fixture_keys(camera);
        }
        loop {
            self.populate_lod_rings(world, renderer, camera);
            if self.lod_pending.is_empty() {
                break;
            }
        }
    }
    fn enqueue_lod_fixture_keys(&mut self, camera: glam::Vec3) {
        let ranges = lod_ring_ranges_default();
        let mut all = Vec::new();
        self.enqueue_lod_ring_keys(camera, &ranges);
        while let Some(key) = self.lod_pending.pop_front() {
            all.push(key);
        }
        let camera_xz = glam::Vec2::new(camera.x, camera.z);
        let nearest = |level: u8| {
            all.iter()
                .copied()
                .filter(|key| key.level == level && key.coord.y == 0)
                .min_by_key(|key| {
                    let center = key.world_origin().as_vec3()
                        + glam::Vec3::splat(16.0 * key.cell_size() as f32);
                    (center.xz() - camera_xz).length() as i32
                })
        };
        let Some(level3) = nearest(3) else { return };
        let level2 = (0_i32..8).map(|index| {
            LodKey::new(
                2,
                level3.coord * 2 + glam::IVec3::new(index & 1, (index >> 1) & 1, (index >> 2) & 1),
            )
        });
        let mut selected = std::collections::HashSet::new();
        selected.insert(level3);
        for key in level2 {
            selected.insert(key);
            for index in 0_i32..8 {
                selected.insert(LodKey::new(
                    1,
                    key.coord * 2 + glam::IVec3::new(index & 1, (index >> 1) & 1, (index >> 2) & 1),
                ));
            }
        }
        if let Some(level1) = nearest(1) {
            selected.insert(level1);
        }
        let mut selected: Vec<_> = selected.into_iter().collect();
        selected.sort_by_key(|key| (key.level, key.coord.x, key.coord.y, key.coord.z));
        self.lod_pending.extend(selected);
    }
    pub fn populate_lod_rings(
        &mut self,
        world: &World,
        renderer: &mut Renderer,
        camera: glam::Vec3,
    ) {
        let ranges = lod_ring_ranges_default();
        if self.lod_pending.is_empty() {
            self.enqueue_lod_ring_keys(camera, &ranges);
        }
        self.generate_procedural_lod_parallel(world, renderer);
        let mut saved = HashMap::<glam::IVec3, Option<Chunk>>::new();
        let started = Instant::now();
        while let Some(key) = self.lod_pending.pop_front() {
            if started.elapsed() >= Duration::from_millis(10) {
                self.lod_pending.push_front(key);
                break;
            }
            if self.lod_cache.grid(key).is_none() {
                if let Some(grid) = self.build_lod_grid(world, key, &mut saved) {
                    self.insert_lod_grid(renderer, grid);
                } else {
                    self.lod_pending.push_back(key);
                }
            }
        }
        self.lod_cache.set_active_ring_protection(camera);
    }

    fn enqueue_lod_ring_keys(&mut self, camera: glam::Vec3, ranges: &[crate::lod::LodRange; 4]) {
        let camera_xz = glam::Vec2::new(camera.x, camera.z);
        for level in 1..=3_u8 {
            let span = GRID_SIZE as i32 * (1_i32 << level);
            let center = glam::IVec3::new(
                (camera.x / span as f32).floor() as i32,
                0,
                (camera.z / span as f32).floor() as i32,
            );
            let extent = ((ranges[level as usize].end + span - 1) / span + 1).max(1);
            let y_end = match level {
                1 => 2,
                2 => 1,
                _ => 0,
            };
            let min_distance = ranges[level as usize].start as f32;
            let max_distance = ranges[level as usize].end as f32;
            for y in 0..=y_end {
                for z in center.z - extent..=center.z + extent {
                    for x in center.x - extent..=center.x + extent {
                        let key = LodKey::new(level, glam::IVec3::new(x, y, z));
                        let origin = key.world_origin();
                        let center = glam::Vec2::new(origin.x as f32, origin.z as f32)
                            + glam::Vec2::splat(span as f32 * 0.5);
                        if (min_distance..=max_distance).contains(&center.distance(camera_xz)) {
                            self.lod_pending.push_back(key);
                        }
                    }
                }
            }
        }
    }

    fn build_lod_grid(
        &mut self,
        world: &World,
        key: LodKey,
        saved: &mut HashMap<glam::IVec3, Option<Chunk>>,
    ) -> Option<crate::lod::LodGrid> {
        if key.level == 1 {
            // Distant source chunks are almost always procedural and need not
            // materialize a 96 KiB Chunk just to read eight samples.  Sample
            // the generator directly while retaining exact World/Save chunks
            // for modified or resident terrain.
            return Some(self.downsample_source_grid(world, key, saved));
        }
        let child_key = LodKey::new(key.level - 1, key.coord * 2);
        let children: [Option<&crate::lod::LodGrid>; 8] = std::array::from_fn(|index| {
            let index = index as i32;
            let child = LodKey::new(
                child_key.level,
                child_key.coord + glam::IVec3::new(index & 1, (index >> 1) & 1, (index >> 2) & 1),
            );
            self.lod_cache.grid(child)
        });
        let children = children.into_iter().collect::<Option<Vec<_>>>()?;
        let children: [&crate::lod::LodGrid; 8] = std::array::from_fn(|i| children[i]);
        Some(crate::lod::downsample_lod(key, children))
    }

    fn downsample_source_grid(
        &mut self,
        world: &World,
        key: LodKey,
        saved: &mut HashMap<glam::IVec3, Option<Chunk>>,
    ) -> LodGrid {
        let base = key.coord * 2;
        let sources: [Option<Chunk>; 8] = std::array::from_fn(|index| {
            let index = index as i32;
            let cp = base + glam::IVec3::new(index & 1, (index >> 1) & 1, (index >> 2) & 1);
            if let Some(chunk) = world.chunk(cp) {
                return Some(chunk.clone());
            }
            saved
                .entry(cp)
                .or_insert_with(|| {
                    self.save_dir
                        .as_ref()
                        .and_then(|dir| dir.read_chunk(cp).ok().flatten())
                })
                .clone()
        });
        if sources.iter().all(Option::is_none) {
            return super::lod::generated_lod_grid(world, key);
        }
        let mut result = LodGrid::empty(key);
        let mut heights = HashMap::<(i32, i32), i32>::with_capacity(4096);
        for y in 0..GRID_SIZE {
            for z in 0..GRID_SIZE {
                for x in 0..GRID_SIZE {
                    let mut ids = [crate::world::block::AIR; 8];
                    let mut lights = [0_u8; 8];
                    for sample in 0..8 {
                        let fine = glam::IVec3::new(
                            key.world_origin().x + x as i32 * 2 + (sample & 1) as i32,
                            key.world_origin().y + y as i32 * 2 + ((sample >> 1) & 1) as i32,
                            key.world_origin().z + z as i32 * 2 + ((sample >> 2) & 1) as i32,
                        );
                        let cp = crate::world::coords::chunk_of(fine);
                        let slot = ((cp.x - base.x) & 1)
                            | (((cp.y - base.y) & 1) << 1)
                            | (((cp.z - base.z) & 1) << 2);
                        let local = crate::world::coords::local_of(fine);
                        if let Some(chunk) = &sources[slot as usize] {
                            ids[sample] = chunk.get(local);
                            lights[sample] = chunk.get_light(local);
                        } else {
                            let height = *heights
                                .entry((fine.x, fine.z))
                                .or_insert_with(|| world.generator().height_at(fine.x, fine.z));
                            ids[sample] = generated_block(fine.y, height);
                            lights[sample] = 0xf0;
                        }
                    }
                    let index = LodGrid::index(glam::UVec3::new(x as u32, y as u32, z as u32));
                    result.blocks[index] = modal_block(ids);
                    result.light[index] = apply_cell_blocking(
                        downsample_light_step(lights, key.cell_size() as u8),
                        result.blocks[index],
                    );
                }
            }
        }
        result
    }

    pub(super) fn mesh_and_upload(
        &mut self,
        world: &World,
        renderer: &mut Renderer,
        gpu_chunks: &mut Vec<GpuChunkMeshes>,
        cp: glam::IVec3,
    ) {
        self.refresh_lod_ancestors(world, renderer, cp);
        if !world.light_column_initialized(LightColumn { x: cp.x, z: cp.z }) {
            return;
        }
        if world.chunk(cp).is_some_and(|chunk| chunk.is_empty()) {
            self.remove_gpu_chunk(renderer, gpu_chunks, cp);
            return;
        }
        self.upload_mesh(
            world,
            renderer,
            gpu_chunks,
            cp,
            crate::mesh::mesh_chunk_greedy_all(&world.padded(cp)),
        );
    }

    /// Rebuild the three ancestors affected by a base-chunk change.  The
    /// finest LOD grid is sourced from the eight resident world chunks; higher
    /// levels are recursively downsampled from the cache.  Mesh byte counts
    /// are registered in the bounded GPU budget so eviction and telemetry are
    /// real even while the full-detail pipeline is active.
    pub(super) fn refresh_lod_ancestors(
        &mut self,
        world: &World,
        renderer: &mut Renderer,
        base: glam::IVec3,
    ) {
        for key in crate::lod::lod_ancestor_keys(base) {
            renderer.remove_lod_mesh(key);
        }
        self.lod_cache.invalidate_base_edit(base);
        let level1 = base.div_euclid(glam::IVec3::splat(2));
        let children: [Option<&crate::world::chunk::Chunk>; 8] = std::array::from_fn(|index| {
            let index = index as i32;
            let cp = level1 * 2 + glam::IVec3::new(index & 1, (index >> 1) & 1, (index >> 2) & 1);
            world.chunk(cp)
        });
        let Some(children) = children.into_iter().collect::<Option<Vec<_>>>() else {
            return;
        };
        let refs: [&crate::world::chunk::Chunk; 8] = std::array::from_fn(|i| children[i]);
        let grid1 = downsample_base(LodKey::new(1, level1), refs);
        self.insert_lod_grid(renderer, grid1);
        self.rebuild_higher_lod(renderer, LodKey::new(1, level1));
    }

    pub(super) fn insert_lod_grid(&mut self, renderer: &mut Renderer, grid: crate::lod::LodGrid) {
        let key = grid.key;
        let distance = key.coord.x.abs() + key.coord.y.abs() + key.coord.z.abs();
        // Protection is refreshed from the camera ring after streaming work;
        // newly generated entries must not pin the cache between refreshes.
        if self.lod_cache.insert_grid(grid, distance, false) {
            let Some(cached) = self.lod_cache.grid(key) else {
                return;
            };
            let neighbors = [
                self.lod_cache
                    .grid(LodKey::new(key.level, key.coord + glam::IVec3::X)),
                self.lod_cache
                    .grid(LodKey::new(key.level, key.coord - glam::IVec3::X)),
                self.lod_cache
                    .grid(LodKey::new(key.level, key.coord + glam::IVec3::Y)),
                self.lod_cache
                    .grid(LodKey::new(key.level, key.coord - glam::IVec3::Y)),
                self.lod_cache
                    .grid(LodKey::new(key.level, key.coord + glam::IVec3::Z)),
                self.lod_cache
                    .grid(LodKey::new(key.level, key.coord - glam::IVec3::Z)),
            ];
            let mesh = mesh_lod_grid_with_neighbors(cached, |local| {
                let mut wrapped = local;
                let mut neighbor_index = None;
                for axis in 0..3 {
                    if wrapped[axis] < 0 {
                        wrapped[axis] += GRID_SIZE as i32;
                        neighbor_index = Some(axis * 2 + 1);
                    } else if wrapped[axis] >= GRID_SIZE as i32 {
                        wrapped[axis] -= GRID_SIZE as i32;
                        neighbor_index = Some(axis * 2);
                    }
                }
                neighbor_index
                    .and_then(|index| neighbors[index])
                    .map_or(crate::world::block::AIR, |grid| {
                        grid.get(wrapped.as_uvec3())
                    })
            });
            let bytes = mesh.vertices.len() * std::mem::size_of::<crate::mesh::lod::LodVertex>()
                + mesh.indices.len() * std::mem::size_of::<u32>();
            renderer.upload_lod_mesh(key, &mesh);
            let _ = self.lod_cache.insert_gpu_mesh(key, bytes, distance, false);
            renderer.sync_lod_meshes(&self.lod_cache);
        }
    }

    fn rebuild_higher_lod(&mut self, renderer: &mut Renderer, child_key: LodKey) {
        if child_key.level >= 3 {
            return;
        }
        let key = LodKey::new(
            child_key.level + 1,
            child_key.coord.div_euclid(glam::IVec3::splat(2)),
        );
        let child_keys: [LodKey; 8] = std::array::from_fn(|index| {
            let index = index as i32;
            LodKey::new(
                key.level - 1,
                key.coord * 2 + glam::IVec3::new(index & 1, (index >> 1) & 1, (index >> 2) & 1),
            )
        });
        let Some(children) = child_keys
            .into_iter()
            .map(|child| self.lod_cache.grid(child))
            .collect::<Option<Vec<_>>>()
        else {
            return;
        };
        let refs: [&crate::lod::LodGrid; 8] = std::array::from_fn(|i| children[i]);
        let grid = downsample_lod(key, refs);
        self.insert_lod_grid(renderer, grid);
        self.rebuild_higher_lod(renderer, key);
    }

    pub(super) fn upload_mesh(
        &mut self,
        world: &World,
        renderer: &mut Renderer,
        gpu_chunks: &mut Vec<GpuChunkMeshes>,
        cp: glam::IVec3,
        mesh: ChunkMeshes,
    ) {
        self.remove_gpu_chunk(renderer, gpu_chunks, cp);
        if !world.is_loaded(cp) {
            return;
        }
        match renderer.upload_chunk_meshes(&mesh, cp * CHUNK_SIZE) {
            Ok(chunk) => gpu_chunks.push(chunk),
            Err(error) => log::error!("upload chunk {cp:?} failed: {error:#}"),
        }
    }

    pub(super) fn remove_gpu_chunk(
        &self,
        renderer: &mut Renderer,
        gpu_chunks: &mut Vec<GpuChunkMeshes>,
        cp: glam::IVec3,
    ) {
        if let Some(index) = gpu_chunks
            .iter()
            .position(|chunk| chunk.origin == cp * CHUNK_SIZE)
        {
            renderer.remove_chunk_meshes(gpu_chunks.swap_remove(index));
        }
    }

    pub(super) fn remove_unloaded_gpu_chunks(
        &mut self,
        world: &World,
        renderer: &mut Renderer,
        gpu_chunks: &mut Vec<GpuChunkMeshes>,
    ) {
        let mut kept = Vec::with_capacity(gpu_chunks.len());
        for chunk in gpu_chunks.drain(..) {
            let cp = chunk_of(chunk.origin);
            if world.is_loaded(cp) {
                kept.push(chunk);
            } else {
                self.mesh_epochs.remove(&cp);
                renderer.remove_chunk_meshes(chunk);
            }
        }
        *gpu_chunks = kept;
        self.mesh_epochs.retain(|cp, _| world.is_loaded(*cp));
    }
}
