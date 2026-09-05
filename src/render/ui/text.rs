//! Allocation-free ASCII 8x8 text geometry.

use font8x8::{BASIC_FONTS, UnicodeFonts};

use super::{UiVertex, rect};
use crate::ui::DesignRect;

pub(super) fn draw(
    vertices: &mut Vec<UiVertex>,
    output: (u32, u32),
    origin: (f32, f32),
    value: &str,
    scale: f32,
    color: [f32; 4],
) {
    let mut cursor = origin.0;
    for character in value.chars() {
        let Some(rows) = BASIC_FONTS.get(character.to_ascii_uppercase()) else {
            cursor += 9.0 * scale;
            continue;
        };
        for (y, row) in rows.into_iter().enumerate() {
            for x in 0..8 {
                if row & (1 << x) != 0 {
                    rect(
                        vertices,
                        output.0,
                        output.1,
                        DesignRect::new(
                            cursor + x as f32 * scale,
                            origin.1 + y as f32 * scale,
                            scale,
                            scale,
                        ),
                        color,
                    );
                }
            }
        }
        cursor += 9.0 * scale;
    }
}
