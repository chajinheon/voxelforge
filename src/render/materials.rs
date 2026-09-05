//! Deterministic 16x16 material layers and CPU mip generation.

use bytemuck::{Pod, Zeroable};

pub const MATERIAL_SIZE: usize = 16;
pub const MATERIAL_MIPS: usize = 5;
pub const MATERIAL_LAYER_CAP: usize = 256;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MaterialGpu {
    pub base: [f32; 4],
    pub tint: [f32; 4],
    pub flags: [u32; 4],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mip {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialLayer {
    pub name: String,
    pub albedo: Vec<Mip>,
    pub material: Vec<Mip>,
    pub emission: Vec<Mip>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialArrays {
    pub layers: Vec<MaterialLayer>,
}

impl MaterialArrays {
    pub fn procedural(names: &[&str]) -> Self {
        assert!(names.len() <= MATERIAL_LAYER_CAP);
        Self {
            layers: names
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    let albedo = base_albedo(name, i as u64);
                    let material = base_material(name, i as u64);
                    let emission = base_emission(name, i as u64);
                    MaterialLayer {
                        name: (*name).into(),
                        albedo: make_mips(&albedo, 4, *name == "leaves"),
                        material: make_mips(&material, 4, false),
                        emission: make_mips(&emission, 1, false),
                    }
                })
                .collect(),
        }
    }
    pub fn layer_names(&self) -> Vec<&str> {
        self.layers.iter().map(|l| l.name.as_str()).collect()
    }
    pub fn mip_count(&self) -> usize {
        self.layers.first().map_or(0, |l| l.albedo.len())
    }
    pub fn validate_contract(&self) -> bool {
        self.layers.len() <= MATERIAL_LAYER_CAP
            && self.layers.iter().all(|l| {
                l.albedo.len() == MATERIAL_MIPS
                    && l.material.len() == MATERIAL_MIPS
                    && l.emission.len() == MATERIAL_MIPS
                    && l.albedo
                        .iter()
                        .enumerate()
                        .all(|(i, m)| m.width == 16 >> i && m.height == 16 >> i)
            })
    }
}

fn hash(x: usize, y: usize, layer: u64) -> u8 {
    let mut n = (x as u64)
        .wrapping_mul(0x9e3779b97f4a7c15)
        .wrapping_add((y as u64).wrapping_mul(0xbf58476d1ce4e5b9))
        .wrapping_add(layer);
    n ^= n >> 30;
    (n.wrapping_mul(0xbf58476d1ce4e5b9) >> 24) as u8
}
fn base_albedo(name: &str, layer: u64) -> Vec<u8> {
    let color = match name {
        "stone" => [125, 125, 125],
        "brick" => [150, 84, 66],
        "wood" | "log_side" | "log_top" | "planks" => [157, 127, 78],
        "metal" | "cobble" => [190, 198, 210],
        "gold" | "sand" => [220, 170, 42],
        "emissive" | "torch" | "glowstone" | "warm_lamp" | "glow_panel" => [240, 90, 30],
        "sea_lantern" | "cold_lamp" => [120, 220, 240],
        "water" => [63, 118, 228],
        "leaves" | "birch_leaves" | "spruce_leaves" | "dark_oak_leaves" => [72, 132, 61],
        "ice" | "packed_ice" => [142, 205, 232],
        "dirt" | "dirt_path" | "mud" => [134, 96, 67],
        "gravel" | "polished_stone" | "stone_bricks" | "mossy_stone_bricks" | "chiseled_stone"
        | "slate" | "polished_slate" | "basalt" | "polished_basalt" | "limestone"
        | "limestone_bricks" | "marble" | "marble_tiles" | "obsidian" | "prismarine"
        | "dark_prismarine" => [125, 125, 125],
        "sandstone" | "cut_sandstone" | "red_sandstone" | "hay_bale" => [190, 166, 63],
        "birch_log" | "spruce_log" | "dark_oak_log" | "birch_planks" | "spruce_planks"
        | "dark_oak_planks" | "bookshelf" | "crate" | "barrel" => [145, 98, 52],
        "quartz" | "quartz_tiles" | "white_concrete" => [208, 210, 208],
        "light_gray_concrete" => [180, 182, 182],
        "gray_concrete" => [128, 130, 132],
        "black_concrete" => [38, 40, 44],
        "brown_concrete" => [112, 72, 48],
        "red_concrete" => [170, 52, 45],
        "orange_concrete" => [220, 112, 40],
        "yellow_concrete" => [224, 190, 44],
        "lime_concrete" => [120, 190, 48],
        "green_concrete" => [58, 132, 62],
        "cyan_concrete" => [45, 170, 170],
        "light_blue_concrete" => [72, 160, 216],
        "blue_concrete" => [55, 82, 180],
        "purple_concrete" => [122, 68, 170],
        "magenta_concrete" => [190, 62, 160],
        "pink_concrete" => [232, 120, 160],
        "white_glass" => [220, 235, 240],
        "red_glass" => [220, 75, 70],
        "green_glass" => [75, 190, 100],
        "cyan_glass" => [65, 200, 200],
        "blue_glass" => [75, 115, 220],
        "metal_panel" | "rusted_metal" | "copper" | "weathered_copper" => [160, 166, 172],
        "gold_block" => [224, 170, 42],
        _ => [190, 190, 190],
    };
    let mut out = vec![0; 16 * 16 * 4];
    for y in 0..16 {
        for x in 0..16 {
            let mut d = (hash(x, y, layer) % 17) as i16 - 8;
            if name == "stone" {
                d /= 8;
            }
            let i = (y * 16 + x) * 4;
            out[i..i + 3].copy_from_slice(&color.map(|v| (v as i16 + d).clamp(0, 255) as u8));
            out[i + 3] = if name == "leaves" {
                (hash(x, y, layer) % 2) * 255
            } else {
                255
            };
        }
    }
    out
}
fn base_material(name: &str, layer: u64) -> Vec<u8> {
    let rough = match name {
        "metal" | "cobble" | "metal_panel" | "copper" | "weathered_copper" => 48,
        "rusted_metal" => 112,
        "gold" | "sand" | "gold_block" => 65,
        "brick" => 190,
        "wood" | "log_side" | "log_top" | "planks" => 150,
        _ => 210,
    };
    let mut out = vec![128; 16 * 16 * 4];
    let normal_amplitude: i16 = match name {
        "stone" => 1,
        "brick" => 34,
        "wood" | "log_side" | "log_top" | "planks" => 22,
        "metal" | "cobble" | "metal_panel" | "rusted_metal" | "copper" | "weathered_copper" => 10,
        "gold" | "sand" => 45,
        _ => 16,
    };
    for y in 0..16 {
        for x in 0..16 {
            let i = (y * 16 + x) * 4;
            let nx = hash(x, y, layer) as i16 - 128;
            let ny = hash(y, x, layer.wrapping_add(0x9e37)) as i16 - 128;
            out[i] = (128 + nx * normal_amplitude / 128).clamp(0, 255) as u8;
            out[i + 1] = (128 + ny * normal_amplitude / 128).clamp(0, 255) as u8;
            out[i + 2] = rough;
            out[i + 3] = hash(x, y, layer);
        }
    }
    out
}
fn base_emission(name: &str, layer: u64) -> Vec<u8> {
    let value = if matches!(
        name,
        "emissive"
            | "torch"
            | "glowstone"
            | "sea_lantern"
            | "warm_lamp"
            | "cold_lamp"
            | "glow_panel"
    ) {
        255
    } else {
        0
    };
    let mut out: Vec<u8> = vec![value; 16 * 16];
    for (i, v) in out.iter_mut().enumerate() {
        if value > 0 {
            *v = v.saturating_sub(hash(i % 16, i / 16, layer) % 16);
        }
    }
    out
}

pub(crate) fn make_mips(src: &[u8], channels: usize, coverage: bool) -> Vec<Mip> {
    let mut result = Vec::with_capacity(MATERIAL_MIPS);
    let mut w = 16;
    let mut h = 16;
    let mut current = src.to_vec();
    let target_coverage = if coverage && channels >= 4 {
        src.chunks(channels).filter(|p| p[3] >= 128).count() as f32 / 256.0
    } else {
        0.0
    };
    result.push(Mip {
        width: w,
        height: h,
        data: current.clone(),
    });
    while w > 1 {
        let nw = w / 2;
        let nh = h / 2;
        let mut next = vec![0; nw * nh * channels];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..channels {
                    let mut sum = 0u32;
                    for oy in 0..2 {
                        for ox in 0..2 {
                            sum += current[((y * 2 + oy) * w + x * 2 + ox) * channels + c] as u32;
                        }
                    }
                    next[(y * nw + x) * channels + c] = (sum / 4) as u8;
                }
            }
        }
        if coverage && channels >= 4 {
            let wanted = (target_coverage * (nw * nh) as f32).round() as usize;
            let mut order: Vec<(usize, u8)> =
                (0..nw * nh).map(|i| (i, next[i * channels + 3])).collect();
            order.sort_by_key(|(_, value)| std::cmp::Reverse(*value));
            for (rank, (i, _)) in order.into_iter().enumerate() {
                next[i * channels + 3] = if rank < wanted { 255 } else { 0 };
            }
        }
        w = nw;
        h = nh;
        current = next.clone();
        result.push(Mip {
            width: w,
            height: h,
            data: next,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn material_arrays_share_layer_order_and_mips() {
        let a = MaterialArrays::procedural(&["stone", "brick", "wood", "metal"]);
        assert!(a.validate_contract());
        assert_eq!(a.layer_names(), vec!["stone", "brick", "wood", "metal"]);
        assert_eq!(a.mip_count(), 5);
        for l in a.layers {
            assert_eq!(l.albedo.len(), l.material.len());
            assert_eq!(l.material.len(), l.emission.len());
        }
    }
    #[test]
    fn cutout_mips_preserve_coverage() {
        let src = base_albedo("leaves", 2);
        let m = make_mips(&src, 4, true);
        let base = src.chunks(4).filter(|p| p[3] >= 128).count() as f32 / 256.0;
        for mip in m {
            if mip.width == 1 {
                continue;
            }
            let c = mip.data.chunks(4).filter(|p| p[3] >= 128).count() as f32
                / (mip.width * mip.height) as f32;
            assert!((c - base).abs() <= 0.03, "{} vs {}", c, base);
        }
    }
}
