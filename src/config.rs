use anyhow::{Context, Result};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use winit::event::MouseButton;
use winit::keyboard::KeyCode;
pub const SETTINGS_VERSION: u32 = 2;
const HOTBAR_DEFAULT: [u16; 9] = [4, 5, 14, 35, 63, 89, 101, 113, 74];
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformancePreset {
    Performance,
    Balanced,
    Quality,
    #[default]
    M5AirHigh,
    Custom,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputBinding {
    Key(KeyCode),
    Mouse(MouseButton),
}
impl Serialize for InputBinding {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&binding_name(*self))
    }
}
impl<'de> Deserialize<'de> for InputBinding {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BindingVisitor;
        impl<'de> Visitor<'de> for BindingVisitor {
            type Value = InputBinding;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a physical key or mouse binding name")
            }
            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                parse_binding(value).ok_or_else(|| E::custom(format!("unknown binding {value}")))
            }
        }
        deserializer.deserialize_str(BindingVisitor)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VideoSettings {
    pub preset: PerformancePreset,
    pub render_scale: f32,
    pub view_radius: u32,
    pub taa: bool,
    pub sharpen: f32,
    pub shadow_resolution: u32,
    pub shadow_distance: f32,
    pub pom_steps: u32,
    pub ssr_steps: u32,
    pub volumetric_steps: u32,
    pub cloud_view_steps: u32,
    pub cloud_light_steps: u32,
    pub gi_enabled: bool,
    pub gi_rays: u32,
    pub gi_distance: f32,
    pub vsync: bool,
    pub shader_pack: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControlSettings {
    pub mouse_sensitivity: f32,
    pub invert_y: bool,
    pub bindings: BTreeMap<String, InputBinding>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioSettings {
    pub master: f32,
    pub effects: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UiSettings {
    pub scale: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreativeSettings {
    pub hotbar: [u16; 9],
    pub selected_slot: u8,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub version: u32,
    pub video: VideoSettings,
    pub controls: ControlSettings,
    pub audio: AudioSettings,
    pub ui: UiSettings,
    pub creative: CreativeSettings,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            video: VideoSettings {
                preset: PerformancePreset::M5AirHigh,
                render_scale: 0.72,
                view_radius: 10,
                taa: true,
                sharpen: 0.18,
                shadow_resolution: 2048,
                shadow_distance: 224.0,
                pom_steps: 8,
                ssr_steps: 40,
                volumetric_steps: 32,
                cloud_view_steps: 40,
                cloud_light_steps: 6,
                gi_enabled: true,
                gi_rays: 4,
                gi_distance: 48.0,
                vsync: true,
                shader_pack: "builtin".to_string(),
            },
            controls: ControlSettings {
                mouse_sensitivity: 0.002,
                invert_y: false,
                bindings: default_bindings(),
            },
            audio: AudioSettings {
                master: 0.8,
                effects: 0.8,
            },
            ui: UiSettings { scale: 1.0 },
            creative: CreativeSettings {
                hotbar: HOTBAR_DEFAULT,
                selected_slot: 0,
            },
        }
    }
}
impl Settings {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(error).with_context(|| format!("read settings {}", path.display()));
            }
        };
        let mut value: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(error) => {
                preserve_file(path, "invalid");
                log::warn!("settings parse failed ({error}); using defaults");
                return Ok(Self::default());
            }
        };
        let version = value.get("version").and_then(serde_json::Value::as_u64);
        match version {
            Some(1) => {
                migrate_v1(&mut value);
                normalize_bindings(&mut value);
                let mut settings: Self =
                    serde_json::from_value(value).context("decode migrated settings")?;
                settings.clamp_and_warn();
                settings.write_atomic(path)?;
                Ok(settings)
            }
            Some(version) if version == SETTINGS_VERSION as u64 => {
                let defaults =
                    serde_json::to_value(Self::default()).context("encode settings defaults")?;
                merge_defaults(&mut value, &defaults);
                normalize_bindings(&mut value);
                match serde_json::from_value::<Self>(value) {
                    Ok(mut settings) => {
                        settings.clamp_and_warn();
                        Ok(settings)
                    }
                    Err(error) => {
                        preserve_file(path, "invalid");
                        log::warn!("settings decode failed ({error}); using defaults");
                        Ok(Self::default())
                    }
                }
            }
            Some(_) => {
                preserve_file(path, "unsupported");
                log::warn!("unsupported settings version; using defaults");
                Ok(Self::default())
            }
            None => {
                preserve_file(path, "invalid");
                log::warn!("settings version is missing; using defaults");
                Ok(Self::default())
            }
        }
    }
    pub fn clamp_and_warn(&mut self) {
        clamp_f32(&mut self.video.render_scale, 0.50, 1.00, "render_scale");
        clamp_u32(&mut self.video.view_radius, 4, 16, "view_radius");
        clamp_f32(&mut self.video.sharpen, 0.0, 1.0, "sharpen");
        let old_shadow = self.video.shadow_resolution;
        self.video.shadow_resolution = nearest_shadow_resolution(old_shadow);
        if self.video.shadow_resolution != old_shadow {
            log::warn!(
                "clamped shadow_resolution from {old_shadow} to {}",
                self.video.shadow_resolution
            );
        }
        clamp_f32(
            &mut self.video.shadow_distance,
            128.0,
            256.0,
            "shadow_distance",
        );
        clamp_u32(&mut self.video.pom_steps, 1, 32, "pom_steps");
        clamp_u32(&mut self.video.ssr_steps, 16, 64, "ssr_steps");
        clamp_u32(&mut self.video.volumetric_steps, 16, 64, "volumetric_steps");
        clamp_u32(&mut self.video.cloud_view_steps, 16, 64, "cloud_view_steps");
        clamp_u32(
            &mut self.video.cloud_light_steps,
            1,
            16,
            "cloud_light_steps",
        );
        clamp_u32(&mut self.video.gi_rays, 1, 6, "gi_rays");
        clamp_f32(&mut self.video.gi_distance, 16.0, 64.0, "gi_distance");
        clamp_f32(
            &mut self.controls.mouse_sensitivity,
            0.0005,
            0.0100,
            "mouse_sensitivity",
        );
        clamp_f32(&mut self.audio.master, 0.0, 1.0, "master");
        clamp_f32(&mut self.audio.effects, 0.0, 1.0, "effects");
        clamp_f32(&mut self.ui.scale, 0.75, 1.50, "ui.scale");
        self.creative.selected_slot = self.creative.selected_slot.min(8);
        self.version = SETTINGS_VERSION;
        for (action, default) in default_bindings() {
            self.controls.bindings.entry(action).or_insert(default);
        }
        self.controls
            .bindings
            .insert("pause".into(), InputBinding::Key(KeyCode::Escape));
    }
    pub fn write_atomic(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create settings directory {}", parent.display()))?;
        }
        let temp = PathBuf::from(format!("{}.tmp", path.display()));
        let result = (|| -> Result<()> {
            let bytes = serde_json::to_vec_pretty(self).context("serialize settings")?;
            let mut file =
                File::create(&temp).with_context(|| format!("create {}", temp.display()))?;
            file.write_all(&bytes).context("write settings")?;
            file.flush().context("flush settings")?;
            file.sync_all().context("sync settings")?;
            fs::rename(&temp, path).with_context(|| format!("rename {}", temp.display()))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
}
pub fn default_settings_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home).join("Library/Application Support/Voxelforge/settings.json")
    })
}
pub fn settings_path(cli_path: Option<&Path>) -> Option<PathBuf> {
    cli_path
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("VF_SETTINGS").map(PathBuf::from))
        .or_else(default_settings_path)
}
fn default_bindings() -> BTreeMap<String, InputBinding> {
    BTreeMap::from([
        ("backward".into(), InputBinding::Key(KeyCode::KeyS)),
        ("debug_view".into(), InputBinding::Key(KeyCode::F4)),
        ("descend".into(), InputBinding::Key(KeyCode::ShiftLeft)),
        ("forward".into(), InputBinding::Key(KeyCode::KeyW)),
        ("inventory".into(), InputBinding::Key(KeyCode::KeyI)),
        ("jump".into(), InputBinding::Key(KeyCode::Space)),
        ("left".into(), InputBinding::Key(KeyCode::KeyA)),
        ("pause".into(), InputBinding::Key(KeyCode::Escape)),
        (
            "pick_block".into(),
            InputBinding::Mouse(MouseButton::Middle),
        ),
        ("right".into(), InputBinding::Key(KeyCode::KeyD)),
        ("screenshot".into(), InputBinding::Key(KeyCode::F2)),
        ("shader_reload".into(), InputBinding::Key(KeyCode::KeyR)),
        ("sprint".into(), InputBinding::Key(KeyCode::ControlLeft)),
        ("toggle_fly".into(), InputBinding::Key(KeyCode::KeyF)),
    ])
}
fn binding_name(binding: InputBinding) -> String {
    match binding {
        InputBinding::Key(key) => format!("{key:?}"),
        InputBinding::Mouse(mouse) => format!("Mouse{mouse:?}"),
    }
}
fn parse_binding(name: &str) -> Option<InputBinding> {
    const KEYS: &[(&str, KeyCode)] = &[
        ("KeyI", KeyCode::KeyI),
        ("KeyW", KeyCode::KeyW),
        ("KeyS", KeyCode::KeyS),
        ("KeyA", KeyCode::KeyA),
        ("KeyD", KeyCode::KeyD),
        ("KeyF", KeyCode::KeyF),
        ("KeyR", KeyCode::KeyR),
        ("Space", KeyCode::Space),
        ("ControlLeft", KeyCode::ControlLeft),
        ("ShiftLeft", KeyCode::ShiftLeft),
        ("Escape", KeyCode::Escape),
        ("F2", KeyCode::F2),
        ("F4", KeyCode::F4),
    ];
    if let Some((_, key)) = KEYS.iter().find(|(known, _)| *known == name) {
        return Some(InputBinding::Key(*key));
    }
    [
        ("MouseLeft", MouseButton::Left),
        ("MouseRight", MouseButton::Right),
        ("MouseMiddle", MouseButton::Middle),
        ("MouseBack", MouseButton::Back),
        ("MouseForward", MouseButton::Forward),
    ]
    .into_iter()
    .find_map(|(known, mouse)| (known == name).then_some(InputBinding::Mouse(mouse)))
}
fn migrate_v1(value: &mut serde_json::Value) {
    let defaults =
        serde_json::to_value(Settings::default()).unwrap_or_else(|_| serde_json::json!({}));
    merge_defaults(value, &defaults);
    value["version"] = serde_json::json!(SETTINGS_VERSION);
}
fn merge_defaults(value: &mut serde_json::Value, defaults: &serde_json::Value) {
    if let (Some(current), Some(defaults)) = (value.as_object_mut(), defaults.as_object()) {
        for (key, default) in defaults {
            match current.get_mut(key) {
                Some(existing) => merge_defaults(existing, default),
                None => {
                    current.insert(key.clone(), default.clone());
                }
            }
        }
    }
}
fn normalize_bindings(value: &mut serde_json::Value) {
    let Some(bindings) = value
        .get_mut("controls")
        .and_then(|v| v.get_mut("bindings"))
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    let known = default_bindings();
    bindings.retain(|action, _| known.contains_key(action));
    for (action, binding) in bindings.iter_mut() {
        let valid = binding.as_str().and_then(parse_binding);
        if valid.is_none()
            && let Some(default) = known.get(action)
        {
            *binding = serde_json::json!(binding_name(*default));
            log::warn!("unknown binding for {action}; using default");
        }
    }
}
fn preserve_file(path: &Path, suffix: &str) {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let target = PathBuf::from(format!("{}.{}-{}", path.display(), suffix, stamp));
    if let Err(error) = fs::rename(path, &target) {
        log::warn!(
            "could not preserve settings as {}: {error}",
            target.display()
        );
    }
}
fn nearest_shadow_resolution(value: u32) -> u32 {
    [1024, 1536, 2048]
        .into_iter()
        .min_by_key(|candidate| value.abs_diff(*candidate))
        .unwrap_or(1536)
}
fn clamp_f32(value: &mut f32, min: f32, max: f32, name: &str) {
    let old = *value;
    if !value.is_finite() {
        *value = min;
    } else {
        *value = value.clamp(min, max);
    }
    if *value != old {
        log::warn!("clamped {name} from {old} to {}", *value);
    }
}
fn clamp_u32(value: &mut u32, min: u32, max: u32, name: &str) {
    let old = *value;
    *value = (*value).clamp(min, max);
    if *value != old {
        log::warn!("clamped {name} from {old} to {}", *value);
    }
}
#[cfg(test)]
#[path = "config_test.rs"]
mod tests;
