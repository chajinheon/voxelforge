//! Procedural Voxelforge application icon generator.

use std::path::PathBuf;

use anyhow::Context;
use image::{ImageFormat, Rgba, RgbaImage};

const SIZE: u32 = 1024;
const BACKGROUND: Rgba<u8> = Rgba([28, 32, 42, 255]);

fn main() -> anyhow::Result<()> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/dist/voxelforge-icon.png"));
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let icon = make_icon();
    icon.save_with_format(&output, ImageFormat::Png)
        .with_context(|| format!("write icon {}", output.display()))?;
    println!("wrote {} ({}x{})", output.display(), SIZE, SIZE);
    Ok(())
}

fn make_icon() -> RgbaImage {
    let mut image = RgbaImage::from_pixel(SIZE, SIZE, BACKGROUND);
    let top = [(512, 128), (832, 304), (512, 480), (192, 304)];
    let bottom_left = [(192, 304), (512, 480), (512, 800), (192, 624)];
    let bottom_right = [(512, 480), (832, 304), (832, 624), (512, 800)];

    // The cube has three visible vertical material layers.
    fill_face(&mut image, &bottom_left, Rgba([0, 0, 0, 0]));
    fill_face(&mut image, &bottom_right, Rgba([0, 0, 0, 0]));
    fill_layers(&mut image, true);
    fill_layers(&mut image, false);
    fill_face(&mut image, &top, Rgba([104, 184, 72, 255]));

    // Crisp isometric edges survive the generated 16..1024 icon downscales.
    stroke(&mut image, &top, Rgba([18, 28, 29, 255]), 8);
    stroke(&mut image, &bottom_left, Rgba([18, 28, 29, 255]), 8);
    stroke(&mut image, &bottom_right, Rgba([18, 28, 29, 255]), 8);
    image
}

fn fill_layers(image: &mut RgbaImage, left: bool) {
    let colors = [
        Rgba([128, 80, 48, 255]),
        Rgba([104, 65, 46, 255]),
        Rgba([91, 96, 105, 255]),
    ];
    for (index, color) in colors.into_iter().enumerate() {
        let y0 = 304 + index as i32 * 106;
        let y1 = y0 + 106;
        let face = if left {
            [(192, y0), (512, y0 + 176), (512, y1 + 176), (192, y1)]
        } else {
            [(512, y0 + 176), (832, y0), (832, y1), (512, y1 + 176)]
        };
        fill_face(image, &face, color);
    }
}

fn fill_face(image: &mut RgbaImage, points: &[(i32, i32); 4], color: Rgba<u8>) {
    let min_y = points.iter().map(|point| point.1).min().unwrap_or(0).max(0);
    let max_y = points
        .iter()
        .map(|point| point.1)
        .max()
        .unwrap_or(0)
        .min(SIZE as i32 - 1);
    for y in min_y..=max_y {
        let mut intersections = Vec::with_capacity(4);
        for edge in points.iter().zip(points.iter().cycle().skip(1)).take(4) {
            let (a, b) = edge;
            if (a.1 <= y && y < b.1) || (b.1 <= y && y < a.1) {
                let t = (y - a.1) as f32 / (b.1 - a.1) as f32;
                intersections.push(a.0 as f32 + t * (b.0 - a.0) as f32);
            }
        }
        intersections.sort_by(f32::total_cmp);
        for pair in intersections.chunks_exact(2) {
            let start = pair[0].ceil().max(0.0) as u32;
            let end = pair[1].floor().min(SIZE as f32 - 1.0) as u32;
            for x in start..=end {
                image.put_pixel(x, y as u32, color);
            }
        }
    }
}

fn stroke(image: &mut RgbaImage, points: &[(i32, i32); 4], color: Rgba<u8>, width: i32) {
    for edge in points.iter().zip(points.iter().cycle().skip(1)).take(4) {
        let (a, b) = edge;
        let steps = (a.0 - b.0).abs().max((a.1 - b.1).abs()).max(1);
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let x = (a.0 as f32 + (b.0 - a.0) as f32 * t) as i32;
            let y = (a.1 as f32 + (b.1 - a.1) as f32 * t) as i32;
            for oy in -width..=width {
                for ox in -width..=width {
                    if ox * ox + oy * oy <= width * width {
                        let px = x + ox;
                        let py = y + oy;
                        if px >= 0 && py >= 0 && px < SIZE as i32 && py < SIZE as i32 {
                            image.put_pixel(px as u32, py as u32, color);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_has_contract_size_and_opaque_pixels() {
        let image = make_icon();
        assert_eq!(image.dimensions(), (1024, 1024));
        assert!(image.pixels().all(|pixel| pixel[3] == 255));
    }

    #[test]
    fn icon_preserves_background_and_three_material_colors() {
        let image = make_icon();
        assert_eq!(*image.get_pixel(0, 0), BACKGROUND);
        assert!(image.pixels().any(|pixel| pixel[1] == 184));
        assert!(image.pixels().any(|pixel| pixel[0] == 128));
        assert!(image.pixels().any(|pixel| pixel[0] == 91));
    }
}
