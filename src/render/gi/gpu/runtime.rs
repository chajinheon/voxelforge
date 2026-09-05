use super::{GpuGi, GpuParams, make_group};
use crate::render::gi::{Clipmap, GiConfig, GiMode};
use bytemuck::Zeroable;

impl GpuGi {
    /// Drop the temporal surface immediately after a world edit.  The CPU
    /// clipmap deliberately reuses revision numbers after invalidation for
    /// deterministic tests, so the GPU history needs an explicit reset hook.
    pub fn invalidate_history(&mut self) {
        self.history_valid = false;
        self.temporal_parity = false;
        self.has_previous_camera = false;
        self.has_previous_phase = false;
    }

    pub fn set_gbuffer_views(
        &mut self,
        device: &wgpu::Device,
        depth: &wgpu::TextureView,
        normal: &wgpu::TextureView,
        material: &wgpu::TextureView,
    ) {
        let make = |output, input, history, moments_in, history_out, moments_out| {
            make_group(
                device,
                &self._layout,
                &self._material_views,
                &self._light_views,
                &self.params,
                output,
                input,
                history,
                moments_in,
                history_out,
                moments_out,
                depth,
                normal,
                material,
                &self._surface_history_views[0],
                &self._surface_history_views[1],
            )
        };
        self.groups = vec![
            make(
                &self.output_views[0],
                &self.output_views[1],
                &self._history_views[0],
                &self._moments_views[0],
                &self._history_views[1],
                &self._moments_views[1],
            ),
            make(
                &self.output_views[1],
                &self.output_views[0],
                &self._history_views[1],
                &self._moments_views[1],
                &self._history_views[0],
                &self._moments_views[0],
            ),
        ];
        self.temporal_groups = [
            make(
                &self.output_views[1],
                &self.output_views[0],
                &self._history_views[0],
                &self._moments_views[0],
                &self._history_views[1],
                &self._moments_views[1],
            ),
            make(
                &self.output_views[1],
                &self.output_views[0],
                &self._history_views[1],
                &self._moments_views[1],
                &self._history_views[0],
                &self._moments_views[0],
            ),
        ];
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        queue: &wgpu::Queue,
        clipmap: &Clipmap,
        config: GiConfig,
        mode: GiMode,
        inverse_view_proj: [[f32; 4]; 4],
        view_proj: [[f32; 4]; 4],
        camera_position: [f32; 3],
        phase: f32,
        sun_direction: [f32; 4],
        sky_color: [f32; 3],
    ) {
        self.enabled = mode == GiMode::Enabled;
        let mode_changed = self.enabled != self.previous_enabled;
        let camera_delta = if self.has_previous_camera {
            let dx = camera_position[0] - self.previous_camera[0];
            let dy = camera_position[1] - self.previous_camera[1];
            let dz = camera_position[2] - self.previous_camera[2];
            (dx * dx + dy * dy + dz * dz).sqrt()
        } else {
            f32::INFINITY
        };
        let revisions = clipmap
            .levels
            .iter()
            .map(|level| level.revision)
            .collect::<Vec<_>>();
        let revision_changed = revisions
            .iter()
            .enumerate()
            .any(|(i, revision)| *revision != self.previous_revisions[i]);
        let phase_changed = self.has_previous_phase && (phase - self.previous_phase).abs() > 0.01;
        if !self.has_previous_camera
            || camera_delta > 8.0
            || revision_changed
            || mode_changed
            || phase_changed
        {
            self.history_valid = false;
            self.temporal_parity = false;
        }
        let mut params = GpuParams::zeroed();
        for (i, level) in clipmap.levels.iter().enumerate() {
            params.origins[i] = [
                level.origin.x,
                level.origin.y,
                level.origin.z,
                level.cell_size,
            ];
            params.rings[i] = [
                level.ring_offset.x,
                level.ring_offset.y,
                level.ring_offset.z,
                u32::from(level.ready),
            ];
        }
        params.params = [
            config.max_distance,
            config.rays.min(4) as f32,
            config.intensity,
            u8::from(self.enabled) as f32,
        ];
        params.inverse_view_proj = inverse_view_proj;
        params.previous_view_proj = self.previous_view_proj;
        params.camera_position = [
            camera_position[0],
            camera_position[1],
            camera_position[2],
            u8::from(self.history_valid) as f32,
        ];
        params.sun_direction = sun_direction;
        params.sky_color = [sky_color[0], sky_color[1], sky_color[2], 1.0];
        queue.write_buffer(&self.params, 0, bytemuck::bytes_of(&params));
        self.previous_view_proj = view_proj;
        self.previous_camera = camera_position;
        self.has_previous_camera = true;
        self.previous_enabled = self.enabled;
        self.previous_phase = phase;
        self.has_previous_phase = true;
        for (i, revision) in revisions.into_iter().enumerate() {
            self.previous_revisions[i] = revision;
        }
    }

    pub fn record<'a>(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        trace_timing: Option<wgpu::ComputePassTimestampWrites<'a>>,
        temporal_timing: Option<wgpu::ComputePassTimestampWrites<'a>>,
        denoise_timing: Option<wgpu::ComputePassTimestampWrites<'a>>,
    ) {
        if !self.enabled {
            return;
        }
        // Fresh wgpu resources are zero-initialized before first use. The
        // temporal shader also forces history weight to zero until valid.
        let first = 0usize;
        let second = 1usize;
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("vf/m9/gi-trace"),
                timestamp_writes: trace_timing,
            });
            pass.set_pipeline(&self.trace);
            pass.set_bind_group(0, &self.groups[first], &[]);
            pass.dispatch_workgroups(self.width.div_ceil(8), self.height.div_ceil(8), 1);
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("vf/m9/gi-temporal"),
                timestamp_writes: temporal_timing,
            });
            pass.set_pipeline(&self.temporal);
            pass.set_bind_group(
                0,
                &self.temporal_groups[usize::from(self.temporal_parity)],
                &[],
            );
            pass.dispatch_workgroups(self.width.div_ceil(8), self.height.div_ceil(8), 1);
        }
        let denoise_query = denoise_timing.as_ref().map(|timing| {
            (
                timing.query_set,
                timing.beginning_of_pass_write_index,
                timing.end_of_pass_write_index,
            )
        });
        for (stage, (step, output)) in [(1_u32, first), (2, second), (4, first)]
            .into_iter()
            .enumerate()
        {
            let timestamp_writes = denoise_query.and_then(|(query_set, begin, end)| match step {
                1 => Some(wgpu::ComputePassTimestampWrites {
                    query_set,
                    beginning_of_pass_write_index: begin,
                    end_of_pass_write_index: None,
                }),
                4 => Some(wgpu::ComputePassTimestampWrites {
                    query_set,
                    beginning_of_pass_write_index: None,
                    end_of_pass_write_index: end,
                }),
                _ => None,
            });
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(match step {
                    1 => "vf/m9/gi-atrous-1",
                    2 => "vf/m9/gi-atrous-2",
                    _ => "vf/m9/gi-atrous-4",
                }),
                timestamp_writes,
            });
            pass.set_pipeline(&self.denoise[stage]);
            pass.set_bind_group(0, &self.groups[output], &[]);
            pass.dispatch_workgroups(self.width.div_ceil(8), self.height.div_ceil(8), 1);
        }
        self.history_valid = true;
        self.final_output = 0;
        self.temporal_parity = !self.temporal_parity;
    }

    pub fn output_view(&self) -> &wgpu::TextureView {
        &self.output_views[self.final_output]
    }
}
