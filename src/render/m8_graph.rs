//! Dedicated M8 volumetric/cloud GPU graph and its persistent resources.

use crate::render::clouds::{CLOUD_SHADOW_WORLD_SIZE, CloudShadowSchedule};
use crate::render::noise::{BlueNoiseRank, PeriodicNoise};
use crate::render::volumetric::FogLightGpu;
use bytemuck::{Pod, Zeroable};
use glam::Vec2;
use wgpu::util::DeviceExt;

mod api;
mod pipelines;
use pipelines::{make_cloud, make_composite, make_shadow, make_volumetric};

pub(crate) const VOLUME_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct VolumetricGpuParams {
    pub max_distance: f32,
    pub density_scale: f32,
    pub history_weight: f32,
    pub underwater: f32,
    pub sun_height: f32,
    pub parity: [u32; 2],
    pub padding: u32,
    pub tail: [u32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct CloudGpuParams {
    pub base_altitude: f32,
    pub top_altitude: f32,
    pub coverage: f32,
    pub max_distance: f32,
    pub history_weight: f32,
    pub parity: [u32; 2],
    pub padding: u32,
    pub tail: [u32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct M8CameraParams {
    pub inverse_view_proj: [[f32; 4]; 4],
    pub previous_view_proj: [[f32; 4]; 4],
    pub viewport: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct M8CompositeParams {
    pub debug_view: u32,
    pub _padding: [u32; 7],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct CloudShadowGpuParams {
    pub origin: [f32; 2],
    pub world_size: f32,
    pub sun_height: f32,
    pub time: f32,
    pub padding: [f32; 3],
    pub tail: [u32; 4],
}

pub(crate) struct VolumeTargets {
    pub textures: [wgpu::Texture; 2],
    pub views: [wgpu::TextureView; 2],
}

impl VolumeTargets {
    pub(crate) fn new(device: &wgpu::Device, width: u32, height: u32, label: &str) -> Self {
        let textures = [0, 1].map(|index| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("vf/m8/{label}-{index}")),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: VOLUME_FORMAT,
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        });
        let views = [0, 1].map(|index| textures[index].create_view(&Default::default()));
        Self { textures, views }
    }
}

pub struct M8Graph {
    width: u32,
    height: u32,
    frame: u64,
    effect_group: wgpu::BindGroup,
    cloud_group: wgpu::BindGroup,
    volume_params: wgpu::Buffer,
    cloud_params: wgpu::Buffer,
    camera_params: wgpu::Buffer,
    composite_params: wgpu::Buffer,
    shadow_params: wgpu::Buffer,
    lights: wgpu::Buffer,
    base_noise: wgpu::Texture,
    detail_noise: wgpu::Texture,
    blue_noise: wgpu::Texture,
    cloud_shadow: wgpu::Texture,
    shadow_view: wgpu::TextureView,
    volume: VolumeTargets,
    cloud: VolumeTargets,
    volumetric_pipeline: wgpu::ComputePipeline,
    cloud_pipeline: wgpu::ComputePipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    volumetric_groups: [wgpu::BindGroup; 2],
    cloud_groups: [wgpu::BindGroup; 2],
    shadow_groups: [wgpu::BindGroup; 2],
    composite_pipeline: wgpu::RenderPipeline,
    composite_groups: [wgpu::BindGroup; 2],
    shadow_schedule: CloudShadowSchedule,
    last_sun_height: f32,
}
impl M8Graph {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        globals_buffer: &wgpu::Buffer,
        width: u32,
        height: u32,
        scene: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        normal: &wgpu::TextureView,
        csm_shadow: &wgpu::TextureView,
    ) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let params = pipelines::buffer(device, "vf/m8/graph/volumetric-params", 40);
        let cloud_params = pipelines::buffer(device, "vf/m8/graph/cloud-params", 40);
        let camera_params = pipelines::buffer(
            device,
            "vf/m8/graph/camera-params",
            std::mem::size_of::<M8CameraParams>(),
        );
        let composite_params = pipelines::buffer(device, "vf/m8/graph/composite-params", 32);
        let shadow_params = pipelines::buffer(device, "vf/m8/graph/shadow-params", 48);
        let lights = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vf/m8/graph/local-lights-8"),
            contents: bytemuck::cast_slice(&[FogLightGpu::default(); 8]),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let cloud_shadow = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vf/m8/graph/cloud-shadow-512"),
            size: wgpu::Extent3d {
                width: 512,
                height: 512,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let shadow_view = cloud_shadow.create_view(&Default::default());
        let csm_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vf/m8/csm-comparison"),
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let shadow_clear = vec![255u8; 512 * 512];
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &cloud_shadow,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &shadow_clear,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(512),
                rows_per_image: Some(512),
            },
            wgpu::Extent3d {
                width: 512,
                height: 512,
                depth_or_array_layers: 1,
            },
        );
        let effect_layout = pipelines::effect_layout(device);
        let cloud_layout = pipelines::cloud_layout(device);
        let effect_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m8/graph/effect-group"),
            layout: &effect_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: lights.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: camera_params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(csm_shadow),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&csm_sampler),
                },
            ],
        });
        let cloud_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m8/graph/cloud-group"),
            layout: &cloud_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: cloud_params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: lights.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: camera_params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(csm_shadow),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&csm_sampler),
                },
            ],
        });
        let base_cpu = PeriodicNoise::generate(128, 0x72f1_0a9b, 4);
        let detail_cpu = PeriodicNoise::generate(32, 0x3c6e_f372, 3);
        let base_noise = pipelines::noise_volume(device, 128, "vf/m8/graph/base-noise");
        let detail_noise = pipelines::noise_volume(device, 32, "vf/m8/graph/detail-noise");
        pipelines::upload_volume(queue, &base_noise, &base_cpu);
        pipelines::upload_volume(queue, &detail_noise, &detail_cpu);
        let blue_noise = pipelines::blue_texture(device);
        pipelines::upload_blue(queue, &blue_noise, &BlueNoiseRank::default_seed());
        let base_view = pipelines::view3(&base_noise);
        let detail_view = pipelines::view3(&detail_noise);
        let blue_view = blue_noise.create_view(&Default::default());
        let sampler = pipelines::noise_sampler(device);
        let depth_sampler = pipelines::depth_sampler(device);
        let quarter = (width.div_ceil(4), height.div_ceil(4));
        let volume = VolumeTargets::new(device, quarter.0, quarter.1, "volumetric");
        let cloud = VolumeTargets::new(device, quarter.0, quarter.1, "cloud");
        let (volumetric_pipeline, volumetric_groups) = make_volumetric(
            device,
            &effect_layout,
            &volume,
            &blue_view,
            depth,
            normal,
            &sampler,
            &depth_sampler,
        );
        let (cloud_pipeline, cloud_groups) = make_cloud(
            device,
            &effect_layout,
            &cloud,
            &base_view,
            &detail_view,
            depth,
            normal,
            &sampler,
            &depth_sampler,
        );
        let (shadow_pipeline, shadow_groups) =
            make_shadow(device, &shadow_params, &base_view, &detail_view, &sampler);
        let (composite_pipeline, composite_groups) = make_composite(
            device,
            scene,
            depth,
            normal,
            &volume.views,
            &cloud.views,
            &composite_params,
        );
        let mut graph = Self {
            width,
            height,
            frame: 0,
            effect_group,
            cloud_group,
            volume_params: params,
            cloud_params,
            camera_params,
            composite_params,
            shadow_params,
            lights,
            base_noise,
            detail_noise,
            blue_noise,
            cloud_shadow,
            shadow_view,
            volume,
            cloud,
            volumetric_pipeline,
            cloud_pipeline,
            shadow_pipeline,
            volumetric_groups,
            cloud_groups,
            shadow_groups,
            composite_pipeline,
            composite_groups,
            shadow_schedule: CloudShadowSchedule::default(),
            last_sun_height: -1.0,
        };
        graph.update_params(queue, 0.0, false, 1.0, true);
        graph
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        time: f32,
        camera_xz: Vec2,
        underwater: bool,
        sun_height: f32,
        timestamps: [Option<wgpu::ComputePassTimestampWrites<'_>>; 3],
        shadow_timestamp: Option<wgpu::RenderPassTimestampWrites<'_>>,
        inverse_view_proj: glam::Mat4,
        previous_view_proj: glam::Mat4,
        debug_view: u32,
        reset_history: bool,
    ) {
        self.frame = self.frame.saturating_add(1);
        self.update_params(queue, time, underwater, sun_height, reset_history);
        queue.write_buffer(
            &self.camera_params,
            0,
            bytemuck::bytes_of(&M8CameraParams {
                inverse_view_proj: inverse_view_proj.to_cols_array_2d(),
                previous_view_proj: previous_view_proj.to_cols_array_2d(),
                viewport: [
                    self.width as f32,
                    self.height as f32,
                    1.0 / self.width as f32,
                    1.0 / self.height as f32,
                ],
            }),
        );
        queue.write_buffer(
            &self.cloud_params,
            0,
            bytemuck::bytes_of(&CloudGpuParams {
                base_altitude: 180.0,
                top_altitude: 260.0,
                coverage: 0.52,
                max_distance: 512.0,
                history_weight: if reset_history { 0.0 } else { 0.95 },
                parity: [(self.frame & 1) as u32, ((self.frame >> 1) & 1) as u32],
                padding: 0,
                tail: [0; 2],
            }),
        );
        queue.write_buffer(
            &self.composite_params,
            0,
            bytemuck::bytes_of(&M8CompositeParams {
                debug_view,
                _padding: [0; 7],
            }),
        );
        let output = (self.frame as usize) & 1;
        let snapped = CloudShadowSchedule::snapped_origin(camera_xz);
        if self.shadow_schedule.should_update(self.frame, camera_xz)
            || (sun_height - self.last_sun_height).abs() > 0.01
        {
            self.shadow_schedule.update(self.frame, camera_xz);
            self.last_sun_height = sun_height;
            queue.write_buffer(
                &self.shadow_params,
                0,
                bytemuck::bytes_of(&CloudShadowGpuParams {
                    origin: [snapped[0] as f32, snapped[1] as f32],
                    world_size: CLOUD_SHADOW_WORLD_SIZE,
                    sun_height,
                    time,
                    padding: [0.0; 3],
                    tail: [0; 4],
                }),
            );
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vf/m8/cloud-shadow-512"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.shadow_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: shadow_timestamp.clone(),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.shadow_pipeline);
            pass.set_bind_group(0, &self.shadow_groups[0], &[]);
            pass.set_bind_group(1, &self.shadow_groups[1], &[]);
            pass.draw(0..3, 0..1);
        }
        let groups = self.width.div_ceil(4).div_ceil(8);
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("vf/m8/volumetric-quarter"),
            timestamp_writes: timestamps[0].clone(),
        });
        pass.set_pipeline(&self.volumetric_pipeline);
        pass.set_bind_group(0, &self.effect_group, &[]);
        pass.set_bind_group(1, &self.volumetric_groups[output], &[]);
        pass.dispatch_workgroups(groups, self.height.div_ceil(4).div_ceil(8), 1);
        drop(pass);
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("vf/m8/cloud-quarter"),
            timestamp_writes: timestamps[1].clone(),
        });
        pass.set_pipeline(&self.cloud_pipeline);
        pass.set_bind_group(0, &self.cloud_group, &[]);
        pass.set_bind_group(1, &self.cloud_groups[output], &[]);
        pass.dispatch_workgroups(groups, self.height.div_ceil(4).div_ceil(8), 1);
        drop(pass);
        let extent = wgpu::Extent3d {
            width: self.width.div_ceil(4),
            height: self.height.div_ceil(4),
            depth_or_array_layers: 1,
        };
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.volume.textures[output],
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &self.volume.textures[output ^ 1],
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            extent,
        );
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.cloud.textures[output],
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &self.cloud.textures[output ^ 1],
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            extent,
        );
    }
}
