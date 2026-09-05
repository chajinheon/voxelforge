//! M7-M9 frame-graph wiring kept out of the main renderer module.

use super::{FRAME_FORMAT, RenderView, Renderer};
use crate::render::globals::Globals;
use crate::render::gpu_timing::TimedPass;
use crate::render::m7_effects::{M7EffectInputs, M7EffectParams, M7Effects};
use crate::render::runtime_effects::{RuntimeEffectParams, RuntimeEffects};

impl Renderer {
    pub(super) fn prepare_effect_graph(
        &mut self,
        internal: (u32, u32),
        native: (u32, u32),
        globals: &Globals,
    ) {
        let (Some(gbuffer), Some(frame)) = (self.gbuffer.as_ref(), self.frame_targets.as_ref())
        else {
            return;
        };
        // The graph is built only on first use or after a resize/preset change;
        // this aggregate event is intentionally outside the steady frame path.
        self.allocation_stats.note_gpu_resource_creation();
        let mut runtime = RuntimeEffects::new(&self.device, internal.0, internal.1, FRAME_FORMAT);
        runtime.prepare(&self.device, &frame.color_view, &gbuffer.depth_view);
        if let Some(gpu_gi) = self.gpu_gi.as_ref() {
            runtime.prepare_gi_source(&self.device, &frame.color_view, gpu_gi.output_view());
        }
        self.water.prepare_scene(
            &self.device,
            &frame.color_view,
            &gbuffer.depth_view,
            &gbuffer.depth,
            runtime.dimensions(),
            runtime.depth_pyramid_view(),
            runtime.sampler(),
        );
        let mut m7 = M7Effects::new(&self.device, &self.queue, internal, native, FRAME_FORMAT);
        let scene_bindings = self.chunks.scene_bindings();
        let m8 = super::super::m8_graph::M8Graph::new(
            &self.device,
            &self.queue,
            &scene_bindings.globals_buffer,
            internal.0,
            internal.1,
            runtime.m8_scene_view(),
            runtime.depth_pyramid_view(),
            &gbuffer.normal_view,
            self.shadow.view(),
        );
        m7.prepare(
            &self.device,
            M7EffectInputs {
                hdr: runtime.full_graph_view(),
                depth: &gbuffer.depth_view,
                normal: &gbuffer.normal_view,
                material: &gbuffer.light_view,
                motion: &gbuffer.motion_view,
                reactive: &gbuffer.reactive_view,
                sky: &scene_bindings.sky_cubemap_view,
                inverse_view_proj: glam::Mat4::from_cols_array_2d(&globals.unjittered_view_proj)
                    .inverse()
                    .to_cols_array_2d(),
                camera_pos: [globals.cam_pos[0], globals.cam_pos[1], globals.cam_pos[2]],
                viewport: [gbuffer.width as f32, gbuffer.height as f32],
            },
        );
        m7.update(&self.queue, M7EffectParams::default());
        self.final_pass.prepare_pair(
            &self.device,
            &self.queue,
            m7.history_views(),
            native,
            1.0,
            self.sharpen,
        );
        if self
            .ui_blur
            .as_ref()
            .is_none_or(|blur| blur.dimensions() != native)
        {
            self.ui_blur = Some(super::super::ui_blur::UiBlur::new(
                &self.device,
                native.0,
                native.1,
                self.color_format,
            ));
        }
        self.runtime_effects = Some(runtime);
        self.m8_graph = Some(m8);
        self.m7_effects = Some(m7);
    }

