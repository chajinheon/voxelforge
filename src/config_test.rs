use super::*;
use std::fs;
fn test_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("voxelforge-{label}-{}.json", std::process::id()))
}
#[test]
fn settings_roundtrip_and_clamp() {
    let path = test_path("settings-roundtrip");
    let mut settings = Settings::default();
    settings.video.render_scale = 4.0;
    settings.audio.master = -1.0;
    settings.clamp_and_warn();
    settings.write_atomic(&path).expect("write settings");
    let loaded = Settings::load(&path).expect("load settings");
    assert_eq!(loaded.video.render_scale, 1.0);
    assert_eq!(loaded.audio.master, 0.0);
    let _ = fs::remove_file(path);
}

#[test]
fn video_quality_controls_clamp_to_shader_bounds() {
    let mut settings = Settings::default();
    settings.video.shadow_resolution = 999;
    settings.video.shadow_distance = 999.0;
    settings.video.pom_steps = 0;
    settings.video.ssr_steps = 999;
    settings.video.volumetric_steps = 0;
    settings.video.cloud_view_steps = 999;
    settings.video.cloud_light_steps = 0;
    settings.clamp_and_warn();
    assert_eq!(settings.video.shadow_resolution, 1024);
    assert_eq!(settings.video.shadow_distance, 256.0);
    assert_eq!(settings.video.pom_steps, 1);
    assert_eq!(settings.video.ssr_steps, 64);
    assert_eq!(settings.video.volumetric_steps, 16);
    assert_eq!(settings.video.cloud_view_steps, 64);
    assert_eq!(settings.video.cloud_light_steps, 1);
}
#[test]
fn settings_unknown_fields_are_ignored() {
    let path = test_path("settings-unknown");
    fs::write(&path, r#"{"version":2,"future":{"flag":true}}"#).expect("write");
    let settings = Settings::load(&path).expect("load");
    assert_eq!(settings.version, SETTINGS_VERSION);
    let _ = fs::remove_file(path);
}
#[test]
fn settings_atomic_write_leaves_no_temp_file() {
    let path = test_path("settings-atomic");
    Settings::default().write_atomic(&path).expect("write");
    assert!(!PathBuf::from(format!("{}.tmp", path.display())).exists());
    let _ = fs::remove_file(path);
}
#[test]
fn binding_names_roundtrip() {
    for binding in [
        InputBinding::Key(KeyCode::KeyI),
        InputBinding::Key(KeyCode::KeyW),
        InputBinding::Key(KeyCode::Space),
        InputBinding::Key(KeyCode::F2),
        InputBinding::Mouse(MouseButton::Middle),
    ] {
        let encoded = serde_json::to_string(&binding).expect("serialize binding");
        let decoded: InputBinding = serde_json::from_str(&encoded).expect("deserialize binding");
        assert_eq!(decoded, binding);
    }
}
#[test]
fn settings_v1_migrates_to_v2() {
    let path = test_path("settings-v1");
    fs::write(&path, r#"{"version":1,"video":{"preset":"balanced","render_scale":0.75},"controls":{"bindings":{"forward":"KeyW"}}}"#).expect("write v1");
    let settings = Settings::load(&path).expect("migrate");
    assert_eq!(settings.version, 2);
    assert_eq!(settings.video.render_scale, 0.75);
    assert_eq!(settings.creative.hotbar, HOTBAR_DEFAULT);
    let stored: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("read migrated")).expect("json");
    assert_eq!(stored["version"], 2);
    let _ = fs::remove_file(path);
}
#[test]
fn creative_hotbar_roundtrips() {
    let path = test_path("creative-hotbar");
    let mut settings = Settings::default();
    settings.creative.hotbar = [1, 2, 3, 4, 5, 6, 7, 8, 9];
    settings.creative.selected_slot = 8;
    settings.write_atomic(&path).expect("write");
    let loaded = Settings::load(&path).expect("load");
    assert_eq!(loaded.creative, settings.creative);
    let _ = fs::remove_file(path);
}
