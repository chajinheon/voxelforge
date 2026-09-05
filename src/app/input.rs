use super::App;
use voxelforge::audio::{Effect, VoiceKind, event_seed, generate};
use voxelforge::player::pick::pick_block;
use voxelforge::player::place::EditAction;
use voxelforge::player::place::place_item;
use voxelforge::player::place_rejected_inside_player_aabb;
use voxelforge::ui::ActionRequest;
use voxelforge::world::block::{AIR, WATER};
use winit::event::{MouseButton, MouseScrollDelta};
use winit::window::CursorGrabMode;

impl App {
    fn cycle_hotbar(&mut self, steps: i32) {
        let _ = self.inventory.handle_input(
            voxelforge::ui::InventoryInput::Wheel { delta: steps },
            &self.ui_records,
        );
    }

    pub(super) fn scroll_hotbar(&mut self, delta: MouseScrollDelta) {
        let amount = match delta {
            MouseScrollDelta::LineDelta(_, y) => f64::from(y),
            MouseScrollDelta::PixelDelta(position) => position.y / 40.0,
        };
        self.wheel_remainder += amount;
        let steps = self.wheel_remainder.trunc() as i32;
        if steps != 0 {
            self.wheel_remainder -= f64::from(steps);
            self.cycle_hotbar(steps);
        }
    }

    fn edit_with_button(&mut self, button: MouseButton) {
        let Some(hit) = self.ray_hit else {
            return;
        };
        let placed_id = match button {
            MouseButton::Left => self.world.set_block(hit.block, AIR).then_some(AIR),
            MouseButton::Right => {
                let target = hit.block + hit.normal;
                let target_id = self.world.get_block(target);
                if (target_id == AIR || target_id == WATER)
                    && !place_rejected_inside_player_aabb(target, self.camera.pos)
                {
                    place_item(
                        &mut self.world,
                        hit,
                        self.inventory.hotbar[self.inventory.selected_slot as usize],
                        self.camera.forward(),
                    )
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(placed_id) = placed_id {
            self.interaction_id = self.interaction_id.wrapping_add(1).max(1);
            self.viewmodel.start_action_with_id(
                if button == MouseButton::Left {
                    ActionRequest::Break
                } else {
                    ActionRequest::Place
                },
                self.interaction_id,
            );
            let effect = if button == MouseButton::Left {
                Effect::Break
            } else {
                Effect::Place {
                    pane_or_fence: matches!(placed_id, 768..=863 | 896..=959),
                }
            };
            let pcm = generate(
                effect,
                event_seed(
                    self.world.seed(),
                    self.interaction_id,
                    self.inventory.hotbar[self.inventory.selected_slot as usize],
                ),
            );
            let _ = self
                .audio
                .play(&pcm, 0.0, VoiceKind::Other, self.frames as u64);
            for cp in self.world.take_dirty() {
                self.streamer.mark_urgent(cp);
            }
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.invalidate_shadows();
                renderer.invalidate_gi_clipmap();
            }
        }
    }

    pub(super) fn tick_edit_repeat(&mut self, dt: f32) {
        if self.inventory.open || (!self.lmb_down && !self.rmb_down) {
            let _ = self.hold_repeat.tick(dt, false, false);
            return;
        }
        if let Some(action) = self.hold_repeat.tick(dt, self.lmb_down, self.rmb_down) {
            match action {
                EditAction::Break => self.edit_with_button(MouseButton::Left),
                EditAction::Place => self.edit_with_button(MouseButton::Right),
            }
        }
    }

    pub(super) fn unlock_cursor(&mut self) {
        if let Some(gpu) = self.gpu.as_ref() {
            if let Err(error) = gpu.window.set_cursor_grab(CursorGrabMode::None) {
                log::warn!("release cursor failed: {error}");
            }
            gpu.window.set_cursor_visible(true);
        }
        self.cursor_locked = false;
        self.controller.clear();
    }

    pub(super) fn lock_cursor(&mut self) {
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        let result = gpu
            .window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| gpu.window.set_cursor_grab(CursorGrabMode::Confined));
        match result {
            Ok(()) => {
                gpu.window.set_cursor_visible(false);
                self.cursor_locked = true;
            }
            Err(error) => log::warn!("grab cursor failed: {error}"),
        }
    }

    pub(super) fn pick_target(&mut self) {
        if let Some(hit) = self.ray_hit {
            let mut selected = usize::from(self.inventory.selected_slot);
            let previous = self.inventory.hotbar[selected];
            if pick_block(&mut self.inventory.hotbar, &mut selected, hit.block_id) {
                self.inventory.selected_slot = selected as u8;
                self.interaction_id = self.interaction_id.wrapping_add(1).max(1);
                self.viewmodel.start_action_with_id(
                    ActionRequest::Switch {
                        from: previous,
                        to: self.inventory.hotbar[selected],
                    },
                    self.interaction_id,
                );
                let pcm = generate(
                    Effect::Switch,
                    event_seed(self.world.seed(), self.interaction_id, 9),
                );
                let _ = self
                    .audio
                    .play(&pcm, 0.0, VoiceKind::Other, self.frames as u64);
            }
        }
    }

    pub(super) fn toggle_inventory(&mut self) {
        let event = self
            .inventory
            .handle_input(voxelforge::ui::InventoryInput::Toggle, &[]);
        self.clock.set_paused(self.inventory.open);
        self.paused = self.inventory.open;
        if self.inventory.open {
            self.unlock_cursor();
        }
        log::info!(
            "inventory: {}",
            if self.inventory.open {
                "open"
            } else {
                "closed"
            }
        );
        if event.is_some() {
            let pcm = generate(
                Effect::Inventory,
                event_seed(self.world.seed(), self.frames as u64, 7),
            );
            let _ = self
                .audio
                .play(&pcm, 0.0, VoiceKind::Other, self.frames as u64);
        }
    }

    pub(super) fn open_settings(&mut self) {
        self.settings_open = true;
        self.settings_row = 0;
        self.unlock_cursor();
        log::info!("settings: open");
    }

    pub(super) fn close_settings(&mut self) {
        self.settings_open = false;
        self.save();
        log::info!("settings: closed");
    }

    pub(super) fn adjust_settings(&mut self, direction: f32) {
        match self.settings_row {
            0 => {
                self.settings.video.render_scale =
                    (self.settings.video.render_scale + direction * 0.05).clamp(0.5, 1.0)
            }
            1 => self.settings.video.taa = direction.is_sign_positive(),
            2 => self.settings.video.gi_enabled = direction.is_sign_positive(),
            3 => {
                self.settings.ui.scale =
                    (self.settings.ui.scale + direction * 0.05).clamp(0.75, 1.5)
            }
            4 => self.close_settings(),
            _ => {}
        }
        self.apply_runtime_settings();
    }
}
