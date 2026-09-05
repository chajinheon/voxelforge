//! M7 half-resolution GTAO chain: horizon, separable bilateral, and temporal history.

use super::gtao_gpu::*;
use wgpu::util::DeviceExt;

const SHADER: &str = include_str!("../../../assets/shaders/gtao.wgsl");
const NEAR: f32 = 0.05;
const FAR: f32 = 1000.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GtaoUniform {
    inverse_view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4],
    viewport: [f32; 4],
    depth_range: [f32; 4],
    frame: u32,
    static_weight: f32,
    motion_weight: f32,
    _pad: f32,
}

pub(super) struct GtaoPass {
    size: (u32, u32),
    _raw: wgpu::Texture,
    _horizontal: wgpu::Texture,
    history: [wgpu::Texture; 2],
    stable: wgpu::Texture,
    _blue_noise: wgpu::Texture,
    raw_view: wgpu::TextureView,
    horizontal_view: wgpu::TextureView,
    history_views: [wgpu::TextureView; 2],
    stable_view: wgpu::TextureView,
    blue_view: wgpu::TextureView,
    layout: wgpu::BindGroupLayout,
    raw_pipeline: wgpu::RenderPipeline,
    horizontal_pipeline: wgpu::RenderPipeline,
    temporal_pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    params: wgpu::Buffer,
    groups: Option<[wgpu::BindGroup; 2]>,
    raw_groups: Option<[wgpu::BindGroup; 2]>,
    filter_groups: Option<[wgpu::BindGroup; 2]>,
    read: usize,
    _valid: bool,
    inverse_view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 3],
    viewport: [f32; 2],
}

pub(super) struct GtaoInputs<'a> {
    pub(super) depth: &'a wgpu::TextureView,
    pub(super) normal: &'a wgpu::TextureView,
    pub(super) motion: &'a wgpu::TextureView,
    pub(super) old_depth: &'a [wgpu::TextureView; 2],
    pub(super) old_normal: &'a [wgpu::TextureView; 2],
    pub(super) inverse_view_proj: [[f32; 4]; 4],
    pub(super) camera_pos: [f32; 3],
    pub(super) viewport: [f32; 2],
}

