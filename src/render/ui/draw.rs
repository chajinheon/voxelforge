//! UI geometry helpers.
use super::text;
use super::{UiVertex, draw_item_icon};
use crate::ui::{CreativeInventory, DesignRect, HudLayout, ItemRecord, UiLayout, UiScale};

pub(super) fn draw_inventory(
    v: &mut Vec<UiVertex>,
    w: u32,
    h: u32,
    scale: &UiScale,
    state: &CreativeInventory,
    records: &[ItemRecord],
) {
    rect(
        v,
        w,
        h,
        DesignRect::new(0.0, 0.0, w as f32, h as f32),
        [0.01, 0.02, 0.03, 0.35],
    );
    let layout = UiLayout::default();
    let panel = scale.to_physical(layout.inventory_panel);
    rect(v, w, h, panel, [0.07, 0.09, 0.13, 0.94]);
    outline(v, w, h, panel, [0.38, 0.46, 0.58, 1.0], 2.0);
    text::draw(
        v,
        (w, h),
        (panel.x + 24.0, panel.y + 20.0),
        "CREATIVE INVENTORY",
        2.0,
        [0.88, 0.92, 0.98, 1.0],
    );
    let search = scale.to_physical(layout.inventory_search);
    rect(v, w, h, search, [0.02, 0.03, 0.05, 0.96]);
    outline(v, w, h, search, [0.58, 0.66, 0.78, 1.0], 1.0);
    let category = scale.to_physical(layout.inventory_category_column);
    let selected_category = match state.category {
        None => 0,
        Some(crate::ui::ItemCategory::Terrain) => 1,
        Some(crate::ui::ItemCategory::Masonry) => 2,
        Some(crate::ui::ItemCategory::Wood) => 3,
        Some(crate::ui::ItemCategory::Color) => 4,
        Some(crate::ui::ItemCategory::GlassLight) => 5,
        Some(crate::ui::ItemCategory::Detail) => 6,
        Some(crate::ui::ItemCategory::Shapes) => 7,
    };
    for (index, label) in [
        "ALL", "TERRAIN", "MASONRY", "WOOD", "COLOR", "GLASS", "DETAIL", "SHAPES",
    ]
    .into_iter()
    .enumerate()
    {
        let y = category.y + index as f32 * (category.height + 6.0 * scale.design_scale());
        let button = DesignRect::new(category.x, y, category.width, category.height);
        rect(
            v,
            w,
            h,
            button,
            if index == selected_category {
                [0.17, 0.24, 0.34, 1.0]
            } else {
                [0.11, 0.15, 0.21, 1.0]
            },
        );
        text::draw(
            v,
            (w, h),
            (button.x + 10.0, button.y + 13.0),
            label,
            1.5,
            [0.82, 0.87, 0.94, 1.0],
        );
    }
    let grid = scale.to_physical(layout.inventory_grid);
    let cell = 66.0 * scale.design_scale();
    let gap = 6.0 * scale.design_scale();
    for row in 0..CreativeInventory::GRID_ROWS {
        for col in 0..CreativeInventory::GRID_COLUMNS {
            let x = grid.x + col as f32 * (cell + gap);
            let y = grid.y + row as f32 * (cell + gap);
            // Keep empty cells fully opaque and uniform.  The pixel contract
            // counts variance inside each 66px cell, so an outline or a
            // translucent fill would make an empty filtered slot look used.
            box_(v, w, h, x, y, cell, cell, [0.14, 0.18, 0.24, 1.0]);
            let slot = row * CreativeInventory::GRID_COLUMNS + col;
            if let Some(item) = visible_item_at(state, records, slot) {
                draw_item_icon(v, w, h, x + cell * 0.2, y + cell * 0.2, cell * 0.6, item.id);
            }
        }
    }
    let bar = scale.to_physical(layout.inventory_scrollbar);
    rect(v, w, h, bar, [0.03, 0.04, 0.06, 0.9]);
    box_(
        v,
        w,
        h,
        bar.x + 2.0,
        bar.y + 2.0,
        bar.width - 4.0,
        54.0,
        [0.35, 0.43, 0.55, 1.0],
    );
    text::draw(
        v,
        (w, h),
        (search.x + 12.0, search.y + 10.0),
        if state.query.is_empty() {
            "SEARCH"
        } else {
            &state.query
        },
        1.5,
        [0.76, 0.82, 0.90, 1.0],
    );
}

/// Find one visible item without allocating the temporary `Vec` used by the
/// state-layer convenience API. `catalog_records` is already ID ordered, so a
/// linear filtered walk preserves the gallery's deterministic order.
fn visible_item_at<'a>(
    state: &CreativeInventory,
    records: &'a [ItemRecord],
    visible_index: usize,
) -> Option<&'a ItemRecord> {
    let start = usize::from(state.scroll_row) * CreativeInventory::GRID_COLUMNS;
    records
        .iter()
        .filter(|item| {
            state
                .category
                .is_none_or(|category| item.category == category)
                && (state.query.is_empty()
                    || contains_ascii_case_insensitive(item.name, &state.query)
                    || item
                        .search_terms
                        .iter()
                        .any(|term| contains_ascii_case_insensitive(term, &state.query)))
        })
        .nth(start + visible_index)
}

fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}
pub(super) fn draw_hud(
    v: &mut Vec<UiVertex>,
    w: u32,
    h: u32,
    scale: &UiScale,
    state: &CreativeInventory,
    show_crosshair: bool,
) {
    let hud = HudLayout::at_1280x720(state.selected_slot);
    let hotbar = scale.to_physical(hud.hotbar);
    rect(v, w, h, hotbar, [0.03, 0.04, 0.06, 0.82]);
    let slot = hotbar.width / 9.0;
    for index in 0..9 {
        let x = hotbar.x + index as f32 * slot;
        outline(
            v,
            w,
            h,
            DesignRect::new(x + 1.0, hotbar.y + 1.0, slot - 2.0, hotbar.height - 2.0),
            if index == usize::from(state.selected_slot) {
                [0.96, 0.96, 0.96, 1.0]
            } else {
                [0.34, 0.4, 0.49, 1.0]
            },
            if index == usize::from(state.selected_slot) {
                3.0
            } else {
                1.0
            },
        );
        draw_item_icon(
            v,
            w,
            h,
            x + slot * 0.28,
            hotbar.y + hotbar.height * 0.24,
            slot * 0.44,
            state.hotbar[index],
        );
    }
    if !show_crosshair {
        draw_hotbar_label(v, w, h, hotbar, state);
        return;
    }
    let cross = scale.to_physical(hud.crosshair);
    rect(
        v,
        w,
        h,
        DesignRect::new(cross.x + 2.5, cross.y - 1.0, 4.0, cross.height + 2.0),
        [0.0, 0.0, 0.0, 0.85],
    );
    rect(
        v,
        w,
        h,
        DesignRect::new(cross.x - 1.0, cross.y + 2.5, cross.width + 2.0, 4.0),
        [0.0, 0.0, 0.0, 0.85],
    );
    for r in [
        DesignRect::new(cross.x + 3.5, cross.y, 2.0, 3.0),
        DesignRect::new(cross.x + 3.5, cross.y + 6.0, 2.0, 3.0),
        DesignRect::new(cross.x, cross.y + 3.5, 3.0, 2.0),
        DesignRect::new(cross.x + 6.0, cross.y + 3.5, 3.0, 2.0),
    ] {
        rect(v, w, h, r, [1.0, 1.0, 1.0, 1.0]);
    }
    if let Some(item) = crate::world::catalog::item(state.hotbar[usize::from(state.selected_slot)])
    {
        text::draw(
            v,
            (w, h),
            (hotbar.x, hotbar.y - 18.0),
            item.name,
            1.25,
            [0.94, 0.96, 1.0, 1.0],
        );
    }
}

pub(super) fn draw_hotbar_label(
    v: &mut Vec<UiVertex>,
    w: u32,
    h: u32,
    hotbar: DesignRect,
    state: &CreativeInventory,
) {
    if let Some(item) = crate::world::catalog::item(state.hotbar[usize::from(state.selected_slot)])
    {
        text::draw(
            v,
            (w, h),
            (hotbar.x, hotbar.y - 18.0),
            item.name,
            1.25,
            [0.94, 0.96, 1.0, 1.0],
        );
    }
}

pub(super) fn rect(v: &mut Vec<UiVertex>, w: u32, h: u32, r: DesignRect, color: [f32; 4]) {
    let p = |x: f32, y: f32| UiVertex {
        position: [x / w as f32 * 2.0 - 1.0, 1.0 - y / h as f32 * 2.0],
        color,
        icon_uv: [0.0, 0.0],
        icon_layer: -1.0,
    };
    let (x0, y0, x1, y1) = (r.x, r.y, r.x + r.width, r.y + r.height);
    v.extend([
        p(x0, y0),
        p(x1, y0),
        p(x1, y1),
        p(x0, y0),
        p(x1, y1),
        p(x0, y1),
    ]);
}
#[allow(clippy::too_many_arguments)]
pub(super) fn box_(
    v: &mut Vec<UiVertex>,
    w: u32,
    h: u32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
) {
    rect(v, w, h, DesignRect::new(x, y, width, height), color);
}

pub(super) fn outline(
    v: &mut Vec<UiVertex>,
    w: u32,
    h: u32,
    r: DesignRect,
    color: [f32; 4],
    thickness: f32,
) {
    for edge in [
        DesignRect::new(r.x, r.y, r.width, thickness),
        DesignRect::new(r.x, r.y + r.height - thickness, r.width, thickness),
        DesignRect::new(r.x, r.y, thickness, r.height),
        DesignRect::new(r.x + r.width - thickness, r.y, thickness, r.height),
    ] {
        rect(v, w, h, edge, color);
    }
}
