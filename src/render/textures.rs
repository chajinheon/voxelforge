//! The block `texture_2d_array` and its deterministic fallback layers.

use std::path::Path;

const SIZE: u32 = 16;

/// GPU-resident block texture layers in registry order.
pub struct BlockTextures {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub names: Vec<String>,
}

impl BlockTextures {
    /// Load named layers from `assets/textures/blocks`, generating absent
    /// layers procedurally. A supplied PNG must be exactly 16 by 16 pixels.
    pub fn from_assets(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        asset_root: &Path,
        names: &[&str],
    ) -> anyhow::Result<Self> {
        if names.is_empty() {
            anyhow::bail!("block texture registry is empty");
        }
        let mut layers = Vec::with_capacity(names.len());
        for (layer, name) in names.iter().enumerate() {
            let path = asset_root
                .join("textures/blocks")
                .join(format!("{name}.png"));
            layers.push(load_layer(&path, name, layer as u64)?);
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("block-texture-array"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: layers.len() as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (layer, data) in layers.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(SIZE * 4),
                    rows_per_image: Some(SIZE),
                },
                wgpu::Extent3d {
                    width: SIZE,
                    height: SIZE,
                    depth_or_array_layers: 1,
                },
            );
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("block-texture-array-view"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("block-nearest-repeat-sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        Ok(Self {
            texture,
            view,
            sampler,
            names: names.iter().map(|name| (*name).to_owned()).collect(),
        })
    }

    /// Construct layers using the block registry from the world module.
    pub fn from_registry(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        asset_root: &Path,
    ) -> anyhow::Result<Self> {
        Self::from_assets(device, queue, asset_root, crate::world::block::TEXTURES)
    }

    /// Replace one existing registry layer in place.  The bind groups used by
    /// all chunk pipelines point at this texture handle, so a queue upload is
    /// sufficient and does not invalidate meshes or pipeline state.
    pub(crate) fn apply_override(
        &self,
        queue: &wgpu::Queue,
        relative_path: &str,
        data: &[u8],
    ) -> anyhow::Result<()> {
        let name = std::path::Path::new(relative_path)
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid block texture path: {relative_path}"))?;
        let Some(layer) = self.names.iter().position(|entry| entry == name) else {
            anyhow::bail!("shader pack block texture is not in the registry: {name}");
        };
        if data.len() != (SIZE * SIZE * 4) as usize {
            anyhow::bail!("shader pack texture {relative_path} is not RGBA8 16x16");
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer as u32,
                },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE * 4),
                rows_per_image: Some(SIZE),
            },
            wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }
}

fn load_layer(path: &Path, name: &str, layer: u64) -> anyhow::Result<Vec<u8>> {
    if path.is_file() {
        let image = image::ImageReader::open(path)?.decode()?.to_rgba8();
        if image.dimensions() != (SIZE, SIZE) {
            anyhow::bail!(
                "texture {} is {}x{}, expected 16x16",
                path.display(),
                image.width(),
                image.height()
            );
        }
        return Ok(image.into_raw());
    }
    Ok(procedural_layer(name, layer))
}

fn hash(x: u32, y: u32, layer: u64) -> u8 {
    let mut n = (x as u64)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add((y as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9))
        .wrapping_add(layer.wrapping_mul(0x94d0_49bb_1331_11eb));
    n ^= n >> 30;
    n = n.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    n ^= n >> 27;
    (n as u8) % 17
}

fn noisy(base: [u8; 3], x: u32, y: u32, layer: u64) -> [u8; 3] {
    let delta = hash(x, y, layer) as i16 - 8;
    [
        (base[0] as i16 + delta).clamp(0, 255) as u8,
        (base[1] as i16 + delta).clamp(0, 255) as u8,
        (base[2] as i16 + delta).clamp(0, 255) as u8,
    ]
}

