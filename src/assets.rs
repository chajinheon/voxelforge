//! Asset path resolution and small, non-panicking file helpers.

use std::path::{Path, PathBuf};

/// Return the asset root used by the application.
///
/// `VF_ASSETS` is useful for running the binary from a different checkout or
/// for tests.  The repository's `assets` directory is the default.
pub fn dir() -> PathBuf {
    std::env::var_os("VF_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets")))
}

/// Resolve a path relative to the asset root.
pub fn path(relative: impl AsRef<Path>) -> PathBuf {
    dir().join(relative)
}

/// Read a UTF-8 asset, adding the resolved path to I/O errors.
pub fn read_text(relative: impl AsRef<Path>) -> anyhow::Result<String> {
    let path = path(relative);
    let bytes = std::fs::read(&path)?;
    String::from_utf8(bytes).map_err(|error| anyhow::anyhow!("{}: {error}", path.display()))
}
