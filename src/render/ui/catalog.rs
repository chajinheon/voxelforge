//! Creative catalog adapters and deterministic 64px icon layers.

use super::UiVertex;
use crate::ui::{DesignRect, ItemCategory as UiCategory, ItemRecord};
use crate::world::{
    block, catalog,
    raycast::shape_kind,
    shape::{self, Face, ShapeKind},
};

pub const ITEM_ICON_LAYERS: usize = 122;
pub const ITEM_ICON_SIZE: usize = 64;

pub fn catalog_records() -> Vec<ItemRecord> {
    crate::world::catalog::all_items()
        .map(|item| ItemRecord {
            id: item.id,
            name: item.name,
            search_terms: item.search_terms,
            category: match item.category {
                crate::world::catalog::ItemCategory::Terrain => UiCategory::Terrain,
                crate::world::catalog::ItemCategory::Masonry => UiCategory::Masonry,
                crate::world::catalog::ItemCategory::WoodNature => UiCategory::Wood,
                crate::world::catalog::ItemCategory::Color => UiCategory::Color,
                crate::world::catalog::ItemCategory::GlassLight => UiCategory::GlassLight,
                crate::world::catalog::ItemCategory::DetailUtility => UiCategory::Detail,
                crate::world::catalog::ItemCategory::Shapes => UiCategory::Shapes,
            },
        })
        .collect()
}

pub const fn item_icon_memory_bytes() -> usize {
    ITEM_ICON_LAYERS * ITEM_ICON_SIZE * ITEM_ICON_SIZE * 4
}

pub fn bake_item_icons() -> Vec<Vec<u8>> {
    (1..=ITEM_ICON_LAYERS as u16)
        .map(item_icon_pixels)
        .collect()
}

pub fn item_icon_pixels(id: u16) -> Vec<u8> {
    let mut pixels = vec![0_u8; ITEM_ICON_SIZE * ITEM_ICON_SIZE * 4];
    let Some(item) = catalog::item(id) else {
        return pixels;
    };
    let kind = shape_kind(item.icon_block);
    let template = shape::shape_template(kind);
    let seed = u32::from(id).wrapping_mul(37).wrapping_add(19);
    let mut base = [
        0.28 + (seed % 120) as f32 / 255.0,
        0.32 + ((seed / 3) % 105) as f32 / 255.0,
        0.38 + ((seed / 7) % 90) as f32 / 255.0,
    ];
    let emission = block::def(item.icon_block).emission_rgb;
    if emission != [0.0; 3] {
        base = [
            0.38 + emission[0] / 24.0,
            0.28 + emission[1] / 24.0,
            0.18 + emission[2] / 24.0,
        ];
    }
    let alpha = if block::def(item.icon_block).translucent {
        190
    } else {
        255
    };
    for quad in &template.quads {
        let [a, b, c, d] = icon_quad(quad, kind);
        let shade = face_shade(quad.face);
        raster_icon_triangle(&mut pixels, [a, b, c], base, alpha, shade, seed);
        raster_icon_triangle(&mut pixels, [a, c, d], base, alpha, shade, seed);
    }
    pixels
}

fn icon_quad(quad: &shape::TemplateQuad, kind: ShapeKind) -> [[f32; 2]; 4] {
    let a = quad.min.map(|v| v as f32 / 16.0 - 0.5);
    let b = quad.max.map(|v| v as f32 / 16.0 - 0.5);
    let corners = match quad.face {
        Face::NegX => [
            [a[0], a[1], a[2]],
            [a[0], b[1], a[2]],
            [a[0], b[1], b[2]],
            [a[0], a[1], b[2]],
        ],
        Face::PosX => [
            [b[0], a[1], a[2]],
            [b[0], a[1], b[2]],
            [b[0], b[1], b[2]],
            [b[0], b[1], a[2]],
        ],
        Face::NegY => [
            [a[0], a[1], a[2]],
            [b[0], a[1], a[2]],
            [b[0], a[1], b[2]],
            [a[0], a[1], b[2]],
        ],
        Face::PosY => [
            [a[0], b[1], a[2]],
            [a[0], b[1], b[2]],
            [b[0], b[1], b[2]],
            [b[0], b[1], a[2]],
        ],
        Face::NegZ => [
            [a[0], a[1], a[2]],
            [b[0], a[1], a[2]],
            [b[0], b[1], a[2]],
            [a[0], b[1], a[2]],
        ],
        Face::PosZ => [
            [b[0], a[1], b[2]],
            [a[0], a[1], b[2]],
            [a[0], b[1], b[2]],
            [b[0], b[1], b[2]],
        ],
    };
    corners.map(|p| {
        let scale = if matches!(kind, ShapeKind::Pane { .. } | ShapeKind::Fence { .. }) {
            27.0
        } else {
            29.0
        };
        [
            32.0 + (p[0] - p[2]) * scale,
            32.0 - p[1] * 30.0 + (p[0] + p[2]) * 15.0,
        ]
    })
}