    pub(super) fn encode_depth_and_gtao(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if let Some(runtime) = self.runtime_effects.as_ref() {
            runtime.record_depth_pyramid(
                encoder,
                self.gpu_timing.compute_writes(TimedPass::DepthPyramid),
            );
        }
        if let Some(m7) = self.m7_effects.as_mut() {
            m7.encode_gtao(encoder, self.gpu_timing.writes(TimedPass::Gtao));
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn encode_runtime_effects(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        time: f32,
        view: RenderView,
        camera: glam::Vec3,
        reset_history: bool,
        sun_height: f32,
        inverse_view_proj: glam::Mat4,
        previous_view_proj: glam::Mat4,
    ) {
        let Some(runtime) = self.runtime_effects.as_mut() else {
            return;
        };
        let debug_view = match view {
            RenderView::Water => 1.0,
            RenderView::Volumetric => 2.0,
            RenderView::Cloud => 3.0,
            RenderView::Lod => 4.0,
            RenderView::Gi => 5.0,
            RenderView::Clipmap => 6.0,
            _ => 0.0,
        };
        runtime.update(
            &self.queue,
            RuntimeEffectParams {
                time,
                water: 1.0,
                clouds: 1.0,
                volumetric: 1.0,
                gi: 0.65,
                gi_mode: u8::from(self.gi_enabled) as f32,
                exposure: self.exposure,
                debug_view,
                underwater: f32::from(self.underwater),
                _padding: [0.0; 3],
                quality0: [
                    self.volumetric_steps as f32,
                    self.cloud_view_steps as f32,
                    self.cloud_light_steps as f32,
                    0.0,
                ],
            },
        );
        runtime.begin_frame();
        runtime.record_gi_composite(encoder);
        if let Some(m8) = self.m8_graph.as_mut() {
            m8.update_local_lights(&self.queue, &self.m8_local_lights);
            let shadow_due =
                m8.cloud_shadow_will_update(glam::Vec2::new(camera.x, camera.z), sun_height);
            m8.record(
                encoder,
                &self.queue,
                time,
                glam::Vec2::new(camera.x, camera.z),
                self.underwater,
                sun_height,
                [
                    self.gpu_timing.compute_writes(TimedPass::Volumetric),
                    self.gpu_timing.compute_writes(TimedPass::Clouds),
                    None,
                ],
                if shadow_due {
                    self.gpu_timing.writes(TimedPass::CloudShadow)
                } else {
                    None
                },
                inverse_view_proj,
                previous_view_proj,
                debug_view as u32,
                reset_history,
            );
            // Ping one is the stable HDR graph output consumed by M7.
            m8.record_composite(encoder, runtime.full_graph_view());
        } else {
            runtime.record_quarter_atmospherics(
                encoder,
                [
                    self.gpu_timing.writes(TimedPass::Volumetric),
                    self.gpu_timing.writes(TimedPass::Clouds),
                    self.gpu_timing.writes(TimedPass::CloudShadow),
                ],
            );
        }
    }

    pub(super) fn encode_m7_post(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: RenderView,
        phase: f32,
        sun_mu: f32,
    ) {
        let Some(m7) = self.m7_effects.as_mut() else {
            return;
        };
        m7.update(
            &self.queue,
            M7EffectParams {
                exposure: self.exposure,
                atmosphere: if matches!(view, RenderView::Gi | RenderView::Clipmap) {
                    0.0
                } else {
                    1.0
                },
                history: if self.taa_enabled { 1.0 } else { 0.0 },
                phase,
                sun_mu,
                auto_exposure: f32::from(self.auto_exposure),
                ..M7EffectParams::default()
            },
        );
        // Exposure is a persistent HDR pre-pass. Its output feeds atmosphere,
        // keeping the timestamp distinct from the bloom and reconstruction work.
        self.gpu_timing
            .begin_encoder_span(encoder, TimedPass::Exposure);
        m7.encode_exposure(encoder, None);
        self.gpu_timing
            .end_encoder_span(encoder, TimedPass::Exposure);
        let sky_cubemap = &self.chunks.scene_bindings().sky_cubemap;
        self.gpu_timing
            .begin_encoder_span(encoder, TimedPass::Atmosphere);
        m7.encode_atmosphere(encoder, None, sky_cubemap);
        self.gpu_timing
            .end_encoder_span(encoder, TimedPass::Atmosphere);
        self.gpu_timing
            .begin_encoder_span(encoder, TimedPass::Bloom);
        m7.encode_bloom(encoder, None);
        self.gpu_timing.end_encoder_span(encoder, TimedPass::Bloom);
        self.gpu_timing
            .begin_encoder_span(encoder, TimedPass::TaauTonemap);
        m7.encode_taau(encoder, None);
        self.gpu_timing
            .end_encoder_span(encoder, TimedPass::TaauTonemap);
    }

    pub(super) fn encode_ui(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        mode: super::super::ui::UiMode,
    ) {
        let draw_viewmodel = matches!(mode, super::super::ui::UiMode::Hud);
        for (label, timing, viewmodel) in [
            ("vf/m10/viewmodel", TimedPass::Viewmodel, true),
            ("vf/m10/ui", TimedPass::Ui, false),
        ] {
            if viewmodel && !draw_viewmodel {
                continue;
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: if viewmodel {
                            wgpu::LoadOp::Clear(1.0)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: self.gpu_timing.writes(timing),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if viewmodel {
                self.ui_renderer.draw_viewmodel(&mut pass);
            } else {
                self.ui_renderer.draw_ui(&mut pass);
            }
        }
    }

    pub(super) fn encode_present_and_modal_blur(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
        ui: Option<&super::super::ui::UiFrame<'_>>,
    ) {
        let modal = ui.is_some_and(|frame| {
            matches!(
                frame.mode,
                super::super::ui::UiMode::Inventory
                    | super::super::ui::UiMode::Pause
                    | super::super::ui::UiMode::Settings
            )
        });
        let present_target = if modal {
            self.ui_blur
                .as_ref()
                .map_or(output, super::super::ui_blur::UiBlur::scene_view)
        } else {
            output
        };
        self.gpu_timing
            .begin_encoder_span(encoder, TimedPass::Present);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vf/m7/present-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: present_target,
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
            let source = self
                .m7_effects
                .as_ref()
                .map_or(0, super::super::m7_effects::M7Effects::history_index);
            self.final_pass.draw_index(&mut pass, source);
        }
        if modal && let Some(blur) = self.ui_blur.as_ref() {
            blur.encode(encoder, output);
        }
        self.gpu_timing
            .end_encoder_span(encoder, TimedPass::Present);
    }
}
