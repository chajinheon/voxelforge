use glam::{IVec3, UVec3, Vec3};

pub const CLIPMAP_RESOLUTION: u32 = 128;
pub const CLIPMAP_LEVELS: usize = 4;
pub const UPLOAD_BUDGET_BYTES: usize = 4 * 1024 * 1024;
pub const TEXTURE_MEMORY_BYTES: usize = CLIPMAP_LEVELS * 2 * 128 * 128 * 128 * 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlabRegion {
    pub axis: u8,
    pub logical_start: IVec3,
    pub extent: UVec3,
    pub physical_start: UVec3,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoxelUpload {
    pub level: u8,
    pub logical_origin: IVec3,
    pub extent: UVec3,
    pub material: Vec<u8>,
    pub light: Vec<u8>,
    pub revision: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipmapUpdate {
    pub level: usize,
    pub revision: u32,
    pub full_rebuild: bool,
    pub exposed_cells: usize,
    pub regions: Vec<SlabRegion>,
    pub history_reset: bool,
}

#[derive(Clone, Debug)]
pub struct ClipmapLevel {
    pub level: usize,
    pub cell_size: i32,
    pub origin: IVec3,
    pub ring_offset: UVec3,
    pub revision: u32,
    pub ready: bool,
}

impl ClipmapLevel {
    pub fn new(level: usize) -> Self {
        assert!(level < CLIPMAP_LEVELS);
        Self {
            level,
            cell_size: 1_i32 << level,
            origin: IVec3::ZERO,
            ring_offset: UVec3::ZERO,
            revision: 0,
            ready: false,
        }
    }

    pub fn quantum(&self) -> i32 {
        match self.level {
            0 | 1 => 4,
            2 => 2,
            _ => 1,
        }
    }

    pub fn origin_for_camera(&self, camera_world: Vec3) -> IVec3 {
        let camera_cell = (camera_world / self.cell_size as f32).floor().as_ivec3();
        let unsnapped = camera_cell - IVec3::splat(64);
        let q = IVec3::splat(self.quantum());
        (unsnapped.div_euclid(q)) * q
    }

    pub fn logical_to_physical(&self, logical: IVec3) -> UVec3 {
        let local = logical - self.origin + self.ring_offset.as_ivec3();
        UVec3::new(
            local.x.rem_euclid(128) as u32,
            local.y.rem_euclid(128) as u32,
            local.z.rem_euclid(128) as u32,
        )
    }

    pub fn mark_ready(&mut self, revision: u32) {
        self.revision = revision;
        self.ready = true;
    }

    pub fn move_to(&mut self, new_origin: IVec3) -> ClipmapUpdate {
        let delta = new_origin - self.origin;
        // A newly allocated level has no valid texels, even when its first
        // camera origin overlaps the zero origin.  Treat it as a full upload.
        let full_rebuild = !self.ready || delta.to_array().iter().any(|v| v.abs() >= 128);
        let exposed_cells = if full_rebuild {
            128 * 128 * 128
        } else {
            changed_cell_count(delta)
        };
        let new_ring_offset = UVec3::new(
            (self.ring_offset.x as i32 + delta.x).rem_euclid(128) as u32,
            (self.ring_offset.y as i32 + delta.y).rem_euclid(128) as u32,
            (self.ring_offset.z as i32 + delta.z).rem_euclid(128) as u32,
        );
        let regions = if full_rebuild {
            vec![SlabRegion {
                axis: 3,
                logical_start: new_origin,
                extent: UVec3::splat(128),
                physical_start: UVec3::ZERO,
            }]
        } else {
            slab_regions(new_origin, new_ring_offset, delta)
        };
        self.ring_offset = new_ring_offset;
        self.origin = new_origin;
        self.ready = false;
        self.revision = self.revision.wrapping_add(1);
        ClipmapUpdate {
            level: self.level,
            revision: self.revision,
            full_rebuild,
            exposed_cells,
            regions,
            history_reset: exposed_cells * 10 >= 128 * 128 * 128,
        }
    }
}

fn changed_cell_count(delta: IVec3) -> usize {
    let d = delta.to_array().map(|v| v.unsigned_abs().min(128) as usize);
    let overlap = (128 - d[0]) * (128 - d[1]) * (128 - d[2]);
    128 * 128 * 128 - overlap
}

fn slab_regions(new_origin: IVec3, ring_offset: UVec3, delta: IVec3) -> Vec<SlabRegion> {
    let mut regions = Vec::with_capacity(3);
    for axis in 0..3 {
        let d = delta[axis];
        if d == 0 {
            continue;
        }
        let count = d.unsigned_abs().min(128);
        let mut logical_start = new_origin;
        logical_start[axis] = if d > 0 {
            new_origin[axis] + 128 - count as i32
        } else {
            new_origin[axis]
        };
        let mut extent = UVec3::splat(128);
        extent[axis] = count;
        regions.push(SlabRegion {
            axis: axis as u8,
            logical_start,
            extent,
            physical_start: {
                let local = logical_start - new_origin + ring_offset.as_ivec3();
                UVec3::new(
                    local.x.rem_euclid(128) as u32,
                    local.y.rem_euclid(128) as u32,
                    local.z.rem_euclid(128) as u32,
                )
            },
        });
    }
    regions
}

#[derive(Clone, Debug)]
pub struct Clipmap {
    pub levels: [ClipmapLevel; CLIPMAP_LEVELS],
    pub upload_budget: usize,
}

impl Clipmap {
    pub fn new() -> Self {
        Self {
            levels: std::array::from_fn(ClipmapLevel::new),
            upload_budget: UPLOAD_BUDGET_BYTES,
        }
    }

    pub fn update_camera(&mut self, camera_world: Vec3) -> Vec<ClipmapUpdate> {
        let mut updates = Vec::new();
        for level in &mut self.levels {
            let origin = level.origin_for_camera(camera_world);
            // Revision zero means the texture has never been scheduled. Once
            // an upload is in flight, do not restart the same full rebuild on
            // every frame merely because it is not ready yet.
            if origin != level.origin || (!level.ready && level.revision == 0) {
                updates.push(level.move_to(origin));
            }
        }
        updates
    }

    /// Invalidate every level after a world edit. The next camera update then
    /// schedules a full upload for each level; the streamer applies its normal
    /// per-frame upload budget while draining those uploads.
    pub fn invalidate_all(&mut self) {
        for level in &mut self.levels {
            level.ready = false;
            level.revision = 0;
        }
    }

    pub fn level_for_distance(distance: f32) -> usize {
        if distance < 8.0 {
            0
        } else if distance < 16.0 {
            1
        } else if distance < 32.0 {
            2
        } else {
            3
        }
    }

    pub fn level_selection(distance: f32) -> usize {
        Self::level_for_distance(distance)
    }

    pub fn texture_memory_bytes(&self) -> usize {
        TEXTURE_MEMORY_BYTES
    }
}

impl Default for Clipmap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipmap_toroidal_wrap_preserves_logical_voxels() {
        let mut level = ClipmapLevel::new(0);
        level.origin = IVec3::new(-64, -64, -64);
        level.ring_offset = UVec3::new(7, 19, 63);
        let logical = IVec3::new(12, -2, 91);
        let before = level.logical_to_physical(logical);
        level.move_to(level.origin + IVec3::new(5, -3, 2));
        let after = level.logical_to_physical(logical);
        assert_eq!(before, after);
    }

    #[test]
    fn clipmap_move_updates_only_exposed_slabs() {
        let mut level = ClipmapLevel::new(0);
        level.ready = true;
        let update = level.move_to(IVec3::X);
        assert!(!update.full_rebuild);
        assert_eq!(update.exposed_cells, 128 * 128);
        assert_eq!(update.regions.len(), 1);
    }

    #[test]
    fn clipmap_teleport_requests_full_rebuild() {
        let mut level = ClipmapLevel::new(0);
        let update = level.move_to(IVec3::new(128, 0, 0));
        assert!(update.full_rebuild);
        assert_eq!(update.exposed_cells, 128 * 128 * 128);
        assert!(update.history_reset);
    }

    #[test]
    fn first_update_is_full_rebuild_even_at_zero_origin() {
        let mut clipmap = Clipmap::new();
        let updates = clipmap.update_camera(Vec3::new(64.0, 64.0, 64.0));
        assert_eq!(updates.len(), CLIPMAP_LEVELS);
        assert!(updates.iter().all(|update| update.full_rebuild));
        assert!(updates.iter().all(|update| update.revision == 1));
    }

    #[test]
    fn in_flight_origin_does_not_restart_full_rebuild() {
        let mut clipmap = Clipmap::new();
        let camera = Vec3::new(64.0, 64.0, 64.0);
        assert_eq!(clipmap.update_camera(camera).len(), CLIPMAP_LEVELS);
        assert!(clipmap.update_camera(camera).is_empty());
        assert!(clipmap.levels.iter().all(|level| !level.ready));
    }

    #[test]
    fn edit_invalidates_all_levels_for_full_rebuild() {
        let mut clipmap = Clipmap::new();
        let camera = Vec3::new(64.0, 64.0, 64.0);
        let initial = clipmap.update_camera(camera);
        assert_eq!(initial.len(), CLIPMAP_LEVELS);
        for update in initial {
            clipmap.levels[update.level].mark_ready(update.revision);
        }

        clipmap.invalidate_all();
        let updates = clipmap.update_camera(camera);
        assert_eq!(updates.len(), CLIPMAP_LEVELS);
        assert!(updates.iter().all(|update| update.full_rebuild));
        assert!(
            updates
                .iter()
                .all(|update| update.exposed_cells == 128 * 128 * 128)
        );
        assert!(updates.iter().all(|update| update.revision == 1));
    }

    #[test]
    fn clipmap_level_selection_matches_distance_bands() {
        assert_eq!(Clipmap::level_selection(0.0), 0);
        assert_eq!(Clipmap::level_selection(7.99), 0);
        assert_eq!(Clipmap::level_selection(8.0), 1);
        assert_eq!(Clipmap::level_selection(15.99), 1);
        assert_eq!(Clipmap::level_selection(16.0), 2);
        assert_eq!(Clipmap::level_selection(31.99), 2);
        assert_eq!(Clipmap::level_selection(32.0), 3);
    }
}
