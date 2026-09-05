//! Shared scene GPU bindings used by opaque and translucent geometry passes.

use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};

use super::globals::{GLOBALS_SIZE, Globals};
use super::materials::{MATERIAL_MIPS, MaterialArrays, MaterialGpu};
use super::textures::BlockTextures;
use bytemuck::Zeroable;

mod helpers;
use helpers::*;

pub(crate) const CHUNK_UNIFORM_SIZE: u64 = 16;
const DEFAULT_SLOTS: u32 = 16384;

#[derive(Clone)]
pub struct SceneBindings {
    pub globals_buffer: wgpu::Buffer,
    pub globals_bind_group: wgpu::BindGroup,
    pub globals_layout: wgpu::BindGroupLayout,
    pub texture_bind_group: wgpu::BindGroup,
    pub texture_layout: wgpu::BindGroupLayout,
    /// PBR-only group. The legacy group above remains stable for M6 light and
    /// translucent/fallback draws; opaque M7 geometry uses this distinct set.
    pub pbr_bind_group: wgpu::BindGroup,
    pub pbr_layout: wgpu::BindGroupLayout,
    /// Mipped material arrays. All three use the same registry layer order.
    pub albedo_texture: wgpu::Texture,
    pub albedo_view: wgpu::TextureView,
    pub material_texture: wgpu::Texture,
    pub material_view: wgpu::TextureView,
    pub emission_texture: wgpu::Texture,
    pub emission_view: wgpu::TextureView,
    pub material_sampler: wgpu::Sampler,
    pub material_buffer: wgpu::Buffer,
    pub material_count: u32,
    pub sky_cubemap: wgpu::Texture,
    pub sky_cubemap_view: wgpu::TextureView,
    pub chunk_uniforms: wgpu::Buffer,
    pub chunk_bind_group: wgpu::BindGroup,
    pub chunk_layout: wgpu::BindGroupLayout,
    pub slot_size: u64,
    free_slots: Arc<Mutex<Vec<u32>>>,
}