fn raster_icon_triangle(
    pixels: &mut [u8],
    tri: [[f32; 2]; 3],
    base: [f32; 3],
    alpha: u8,
    shade: f32,
    seed: u32,
) {
    let min_x = tri
        .iter()
        .map(|p| p[0])
        .fold(64.0, f32::min)
        .floor()
        .max(0.0) as usize;
    let max_x = tri
        .iter()
        .map(|p| p[0])
        .fold(0.0, f32::max)
        .ceil()
        .min(64.0) as usize;
    let min_y = tri
        .iter()
        .map(|p| p[1])
        .fold(64.0, f32::min)
        .floor()
        .max(0.0) as usize;
    let max_y = tri
        .iter()
        .map(|p| p[1])
        .fold(0.0, f32::max)
        .ceil()
        .min(64.0) as usize;
    let edge = |a: [f32; 2], b: [f32; 2], p: [f32; 2]| {
        (p[0] - a[0]) * (b[1] - a[1]) - (p[1] - a[1]) * (b[0] - a[0])
    };
    let area = edge(tri[0], tri[1], tri[2]);
    if area.abs() < 0.001 {
        return;
    }
    for y in min_y..max_y {
        for x in min_x..max_x {
            let p = [x as f32 + 0.5, y as f32 + 0.5];
            let inside = [
                edge(tri[0], tri[1], p),
                edge(tri[1], tri[2], p),
                edge(tri[2], tri[0], p),
            ]
            .iter()
            .all(|v| *v * area >= 0.0);
            if !inside {
                continue;
            }
            let grain = ((seed
                ^ (x as u32).wrapping_mul(0x9e37_79b9)
                ^ (y as u32).wrapping_mul(0x85eb_ca6b))
                % 11) as f32
                - 5.0;
            let i = (y * 64 + x) * 4;
            pixels[i..i + 4].copy_from_slice(&[
                (base[0] * shade * 255.0 + grain).clamp(0.0, 255.0) as u8,
                (base[1] * shade * 255.0 + grain).clamp(0.0, 255.0) as u8,
                (base[2] * shade * 255.0 + grain).clamp(0.0, 255.0) as u8,
                alpha,
            ]);
        }
    }
}

const fn face_shade(face: Face) -> f32 {
    match face {
        Face::PosY => 1.0,
        Face::NegY => 0.48,
        Face::PosX => 0.84,
        Face::NegX => 0.72,
        Face::PosZ => 0.9,
        Face::NegZ => 0.6,
    }
}

/// Draw one textured quad for an item icon. The layer is sampled from the
/// persistent 122-slice atlas by `ui.wgsl`; geometry stays constant regardless
/// of the icon's on-screen size.
pub(super) fn draw_item_icon(
    v: &mut Vec<UiVertex>,
    w: u32,
    h: u32,
    x: f32,
    y: f32,
    size: f32,
    id: u16,
) {
    icon_box(
        v,
        (w, h),
        DesignRect::new(x, y, size, size),
        [1.0, 1.0, 1.0, 1.0],
        id.saturating_sub(1),
    );
}

fn icon_box(
    v: &mut Vec<UiVertex>,
    output: (u32, u32),
    rect: DesignRect,
    color: [f32; 4],
    layer: u16,
) {
    let (w, h) = output;
    let (x, y, width, height) = (rect.x, rect.y, rect.width, rect.height);
    let vertex = |px: f32, py: f32, uv: [f32; 2]| UiVertex {
        position: [px / w as f32 * 2.0 - 1.0, 1.0 - py / h as f32 * 2.0],
        color,
        icon_uv: uv,
        icon_layer: layer as f32,
    };
    v.extend([
        vertex(x, y, [0.0, 0.0]),
        vertex(x + width, y, [1.0, 0.0]),
        vertex(x + width, y + height, [1.0, 1.0]),
        vertex(x, y, [0.0, 0.0]),
        vertex(x + width, y + height, [1.0, 1.0]),
        vertex(x, y + height, [0.0, 1.0]),
    ]);
}
