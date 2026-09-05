//! Winit event-loop integration for the Voxelforge app.

use std::sync::Arc;
use std::time::Instant;

use voxelforge::config::InputBinding;
use voxelforge::render::Renderer;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use super::events::{EscapeTarget, escape_target};
use super::{App, WINDOW_HEIGHT, WINDOW_WIDTH};
use crate::window_gpu::WindowGpu;

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let size = if self.benchmark {
            winit::dpi::Size::Physical(winit::dpi::PhysicalSize::new(2560, 1440))
        } else {
            winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
        };
        let attributes = Window::default_attributes()
            .with_title("voxelforge | starting")
            .with_inner_size(size);
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                log::error!("create window failed: {error}");
                event_loop.exit();
                return;
            }
        };
        let gpu = match WindowGpu::new(window, self.settings.video.vsync) {
            Ok(gpu) => gpu,
            Err(error) => {
                log::error!("initialize renderer failed: {error:#}");
                event_loop.exit();
                return;
            }
        };
        let renderer = match Renderer::new(&gpu.device, &gpu.queue, gpu.config.format) {
            Ok(renderer) => renderer,
            Err(error) => {
                log::error!("initialize renderer failed: {error:#}");
                event_loop.exit();
                return;
            }
        };
        let mut renderer = renderer;
        // The pack is compiled into the affected pipeline bundle only after
        // the builtin renderer exists, so any failure leaves builtin active.
        if let Some(pack) = self.shader_pack.take() {
            match renderer.apply_shader_pack(&pack) {
                Ok(true) => log::info!("shader pack active: {}", pack.manifest().name),
                Ok(false) => log::info!(
                    "shader pack active with builtin chunk shader: {}",
                    pack.manifest().name
                ),
                Err(error) => log::warn!(
                    "shader pack {} rejected: {error}; using builtin",
                    pack.manifest().name
                ),
            }
        }
        renderer.configure_gpu_gi(&gpu.adapter, gpu.config.width, gpu.config.height);
        let configured_preset = match self.settings.video.preset {
            voxelforge::config::PerformancePreset::Performance => {
                voxelforge::render::RenderPreset::Performance
            }
            voxelforge::config::PerformancePreset::Balanced => {
                voxelforge::render::RenderPreset::Balanced
            }
            voxelforge::config::PerformancePreset::Quality
            | voxelforge::config::PerformancePreset::M5AirHigh
            | voxelforge::config::PerformancePreset::Custom => {
                voxelforge::render::RenderPreset::M5AirHigh
            }
        };
        renderer.configure_preset(configured_preset, Some(self.settings.video.render_scale));
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
        if let Ok(name) = std::env::var("VF_PRESET") {
            match voxelforge::render::RenderPreset::parse(&name) {
                Ok(preset) => renderer.configure_preset(preset, None),
                Err(error) => log::warn!("render preset rejected: {error}; using m5_air_high"),
            }
        }
        renderer.configure_gpu_timing(self.benchmark);
        self.gpu = Some(gpu);
        self.renderer = Some(renderer);
        let center = self.center_chunk();
        let Some(renderer) = self.renderer.as_mut() else {
            log::error!("renderer missing immediately after initialization");
            event_loop.exit();
            return;
        };
        self.streamer.bootstrap(
            &mut self.world,
            renderer,
            &mut self.chunks,
            center,
            self.camera.pos,
        );
        self.last_frame = Instant::now();
        if let Some(gpu) = self.gpu.as_ref() {
            gpu.window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.save();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(size.width, size.height);
                }
            }
            WindowEvent::Focused(false) => self.unlock_cursor(),
            WindowEvent::MouseInput { state, button, .. } => {
                if self.inventory.open {
                    if state == ElementState::Pressed && button == winit::event::MouseButton::Left {
                        self.handle_inventory_click();
                    }
                    return;
                }
                if state == ElementState::Pressed && !self.cursor_locked && self.paused {
                    let (width, height) = self.gpu.as_ref().map_or((1280.0, 720.0), |gpu| {
                        (gpu.config.width as f32, gpu.config.height as f32)
                    });
                    let point = (self.cursor_position.0 as f32, self.cursor_position.1 as f32);
                    if self.settings_open {
                        let panel_x = width * 0.27;
                        let panel_y = height * 0.18;
                        if point.0 >= panel_x
                            && point.0 < width * 0.73
                            && point.1 >= panel_y + 74.0
                            && point.1 < panel_y + 82.0 + 5.0 * 46.0
                        {
                            let row = ((point.1 - (panel_y + 74.0)) / 46.0).floor() as usize;
                            self.settings_row = row.min(4);
                            if button == winit::event::MouseButton::Left {
                                if self.settings_row == 4 {
                                    self.close_settings();
                                } else {
                                    self.adjust_settings(1.0);
                                }
                            }
                        }
                    } else if button == winit::event::MouseButton::Left {
                        let panel_x = width * 0.33;
                        let panel_y = height * 0.30;
                        if point.0 >= panel_x
                            && point.0 < width * 0.67
                            && point.1 >= panel_y + 76.0
                            && point.1 < panel_y + 76.0 + 3.0 * 54.0
                        {
                            let row = ((point.1 - (panel_y + 76.0)) / 54.0).floor() as usize;
                            match row {
                                0 => {
                                    self.paused = false;
                                    self.clock.set_paused(false);
                                }
                                1 => self.open_settings(),
                                2 => {
                                    self.save();
                                    event_loop.exit();
                                }
                                _ => {}
                            }
                        }
                    }
                    return;
                }
                match button {
                    winit::event::MouseButton::Left => {
                        self.lmb_down = state == ElementState::Pressed
                    }
                    winit::event::MouseButton::Right => {
                        self.rmb_down = state == ElementState::Pressed
                    }
                    _ => {}
                }
                if state != ElementState::Pressed {
                    return;
                }
                if !self.cursor_locked {
                    self.lock_cursor();
                } else if matches!(
                    self.settings.controls.bindings.get("pick_block"),
                    Some(InputBinding::Mouse(bound)) if *bound == button
                ) {
                    self.pick_target();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => self.scroll_hotbar(delta),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_position = (position.x, position.y);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let pressed = event.state == ElementState::Pressed;
                if matches!(code, KeyCode::ControlLeft | KeyCode::ControlRight) {
                    self.control_down = pressed;
                }
                if pressed
                    && !event.repeat
                    && code == KeyCode::KeyF
                    && self.control_down
                    && self.inventory.open
                {
                    self.inventory.handle_input(
                        voxelforge::ui::InventoryInput::SearchFocus,
                        &self.ui_records,
                    );
                    return;
                }
                if pressed && !event.repeat && code == KeyCode::Backspace && self.inventory.open {
                    self.handle_inventory_backspace();
                    return;
                }
                if code == KeyCode::Escape && pressed && !event.repeat {
                    match escape_target(
                        self.inventory.open,
                        self.settings_open,
                        self.paused,
                        self.cursor_locked,
                    ) {
                        EscapeTarget::Inventory => {
                            self.inventory
                                .handle_input(voxelforge::ui::InventoryInput::Escape, &[]);
                            self.paused = false;
                            self.clock.set_paused(false);
                            log::info!("inventory: closed");
                        }
                        EscapeTarget::CloseSettings => {
                            self.close_settings();
                        }
                        EscapeTarget::OpenPause => {
                            self.paused = true;
                            self.clock.set_paused(true);
                            log::info!("game: paused");
                        }
                        EscapeTarget::UnlockCursor => {
                            self.unlock_cursor();
                        }
                        EscapeTarget::Resume => {
                            self.paused = false;
                            self.clock.set_paused(false);
                            self.lock_cursor();
                            log::info!("game: resumed");
                        }
                    }
                    return;
                }
                if pressed && !event.repeat && self.paused {
                    match code {
                        KeyCode::ArrowUp if self.settings_open => {
                            self.settings_row = self.settings_row.saturating_sub(1);
                            return;
                        }
                        KeyCode::ArrowDown if self.settings_open => {
                            self.settings_row = (self.settings_row + 1).min(4);
                            return;
                        }
                        KeyCode::ArrowLeft if self.settings_open => {
                            self.adjust_settings(-1.0);
                            return;
                        }
                        KeyCode::ArrowRight if self.settings_open => {
                            self.adjust_settings(1.0);
                            return;
                        }
                        KeyCode::Enter if self.settings_open && self.settings_row == 4 => {
                            self.close_settings();
                            return;
                        }
                        _ => {}
                    }
                }
                let action = self.key_action(code);
                if pressed && !event.repeat {
                    match action.as_deref() {
                        Some("inventory") => self.toggle_inventory(),
                        Some("debug_view") => {
                            self.render_view = match self.render_view {
                                voxelforge::render::RenderView::Final => {
                                    voxelforge::render::RenderView::Light
                                }
                                voxelforge::render::RenderView::Light => {
                                    voxelforge::render::RenderView::Albedo
                                }
                                voxelforge::render::RenderView::Albedo => {
                                    voxelforge::render::RenderView::Normal
                                }
                                voxelforge::render::RenderView::Normal => {
                                    voxelforge::render::RenderView::Depth
                                }
                                voxelforge::render::RenderView::Depth => {
                                    voxelforge::render::RenderView::Material
                                }
                                voxelforge::render::RenderView::Material => {
                                    voxelforge::render::RenderView::Motion
                                }
                                voxelforge::render::RenderView::Motion => {
                                    voxelforge::render::RenderView::Reactive
                                }
                                voxelforge::render::RenderView::Reactive => {
                                    voxelforge::render::RenderView::Water
                                }
                                voxelforge::render::RenderView::Water => {
                                    voxelforge::render::RenderView::Volumetric
                                }
                                voxelforge::render::RenderView::Volumetric => {
                                    voxelforge::render::RenderView::Cloud
                                }
                                voxelforge::render::RenderView::Cloud => {
                                    voxelforge::render::RenderView::Lod
                                }
                                voxelforge::render::RenderView::Lod => {
                                    voxelforge::render::RenderView::Gi
                                }
                                voxelforge::render::RenderView::Gi => {
                                    voxelforge::render::RenderView::Clipmap
                                }
                                voxelforge::render::RenderView::Clipmap => {
                                    voxelforge::render::RenderView::Final
                                }
                            };
                        }
                        Some("screenshot") => {
                            if let Some(gpu) = self.gpu.as_mut() {
                                gpu.request_screenshot();
                            }
                        }
                        Some("toggle_fly") => {
                            self.body.fly = !self.body.fly;
                            if self.body.fly {
                                self.body.vel.y = 0.0;
                            }
                        }
                        Some("shader_reload") => {
                            if let Some(renderer) = self.renderer.as_mut() {
                                renderer.force_shader_reload();
                            }
                        }
                        Some("forward") | Some("backward") | Some("left") | Some("right")
                        | Some("jump") | Some("descend") | Some("sprint") | None => {
                            // Movement state is updated below for both press and release.
                        }
                        _ => {}
                    }
                    match code {
                        KeyCode::Digit1 => self.handle_number(1),
                        KeyCode::Digit2 => self.handle_number(2),
                        KeyCode::Digit3 => self.handle_number(3),
                        KeyCode::Digit4 => self.handle_number(4),
                        KeyCode::Digit5 => self.handle_number(5),
                        KeyCode::Digit6 => self.handle_number(6),
                        KeyCode::Digit7 => self.handle_number(7),
                        KeyCode::Digit8 => self.handle_number(8),
                        KeyCode::Digit9 => self.handle_number(9),
                        KeyCode::F3 => {
                            self.debug_stats = !self.debug_stats;
                            log::info!(
                                "console stats: {}",
                                if self.debug_stats { "on" } else { "off" }
                            );
                        }
                        _ => {}
                    }
                }
                if pressed
                    && self.inventory.open
                    && let Some(text) = event.text.as_deref()
                    && !matches!(
                        code,
                        KeyCode::Digit1
                            | KeyCode::Digit2
                            | KeyCode::Digit3
                            | KeyCode::Digit4
                            | KeyCode::Digit5
                            | KeyCode::Digit6
                            | KeyCode::Digit7
                            | KeyCode::Digit8
                            | KeyCode::Digit9
                    )
                {
                    self.handle_inventory_text(text);
                }
                let movement_key = match action.as_deref() {
                    Some("forward") => Some(KeyCode::KeyW),
                    Some("backward") => Some(KeyCode::KeyS),
                    Some("left") => Some(KeyCode::KeyA),
                    Some("right") => Some(KeyCode::KeyD),
                    Some("jump") => Some(KeyCode::Space),
                    Some("descend") => Some(KeyCode::ShiftLeft),
                    Some("sprint") => Some(KeyCode::ControlLeft),
                    _ => None,
                };
                if let Some(movement_key) = movement_key {
                    self.controller.set_key(movement_key, pressed);
                }
            }
            WindowEvent::RedrawRequested => self.frame(event_loop),
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if self.cursor_locked
            && let DeviceEvent::MouseMotion { delta: (dx, dy) } = event
        {
            self.controller.mouse_motion(dx, dy);
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(gpu) = self.gpu.as_ref() {
            gpu.window.request_redraw();
        }
    }
}
