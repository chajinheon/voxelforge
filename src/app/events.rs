//! Window event handling for the Voxelforge app.

use std::time::Instant;

use glam::Vec3;
use voxelforge::config::InputBinding;
use voxelforge::player::camera::EYE_HEIGHT;
use voxelforge::player::step;
use voxelforge::render::Renderer;
use voxelforge::render::ui::{UiFrame, UiMode, UiSettingsDisplay, ViewModelLighting};
use voxelforge::world::chunk::{block_light, sky_light};
use voxelforge::world::raycast::raycast;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::KeyCode;

use super::App;
use super::benchmark::benchmark_phase;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EscapeTarget {
    Inventory,
    OpenPause,
    CloseSettings,
    UnlockCursor,
    Resume,
}

pub(crate) fn escape_target(
    inventory_open: bool,
    settings_open: bool,
    paused: bool,
    cursor_locked: bool,
) -> EscapeTarget {
    if inventory_open {
        EscapeTarget::Inventory
    } else if settings_open {
        EscapeTarget::CloseSettings
    } else if !paused && !cursor_locked {
        EscapeTarget::OpenPause
    } else if cursor_locked {
        EscapeTarget::UnlockCursor
    } else {
        EscapeTarget::Resume
    }
}

impl App {
    pub(super) fn apply_runtime_settings(&mut self) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        renderer.configure_preset(renderer.preset(), Some(self.settings.video.render_scale));
        renderer.configure_gi(self.settings.video.gi_enabled);
        renderer.configure_render_quality(
            self.settings.video.taa,
            self.settings.video.sharpen,
            self.settings.video.gi_rays,
            self.settings.video.gi_distance,
            self.settings.video.volumetric_steps,
            self.settings.video.cloud_view_steps,
            self.settings.video.cloud_light_steps,
            self.settings.video.ssr_steps,
            self.settings.video.shadow_resolution,
            self.settings.video.shadow_distance,
            self.settings.video.pom_steps,
        );
    }

    pub(crate) fn key_action(&self, code: KeyCode) -> Option<String> {
        self.settings
            .controls
            .bindings
            .iter()
            .find_map(|(action, binding)| match binding {
                InputBinding::Key(bound) if *bound == code => Some(action.clone()),
                _ => None,
            })
    }

    pub(crate) fn frame(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        self.autopilot_step();
        let previous_pos = self.body.pos;
        let input = self.controller.update(&mut self.camera);
        self.clock.update(dt);
        self.viewmodel
            .advance(dt, 0.0, false, false, self.paused || self.inventory.open);
        self.tick_edit_repeat(dt);
        if !self.inventory.open && !self.paused {
            step(&self.world, &mut self.body, input, dt);
            self.camera.pos = self.body.pos + Vec3::Y * EYE_HEIGHT;
        }
        self.emit_movement_audio(previous_pos, input.sprint);
        self.stream();
        if self.settled_seconds.is_none() && self.streamer.queue_len() == 0 {
            self.settled_seconds = Some(f64::from(self.clock.seconds()));
            log::info!("autopilot: settled seconds={:.3}", self.clock.seconds());
        }
        self.ray_hit = raycast(&self.world, self.camera.pos, self.camera.forward(), 6.0);
        let globals = self.globals();
        let head_light = self.world.get_light(self.camera.pos.floor().as_ivec3());
        let ui = UiFrame {
            width: self.gpu.as_ref().map_or(1, |gpu| gpu.config.width),
            height: self.gpu.as_ref().map_or(1, |gpu| gpu.config.height),
            scale: self.settings.ui.scale,
            mode: if self.inventory.open {
                UiMode::Inventory
            } else if self.settings_open {
                UiMode::Settings
            } else if self.paused {
                UiMode::Pause
            } else {
                UiMode::Hud
            },
            inventory: &self.inventory,
            records: &self.ui_records,
            viewmodel: Some(&self.viewmodel),
            viewmodel_lighting: ViewModelLighting {
                player_head_sky: sky_light(head_light),
                player_head_block: block_light(head_light),
                sun_factor: globals.sun_dir[3],
                sky_color: [
                    globals.sky_color[0],
                    globals.sky_color[1],
                    globals.sky_color[2],
                ],
                sun_dir: [globals.sun_dir[0], globals.sun_dir[1], globals.sun_dir[2]],
            },
            settings: self.settings_open.then_some(UiSettingsDisplay {
                render_scale: self.settings.video.render_scale,
                taa: self.settings.video.taa,
                gi_enabled: self.settings.video.gi_enabled,
                ui_scale: self.settings.ui.scale,
            }),
        };
        let rendered = match (self.gpu.as_mut(), self.renderer.as_mut()) {
            (Some(gpu), Some(renderer)) => {
                renderer.set_underwater(self.body.in_water);
                gpu.render(
                    renderer,
                    &globals,
                    &self.chunks,
                    self.ray_hit.map(|hit| hit.block),
                    self.render_view,
                    &ui,
                )
            }
            _ => None,
        };
        let frame_ms = now.elapsed().as_secs_f64() * 1000.0;
        // The first 600 rendered frames are the prescribed warmup window.
        if self.benchmark && self.frames >= 600 {
            self.frame_times_ms.push(frame_ms);
            if let Some(phase) = benchmark_phase(u64::from(self.frames)) {
                self.frame_phase_times_ms[phase].push(frame_ms);
            }
            if let Some(timing) = self
                .renderer
                .as_mut()
                .and_then(Renderer::take_last_gpu_timings)
                .filter(|timing| timing.rendered_frame_index() >= 600)
            {
                self.gpu_times_ms.push(timing.total_ms());
                if let Some(phase) = benchmark_phase(timing.rendered_frame_index()) {
                    self.gpu_phase_times_ms[phase].push(timing.total_ms());
                }
            }
            if self.allocation_stats_enabled
                && let Some(renderer) = self.renderer.as_ref()
            {
                self.allocation_samples
                    .push(renderer.last_allocation_stats().total());
            }
        }
        self.title_max_ms = self.title_max_ms.max(frame_ms);
        if let Some(drawn) = rendered {
            self.drawn_chunks = drawn;
            self.title_frames = self.title_frames.saturating_add(1);
            self.frames = self.frames.saturating_add(1);
            if !self.benchmark && self.smoke_frames.is_some_and(|limit| self.frames >= limit) {
                self.smoke_frames = None;
                log::info!("smoke: rendered {} frames, exiting", self.frames);
                self.save();
                event_loop.exit();
                return;
            }
            if self.benchmark && self.smoke_frames.is_some_and(|limit| self.frames >= limit) {
                self.finish_benchmark(event_loop);
                return;
            }
        }
        self.update_title();
    }
}

#[cfg(test)]
mod tests {
    use super::{EscapeTarget, escape_target};

    #[test]
    fn escape_closes_settings_before_resuming_world() {
        assert_eq!(
            escape_target(false, true, true, false),
            EscapeTarget::CloseSettings
        );
        assert_eq!(
            escape_target(false, false, true, false),
            EscapeTarget::Resume
        );
    }

    #[test]
    fn escape_opens_pause_only_when_cursor_is_free() {
        assert_eq!(
            escape_target(false, false, false, false),
            EscapeTarget::OpenPause
        );
        assert_eq!(
            escape_target(false, false, false, true),
            EscapeTarget::UnlockCursor
        );
    }
}
