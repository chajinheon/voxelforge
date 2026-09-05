use super::*;

#[test]
fn shape_mask_counts_match_contract() {
    assert_eq!(shape_mask(ShapeKind::Slab { top: false }).count(), 2048);
    assert_eq!(shape_mask(ShapeKind::Slab { top: true }).count(), 2048);
    assert_eq!(
        shape_mask(ShapeKind::Stair {
            upside_down: false,
            facing: Facing::North,
        })
        .count(),
        3072
    );
    assert_eq!(shape_mask(ShapeKind::Pane { connections: 0 }).count(), 64);
    assert_eq!(shape_mask(ShapeKind::Fence { connections: 0 }).count(), 256);
}

#[test]
fn shape_template_quads_cover_mask_surface() {
    let t = shape_template(ShapeKind::Slab { top: false });
    let surface: u32 = t.quads.iter().map(TemplateQuad::area).sum();
    assert_eq!(surface, 2 * 16 * 16 + 4 * 16 * 8);
}

#[test]
fn partial_neighbor_subtracts_only_covered_face_area() {
    assert_eq!(subtract_face_coverage(0xffff, 0x00ff).count_ones(), 8);
}

#[test]
fn micro_ao_reads_shape_occupancy() {
    let m = ShapeMask::from_box(0, 16, 0, 16, 0, 1);
    assert_eq!(micro_ao(&m, Face::NegZ, 0, 0), 3);
    assert_eq!(micro_ao(&m, Face::PosZ, 0, 0), 0);
}

#[test]
fn stair_collision_matches_orientation() {
    let n = collision_boxes(
        ShapeKind::Stair {
            upside_down: false,
            facing: Facing::North,
        },
        &shape_mask(ShapeKind::Stair {
            upside_down: false,
            facing: Facing::North,
        }),
    );
    let e = collision_boxes(
        ShapeKind::Stair {
            upside_down: false,
            facing: Facing::East,
        },
        &shape_mask(ShapeKind::Stair {
            upside_down: false,
            facing: Facing::East,
        }),
    );
    assert_ne!(n.boxes[1], e.boxes[1]);
}

#[test]
fn fence_collision_height_is_one_point_five() {
    let c = collision_boxes(
        ShapeKind::Fence { connections: 0 },
        &shape_mask(ShapeKind::Fence { connections: 0 }),
    );
    assert_eq!(c.boxes[0].max[1], 24);
}

#[test]
fn pane_collision_keeps_disconnected_space_empty() {
    let c = collision_boxes(
        ShapeKind::Pane { connections: 1 },
        &shape_mask(ShapeKind::Pane { connections: 1 }),
    );
    assert_eq!(c.len, 2);
    assert_eq!(c.boxes[1].min, [7, 0, 0]);
    assert_eq!(c.boxes[1].max, [9, 16, 7]);
}
