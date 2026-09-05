use super::{
    ITEM_ICON_SIZE, bake_item_icons, catalog_records, draw_item_icon, item_icon_memory_bytes,
};

#[test]
fn catalog_records_cover_all_items_in_stable_order() {
    let records = catalog_records();
    assert_eq!(records.len(), 122);
    assert_eq!(records.first().map(|item| item.id), Some(1));
    assert_eq!(records.last().map(|item| item.id), Some(122));
}

#[test]
fn item_icon_bake_has_122_layers() {
    let layers = bake_item_icons();
    assert_eq!(layers.len(), 122);
    assert_eq!(
        layers.iter().map(Vec::len).sum::<usize>(),
        item_icon_memory_bytes()
    );
}

#[test]
fn item_icon_alpha_background_is_clear() {
    for layer in bake_item_icons() {
        let zero = layer.chunks_exact(4).filter(|pixel| pixel[3] == 0).count();
        let ratio = zero as f32 / (ITEM_ICON_SIZE * ITEM_ICON_SIZE) as f32;
        assert!((0.30..=0.80).contains(&ratio), "alpha-zero ratio {ratio}");
        let mut bounds = [ITEM_ICON_SIZE, ITEM_ICON_SIZE, 0, 0];
        for (index, pixel) in layer.chunks_exact(4).enumerate() {
            if pixel[3] > 0 {
                let x = index % ITEM_ICON_SIZE;
                let y = index / ITEM_ICON_SIZE;
                bounds[0] = bounds[0].min(x);
                bounds[1] = bounds[1].min(y);
                bounds[2] = bounds[2].max(x);
                bounds[3] = bounds[3].max(y);
            }
        }
        let width = bounds[2] - bounds[0] + 1;
        let height = bounds[3] - bounds[1] + 1;
        assert!((20..=60).contains(&width), "icon bbox width {width}");
        assert!((20..=60).contains(&height), "icon bbox height {height}");
    }
}

#[test]
fn item_icon_draw_is_one_textured_quad() {
    let mut vertices = Vec::new();
    draw_item_icon(&mut vertices, 1280, 720, 10.0, 20.0, 48.0, 101);
    assert_eq!(vertices.len(), 6);
    assert!(vertices.iter().all(|vertex| vertex.icon_layer == 100.0));
    assert_eq!(vertices[0].icon_uv, [0.0, 0.0]);
    assert_eq!(vertices[2].icon_uv, [1.0, 1.0]);
}
