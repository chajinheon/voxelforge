#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesignRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl DesignRect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiScale {
    pub output_width: u32,
    pub output_height: u32,
    pub settings_scale: f32,
}

impl UiScale {
    pub fn design_scale(self) -> f32 {
        let user_scale = self.settings_scale.clamp(0.75, 1.50);
        (self.output_height as f32 / 720.0 * user_scale)
            .round()
            .max(1.0)
    }

    pub fn design_offset(self) -> (f32, f32) {
        let scale = self.design_scale();
        (
            ((self.output_width as f32 / scale - 1280.0) / 2.0).max(0.0),
            ((self.output_height as f32 / scale - 720.0) / 2.0).max(0.0),
        )
    }

    pub fn to_physical(self, rect: DesignRect) -> DesignRect {
        let scale = self.design_scale();
        let (offset_x, offset_y) = self.design_offset();
        DesignRect::new(
            ((offset_x + rect.x) * scale).round(),
            ((offset_y + rect.y) * scale).round(),
            (rect.width * scale).round(),
            (rect.height * scale).round(),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiLayout {
    pub inventory_panel: DesignRect,
    pub inventory_search: DesignRect,
    pub inventory_category_column: DesignRect,
    pub inventory_grid: DesignRect,
    pub inventory_scrollbar: DesignRect,
    pub inventory_hotbar_y: f32,
}

impl Default for UiLayout {
    fn default() -> Self {
        Self {
            inventory_panel: DesignRect::new(174.0, 66.0, 932.0, 588.0),
            inventory_search: DesignRect::new(568.0, 82.0, 502.0, 34.0),
            inventory_category_column: DesignRect::new(198.0, 132.0, 148.0, 42.0),
            inventory_grid: DesignRect::new(
                374.0,
                132.0,
                9.0 * 66.0 + 8.0 * 6.0,
                6.0 * 66.0 + 5.0 * 6.0,
            ),
            inventory_scrollbar: DesignRect::new(1029.0, 132.0, 12.0, 426.0),
            inventory_hotbar_y: 584.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudLayout {
    pub hotbar: DesignRect,
    pub crosshair: DesignRect,
}

impl HudLayout {
    pub fn at_1280x720(selected_slot: u8) -> Self {
        let slot = 44.0;
        let gap = 4.0;
        let width = slot * 9.0 + gap * 8.0;
        let x = (1280.0 - width) / 2.0;
        let y = 720.0 - 18.0 - slot;
        let _ = selected_slot;
        Self {
            hotbar: DesignRect::new(x, y, width, slot),
            crosshair: DesignRect::new(635.5, 355.5, 9.0, 9.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotbar_is_centered_at_1280x720() {
        let layout = HudLayout::at_1280x720(0);
        assert_eq!(layout.hotbar, DesignRect::new(426.0, 658.0, 428.0, 44.0));
    }

    #[test]
    fn inventory_panel_matches_contract_bounds() {
        assert_eq!(
            UiLayout::default().inventory_panel,
            DesignRect::new(174.0, 66.0, 932.0, 588.0)
        );
    }
}
