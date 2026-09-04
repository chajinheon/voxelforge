use std::collections::{HashMap, HashSet};
use std::thread;
use std::time::Instant;

use glam::IVec3;

use crate::world::light::LightColumn;
use crate::world::world::World;

use super::{Job, MAIN_BUDGET, Streamer};

impl Streamer {
    pub(super) fn pump_lighting(
        &mut self,
        world: &mut World,
        desired: &HashSet<IVec3>,
        center: IVec3,
        started: Instant,
    ) {
        while started.elapsed() < MAIN_BUDGET {
            self.apply_completed_lights(world, desired, started);
            self.collect_dirty(world);
            self.ingest_light_dirty(world);
            let before = self.total_inflight();
            self.issue_lighting(world, desired, center, started);
            let active = self.lighting_busy();
            if !active {
                break;
            }
            if self.total_inflight() == before && self.completed_lights.is_empty() {
                thread::yield_now();
            }
            self.drain_results(world, desired);
        }
        self.apply_completed_lights(world, desired, started);
        self.collect_dirty(world);
    }

    pub(super) fn ingest_light_dirty(&mut self, world: &mut World) {
        let wave_active = !self.light_pending.is_empty()
            || !self.light_inflight.is_empty()
            || !self.completed_lights.is_empty()
            || !self.light_next_pending.is_empty();
        for column in world.take_light_dirty() {
            if wave_active {
                // A completed neighbor may dirty a column that is still in
                // the current pending wave. Move it forward instead of
                // solving the pre-update snapshot and then solving it again.
                self.light_pending.retain(|pending| pending != &column);
                self.light_next_pending.insert(column);
                continue;
            }
            if self.light_pending.contains(&column) {
                continue;
            }
            self.light_pending.push_back(column);
        }
    }

    pub(super) fn apply_completed_lights(
        &mut self,
        world: &mut World,
        desired: &HashSet<IVec3>,
        started: Instant,
    ) {
        while started.elapsed() < MAIN_BUDGET {
            let Some((result, elapsed)) = self.completed_lights.pop_front() else {
                break;
            };
            let c = result.column;
            if !desired.contains(&IVec3::new(c.x, 0, c.z))
                || world.light_epoch(c) != result.epoch
                || !world.column_loaded(c)
            {
                // A result that no longer belongs to the desired/current
                // topology is bookkeeping noise, not a fixed-point solve.
                continue;
            }
            self.light_solves += 1;
            self.light_times.push(elapsed);
            self.record_light_solve(c, result.epoch);
            world.apply_light_column(result);
        }
    }

    pub(super) fn issue_lighting(
        &mut self,
        world: &World,
        desired: &HashSet<IVec3>,
        center: IVec3,
        budget_start: Instant,
    ) {
        let available = self
            .worker_threads()
            .saturating_sub(self.light_inflight.len())
            .min(self.total_limit().saturating_sub(self.total_inflight()));
        if available == 0 {
            return;
        }
        if self.light_pending.is_empty()
            && self.light_inflight.is_empty()
            && self.completed_lights.is_empty()
        {
            self.light_pending = self.light_next_pending.drain().collect();
        }
        let mut pending: Vec<_> = self.light_pending.drain(..).collect();
        pending.retain(|c| desired.contains(&IVec3::new(c.x, 0, c.z)));
        pending.sort_by_key(|c| {
            let urgent = self.light_urgent.contains(c);
            let initialized = world.light_column_initialized(*c);
            (
                !urgent,
                initialized,
                (c.x - center.x).pow(2) + (c.z - center.z).pow(2),
                c.x,
                c.z,
            )
        });
        let mut spawned = 0;
        for column in pending {
            if budget_start.elapsed() >= MAIN_BUDGET {
                self.light_pending.push_back(column);
                continue;
            }
            if spawned >= available {
                self.light_pending.push_back(column);
                continue;
            }
            if !world.column_loaded(column) {
                self.light_pending.push_back(column);
                continue;
            }
            let epoch = world.light_epoch(column);
            let Some(snapshot) = world.light_column_snapshot(column, epoch) else {
                continue;
            };
            self.light_urgent.remove(&column);
            self.light_next_pending.remove(&column);
            self.light_inflight.insert(column);
            self.spawn(Job::Light { snapshot });
            spawned += 1;
        }
    }

