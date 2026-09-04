//! Lightweight mtime polling for editable WGSL shaders.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Polls one shader path at a 0.5 second cadence.
pub struct ShaderWatcher {
    path: PathBuf,
    last_modified: Option<SystemTime>,
    last_poll: SystemTime,
    interval: Duration,
    forced: bool,
}

impl ShaderWatcher {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        Self {
            last_modified: modified(&path),
            path,
            last_poll: SystemTime::now(),
            interval: Duration::from_millis(500),
            forced: false,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Ask whether a reload should be attempted. It returns true only on the
    /// polling cadence (or after [`Self::force`]) and does not read shader text.
    pub fn poll(&mut self) -> bool {
        if self.forced {
            self.forced = false;
            self.last_poll = SystemTime::now();
            self.last_modified = modified(&self.path);
            return true;
        }
        let now = SystemTime::now();
        if now.duration_since(self.last_poll).unwrap_or(Duration::ZERO) < self.interval {
            return false;
        }
        self.last_poll = now;
        let current = modified(&self.path);
        if current != self.last_modified {
            self.last_modified = current;
            true
        } else {
            false
        }
    }

    pub fn force(&mut self) {
        self.forced = true;
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
}
