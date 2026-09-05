use super::LEVELS;

#[allow(clippy::too_many_arguments)]
pub(super) fn make_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    material: &[wgpu::TextureView],
    light: &[wgpu::TextureView],
    params: &wgpu::Buffer,
    output: &wgpu::TextureView,
    input: &wgpu::TextureView,
    history: &wgpu::TextureView,
    moment_input: &wgpu::TextureView,
    history_output: &wgpu::TextureView,
    moment_output: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    normal: &wgpu::TextureView,
    material_gbuffer: &wgpu::TextureView,
    previous_surface: &wgpu::TextureView,
    surface_output: &wgpu::TextureView,
) -> wgpu::BindGroup {
    let mut entries = Vec::with_capacity(18);
    for i in 0..LEVELS {
        entries.push(wgpu::BindGroupEntry {
            binding: (i * 2) as u32,
            resource: wgpu::BindingResource::TextureView(&material[i]),
        });
        entries.push(wgpu::BindGroupEntry {
            binding: (i * 2 + 1) as u32,
            resource: wgpu::BindingResource::TextureView(&light[i]),
        });
    }
    entries.push(wgpu::BindGroupEntry {
        binding: 8,
        resource: params.as_entire_binding(),
    });
    for (binding, view) in [
        (9, output),
        (10, input),
        (11, history),
        (12, moment_input),
        (13, history_output),
        (14, moment_output),
        (15, depth),
        (16, normal),
        (17, material_gbuffer),
        (18, previous_surface),
        (19, surface_output),
    ] {
        entries.push(wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(view),
        });
    }
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("vf/m9/gi-bindings"),
        layout,
        entries: &entries,
    })
}

pub(super) fn texture_binding(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

pub(super) fn storage_binding(binding: u32) -> wgpu::BindGroupLayoutEntry {
    storage_binding_format(binding, wgpu::TextureFormat::Rgba16Float)
}

pub(super) fn storage_binding_format(
    binding: u32,
    format: wgpu::TextureFormat,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format,
            view_dimension: wgpu::TextureViewDimension::D2,
        },
        count: None,
    }
}