fn procedural_layer(name: &str, layer: u64) -> Vec<u8> {
    let mut out = vec![0_u8; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let color = match name {
                "stone" => noisy([125, 125, 125], x, y, layer),
                "dirt" => noisy([134, 96, 67], x, y, layer),
                "grass_top" => noisy([95, 159, 53], x, y, layer),
                "grass_side" if y < 4 => noisy([95, 159, 53], x, y, layer),
                "grass_side" => noisy([134, 96, 67], x, y, layer),
                "sand" => noisy([219, 211, 160], x, y, layer),
                "water" => noisy([63, 118, 228], x, y, layer),
                "log_side" => {
                    let base = if x % 4 == 0 {
                        [87, 66, 40]
                    } else {
                        [109, 85, 50]
                    };
                    noisy(base, x, y, layer)
                }
                "log_top" => {
                    let ring = x.min(y).min(SIZE - 1 - x).min(SIZE - 1 - y);
                    noisy(
                        if ring % 3 == 0 {
                            [87, 66, 40]
                        } else {
                            [145, 105, 60]
                        },
                        x,
                        y,
                        layer,
                    )
                }
                "leaves" => noisy([60, 120, 40], x, y, layer),
                "planks" => noisy([157, 127, 78], x, y, layer)
                    .map(|v| v.saturating_add(if y % 4 == 0 { 10 } else { 0 })),
                "glass" => {
                    if x == 0 || y == 0 || x == SIZE - 1 || y == SIZE - 1 {
                        [160, 205, 215]
                    } else {
                        [200, 235, 240]
                    }
                }
                "brick" => {
                    let mortar = y % 4 == 0 || (y / 4 + x / 8) % 2 == 0 && x % 8 == 0;
                    if mortar {
                        [95, 65, 55]
                    } else {
                        noisy([150, 84, 66], x, y, layer)
                    }
                }
                "cobble" => {
                    let mortar = x % 5 == 0 || y % 5 == 0;
                    if mortar {
                        [80, 80, 80]
                    } else {
                        noisy([120, 120, 120], x, y, layer)
                    }
                }
                "torch" => {
                    let dx = x as i32 - 8;
                    let dy = y as i32 - 5;
                    let flame = dx * dx + dy * dy < 28 && y < 11;
                    let stick = (6..14).contains(&y) && (7..10).contains(&x);
                    if flame {
                        [255, 190 + (hash(x, y, layer) % 40), 45]
                    } else if stick {
                        [116, 72, 34]
                    } else {
                        [0, 0, 0]
                    }
                }
                n if n.ends_with("_glass") => {
                    let tint: [u8; 3] = match n {
                        "red_glass" => [220, 75, 70],
                        "green_glass" => [75, 190, 100],
                        "cyan_glass" => [65, 200, 200],
                        "blue_glass" => [75, 115, 220],
                        _ => [220, 235, 240],
                    };
                    if x == 0 || y == 0 || x == SIZE - 1 || y == SIZE - 1 {
                        tint.map(|v| v.saturating_sub(35))
                    } else {
                        tint
                    }
                }
                n if n.contains("concrete") => {
                    let color = match n {
                        "white_concrete" => [208, 210, 208],
                        "light_gray_concrete" => [180, 182, 182],
                        "gray_concrete" => [128, 130, 132],
                        "black_concrete" => [38, 40, 44],
                        "brown_concrete" => [112, 72, 48],
                        "red_concrete" => [170, 52, 45],
                        "orange_concrete" => [220, 112, 40],
                        "yellow_concrete" => [224, 190, 44],
                        "lime_concrete" => [120, 190, 48],
                        "green_concrete" => [58, 132, 62],
                        "cyan_concrete" => [45, 170, 170],
                        "light_blue_concrete" => [72, 160, 216],
                        "blue_concrete" => [55, 82, 180],
                        "purple_concrete" => [122, 68, 170],
                        "magenta_concrete" => [190, 62, 160],
                        _ => [232, 120, 160],
                    };
                    noisy(color, x, y, layer)
                }
                n if matches!(
                    n,
                    "glowstone" | "sea_lantern" | "warm_lamp" | "cold_lamp" | "glow_panel"
                ) =>
                {
                    let color = if n == "sea_lantern" || n == "cold_lamp" {
                        [120, 220, 240]
                    } else {
                        [240, 120, 45]
                    };
                    noisy(color, x, y, layer)
                }
                n if matches!(
                    n,
                    "gold_block" | "copper" | "weathered_copper" | "metal_panel" | "rusted_metal"
                ) =>
                {
                    let color = if n == "gold_block" {
                        [224, 170, 42]
                    } else if n == "copper" {
                        [180, 105, 62]
                    } else {
                        [160, 166, 172]
                    };
                    noisy(color, x, y, layer)
                }
                _ => noisy([190, 190, 190], x, y, layer),
            };
            let index = ((y * SIZE + x) * 4) as usize;
            let alpha = match name {
                "water" => 150,
                "glass" | "white_glass" | "red_glass" | "green_glass" | "cyan_glass"
                | "blue_glass" => 90,
                "torch"
                    if {
                        let dx = x as i32 - 8;
                        let dy = y as i32 - 5;
                        dx * dx + dy * dy < 28 && y < 11
                    } =>
                {
                    255
                }
                "torch" => 0,
                _ => 255,
            };
            out[index..index + 4].copy_from_slice(&[color[0], color[1], color[2], alpha]);
        }
    }
    out
}
