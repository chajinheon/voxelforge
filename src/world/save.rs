//! Persistence for world metadata and modified chunks.
//!
//! Chunk files deliberately have a small, self-identifying format:
//! `VFC1` followed by an lz4-flex size-prepended stream of little-endian
//! `u16` block ids.

use super::chunk::{CHUNK_VOLUME, Chunk};
use anyhow::{Context, Result, bail};
use glam::IVec3;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};

const CHUNK_MAGIC: &[u8; 4] = b"VFC1";
const META_FILE: &str = "world.json";
const CHUNKS_DIR: &str = "chunks";

/// The player state stored in `world.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerMeta {
    pub pos: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub fly: bool,
}

/// Save-file metadata. The version is the on-disk format version.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldMeta {
    pub version: u32,
    pub seed: u64,
    pub player: PlayerMeta,
}

/// A directory containing one world save.
#[derive(Clone, Debug)]
pub struct SaveDir {
    root: PathBuf,
}

impl SaveDir {
    /// Open (and create) `saves/<name>/` in the current directory.
    pub fn open(name: &str) -> Result<Self> {
        let mut components = Path::new(name).components();
        let valid = !name.is_empty()
            && !name.contains('/')
            && !name.contains('\\')
            && matches!(components.next(), Some(Component::Normal(_)))
            && components.next().is_none();
        if !valid {
            bail!("save name must be exactly one normal path component");
        }
        let root = PathBuf::from("saves").join(name);
        fs::create_dir_all(root.join(CHUNKS_DIR))
            .with_context(|| format!("create save directory {}", root.display()))?;
        Ok(Self { root })
    }

    pub fn chunk_path(&self, cp: IVec3) -> PathBuf {
        self.root
            .join(CHUNKS_DIR)
            .join(format!("c_{}_{}_{}.bin", cp.x, cp.y, cp.z))
    }

    pub fn write_chunk(&self, cp: IVec3, chunk: &Chunk) -> Result<()> {
        let mut raw = Vec::with_capacity(CHUNK_VOLUME * 2);
        for &id in chunk.blocks.iter() {
            raw.extend_from_slice(&id.to_le_bytes());
        }
        let compressed = lz4_flex::compress_prepend_size(&raw);
        let mut encoded = Vec::with_capacity(CHUNK_MAGIC.len() + compressed.len());
        encoded.extend_from_slice(CHUNK_MAGIC);
        encoded.extend_from_slice(&compressed);
        fs::write(self.chunk_path(cp), encoded).with_context(|| format!("write chunk {cp:?}"))?;
        Ok(())
    }

