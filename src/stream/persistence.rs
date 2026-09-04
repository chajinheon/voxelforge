use std::collections::HashSet;
use std::time::Instant;

use glam::IVec3;

use super::{Job, Streamer};
use crate::world::chunk::Chunk;
use crate::world::save::SaveDir;
use crate::world::world::World;

impl Streamer {
    pub fn new_with_save(save_dir: SaveDir) -> Self {
        let mut streamer = Self::new();
        streamer.save_dir = Some(save_dir);
        streamer
    }

    pub fn save_dir(&self) -> Option<&SaveDir> {
        self.save_dir.as_ref()
    }

    pub fn save_now(&mut self, world: &mut World) {
        let Some(save) = self.save_dir.as_ref() else {
            return;
        };
        match world.save_modified(save) {
            Ok(written) if written > 0 => log::info!("save: wrote {written} modified chunks"),
            Ok(_) => {}
            Err(error) => log::error!("save modified chunks failed: {error:#}"),
        }
        self.last_periodic_save = Instant::now();
    }

    pub(super) fn handle_load_result(
        &mut self,
        cp: IVec3,
        result: Result<Option<Chunk>, String>,
        world: &mut World,
        desired: &HashSet<IVec3>,
    ) {
        self.load_inflight.remove(&cp);
        if !desired.contains(&cp) || world.is_loaded(cp) {
            return;
        }
        match result {
            Ok(Some(chunk)) => world.insert_loaded(cp, chunk),
            Ok(None) => self.schedule_generation(world, cp),
            Err(error) => {
                log::warn!("load chunk {cp:?} failed ({error}); regenerating");
                self.schedule_generation(world, cp);
            }
        }
    }

    pub(super) fn saved_chunk(&self, cp: IVec3) -> Option<SaveDir> {
        self.save_dir
            .as_ref()
            .filter(|save| save.chunk_path(cp).is_file())
            .cloned()
    }

    pub(super) fn load_or_generate(&self, world: &mut World, cp: IVec3) {
        let generator = world.generator().clone();
        let Some(save) = self.save_dir.as_ref() else {
            world.insert_generated(cp, generator.generate(cp));
            return;
        };
        match save.read_chunk(cp) {
            Ok(Some(chunk)) => world.insert_loaded(cp, chunk),
            Ok(None) => world.insert_generated(cp, generator.generate(cp)),
            Err(error) => {
                log::warn!("load chunk {cp:?} failed ({error:#}); regenerating");
                world.insert_generated(cp, generator.generate(cp));
            }
        }
    }

    fn schedule_generation(&mut self, world: &World, cp: IVec3) {
        if self.gen_inflight.insert(cp) {
            self.spawn(Job::Gen {
                cp,
                generator: world.generator().clone(),
            });
        }
    }

    pub(super) fn save_modified_before_unload(&mut self, world: &mut World) -> bool {
        let Some(save) = self.save_dir.as_ref() else {
            return true;
        };
        match world.save_modified(save) {
            Ok(_) => true,
            Err(error) => {
                log::error!("save before unload failed: {error:#}");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::BRICK;
    use glam::UVec3;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn world_loads_saved_chunk_instead_of_generating() {
        let name = format!(
            "stream-load-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock before epoch")
                .as_nanos()
        );
        let cp = IVec3::new(0, 2, 0);
        let save = SaveDir::open(&name).expect("open isolated save");
        let save_root = save
            .chunk_path(cp)
            .parent()
            .and_then(std::path::Path::parent)
            .expect("save chunk path has save root")
            .to_path_buf();
        let marker = UVec3::new(3, 4, 5);
        let mut saved = Chunk::new_air();
        saved.set(marker, BRICK);
        save.write_chunk(cp, &saved).expect("write saved chunk");

        let mut world = World::new(1);
        let mut streamer = Streamer::new_with_save(save);
        let desired = HashSet::from([cp]);
        streamer.pump_generation(&mut world, &desired, cp);
        assert!(streamer.load_inflight.contains(&cp));
        assert!(!streamer.gen_inflight.contains(&cp));

        let deadline = Instant::now() + Duration::from_secs(2);
        while !world.is_loaded(cp) && Instant::now() < deadline {
            streamer.drain_results(&mut world, &desired);
            std::thread::yield_now();
        }
        assert_eq!(world.chunk(cp).expect("loaded chunk").get(marker), BRICK);

        std::fs::remove_dir_all(save_root).expect("remove isolated save");
    }
}
