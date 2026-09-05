//! Deterministic CPU-side material recipes shared by texture generation and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexturePattern {
    StoneNoise,
    Cobble,
    Brick,
    Tile,
    Concrete,
    Plank,
    LogSide,
    LogTop,
    Leaves,
    Glass,
    MetalPanel,
    Organic,
    Ice,
    EmissiveGrid,
    Bookshelf,
    Crate,
    Barrel,
    Hay,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureRecipe {
    pub name: &'static str,
    pub pattern: TexturePattern,
    pub base_srgb: [u8; 3],
    pub accent_srgb: [u8; 3],
    pub mortar_srgb: [u8; 3],
    pub roughness: u8,
    pub metallic: u8,
    pub normal_strength: f32,
    pub height_scale: f32,
    pub emission: u8,
}
const fn r(
    name: &'static str,
    pattern: TexturePattern,
    base: [u8; 3],
    accent: [u8; 3],
    rough: u8,
    metal: u8,
) -> TextureRecipe {
    TextureRecipe {
        name,
        pattern,
        base_srgb: base,
        accent_srgb: accent,
        mortar_srgb: [base[0] / 2, base[1] / 2, base[2] / 2],
        roughness: rough,
        metallic: metal,
        normal_strength: 1.0,
        height_scale: 0.025,
        emission: 0,
    }
}
pub const RECIPES: [TextureRecipe; 14] = [
    r(
        "stone",
        TexturePattern::StoneNoise,
        [118, 122, 126],
        [151, 154, 157],
        218,
        0,
    ),
    r(
        "cobble",
        TexturePattern::Cobble,
        [112, 116, 118],
        [148, 151, 152],
        238,
        0,
    ),
    r(
        "brick",
        TexturePattern::Brick,
        [151, 66, 48],
        [190, 91, 65],
        215,
        0,
    ),
    r(
        "oak_plank",
        TexturePattern::Plank,
        [153, 108, 59],
        [196, 145, 82],
        175,
        0,
    ),
    r(
        "leaves",
        TexturePattern::Leaves,
        [72, 132, 61],
        [112, 156, 73],
        210,
        0,
    ),
    r(
        "glass",
        TexturePattern::Glass,
        [190, 224, 232],
        [226, 246, 250],
        20,
        0,
    ),
    r(
        "metal_panel",
        TexturePattern::MetalPanel,
        [113, 121, 128],
        [159, 166, 171],
        92,
        230,
    ),
    r(
        "gold",
        TexturePattern::MetalPanel,
        [224, 170, 42],
        [249, 210, 80],
        82,
        255,
    ),
    r(
        "concrete",
        TexturePattern::Concrete,
        [208, 210, 208],
        [225, 226, 224],
        230,
        0,
    ),
    r(
        "ice",
        TexturePattern::Ice,
        [142, 205, 232],
        [206, 238, 250],
        35,
        0,
    ),
    r(
        "bookshelf",
        TexturePattern::Bookshelf,
        [120, 76, 42],
        [180, 118, 60],
        190,
        0,
    ),
    r(
        "crate",
        TexturePattern::Crate,
        [145, 98, 52],
        [190, 134, 72],
        190,
        0,
    ),
    r(
        "barrel",
        TexturePattern::Barrel,
        [118, 71, 39],
        [170, 107, 56],
        190,
        0,
    ),
    r(
        "hay",
        TexturePattern::Hay,
        [190, 166, 63],
        [226, 205, 96],
        220,
        0,
    ),
];
pub fn recipe(name: &str) -> Option<&'static TextureRecipe> {
    RECIPES.iter().find(|r| r.name == name).or_else(|| {
        let pattern = if name.ends_with("_glass") || name == "water" {
            "glass"
        } else if name.contains("concrete") {
            "concrete"
        } else if name.contains("leaves") {
            "leaves"
        } else if name.contains("log")
            || name.contains("plank")
            || matches!(name, "bookshelf" | "crate" | "barrel")
        {
            "oak_plank"
        } else if matches!(
            name,
            "metal_panel" | "rusted_metal" | "copper" | "weathered_copper" | "gold_block"
        ) {
            "metal_panel"
        } else if matches!(name, "ice" | "packed_ice") {
            "ice"
        } else if matches!(
            name,
            "glowstone" | "sea_lantern" | "warm_lamp" | "cold_lamp" | "glow_panel" | "torch"
        ) {
            "gold"
        } else if matches!(name, "brick" | "stone_bricks" | "mossy_stone_bricks") {
            "brick"
        } else {
            "stone"
        };
        RECIPES.iter().find(|r| r.name == pattern)
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recipes_are_finite_and_unique() {
        let mut n = std::collections::BTreeSet::new();
        for r in RECIPES {
            assert!(r.normal_strength.is_finite() && r.height_scale.is_finite());
            assert!(n.insert(r.name));
        }
    }

    #[test]
    fn registry_names_resolve_to_deterministic_recipes() {
        for name in crate::world::block::TEXTURES {
            let r = recipe(name).expect(name);
            assert!(r.normal_strength.is_finite() && r.height_scale.is_finite());
        }
    }
}
