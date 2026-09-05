//! Fullscreen deferred PBR pass over the M7 opaque G-buffer.

use super::{gbuffer::GBuffer, scene_bindings::SceneBindings, shader_source::compose};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DeferredShadowUniform {
    inverse_view_proj: [[f32; 4]; 4],
    light_view_proj: [[[f32; 4]; 4]; 3],
    splits: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CloudShadowWorldUniform {
    origin: [f32; 2],
    world_size: f32,
    _padding: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeferredDebug {
    #[default]
    Final = 0,
    Albedo = 1,
    Normal = 2,
    Depth = 3,
    Light = 4,
    Material = 5,
    Motion = 6,
    Reactive = 7,
}

pub struct DeferredPipeline {
    pub pipeline: wgpu::RenderPipeline,
    #[allow(dead_code)]
    shader_module: wgpu::ShaderModule,
    bindings: SceneBindings,
    gbuffer_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    shadow_sampler: wgpu::Sampler,
    _shadow_layout: wgpu::BindGroupLayout,
    shadow_buffer: wgpu::Buffer,
    shadow_bind_group: wgpu::BindGroup,
    cloud_shadow_world_buffer: wgpu::Buffer,
    debug_buffer: wgpu::Buffer,
    debug_bind_group: wgpu::BindGroup,
    gbuffer_bind_group: Option<wgpu::BindGroup>,
    size: (u32, u32),
    debug: DeferredDebug,
    exposure: f32,
}

impl DeferredPipeline {
    pub fn new(
        device: &wgpu::Device,
        bindings: SceneBindings,
        color_format: wgpu::TextureFormat,
    ) -> anyhow::Result<Self> {
        Self::new_source(
            device,
            bindings,
            color_format,
            include_str!("../../assets/shaders/deferred.wgsl"),
        )
    }

    /// Build from a pass source supplied by a shader pack.  The common PBR
    /// declarations are always prepended here, so packs replace only the
    /// pass body and cannot silently change the shared binding contract.
    pub(crate) fn new_source(
        device: &wgpu::Device,
        bindings: SceneBindings,
        color_format: wgpu::TextureFormat,
        pass_source: &str,
    ) -> anyhow::Result<Self> {
        let source = compose(pass_source);
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m7/deferred/shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let gbuffer_layout = create_gbuffer_layout(device);
        let debug_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vf/m7/deferred/debug-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
                count: None,
            }],
        });
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vf/m7/deferred/shadow-uniform-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<
                            DeferredShadowUniform,
                        >() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<
                            CloudShadowWorldUniform,
                        >() as u64),
                    },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vf/m7/deferred/layout"),
            bind_group_layouts: &[
                Some(&bindings.globals_layout),
                Some(&gbuffer_layout),
                Some(&debug_layout),
                Some(&shadow_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vf/m7/deferred/pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let debug_words = [DeferredDebug::Final as u32, 1.0_f32.to_bits(), 0, 0];
        let debug_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vf/m7/deferred/debug-uniform"),
            contents: bytemuck::cast_slice(&debug_words),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let debug_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m7/deferred/debug-bind-group"),
            layout: &debug_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: debug_buffer.as_entire_binding(),
            }],
        });
        let shadow_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vf/m7/deferred/shadow-uniform"),
            size: std::mem::size_of::<DeferredShadowUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cloud_shadow_world_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vf/m8/deferred/cloud-shadow-world"),
            size: std::mem::size_of::<CloudShadowWorldUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m7/deferred/shadow-bind-group"),
            layout: &shadow_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: shadow_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: cloud_shadow_world_buffer.as_entire_binding(),
                },
            ],
        });
        if let Some(error) = pollster::block_on(scope.pop()) {
            anyhow::bail!("deferred shader validation failed: {error}");
        }
        Ok(Self {
            pipeline,
            shader_module,
            bindings,
            gbuffer_layout,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("vf/m7/deferred/gbuffer-sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            }),
            shadow_sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("vf/m7/deferred/shadow-compare"),
                compare: Some(wgpu::CompareFunction::LessEqual),
                ..Default::default()
            }),
            _shadow_layout: shadow_layout,
            shadow_buffer,
            shadow_bind_group,
            cloud_shadow_world_buffer,
            debug_buffer,
            debug_bind_group,
            gbuffer_bind_group: None,
            size: (0, 0),
            debug: DeferredDebug::Final,
            exposure: 1.0,
        })
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        gbuffer: &GBuffer,
        shadow_view: &wgpu::TextureView,
        ao_view: &wgpu::TextureView,
        cloud_shadow_view: &wgpu::TextureView,
    ) {
        if self.size == (gbuffer.width, gbuffer.height) && self.gbuffer_bind_group.is_some() {
            return;
        }
        self.gbuffer_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m7/deferred/gbuffer-bind-group"),
            layout: &self.gbuffer_layout,
            entries: &[
                texture_entry(0, &gbuffer.albedo_view),
                texture_entry(1, &gbuffer.normal_view),
                texture_entry(2, &gbuffer.light_view),
                texture_entry(3, &gbuffer.motion_view),
                texture_entry(4, &gbuffer.reactive_view),
                texture_entry(5, &gbuffer.depth_view),
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&self.bindings.sky_cubemap_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::Sampler(&self.shadow_sampler),
                },
                texture_entry(10, ao_view),
                texture_entry(11, cloud_shadow_view),
            ],
        }));
        self.size = (gbuffer.width, gbuffer.height);
    }

    pub fn set_debug(&mut self, queue: &wgpu::Queue, debug: DeferredDebug) {
        self.debug = debug;
        self.write_debug(queue);
    }

    pub fn update_shadow(
        &self,
        queue: &wgpu::Queue,
        view_proj: glam::Mat4,
        camera: glam::Vec3,
        sun: glam::Vec3,
        resolution: u32,
        far_distance: f32,
    ) {
        let s = crate::render::shadow::gpu_uniform(
            camera,
            sun,
            crate::render::shadow::pssm_splits(
                crate::render::shadow::CAMERA_NEAR,
                far_distance,
                crate::render::shadow::PSSM_LAMBDA,
            ),
            resolution,
        );
        let u = DeferredShadowUniform {
            inverse_view_proj: view_proj.inverse().to_cols_array_2d(),
            light_view_proj: s.light_view_proj,
            splits: s.splits,
        };
        queue.write_buffer(&self.shadow_buffer, 0, bytemuck::bytes_of(&u));
    }

    pub fn update_cloud_shadow(&self, queue: &wgpu::Queue, origin: glam::Vec2, world_size: f32) {
        let uniform = CloudShadowWorldUniform {
            origin: origin.to_array(),
            world_size: world_size.max(1.0),
            _padding: 0.0,
        };
        queue.write_buffer(
            &self.cloud_shadow_world_buffer,
            0,
            bytemuck::bytes_of(&uniform),
        );
    }

    pub fn set_exposure(&mut self, queue: &wgpu::Queue, exposure: f32) {
        if exposure.is_finite() && exposure > 0.0 {
            self.exposure = exposure;
            self.write_debug(queue);
        }
    }

    fn write_debug(&self, queue: &wgpu::Queue) {
        let mut bytes = [0_u8; 16];
        bytes[..4].copy_from_slice(&(self.debug as u32).to_ne_bytes());
        bytes[4..8].copy_from_slice(&self.exposure.to_ne_bytes());
        queue.write_buffer(&self.debug_buffer, 0, &bytes);
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        let Some(gbuffer) = self.gbuffer_bind_group.as_ref() else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bindings.globals_bind_group, &[]);
        pass.set_bind_group(1, gbuffer, &[]);
        pass.set_bind_group(2, &self.debug_bind_group, &[]);
        pass.set_bind_group(3, &self.shadow_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn texture_entry(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}

fn create_gbuffer_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let mut entries = Vec::with_capacity(12);
    for binding in 0..6 {
        let sample_type = if binding == 5 {
            wgpu::TextureSampleType::Depth
        } else {
            wgpu::TextureSampleType::Float { filterable: true }
        };
        entries.push(wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
    }
    entries.push(wgpu::BindGroupLayoutEntry {
        binding: 6,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    });
    entries.push(wgpu::BindGroupLayoutEntry {
        binding: 7,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::Cube,
            multisampled: false,
        },
        count: None,
    });
    entries.push(wgpu::BindGroupLayoutEntry {
        binding: 8,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Depth,
            view_dimension: wgpu::TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    });
    entries.push(wgpu::BindGroupLayoutEntry {
        binding: 9,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
        count: None,
    });
    entries.push(wgpu::BindGroupLayoutEntry {
        binding: 10,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    });
    entries.push(wgpu::BindGroupLayoutEntry {
        binding: 11,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    });
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("vf/m7/deferred/gbuffer-layout"),
        entries: &entries,
    })
}

pub const DEFERRED_WGSL: &str = include_str!("../../assets/shaders/deferred.wgsl");
pub fn composed_source() -> String {
    compose(DEFERRED_WGSL)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deferred_source_contains_ggx_and_common_prefix() {
        let source = composed_source();
        assert!(source.contains("fn ggx"));
        assert!(source.find("const PI").unwrap() < source.find("fn fs_main").unwrap());
    }
    #[test]
    fn debug_modes_have_stable_shader_values() {
        assert_eq!(DeferredDebug::Final as u32, 0);
        assert_eq!(DeferredDebug::Reactive as u32, 7);
    }
}
