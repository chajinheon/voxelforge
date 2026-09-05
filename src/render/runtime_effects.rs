//! Persistent M8/M9 screen-space resources and passes.
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
mod accessors;
mod resources;
mod stages;
use resources::*;
const MAX_MIPS: u32 = 16;
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct RuntimeEffectParams {
    pub time: f32,
    pub water: f32,
    pub clouds: f32,
    pub volumetric: f32,
    pub gi: f32,
    pub gi_mode: f32,
    pub exposure: f32,
    pub debug_view: f32,
    pub underwater: f32,
    pub _padding: [f32; 3],
    pub quality0: [f32; 4], // volumetric steps, cloud view steps, cloud light steps, reserved
}
impl Default for RuntimeEffectParams {
    fn default() -> Self {
        Self {
            time: 0.0,
            water: 1.0,
            clouds: 1.0,
            volumetric: 1.0,
            gi: 0.65,
            gi_mode: 1.0,
            exposure: 1.0,
            debug_view: 0.0,
            underwater: 0.0,
            _padding: [0.0; 3],
            quality0: [32.0, 40.0, 6.0, 0.0],
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimeMode {
    pub water: bool,
    pub clouds: bool,
    pub volumetric: bool,
    pub gi: bool,
    pub debug_view: u32,
}
#[derive(Clone, Copy)]
pub(super) enum EffectStage {
    GiTrace,
    GiTemporal,
    GiDenoise,
    GiComposite,
    Volumetric,
    Clouds,
    CloudShadow,
}
impl EffectStage {
    const ALL: [Self; 7] = [
        Self::GiTrace,
        Self::GiTemporal,
        Self::GiDenoise,
        Self::GiComposite,
        Self::Volumetric,
        Self::Clouds,
        Self::CloudShadow,
    ];
    const fn index(self) -> usize {
        self as usize
    }
    const fn entry(self) -> &'static str {
        match self {
            Self::GiTrace => "gi_trace",
            Self::GiTemporal => "gi_temporal",
            Self::GiDenoise => "gi_denoise",
            Self::GiComposite => "gi_composite",
            Self::Volumetric => "volumetric",
            Self::Clouds => "clouds",
            Self::CloudShadow => "cloud_shadow",
        }
    }
}
pub struct RuntimeEffects {
    width: u32,
    height: u32,
    quarter_width: u32,
    quarter_height: u32,
    mip_count: u32,
    format: wgpu::TextureFormat,
    _depth: wgpu::Texture,
    depth_views: Vec<wgpu::TextureView>,
    depth_full_view: wgpu::TextureView,
    depth_linear_layout: wgpu::BindGroupLayout,
    depth_reduce_layout: wgpu::BindGroupLayout,
    depth_linear_pipeline: wgpu::ComputePipeline,
    depth_reduce_pipeline: wgpu::ComputePipeline,
    depth_groups: Vec<wgpu::BindGroup>,
    effect_layout: wgpu::BindGroupLayout,
    temporal_layout: wgpu::BindGroupLayout,
    upsample_layout: wgpu::BindGroupLayout,
    gi_layout: wgpu::BindGroupLayout,
    gi_pipeline: wgpu::RenderPipeline,
    pub(super) effect_pipelines: [wgpu::RenderPipeline; 7],
    quarter_upsample_pipeline: wgpu::RenderPipeline,
    quarter_temporal_pipeline: wgpu::RenderPipeline,
    pub(super) sampler: wgpu::Sampler,
    pub(super) params: wgpu::Buffer,
    params_cpu: RuntimeEffectParams,
    _ping: [wgpu::Texture; 2],
    pub(super) ping_views: [wgpu::TextureView; 2],
    _quarter_ping: [wgpu::Texture; 2],
    quarter_views: [wgpu::TextureView; 2],
    _quarter_history: [wgpu::Texture; 2],
    quarter_history_views: [wgpu::TextureView; 2],
    quarter_groups: Option<[wgpu::BindGroup; 2]>,
    quarter_temporal_groups: Option<[wgpu::BindGroup; 2]>,
    upsample_groups: Option<[wgpu::BindGroup; 2]>,
    quarter_history_read_groups: Option<[wgpu::BindGroup; 2]>,
    quarter_read: usize,
    pub(super) source_group: Option<wgpu::BindGroup>,
    pub(super) gi_group: Option<wgpu::BindGroup>,
    pub(super) ping_groups: Option<[wgpu::BindGroup; 2]>,
    pub(super) active: Option<usize>,
}
impl RuntimeEffects {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        // Atmospherics are evaluated at quarter resolution and reconstructed
        // once at the end of the graph.
        let quarter_width = width.div_ceil(4);
        let quarter_height = height.div_ceil(4);
        let mip_count = width.max(height).ilog2().saturating_add(1).min(MAX_MIPS);
        let depth = create_depth_texture(device, width, height, mip_count);
        let depth_views = create_mip_views(&depth, mip_count);
        let depth_full_view = depth.create_view(&wgpu::TextureViewDescriptor {
            label: Some("vf/m8/depth-pyramid-full"),
            base_mip_level: 0,
            mip_level_count: Some(mip_count),
            ..Default::default()
        });
        let depth_linear_layout = depth_layout(device, wgpu::TextureSampleType::Depth);
        let depth_reduce_layout =
            depth_layout(device, wgpu::TextureSampleType::Float { filterable: false });
        let depth_linear_pipeline = compute_pipeline(
            device,
            "vf/m8/depth-linear",
            &depth_linear_layout,
            include_str!("../../assets/shaders/runtime_depth_linear.wgsl"),
        );
        let depth_reduce_pipeline = compute_pipeline(
            device,
            "vf/m8/depth-reduce",
            &depth_reduce_layout,
            include_str!("../../assets/shaders/runtime_depth_reduce.wgsl"),
        );
        let effect_layout = effect_layout(device);
        let temporal_layout = temporal_layout(device);
        let upsample_layout = upsample_layout(device);
        let gi_layout = gi_composite_layout(device);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m8-m9/effects-shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../assets/shaders/runtime_effects.wgsl").into(),
            ),
        });
        let effect_pipelines = EffectStage::ALL
            .map(|stage| render_pipeline(device, &effect_layout, &module, format, stage.entry()));
        let quarter_upsample_pipeline = render_pipeline(
            device,
            &upsample_layout,
            &module,
            format,
            "quarter_upsample",
        );
        let quarter_temporal_pipeline = render_pipeline(
            device,
            &temporal_layout,
            &module,
            format,
            "quarter_temporal",
        );
        let gi_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m9/gi-composite-shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../assets/shaders/gi_composite_render.wgsl").into(),
            ),
        });
        let gi_pipeline = render_pipeline(device, &gi_layout, &gi_module, format, "gi_composite");
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vf/m8-m9/effects-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let params_cpu = RuntimeEffectParams::default();
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vf/m8-m9/effects-params"),
            contents: bytemuck::bytes_of(&params_cpu),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let ping = [
            create_color_texture(device, width, height, format, "vf/m8-m9/ping-a"),
            create_color_texture(device, width, height, format, "vf/m8-m9/ping-b"),
        ];
        let ping_views = [
            ping[0].create_view(&Default::default()),
            ping[1].create_view(&Default::default()),
        ];
        let quarter_ping = [
            create_color_texture(
                device,
                quarter_width,
                quarter_height,
                format,
                "vf/m8/quarter-ping-a",
            ),
            create_color_texture(
                device,
                quarter_width,
                quarter_height,
                format,
                "vf/m8/quarter-ping-b",
            ),
        ];
        let quarter_views = [
            quarter_ping[0].create_view(&Default::default()),
            quarter_ping[1].create_view(&Default::default()),
        ];
        let quarter_history = [
            create_color_texture(
                device,
                quarter_width,
                quarter_height,
                format,
                "vf/m8/quarter-history-a",
            ),
            create_color_texture(
                device,
                quarter_width,
                quarter_height,
                format,
                "vf/m8/quarter-history-b",
            ),
        ];
        let quarter_history_views = [
            quarter_history[0].create_view(&Default::default()),
            quarter_history[1].create_view(&Default::default()),
        ];
        Self {
            width,
            height,
            quarter_width,
            quarter_height,
            mip_count,
            format,
            _depth: depth,
            depth_views,
            depth_full_view,
            depth_linear_layout,
            depth_reduce_layout,
            depth_linear_pipeline,
            depth_reduce_pipeline,
            depth_groups: Vec::new(),
            effect_layout,
            temporal_layout,
            upsample_layout,
            gi_layout,
            gi_pipeline,
            effect_pipelines,
            quarter_upsample_pipeline,
            quarter_temporal_pipeline,
            sampler,
            params,
            params_cpu,
            _ping: ping,
            ping_views,
            _quarter_ping: quarter_ping,
            quarter_views,
            _quarter_history: quarter_history,
            quarter_history_views,
            quarter_groups: None,
            quarter_temporal_groups: None,
            upsample_groups: None,
            quarter_history_read_groups: None,
            quarter_read: 0,
            source_group: None,
            gi_group: None,
            ping_groups: None,
            active: None,
        }
    }
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if (self.width, self.height) != (width.max(1), height.max(1)) {
            *self = Self::new(device, width, height, self.format);
        }
    }
    pub fn configure_mode(&mut self, mode: RuntimeMode) {
        self.params_cpu.water = u8::from(mode.water) as f32;
        self.params_cpu.clouds = u8::from(mode.clouds) as f32;
        self.params_cpu.volumetric = u8::from(mode.volumetric) as f32;
        self.params_cpu.gi_mode = u8::from(mode.gi) as f32;
        self.params_cpu.debug_view = mode.debug_view as f32;
    }
    pub fn update(&mut self, queue: &wgpu::Queue, params: RuntimeEffectParams) {
        self.params_cpu = params;
        queue.write_buffer(&self.params, 0, bytemuck::bytes_of(&params));
    }
}
