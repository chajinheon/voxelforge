//! Safe, renderer-owned allocation counters.
//!
//! This deliberately does not install a global allocator.  The counters cover
//! the collections and GPU graph construction owned by [`Renderer`], which is
//! the useful boundary for the steady-frame contract and keeps the game free
//! without allocator-level instrumentation.

use std::cell::Cell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

/// The allocation events observed during one rendered frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AllocationDelta {
    pub cpu_capacity_growths: u32,
    pub gpu_resource_creations: u32,
}

impl AllocationDelta {
    pub const fn total(self) -> u32 {
        self.cpu_capacity_growths + self.gpu_resource_creations
    }
}

/// A cheap, cloneable counter source shared by renderer scratch collections.
#[derive(Debug)]
pub struct AllocationStats {
    enabled: bool,
    cpu_capacity_growths: Cell<u32>,
    gpu_resource_creations: Cell<u32>,
    last_frame: Cell<AllocationDelta>,
}

impl AllocationStats {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            cpu_capacity_growths: Cell::new(0),
            gpu_resource_creations: Cell::new(0),
            last_frame: Cell::new(AllocationDelta::default()),
        }
    }

    pub fn from_environment() -> Rc<Self> {
        Rc::new(Self::new(
            std::env::var("VF_ALLOC_STATS").is_ok_and(|value| value == "1"),
        ))
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Start a new render frame. Counters are no-ops unless explicitly enabled.
    pub fn begin_frame(&self) {
        if self.enabled {
            self.cpu_capacity_growths.set(0);
            self.gpu_resource_creations.set(0);
        }
    }

    pub fn finish_frame(&self) -> AllocationDelta {
        let delta = if self.enabled {
            AllocationDelta {
                cpu_capacity_growths: self.cpu_capacity_growths.get(),
                gpu_resource_creations: self.gpu_resource_creations.get(),
            }
        } else {
            AllocationDelta::default()
        };
        self.last_frame.set(delta);
        delta
    }

    pub fn last_frame(&self) -> AllocationDelta {
        self.last_frame.get()
    }

    pub fn note_cpu_capacity_growth(&self) {
        if self.enabled {
            self.cpu_capacity_growths
                .set(self.cpu_capacity_growths.get().saturating_add(1));
        }
    }

    pub fn note_gpu_resource_creation(&self) {
        if self.enabled {
            self.gpu_resource_creations
                .set(self.gpu_resource_creations.get().saturating_add(1));
        }
    }

    pub fn note_gpu_resource_creations(&self, count: u32) {
        if self.enabled {
            self.gpu_resource_creations
                .set(self.gpu_resource_creations.get().saturating_add(count));
        }
    }
}

/// A `Vec` whose capacity growths are counted without changing its indexing or
/// slice ergonomics.  Callers should use [`Self::push`] and [`Self::reserve`]
/// for mutations so every growth is observable.
pub struct TrackedScratch<T> {
    values: Vec<T>,
    stats: Rc<AllocationStats>,
}

impl<T> Default for TrackedScratch<T> {
    fn default() -> Self {
        Self::new(Rc::new(AllocationStats::new(false)))
    }
}

impl<T> TrackedScratch<T> {
    pub fn new(stats: Rc<AllocationStats>) -> Self {
        Self {
            values: Vec::new(),
            stats,
        }
    }

    pub fn with_capacity(stats: Rc<AllocationStats>, capacity: usize) -> Self {
        Self {
            values: Vec::with_capacity(capacity),
            stats,
        }
    }

    pub fn clear(&mut self) {
        self.values.clear();
    }

    pub fn push(&mut self, value: T) {
        let capacity = self.values.capacity();
        self.values.push(value);
        if self.values.capacity() > capacity {
            self.stats.note_cpu_capacity_growth();
        }
    }

    pub fn reserve(&mut self, additional: usize) {
        let capacity = self.values.capacity();
        self.values.reserve(additional);
        if self.values.capacity() > capacity {
            self.stats.note_cpu_capacity_growth();
        }
    }

    pub fn capacity(&self) -> usize {
        self.values.capacity()
    }

    pub fn stats(&self) -> &Rc<AllocationStats> {
        &self.stats
    }
}

impl<T> Deref for TrackedScratch<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl<T> DerefMut for TrackedScratch<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}

impl<'a, T> IntoIterator for &'a TrackedScratch<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut TrackedScratch<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracked_scratch_reports_growth_once_per_capacity_change() {
        let stats = Rc::new(AllocationStats::new(true));
        let mut scratch = TrackedScratch::with_capacity(stats.clone(), 1);
        stats.begin_frame();
        scratch.push(1_u8);
        scratch.push(2_u8);
        let delta = stats.finish_frame();
        assert_eq!(delta.cpu_capacity_growths, 1);
        assert_eq!(delta.gpu_resource_creations, 0);
    }

    #[test]
    fn disabled_stats_are_zero_and_allocation_free_contract_is_explicit() {
        let stats = AllocationStats::new(false);
        stats.begin_frame();
        stats.note_cpu_capacity_growth();
        stats.note_gpu_resource_creation();
        assert_eq!(stats.finish_frame(), AllocationDelta::default());
    }
}
