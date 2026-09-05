//! Deterministic 1/16-block shape geometry.  This module is deliberately
//! independent of the block registry so meshing and collision can share it.

pub const SUBVOXEL: i32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShapeMask {
    pub bits: [u64; 64],
}

impl ShapeMask {
    pub const EMPTY: Self = Self { bits: [0; 64] };
    pub fn full() -> Self {
        Self::from_box(0, 16, 0, 16, 0, 16)
    }
    pub fn from_box(x0: u8, x1: u8, y0: u8, y1: u8, z0: u8, z1: u8) -> Self {
        let mut out = Self::EMPTY;
        for y in y0..y1 {
            for z in z0..z1 {
                for x in x0..x1 {
                    out.set(x, y, z, true);
                }
            }
        }
        out
    }
    #[inline]
    pub fn index(x: u8, y: u8, z: u8) -> usize {
        x as usize + 16 * (z as usize + 16 * y as usize)
    }
    pub fn get(&self, x: u8, y: u8, z: u8) -> bool {
        if x >= 16 || y >= 16 || z >= 16 {
            return false;
        }
        let i = Self::index(x, y, z);
        (self.bits[i >> 6] >> (i & 63)) & 1 != 0
    }
    pub fn set(&mut self, x: u8, y: u8, z: u8, value: bool) {
        if x >= 16 || y >= 16 || z >= 16 {
            return;
        }
        let i = Self::index(x, y, z);
        let bit = 1u64 << (i & 63);
        if value {
            self.bits[i >> 6] |= bit;
        } else {
            self.bits[i >> 6] &= !bit;
        }
    }
    pub fn count(&self) -> u32 {
        self.bits.iter().map(|v| v.count_ones()).sum()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Cube,
    Slab { top: bool },
    Stair { upside_down: bool, facing: Facing },
    Pane { connections: u8 },
    Fence { connections: u8 },
    Log { axis: Axis },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facing {
    North,
    East,
    South,
    West,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    NegX,
    PosX,
    NegY,
    PosY,
    NegZ,
    PosZ,
}

/// Resolve every persisted shape state, including the legacy Y-axis log IDs.
/// Keeping this mapping beside the geometry prevents meshing, picking, and
/// collision from drifting apart.
pub fn shape_kind(id: u16) -> ShapeKind {
    match id {
        6 | 38..=40 => ShapeKind::Log { axis: Axis::Y },
        128 | 130 | 132 | 134 => ShapeKind::Log { axis: Axis::X },
        129 | 131 | 133 | 135 => ShapeKind::Log { axis: Axis::Z },
        256..=279 => ShapeKind::Slab {
            top: (id - 256) % 2 == 1,
        },
        512..=607 => {
            let state = (id - 512) % 8;
            ShapeKind::Stair {
                upside_down: state & 4 != 0,
                facing: [Facing::North, Facing::East, Facing::South, Facing::West]
                    [(state & 3) as usize],
            }
        }
        768..=863 => ShapeKind::Pane {
            connections: ((id - 768) % 16) as u8,
        },
        896..=959 => ShapeKind::Fence {
            connections: ((id - 896) % 16) as u8,
        },
        _ => ShapeKind::Cube,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Aabb {
    pub min: [u8; 3],
    pub max: [u8; 3],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmallAabbList {
    pub boxes: [Aabb; 12],
    pub len: u8,
}
impl SmallAabbList {
    pub const EMPTY: Self = Self {
        boxes: [Aabb {
            min: [0; 3],
            max: [0; 3],
        }; 12],
        len: 0,
    };
    fn push(&mut self, aabb: Aabb) {
        if (self.len as usize) < self.boxes.len() {
            self.boxes[self.len as usize] = aabb;
            self.len += 1;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TemplateQuad {
    pub face: Face,
    pub min: [u8; 3],
    pub max: [u8; 3],
}
impl TemplateQuad {
    pub fn area(&self) -> u32 {
        let d = [0, 1, 2];
        let axis = self.face as usize / 2;
        d.iter()
            .filter(|&&a| a != axis)
            .map(|&a| (self.max[a] - self.min[a]) as u32)
            .product()
    }
}

pub struct ShapeTemplate {
    pub mask: ShapeMask,
    pub quads: Vec<TemplateQuad>,
    pub boundary: [[u16; 16]; 6],
    pub collision: SmallAabbList,
}

fn rotate_facing(mask: &mut ShapeMask, facing: Facing) {
    if facing == Facing::North {
        return;
    }
    let old = *mask;
    *mask = ShapeMask::EMPTY;
    for y in 0..16 {
        for z in 0..16 {
            for x in 0..16 {
                if !old.get(x, y, z) {
                    continue;
                }
                let (nx, nz) = match facing {
                    Facing::East => (15 - z, x),
                    Facing::South => (15 - x, 15 - z),
                    Facing::West => (z, 15 - x),
                    Facing::North => (x, z),
                };
                mask.set(nx, y, nz, true);
            }
        }
    }
}

pub fn shape_mask(kind: ShapeKind) -> ShapeMask {
    let mut m = match kind {
        ShapeKind::Cube => ShapeMask::full(),
        ShapeKind::Slab { top: false } => ShapeMask::from_box(0, 16, 0, 8, 0, 16),
        ShapeKind::Slab { top: true } => ShapeMask::from_box(0, 16, 8, 16, 0, 16),
        ShapeKind::Stair {
            upside_down: false,
            facing: _,
        } => {
            let mut a = ShapeMask::from_box(0, 16, 0, 8, 0, 16);
            for y in 8..16 {
                for z in 0..8 {
                    for x in 0..16 {
                        a.set(x, y, z, true);
                    }
                }
            }
            a
        }
        ShapeKind::Stair {
            upside_down: true,
            facing: _,
        } => {
            let mut a = ShapeMask::from_box(0, 16, 8, 16, 0, 16);
            for y in 0..8 {
                for z in 0..8 {
                    for x in 0..16 {
                        a.set(x, y, z, true);
                    }
                }
            }
            a
        }
        ShapeKind::Pane { connections } => pane_mask(connections, false),
        ShapeKind::Fence { connections } => pane_mask(connections, true),
        ShapeKind::Log { axis: Axis::Y } => ShapeMask::from_box(6, 10, 0, 16, 6, 10),
        ShapeKind::Log { axis: Axis::X } => ShapeMask::from_box(0, 16, 6, 10, 6, 10),
        ShapeKind::Log { axis: Axis::Z } => ShapeMask::from_box(6, 10, 6, 10, 0, 16),
    };
    if let ShapeKind::Stair { facing, .. } = kind {
        rotate_facing(&mut m, facing);
    }
    m
}

fn pane_mask(connections: u8, fence: bool) -> ShapeMask {
    let (lo, hi) = if fence { (6, 10) } else { (7, 9) };
    let mut m = ShapeMask::from_box(lo, hi, 0, 16, lo, hi);
    let arm_lo = 0;
    let arm_hi = if fence { 6 } else { 7 };
    if connections & 1 != 0 {
        let y_ranges: &[(u8, u8)] = if fence {
            &[(5, 8), (11, 14)]
        } else {
            &[(0, 16)]
        };
        for &(y0, y1) in y_ranges {
            for y in y0..y1 {
                for z in arm_lo..arm_hi {
                    for x in lo..hi {
                        m.set(x, y, z, true);
                    }
                }
            }
        }
    }
    if connections & 2 != 0 {
        m = union(m, ShapeMask::from_box(hi, 16, 0, 16, lo, hi));
    }
    if connections & 4 != 0 {
        m = union(m, ShapeMask::from_box(lo, hi, 0, 16, hi, 16));
    }
    if connections & 8 != 0 {
        m = union(m, ShapeMask::from_box(0, lo, 0, 16, lo, hi));
    }
    m
}
fn union(mut a: ShapeMask, b: ShapeMask) -> ShapeMask {
    for i in 0..64 {
        a.bits[i] |= b.bits[i];
    }
    a
}

pub fn shape_template(kind: ShapeKind) -> ShapeTemplate {
    let mask = shape_mask(kind);
    let mut boundary = [[0u16; 16]; 6];
    for row in 0..16 {
        for col in 0..16 {
            let samples = [
                (col, row, 0),
                (col, row, 15),
                (0, row, col),
                (15, row, col),
                (col, 0, row),
                (col, 15, row),
            ];
            for (i, &(x, y, z)) in samples.iter().enumerate() {
                if mask.get(x, y, z) {
                    boundary[i][row as usize] |= 1 << col;
                }
            }
        }
    }
    let mut quads = Vec::new();
    for face in [
        Face::NegX,
        Face::PosX,
        Face::NegY,
        Face::PosY,
        Face::NegZ,
        Face::PosZ,
    ] {
        for plane in 0..=16 {
            let mut exposed = [[false; 16]; 16];
            for v in 0..16 {
                for u in 0..16 {
                    let (p, n) = face_cell_plane(face, plane, u, v);
                    exposed[v as usize][u as usize] =
                        mask.get(p[0], p[1], p[2]) && !mask.get(n[0], n[1], n[2]);
                }
            }
            for v in 0..16 {
                for u in 0..16 {
                    if !exposed[v as usize][u as usize] {
                        continue;
                    }
                    let mut width = 1;
                    while u + width < 16 && exposed[v as usize][(u + width) as usize] {
                        width += 1;
                    }
                    let mut height = 1;
                    'grow: while v + height < 16 {
                        for x in u..u + width {
                            if !exposed[(v + height) as usize][x as usize] {
                                break 'grow;
                            }
                        }
                        height += 1;
                    }
                    for row in v..v + height {
                        for col in u..u + width {
                            exposed[row as usize][col as usize] = false;
                        }
                    }
                    let (min, max) = face_rect_plane(face, plane, u, v, width, height);
                    quads.push(TemplateQuad { face, min, max });
                }
            }
        }
    }
    ShapeTemplate {
        mask,
        quads,
        boundary,
        collision: collision_boxes(kind, &mask),
    }
}

fn face_cell_plane(face: Face, plane: u8, u: u8, v: u8) -> ([u8; 3], [u8; 3]) {
    match face {
        Face::NegX => ([plane, v, u], [plane.wrapping_sub(1), v, u]),
        Face::PosX => ([plane.wrapping_sub(1), v, u], [plane, v, u]),
        Face::NegY => ([u, plane, v], [u, plane.wrapping_sub(1), v]),
        Face::PosY => ([u, plane.wrapping_sub(1), v], [u, plane, v]),
        Face::NegZ => ([u, v, plane], [u, v, plane.wrapping_sub(1)]),
        Face::PosZ => ([u, v, plane.wrapping_sub(1)], [u, v, plane]),
    }
}

fn face_rect_plane(
    face: Face,
    plane: u8,
    u: u8,
    v: u8,
    width: u8,
    height: u8,
) -> ([u8; 3], [u8; 3]) {
    match face {
        Face::NegX | Face::PosX => ([plane, v, u], [plane, v + height, u + width]),
        Face::NegY | Face::PosY => ([u, plane, v], [u + width, plane, v + height]),
        Face::NegZ | Face::PosZ => ([u, v, plane], [u + width, v + height, plane]),
    }
}

pub fn collision_boxes(kind: ShapeKind, mask: &ShapeMask) -> SmallAabbList {
    let mut out = SmallAabbList::EMPTY;
    if let ShapeKind::Pane { connections } = kind {
        out.push(Aabb {
            min: [7, 0, 7],
            max: [9, 16, 9],
        });
        let arms = [
            (1, [7, 0, 0], [9, 16, 7]),
            (2, [9, 0, 7], [16, 16, 9]),
            (4, [7, 0, 9], [9, 16, 16]),
            (8, [0, 0, 7], [7, 16, 9]),
        ];
        for (bit, min, max) in arms {
            if connections & bit != 0 {
                out.push(Aabb { min, max });
            }
        }
    } else if let ShapeKind::Fence { connections } = kind {
        out.push(Aabb {
            min: [6, 0, 6],
            max: [10, 24, 10],
        });
        let arms = [
            (1, [6, 0, 0], [10, 24, 6]),
            (2, [10, 0, 6], [16, 24, 10]),
            (4, [6, 0, 10], [10, 24, 16]),
            (8, [0, 0, 6], [6, 24, 10]),
        ];
        for (bit, min, max) in arms {
            if connections & bit != 0 {
                out.push(Aabb { min, max });
            }
        }
    } else if let ShapeKind::Stair {
        upside_down,
        facing,
    } = kind
    {
        out.push(Aabb {
            min: [0, if upside_down { 8 } else { 0 }, 0],
            max: [16, if upside_down { 16 } else { 8 }, 16],
        });
        let (x0, x1, z0, z1) = match facing {
            Facing::North => (0, 16, 0, 8),
            Facing::East => (8, 16, 0, 16),
            Facing::South => (0, 16, 8, 16),
            Facing::West => (0, 8, 0, 16),
        };
        out.push(Aabb {
            min: [x0, if upside_down { 0 } else { 8 }, z0],
            max: [x1, if upside_down { 8 } else { 16 }, z1],
        });
    } else {
        let mut min = [16; 3];
        let mut max = [0; 3];
        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    if mask.get(x, y, z) {
                        min = [min[0].min(x), min[1].min(y), min[2].min(z)];
                        max = [max[0].max(x + 1), max[1].max(y + 1), max[2].max(z + 1)];
                    }
                }
            }
        }
        if max != [0; 3] {
            out.push(Aabb { min, max });
        }
    }
    out
}

pub fn subtract_face_coverage(own: u16, neighbor: u16) -> u16 {
    own & !neighbor
}
pub fn micro_ao(mask: &ShapeMask, face: Face, row: u8, col: u8) -> u8 {
    let (x, y, z) = match face {
        Face::NegX => (0, row, col),
        Face::PosX => (15, row, col),
        Face::NegY => (col, 0, row),
        Face::PosY => (col, 15, row),
        Face::NegZ => (col, row, 0),
        Face::PosZ => (col, row, 15),
    };
    let (a, b) = match face {
        Face::NegX | Face::PosX => ([0, 1, 0], [0, 0, 1]),
        Face::NegY | Face::PosY => ([1, 0, 0], [0, 0, 1]),
        _ => ([1, 0, 0], [0, 1, 0]),
    };
    let p = |v: [i32; 3]| {
        mask.get(
            (x as i32 + v[0]).clamp(0, 15) as u8,
            (y as i32 + v[1]).clamp(0, 15) as u8,
            (z as i32 + v[2]).clamp(0, 15) as u8,
        )
    };
    let s1 = p(a);
    let s2 = p(b);
    let c = p([a[0] + b[0], a[1] + b[1], a[2] + b[2]]);
    if s1 && s2 {
        3
    } else {
        s1 as u8 + s2 as u8 + c as u8
    }
}

#[cfg(test)]
#[path = "shape_tests.rs"]
mod tests;
