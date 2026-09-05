use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempPack(PathBuf);

impl TempPack {
    fn new(manifest: &str, files: &[&str]) -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("voxelforge-shaderpack-{suffix}"));
        fs::create_dir_all(root.join("shaders")).expect("create pack");
        fs::create_dir_all(root.join("textures/blocks")).expect("create textures");
        fs::create_dir_all(root.join("textures/materials")).expect("create materials");
        fs::create_dir_all(root.join("textures/emission")).expect("create emission");
        fs::create_dir_all(root.join("textures/player")).expect("create player");
        fs::write(root.join("pack.json"), manifest).expect("write manifest");
        for file in files {
            let path = root.join(file);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create file parent");
            }
            if path.extension().and_then(|value| value.to_str()) == Some("png") {
                if path.to_string_lossy().contains("/emission/") {
                    image::GrayImage::from_pixel(16, 16, image::Luma([128]))
                        .save(&path)
                        .expect("write emission texture");
                } else {
                    image::RgbaImage::from_pixel(16, 16, image::Rgba([128, 96, 64, 255]))
                        .save(&path)
                        .expect("write texture");
                }
            } else {
                fs::write(path, "shader source").expect("write file");
            }
        }
        Self(root)
    }
}

impl Drop for TempPack {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn manifest(contract: u32, shaders: &[&str], textures: &[&str]) -> String {
    serde_json::json!({
        "version": 1,
        "contract_version": contract,
        "name": "TestPack",
        "author": "Tests",
        "shaders": shaders,
        "textures": textures,
    })
    .to_string()
}

#[test]
fn shaderpack_contract_one_is_rejected() {
    let json = manifest(1, &[], &[]);
    assert!(matches!(
        ShaderPackManifest::parse(&json),
        Err(ShaderPackError::UnsupportedContract(1))
    ));
}

#[test]
fn shaderpack_contract_two_accepts_all_render_passes() {
    let names = [
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
        "bloom.wgsl",
        "exposure.wgsl",
        "tonemap.wgsl",
        "present.wgsl",
        "gi_trace.wgsl",
        "gi_temporal.wgsl",
        "gi_denoise.wgsl",
        "gi_composite.wgsl",
    ];
    let manifest = manifest(2, &names, &[]);
    let parsed = ShaderPackManifest::parse(&manifest).expect("contract-2 pass list");
    assert_eq!(parsed.shaders.len(), names.len());
    assert_eq!(
        canonical_shader_path("ssao.wgsl").unwrap(),
        "shaders/gtao.wgsl"
    );
    assert_eq!(
        canonical_shader_path("sky_lut.wgsl").unwrap(),
        "shaders/atmosphere.wgsl"
    );
    assert_eq!(
        canonical_shader_path("glass.wgsl").unwrap(),
        "shaders/chunk.wgsl"
    );
}

#[test]
fn shaderpack_alias_reads_declared_source_path() {
    let temp = TempPack::new(&manifest(2, &["ssao.wgsl"], &[]), &["shaders/ssao.wgsl"]);
    let pack = ShaderPack::load(&temp.0).expect("alias pack");
    assert_eq!(
        pack.shader_source("gtao.wgsl").unwrap().as_deref(),
        Some("shader source")
    );
    assert_eq!(
        pack.shader_source("ssao.wgsl").unwrap().as_deref(),
        Some("shader source")
    );
}

#[test]
fn shaderpack_rejects_duplicate_alias_targets() {
    let json = manifest(2, &["ssao.wgsl", "gtao.wgsl"], &[]);
    assert!(ShaderPackManifest::parse(&json).is_ok());
    let temp = TempPack::new(&json, &["shaders/ssao.wgsl", "shaders/gtao.wgsl"]);
    assert!(matches!(
        ShaderPack::load(&temp.0),
        Err(ShaderPackError::Manifest(message)) if message.contains("multiple sources")
    ));
}

#[test]
fn shaderpack_contract_two_is_transactional() {
    let temp = TempPack::new(
        &manifest(2, &["deferred.wgsl"], &[]),
        &["shaders/deferred.wgsl"],
    );
    let pack = ShaderPack::load(&temp.0).expect("valid pack");
    let mut store = TransactionalPack::new(String::from("builtin"));
    let failed = store.replace_with(
        &pack,
        |_| Ok::<_, &str>(String::from("candidate")),
        |_| Err("pipeline"),
    );
    assert!(failed.is_err());
    assert_eq!(store.active(), "builtin");
    store
        .replace_with(
            &pack,
            |_| Ok::<_, &str>(String::from("candidate")),
            |_| Ok(()),
        )
        .expect("swap succeeds");
    assert_eq!(store.active(), "candidate");
}

#[test]
fn shaderpack_rejects_path_traversal() {
    for path in [
        "../deferred.wgsl",
        "shaders/../../deferred.wgsl",
        "shaders\\deferred.wgsl",
    ] {
        let json = manifest(2, &[path], &[]);
        assert!(ShaderPackManifest::parse(&json).is_err(), "{path}");
        let temp = TempPack::new(&json, &[]);
        assert!(ShaderPack::load(&temp.0).is_err(), "{path}");
    }
    assert!(canonical_texture("textures/blocks/../secret.png").is_err());
    assert!(canonical_texture("textures/registry.png").is_err());
    assert!(canonical_shader_path("ui.wgsl").is_err());
    assert!(canonical_shader_path("registry.wgsl").is_err());
    assert!(canonical_shader_path("shape.wgsl").is_err());
}

#[test]
fn shaderpack_texture_precedence_matches_contract() {
    let paths = ["textures/blocks/stone.png"];
    let temp = TempPack::new(&manifest(2, &[], &paths), &paths);
    let pack = ShaderPack::load(&temp.0).expect("valid pack");
    let builtin = PathBuf::from("/builtin/assets");
    for path in paths {
        assert!(
            pack.resolve_texture(path, &builtin)
                .expect("resolve")
                .from_pack
        );
    }
    assert!(
        pack.resolve_texture("dirt.png", &builtin)
            .is_ok_and(|asset| !asset.from_pack)
    );
}

#[test]
fn shaderpack_rejects_non_rgba_or_wrong_size_textures() {
    let paths = ["textures/blocks/stone.png"];
    let temp = TempPack::new(&manifest(2, &[], &paths), &paths);
    image::RgbaImage::from_pixel(8, 16, image::Rgba([128, 96, 64, 255]))
        .save(temp.0.join(paths[0]))
        .expect("write wrong-size texture");
    assert!(matches!(
        ShaderPack::load(&temp.0),
        Err(ShaderPackError::InvalidTexture(_))
    ));

    let temp = TempPack::new(&manifest(2, &[], &paths), &paths);
    image::GrayImage::from_pixel(16, 16, image::Luma([128]))
        .save(temp.0.join(paths[0]))
        .expect("write wrong-channel texture");
    assert!(matches!(
        ShaderPack::load(&temp.0),
        Err(ShaderPackError::InvalidTexture(_))
    ));
}

#[test]
fn shaderpack_contract_two_accepts_all_texture_kinds() {
    let paths = [
        "textures/blocks/stone.png",
        "textures/materials/stone_material.png",
        "textures/emission/stone_emission.png",
        "textures/player/hand.png",
    ];
    let temp = TempPack::new(&manifest(2, &[], &paths), &paths);
    let pack = ShaderPack::load(&temp.0).expect("all contract texture kinds load");
    assert_eq!(
        pack.texture_override(paths[0]).unwrap().unwrap().data.len(),
        1024
    );
    assert_eq!(
        pack.texture_override(paths[1]).unwrap().unwrap().data.len(),
        1024
    );
    assert_eq!(
        pack.texture_override(paths[2]).unwrap().unwrap().data.len(),
        256
    );
    assert_eq!(
        pack.texture_override(paths[3]).unwrap().unwrap().data.len(),
        1024
    );
}

#[test]
fn shaderpack_rejects_wrong_kind_or_target_names() {
    for path in [
        "textures/materials/stone.png",
        "textures/emission/stone_material.png",
        "textures/player/hand_extra.png",
    ] {
        let json = manifest(2, &[], &[path]);
        assert!(ShaderPackManifest::parse(&json).is_err(), "{path}");
    }
    let paths = ["textures/emission/stone_emission.png"];
    let temp = TempPack::new(&manifest(2, &[], &paths), &paths);
    image::RgbaImage::from_pixel(16, 16, image::Rgba([128, 96, 64, 255]))
        .save(temp.0.join(paths[0]))
        .expect("write wrong emission channels");
    assert!(matches!(
        ShaderPack::load(&temp.0),
        Err(ShaderPackError::InvalidTexture(_))
    ));
}
