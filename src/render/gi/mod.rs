//! Voxel GI primitives.  The CPU side is deterministic and is also the
//! reference contract used by the compute passes.

pub mod clipmap;
pub mod denoise;
pub mod gpu;
pub mod temporal;
pub mod trace;
pub mod voxelize;

pub use clipmap::{
    CLIPMAP_LEVELS, CLIPMAP_RESOLUTION, Clipmap, ClipmapLevel, ClipmapUpdate, SlabRegion,
    TEXTURE_MEMORY_BYTES, UPLOAD_BUDGET_BYTES, VoxelUpload,
};
pub use denoise::{AtrousImage, atrous_filter};
pub use gpu::GpuGi;
pub use temporal::{TemporalHistory, TemporalSample, history_accepts, temporal_resolve};
pub use trace::{
    DdaHit, GiRay, MAX_CROSSINGS, MAX_DISTANCE, MAX_RAYS, TraceConfig, WORKGROUP_SIZE,
    cosine_hemisphere_sample, dda_first_hit, hammersley, select_level,
};
pub use voxelize::{VoxelCell, VoxelKind, VoxelPacker, emission_rgb};

use bytemuck::{Pod, Zeroable};

pub const GI_INTENSITY: f32 = 0.65;
pub const GI_HISTORY_WEIGHT: f32 = 0.90;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ClipmapUniform {
    pub origin_cell_size: [[i32; 4]; CLIPMAP_LEVELS],
    pub ring_offset_ready: [[u32; 4]; CLIPMAP_LEVELS],
    pub params: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GiMode {
    Enabled,
    Fallback,
    DisabledByUser,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GiConfig {
    pub rays: u32,
    pub max_distance: f32,
    pub intensity: f32,
}

impl Default for GiConfig {
    fn default() -> Self {
        Self {
            rays: 4,
            max_distance: 48.0,
            intensity: GI_INTENSITY,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GiCapabilities {
    pub rgba8_3d: bool,
    pub rgba16_storage: bool,
    pub max_3d_dimension: u32,
}

impl GiCapabilities {
    pub fn supported(self) -> bool {
        self.rgba8_3d && self.rgba16_storage && self.max_3d_dimension >= CLIPMAP_RESOLUTION
    }

    pub fn unsupported_reason(self) -> &'static str {
        if !self.rgba8_3d {
            "Rgba8Unorm 3D texture usage unsupported"
        } else if !self.rgba16_storage {
            "Rgba16Float storage texture unsupported"
        } else if self.max_3d_dimension < CLIPMAP_RESOLUTION {
            "3D texture dimension limit below 128"
        } else {
            "unknown capability failure"
        }
    }

    /// Query adapter format features without creating any GI resources.
    pub fn from_adapter(adapter: &wgpu::Adapter) -> Self {
        let rgba8 = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba8Unorm);
        let rgba16 = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba16Float);
        let rgba8_usage = wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_DST;
        Self {
            rgba8_3d: rgba8.allowed_usages.contains(rgba8_usage),
            rgba16_storage: rgba16
                .allowed_usages
                .contains(wgpu::TextureUsages::STORAGE_BINDING),
            max_3d_dimension: adapter.limits().max_texture_dimension_3d,
        }
    }
}

#[derive(Debug)]
pub struct GiRenderer {
    pub mode: GiMode,
    pub config: GiConfig,
    pub clipmap: Clipmap,
    pub history: TemporalHistory,
    dispatches: u64,
    fallback_reason: Option<String>,
}

impl GiRenderer {
    /// All state is constructed before publishing Enabled, so a failed init
    /// cannot leave half of the GI graph active.
    pub fn transactional_init(
        capabilities: GiCapabilities,
        config: GiConfig,
        force_failure: bool,
    ) -> Self {
        let failure = force_failure
            .then_some("forced GI initialization failure")
            .or_else(|| (!capabilities.supported()).then(|| capabilities.unsupported_reason()));
        if let Some(reason) = failure {
            return Self {
                mode: GiMode::Fallback,
                config,
                clipmap: Clipmap::new(),
                history: TemporalHistory::new(),
                dispatches: 0,
                fallback_reason: Some(reason.into()),
            };
        }
        Self {
            mode: GiMode::Enabled,
            config,
            clipmap: Clipmap::new(),
            history: TemporalHistory::new(),
            dispatches: 0,
            fallback_reason: None,
        }
    }

    pub fn disabled_by_user(config: GiConfig) -> Self {
        Self {
            mode: GiMode::DisabledByUser,
            config,
            clipmap: Clipmap::new(),
            history: TemporalHistory::new(),
            dispatches: 0,
            fallback_reason: None,
        }
    }

    pub fn dispatch(&mut self) -> bool {
        if self.mode == GiMode::Enabled {
            self.dispatches = self.dispatches.saturating_add(1);
            true
        } else {
            false
        }
    }

    pub fn dispatch_count(&self) -> u64 {
        self.dispatches
    }

    pub fn fallback_message(&self) -> Option<String> {
        self.fallback_reason
            .as_ref()
            .map(|reason| format!("GI disabled: {reason}; using baked light + GTAO"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gi_failure_uses_baked_lighting() {
        let mut gi = GiRenderer::transactional_init(
            GiCapabilities {
                rgba8_3d: true,
                rgba16_storage: true,
                max_3d_dimension: 128,
            },
            GiConfig::default(),
            true,
        );
        assert_eq!(gi.mode, GiMode::Fallback);
        assert!(!gi.dispatch());
        assert_eq!(gi.dispatch_count(), 0);
        let message = gi.fallback_message().unwrap_or_default();
        assert_eq!(message.matches("GI disabled:").count(), 1);
        assert!(message.contains("using baked light + GTAO"));
        assert!(0.65_f32.is_finite());
    }
}
