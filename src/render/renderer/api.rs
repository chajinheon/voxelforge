//! Public renderer configuration and frontend entry points.

use super::{GpuChunkMeshes, RenderPreset, RenderView, Renderer};
use crate::render::allocation_stats::AllocationDelta;
use crate::render::globals::Globals;
use crate::render::gpu_timing::GpuFrameTimings;
use crate::render::ui::UiFrame;
use crate::shaderpack::ShaderPack;

impl Renderer {
    pub fn set_m8_local_lights(&mut self, lights: &[super::super::volumetric::FogLight]) {
        self.m8_local_lights.clear();
        self.m8_local_lights.extend_from_slice(lights);
    }
    /// Compile and atomically publish the pack's shader/texture bundle.
    pub fn apply_shader_pack(&mut self, pack: &ShaderPack) -> anyhow::Result<bool> {
        let mut shader_modules = std::collections::BTreeMap::new();
        for relative in &pack.manifest().shaders {
            let canonical = std::path::Path::new(relative)
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| anyhow::anyhow!("invalid shader path: {relative}"))?;
            let source = pack
                .shader_source(canonical)?
                .ok_or_else(|| anyhow::anyhow!("missing shader source: {canonical}"))?;
            let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
            let module = self
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(canonical),
                    source: wgpu::ShaderSource::Wgsl(source.into()),
                });
            if let Some(error) = pollster::block_on(scope.pop()) {
                anyhow::bail!("shader pack compile rejected ({canonical}): {error}");
            }
            shader_modules.insert(canonical.to_owned(), module);
        }
        let chunk_source = pack.shader_source("chunk.wgsl")?;
        let deferred_source = pack.shader_source("deferred.wgsl")?;
        let water_source = pack.shader_source("water.wgsl")?;
        let mut overrides = Vec::new();
        for path in &pack.manifest().textures {
            if let Some(override_asset) = pack.texture_override(path)? {
                overrides.push(override_asset);
            }
        }
        if chunk_source.is_none()
            && deferred_source.is_none()
            && water_source.is_none()
            && overrides.is_empty()
        {
            return Ok(false);
        }

        // Compile every requested replacement against the live layouts before
        // publishing any field.  Texture targets are checked as well, so a
        // bad layer name cannot leave a partially applied pack behind.
        for override_asset in &overrides {
            let name = std::path::Path::new(&override_asset.relative_path)
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(|| anyhow::anyhow!("invalid texture path"))?;
            let registry_name = match override_asset.kind {
                crate::shaderpack::AssetKind::BlockTexture => name,
                crate::shaderpack::AssetKind::MaterialTexture => {
                    name.strip_suffix("_material").unwrap_or(name)
                }
                crate::shaderpack::AssetKind::EmissionTexture => {
                    name.strip_suffix("_emission").unwrap_or(name)
                }
                crate::shaderpack::AssetKind::HandTexture => {
                    if override_asset.data.len() != 16 * 16 * 4 {
                        anyhow::bail!("shader pack hand texture is not RGBA8 16x16");
                    }
                    continue;
                }
                crate::shaderpack::AssetKind::Shader => unreachable!(),
            };
            if !self
                .textures
                .names
                .iter()
                .any(|entry| entry == registry_name)
            {
                anyhow::bail!("shader pack texture is not in the registry: {registry_name}");
            }
            let expected = match override_asset.kind {
                crate::shaderpack::AssetKind::BlockTexture
                | crate::shaderpack::AssetKind::MaterialTexture => 16 * 16 * 4,
                crate::shaderpack::AssetKind::EmissionTexture => 16 * 16,
                crate::shaderpack::AssetKind::HandTexture => 16 * 16 * 4,
                crate::shaderpack::AssetKind::Shader => 0,
            };
            if override_asset.data.len() != expected {
                anyhow::bail!(
                    "shader pack texture {} has {} bytes, expected {expected}",
                    override_asset.relative_path,
                    override_asset.data.len()
                );
            }
        }
        let bindings = self.chunks.scene_bindings();
        let chunk_candidates = if let Some(source) = chunk_source.as_deref() {
            let shader_path = pack.root().join("shaders/chunk.wgsl");
            Some((
                super::super::chunk_pipeline::ChunkPipeline::new_shared_source(
                    &self.device,
                    self.color_format,
                    source,
                    &shader_path,
                    bindings.clone(),
                    false,
                )?,
                super::super::translucent::TranslucentPipeline::new_shared_source(
                    &self.device,
                    super::FRAME_FORMAT,
                    source,
                    &shader_path,
                    bindings.clone(),
                )?,
            ))
        } else {
            None
        };
        let deferred_candidate = deferred_source
            .as_deref()
            .map(|source| {
                super::super::deferred::DeferredPipeline::new_source(
                    &self.device,
                    bindings.clone(),
                    super::FRAME_FORMAT,
                    source,
                )
            })
            .transpose()?;
        let shadow_candidate = pack
            .shader_source("shadow.wgsl")?
            .map(|source| {
                super::super::shadow::ShadowPipeline::new_with_config_source(
                    &self.device,
                    &bindings,
                    self.shadow.resolution(),
                    self.shadow.far_distance(),
                    &source,
                )
            })
            .transpose()?;
        let present_candidate = pack.shader_source("present.wgsl")?.map(|source| {
            super::super::final_pass::FinalPass::new_source(
                &self.device,
                self.color_format,
                &source,
            )
        });
        let water_candidate = water_source
            .as_deref()
            .map(|source| {
                super::super::water_pipeline::WaterPipeline::new_source(
                    &self.device,
                    bindings.clone(),
                    source.to_owned(),
                    pack.root().join("shaders/water.wgsl"),
                )
            })
            .transpose()?;

        // The GPU writes below cannot fail synchronously.  All fallible
        // validation and construction has completed, so publishing is one
        // short commit point for the entire pack bundle.
        for override_asset in &overrides {
            match override_asset.kind {
                crate::shaderpack::AssetKind::BlockTexture => self.textures.apply_override(
                    &self.queue,
                    &override_asset.relative_path,
                    &override_asset.data,
                )?,
                crate::shaderpack::AssetKind::MaterialTexture
                | crate::shaderpack::AssetKind::EmissionTexture => {
                    self.chunks.scene_bindings().apply_texture_override(
                        &self.queue,
                        override_asset.kind,
                        &override_asset.relative_path,
                        &override_asset.data,
                    )?
                }
                crate::shaderpack::AssetKind::HandTexture => {
                    // UiRenderer owns the hand resource and its bind group.
                    self.ui_renderer
                        .apply_hand_override(&self.queue, &override_asset.data)?;
                }
                crate::shaderpack::AssetKind::Shader => unreachable!(),
            }
        }
        if let Some((opaque, translucent)) = chunk_candidates {
            self.chunks = opaque;
            self.translucent = translucent;
        }
        if let Some(deferred) = deferred_candidate {
            self.deferred = deferred;
        }
        if let Some(water) = water_candidate {
            self.water = water;
        }
        if let Some(shadow) = shadow_candidate {
            self.shadow = shadow;
            self.shadow_camera = None;
        }
        if let Some(present) = present_candidate {
            self.final_pass = present;
        }
        self.shader_pack_modules = shader_modules;
        if let Some(effects) = self.m7_effects.as_mut() {
            effects.history_reset();
        }
        Ok(true)
    }

    /// Initialize the GPU clipmap after the platform adapter is available.
    pub fn configure_gpu_gi(&mut self, adapter: &wgpu::Adapter, width: u32, height: u32) {
        if self.gpu_gi.is_some() {
            return;
        }
        self.gi_adapter = Some(adapter.clone());
        match super::super::gi::GpuGi::try_new(&self.device, adapter, width, height) {
            Ok(gpu_gi) => {
                log::info!("M9 GPU GI enabled: 4x128^3 clipmap, quarter-res trace");
                self.gpu_gi = Some(gpu_gi);
            }
            Err(reason) => log::warn!("GI disabled: {reason}; using baked light + GTAO"),
        }
    }

    pub fn gi_dispatch_count(&self) -> u64 {
        self.gi_dispatches
    }

    pub fn upload_gi(&mut self, upload: &super::super::gi::VoxelUpload) {
        if let Some(gpu_gi) = self.gpu_gi.as_mut() {
            gpu_gi.upload(&self.queue, &self.gi_clipmap, upload);
        }
    }

    /// Publish a clipmap revision only after every queued slab for that
    /// revision has reached the GPU.
    pub fn mark_gi_level_ready(&mut self, level: usize, revision: u32) {
        if let Some(level_state) = self.gi_clipmap.levels.get_mut(level)
            && level_state.revision == revision
        {
            level_state.mark_ready(revision);
        }
    }

    pub fn update_gi_clipmap(
        &mut self,
        camera: glam::Vec3,
    ) -> Vec<super::super::gi::ClipmapUpdate> {
        self.gi_clipmap.update_camera(camera)
    }

    /// Force all clipmap levels to be re-uploaded after a world edit.
    pub fn invalidate_gi_clipmap(&mut self) {
        self.gi_clipmap.invalidate_all();
        if let Some(gpu_gi) = self.gpu_gi.as_mut() {
            gpu_gi.invalidate_history();
        }
    }

    /// Force the cascaded shadow maps to cover a world edit immediately.
    pub fn invalidate_shadows(&mut self) {
        self.shadow.invalidate();
    }

    /// Actual live scene and UI vertex/index allocations owned by this
    /// renderer.
    pub fn uploaded_buffer_memory_bytes(&self, chunks: &[GpuChunkMeshes]) -> usize {
        chunks
            .iter()
            .map(GpuChunkMeshes::buffer_memory_bytes)
            .chain(
                self.lod_meshes
                    .iter()
                    .map(super::super::lod_pipeline::GpuLodMesh::buffer_memory_bytes),
            )
            .fold(
                self.ui_renderer.buffer_memory_bytes(),
                usize::saturating_add,
            )
    }

    /// Actual live render-target allocations. Optional graphs contribute zero
    /// until they have been created for a rendered frame.
    pub fn render_target_memory_bytes(&self) -> usize {
        let gbuffer = self
            .gbuffer
            .as_ref()
            .map_or(0, super::super::gbuffer::GBuffer::texture_memory_bytes);
        let frame = self.frame_targets.as_ref().map_or(
            0,
            super::super::frame_targets::FrameTargets::texture_memory_bytes,
        );
        let m7 = self
            .m7_effects
            .as_ref()
            .map_or(0, super::super::m7_effects::M7Effects::texture_memory_bytes);
        let runtime = self.runtime_effects.as_ref().map_or(
            0,
            super::super::runtime_effects::RuntimeEffects::texture_memory_bytes,
        );
        let m8 = self
            .m8_graph
            .as_ref()
            .map_or(0, super::super::m8_graph::M8Graph::texture_memory_bytes);
        let gi_auxiliary = self
            .gpu_gi
            .as_ref()
            .map_or(0, super::super::gi::GpuGi::auxiliary_texture_memory_bytes);
        let ui_blur = self
            .ui_blur
            .as_ref()
            .map_or(0, super::super::ui_blur::UiBlur::texture_memory_bytes);
        gbuffer
            .saturating_add(frame)
            .saturating_add(m7)
            .saturating_add(runtime)
            .saturating_add(m8)
            .saturating_add(gi_auxiliary)
            .saturating_add(ui_blur)
            .saturating_add(self.shadow.texture_memory_bytes())
    }

    /// Live 128^3 material/light clipmap allocation. A missing GPU GI object is
    /// reported as zero rather than the CPU contract's theoretical size.
    pub fn gi_clipmap_memory_bytes(&self) -> usize {
        self.gpu_gi
            .as_ref()
            .map_or(0, super::super::gi::GpuGi::clipmap_texture_memory_bytes)
    }

    pub fn configure_preset(&mut self, preset: RenderPreset, scale: Option<f32>) {
        self.preset = preset;
        self.render_scale = scale;
        self.gbuffer = None;
        self.frame_targets = None;
        self.m7_effects = None;
        self.runtime_effects = None;
        self.previous_view_proj = None;
        self.previous_camera = None;
    }

    pub fn preset(&self) -> RenderPreset {
        self.preset
    }

    pub fn configure_exposure(&mut self, exposure: Option<f32>) {
        self.auto_exposure = exposure.is_none();
        if let Some(exposure) = exposure
            && exposure.is_finite()
            && exposure > 0.0
        {
            self.exposure = exposure;
            self.frame_targets = None;
        }
    }

    pub fn configure_gpu_timing(&mut self, enabled: bool) {
        self.gpu_timing.set_enabled(enabled);
    }

    pub fn configure_gi(&mut self, enabled: bool) {
        self.gi_enabled = enabled;
    }

    /// Apply persisted quality controls before the first frame.  Graphs read
    /// these values during their next prepare/update, so this remains a
    /// transactional front-end setting rather than a hot-path mutation.
    #[allow(clippy::too_many_arguments)]
    pub fn configure_render_quality(
        &mut self,
        taa: bool,
        sharpen: f32,
        gi_rays: u32,
        gi_distance: f32,
        volumetric_steps: u32,
        cloud_view_steps: u32,
        cloud_light_steps: u32,
        ssr_steps: u32,
        shadow_resolution: u32,
        shadow_distance: f32,
        pom_steps: u32,
    ) {
        self.taa_enabled = taa;
        self.sharpen = sharpen.clamp(0.0, 1.0);
        self.gi_config.rays = gi_rays.clamp(1, 6);
        self.gi_config.max_distance = gi_distance.clamp(16.0, 64.0);
        self.volumetric_steps = volumetric_steps.clamp(16, 64);
        self.cloud_view_steps = cloud_view_steps.clamp(16, 64);
        self.cloud_light_steps = cloud_light_steps.clamp(1, 16);
        self.ssr_steps = ssr_steps.clamp(16, 64);
        let shadow_resolution = [1024, 1536, 2048]
            .into_iter()
            .min_by_key(|candidate| shadow_resolution.abs_diff(*candidate))
            .unwrap_or(2048);
        let shadow_distance = shadow_distance.clamp(128.0, 256.0);
        if self.shadow.resolution() != shadow_resolution
            || (self.shadow.far_distance() - shadow_distance).abs() > f32::EPSILON
        {
            if let Ok(shadow) = super::super::shadow::ShadowPipeline::new_with_config(
                &self.device,
                &self.chunks.scene_bindings(),
                shadow_resolution,
                shadow_distance,
            ) {
                self.shadow = shadow;
                self.shadow_camera = None;
            } else {
                log::warn!("quality shadow settings rejected; retaining previous shadow pipeline");
            }
        }
        self.pom_steps = pom_steps.clamp(1, 32);
    }

    pub fn gpu_timing_supported(&self) -> bool {
        self.gpu_timing.supported()
    }

    pub fn force_shader_reload(&mut self) {
        self.watcher.force();
        self.outline_watcher.force();
        if let Some(effects) = self.m7_effects.as_mut() {
            effects.history_reset();
        }
        self.previous_view_proj = None;
        self.previous_camera = None;
    }

    /// Supplies the simulation's player-volume state to the forward water pass.
    pub fn set_underwater(&mut self, underwater: bool) {
        self.underwater = underwater;
    }

    pub fn render(
        &mut self,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        globals: &Globals,
        chunks: &[GpuChunkMeshes],
    ) -> usize {
        self.render_internal(color, depth, globals, chunks, None, RenderView::Final, None)
    }

    pub fn render_view(
        &mut self,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        globals: &Globals,
        chunks: &[GpuChunkMeshes],
        view: RenderView,
    ) -> usize {
        self.render_internal(color, depth, globals, chunks, None, view, None)
    }

    pub fn render_view_with_outline(
        &mut self,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        globals: &Globals,
        chunks: &[GpuChunkMeshes],
        selected: Option<glam::IVec3>,
        view: RenderView,
    ) -> usize {
        self.render_internal(color, depth, globals, chunks, selected, view, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_app_frame(
        &mut self,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        globals: &Globals,
        chunks: &[GpuChunkMeshes],
        selected: Option<glam::IVec3>,
        view: RenderView,
        ui: &UiFrame<'_>,
    ) -> usize {
        self.render_internal(color, depth, globals, chunks, selected, view, Some(ui))
    }

    pub fn render_view_with_ui(
        &mut self,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        globals: &Globals,
        chunks: &[GpuChunkMeshes],
        view: RenderView,
        ui: &UiFrame<'_>,
    ) -> usize {
        self.render_internal(color, depth, globals, chunks, None, view, Some(ui))
    }

    pub fn render_with_outline(
        &mut self,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        globals: &Globals,
        chunks: &[GpuChunkMeshes],
        selected: Option<glam::IVec3>,
    ) -> usize {
        self.render_internal(
            color,
            depth,
            globals,
            chunks,
            selected,
            RenderView::Final,
            None,
        )
    }

    pub fn last_gpu_timing_ns(&self) -> Option<u64> {
        self.gpu_timing
            .last_frame
            .map(|timing| (timing.total_ms() * 1_000_000.0) as u64)
    }

    pub fn last_gpu_timings(&self) -> Option<GpuFrameTimings> {
        self.gpu_timing.last_frame
    }

    /// Take the newest completed GPU sample.  GPU readback is asynchronous;
    /// callers that record a series of frames should consume samples rather
    /// than repeatedly peeking the same completed readback.
    pub fn take_last_gpu_timings(&mut self) -> Option<GpuFrameTimings> {
        self.gpu_timing.take_last_frame()
    }

    /// Complete already-submitted timing maps. Offscreen benchmark tools may
    /// call this after an explicit device wait; the interactive render loop
    /// continues to use nonblocking polling only.
    pub fn poll_gpu_timings(&mut self) {
        self.gpu_timing.finish(&self.device, &self.queue);
    }

    /// Return the renderer-owned allocation events observed by the last frame.
    /// Counters are enabled only when `VF_ALLOC_STATS=1`.
    pub fn last_allocation_stats(&self) -> AllocationDelta {
        self.allocation_stats.last_frame()
    }

    pub fn allocation_stats_enabled(&self) -> bool {
        self.allocation_stats.enabled()
    }

    pub fn icon_bake_ms(&self) -> u64 {
        self.ui_renderer.icon_bake_ms()
    }
}