    /// Solve all currently dirty loaded columns synchronously to a boundary fixed point.
    /// This is also the bootstrap path used before the first mesh is emitted.
    pub fn solve_lighting_sync(&mut self, world: &mut World) -> Result<(), String> {
        let mut pending: Vec<LightColumn> = world.take_light_dirty();
        let mut counts: HashMap<LightColumn, u8> = HashMap::new();
        while !pending.is_empty() {
            pending.sort_by_key(|c| (c.x, c.z));
            pending.dedup();
            for column in pending.drain(..) {
                if !world.column_loaded(column) {
                    continue;
                }
                let count = counts.entry(column).or_default();
                *count = count.saturating_add(1);
                if *count > 16 {
                    return Err(format!(
                        "lighting fixed point exceeded 16 solves for {column:?}"
                    ));
                }
                let epoch = world.light_epoch(column);
                let Some(snapshot) = world.light_column_snapshot(column, epoch) else {
                    continue;
                };
                let started = Instant::now();
                let result = crate::world::light::solve_column(&snapshot);
                self.light_solves += 1;
                self.light_times.push(started.elapsed());
                self.record_light_solve(column, epoch);
                world.apply_light_column(result);
            }
            pending = world.take_light_dirty();
        }
        Ok(())
    }

    pub(super) fn log_lighting_stats(&self) {
        if self.light_solves == 0 {
            log::info!("lighting: columns 0 solves 0 median 0.00ms p95 0.00ms max_requeues 0");
            return;
        }
        let mut values: Vec<f64> = self
            .light_times
            .iter()
            .map(|d| d.as_secs_f64() * 1000.0)
            .collect();
        values.sort_by(f64::total_cmp);
        let percentile = |p: f64| values[((values.len() - 1) as f64 * p).round() as usize];
        log::info!(
            "lighting: columns {} solves {} median {:.2}ms p95 {:.2}ms max_requeues {}",
            self.light_columns_seen.len(),
            self.light_solves,
            percentile(0.50),
            percentile(0.95),
            self.max_light_requeues
        );
    }

    fn record_light_solve(&mut self, column: LightColumn, epoch: u64) {
        self.light_columns_seen.insert(column);
        let solves = self.light_solve_counts.entry((column, epoch)).or_default();
        *solves += 1;
        self.max_light_requeues = self.max_light_requeues.max(solves.saturating_sub(1));
    }
}

