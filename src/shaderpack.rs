//! Contract-2 shader-pack manifest validation and transactional loading.
//!
//! The pack format deliberately carries no registry or geometry data. A pack
//! can replace source and texture assets only from the allow-list below.
use image::GenericImageView;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
pub const MANIFEST_VERSION: u32 = 1;
pub const CONTRACT_VERSION: u32 = 2;
// Contract-2 pass sources. UI and geometry registries remain deliberately
// outside this list. Every accepted source is compiled into the renderer's
// candidate bundle; pipeline-backed passes additionally replace their live
// pipeline at the transaction commit point.
const ALLOWED_SHADERS: &[&str] = &[
    "chunk.wgsl",
    "shadow.wgsl",
    "ssao.wgsl",
    "gtao.wgsl",
    "sky_lut.wgsl",
    "atmosphere.wgsl",
    "deferred.wgsl",
    "translucent.wgsl",
    "water.wgsl",
    "volumetric.wgsl",
    "clouds.wgsl",
    "cloud_shadow.wgsl",
    "glass.wgsl",
    "bloom.wgsl",
    "exposure.wgsl",
    "tonemap.wgsl",
    "present.wgsl",
    "gi_trace.wgsl",
    "gi_temporal.wgsl",
    "gi_denoise.wgsl",
    "gi_composite.wgsl",
    "taau_tonemap.wgsl",
];
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ShaderPackManifest {
    pub version: u32,
    pub contract_version: u32,
    pub name: String,
    pub author: String,
    #[serde(default)]
    pub shaders: Vec<String>,
    #[serde(default)]
    pub textures: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShaderPackError {
    Io(String),
    Manifest(String),
    UnsupportedContract(u32),
    InvalidPath(String),
    ForbiddenOverride(String),
    MissingAsset(String),
    InvalidTexture(String),
    Compile(String),
}
impl fmt::Display for ShaderPackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(value) => write!(f, "shader pack I/O error: {value}"),
            Self::Manifest(value) => write!(f, "shader pack manifest error: {value}"),
            Self::UnsupportedContract(version) => {
                write!(
                    f,
                    "shader pack contract {version} rejected; expected {CONTRACT_VERSION}"
                )
            }
            Self::InvalidPath(value) => write!(f, "shader pack invalid path: {value}"),
            Self::ForbiddenOverride(value) => {
                write!(f, "shader pack override is forbidden: {value}")
            }
            Self::MissingAsset(value) => write!(f, "shader pack asset is missing: {value}"),
            Self::InvalidTexture(value) => write!(f, "shader pack texture is invalid: {value}"),
            Self::Compile(value) => write!(f, "shader pack compile rejected: {value}"),
        }
    }
}
impl std::error::Error for ShaderPackError {}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Shader,
    BlockTexture,
    MaterialTexture,
    EmissionTexture,
    HandTexture,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAsset {
    pub kind: AssetKind,
    pub relative_path: String,
    pub path: PathBuf,
    pub from_pack: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureOverride {
    pub kind: AssetKind,
    pub relative_path: String,
    pub data: Vec<u8>,
}
#[derive(Debug, Clone)]
pub struct ShaderPack {
    root: PathBuf,
    manifest: ShaderPackManifest,
    shaders: BTreeSet<String>,
    shader_files: BTreeMap<String, String>,
    textures: BTreeSet<String>,
}
impl ShaderPackManifest {
    pub fn parse(json: &str) -> Result<Self, ShaderPackError> {
        let manifest: Self = serde_json::from_str(json)
            .map_err(|error| ShaderPackError::Manifest(error.to_string()))?;
        if manifest.version != MANIFEST_VERSION {
            return Err(ShaderPackError::Manifest(format!(
                "version {} is unsupported; expected {MANIFEST_VERSION}",
                manifest.version
            )));
        }
        if manifest.contract_version != CONTRACT_VERSION {
            return Err(ShaderPackError::UnsupportedContract(
                manifest.contract_version,
            ));
        }
        validate_component(&manifest.name)?;
        for path in &manifest.shaders {
            canonical_shader_path(path)?;
        }
        for path in &manifest.textures {
            canonical_texture_path(path)?;
        }
        Ok(manifest)
    }
}
impl ShaderPack {
    pub fn load(root: impl AsRef<Path>) -> Result<Self, ShaderPackError> {
        let root = root.as_ref().to_path_buf();
        let manifest_path = root.join("pack.json");
        let json = fs::read_to_string(&manifest_path).map_err(|error| {
            ShaderPackError::Io(format!("{}: {error}", manifest_path.display()))
        })?;
        let manifest = ShaderPackManifest::parse(&json)?;
        let mut shader_files = BTreeMap::new();
        for declared in &manifest.shaders {
            let declared_path = declared_shader_path(declared)?;
            let canonical = canonical_shader_path(declared)?;
            if shader_files
                .insert(canonical.clone(), declared_path)
                .is_some()
            {
                return Err(ShaderPackError::Manifest(format!(
                    "multiple sources target {canonical}"
                )));
            }
        }
        let shaders = shader_files.keys().cloned().collect::<BTreeSet<_>>();
        let textures = manifest
            .textures
            .iter()
            .map(|path| canonical_texture_path(path))
            .collect::<Result<BTreeSet<_>, _>>()?;
        for relative in textures.iter() {
            let path = root.join(relative);
            if !path.is_file() {
                return Err(ShaderPackError::MissingAsset(relative.clone()));
            }
        }
        for relative in shader_files.values() {
            let path = root.join(relative);
            if !path.is_file() {
                return Err(ShaderPackError::MissingAsset(relative.clone()));
            }
        }
        for relative in &textures {
            validate_texture_file(&root.join(relative), relative)?;
        }
        Ok(Self {
            root,
            manifest,
            shaders,
            shader_files,
            textures,
        })
    }
    pub fn manifest(&self) -> &ShaderPackManifest {
        &self.manifest
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn shader_source(&self, name: &str) -> Result<Option<String>, ShaderPackError> {
        let relative = canonical_shader_path(name)?;
        if !self.shaders.contains(&relative) {
            return Ok(None);
        }
        let declared = self
            .shader_files
            .get(&relative)
            .ok_or_else(|| ShaderPackError::MissingAsset(relative.clone()))?;
        fs::read_to_string(self.root.join(declared))
            .map(Some)
            .map_err(|error| ShaderPackError::Io(format!("{relative}: {error}")))
    }
    pub fn validate_wgsl(&self, device: &wgpu::Device) -> Result<(), ShaderPackError> {
        for relative in &self.shaders {
            let source = self
                .shader_source(relative)?
                .ok_or_else(|| ShaderPackError::MissingAsset(relative.clone()))?;
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let _ = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(relative),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            if let Some(error) = pollster::block_on(scope.pop()) {
                return Err(ShaderPackError::Compile(format!("{relative}: {error}")));
            }
        }
        Ok(())
    }
    pub fn resolve_shader(
        &self,
        name: &str,
        builtin_root: impl AsRef<Path>,
    ) -> Result<ResolvedAsset, ShaderPackError> {
        let relative = canonical_shader_path(name)?;
        if self.shaders.contains(&relative) {
            return Ok(ResolvedAsset {
                kind: AssetKind::Shader,
                path: self
                    .shader_files
                    .get(&relative)
                    .map_or_else(|| self.root.join(&relative), |path| self.root.join(path)),
                relative_path: relative,
                from_pack: true,
            });
        }
        let builtin = builtin_root.as_ref().join(&relative);
        Ok(ResolvedAsset {
            kind: AssetKind::Shader,
            path: builtin,
            relative_path: relative,
            from_pack: false,
        })
    }
    pub fn resolve_texture(
        &self,
        name: &str,
        builtin_root: impl AsRef<Path>,
    ) -> Result<ResolvedAsset, ShaderPackError> {
        let (kind, relative) = canonical_texture(name)?;
        if self.textures.contains(&relative) {
            return Ok(ResolvedAsset {
                kind,
                path: self.root.join(&relative),
                relative_path: relative,
                from_pack: true,
            });
        }
        let fallback = match kind {
            AssetKind::BlockTexture => builtin_root.as_ref().join(&relative),
            AssetKind::MaterialTexture | AssetKind::EmissionTexture | AssetKind::HandTexture => {
                builtin_root.as_ref().join(&relative)
            }
            AssetKind::Shader => unreachable!("canonical_texture never returns shader"),
        };
        Ok(ResolvedAsset {
            kind,
            path: fallback,
            relative_path: relative,
            from_pack: false,
        })
    }
    pub fn texture_override(&self, name: &str) -> Result<Option<TextureOverride>, ShaderPackError> {
        let (kind, relative) = canonical_texture(name)?;
        if !self.textures.contains(&relative) {
            return Ok(None);
        }
        let path = self.root.join(&relative);
        validate_texture_file(&path, &relative)?;
        let decoded = image::ImageReader::open(&path)
            .map_err(|error| ShaderPackError::InvalidTexture(format!("{relative}: {error}")))?
            .decode()
            .map_err(|error| ShaderPackError::InvalidTexture(format!("{relative}: {error}")))?;
        let data = match kind {
            AssetKind::EmissionTexture => decoded.to_luma8().into_raw(),
            AssetKind::BlockTexture | AssetKind::MaterialTexture | AssetKind::HandTexture => {
                decoded.to_rgba8().into_raw()
            }
            AssetKind::Shader => unreachable!(),
        };
        Ok(Some(TextureOverride {
            kind,
            relative_path: relative,
            data,
        }))
    }
}
#[derive(Debug, Clone)]
pub struct TransactionalPack<T> {
    active: T,
}
impl<T> TransactionalPack<T> {
    pub fn new(builtin: T) -> Self {
        Self { active: builtin }
    }
    pub fn active(&self) -> &T {
        &self.active
    }
    pub fn replace_with<C, V, E>(
        &mut self,
        candidate: &ShaderPack,
        compile: C,
        validate: V,
    ) -> Result<(), ShaderPackError>
    where
        C: FnOnce(&ShaderPack) -> Result<T, E>,
        V: FnOnce(&T) -> Result<(), E>,
        E: fmt::Display,
    {
        let compiled =
            compile(candidate).map_err(|error| ShaderPackError::Compile(error.to_string()))?;
        validate(&compiled).map_err(|error| ShaderPackError::Compile(error.to_string()))?;
        self.active = compiled;
        Ok(())
    }
    pub fn reload_from_root<C, V, E>(
        &mut self,
        root: impl AsRef<Path>,
        compile: C,
        validate: V,
    ) -> Result<(), ShaderPackError>
    where
        C: FnOnce(&ShaderPack) -> Result<T, E>,
        V: FnOnce(&T) -> Result<(), E>,
        E: fmt::Display,
    {
        let candidate = ShaderPack::load(root)?;
        self.replace_with(&candidate, compile, validate)
    }
}
fn validate_component(value: &str) -> Result<(), ShaderPackError> {
    let path = Path::new(value);
    let mut components = path.components();
    let valid = !value.is_empty()
        && !value.contains('/')
        && !value.contains('\\')
        && value != "."
        && value != ".."
        && matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none();
    if valid {
        Ok(())
    } else {
        Err(ShaderPackError::InvalidPath(value.to_owned()))
    }
}
fn normalize_relative(value: &str) -> Result<Vec<&str>, ShaderPackError> {
    if value.is_empty() || value.contains('\\') || value.starts_with('/') {
        return Err(ShaderPackError::InvalidPath(value.to_owned()));
    }
    let segments = value.split('/').collect::<Vec<_>>();
    if segments
        .iter()
        .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
    {
        return Err(ShaderPackError::InvalidPath(value.to_owned()));
    }
    Ok(segments)
}
fn canonical_shader_path(value: &str) -> Result<String, ShaderPackError> {
    let declared = declared_shader_path(value)?;
    let name = Path::new(&declared)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ShaderPackError::InvalidPath(value.to_owned()))?;
    let canonical = match name {
        "ssao.wgsl" => "gtao.wgsl",
        "sky_lut.wgsl" => "atmosphere.wgsl",
        "translucent.wgsl" | "glass.wgsl" => "chunk.wgsl",
        "taau_tonemap.wgsl" => "tonemap.wgsl",
        _ => name,
    };
    Ok(format!("shaders/{canonical}"))
}
fn declared_shader_path(value: &str) -> Result<String, ShaderPackError> {
    let segments = normalize_relative(value)?;
    let name = match segments.as_slice() {
        [name] => *name,
        [prefix, name] if *prefix == "shaders" => *name,
        _ => return Err(ShaderPackError::ForbiddenOverride(value.to_owned())),
    };
    if name == "ui.wgsl" {
        return Err(ShaderPackError::ForbiddenOverride(value.to_owned()));
    }
    if !ALLOWED_SHADERS.contains(&name) {
        return Err(ShaderPackError::ForbiddenOverride(value.to_owned()));
    }
    Ok(format!("shaders/{name}"))
}
fn canonical_texture(value: &str) -> Result<(AssetKind, String), ShaderPackError> {
    let segments = normalize_relative(value)?;
    let (kind, name) = match segments.as_slice() {
        [name] if name.ends_with(".png") => (AssetKind::BlockTexture, *name),
        ["textures", "blocks", name] => (AssetKind::BlockTexture, *name),
        ["textures", "materials", name] => (AssetKind::MaterialTexture, *name),
        ["textures", "emission", name] => (AssetKind::EmissionTexture, *name),
        ["textures", "player", "hand.png"] => (AssetKind::HandTexture, "hand.png"),
        _ => return Err(ShaderPackError::ForbiddenOverride(value.to_owned())),
    };
    validate_component(name)?;
    if !name.ends_with(".png") {
        return Err(ShaderPackError::ForbiddenOverride(value.to_owned()));
    }
    match kind {
        AssetKind::MaterialTexture if !name.ends_with("_material.png") => {
            return Err(ShaderPackError::ForbiddenOverride(value.to_owned()));
        }
        AssetKind::EmissionTexture if !name.ends_with("_emission.png") => {
            return Err(ShaderPackError::ForbiddenOverride(value.to_owned()));
        }
        _ => {}
    }
    Ok((
        kind,
        format!(
            "textures/{}/{name}",
            match kind {
                AssetKind::BlockTexture => "blocks",
                AssetKind::MaterialTexture => "materials",
                AssetKind::EmissionTexture => "emission",
                AssetKind::HandTexture => "player",
                AssetKind::Shader => unreachable!(),
            }
        ),
    ))
}
fn validate_texture_file(path: &Path, relative: &str) -> Result<(), ShaderPackError> {
    let reader = image::ImageReader::open(path)
        .map_err(|error| ShaderPackError::InvalidTexture(format!("{relative}: {error}")))?;
    let image = reader
        .decode()
        .map_err(|error| ShaderPackError::InvalidTexture(format!("{relative}: {error}")))?;
    if image.dimensions() != (16, 16) {
        return Err(ShaderPackError::InvalidTexture(format!(
            "{relative}: expected 16x16, got {}x{}",
            image.width(),
            image.height()
        )));
    }
    let emission = relative.starts_with("textures/emission/");
    let expected = if emission {
        image::ColorType::L8
    } else {
        image::ColorType::Rgba8
    };
    if image.color() != expected {
        return Err(ShaderPackError::InvalidTexture(format!(
            "{relative}: expected {expected:?} PNG, got {:?}",
            image.color()
        )));
    }
    Ok(())
}
fn canonical_texture_path(value: &str) -> Result<String, ShaderPackError> {
    canonical_texture(value).map(|(_, path)| path)
}
#[cfg(test)]
#[path = "shaderpack/tests.rs"]
mod tests;
