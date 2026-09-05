//! LOD ring selection, source precedence, invalidation, and bounded caches.

use std::collections::{HashMap, HashSet};

use glam::{IVec3, Vec3};

use super::grid::{LodGrid, LodKey};

pub const DEFAULT_FULL_DETAIL_RADIUS: i32 = 10;
pub const CPU_LOD_MEMORY_CAP: usize = 128 * 1024 * 1024;
pub const GPU_LOD_MEMORY_CAP: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LodRange {
    pub level: u8,
    pub start: i32,
    pub end: i32,
}

pub fn lod_ring_ranges(full_detail_radius: i32) -> [LodRange; 4] {
    let radius = full_detail_radius.max(1);
    [
        LodRange {
            level: 0,
            start: 0,
            end: 32 * radius,
        },
        LodRange {
            level: 1,
            start: 32 * (radius - 1),
            end: 64 * radius,
        },
        LodRange {
            level: 2,
            start: 64 * radius - 32,
            end: 96 * radius,
        },
        LodRange {
            level: 3,
            start: 96 * radius - 32,
            end: 128 * radius,
        },
    ]
}

pub fn lod_ring_ranges_default() -> [LodRange; 4] {
    lod_ring_ranges(DEFAULT_FULL_DETAIL_RADIUS)
}

pub fn ring_ranges(full_detail_radius: i32) -> [LodRange; 4] {
    lod_ring_ranges(full_detail_radius)
}

pub fn lod_for_distance(distance: i32, full_detail_radius: i32) -> u8 {
    let distance = distance.max(0);
    lod_ring_ranges(full_detail_radius)
        .into_iter()
        .find(|range| distance <= range.end)
        .map_or(3, |range| range.level)
}

