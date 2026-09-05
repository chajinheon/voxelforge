//! Surface-independent frame renderer.
use super::allocation_stats::AllocationStats;
use super::chunk_pipeline::{ChunkPipeline, GpuChunkMeshes};
use super::deferred::DeferredPipeline;
use super::draw_lists::DrawLists;
use super::draw_lists::TransparentKind;
use super::final_pass::FinalPass;
use super::frame_targets::FrameTargets;
use super::frustum::Frustum;
use super::gbuffer::{GBuffer, GBufferPipeline};
use super::gi::{Clipmap as GiClipmap, GiConfig, GiMode, GpuGi};
use super::globals::Globals;
use super::gpu_timing::{GpuTiming, TimedPass};
use super::lod_pipeline::{GpuLodMesh, LodPipeline};
use super::outline::OutlinePipeline;
use super::preset::RenderPreset;
use super::scene_bindings::SceneBindings;
use super::shader_watch::ShaderWatcher;
use super::shadow::ShadowPipeline;
use super::textures::BlockTextures;
use super::translucent::TranslucentPipeline;
use super::water_pipeline::WaterPipeline;
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;
mod api;
mod effects;
mod frame;
const FRAME_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

fn halton_2d(index: u64) -> [f32; 2] {
    fn radical_inverse(mut value: u64, base: u64) -> f32 {
        let mut fraction = 1.0_f32 / base as f32;
        let mut result = 0.0;
        while value > 0 {
            result += (value % base) as f32 * fraction;
            value /= base;
            fraction /= base as f32;
        }
        result
    }
    [radical_inverse(index, 2), radical_inverse(index, 3)]
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderView {
    #[default]
    Final,
    Light,
    Albedo,
    Normal,
    Depth,
    Material,
    Motion,
    Reactive,
    Water,
    Volumetric,
    Cloud,
    Lod,
    Gi,
    Clipmap,
}
pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub textures: BlockTextures,
    pub chunks: ChunkPipeline,
    pub translucent: TranslucentPipeline,
    pub water: WaterPipeline,
    pub outline: OutlinePipeline,
    pub gbuffer_pipeline: GBufferPipeline,
    pub deferred: DeferredPipeline,
    shadow: ShadowPipeline,
    shadow_camera: Option<glam::Vec3>,
    frame_index: u64,
    previous_view_proj: Option<glam::Mat4>,
    previous_camera: Option<glam::Vec3>,
    gbuffer: Option<GBuffer>,
    preset: RenderPreset,
    render_scale: Option<f32>,
    watcher: ShaderWatcher,
    outline_watcher: ShaderWatcher,
    color_format: wgpu::TextureFormat,
    frame_targets: Option<FrameTargets>,
    final_pass: FinalPass,
    exposure: f32,
    auto_exposure: bool,
    gpu_timing: GpuTiming,
    draw_lists: DrawLists,
    m7_effects: Option<super::m7_effects::M7Effects>,
    runtime_effects: Option<super::runtime_effects::RuntimeEffects>,
    m8_graph: Option<super::m8_graph::M8Graph>,
    m8_local_lights: Vec<super::volumetric::FogLight>,
    gi_enabled: bool,
    gi_config: GiConfig,
    taa_enabled: bool,
    sharpen: f32,
    volumetric_steps: u32,
    cloud_view_steps: u32,
    cloud_light_steps: u32,
    ssr_steps: u32,
    pom_steps: u32,
    underwater: bool,
    ui_renderer: super::ui::UiRenderer,
    ui_blur: Option<super::ui_blur::UiBlur>,
    pub(crate) lod_pipeline: LodPipeline,
    pub(crate) lod_meshes: Vec<GpuLodMesh>,
    pub(crate) gpu_gi: Option<GpuGi>,
    pub(crate) gi_adapter: Option<wgpu::Adapter>,
    pub(crate) gi_clipmap: GiClipmap,
    pub(crate) gi_dispatches: u64,
    allocation_stats: Rc<AllocationStats>,
    pub(crate) shader_pack_modules: BTreeMap<String, wgpu::ShaderModule>,
}
impl Renderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
    ) -> anyhow::Result<Self> {
        Self::with_assets(device, queue, color_format, &crate::assets::dir())
    }
    pub fn with_assets(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        asset_root: &Path,
    ) -> anyhow::Result<Self> {
        let textures = BlockTextures::from_registry(device, queue, asset_root)?;
        let shader_path = asset_root.join("shaders/chunk.wgsl");
        let bindings = SceneBindings::new(device, queue, &textures);
        let chunks =
            ChunkPipeline::new_shared(device, color_format, &shader_path, bindings.clone(), false)?;
        let translucent =
            TranslucentPipeline::new_shared(device, FRAME_FORMAT, &shader_path, bindings.clone())?;
        let water = WaterPipeline::new(
            device,
            bindings.clone(),
            asset_root.join("shaders/water.wgsl"),
        )?;
        let gbuffer_pipeline = GBufferPipeline::new(device, bindings.clone())?;
        let deferred = DeferredPipeline::new(device, bindings, FRAME_FORMAT)?;
        let shadow = ShadowPipeline::new(device, &chunks.scene_bindings())?;
        let outline_path = asset_root.join("shaders/outline.wgsl");
        let outline = OutlinePipeline::new(
            device,
            FRAME_FORMAT,
            chunks.globals_layout(),
            chunks.globals_bind_group(),
            &outline_path,
        )?;
        let lod_pipeline = LodPipeline::new(
            device,
            chunks.globals_layout(),
            chunks.globals_bind_group(),
            FRAME_FORMAT,
        )?;
        let allocation_stats = AllocationStats::from_environment();
        Ok(Self {
            device: device.clone(),
            queue: queue.clone(),
            textures,
            chunks,
            translucent,
            water,
            outline,
            gbuffer_pipeline,
            deferred,
            shadow,
            shadow_camera: None,
            frame_index: 0,
            previous_view_proj: None,
            previous_camera: None,
            gbuffer: None,
            preset: RenderPreset::default(),
            render_scale: None,
            watcher: ShaderWatcher::new(shader_path),
            outline_watcher: ShaderWatcher::new(outline_path),
            color_format,
            frame_targets: None,
            final_pass: FinalPass::new(device, color_format),
            exposure: 1.0,
            auto_exposure: true,
            gpu_timing: GpuTiming::new(device),
            draw_lists: DrawLists::with_stats(allocation_stats.clone()),
            m7_effects: None,
            runtime_effects: None,
            m8_graph: None,
            m8_local_lights: Vec::new(),
            gi_enabled: true,
            gi_config: GiConfig::default(),
            taa_enabled: true,
            sharpen: 0.18,
            volumetric_steps: 32,
            cloud_view_steps: 40,
            cloud_light_steps: 6,
            ssr_steps: 40,
            pom_steps: 8,
            underwater: false,
            ui_renderer: super::ui::UiRenderer::new_with_stats(
                device,
                queue,
                color_format,
                Some(allocation_stats.clone()),
            ),
            ui_blur: None,
            lod_pipeline,
            lod_meshes: Vec::new(),
            gpu_gi: None,
            gi_adapter: None,
            gi_clipmap: GiClipmap::new(),
            gi_dispatches: 0,
            allocation_stats,
            shader_pack_modules: BTreeMap::new(),
        })
    }

    /// Build the coupled opaque/translucent/water bundle before publishing any
    /// member. A malformed edit therefore leaves the complete previous bundle
    /// active for the next frame.
    fn reload_chunk_water_bundle(&mut self) {
        let chunk_path = self.chunks.shader_path().to_owned();
        let water_path = self.water.shader_path().to_owned();
        let Ok(chunk_source) = std::fs::read_to_string(&chunk_path) else {
            return;
        };
        let Ok(water_source) = std::fs::read_to_string(&water_path) else {
            return;
        };
        let bindings = self.chunks.scene_bindings();
        let Ok(opaque) = ChunkPipeline::new_shared_source(
            &self.device,
            self.color_format,
            &chunk_source,
            &chunk_path,
            bindings.clone(),
            false,
        ) else {
            log::warn!("chunk shader reload rejected");
            return;
        };
        let Ok(translucent) = TranslucentPipeline::new_shared_source(
            &self.device,
            FRAME_FORMAT,
            &chunk_source,
            &chunk_path,
            bindings.clone(),
        ) else {
            log::warn!("translucent shader reload rejected");
            return;
        };
        let Ok(water) =
            WaterPipeline::new_source(&self.device, bindings, water_source, &water_path)
        else {
            log::warn!("water shader reload rejected");
            return;
        };
        self.chunks = opaque;
        self.translucent = translucent;
        self.water = water;
        if let Some(effects) = self.m7_effects.as_mut() {
            effects.history_reset();
        }
    }
}