impl SceneBindings {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, textures: &BlockTextures) -> Self {
        let arrays = MaterialArrays::procedural(
            &textures
                .names
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        );
        let (albedo_texture, albedo_view) = upload_array(device, queue, &arrays, ArrayKind::Albedo);
        let (material_texture, material_view) =
            upload_array(device, queue, &arrays, ArrayKind::Material);
        let (emission_texture, emission_view) =
            upload_array(device, queue, &arrays, ArrayKind::Emission);
        let material_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vf/m7/scene_bindings/material-filtering-sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let material_values = arrays
            .layers
            .iter()
            .map(|layer| material_gpu(&layer.name))
            .collect::<Vec<_>>();
        let material_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vf/m7/scene_bindings/material-lut"),
            size: (material_values.len().max(1) * std::mem::size_of::<MaterialGpu>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&material_buffer, 0, bytemuck::cast_slice(&material_values));
        let sky_cubemap = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vf/m7/scene_bindings/sky-cubemap"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 6,
            },
            mip_level_count: 7,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        initialize_sky(device, queue, &sky_cubemap);
        let sky_cubemap_view = sky_cubemap.create_view(&wgpu::TextureViewDescriptor {
            label: Some("vf/m7/scene_bindings/sky-cubemap-view"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vf/m7/scene_bindings/globals-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(GLOBALS_SIZE),
                },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vf/m7/scene_bindings/material-texture-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let chunk_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vf/m7/scene_bindings/chunk-dynamic-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(CHUNK_UNIFORM_SIZE),
                },
                count: None,
            }],
        });
        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vf/m7/scene_bindings/globals-uniform"),
            size: GLOBALS_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&globals_buffer, 0, bytemuck::bytes_of(&Globals::zeroed()));
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m7/scene_bindings/globals-bind-group"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });
        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m7/scene_bindings/material-texture-bind-group"),
            layout: &texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&textures.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&textures.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: material_buffer.as_entire_binding(),
                },
            ],
        });
        let pbr_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vf/m7/scene_bindings/pbr-material-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pbr_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m7/scene_bindings/pbr-material-bind-group"),
            layout: &pbr_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&albedo_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&material_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&emission_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&material_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: material_buffer.as_entire_binding(),
                },
            ],
        });
        let alignment = device.limits().min_uniform_buffer_offset_alignment as u64;
        let slot_size = CHUNK_UNIFORM_SIZE
            .max(256)
            .next_multiple_of(alignment.max(1));
        let chunk_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vf/m7/scene_bindings/chunk-uniform-arena"),
            size: slot_size * DEFAULT_SLOTS as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let chunk_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m7/scene_bindings/chunk-dynamic-bind-group"),
            layout: &chunk_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &chunk_uniforms,
                    offset: 0,
                    size: NonZeroU64::new(CHUNK_UNIFORM_SIZE),
                }),
            }],
        });
        Self {
            globals_buffer,
            globals_bind_group,
            globals_layout,
            texture_bind_group,
            texture_layout,
            pbr_bind_group,
            pbr_layout,
            albedo_texture,
            albedo_view,
            material_texture,
            material_view,
            emission_texture,
            emission_view,
            material_sampler,
            material_buffer,
            material_count: arrays.layers.len() as u32,
            sky_cubemap,
            sky_cubemap_view,
            chunk_uniforms,
            chunk_bind_group,
            chunk_layout,
            slot_size,
            free_slots: Arc::new(Mutex::new((0..DEFAULT_SLOTS).rev().collect())),
        }
    }

    /// Upload one validated shader-pack material layer and its generated mips
    /// without replacing the bind group or pipeline objects.
    pub(crate) fn apply_texture_override(
        &self,
        queue: &wgpu::Queue,
        kind: crate::shaderpack::AssetKind,
        relative_path: &str,
        data: &[u8],
    ) -> anyhow::Result<()> {
        let name = std::path::Path::new(relative_path)
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid material texture path: {relative_path}"))?;
        let layer_name = name
            .strip_suffix("_material.png")
            .or_else(|| name.strip_suffix("_emission.png"))
            .unwrap_or(name.strip_suffix(".png").unwrap_or(name));
        let Some(layer) = self.layers_position(layer_name) else {
            anyhow::bail!("shader pack texture is not in the registry: {layer_name}");
        };
        let (channels, mip_count, texture) = match kind {
            crate::shaderpack::AssetKind::MaterialTexture => {
                (4, MATERIAL_MIPS, &self.material_texture)
            }
            crate::shaderpack::AssetKind::EmissionTexture => {
                (1, MATERIAL_MIPS, &self.emission_texture)
            }
            _ => anyhow::bail!("texture kind is not a material array: {relative_path}"),
        };
        let expected = 16 * 16 * channels;
        if data.len() != expected {
            anyhow::bail!(
                "shader pack texture {relative_path} has {} bytes, expected {expected}",
                data.len()
            );
        }
        let mips = crate::render::materials::make_mips(data, channels, false);
        for (mip_idx, mip) in mips.into_iter().take(mip_count).enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: mip_idx as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &mip.data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some((mip.width * channels) as u32),
                    rows_per_image: Some(mip.height as u32),
                },
                wgpu::Extent3d {
                    width: mip.width as u32,
                    height: mip.height as u32,
                    depth_or_array_layers: 1,
                },
            );
        }
        Ok(())
    }

    fn layers_position(&self, name: &str) -> Option<u32> {
        // The material buffer and arrays share the block registry order.
        self.registry_names()
            .and_then(|names| names.iter().position(|entry| entry == name))
            .map(|index| index as u32)
    }

    fn registry_names(&self) -> Option<Vec<String>> {
        // The bind group does not retain names; derive the stable registry
        // ordering from the canonical world table.
        Some(
            crate::world::block::TEXTURES
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
        )
    }

    pub(crate) fn reserve_slot(&self) -> Option<u32> {
        self.free_slots.lock().ok()?.pop()
    }
    pub(crate) fn release_slot(&self, slot: u32) {
        if let Ok(mut slots) = self.free_slots.lock() {
            slots.push(slot);
        }
    }
}
