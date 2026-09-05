//! Reusable, allocation-stable visible chunk index lists.

use super::allocation_stats::{AllocationStats, TrackedScratch};
use super::chunk_pipeline::GpuChunkMeshes;
use super::frustum::Frustum;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransparentKind {
    Glass,
    Water,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransparentDraw {
    pub chunk_index: usize,
    pub kind: TransparentKind,
    pub distance_squared: f32,
}

#[derive(Default)]
pub struct DrawLists {
    pub opaque: TrackedScratch<usize>,
    pub transparent: TrackedScratch<TransparentDraw>,
}

impl DrawLists {
    pub fn with_stats(stats: Rc<AllocationStats>) -> Self {
        Self {
            opaque: TrackedScratch::with_capacity(stats.clone(), 32),
            transparent: TrackedScratch::with_capacity(stats, 16),
        }
    }

    pub fn update(
        &mut self,
        chunks: &[GpuChunkMeshes],
        frustum: &Frustum,
        camera: glam::Vec3,
    ) -> usize {
        self.opaque.clear();
        self.transparent.clear();
        let mut visible = 0;
        for (index, chunk) in chunks.iter().enumerate() {
            let min = chunk.origin.as_vec3();
            let max = min + glam::Vec3::splat(crate::world::coords::CHUNK_SIZE as f32);
            if !frustum.intersects_aabb(min, max) {
                continue;
            }
            if chunk.opaque.is_some() {
                self.opaque.push(index);
            }
            let center = chunk.origin.as_vec3()
                + glam::Vec3::splat(crate::world::coords::CHUNK_SIZE as f32 * 0.5);
            let distance_squared = center.distance_squared(camera);
            if chunk.translucent.is_some() {
                self.transparent.push(TransparentDraw {
                    chunk_index: index,
                    kind: TransparentKind::Glass,
                    distance_squared,
                });
            }
            if chunk.water.is_some() {
                self.transparent.push(TransparentDraw {
                    chunk_index: index,
                    kind: TransparentKind::Water,
                    distance_squared,
                });
            }
            visible += usize::from(
                chunk.opaque.is_some() || chunk.translucent.is_some() || chunk.water.is_some(),
            );
        }
        self.transparent.sort_by(|a, b| {
            b.distance_squared
                .total_cmp(&a.distance_squared)
                .then_with(|| a.chunk_index.cmp(&b.chunk_index))
                .then_with(|| (a.kind as u8).cmp(&(b.kind as u8)))
        });
        visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_lists_reuse_capacity_after_clear() {
        let mut lists = DrawLists::default();
        lists.opaque.reserve(32);
        let capacity = lists.opaque.capacity();
        lists.opaque.clear();
        assert_eq!(lists.opaque.capacity(), capacity);
    }

    #[test]
    fn glass_and_water_share_one_back_to_front_list() {
        let mut list = [
            TransparentDraw {
                chunk_index: 0,
                kind: TransparentKind::Glass,
                distance_squared: 4.0,
            },
            TransparentDraw {
                chunk_index: 1,
                kind: TransparentKind::Water,
                distance_squared: 9.0,
            },
        ];
        list.sort_by(|a, b| b.distance_squared.total_cmp(&a.distance_squared));
        assert_eq!(list[0].kind, TransparentKind::Water);
        assert_eq!(list[1].kind, TransparentKind::Glass);
    }
}