impl Streamer {
    pub(crate) fn mark_light_urgent(&mut self, cp: IVec3) {
        let c = LightColumn { x: cp.x, z: cp.z };
        self.light_urgent.insert(c);
        if !self.light_inflight.contains(&c) && !self.light_pending.contains(&c) {
            self.light_pending.push_front(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::light::solve_column;
    use std::time::Duration;

    #[test]
    fn stale_light_result_is_discarded() {
        let mut world = World::new(7);
        for y in 0..crate::world::coords::WORLD_CHUNKS_Y {
            assert!(world.ensure_loaded(IVec3::new(0, y, 0)));
        }
        let mut streamer = Streamer::new();
        streamer
            .solve_lighting_sync(&mut world)
            .expect("initial light");
        world.take_dirty();
        let column = LightColumn { x: 0, z: 0 };
        let old_epoch = world.light_epoch(column);
        let snapshot = world
            .light_column_snapshot(column, old_epoch)
            .expect("loaded column snapshot");
        let result = solve_column(&snapshot);
        world.mark_light_dirty(column);
        world.take_light_dirty();

        let before_light = world
            .chunk(IVec3::new(0, 0, 0))
            .expect("chunk")
            .get_light(glam::UVec3::new(0, 0, 0));
        let before_version = world.chunk_version(IVec3::new(0, 0, 0));
        let before_modified = world.modified.clone();
        let before_dirty = world.dirty.clone();
        streamer
            .completed_lights
            .push_back((result, Duration::ZERO));
        streamer.apply_completed_lights(
            &mut world,
            &HashSet::from([IVec3::new(0, 0, 0)]),
            Instant::now(),
        );

        assert_eq!(
            world
                .chunk(IVec3::new(0, 0, 0))
                .unwrap()
                .get_light(glam::UVec3::new(0, 0, 0)),
            before_light
        );
        assert_eq!(world.chunk_version(IVec3::new(0, 0, 0)), before_version);
        assert_eq!(world.modified, before_modified);
        assert_eq!(world.dirty, before_dirty);
        assert_eq!(streamer.max_light_requeues, 0);
    }

    #[test]
    fn outside_light_result_is_not_counted_as_requeue() {
        let mut world = World::new(13);
        for y in 0..crate::world::coords::WORLD_CHUNKS_Y {
            assert!(world.ensure_loaded(IVec3::new(0, y, 0)));
        }
        let mut streamer = Streamer::new();
        streamer
            .solve_lighting_sync(&mut world)
            .expect("initial light");
        let solves_before = streamer.light_solves;
        let column = LightColumn { x: 0, z: 0 };
        let epoch = world.light_epoch(column);
        let snapshot = world
            .light_column_snapshot(column, epoch)
            .expect("loaded column snapshot");
        streamer
            .completed_lights
            .push_back((solve_column(&snapshot), Duration::ZERO));
        streamer.apply_completed_lights(&mut world, &HashSet::new(), Instant::now());
        assert_eq!(streamer.max_light_requeues, 0);
        assert_eq!(streamer.light_solves, solves_before);
    }

    #[test]
    fn boundary_dirty_moves_pending_column_to_next_wave() {
        let mut world = World::new(17);
        let column = LightColumn { x: 2, z: -3 };
        streamer_pending_column(&mut world, column);
        let mut streamer = Streamer::new();
        streamer.light_pending.push_back(column);
        streamer.ingest_light_dirty(&mut world);
        assert!(!streamer.light_pending.contains(&column));
        assert!(streamer.light_next_pending.contains(&column));
    }

    #[test]
    fn dirty_column_coalesces_with_existing_next_wave() {
        let mut world = World::new(19);
        let existing = LightColumn { x: 1, z: 1 };
        let dirty = LightColumn { x: 2, z: 1 };
        world.mark_light_boundary_dirty(dirty);
        let mut streamer = Streamer::new();
        streamer.light_next_pending.insert(existing);
        streamer.ingest_light_dirty(&mut world);
        assert!(streamer.light_pending.is_empty());
        assert_eq!(streamer.light_next_pending.len(), 2);
        assert!(streamer.light_next_pending.contains(&dirty));
    }

    fn streamer_pending_column(world: &mut World, column: LightColumn) {
        world.mark_light_boundary_dirty(column);
    }

    #[test]
    fn bootstrap_three_by_three_columns_reach_light_fixed_point() {
        let mut world = World::new(11);
        for z in -1..=1 {
            for x in -1..=1 {
                for y in 0..crate::world::coords::WORLD_CHUNKS_Y {
                    world.ensure_loaded(IVec3::new(x, y, z));
                }
            }
        }
        let mut streamer = Streamer::new();
        streamer
            .solve_lighting_sync(&mut world)
            .expect("bootstrap light");
        for z in -1..=1 {
            for x in -1..=1 {
                assert!(world.light_column_initialized(LightColumn { x, z }));
            }
        }
    }

    #[test]
    fn light_requeue_counter_resets_for_new_epoch() {
        let mut streamer = Streamer::new();
        let column = LightColumn { x: 3, z: -2 };
        streamer.record_light_solve(column, 10);
        streamer.record_light_solve(column, 10);
        assert_eq!(streamer.max_light_requeues, 1);
        streamer.record_light_solve(column, 11);
        streamer.record_light_solve(column, 11);
        assert_eq!(streamer.max_light_requeues, 1);
    }
}
