use std::path::PathBuf;

use anyhow::Result;
use voxelforge::config::{PerformancePreset, Settings, settings_path};
use voxelforge::shaderpack::ShaderPack;

/// Command-line values that affect this process only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CliOptions {
    pub(crate) settings_path: Option<PathBuf>,
    pub(crate) preset: Option<PerformancePreset>,
    pub(crate) shader_pack: Option<String>,
    pub(crate) save_name: String,
    pub(crate) audio_self_test: Option<PathBuf>,
}

/// Values saved in settings before a transient CLI/environment override.
/// Keeping these originals lets normal UI changes persist without leaking a
/// one-shot launch override into settings.json.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RuntimeOverrides {
    pub(crate) preset: Option<PerformancePreset>,
    pub(crate) shader_pack: Option<String>,
}

impl RuntimeOverrides {
    pub(crate) fn restore_persisted(&self, settings: &mut Settings) {
        if let Some(preset) = self.preset {
            settings.video.preset = preset;
        }
        if let Some(shader_pack) = self.shader_pack.as_ref() {
            settings.video.shader_pack.clone_from(shader_pack);
        }
    }
}

pub(crate) fn parse_args<I>(args: I) -> Result<CliOptions>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let _program = args.next();
    let mut options = CliOptions {
        save_name: std::env::var("VF_SAVE").unwrap_or_else(|_| "default".to_string()),
        ..CliOptions::default()
    };
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--settings=") {
            options.settings_path = Some(non_empty_path("--settings", value)?);
        } else if arg == "--settings" {
            options.settings_path = Some(next_value(&mut args, "--settings")?.into());
        } else if let Some(value) = arg.strip_prefix("--preset=") {
            options.preset = Some(parse_preset(value)?);
        } else if arg == "--preset" {
            options.preset = Some(parse_preset(&next_value(&mut args, "--preset")?)?);
        } else if let Some(value) = arg.strip_prefix("--shader-pack=") {
            options.shader_pack = Some(non_empty_string("--shader-pack", value)?);
        } else if arg == "--shader-pack" {
            options.shader_pack = Some(next_value(&mut args, "--shader-pack")?);
        } else if let Some(value) = arg.strip_prefix("--save=") {
            options.save_name = non_empty_string("--save", value)?;
        } else if arg == "--save" {
            options.save_name = next_value(&mut args, "--save")?;
        } else if let Some(value) = arg.strip_prefix("--audio-self-test=") {
            options.audio_self_test = Some(non_empty_path("--audio-self-test", value)?);
        } else if arg == "--audio-self-test" {
            options.audio_self_test = Some(next_value(&mut args, "--audio-self-test")?.into());
        } else {
            anyhow::bail!("unknown argument {arg}");
        }
    }
    Ok(options)
}

pub(crate) fn resolve_settings(
    options: &CliOptions,
) -> Result<(Settings, Option<PathBuf>, RuntimeOverrides)> {
    let env_preset = std::env::var("VF_PRESET").ok();
    let env_shader_pack = std::env::var("VF_SHADERPACK")
        .ok()
        .map(|value| non_empty_string("VF_SHADERPACK", &value))
        .transpose()?;
    resolve_settings_with_env(options, env_preset.as_deref(), env_shader_pack.as_deref())
}

fn resolve_settings_with_env(
    options: &CliOptions,
    env_preset: Option<&str>,
    env_shader_pack: Option<&str>,
) -> Result<(Settings, Option<PathBuf>, RuntimeOverrides)> {
    let path = settings_path(options.settings_path.as_deref());
    let mut settings = path
        .as_deref()
        .map(Settings::load)
        .transpose()?
        .unwrap_or_default();

    let env_preset = env_preset.map(parse_preset).transpose()?;
    let preset = options.preset.or(env_preset);
    let shader_pack = options
        .shader_pack
        .clone()
        .or_else(|| env_shader_pack.map(str::to_owned));
    let overrides = RuntimeOverrides {
        preset: preset.map(|_| settings.video.preset),
        shader_pack: shader_pack
            .as_ref()
            .map(|_| settings.video.shader_pack.clone()),
    };
    if let Some(preset) = preset {
        settings.video.preset = preset;
    }
    if let Some(shader_pack) = shader_pack {
        settings.video.shader_pack = shader_pack;
    }
    settings.clamp_and_warn();
    Ok((settings, path, overrides))
}

pub(crate) fn load_shader_pack(
    settings: &Settings,
    _options: &CliOptions,
) -> Result<Option<ShaderPack>> {
    if settings.video.shader_pack == "builtin" {
        return Ok(None);
    }
    if !normal_component(&settings.video.shader_pack) {
        log::warn!(
            "shader pack rejected: invalid pack name {:?}; using builtin",
            settings.video.shader_pack
        );
        return Ok(None);
    }
    let packs = std::env::var_os("VF_SHADERPACKS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("shaderpacks"));
    let root = packs.join(&settings.video.shader_pack);
    match ShaderPack::load(&root) {
        Ok(pack) => Ok(Some(pack)),
        Err(error) => {
            log::warn!("shader pack rejected: {error}; using builtin");
            Ok(None)
        }
    }
}

fn parse_preset(value: &str) -> Result<PerformancePreset> {
    match value {
        "performance" => Ok(PerformancePreset::Performance),
        "balanced" => Ok(PerformancePreset::Balanced),
        "quality" => Ok(PerformancePreset::Quality),
        "m5_air_high" => Ok(PerformancePreset::M5AirHigh),
        _ => anyhow::bail!(
            "--preset must be performance, balanced, quality, or m5_air_high (got {value:?})"
        ),
    }
}

fn next_value<I>(args: &mut I, option: &str) -> Result<String>
where
    I: Iterator<Item = String>,
{
    let value = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("{option} requires a value"))?;
    if value.starts_with("--") {
        anyhow::bail!("{option} requires a value");
    }
    non_empty_string(option, &value)
}

