pub type BlockId = u16;

pub const AIR: BlockId = 0;
pub const STONE: BlockId = 1;
pub const DIRT: BlockId = 2;
pub const GRASS: BlockId = 3;
pub const SAND: BlockId = 4;
pub const WATER: BlockId = 5;
pub const LOG: BlockId = 6;
pub const LEAVES: BlockId = 7;
pub const PLANKS: BlockId = 8;
pub const GLASS: BlockId = 9;
pub const BRICK: BlockId = 10;
pub const COBBLE: BlockId = 11;
pub const TORCH: BlockId = 12;

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
];

pub struct BlockDef {
    pub name: &'static str,
    pub solid: bool,
    pub opaque: bool,
    pub translucent: bool,
    pub textures: [u16; 6],
    pub emission: u8,
}

const AIR_DEF: BlockDef = BlockDef {
    name: "air",
    solid: false,
    opaque: false,
    translucent: false,
    textures: [0; 6],
    emission: 0,
};
const DEFS: [BlockDef; 13] = [
    AIR_DEF,
    BlockDef {
        name: "stone",
        solid: true,
        opaque: true,
        translucent: false,
        textures: [0; 6],
        emission: 0,
    },
    BlockDef {
        name: "dirt",
        solid: true,
        opaque: true,
        translucent: false,
        textures: [1; 6],
        emission: 0,
    },
    BlockDef {
        name: "grass",
        solid: true,
        opaque: true,
        translucent: false,
        textures: [3, 3, 2, 1, 3, 3],
        emission: 0,
    },
    BlockDef {
        name: "sand",
        solid: true,
        opaque: true,
        translucent: false,
        textures: [4; 6],
        emission: 0,
    },
    BlockDef {
        name: "water",
        solid: false,
        opaque: false,
        translucent: true,
        textures: [5; 6],
        emission: 0,
    },
    BlockDef {
        name: "log",
        solid: true,
        opaque: true,
        translucent: false,
        textures: [6, 6, 7, 7, 6, 6],
        emission: 0,
    },
    BlockDef {
        name: "leaves",
        solid: true,
        opaque: false,
        translucent: false,
        textures: [8; 6],
        emission: 0,
    },
    BlockDef {
        name: "planks",
        solid: true,
        opaque: true,
        translucent: false,
        textures: [9; 6],
        emission: 0,
    },
    BlockDef {
        name: "glass",
        solid: true,
        opaque: false,
        translucent: true,
        textures: [10; 6],
        emission: 0,
    },
    BlockDef {
        name: "brick",
        solid: true,
        opaque: true,
        translucent: false,
        textures: [11; 6],
        emission: 0,
    },
    BlockDef {
        name: "cobble",
        solid: true,
        opaque: true,
        translucent: false,
        textures: [12; 6],
        emission: 0,
    },
    BlockDef {
        name: "torch",
        solid: true,
        opaque: true,
        translucent: false,
        textures: [13; 6],
        emission: 14,
    },
];

pub fn def(id: BlockId) -> &'static BlockDef {
    DEFS.get(id as usize).unwrap_or(&AIR_DEF)
}

pub const HOTBAR: [BlockId; 9] = [STONE, GRASS, SAND, WATER, LOG, PLANKS, GLASS, BRICK, TORCH];