    pub fn read_chunk(&self, cp: IVec3) -> Result<Option<Chunk>> {
        let path = self.chunk_path(cp);
        let encoded = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).with_context(|| format!("read chunk {cp:?}")),
        };
        if encoded.len() < CHUNK_MAGIC.len() || &encoded[..CHUNK_MAGIC.len()] != CHUNK_MAGIC {
            bail!("invalid chunk magic in {}", path.display());
        }
        let raw = lz4_flex::decompress_size_prepended(&encoded[CHUNK_MAGIC.len()..])
            .with_context(|| format!("decompress chunk {cp:?}"))?;
        if raw.len() != CHUNK_VOLUME * 2 {
            bail!(
                "invalid chunk size for {cp:?}: got {} bytes, expected {}",
                raw.len(),
                CHUNK_VOLUME * 2
            );
        }
        let mut chunk = Chunk::new_air();
        for (slot, bytes) in chunk.blocks.iter_mut().zip(raw.chunks_exact(2)) {
            *slot = u16::from_le_bytes([bytes[0], bytes[1]]);
        }
        chunk.non_air = chunk.blocks.iter().filter(|&&id| id != 0).count() as u32;
        Ok(Some(chunk))
    }

    pub fn write_meta(&self, meta: &WorldMeta) -> Result<()> {
        let json = serde_json::to_vec_pretty(meta).context("serialize world metadata")?;
        fs::write(self.root.join(META_FILE), json).context("write world metadata")?;
        Ok(())
    }

    pub fn read_meta(&self) -> Result<Option<WorldMeta>> {
        let path = self.root.join(META_FILE);
        let json = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
        };
        serde_json::from_slice(&json)
            .with_context(|| format!("parse world metadata {}", path.display()))
            .map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::{DIRT, STONE};
    use crate::world::world::World;
    use glam::UVec3;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "voxelforge-save-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock before epoch")
                .as_nanos()
        ))
    }

    fn save_at(root: &Path) -> SaveDir {
        fs::create_dir_all(root.join(CHUNKS_DIR)).expect("create temp save");
        SaveDir {
            root: root.to_path_buf(),
        }
    }

    #[test]
    fn save_roundtrip_chunk_bytes_equal() {
        let root = temp_root("chunk");
        let save = save_at(&root);
        let cp = IVec3::new(-2, 3, 7);
        let mut original = Chunk::new_air();
        original.set(UVec3::new(0, 0, 0), STONE);
        original.set(UVec3::new(31, 31, 31), DIRT);
        original.set(UVec3::new(13, 22, 4), STONE);
        save.write_chunk(cp, &original).expect("write chunk");
        let loaded = save
            .read_chunk(cp)
            .expect("read chunk")
            .expect("chunk exists");
        assert_eq!(original.blocks.as_ref(), loaded.blocks.as_ref());
        assert_eq!(original.non_air, loaded.non_air);
        fs::remove_dir_all(root).expect("remove temp save");
    }

    #[test]
    fn world_meta_roundtrip() {
        let root = temp_root("meta");
        let save = save_at(&root);
        let expected = WorldMeta {
            version: 1,
            seed: 0x1234_5678_9abc_def0,
            player: PlayerMeta {
                pos: [1.25, 32.0, -4.5],
                yaw: 0.75,
                pitch: -0.2,
                fly: true,
            },
        };
        save.write_meta(&expected).expect("write metadata");
        assert_eq!(save.read_meta().expect("read metadata"), Some(expected));
        fs::remove_dir_all(root).expect("remove temp save");
    }

    #[test]
    fn world_accepts_saved_chunk_bytes() {
        let root = temp_root("load");
        let save = save_at(&root);
        let cp = IVec3::new(0, 0, 0);
        let mut saved = Chunk::new_air();
        saved.set(UVec3::new(3, 4, 5), STONE);
        save.write_chunk(cp, &saved).expect("write saved chunk");

        let mut world = World::new(1);
        world.insert_loaded(
            cp,
            save.read_chunk(cp)
                .expect("read saved chunk")
                .expect("saved chunk exists"),
        );
        assert_eq!(
            world
                .chunk(cp)
                .expect("loaded chunk")
                .get(UVec3::new(3, 4, 5)),
            STONE
        );
        assert_eq!(world.chunk(cp).expect("loaded chunk").non_air, 1);
        fs::remove_dir_all(root).expect("remove temp save");
    }

    #[test]
    fn unmodified_chunks_are_not_written() {
        let root = temp_root("unmodified");
        let save = save_at(&root);
        let mut world = World::new(1);
        assert!(world.ensure_loaded(IVec3::new(0, 0, 0)));
        assert_eq!(world.save_modified(&save).expect("save world"), 0);
        assert_eq!(
            fs::read_dir(root.join(CHUNKS_DIR))
                .expect("read chunks")
                .count(),
            0
        );
        fs::remove_dir_all(root).expect("remove temp save");
    }

    #[test]
    fn save_name_must_be_one_normal_component() {
        for name in ["", ".", "..", "nested/world", "/absolute", "world/"] {
            assert!(
                SaveDir::open(name).is_err(),
                "accepted invalid name {name:?}"
            );
        }
    }
}