pub fn select_lod(distance: i32) -> u8 {
    lod_for_distance(distance, DEFAULT_FULL_DETAIL_RADIUS)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LodSource {
    World,
    Save,
    WorldGen,
}

pub fn choose_source<T>(
    world: Option<T>,
    save: Option<T>,
    world_gen: Option<T>,
) -> Option<(LodSource, T)> {
    world
        .map(|value| (LodSource::World, value))
        .or_else(|| save.map(|value| (LodSource::Save, value)))
        .or_else(|| world_gen.map(|value| (LodSource::WorldGen, value)))
}

pub fn resolve_source<T>(
    world: Option<T>,
    save: Option<T>,
    world_gen: Option<T>,
) -> Option<(LodSource, T)> {
    choose_source(world, save, world_gen)
}

pub fn lod_ancestor_keys(base_chunk: IVec3) -> [LodKey; 3] {
    let level1 = base_chunk.div_euclid(IVec3::splat(2));
    let level2 = base_chunk.div_euclid(IVec3::splat(4));
    let level3 = base_chunk.div_euclid(IVec3::splat(8));
    [
        LodKey::new(1, level1),
        LodKey::new(2, level2),
        LodKey::new(3, level3),
    ]
}

pub fn ancestor_keys(base_chunk: IVec3) -> [LodKey; 3] {
    lod_ancestor_keys(base_chunk)
}

#[derive(Clone, Copy, Debug)]
struct CacheMeta {
    distance: i32,
    protected: bool,
}

/// Bounded CPU grids plus bounded GPU mesh allocation metadata.
pub struct LodCache {
    grids: HashMap<LodKey, (LodGrid, CacheMeta)>,
    gpu_meshes: HashMap<LodKey, CacheMeta>,
    gpu_allocations: HashMap<LodKey, usize>,
    cpu_bytes: usize,
    gpu_bytes: usize,
}

impl Default for LodCache {
    fn default() -> Self {
        Self::new()
    }
}

impl LodCache {
    pub fn new() -> Self {
        Self {
            grids: HashMap::new(),
            gpu_meshes: HashMap::new(),
            gpu_allocations: HashMap::new(),
            cpu_bytes: 0,
            gpu_bytes: 0,
        }
    }

    pub fn grid(&self, key: LodKey) -> Option<&LodGrid> {
        self.grids.get(&key).map(|(grid, _)| grid)
    }

    pub fn keys(&self, level: u8) -> Vec<LodKey> {
        let mut keys: Vec<_> = self
            .grids
            .keys()
            .copied()
            .filter(|key| key.level == level)
            .collect();
        keys.sort_by_key(|key| (key.coord.x, key.coord.y, key.coord.z));
        keys
    }

    pub fn len(&self, level: u8) -> usize {
        self.grids.keys().filter(|key| key.level == level).count()
    }

    pub fn cpu_bytes(&self) -> usize {
        self.cpu_bytes
    }

    pub fn gpu_bytes(&self) -> usize {
        self.gpu_bytes
    }

    pub fn gpu_mesh_count(&self, level: u8) -> usize {
        self.gpu_meshes
            .keys()
            .filter(|key| key.level == level)
            .count()
    }

    pub fn has_gpu_mesh(&self, key: LodKey) -> bool {
        self.gpu_meshes.contains_key(&key)
    }

    pub fn insert_grid(&mut self, grid: LodGrid, distance: i32, protected: bool) -> bool {
        let key = grid.key;
        if !matches!(key.level, 1..=3) {
            return false;
        }
        let bytes = grid.byte_size();
        if let Some((old, _)) = self.grids.remove(&key) {
            self.cpu_bytes = self.cpu_bytes.saturating_sub(old.byte_size());
        }
        self.evict_cpu_until(key.level, bytes);
        if self.cpu_bytes.saturating_add(bytes) > CPU_LOD_MEMORY_CAP {
            return false;
        }
        self.cpu_bytes += bytes;
        self.grids.insert(
            key,
            (
                grid,
                CacheMeta {
                    distance,
                    protected,
                },
            ),
        );
        true
    }

    pub fn insert_gpu_mesh(
        &mut self,
        key: LodKey,
        bytes: usize,
        distance: i32,
        protected: bool,
    ) -> bool {
        if !matches!(key.level, 1..=3) || bytes > GPU_LOD_MEMORY_CAP {
            return false;
        }
        if self.gpu_meshes.remove(&key).is_some() {
            let old_bytes = self.gpu_allocations.remove(&key).unwrap_or(0);
            self.gpu_bytes = self.gpu_bytes.saturating_sub(old_bytes);
        }
        self.gpu_allocations.insert(key, bytes);
        self.gpu_bytes = self.gpu_bytes.saturating_add(bytes);
        self.gpu_meshes.insert(
            key,
            CacheMeta {
                distance,
                protected,
            },
        );
        self.evict_gpu_until();
        if self.gpu_bytes > GPU_LOD_MEMORY_CAP {
            self.invalidate(key);
            return false;
        }
        true
    }

    pub fn set_protected(&mut self, keys: &HashSet<LodKey>) {
        for (key, (_, meta)) in &mut self.grids {
            meta.protected = keys.contains(key);
        }
        for (key, meta) in &mut self.gpu_meshes {
            meta.protected = keys.contains(key);
        }
    }

    pub fn set_active_ring_protection(&mut self, camera_world: Vec3) {
        let ranges = lod_ring_ranges_default();
        let is_active = |key: &LodKey| {
            let origin = key.world_origin();
            let span = super::grid::GRID_SIZE as f32 * key.cell_size() as f32;
            let center = Vec3::new(
                origin.x as f32 + span * 0.5,
                origin.y as f32 + span * 0.5,
                origin.z as f32 + span * 0.5,
            );
            let distance =
                ((center.x - camera_world.x).powi(2) + (center.z - camera_world.z).powi(2)).sqrt();
            ranges.iter().any(|range| {
                range.level == key.level
                    && distance >= range.start as f32
                    && distance <= range.end as f32
            })
        };
        let keys: HashSet<_> = self
            .grids
            .keys()
            .chain(self.gpu_meshes.keys())
            .copied()
            .filter(is_active)
            .collect();
        self.set_protected(&keys);
    }

    pub fn invalidate(&mut self, key: LodKey) {
        if let Some((grid, _)) = self.grids.remove(&key) {
            self.cpu_bytes = self.cpu_bytes.saturating_sub(grid.byte_size());
        }
        if self.gpu_meshes.remove(&key).is_some() {
            let bytes = self.gpu_allocations.remove(&key).unwrap_or(0);
            self.gpu_bytes = self.gpu_bytes.saturating_sub(bytes);
        }
    }

    pub fn invalidate_base_edit(&mut self, base_chunk: IVec3) {
        for key in lod_ancestor_keys(base_chunk) {
            self.invalidate(key);
        }
    }

    fn evict_cpu_until(&mut self, level: u8, incoming: usize) {
        while self.len(level) >= level_cap(level)
            || self.cpu_bytes.saturating_add(incoming) > CPU_LOD_MEMORY_CAP
        {
            let candidate = self
                .grids
                .iter()
                .filter(|(key, (_, meta))| key.level == level && !meta.protected)
                .max_by_key(|(key, (_, meta))| {
                    (
                        meta.distance,
                        key.level,
                        key.coord.x,
                        key.coord.y,
                        key.coord.z,
                    )
                })
                .map(|(key, _)| *key);
            let Some(key) = candidate else { break };
            self.invalidate(key);
        }
    }

    fn evict_gpu_until(&mut self) {
        while self.gpu_bytes > GPU_LOD_MEMORY_CAP {
            let candidate = self
                .gpu_meshes
                .iter()
                .filter(|(_, meta)| !meta.protected)
                .max_by_key(|(key, meta)| {
                    (
                        meta.distance,
                        key.level,
                        key.coord.x,
                        key.coord.y,
                        key.coord.z,
                    )
                })
                .map(|(key, _)| *key);
            let Some(key) = candidate else { break };
            self.invalidate(key);
        }
    }
}

fn level_cap(level: u8) -> usize {
    match level {
        // R10 annulus residency includes the mandated vertical bands
        // (L1: four, L2: two, L3: one). Keep the total below the 128MiB
        // fixed-grid budget while leaving room for overlap cells.
        1 => 768,
        2 => 256,
        3 => 96,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lod::{GRID_SIZE, grid::LodGrid};

    #[test]
    fn lod_parent_invalidation_reaches_all_levels() {
        let base = IVec3::new(-3, 6, 9);
        let mut cache = LodCache::new();
        for key in lod_ancestor_keys(base) {
            assert!(cache.insert_grid(LodGrid::empty(key), 1, false));
            assert!(cache.insert_gpu_mesh(key, 64, 1, false));
        }
        cache.invalidate_base_edit(base);
        for key in lod_ancestor_keys(base) {
            assert!(cache.grid(key).is_none());
        }
        assert_eq!(cache.cpu_bytes(), 0);
        assert_eq!(cache.gpu_bytes(), 0);
    }

    #[test]
    fn lod_ring_selection_has_exact_overlap() {
        let ranges = lod_ring_ranges_default();
        assert_eq!(
            ranges[0],
            LodRange {
                level: 0,
                start: 0,
                end: 320
            }
        );
        assert_eq!(
            ranges[1],
            LodRange {
                level: 1,
                start: 288,
                end: 640
            }
        );
        assert_eq!(
            ranges[2],
            LodRange {
                level: 2,
                start: 608,
                end: 960
            }
        );
        assert_eq!(
            ranges[3],
            LodRange {
                level: 3,
                start: 928,
                end: 1280
            }
        );
        assert_eq!(select_lod(320), 0);
        assert_eq!(select_lod(321), 1);
    }

    #[test]
    fn lod_cache_respects_memory_caps() {
        let mut cache = LodCache::new();
        for i in 0..1500 {
            let key = LodKey::new(1, IVec3::new(i, 0, 0));
            assert!(cache.insert_grid(LodGrid::empty(key), i, false));
        }
        assert!(cache.len(1) <= level_cap(1));
        assert!(cache.cpu_bytes() <= CPU_LOD_MEMORY_CAP);
        for i in 0..1024 {
            let key = LodKey::new(2, IVec3::new(i, 0, 0));
            assert!(cache.insert_gpu_mesh(key, 256 * 1024, i, false));
        }
        assert!(cache.gpu_bytes() <= GPU_LOD_MEMORY_CAP);
    }

    #[test]
    fn active_ring_protection_does_not_pin_every_cached_entry() {
        let mut cache = LodCache::new();
        let near = LodKey::new(1, IVec3::new(5, 0, 0));
        let far = LodKey::new(1, IVec3::new(40, 0, 0));
        let level3 = LodKey::new(3, IVec3::new(4, 0, 0));
        for key in [near, far, level3] {
            assert!(cache.insert_grid(LodGrid::empty(key), 0, false));
            assert!(cache.insert_gpu_mesh(key, 64, 0, false));
        }

        cache.set_active_ring_protection(Vec3::new(0.0, 0.0, 0.0));

        assert!(
            cache
                .grids
                .get(&near)
                .is_some_and(|(_, meta)| meta.protected)
        );
        assert!(
            cache
                .grids
                .get(&level3)
                .is_some_and(|(_, meta)| meta.protected)
        );
        assert!(
            cache
                .grids
                .get(&far)
                .is_some_and(|(_, meta)| !meta.protected)
        );
        assert!(
            cache
                .gpu_meshes
                .get(&near)
                .is_some_and(|meta| meta.protected)
        );
        assert!(
            cache
                .gpu_meshes
                .get(&far)
                .is_some_and(|meta| !meta.protected)
        );
    }

    #[test]
    fn active_ring_caps_fit_the_fixed_cpu_budget() {
        let grid_bytes = LodGrid::empty(LodKey::new(1, IVec3::ZERO)).byte_size();
        let reserved = (level_cap(1) + level_cap(2) + level_cap(3)) * grid_bytes;
        assert!(reserved <= CPU_LOD_MEMORY_CAP);
        let ranges = lod_ring_ranges_default();
        for level in 1..=3_usize {
            let span = (GRID_SIZE as i32 * (1_i32 << level)) as f32;
            let extent = (ranges[level].end as f32 / span).ceil() as i32 + 1;
            let y_count = [0, 3, 2, 1][level];
            let mut count = 0usize;
            for y in 0..y_count {
                let _ = y;
                for z in -extent..=extent {
                    for x in -extent..=extent {
                        let distance = ((x as f32 + 0.5) * span).hypot((z as f32 + 0.5) * span);
                        if distance >= ranges[level].start as f32
                            && distance <= ranges[level].end as f32
                        {
                            count += 1;
                        }
                    }
                }
            }
            assert!(
                count <= level_cap(level as u8),
                "L{level} active grids: {count}"
            );
        }
    }
}
