//! Mouse, number-key, and text interaction for the creative inventory.

use voxelforge::audio::{Effect, VoiceKind, event_seed, generate};
use voxelforge::ui::{
    ActionRequest, DesignRect, InventoryEvent, InventoryInput, ItemCategory, UiLayout, UiScale,
};

use super::App;

impl App {
    pub(super) fn handle_number(&mut self, number: u8) {
        let event = self
            .inventory
            .handle_input(InventoryInput::Number(number), &self.ui_records);
        self.apply_inventory_event(event);
    }

    pub(super) fn handle_inventory_text(&mut self, text: &str) {
        if !self.inventory.open || !self.inventory.search_focused {
            return;
        }
        for character in text.chars().filter(|character| character.is_ascii()) {
            let _ = self
                .inventory
                .handle_input(InventoryInput::Text(character), &self.ui_records);
        }
        self.inventory.clamp_scroll(&self.ui_records);
    }

    pub(super) fn handle_inventory_backspace(&mut self) {
        if !self.inventory.open || !self.inventory.search_focused {
            return;
        }
        self.inventory
            .handle_input(InventoryInput::Backspace, &self.ui_records);
        self.inventory.clamp_scroll(&self.ui_records);
    }

    pub(super) fn handle_inventory_click(&mut self) {
        if !self.inventory.open {
            return;
        }
        let (width, height) = self
            .gpu
            .as_ref()
            .map_or((1280, 720), |gpu| (gpu.config.width, gpu.config.height));
        let scale = UiScale {
            output_width: width,
            output_height: height,
            settings_scale: self.settings.ui.scale,
        };
        let layout = UiLayout::default();
        let point = (self.cursor_position.0 as f32, self.cursor_position.1 as f32);
        if contains(scale.to_physical(layout.inventory_search), point) {
            self.inventory.search_focused = true;
            return;
        }
        let category = scale.to_physical(layout.inventory_category_column);
        let stride = category.height + 6.0 * scale.design_scale();
        for row in 0..8 {
            let button = DesignRect::new(
                category.x,
                category.y + row as f32 * stride,
                category.width,
                category.height,
            );
            if contains(button, point) {
                self.inventory.category = category_for_row(row);
                self.inventory.scroll_row = 0;
                self.inventory.search_focused = false;
                return;
            }
        }
        let grid = scale.to_physical(layout.inventory_grid);
        let cell = 66.0 * scale.design_scale();
        let stride = cell + 6.0 * scale.design_scale();
        if contains(grid, point) {
            let column = ((point.0 - grid.x) / stride).floor() as usize;
            let row = ((point.1 - grid.y) / stride).floor() as usize;
            if column < 9 && row < 6 {
                let visible = self.inventory.visible_items(&self.ui_records);
                if let Some(item) = visible.get(row * 9 + column) {
                    self.inventory.hovered = Some(item.id);
                    let event = self
                        .inventory
                        .handle_input(InventoryInput::ClickItem(item.id), &self.ui_records);
                    self.apply_inventory_event(event);
                }
            }
            return;
        }
        let hotbar = scale.to_physical(voxelforge::ui::HudLayout::at_1280x720(0).hotbar);
        if contains(hotbar, point) {
            let slot = ((point.0 - hotbar.x) / (hotbar.width / 9.0)).floor() as u8;
            let event = self
                .inventory
                .handle_input(InventoryInput::ClickHotbar(slot.min(8)), &self.ui_records);
            self.apply_inventory_event(event);
        }
    }

    fn apply_inventory_event(&mut self, event: Option<InventoryEvent>) {
        let Some(event) = event else { return };
        if let InventoryEvent::Switch { from, to, .. } = event {
            self.interaction_id = self.interaction_id.wrapping_add(1).max(1);
            self.viewmodel
                .start_action_with_id(ActionRequest::Switch { from, to }, self.interaction_id);
            let sound = generate(
                Effect::Switch,
                event_seed(self.world.seed(), self.interaction_id, to),
            );
            let _ = self
                .audio
                .play(&sound, 0.0, VoiceKind::Other, u64::from(self.frames));
        }
    }
}

fn contains(rect: DesignRect, point: (f32, f32)) -> bool {
    point.0 >= rect.x
        && point.0 < rect.x + rect.width
        && point.1 >= rect.y
        && point.1 < rect.y + rect.height
}

fn category_for_row(row: usize) -> Option<ItemCategory> {
    match row {
        0 => None,
        1 => Some(ItemCategory::Terrain),
        2 => Some(ItemCategory::Masonry),
        3 => Some(ItemCategory::Wood),
        4 => Some(ItemCategory::Color),
        5 => Some(ItemCategory::GlassLight),
        6 => Some(ItemCategory::Detail),
        7 => Some(ItemCategory::Shapes),
        _ => None,
    }
}
