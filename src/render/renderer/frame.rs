//! Surface-independent frame encoding.
use super::*;

impl Renderer {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_internal(
        &mut self,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        globals: &Globals,
        chunks: &[GpuChunkMeshes],
        selected_block: Option<glam::IVec3>,
        view: RenderView,
        ui: Option<&super::ui::UiFrame<'_>>,
    ) -> usize {
        self.allocation_stats.begin_frame();
        let unjittered_view_proj = glam::Mat4::from_cols_array_2d(&globals.view_proj);
        let camera_now =
            glam::Vec3::new(globals.cam_pos[0], globals.cam_pos[1], globals.cam_pos[2]);
        let teleported = self
            .previous_camera
            .is_some_and(|previous| previous.distance(camera_now) > 8.0);
        let previous_view_proj = if teleported {
            unjittered_view_proj
        } else {
            self.previous_view_proj.unwrap_or(unjittered_view_proj)
        };
        let mut frame_globals = *globals;
        frame_globals.unjittered_view_proj = unjittered_view_proj.to_cols_array_2d();
        frame_globals.previous_view_proj = previous_view_proj.to_cols_array_2d();
        let jitter = halton_2d(self.frame_index + 1);
        let mut jittered = unjittered_view_proj.to_cols_array_2d();
        jittered[2][0] += (jitter[0] - 0.5) * 2.0 / globals.time_res[1].max(1.0);
        jittered[2][1] += (jitter[1] - 0.5) * 2.0 / globals.time_res[2].max(1.0);
        frame_globals.view_proj = jittered;
        if self.watcher.poll() {
            self.reload_chunk_water_bundle();
        }
        if self.outline_watcher.poll() {
            let _ = self.outline.reload_shader(&self.device, FRAME_FORMAT);
        }
        self.chunks.update_globals(&self.queue, &frame_globals);
        let mut quality_globals = frame_globals;
        quality_globals.time_res[3] = self.pom_steps as f32;
        // The LOD inspection view is the only live debug switch for the
        // distant-terrain material; normal/final frames keep material colours.
        if view == RenderView::Lod {
            quality_globals.time_res[3] = -1.0;
        }
        quality_globals.write(&self.queue, &self.chunks.scene_bindings().globals_buffer);
        self.translucent.update_globals(&self.queue, &frame_globals);
        self.water.update_globals(&self.queue, &frame_globals);
        if let Some(ui) = ui {
            self.ui_renderer.prepare(&self.device, &self.queue, ui);
        }
        if let Some(block) = selected_block {
            self.outline.update(&self.queue, block);
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("voxelforge-frame"),
            });
        self.ui_renderer.encode_icon_bake(&mut encoder);
        let view_projection = unjittered_view_proj;
        let frustum = Frustum::from_view_proj(view_projection);
        let camera =
            glam::Vec3::from_array([globals.cam_pos[0], globals.cam_pos[1], globals.cam_pos[2]]);
        let visible_chunks = self.draw_lists.update(chunks, &frustum, camera);
        if view == RenderView::Light {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("voxelforge-opaque-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: globals.sky_color[0] as f64,
                            g: globals.sky_color[1] as f64,
                            b: globals.sky_color[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.chunks
                .draw_mesh_indices(&mut pass, chunks, &self.draw_lists.opaque, false, true);
        } else {
            let (width, height) = self.preset.internal_size_with_scale(
                globals.time_res[1].max(1.0) as u32,
                globals.time_res[2].max(1.0) as u32,
                self.render_scale
                    .unwrap_or_else(|| self.preset.render_scale()),
            );
            let needs_gbuffer = self
                .gbuffer
                .as_ref()
                .is_none_or(|target| (target.width, target.height) != (width, height));
            if needs_gbuffer {
                self.allocation_stats.note_gpu_resource_creations(6);
                self.gbuffer = Some(GBuffer::new(&self.device, width, height));
                let gi_needs_resize = self.gpu_gi.as_ref().is_some_and(|gi| {
                    gi.dispatch_dimensions() != (width.div_ceil(4), height.div_ceil(4))
                });
                if gi_needs_resize
                    && let Some(adapter) = self.gi_adapter.as_ref()
                    && let Ok(gpu_gi) = GpuGi::try_new(&self.device, adapter, width, height)
                {
                    self.gpu_gi = Some(gpu_gi);
                }
            }
            let needs_frame = self
                .frame_targets
                .as_ref()
                .is_none_or(|target| (target.width, target.height) != (width, height));
            if needs_frame {
                self.allocation_stats.note_gpu_resource_creations(2);
                self.frame_targets = Some(FrameTargets::new(&self.device, width, height));
            }
            let native = (
                globals.time_res[1].max(1.0) as u32,
                globals.time_res[2].max(1.0) as u32,
            );
            if needs_gbuffer
                || needs_frame
                || self.m7_effects.is_none()
                || self.runtime_effects.is_none()
                || self.m8_graph.is_none()
            {
                self.prepare_effect_graph((width, height), native, &frame_globals);
            }
            let (Some(gbuffer), Some(_)) = (self.gbuffer.as_ref(), self.frame_targets.as_ref())
            else {
                return visible_chunks;
            };
            if self
                .shadow_camera
                .is_none_or(|previous| previous.distance(camera) > 0.02)
            {
                self.shadow.invalidate();
                self.shadow_camera = Some(camera);
            }
            self.shadow.update(
                &self.queue,
                camera,
                glam::Vec3::from_array([
                    globals.sun_dir[0],
                    globals.sun_dir[1],
                    globals.sun_dir[2],
                ]),
            );
            self.shadow.record(
                &mut encoder,
                chunks,
                &self.draw_lists.opaque,
                &self.chunks.scene_bindings(),
                self.frame_index,
                self.gpu_timing.writes(TimedPass::Shadow),
            );
            let ao_view = self.m7_effects.as_ref().map(|effects| effects.ao_view());
            let cloud_shadow_view = self
                .m8_graph
                .as_ref()
                .map(super::m8_graph::M8Graph::cloud_shadow_view);
            if let (Some(ao_view), Some(cloud_shadow_view)) = (ao_view, cloud_shadow_view) {
                self.deferred.prepare(
                    &self.device,
                    gbuffer,
                    self.shadow.view(),
                    ao_view,
                    cloud_shadow_view,
                );
            }
            self.deferred
                .set_debug(&self.queue, super::final_pass::debug_for_view(view));
            self.deferred.update_shadow(
                &self.queue,
                glam::Mat4::from_cols_array_2d(&globals.view_proj),
                camera,
                glam::Vec3::from_array([
                    globals.sun_dir[0],
                    globals.sun_dir[1],
                    globals.sun_dir[2],
                ]),
                self.shadow.resolution(),
                self.shadow.far_distance(),
            );
            let cloud_origin = super::clouds::CloudShadowSchedule::snapped_origin(glam::Vec2::new(
                camera.x, camera.z,
            ));
            self.deferred.update_cloud_shadow(
                &self.queue,
                glam::Vec2::new(cloud_origin[0] as f32, cloud_origin[1] as f32),
                super::clouds::CLOUD_SHADOW_WORLD_SIZE,
            );
            {
                let Some(gbuffer) = self.gbuffer.as_ref() else {
                    return visible_chunks;
                };
                let attachments = gbuffer.color_attachments();
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("vf/m7/gbuffer-pass"),
                    color_attachments: &attachments,
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &gbuffer.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: self.gpu_timing.writes(TimedPass::GBuffer),
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                self.gbuffer_pipeline
                    .draw_mesh_indices(&mut pass, chunks, &self.draw_lists.opaque);
            }
            self.encode_depth_and_gtao(&mut encoder);
            {
                let Some(frame_targets) = self.frame_targets.as_ref() else {
                    return visible_chunks;
                };
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("vf/m7/deferred-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &frame_targets.color_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: self.gpu_timing.writes(TimedPass::Deferred),
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                self.deferred.draw(&mut pass);
            }
            if let Some(gpu_gi) = self.gpu_gi.as_mut() {
                if let Some(gbuffer) = self.gbuffer.as_ref() {
                    gpu_gi.set_gbuffer_views(
                        &self.device,
                        &gbuffer.depth_view,
                        &gbuffer.normal_view,
                        // G-buffer light alpha carries the stable material
                        // layer/class ID used by GI history rejection.
                        &gbuffer.light_view,
                    );
                }
                let (trace_timing, temporal_timing, denoise_timing) = if self.gi_enabled {
                    (
                        self.gpu_timing.compute_writes(TimedPass::GiTrace),
                        self.gpu_timing.compute_writes(TimedPass::GiTemporal),
                        self.gpu_timing.compute_writes(TimedPass::GiDenoise),
                    )
                } else {
                    (None, None, None)
                };
                gpu_gi.update(
                    &self.queue,
                    &self.gi_clipmap,
                    self.gi_config,
                    if self.gi_enabled {
                        GiMode::Enabled
                    } else {
                        GiMode::DisabledByUser
                    },
                    view_projection.inverse().to_cols_array_2d(),
                    view_projection.to_cols_array_2d(),
                    camera.to_array(),
                    globals.sky_color[3],
                    globals.sun_dir,
                    [
                        globals.sky_color[0],
                        globals.sky_color[1],
                        globals.sky_color[2],
                    ],
                );
                let before = self.gi_dispatches;
                gpu_gi.record(&mut encoder, trace_timing, temporal_timing, denoise_timing);
                if gpu_gi.history_valid() && self.gi_enabled {
                    self.gi_dispatches = before.saturating_add(5);
                }
            }
            self.encode_runtime_effects(
                &mut encoder,
                globals.time_res[0],
                view,
                camera,
                teleported,
                globals.sun_dir[3],
                unjittered_view_proj.inverse(),
                previous_view_proj,
            );
            {
                let Some(frame_targets) = self.frame_targets.as_ref() else {
                    return visible_chunks;
                };
                let Some(gbuffer) = self.gbuffer.as_ref() else {
                    return visible_chunks;
                };
                let forward_view = if self.m8_graph.is_some() {
                    self.runtime_effects.as_ref().map_or(
                        &frame_targets.color_view,
                        super::runtime_effects::RuntimeEffects::full_graph_view,
                    )
                } else {
                    match self
                        .runtime_effects
                        .as_ref()
                        .and_then(super::runtime_effects::RuntimeEffects::active_view)
                    {
                        Some(view) => view,
                        None => &frame_targets.color_view,
                    }
                };
                self.water.copy_depth(
                    &mut encoder,
                    &gbuffer.depth,
                    (gbuffer.width, gbuffer.height),
                );
                let has_glass = self
                    .draw_lists
                    .transparent
                    .iter()
                    .any(|draw| matches!(draw.kind, TransparentKind::Glass));
                let has_water = self
                    .draw_lists
                    .transparent
                    .iter()
                    .any(|draw| matches!(draw.kind, TransparentKind::Water));
                if has_glass {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("vf/m10/glass-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: forward_view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &gbuffer.depth_view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: self.gpu_timing.writes(TimedPass::Glass),
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    self.lod_pipeline
                        .draw_selected(&mut pass, &self.lod_meshes, camera);
                    for draw in self.draw_lists.transparent.iter() {
                        if matches!(draw.kind, TransparentKind::Glass) {
                            self.translucent.draw_mesh_index(
                                &mut pass,
                                chunks,
                                draw.chunk_index,
                                false,
                            );
                        }
                    }
                    if selected_block.is_some() {
                        self.outline.draw(&mut pass);
                    }
                }
                if has_water {
                    self.water.update(
                        &self.queue,
                        globals,
                        globals.time_res[0],
                        self.underwater,
                        (gbuffer.width, gbuffer.height),
                        self.ssr_steps,
                    );
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("vf/m10/water-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: forward_view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &gbuffer.depth_view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: self.gpu_timing.writes(TimedPass::Water),
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    for draw in self.draw_lists.transparent.iter() {
                        if matches!(draw.kind, TransparentKind::Water) {
                            self.water
                                .draw_mesh_index(&mut pass, chunks, draw.chunk_index);
                        }
                    }
                }
            }
            self.encode_m7_post(&mut encoder, view, globals.sky_color[3], globals.sun_dir[1]);
            self.encode_present_and_modal_blur(&mut encoder, color_view, ui);
            if let Some(ui_frame) = ui
                && let Some(depth) = self.ui_renderer.native_depth_view()
            {
                self.encode_ui(&mut encoder, color_view, depth, ui_frame.mode);
            }
        }
        if view != RenderView::Light {
            self.gpu_timing.resolve(&mut encoder);
        }
        self.queue.submit([encoder.finish()]);
        self.frame_index = self.frame_index.wrapping_add(1);
        self.previous_view_proj = Some(unjittered_view_proj);
        self.previous_camera = Some(camera_now);
        if view != RenderView::Light {
            self.gpu_timing.finish(&self.device, &self.queue);
        }
        self.allocation_stats.finish_frame();
        visible_chunks
    }

}
