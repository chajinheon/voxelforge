use super::*;
use voxelforge::world::block::STONE;

#[test]
fn snapshot_noop_edit_is_warning() {
    let mut world = World::new(DEFAULT_SEED);
    let position = IVec3::new(0, 0, 0);
    world.ensure_loaded(chunk_of(position));
    let existing = world.get_block(position);
    assert_eq!(existing, STONE);

    let outcome = apply_edit(
        &mut world,
        Edit {
            position,
            id: existing,
        },
    )
    .expect("same-ID edit should be accepted as a warning");
    let EditOutcome::Noop(warning) = outcome else {
        panic!("same-ID edit should return its warning outcome");
    };
    assert!(warning.contains("IVec3(0, 0, 0)"));
    assert!(warning.contains("existing id 1"));
}

#[test]
fn snapshot_fixture_and_view_flags_parse() {
    let options = parse_args([
        "--fixture".into(),
        "m6-light-room".into(),
        "--view".into(),
        "light".into(),
        "--day-phase".into(),
        "0.25".into(),
        "--edits".into(),
        "set 0,0,0,12".into(),
    ])
    .expect("fixture flags should parse");
    assert_eq!(options.fixture, Fixture::LightRoom);
    assert_eq!(options.view, View::Light);
    assert_eq!(options.day_phase, Some(0.25));
    assert_eq!(options.world_time, None);
    assert_eq!(options.edits[0].id, TORCH);
}

#[test]
fn snapshot_rejects_two_time_controls() {
    let result = parse_args([
        "--day-phase".into(),
        "0.25".into(),
        "--world-time".into(),
        "10".into(),
    ]);
    let error = match result {
        Ok(_) => panic!("day phase and world time must be exclusive"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("mutually exclusive"));
}

#[test]
fn snapshot_m7_to_m10_fixtures_and_views_parse() {
    for fixture in [
        "m7-taau",
        "m7-passes",
        "m8-water",
        "m8-godrays",
        "m8-clouds",
        "m8-lod",
        "m9-gi-room",
        "m9-clipmap",
        "m10-shapes",
        "m10-inventory",
        "m10-viewmodel",
    ] {
        assert!(
            Fixture::parse(fixture).is_ok(),
            "fixture should parse: {fixture}"
        );
    }
    for view in ["water", "volumetric", "cloud", "lod", "gi", "clipmap"] {
        assert!(View::parse(view).is_ok(), "view should parse: {view}");
    }
}

#[test]
fn snapshot_ui_inventory_gi_and_hand_flags_parse() {
    let options = parse_args([
        "--fixture".into(),
        "m10-viewmodel".into(),
        "--ui".into(),
        "inventory".into(),
        "--inventory-category".into(),
        "shapes".into(),
        "--inventory-query".into(),
        "stair".into(),
        "--gi".into(),
        "on".into(),
        "--hand-action".into(),
        "break:0.13".into(),
        "--held-item".into(),
        "101".into(),
    ])
    .expect("M10 snapshot flags should parse");
    assert_eq!(options.ui, args::UiMode::Inventory);
    assert_eq!(options.inventory_category, args::InventoryCategory::Shapes);
    assert_eq!(options.inventory_query, "stair");
    assert_eq!(options.gi, args::GiMode::On);
    assert_eq!(options.held_item, 101);
    assert_eq!(
        options.hand_action,
        args::HandActionSpec::Break { elapsed: 0.13 }
    );
}

#[test]
fn snapshot_rejects_invalid_new_flags() {
    for (flag, value) in [
        ("--gi", "maybe"),
        ("--ui", "menu"),
        ("--inventory-category", "unknown"),
        ("--hand-action", "break:-1"),
        ("--held-item", "123"),
    ] {
        let result = parse_args([flag.into(), value.into()]);
        assert!(result.is_err(), "invalid {flag} should be rejected");
    }
}

#[test]
fn snapshot_timing_values_report_real_statistics() {
    let value = timing_value(&[1.0, 2.0, 4.0]);
    assert_eq!(value["max_ms"], 4.0);
    assert_eq!(value["median_ms"], 2.0);
    assert_eq!(value["p95_ms"], 2.0);

    let unavailable = timing_value(&[]);
    assert!(unavailable["max_ms"].is_null());
    assert!(unavailable["median_ms"].is_null());
}

#[test]
fn snapshot_sparse_gpu_samples_keep_post_warmup_frames() {
    let first = voxelforge::render::gpu_timing::GpuFrameTimings::default();
    let second = voxelforge::render::gpu_timing::GpuFrameTimings::default();
    let samples = vec![(2, first), (9, second)];
    let retained = post_warmup_gpu_frames(5, &samples);
    assert_eq!(retained.len(), 1);
}
