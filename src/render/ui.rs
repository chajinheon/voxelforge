//! Native-resolution, runtime-generated alpha-blended UI geometry for M10.
use super::allocation_stats::AllocationStats;
use crate::ui::UiScale;
use bytemuck::{Pod, Zeroable};
use std::rc::Rc;
mod catalog;
mod draw;
mod draw_methods;
mod icon_bake;
mod lifecycle;
mod panels;
mod text;
mod types;
mod viewmodel;
use catalog::draw_item_icon;
pub use catalog::*;
use draw::{draw_hud, draw_inventory};
use draw::{outline, rect};
use icon_bake::IconBake;
use panels::draw_menu;
pub use types::{UiFrame, UiMode, UiSettingsDisplay, ViewModelLighting};
use viewmodel::{GpuViewModelMeshes, ViewModelMeshCache};
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct UiVertex {
    position: [f32; 2],
    color: [f32; 4],
    icon_uv: [f32; 2],
    icon_layer: f32,
}
pub struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    viewmodel_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    vertices: Vec<UiVertex>,
    vertex_capacity: usize,
    vertex_count: u32,
    viewmodel_count: u32,
    viewmodel_item: u16,
    viewmodel_meshes: ViewModelMeshCache,
    gpu_viewmodel: GpuViewModelMeshes,
    allocation_stats: Option<Rc<AllocationStats>>,
    icon_bake: IconBake,
    icon_bind_group: wgpu::BindGroup,
    hand_texture: wgpu::Texture,
    hand_bind_group: wgpu::BindGroup,
    native_depth: Option<(u32, u32, wgpu::Texture, wgpu::TextureView)>,
}
impl UiRenderer {
    pub fn native_depth_view(&self) -> Option<&wgpu::TextureView> {
        self.native_depth.as_ref().map(|(_, _, _, view)| view)
    }
    /// Actual native-resolution UI vertex allocation owned by the renderer.
    pub fn buffer_memory_bytes(&self) -> usize {
        self.vertex_buffer.size() as usize
            + self.icon_bake.memory_bytes()
            + self.viewmodel_meshes.memory_bytes()
            + self.gpu_viewmodel.memory_bytes()
            + 16 * 16 * 4
    }
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self::new_with_stats(device, queue, format, None)
    }

    pub(crate) fn new_with_stats(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        allocation_stats: Option<Rc<AllocationStats>>,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m10/ui/shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../assets/shaders/ui.wgsl").into()),
        });
        let icon_bake = IconBake::new(device, queue);
        let _icon_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m10/item-icon-shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../assets/shaders/item_icon.wgsl").into(),
            ),
        });
        let _viewmodel_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m10/viewmodel-shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../assets/shaders/viewmodel.wgsl").into(),
            ),
        });
        let _blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m10/ui-blur-shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../assets/shaders/ui_blur.wgsl").into(),
            ),
        });
        let icon_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vf/m10/ui/icons-layout"),
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
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let icon_sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
        let icon_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m10/ui/icons"),
            layout: &icon_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(icon_bake.atlas_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&icon_sampler),
                },
            ],
        });
        let hand_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vf/m10/viewmodel/hand-16x16"),
            size: wgpu::Extent3d {
                width: 16,
                height: 16,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &hand_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &builtin_hand_pixels(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(16 * 4),
                rows_per_image: Some(16),
            },
            wgpu::Extent3d {
                width: 16,
                height: 16,
                depth_or_array_layers: 1,
            },
        );
        let hand_view = hand_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let hand_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let hand_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vf/m10/viewmodel/hand-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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
        let hand_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m10/viewmodel/hand"),
            layout: &hand_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&hand_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&hand_sampler),
                },
            ],
        });
        let viewmodel_params_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("vf/m10/viewmodel/params-layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<
                            viewmodel::GpuParams,
                        >() as u64),
                    },
                    count: None,
                }],
            });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vf/m10/ui/layout"),
            bind_group_layouts: &[Some(&icon_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vf/m10/ui/pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<UiVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4, 2 => Float32x2, 3 => Float32],
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let viewmodel_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m10/viewmodel/pipeline-shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../assets/shaders/viewmodel.wgsl").into(),
            ),
        });
        let viewmodel_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vf/m10/viewmodel/layout"),
            bind_group_layouts: &[
                Some(&icon_layout),
                Some(&hand_layout),
                Some(&viewmodel_params_layout),
            ],
            immediate_size: 0,
        });
        let viewmodel_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vf/m10/viewmodel/pipeline"),
            layout: Some(&viewmodel_layout),
            vertex: wgpu::VertexState {
                module: &viewmodel_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<viewmodel::GpuVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4, 4 => Float32],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &viewmodel_module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let vertex_capacity = 6 * 64;
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vf/m10/ui/vertices"),
            size: (vertex_capacity * std::mem::size_of::<UiVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            viewmodel_pipeline,
            vertex_buffer,
            vertices: Vec::with_capacity(vertex_capacity),
            vertex_capacity,
            vertex_count: 0,
            viewmodel_count: 0,
            viewmodel_item: 1,
            viewmodel_meshes: ViewModelMeshCache::new(),
            gpu_viewmodel: GpuViewModelMeshes::new(device, &viewmodel_params_layout),
            allocation_stats,
            icon_bake,
            icon_bind_group,
            hand_texture,
            hand_bind_group,
            native_depth: None,
        }
    }

    /// Replace the optional 16x16 sRGB hand texture without rebuilding the UI
    /// pipeline. ShaderPack has already decoded and validated the raw RGBA.
    pub(crate) fn apply_hand_override(
        &mut self,
        queue: &wgpu::Queue,
        data: &[u8],
    ) -> anyhow::Result<()> {
        if data.len() != 16 * 16 * 4 {
            anyhow::bail!("hand texture must be exactly 16x16 RGBA");
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.hand_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(16 * 4),
                rows_per_image: Some(16),
            },
            wgpu::Extent3d {
                width: 16,
                height: 16,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    /// Rebuild one native-resolution batch from runtime geometry.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &UiFrame<'_>) {
        let dimensions = (frame.width.max(1), frame.height.max(1));
        if self
            .native_depth
            .as_ref()
            .is_none_or(|(w, h, _, _)| (*w, *h) != dimensions)
        {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("vf/m10/viewmodel/native-depth32"),
                size: wgpu::Extent3d {
                    width: dimensions.0,
                    height: dimensions.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.native_depth = Some((dimensions.0, dimensions.1, texture, view));
        }
        self.vertices.clear();
        if frame.width == 0 || frame.height == 0 {
            self.vertex_count = 0;
            self.viewmodel_count = 0;
            return;
        }
        let scale = UiScale {
            output_width: frame.width,
            output_height: frame.height,
            settings_scale: frame.scale,
        };
        if matches!(frame.mode, UiMode::Hud)
            && let Some(viewmodel) = frame.viewmodel
        {
            self.gpu_viewmodel.update(
                queue,
                viewmodel,
                frame.width as f32 / frame.height.max(1) as f32,
                frame.viewmodel_lighting,
            );
            self.viewmodel_item = viewmodel.displayed_item();
        }
        self.viewmodel_count = 0;
        match frame.mode {
            UiMode::Hud => draw_hud(
                &mut self.vertices,
                frame.width,
                frame.height,
                &scale,
                frame.inventory,
                true,
            ),
            UiMode::Inventory => {
                draw_inventory(
                    &mut self.vertices,
                    frame.width,
                    frame.height,
                    &scale,
                    frame.inventory,
                    frame.records,
                );
            }
            UiMode::Pause => draw_menu(&mut self.vertices, frame.width, frame.height),
            UiMode::Settings => panels::draw_settings(
                &mut self.vertices,
                frame.width,
                frame.height,
                frame.settings,
            ),
        }
        if matches!(frame.mode, UiMode::Inventory) {
            draw_hud(
                &mut self.vertices,
                frame.width,
                frame.height,
                &scale,
                frame.inventory,
                false,
            );
        }
        if self.vertices.len() > self.vertex_capacity {
            if let Some(stats) = self.allocation_stats.as_ref() {
                stats.note_cpu_capacity_growth();
                stats.note_gpu_resource_creation();
            }
            self.vertex_capacity = self.vertices.len().next_power_of_two();
            self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("vf/m10/ui/vertices-grown"),
                size: (self.vertex_capacity * std::mem::size_of::<UiVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        self.vertex_count = self.vertices.len() as u32;
        if !self.vertices.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&self.vertices));
        }
    }
}

fn builtin_hand_pixels() -> Vec<u8> {
    let mut pixels = vec![0_u8; 16 * 16 * 4];
    for y in 0..16 {
        for x in 0..16 {
            let sleeve = y >= 10;
            let base = if sleeve {
                [54, 78, 122]
            } else if (x + y) % 5 == 0 {
                [224, 169, 126]
            } else {
                [196, 139, 101]
            };
            let index = (y * 16 + x) * 4;
            pixels[index..index + 4].copy_from_slice(&[base[0], base[1], base[2], 255]);
        }
    }
    pixels
}

#[cfg(test)]
#[path = "ui/tests.rs"]
mod tests;
