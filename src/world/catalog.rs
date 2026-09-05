//! Stable creative inventory catalog (ItemId is deliberately independent of BlockId).
use super::block::*;
pub type ItemId = u16;
pub const EMPTY_ITEM: ItemId = 0;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemCategory {
    Terrain,
    Masonry,
    WoodNature,
    Color,
    GlassLight,
    DetailUtility,
    Shapes,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementKind {
    Block(BlockId),
    AxisLog { y: BlockId, x: BlockId, z: BlockId },
    Slab { material: u8, full: BlockId },
    Stair { material: u8 },
    Pane { material: u8 },
    Fence { material: u8 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemDef {
    pub id: ItemId,
    pub name: &'static str,
    pub category: ItemCategory,
    pub placement: PlacementKind,
    pub icon_block: BlockId,
    pub search_terms: &'static [&'static str],
}
pub const DEFAULT_HOTBAR_ITEMS: [ItemId; 9] = [4, 5, 14, 35, 63, 89, 101, 113, 74];
const NAMES: [&str; 122] = [
    "Grass",
    "Dirt",
    "Dirt Path",
    "Stone",
    "Cobblestone",
    "Gravel",
    "Sand",
    "Red Sand",
    "Clay",
    "Mud",
    "Snow",
    "Moss",
    "Polished Stone",
    "Stone Bricks",
    "Mossy Stone Bricks",
    "Chiseled Stone",
    "Slate",
    "Polished Slate",
    "Basalt",
    "Polished Basalt",
    "Limestone",
    "Limestone Bricks",
    "Marble",
    "Marble Tiles",
    "Sandstone",
    "Cut Sandstone",
    "Red Sandstone",
    "Quartz",
    "Quartz Tiles",
    "Obsidian",
    "Oak Log",
    "Birch Log",
    "Spruce Log",
    "Dark Oak Log",
    "Oak Planks",
    "Birch Planks",
    "Spruce Planks",
    "Dark Oak Planks",
    "Oak Leaves",
    "Birch Leaves",
    "Spruce Leaves",
    "Dark Oak Leaves",
    "Bookshelf",
    "Crate",
    "Barrel",
    "Hay Bale",
    "White Concrete",
    "Light Gray Concrete",
    "Gray Concrete",
    "Black Concrete",
    "Brown Concrete",
    "Red Concrete",
    "Orange Concrete",
    "Yellow Concrete",
    "Lime Concrete",
    "Green Concrete",
    "Cyan Concrete",
    "Light Blue Concrete",
    "Blue Concrete",
    "Purple Concrete",
    "Magenta Concrete",
    "Pink Concrete",
    "Clear Glass",
    "White Glass",
    "Red Glass",
    "Green Glass",
    "Cyan Glass",
    "Blue Glass",
    "Glowstone",
    "Sea Lantern",
    "Warm Lamp",
    "Cold Lamp",
    "Glow Panel",
    "Torch",
    "Red Brick",
    "Roof Tile",
    "Checker Tile",
    "Ceramic Tile",
    "Metal Panel",
    "Rusted Metal",
    "Copper",
    "Weathered Copper",
    "Gold Block",
    "Prismarine",
    "Dark Prismarine",
    "Ice",
    "Packed Ice",
    "Water",
    "Stone Slab",
    "Stone Brick Slab",
    "Cobblestone Slab",
    "Slate Slab",
    "Marble Slab",
    "Limestone Brick Slab",
    "Sandstone Slab",
    "Red Brick Slab",
    "Oak Slab",
    "Birch Slab",
    "Spruce Slab",
    "Dark Oak Slab",
    "Stone Stairs",
    "Stone Brick Stairs",
    "Cobblestone Stairs",
    "Slate Stairs",
    "Marble Stairs",
    "Limestone Brick Stairs",
    "Sandstone Stairs",
    "Red Brick Stairs",
    "Oak Stairs",
    "Birch Stairs",
    "Spruce Stairs",
    "Dark Oak Stairs",
    "Clear Glass Pane",
    "White Glass Pane",
    "Red Glass Pane",
    "Green Glass Pane",
    "Cyan Glass Pane",
    "Blue Glass Pane",
    "Oak Fence",
    "Birch Fence",
    "Spruce Fence",
    "Dark Oak Fence",
];
const TERRAIN: &[&str] = &["terrain", "block"];
const MASONRY: &[&str] = &["masonry", "block"];
const WOOD: &[&str] = &["woodnature", "block"];
const COLOR: &[&str] = &["color", "block"];
const GLASS: &[&str] = &["glasslight", "block", "transparent"];
const DETAIL: &[&str] = &["detailutility", "block"];
const LIGHT: &[&str] = &["glasslight", "block", "light", "glowing", "emissive"];
const AXIS_LOG: &[&str] = &["woodnature", "block", "log", "wood", "axis"];
const SHAPE_SLAB: &[&str] = &["shapes", "shape", "slab", "half"];
const SHAPE_STAIR: &[&str] = &["shapes", "shape", "stair", "steps"];
const SHAPE_PANE: &[&str] = &["shapes", "shape", "pane", "glass"];
const SHAPE_FENCE: &[&str] = &["shapes", "shape", "fence", "wood"];
const fn placement(id: ItemId) -> PlacementKind {
    match id {
        31 => PlacementKind::AxisLog {
            y: 6,
            x: 128,
            z: 129,
        },
        32 => PlacementKind::AxisLog {
            y: 38,
            x: 130,
            z: 131,
        },
        33 => PlacementKind::AxisLog {
            y: 39,
            x: 132,
            z: 133,
        },
        34 => PlacementKind::AxisLog {
            y: 40,
            x: 134,
            z: 135,
        },
        89..=100 => PlacementKind::Slab {
            material: (id - 89) as u8,
            full: [1, 21, 11, 24, 30, 29, 32, 10, 8, 41, 42, 43][((id - 89) % 12) as usize],
        },
        101..=112 => PlacementKind::Stair {
            material: (id - 101) as u8,
        },
        113..=118 => PlacementKind::Pane {
            material: (id - 113) as u8,
        },
        119..=122 => PlacementKind::Fence {
            material: (id - 119) as u8,
        },
        _ => PlacementKind::Block(match id {
            1 => 3,
            2 => 2,
            3 => 13,
            4 => 1,
            5 => 11,
            6 => 14,
            7 => 4,
            8 => 15,
            9 => 16,
            10 => 17,
            11 => 18,
            12 => 19,
            13 => 20,
            14 => 21,
            15 => 22,
            16 => 23,
            17 => 24,
            18 => 25,
            19 => 26,
            20 => 27,
            21 => 28,
            22 => 29,
            23 => 30,
            24 => 31,
            25 => 32,
            26 => 33,
            27 => 34,
            28 => 35,
            29 => 36,
            30 => 37,
            35 => 8,
            36 => 41,
            37 => 42,
            38 => 43,
            39 => 7,
            40 => 44,
            41 => 45,
            42 => 46,
            43 => 47,
            44 => 48,
            45 => 49,
            46 => 50,
            47 => 51,
            48 => 52,
            49 => 53,
            50 => 54,
            51 => 55,
            52 => 56,
            53 => 57,
            54 => 58,
            55 => 59,
            56 => 60,
            57 => 61,
            58 => 62,
            59 => 63,
            60 => 64,
            61 => 65,
            62 => 66,
            63 => 9,
            64 => 67,
            65 => 68,
            66 => 69,
            67 => 70,
            68 => 71,
            69 => 72,
            70 => 73,
            71 => 74,
            72 => 75,
            73 => 76,
            74 => 12,
            75 => 10,
            76 => 77,
            77 => 78,
            78 => 79,
            79 => 80,
            80 => 81,
            81 => 82,
            82 => 83,
            83 => 84,
            84 => 85,
            85 => 86,
            86 => 87,
            87 => 88,
            88 => 5,
            _ => 0,
        }),
    }
}
pub fn item(id: ItemId) -> Option<ItemDef> {
    if !(1..=122).contains(&id) {
        return None;
    }
    let p = placement(id);
    let icon = match p {
        PlacementKind::Block(b) => b,
        PlacementKind::AxisLog { y, .. } => y,
        PlacementKind::Slab { material, .. } => SLAB_BASE + material as u16 * 2,
        PlacementKind::Stair { material } => STAIR_BASE + material as u16 * 8 + 2,
        PlacementKind::Pane { material } => PANE_BASE + material as u16 * 16 + 15,
        PlacementKind::Fence { material } => FENCE_BASE + material as u16 * 16 + 15,
    };
    let category = match id {
        1..=12 => ItemCategory::Terrain,
        13..=30 => ItemCategory::Masonry,
        31..=46 => ItemCategory::WoodNature,
        47..=62 => ItemCategory::Color,
        63..=74 => ItemCategory::GlassLight,
        75..=88 => ItemCategory::DetailUtility,
        _ => ItemCategory::Shapes,
    };
    let search_terms = match p {
        PlacementKind::Slab { .. } => SHAPE_SLAB,
        PlacementKind::Stair { .. } => SHAPE_STAIR,
        PlacementKind::Pane { .. } => SHAPE_PANE,
        PlacementKind::Fence { .. } => SHAPE_FENCE,
        PlacementKind::AxisLog { .. } => AXIS_LOG,
        PlacementKind::Block(b) => {
            if def(b).emission > 0 {
                LIGHT
            } else if def(b).translucent {
                GLASS
            } else {
                match category {
                    ItemCategory::Terrain => TERRAIN,
                    ItemCategory::Masonry => MASONRY,
                    ItemCategory::WoodNature => WOOD,
                    ItemCategory::Color => COLOR,
                    ItemCategory::GlassLight => GLASS,
                    ItemCategory::DetailUtility => DETAIL,
                    ItemCategory::Shapes => SHAPE_SLAB,
                }
            }
        }
    };
    Some(ItemDef {
        id,
        name: NAMES[id as usize - 1],
        category,
        placement: p,
        icon_block: icon,
        search_terms,
    })
}
pub fn all_items() -> impl Iterator<Item = ItemDef> {
    (1..=122).filter_map(item)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn item_catalog_has_exactly_122_unique_items() {
        let v: Vec<_> = all_items().collect();
        assert_eq!(v.len(), 122);
        assert_eq!(
            v.iter()
                .map(|x| x.id)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            122
        )
    }
    #[test]
    fn every_item_has_valid_icon_and_placement() {
        for x in all_items() {
            assert_ne!(x.icon_block, AIR);
            match x.placement {
                PlacementKind::Block(b) => assert!(def(b).name != "air"),
                PlacementKind::AxisLog { y, x, z } => {
                    assert_ne!(def(y).name, "air");
                    assert_ne!(def(x).name, "air");
                    assert_ne!(def(z).name, "air");
                }
                _ => {}
            }
        }
    }
}
