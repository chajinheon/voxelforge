//! Stable on-disk block registry. Concrete state IDs are canonicalized here.
pub type BlockId = u16;
use super::catalog::ItemId;
use super::shape::{Axis, ShapeKind};
pub const AIR: BlockId = 0;
pub const STONE: BlockId = 1;
pub const DIRT: BlockId = 2;
pub const GRASS: BlockId = 3;
pub const SAND: BlockId = 4;
pub const WATER: BlockId = 5;
pub const LOG: BlockId = 6;
pub const OAK_LOG_Y: BlockId = 6;
pub const LEAVES: BlockId = 7;
pub const OAK_LEAVES: BlockId = 7;
pub const PLANKS: BlockId = 8;
pub const OAK_PLANKS: BlockId = 8;
pub const GLASS: BlockId = 9;
pub const CLEAR_GLASS: BlockId = 9;
pub const BRICK: BlockId = 10;
pub const RED_BRICK: BlockId = 10;
pub const COBBLE: BlockId = 11;
pub const COBBLESTONE: BlockId = 11;
pub const TORCH: BlockId = 12;
macro_rules! ids { ($($n:ident=$v:expr),+ $(,)?) => { $(pub const $n:BlockId=$v;)+ } }
ids! { DIRT_PATH=13,GRAVEL=14,RED_SAND=15,CLAY=16,MUD=17,SNOW=18,MOSS=19,POLISHED_STONE=20,STONE_BRICKS=21,MOSSY_STONE_BRICKS=22,CHISELED_STONE=23,SLATE=24,POLISHED_SLATE=25,BASALT=26,POLISHED_BASALT=27,LIMESTONE=28,LIMESTONE_BRICKS=29,MARBLE=30,MARBLE_TILES=31,SANDSTONE=32,CUT_SANDSTONE=33,RED_SANDSTONE=34,QUARTZ=35,QUARTZ_TILES=36,OBSIDIAN=37,BIRCH_LOG_Y=38,SPRUCE_LOG_Y=39,DARK_OAK_LOG_Y=40,BIRCH_PLANKS=41,SPRUCE_PLANKS=42,DARK_OAK_PLANKS=43,BIRCH_LEAVES=44,SPRUCE_LEAVES=45,DARK_OAK_LEAVES=46,BOOKSHELF=47,CRATE=48,BARREL=49,HAY_BALE=50,WHITE_CONCRETE=51,LIGHT_GRAY_CONCRETE=52,GRAY_CONCRETE=53,BLACK_CONCRETE=54,BROWN_CONCRETE=55,RED_CONCRETE=56,ORANGE_CONCRETE=57,YELLOW_CONCRETE=58,LIME_CONCRETE=59,GREEN_CONCRETE=60,CYAN_CONCRETE=61,LIGHT_BLUE_CONCRETE=62,BLUE_CONCRETE=63,PURPLE_CONCRETE=64,MAGENTA_CONCRETE=65,PINK_CONCRETE=66,WHITE_GLASS=67,RED_GLASS=68,GREEN_GLASS=69,CYAN_GLASS=70,BLUE_GLASS=71,GLOWSTONE=72,SEA_LANTERN=73,WARM_LAMP=74,COLD_LAMP=75,GLOW_PANEL=76,ROOF_TILE=77,CHECKER_TILE=78,CERAMIC_TILE=79,METAL_PANEL=80,RUSTED_METAL=81,COPPER=82,WEATHERED_COPPER=83,GOLD_BLOCK=84,PRISMARINE=85,DARK_PRISMARINE=86,ICE=87,PACKED_ICE=88 }
pub const OAK_LOG_X: BlockId = 128;
pub const OAK_LOG_Z: BlockId = 129;
pub const BIRCH_LOG_X: BlockId = 130;
pub const BIRCH_LOG_Z: BlockId = 131;
pub const SPRUCE_LOG_X: BlockId = 132;
pub const SPRUCE_LOG_Z: BlockId = 133;
pub const DARK_OAK_LOG_X: BlockId = 134;
pub const DARK_OAK_LOG_Z: BlockId = 135;
pub const SLAB_BASE: BlockId = 256;
pub const STAIR_BASE: BlockId = 512;
pub const PANE_BASE: BlockId = 768;
pub const FENCE_BASE: BlockId = 896;
pub const TEXTURES: &[&str] = &[
    "stone",
    "dirt",
    "grass_top",
    "grass_side",
    "sand",
    "water",
    "log_side",
    "log_top",
    "leaves",
    "planks",
    "glass",
    "brick",
    "cobble",
    "torch",
    "dirt_path",
    "gravel",
    "red_sand",
    "clay",
    "mud",
    "snow",
    "moss",
    "polished_stone",
    "stone_bricks",
    "mossy_stone_bricks",
    "chiseled_stone",
    "slate",
    "polished_slate",
    "basalt",
    "polished_basalt",
    "limestone",
    "limestone_bricks",
    "marble",
    "marble_tiles",
    "sandstone",
    "cut_sandstone",
    "red_sandstone",
    "quartz",
    "quartz_tiles",
    "obsidian",
    "birch_log",
    "spruce_log",
    "dark_oak_log",
    "birch_planks",
    "spruce_planks",
    "dark_oak_planks",
    "birch_leaves",
    "spruce_leaves",
    "dark_oak_leaves",
    "bookshelf",
    "crate",
    "barrel",
    "hay_bale",
    "white_concrete",
    "light_gray_concrete",
    "gray_concrete",
    "black_concrete",
    "brown_concrete",
    "red_concrete",
    "orange_concrete",
    "yellow_concrete",
    "lime_concrete",
    "green_concrete",
    "cyan_concrete",
    "light_blue_concrete",
    "blue_concrete",
    "purple_concrete",
    "magenta_concrete",
    "pink_concrete",
    "white_glass",
    "red_glass",
    "green_glass",
    "cyan_glass",
    "blue_glass",
    "glowstone",
    "sea_lantern",
    "warm_lamp",
    "cold_lamp",
    "glow_panel",
    "roof_tile",
    "checker_tile",
    "ceramic_tile",
    "metal_panel",
    "rusted_metal",
    "copper",
    "weathered_copper",
    "gold_block",
    "prismarine",
    "dark_prismarine",
    "ice",
    "packed_ice",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderClass {
    Opaque,
    Cutout,
    Translucent,
    Water,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockDef {
    pub name: &'static str,
    pub solid: bool,
    pub opaque: bool,
    pub translucent: bool,
    pub textures: [u16; 6],
    pub emission: u8,
    pub emission_rgb: [f32; 3],
    pub light_blocking: bool,
    pub render_class: RenderClass,
    pub shape: ShapeKind,
    pub pick_item: ItemId,
}
const AIR_DEF: BlockDef = BlockDef {
    name: "air",
    solid: false,
    opaque: false,
    translucent: false,
    textures: [0; 6],
    emission: 0,
    emission_rgb: [0.0; 3],
    light_blocking: false,
    render_class: RenderClass::Opaque,
    shape: ShapeKind::Cube,
    pick_item: 0,
};
const NAMES: [&str; 89] = [
    "air",
    "stone",
    "dirt",
    "grass",
    "sand",
    "water",
    "oak_log_y",
    "oak_leaves",
    "oak_planks",
    "clear_glass",
    "red_brick",
    "cobblestone",
    "torch",
    "dirt_path",
    "gravel",
    "red_sand",
    "clay",
    "mud",
    "snow",
    "moss",
    "polished_stone",
    "stone_bricks",
    "mossy_stone_bricks",
    "chiseled_stone",
    "slate",
    "polished_slate",
    "basalt",
    "polished_basalt",
    "limestone",
    "limestone_bricks",
    "marble",
    "marble_tiles",
    "sandstone",
    "cut_sandstone",
    "red_sandstone",
    "quartz",
    "quartz_tiles",
    "obsidian",
    "birch_log_y",
    "spruce_log_y",
    "dark_oak_log_y",
    "birch_planks",
    "spruce_planks",
    "dark_oak_planks",
    "birch_leaves",
    "spruce_leaves",
    "dark_oak_leaves",
    "bookshelf",
    "crate",
    "barrel",
    "hay_bale",
    "white_concrete",
    "light_gray_concrete",
    "gray_concrete",
    "black_concrete",
    "brown_concrete",
    "red_concrete",
    "orange_concrete",
    "yellow_concrete",
    "lime_concrete",
    "green_concrete",
    "cyan_concrete",
    "light_blue_concrete",
    "blue_concrete",
    "purple_concrete",
    "magenta_concrete",
    "pink_concrete",
    "white_glass",
    "red_glass",
    "green_glass",
    "cyan_glass",
    "blue_glass",
    "glowstone",
    "sea_lantern",
    "warm_lamp",
    "cold_lamp",
    "glow_panel",
    "roof_tile",
    "checker_tile",
    "ceramic_tile",
    "metal_panel",
    "rusted_metal",
    "copper",
    "weathered_copper",
    "gold_block",
    "prismarine",
    "dark_prismarine",
    "ice",
    "packed_ice",
];
const fn make_defs() -> [BlockDef; 89] {
    let mut a = [AIR_DEF; 89];
    let mut i = 0;
    while i < 89 {
        let tr = i == 5 || (67 <= i && i <= 71) || i == 9 || i == 87 || i == 88;
        let layer = if i <= 12 {
            [0, 1, 2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13][i]
        } else {
            i + 1
        } as u16;
        let shape = if i == 6 {
            ShapeKind::Log { axis: Axis::Y }
        } else {
            ShapeKind::Cube
        };
        let water = i == 5;
        let leaves = i == 7 || (i >= 44 && i <= 46);
        let render_class = if water {
            RenderClass::Water
        } else if tr {
            RenderClass::Translucent
        } else if leaves {
            RenderClass::Cutout
        } else {
            RenderClass::Opaque
        };
        let light_blocking = i != 0 && !tr && !leaves;
        a[i] = BlockDef {
            name: NAMES[i],
            solid: i != 0 && i != 5,
            opaque: i != 0 && !tr && i != 7,
            translucent: tr,
            textures: if i == 1 {
                [0; 6]
            } else if i == 2 {
                [1; 6]
            } else if i == 3 {
                [3, 3, 2, 1, 3, 3]
            } else if i == 4 {
                [4; 6]
            } else if i == 5 {
                [5; 6]
            } else if i == 6 {
                [6, 6, 7, 7, 6, 6]
            } else if i == 7 {
                [8; 6]
            } else if i == 8 {
                [9; 6]
            } else if i == 9 {
                [10; 6]
            } else if i == 10 {
                [11; 6]
            } else if i == 11 {
                [12; 6]
            } else if i == 12 {
                [13; 6]
            } else {
                [layer; 6]
            },
            emission: if i == 12 {
                14
            } else if 72 <= i && i <= 76 {
                15
            } else {
                0
            },
            emission_rgb: if i == 12 {
                [8.0, 3.36, 0.96]
            } else if i == 72 {
                [6.0, 3.4, 1.5]
            } else if i == 73 {
                [3.0, 5.5, 6.5]
            } else if i == 74 {
                [8.0, 4.4, 1.8]
            } else if i == 75 {
                [4.5, 6.5, 8.0]
            } else if i == 76 {
                [6.5, 6.8, 7.0]
            } else {
                [0.0; 3]
            },
            light_blocking,
            render_class,
            shape,
            pick_item: pick_item_for_block(i as BlockId),
        };
        i += 1;
    }
    a
}
const fn pick_item_for_block(id: BlockId) -> ItemId {
    match id {
        3 => 1,
        2 => 2,
        13..=34 => id - 10,
        35..=37 => id - 7,
        38..=40 => id - 7,
        41..=46 => id - 6,
        47..=66 => id - 4,
        67..=71 => id - 3,
        72..=76 => id - 3,
        77..=88 => id - 1,
        5 => 88,
        6 => 31,
        _ => 0,
    }
}
const DEFS: [BlockDef; 89] = make_defs();
pub const HOTBAR: [BlockId; 9] = [STONE, GRASS, SAND, WATER, LOG, PLANKS, GLASS, BRICK, TORCH];
pub const fn canonical_base(id: BlockId) -> BlockId {
    match id {
        0..=88 => id,
        128..=135 => match id {
            128 | 129 => 6,
            130 | 131 => 38,
            132 | 133 => 39,
            134 | 135 => 40,
            _ => 0,
        },
        256..=279 => [1, 21, 11, 24, 30, 29, 32, 10, 8, 41, 42, 43][((id - 256) / 2) as usize],
        512..=607 => [1, 21, 11, 24, 30, 29, 32, 10, 8, 41, 42, 43][((id - 512) / 8) as usize],
        768..=863 => [9, 67, 68, 69, 70, 71][((id - 768) / 16) as usize],
        896..=959 => [8, 41, 42, 43][((id - 896) / 16) as usize],
        _ => 0,
    }
}

/// Resolve the material that represents a block in a coarse LOD grid.
///
/// Shape states are intentionally handled before modal selection: otherwise a
/// stair or slab can win a tie and re-introduce a shape ID into a coarse grid.
/// Pane and fence geometry has no solid distant representation and therefore
/// contributes air. Reserved IDs keep the historical out-of-range-as-air
/// contract.
pub const fn lod_equivalent(id: BlockId) -> BlockId {
    match id {
        0..=88 => id,
        128..=135 => canonical_base(id),
        256..=279 | 512..=607 => canonical_base(id),
        768..=863 | 896..=959 => AIR,
        _ => AIR,
    }
}
pub fn def(id: BlockId) -> &'static BlockDef {
    DEFS.get(canonical_base(id) as usize).unwrap_or(&AIR_DEF)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn block_registry_ids_are_stable() {
        assert_eq!(LOG, 6);
        assert_eq!(TORCH, 12);
        assert_eq!(canonical_base(OAK_LOG_X), LOG);
        assert_eq!(canonical_base(SLAB_BASE + 7), SLATE);
        assert_eq!(def(960).name, "air");
    }

    #[test]
    fn every_concrete_block_has_valid_texture_layers() {
        assert_eq!(TEXTURES.len(), 90);
        for id in 0..=88 {
            let block = def(id);
            assert!(
                block
                    .textures
                    .iter()
                    .all(|&layer| (layer as usize) < TEXTURES.len())
            );
            assert!(!(block.opaque && block.translucent));
        }
        assert_eq!(
            &TEXTURES[..14],
            &[
                "stone",
                "dirt",
                "grass_top",
                "grass_side",
                "sand",
                "water",
                "log_side",
                "log_top",
                "leaves",
                "planks",
                "glass",
                "brick",
                "cobble",
                "torch"
            ]
        );
        assert_eq!(def(GLOWSTONE).emission, 15);
        assert!(def(WATER).translucent && !def(WATER).opaque);
    }

    #[test]
    fn lod_equivalent_collapses_shape_states_before_sampling() {
        assert_eq!(lod_equivalent(OAK_LOG_X), OAK_LOG_Y);
        assert_eq!(lod_equivalent(OAK_LOG_Z), OAK_LOG_Y);
        assert_eq!(lod_equivalent(SLAB_BASE + 1), STONE);
        assert_eq!(lod_equivalent(STAIR_BASE + 7), STONE);
        assert_eq!(lod_equivalent(PANE_BASE + 15), AIR);
        assert_eq!(lod_equivalent(FENCE_BASE + 15), AIR);
        assert_eq!(lod_equivalent(127), AIR);
        assert_eq!(lod_equivalent(960), AIR);
    }
}
