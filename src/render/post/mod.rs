//! Deterministic M7 post-processing contracts.
//!
//! The CPU routines in this module are also the reference used by snapshot and
//! validation code. GPU owners can use the resource helpers to allocate the
//! fixed-size ping-pong targets once at init/resize time.

pub mod bloom;
pub mod exposure;
pub mod taau;
pub mod tonemap;

pub use bloom::{BLOOM_LEVELS, BloomConfig, BloomGpu, BloomPyramid};
pub use exposure::{ExposureConfig, ExposureState, average_log_luminance};
pub use taau::{HALTON8, TaauGpu, TaauHistory, halton8, halton8_clip_offset};
pub use tonemap::{aces_fitted, grade_aces, sharpen_5_tap};
