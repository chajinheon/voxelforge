use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use glam::IVec3;

use crate::mesh::{ChunkMeshes, mesh_chunk_greedy_all};
use crate::world::chunk::{Chunk, PaddedChunk};
use crate::world::r#gen::WorldGen;
use crate::world::light::{LightColumnResult, LightColumnSnapshot, solve_column};
use crate::world::save::SaveDir;

pub(super) enum Job {
    Gen {
        cp: IVec3,
        generator: Arc<WorldGen>,
    },
    Load {
        cp: IVec3,
        save: SaveDir,
    },
    #[allow(dead_code)]
    Light {
        snapshot: LightColumnSnapshot,
    },
    Mesh {
        cp: IVec3,
        padded: Box<PaddedChunk>,
        version: u64,
        epoch: u64,
    },
}

pub(super) enum ResultMessage {
    Gen {
        cp: IVec3,
        chunk: Chunk,
    },
    Load {
        cp: IVec3,
        result: Result<Option<Chunk>, String>,
    },
    Light {
        result: LightColumnResult,
        elapsed: Duration,
    },
    Mesh {
        cp: IVec3,
        version: u64,
        epoch: u64,
        mesh: ChunkMeshes,
    },
}

pub(super) fn execute(job: Job, sender: Sender<ResultMessage>) {
    match job {
        Job::Gen { cp, generator } => {
            let chunk = generator.generate(cp);
            let _ = sender.send(ResultMessage::Gen { cp, chunk });
        }
        Job::Load { cp, save } => {
            let result = save.read_chunk(cp).map_err(|error| format!("{error:#}"));
            let _ = sender.send(ResultMessage::Load { cp, result });
        }
        Job::Light { snapshot } => {
            let started = Instant::now();
            let result = solve_column(&snapshot);
            let elapsed = started.elapsed();
            let _ = sender.send(ResultMessage::Light { result, elapsed });
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
    }
}
