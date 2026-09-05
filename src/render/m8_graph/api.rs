use super::{CLOUD_SHADOW_WORLD_SIZE, CloudShadowGpuParams, M8Graph, VolumetricGpuParams};
use crate::render::volumetric::{FogLight, FogLightGpu};

impl M8Graph {
    pub(super) fn update_params(
        &mut self,
        queue: &wgpu::Queue,
        time: f32,
        underwater: bool,
        sun_height: f32,
        reset_history: bool,
    ) {
        let params = VolumetricGpuParams {
            max_distance: 192.0,
            density_scale: 1.0,
            history_weight: if reset_history { 0.0 } else { 0.90 },
            underwater: f32::from(underwater),
            sun_height,
            parity: [(self.frame & 1) as u32, ((self.frame >> 1) & 1) as u32],
            padding: 0,
            tail: [0; 2],
        };
        queue.write_buffer(&self.volume_params, 0, bytemuck::bytes_of(&params));
        let shadow = CloudShadowGpuParams {
            origin: [0.0; 2],
            world_size: CLOUD_SHADOW_WORLD_SIZE,
            sun_height,
            time,
            padding: [0.0; 3],
            tail: [0; 4],
        };
        queue.write_buffer(&self.shadow_params, 0, bytemuck::bytes_of(&shadow));
    }

    pub fn cloud_shadow_will_update(&self, camera_xz: glam::Vec2, sun_height: f32) -> bool {
        self.shadow_schedule
            .should_update(self.frame.saturating_add(1), camera_xz)
            || (sun_height - self.last_sun_height).abs() > 0.01
    }

    pub fn record_composite(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("vf/m8/volumetric-cloud-composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.composite_pipeline);
        pass.set_bind_group(2, &self.composite_groups[(self.frame as usize) & 1], &[]);
        pass.draw(0..3, 0..1);
    }
    pub fn texture_memory_bytes(&self) -> usize {
        [
            &self.base_noise,
            &self.detail_noise,
            &self.blue_noise,
            &self.cloud_shadow,
        ]
        .into_iter()
        .map(crate::render::memory::texture_bytes)
        .sum::<usize>()
            + self
                .volume
                .textures
                .iter()
                .chain(self.cloud.textures.iter())
                .map(crate::render::memory::texture_bytes)
                .sum::<usize>()
    }
    pub fn quarter_resolution(&self) -> (u32, u32) {
        (self.width.div_ceil(4), self.height.div_ceil(4))
    }
    pub fn cloud_shadow_view(&self) -> &wgpu::TextureView {
        &self.shadow_view
    }
    pub fn update_local_lights(&self, queue: &wgpu::Queue, lights: &[FogLight]) {
        let mut selected = [FogLightGpu::default(); 8];
        for (gpu, light) in selected.iter_mut().zip(lights.iter().take(8)) {
            *gpu = FogLightGpu {
                position: light.position.to_array(),
                radius: light.radius,
                color: light.color.to_array(),
                _padding: 0.0,
            };
        }
        queue.write_buffer(&self.lights, 0, bytemuck::cast_slice(&selected));
    }
}