impl GtaoPass {
    pub(super) fn new(device: &wgpu::Device, queue: &wgpu::Queue, internal: (u32, u32)) -> Self {
        let size = (internal.0.div_ceil(2).max(1), internal.1.div_ceil(2).max(1));
        let make = |label: &str| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let raw = make("vf/m7/gtao/raw-horizon");
        let horizontal = make("vf/m7/gtao/bilateral-horizontal");
        let history = [0, 1].map(|index| make(&format!("vf/m7/gtao/history-{index}")));
        let stable = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vf/m7/gtao/stable-output"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let raw_view = raw.create_view(&Default::default());
        let horizontal_view = horizontal.create_view(&Default::default());
        let history_views = history
            .each_ref()
            .map(|texture| texture.create_view(&Default::default()));
        let stable_view = stable.create_view(&Default::default());
        let blue_noise = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vf/m7/gtao/blue-noise-rank"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let noise = crate::render::noise::BlueNoiseRank::default_seed();
        let bytes = noise
            .ranks()
            .iter()
            .map(|rank| (rank / 16) as u8)
            .collect::<Vec<_>>();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &blue_noise,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(64),
                rows_per_image: Some(64),
            },
            wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
        );
        let blue_view = blue_noise.create_view(&Default::default());
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vf/m7/gtao/layout"),
            entries: &[
                texture_depth(0),
                texture_float(1),
                texture_float(2),
                texture_float(3),
                texture_unfilterable(4),
                texture_float(5),
                texture_float(6),
                sampler(7),
                uniform(8),
                texture_float(9),
                texture_float(10),
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m7/gtao/shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let raw_pipeline = pipeline(device, &layout, &shader, "raw_main");
        let horizontal_pipeline = pipeline(device, &layout, &shader, "horizontal_main");
        let temporal_pipeline = pipeline(device, &layout, &shader, "temporal_main");
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vf/m7/gtao/point"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vf/m7/gtao/params"),
            contents: bytemuck::bytes_of(&GtaoUniform {
                inverse_view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
                camera_pos: [0.0; 4],
                viewport: [
                    size.0 as f32,
                    size.1 as f32,
                    1.0 / size.0 as f32,
                    1.0 / size.1 as f32,
                ],
                depth_range: [NEAR, FAR, 0.0, 0.0],
                frame: 0,
                static_weight: 0.90,
                motion_weight: 0.75,
                _pad: 0.0,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            size,
            _raw: raw,
            _horizontal: horizontal,
            history,
            stable,
            _blue_noise: blue_noise,
            raw_view,
            horizontal_view,
            history_views,
            stable_view,
            blue_view,
            layout,
            raw_pipeline,
            horizontal_pipeline,
            temporal_pipeline,
            sampler,
            params,
            groups: None,
            raw_groups: None,
            filter_groups: None,
            read: 0,
            _valid: false,
            inverse_view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
            camera_pos: [0.0; 3],
            viewport: [size.0 as f32, size.1 as f32],
        }
    }

    pub(super) fn prepare(&mut self, device: &wgpu::Device, inputs: GtaoInputs<'_>) {
        let make = |previous: usize| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vf/m7/gtao/bind"),
                layout: &self.layout,
                entries: &[
                    view_entry(0, inputs.depth),
                    view_entry(1, inputs.normal),
                    view_entry(2, inputs.motion),
                    view_entry(3, &self.history_views[previous]),
                    view_entry(4, &inputs.old_depth[previous]),
                    view_entry(5, &inputs.old_normal[previous]),
                    view_entry(6, &self.raw_view),
                    sampler_entry(7, &self.sampler),
                    buffer_entry(8, self.params.as_entire_binding()),
                    view_entry(9, &self.horizontal_view),
                    view_entry(10, &self.blue_view),
                ],
            })
        };
        self.groups = Some([make(0), make(1)]);
        let make_raw = |previous: usize| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vf/m7/gtao/raw-bind"),
                layout: &self.layout,
                entries: &[
                    view_entry(0, inputs.depth),
                    view_entry(1, inputs.normal),
                    view_entry(2, inputs.motion),
                    view_entry(3, &self.history_views[previous]),
                    view_entry(4, &inputs.old_depth[previous]),
                    view_entry(5, &inputs.old_normal[previous]),
                    view_entry(6, &self.stable_view),
                    sampler_entry(7, &self.sampler),
                    buffer_entry(8, self.params.as_entire_binding()),
                    view_entry(9, &self.horizontal_view),
                    view_entry(10, &self.blue_view),
                ],
            })
        };
        self.raw_groups = Some([make_raw(0), make_raw(1)]);
        let make_filter = |previous: usize| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vf/m7/gtao/filter-bind"),
                layout: &self.layout,
                entries: &[
                    view_entry(0, inputs.depth),
                    view_entry(1, inputs.normal),
                    view_entry(2, inputs.motion),
                    view_entry(3, &self.history_views[previous]),
                    view_entry(4, &inputs.old_depth[previous]),
                    view_entry(5, &inputs.old_normal[previous]),
                    view_entry(6, &self.raw_view),
                    sampler_entry(7, &self.sampler),
                    buffer_entry(8, self.params.as_entire_binding()),
                    view_entry(9, &self.stable_view),
                    view_entry(10, &self.blue_view),
                ],
            })
        };
        self.filter_groups = Some([make_filter(0), make_filter(1)]);
        self.camera_pos = inputs.camera_pos;
        self.inverse_view_proj = inputs.inverse_view_proj;
        self.viewport = inputs.viewport;
    }

    pub(super) fn update(&mut self, queue: &wgpu::Queue, frame: u32) {
        queue.write_buffer(
            &self.params,
            0,
            bytemuck::bytes_of(&GtaoUniform {
                inverse_view_proj: self.inverse_view_proj,
                camera_pos: [
                    self.camera_pos[0],
                    self.camera_pos[1],
                    self.camera_pos[2],
                    1.0,
                ],
                viewport: [
                    self.viewport[0],
                    self.viewport[1],
                    1.0 / self.viewport[0].max(1.0),
                    1.0 / self.viewport[1].max(1.0),
                ],
                depth_range: [NEAR, FAR, 0.0, 0.0],
                frame,
                static_weight: 0.90,
                motion_weight: 0.75,
                _pad: 0.0,
            }),
        );
    }

    pub(super) fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        timestamp: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        let (Some(groups), Some(raw_groups), Some(filter_groups)) =
            (&self.groups, &self.raw_groups, &self.filter_groups)
        else {
            return;
        };
        let write = 1 - self.read;
        let beginning = timestamp
            .as_ref()
            .map(|writes| wgpu::RenderPassTimestampWrites {
                query_set: writes.query_set,
                beginning_of_pass_write_index: writes.beginning_of_pass_write_index,
                end_of_pass_write_index: None,
            });
        let ending = timestamp
            .as_ref()
            .map(|writes| wgpu::RenderPassTimestampWrites {
                query_set: writes.query_set,
                beginning_of_pass_write_index: None,
                end_of_pass_write_index: writes.end_of_pass_write_index,
            });
        draw(
            encoder,
            "vf/m7/gtao/raw-horizon",
            &self.raw_view,
            &self.raw_pipeline,
            &raw_groups[self.read],
            beginning,
        );
        draw(
            encoder,
            "vf/m7/gtao/bilateral-horizontal",
            &self.horizontal_view,
            &self.horizontal_pipeline,
            &filter_groups[self.read],
            None,
        );
        draw(
            encoder,
            "vf/m7/gtao/bilateral-vertical-temporal",
            &self.history_views[write],
            &self.temporal_pipeline,
            &groups[self.read],
            ending,
        );
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.history[write],
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &self.stable,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: self.size.0,
                height: self.size.1,
                depth_or_array_layers: 1,
            },
        );
        self.read = write;
        self._valid = true;
    }

    pub(super) fn view<'a>(&'a self, history: &'a [wgpu::TextureView; 2]) -> &'a wgpu::TextureView {
        let _ = history;
        &self.stable_view
    }

    pub(super) fn reset(&mut self) {
        self.read = 0;
        self._valid = false;
    }
}
