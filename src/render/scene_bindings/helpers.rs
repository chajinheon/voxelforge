use super::super::materials::{MATERIAL_MIPS, MATERIAL_SIZE, MaterialArrays, MaterialGpu};
#[derive(Clone, Copy)]
pub(super) enum ArrayKind {
    Albedo,
    Material,
    Emission,
}

pub(super) fn upload_array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    arrays: &MaterialArrays,
    kind: ArrayKind,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vf/m7/scene_bindings/material-array"),
        size: wgpu::Extent3d {
            width: MATERIAL_SIZE as u32,
            height: MATERIAL_SIZE as u32,
            depth_or_array_layers: arrays.layers.len().max(1) as u32,
        },
        mip_level_count: MATERIAL_MIPS as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: match kind {
            ArrayKind::Albedo => wgpu::TextureFormat::Rgba8UnormSrgb,
            ArrayKind::Material => wgpu::TextureFormat::Rgba8Unorm,
            ArrayKind::Emission => wgpu::TextureFormat::R8Unorm,
        },
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer_idx, layer) in arrays.layers.iter().enumerate() {
        let mips = match kind {
            ArrayKind::Albedo => &layer.albedo,
            ArrayKind::Material => &layer.material,
            ArrayKind::Emission => &layer.emission,
        };
        for (mip_idx, mip) in mips.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: mip_idx as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer_idx as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &mip.data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(
                        (mip.width
                            * match kind {
                                ArrayKind::Emission => 1,
                                _ => 4,
                            }) as u32,
                    ),
                    rows_per_image: Some(mip.height as u32),
                },
                wgpu::Extent3d {
                    width: mip.width as u32,
                    height: mip.height as u32,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("vf/m7/scene_bindings/material-array-view"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    (texture, view)
}

pub(super) fn material_gpu(name: &str) -> MaterialGpu {
    let metallic = match name {
        "gold" | "sand" | "gold_block" => 1.0,
        "metal" | "cobble" | "metal_panel" | "rusted_metal" | "copper" | "weathered_copper" => 0.9,
        _ => 0.0,
    };
    let cutout = matches!(
        name,
        "leaves" | "birch_leaves" | "spruce_leaves" | "dark_oak_leaves"
    );
    let pom = !matches!(
        name,
        "leaves" | "birch_leaves" | "spruce_leaves" | "dark_oak_leaves" | "water" | "glass"
    ) && !name.ends_with("_glass");
    MaterialGpu {
        base: [
            metallic,
            1.0,
            0.04,
            if matches!(
                name,
                "torch"
                    | "emissive"
                    | "glowstone"
                    | "sea_lantern"
                    | "warm_lamp"
                    | "cold_lamp"
                    | "glow_panel"
            ) {
                0.6
            } else {
                0.0
            },
        ],
        tint: if name == "torch" {
            [1.0, 0.42, 0.08, 0.5]
        } else {
            [1.0, 0.72, 0.35, 0.5]
        },
        flags: [(pom as u32) | ((cutout as u32) << 1), 0, 0, 0],
    }
}

/// Fill every sky mip and face at initialization. A newly-created texture is
/// not a valid sky source until all subresources have deterministic contents.
pub(super) fn initialize_sky(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) {
    let lut = crate::render::atmosphere::AtmosphereLuts::generate(
        crate::render::atmosphere::AtmosphereConstants::default(),
    );
    for mip in 0..7u32 {
        let size = 64 >> mip;
        let mut data = vec![0u8; (size * size * 6 * 8) as usize];
        for face in 0..6u32 {
            for y in 0..size {
                for x in 0..size {
                    let i = (((face * size + y) * size + x) * 8) as usize;
                    let direction = cube_direction(face, x, y, size);
                    let uv = crate::render::atmosphere::sky_view_uv(direction);
                    let px = (uv[0] * crate::render::atmosphere::SKY_VIEW_SIZE.0 as f32) as usize;
                    let py = (uv[1] * crate::render::atmosphere::SKY_VIEW_SIZE.1 as f32) as usize;
                    let c = lut.sky_view[py.min(107) * 192 + px.min(191)];
                    for (channel, value) in c.iter().enumerate() {
                        data[i + channel * 2..i + channel * 2 + 2]
                            .copy_from_slice(&f32_to_f16(*value).to_le_bytes());
                    }
                }
            }
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: mip,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size * 8),
                rows_per_image: Some(size),
            },
            wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 6,
            },
        );
    }
    let _ = device; // keep the helper signature explicit at the call site
}

fn cube_direction(face: u32, x: u32, y: u32, size: u32) -> [f32; 3] {
    let a = 2.0 * (x as f32 + 0.5) / size as f32 - 1.0;
    let b = 2.0 * (y as f32 + 0.5) / size as f32 - 1.0;
    let v = match face {
        0 => [1.0, -b, -a],
        1 => [-1.0, -b, a],
        2 => [a, 1.0, b],
        3 => [a, -1.0, -b],
        4 => [a, -b, 1.0],
        _ => [-a, -b, -1.0],
    };
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}

fn f32_to_f16(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mantissa = bits & 0x7f_ff_ff;
    if exponent <= 0 {
        return sign;
    }
    if exponent >= 31 {
        return sign | 0x7c00;
    }
    sign | ((exponent as u16) << 10) | ((mantissa >> 13) as u16)
}