fn non_empty_string(option: &str, value: &str) -> Result<String> {
    if value.is_empty() {
        anyhow::bail!("{option} requires a non-empty value");
    }
    Ok(value.to_string())
}

fn non_empty_path(option: &str, value: &str) -> Result<PathBuf> {
    non_empty_string(option, value).map(PathBuf::from)
}

fn normal_component(value: &str) -> bool {
    let mut components = std::path::Path::new(value).components();
    !value.is_empty()
        && !value.contains('/')
        && !value.contains('\\')
        && value != "."
        && value != ".."
        && matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<CliOptions> {
        parse_args(args.iter().map(|value| (*value).to_string()))
    }

    #[test]
    fn cli_accepts_split_and_equals_forms() {
        let options = parse(&[
            "voxelforge",
            "--settings",
            "/tmp/custom.json",
            "--preset=quality",
            "--shader-pack",
            "bright",
            "--save=creative",
            "--audio-self-test",
            "/tmp/test.wav",
        ])
        .expect("parse options");
        assert_eq!(
            options.settings_path,
            Some(PathBuf::from("/tmp/custom.json"))
        );
        assert_eq!(options.preset, Some(PerformancePreset::Quality));
        assert_eq!(options.shader_pack.as_deref(), Some("bright"));
        assert_eq!(options.save_name, "creative");
        assert_eq!(
            options.audio_self_test,
            Some(PathBuf::from("/tmp/test.wav"))
        );
    }

    #[test]
    fn cli_rejects_missing_and_invalid_values() {
        for args in [
            vec!["voxelforge", "--settings"],
            vec!["voxelforge", "--preset=cinematic"],
            vec!["voxelforge", "--shader-pack="],
            vec!["voxelforge", "--save="],
        ] {
            assert!(parse(&args).is_err(), "expected error for {args:?}");
        }
    }

    #[test]
    fn shader_pack_selector_cannot_escape_its_root() {
        for value in ["../pack", "packs/bright", "packs\\bright", ".", "..", ""] {
            assert!(!normal_component(value), "accepted {value:?}");
        }
        assert!(normal_component("bright"));
    }

    #[test]
    fn cli_overrides_are_recorded_without_mutating_original_values() {
        let path = std::env::temp_dir().join(format!("voxelforge-cli-{}.json", std::process::id()));
        let mut base = Settings::default();
        base.video.preset = PerformancePreset::Balanced;
        base.video.shader_pack = "builtin".to_string();
        base.write_atomic(&path).expect("write settings");
        let options = CliOptions {
            settings_path: Some(path.clone()),
            preset: Some(PerformancePreset::Performance),
            shader_pack: Some("bright".to_string()),
            ..Default::default()
        };
        let (settings, _, overrides) =
            resolve_settings_with_env(&options, Some("balanced"), Some("env-pack"))
                .expect("resolve settings");
        assert_eq!(settings.video.preset, PerformancePreset::Performance);
        assert_eq!(settings.video.shader_pack, "bright");
        assert_eq!(overrides.preset, Some(PerformancePreset::Balanced));
        assert_eq!(overrides.shader_pack.as_deref(), Some("builtin"));
        let mut persisted = settings;
        overrides.restore_persisted(&mut persisted);
        assert_eq!(persisted.video.preset, PerformancePreset::Balanced);
        assert_eq!(persisted.video.shader_pack, "builtin");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn environment_overrides_settings_but_cli_wins_over_environment() {
        let path =
            std::env::temp_dir().join(format!("voxelforge-precedence-{}.json", std::process::id()));
        let mut base = Settings::default();
        base.video.preset = PerformancePreset::Quality;
        base.write_atomic(&path).expect("write settings");
        let settings_only = CliOptions {
            settings_path: Some(path.clone()),
            ..Default::default()
        };
        let (env_settings, _, _) =
            resolve_settings_with_env(&settings_only, Some("balanced"), None)
                .expect("environment precedence");
        assert_eq!(env_settings.video.preset, PerformancePreset::Balanced);
        let cli = CliOptions {
            preset: Some(PerformancePreset::Performance),
            ..settings_only
        };
        let (cli_settings, _, _) =
            resolve_settings_with_env(&cli, Some("balanced"), None).expect("cli precedence");
        assert_eq!(cli_settings.video.preset, PerformancePreset::Performance);
        let _ = std::fs::remove_file(path);
    }
}
