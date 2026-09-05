use super::*;

#[test]
fn pass_order_and_labels_are_stable() {
    assert_eq!(TIMED_PASS_COUNT, 20);
    assert_eq!(TimedPass::ALL[0], TimedPass::Shadow);
    assert_eq!(TimedPass::ALL[19], TimedPass::Present);
    assert_eq!(
        TimedPass::ALL
            .iter()
            .map(|pass| pass.label())
            .collect::<Vec<_>>(),
        [
            "shadow",
            "gbuffer",
            "depth_pyramid",
            "gtao",
            "atmosphere",
            "deferred",
            "gi_trace",
            "gi_temporal",
            "gi_denoise",
            "volumetric",
            "clouds",
            "cloud_shadow",
            "glass",
            "water",
            "bloom",
            "exposure",
            "taau_tonemap",
            "viewmodel",
            "ui",
            "present",
        ]
    );
}

#[test]
fn pass_lookup_and_total_use_the_full_set() {
    let mut values = [0.0; TIMED_PASS_COUNT];
    values[TimedPass::Shadow.index()] = 1.0;
    values[TimedPass::Water.index()] = 2.0;
    values[TimedPass::Present.index()] = 3.0;
    let frame = GpuFrameTimings::from_passes(values, u32::MAX);
    assert_eq!(frame.ms(TimedPass::Shadow), 1.0);
    assert_eq!(frame.ms(TimedPass::Water), 2.0);
    assert_eq!(frame.total_ms(), 6.0);
    assert_eq!(frame.forward_ms, 2.0);
}

#[test]
fn legacy_forward_name_maps_to_glass_query() {
    assert_eq!(TimedPass::Forward, TimedPass::Glass);
    assert_eq!(TimedPass::Forward.index(), TimedPass::Glass.index());
}

#[test]
fn legacy_frame_literals_keep_total_behavior() {
    let frame = GpuFrameTimings {
        gbuffer_ms: 1.0,
        deferred_ms: 2.0,
        forward_ms: 3.0,
        present_ms: 4.0,
        ..GpuFrameTimings::default()
    };
    assert_eq!(frame.total_ms(), 10.0);
    assert_eq!(frame.ms(TimedPass::GBuffer), 1.0);
    assert_eq!(frame.ms(TimedPass::Forward), 3.0);
    assert_eq!(frame.ms(TimedPass::Present), 4.0);
}

#[test]
fn missing_samples_are_not_reported_as_real_timing() {
    let mut values = [0.0; TIMED_PASS_COUNT];
    values[TimedPass::GBuffer.index()] = 2.0;
    values[TimedPass::Water.index()] = 99.0;
    let frame = GpuFrameTimings::from_passes(values, 1_u32 << TimedPass::GBuffer.index());
    assert!(frame.has_sample(TimedPass::GBuffer));
    assert!(!frame.has_sample(TimedPass::Water));
    assert_eq!(frame.ms(TimedPass::Water), 0.0);
    assert_eq!(frame.total_ms(), 2.0);
}
