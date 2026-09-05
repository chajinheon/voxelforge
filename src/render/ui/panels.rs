//! Menu and first-person viewmodel geometry.

use super::{UiSettingsDisplay, UiVertex, outline, rect};
use crate::ui::DesignRect;

pub(super) fn draw_menu(v: &mut Vec<UiVertex>, w: u32, h: u32) {
    rect(
        v,
        w,
        h,
        DesignRect::new(0.0, 0.0, w as f32, h as f32),
        [0.0, 0.0, 0.0, 0.46],
    );
    let panel = DesignRect::new(
        w as f32 * 0.33,
        h as f32 * 0.30,
        w as f32 * 0.34,
        h as f32 * 0.36,
    );
    rect(v, w, h, panel, [0.07, 0.09, 0.13, 0.96]);
    outline(v, w, h, panel, [0.38, 0.46, 0.58, 1.0], 2.0);
    super::text::draw(
        v,
        (w, h),
        (panel.x + panel.width * 0.39, panel.y + 28.0),
        "PAUSED",
        2.0,
        [0.92, 0.95, 1.0, 1.0],
    );
    for row in 0..3 {
        rect(
            v,
            w,
            h,
            DesignRect::new(
                panel.x + 26.0,
                panel.y + 76.0 + row as f32 * 54.0,
                panel.width - 52.0,
                34.0,
            ),
            [0.15, 0.2, 0.28, 1.0],
        );
        super::text::draw(
            v,
            (w, h),
            (panel.x + 42.0, panel.y + 87.0 + row as f32 * 54.0),
            ["RESUME", "SETTINGS", "QUIT"][row],
            1.5,
            [0.84, 0.89, 0.96, 1.0],
        );
    }
}

pub(super) fn draw_settings(
    v: &mut Vec<UiVertex>,
    w: u32,
    h: u32,
    values: Option<UiSettingsDisplay>,
) {
    let values = values.unwrap_or(UiSettingsDisplay {
        render_scale: 0.72,
        taa: true,
        gi_enabled: true,
        ui_scale: 1.0,
    });
    rect(
        v,
        w,
        h,
        DesignRect::new(0.0, 0.0, w as f32, h as f32),
        [0.0, 0.0, 0.0, 0.46],
    );
    let panel = DesignRect::new(
        w as f32 * 0.27,
        h as f32 * 0.18,
        w as f32 * 0.46,
        h as f32 * 0.64,
    );
    rect(v, w, h, panel, [0.07, 0.09, 0.13, 0.96]);
    outline(v, w, h, panel, [0.38, 0.46, 0.58, 1.0], 2.0);
    super::text::draw(
        v,
        (w, h),
        (panel.x + 34.0, panel.y + 34.0),
        "SETTINGS",
        2.0,
        [0.92, 0.95, 1.0, 1.0],
    );
    let rows = [
        format!("RENDER SCALE  {:.2}", values.render_scale),
        format!("TAA           {}", if values.taa { "ON" } else { "OFF" }),
        format!(
            "GI            {}",
            if values.gi_enabled { "ON" } else { "OFF" }
        ),
        format!("UI SCALE      {:.2}", values.ui_scale),
        "BACK".to_string(),
    ];
    for (row, label) in rows.iter().enumerate() {
        let y = panel.y + 82.0 + row as f32 * 46.0;
        rect(
            v,
            w,
            h,
            DesignRect::new(panel.x + 26.0, y - 8.0, panel.width - 52.0, 34.0),
            [0.15, 0.2, 0.28, 1.0],
        );
        super::text::draw(
            v,
            (w, h),
            (panel.x + 42.0, y),
            label,
            1.35,
            [0.84, 0.89, 0.96, 1.0],
        );
    }
}
