use super::{
    EffectStage, RuntimeEffects, make_depth_group, make_effect_group, make_gi_composite_group,
    make_temporal_group, make_upsample_group,
};

impl RuntimeEffects {
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        source: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) {
        self.depth_groups.clear();
        self.depth_groups.push(make_depth_group(
            device,
            &self.depth_linear_layout,
            depth,
            &self.depth_views[0],
        ));
        for mip in 1..self.mip_count {
            self.depth_groups.push(make_depth_group(
                device,
                &self.depth_reduce_layout,
                &self.depth_views[mip as usize - 1],
                &self.depth_views[mip as usize],
            ));
        }
        self.source_group = Some(self.make_effect_group(device, source));
        self.gi_group = None;
        self.ping_groups = Some([
            self.make_effect_group(device, &self.ping_views[0]),
            self.make_effect_group(device, &self.ping_views[1]),
        ]);
        self.quarter_groups = Some([
            self.make_effect_group(device, &self.quarter_views[0]),
            self.make_effect_group(device, &self.quarter_views[1]),
        ]);
        self.quarter_temporal_groups = Some([
            make_temporal_group(
                device,
                &self.temporal_layout,
                &self.quarter_views[1],
                &self.depth_views[0],
                &self.sampler,
                &self.params,
                &self.quarter_history_views[1],
            ),
            make_temporal_group(
                device,
                &self.temporal_layout,
                &self.quarter_views[0],
                &self.depth_views[0],
                &self.sampler,
                &self.params,
                &self.quarter_history_views[0],
            ),
        ]);
        self.upsample_groups = Some([
            make_upsample_group(
                device,
                &self.upsample_layout,
                &self.quarter_history_views[0],
                &self.depth_views[0],
                &self.sampler,
                &self.params,
                &self.ping_views[0],
            ),
            make_upsample_group(
                device,
                &self.upsample_layout,
                &self.quarter_history_views[1],
                &self.depth_views[0],
                &self.sampler,
                &self.params,
                &self.ping_views[0],
            ),
        ]);
        self.quarter_history_read_groups = Some([
            make_effect_group(
                device,
                &self.effect_layout,
                &self.quarter_history_views[1],
                &self.depth_views[0],
                &self.sampler,
                &self.params,
            ),
            make_effect_group(
                device,
                &self.effect_layout,
                &self.quarter_history_views[0],
                &self.depth_views[0],
                &self.sampler,
                &self.params,
            ),
        ]);
        self.active = None;
        self.quarter_read = 0;
    }

    /// Use the real GPU clipmap result as the first GI-stage input.
    pub fn prepare_gi_source(
        &mut self,
        device: &wgpu::Device,
        scene: &wgpu::TextureView,
        gi: &wgpu::TextureView,
    ) {
        self.gi_group = Some(make_gi_composite_group(
            device,
            &self.gi_layout,
            scene,
            gi,
            &self.sampler,
            &self.params,
        ));
    }
    pub fn begin_frame(&mut self) {
        self.active = None;
        self.quarter_read ^= 1;
    }
    pub fn record_quarter_atmospherics(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        timestamps: [Option<wgpu::RenderPassTimestampWrites<'_>>; 3],
    ) {
        let Some(groups) = self.quarter_groups.as_ref() else {
            return;
        };
        // The first atmosphere stage must consume the current scene/GI result
        // in the full-resolution ping. Reading quarter_read here would sample
        // an uninitialized transient texture and erase scene geometry.
        let full_group = self
            .active
            .and_then(|index| self.ping_groups.as_ref()?.get(index))
            .or(self.source_group.as_ref());
        let mut input = self.quarter_read;
        for (stage_index, (stage, label, timestamp)) in [
            (
                EffectStage::Volumetric,
                "vf/m8/quarter-volumetric",
                timestamps[0].clone(),
            ),
            (
                EffectStage::Clouds,
                "vf/m8/quarter-clouds",
                timestamps[1].clone(),
            ),
            (
                EffectStage::CloudShadow,
                "vf/m8/quarter-cloud-shadow",
                timestamps[2].clone(),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let output = input ^ 1;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.quarter_views[output],
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: timestamp,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.effect_pipelines[stage.index()]);
            if stage_index == 0 {
                let Some(full_group) = full_group else {
                    return;
                };
                pass.set_bind_group(0, full_group, &[]);
            } else {
                pass.set_bind_group(0, &groups[input], &[]);
            }
            pass.draw(0..3, 0..1);
            drop(pass);
            input = output;
        }
        // Reproject the quarter result into the persistent history before the
        // full-resolution reconstruction. The next frame reads the opposite
        // history texture, so this is a real ping-pong temporal path.
        let Some(temporal_groups) = self.quarter_temporal_groups.as_ref() else {
            return;
        };
        let history_target = self.quarter_read;
        let mut temporal = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("vf/m8/quarter-temporal-history"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.quarter_history_views[history_target],
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
        temporal.set_pipeline(&self.quarter_temporal_pipeline);
        temporal.set_bind_group(0, &temporal_groups[self.quarter_read], &[]);
        temporal.draw(0..3, 0..1);
        drop(temporal);
        // One full-resolution pass remains, but it is only a filtered sample
        // of the quarter result. This is the stable graph output consumed by
        // the existing M7 post chain.
        let output = self.active.map_or(0, |index| index ^ 1);
        let Some(upsample_groups) = self.upsample_groups.as_ref() else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("vf/m8/quarter-atmospherics-upsample"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.ping_views[output],
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
        pass.set_pipeline(&self.quarter_upsample_pipeline);
        pass.set_bind_group(0, &upsample_groups[history_target], &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        self.active = Some(output);
    }
    pub fn record_depth_pyramid(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        timestamp: Option<wgpu::ComputePassTimestampWrites<'_>>,
    ) {
        if self.depth_groups.is_empty() {
            return;
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("vf/m8/depth-pyramid"),
            timestamp_writes: timestamp,
        });
        for (mip, group) in self.depth_groups.iter().enumerate() {
            let divisor = 1_u32 << mip.min(15);
            pass.set_pipeline(if mip == 0 {
                &self.depth_linear_pipeline
            } else {
                &self.depth_reduce_pipeline
            });
            pass.set_bind_group(0, group, &[]);
            pass.dispatch_workgroups(
                self.width.div_ceil(divisor).div_ceil(8),
                self.height.div_ceil(divisor).div_ceil(8),
                1,
            );
        }
    }
}

impl RuntimeEffects {
    fn record_stage(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        stage: EffectStage,
        label: &'static str,
        timestamp: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        let output = self.active.map_or(0, |index| index ^ 1);
        let group = match self.active {
            Some(index) => self
                .ping_groups
                .as_ref()
                .and_then(|groups| groups.get(index)),
            None => self.source_group.as_ref(),
        };
        let Some(group) = group else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.ping_views[output],
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: timestamp,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.effect_pipelines[stage.index()]);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        self.active = Some(output);
    }

    pub fn record_gi_trace(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        t: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        self.record_stage(encoder, EffectStage::GiTrace, "vf/m9/gi-trace", t);
    }
    pub fn record_gi_temporal(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        t: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        self.record_stage(encoder, EffectStage::GiTemporal, "vf/m9/gi-temporal", t);
    }
    pub fn record_gi_denoise(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        t: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        self.record_stage(encoder, EffectStage::GiDenoise, "vf/m9/gi-denoise", t);
    }
    pub fn record_gi_composite(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let Some(group) = self.gi_group.as_ref() else {
            self.record_stage(
                encoder,
                EffectStage::GiComposite,
                "vf/m9/gi-fallback-copy",
                None,
            );
            return;
        };
        let output = self.active.map_or(0, |index| index ^ 1);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("vf/m9/gi-composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.ping_views[output],
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
        pass.set_pipeline(&self.gi_pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        self.active = Some(output);
    }
    pub fn record_volumetric(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        t: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        self.record_stage(encoder, EffectStage::Volumetric, "vf/m8/volumetric", t);
    }
    pub fn record_clouds(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        t: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        self.record_stage(encoder, EffectStage::Clouds, "vf/m8/clouds", t);
    }
    pub fn record_cloud_shadow(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        t: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        self.record_stage(encoder, EffectStage::CloudShadow, "vf/m8/cloud-shadow", t);
    }
}
