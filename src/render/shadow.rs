//! Deterministic shadow contracts shared by the CPU scheduler and the GPU pass.
//!
//! The module deliberately keeps projection math independent from `wgpu`.  This
//! makes cascade invalidation and split selection testable without a device and
//! gives the renderer one authoritative set of M7.2 constants.
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

#[path = "shadow_contracts.rs"]
mod contracts;
pub use contracts::*;

pub const CASCADE_COUNT: usize = 3;
pub const M5_AIR_HIGH_FAR: f32 = 224.0;
pub const DEFAULT_FAR: f32 = 192.0;
pub const CAMERA_NEAR: f32 = 0.05;
pub const PSSM_LAMBDA: f32 = 0.65;
pub const SHADOW_RESOLUTION: u32 = 2048;
pub const RECEIVER_BIAS: f32 = 0.0008;
pub const NORMAL_OFFSET_BLOCKS: f32 = 0.025;
pub const CASCADE_BLEND_FRACTION: f32 = 0.10;
pub const CONTACT_MAX_DISTANCE: f32 = 2.5;
pub const CONTACT_STEPS: usize = 8;
pub const CONTACT_THICKNESS_BASE: f32 = 0.06;
pub const CONTACT_THICKNESS_SLOPE: f32 = 0.0015;
pub const SHADOW_WGSL: &str = include_str!("../../assets/shaders/shadow.wgsl");

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ShadowGpuUniform {
    pub light_view_proj: [[[f32; 4]; 4]; 3],
    pub splits: [f32; 4],
}

#[allow(deprecated)]
pub fn gpu_uniform(camera: Vec3, sun: Vec3, splits: [f32; 4], resolution: u32) -> ShadowGpuUniform {
    let d = sun.normalize_or_zero();
    let up = if d.y.abs() > 0.92 { Vec3::Z } else { Vec3::Y };
    let center = snap_light_center([camera.x, camera.z], splits[3], resolution);
    let snapped_camera = Vec3::new(center[0], camera.y, center[1]);
    let view = Mat4::look_at_rh(snapped_camera - d * 128.0, snapped_camera, up);
    let matrix =
        |r: f32| (Mat4::orthographic_rh(-r, r, -r, r, 0.1, 320.0) * view).to_cols_array_2d();
    ShadowGpuUniform {
        light_view_proj: [matrix(splits[1]), matrix(splits[2]), matrix(splits[3])],
        splits,
    }
}

/// Persistent three-layer depth target and the opaque/cutout shadow pipeline.
/// The current frame's camera matrix is intentionally reused as the light
/// matrix: this keeps the pass valid for all scene bindings while the
/// renderer's sun direction remains the authoritative lighting direction.
pub struct ShadowPipeline {
    pipeline: wgpu::RenderPipeline,
    depth: wgpu::Texture,
    view: wgpu::TextureView,
    cascade_views: [wgpu::TextureView; CASCADE_COUNT],
    uniform: wgpu::Buffer,
    _uniform_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    invalidated: bool,
    last_sun: Option<Vec3>,
    resolution: u32,
    far_distance: f32,
}

impl ShadowPipeline {
    pub fn new(
        device: &wgpu::Device,
        bindings: &crate::render::scene_bindings::SceneBindings,
    ) -> anyhow::Result<Self> {
        Self::new_with_config(device, bindings, SHADOW_RESOLUTION, M5_AIR_HIGH_FAR)
    }

    pub fn new_with_config(
        device: &wgpu::Device,
        bindings: &crate::render::scene_bindings::SceneBindings,
        resolution: u32,
        far_distance: f32,
    ) -> anyhow::Result<Self> {
        Self::new_with_config_source(device, bindings, resolution, far_distance, SHADOW_WGSL)
    }

    pub(crate) fn new_with_config_source(
        device: &wgpu::Device,
        bindings: &crate::render::scene_bindings::SceneBindings,
        resolution: u32,
        far_distance: f32,
        source: &str,
    ) -> anyhow::Result<Self> {
        let resolution = resolution.clamp(1024, 2048);
        let far_distance = far_distance.clamp(128.0, 256.0);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m7/shadow/shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vf/m7/shadow/uniform-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<ShadowGpuUniform>() as u64,
                    ),
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vf/m7/shadow/layout"),
            bind_group_layouts: &[
                Some(&bindings.globals_layout),
                Some(&bindings.pbr_layout),
                Some(&bindings.chunk_layout),
                Some(&uniform_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vf/m7/shadow/pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Uint32x2],
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
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[],
            }),
            multiview_mask: None,
            cache: None,
        });
        let (depth, view) = make_target(device, resolution);
        let cascade_views = std::array::from_fn(|layer| {
            depth.create_view(&wgpu::TextureViewDescriptor {
                label: Some("vf/m7/shadow/cascade"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer as u32,
                array_layer_count: Some(1),
                ..Default::default()
            })
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vf/m7/shadow/uniform"),
            size: std::mem::size_of::<ShadowGpuUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m7/shadow/uniform-bind-group"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        Ok(Self {
            pipeline,
            depth,
            view,
            cascade_views,
            uniform,
            _uniform_layout: uniform_layout,
            bind_group,
            invalidated: true,
            last_sun: None,
            resolution,
            far_distance,
        })
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn resolution(&self) -> u32 {
        self.resolution
    }
    pub fn far_distance(&self) -> f32 {
        self.far_distance
    }

    pub fn texture_memory_bytes(&self) -> usize {
        crate::render::memory::texture_bytes(&self.depth)
    }

    pub fn invalidate(&mut self) {
        self.invalidated = true;
    }

    pub fn update(&mut self, queue: &wgpu::Queue, camera: Vec3, sun: Vec3) {
        let direction = sun.normalize_or_zero();
        if self
            .last_sun
            .is_some_and(|previous| previous.dot(direction) < 0.99999)
        {
            self.invalidated = true;
        }
        self.last_sun = Some(direction);
        queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::bytes_of(&gpu_uniform(
                camera,
                sun,
                pssm_splits(CAMERA_NEAR, self.far_distance, PSSM_LAMBDA),
                self.resolution,
            )),
        );
    }

    pub fn record(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        meshes: &[crate::render::chunk_pipeline::GpuChunkMeshes],
        indices: &[usize],
        bindings: &crate::render::scene_bindings::SceneBindings,
        frame: u64,
        timestamp: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        for layer in 0..CASCADE_COUNT {
            if !self.invalidated
                && !cascade_needs_update(
                    layer,
                    CascadeUpdateInput {
                        frame_index: frame,
                        ..Default::default()
                    },
                )
            {
                continue;
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vf/m7/shadow/cascade"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.cascade_views[layer],
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: if layer == 0 { timestamp.clone() } else { None },
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bindings.globals_bind_group, &[]);
            pass.set_bind_group(1, &bindings.pbr_bind_group, &[]);
            pass.set_bind_group(3, &self.bind_group, &[]);
            for &index in indices {
                if let Some(chunk) = meshes.get(index).and_then(|m| m.opaque.as_ref()) {
                    pass.set_bind_group(2, &bindings.chunk_bind_group, &[chunk.uniform_offset]);
                    pass.set_vertex_buffer(0, chunk.vertex_buffer.slice(..));
                    pass.set_index_buffer(chunk.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..chunk.index_count, 0, layer as u32..layer as u32 + 1);
                }
            }
        }
        self.invalidated = false;
    }
}

fn make_target(device: &wgpu::Device, size: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vf/m7/shadow/cascades"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: CASCADE_COUNT as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = depth.create_view(&wgpu::TextureViewDescriptor {
        label: Some("vf/m7/shadow/cascades-view"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        array_layer_count: Some(CASCADE_COUNT as u32),
        ..Default::default()
    });
    (depth, view)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pssm_splits_match_contract() {
        let splits = m5_air_high_splits();
        let expected = [0.05, 26.680_8, 61.104_7, 224.0];
        for (actual, expected) in splits.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-3, "{actual} != {expected}");
        }
        assert!(splits.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn csm_update_cadence_matches_contract() {
        for frame in 0..8 {
            let input = CascadeUpdateInput {
                frame_index: frame,
                ..Default::default()
            };
            assert!(cascade_needs_update(0, input));
            assert_eq!(cascade_needs_update(1, input), frame % 2 == 0);
            assert_eq!(cascade_needs_update(2, input), frame % 4 == 0);
        }
        assert!(cascade_needs_update(
            1,
            CascadeUpdateInput {
                frame_index: 1,
                camera_distance_blocks: 2.0,
                ..Default::default()
            }
        ));
        assert!(cascade_needs_update(
            2,
            CascadeUpdateInput {
                frame_index: 1,
                edited_in_cascade: true,
                ..Default::default()
            }
        ));
    }

    #[test]
    fn shadow_texel_snapping_is_stable_under_subtexel_motion() {
        let radius = 112.0;
        let texel = 2.0 * radius / SHADOW_RESOLUTION as f32;
        let a = snap_light_center([3.125, -7.75], radius, SHADOW_RESOLUTION);
        let b = snap_light_center(
            [3.125 + texel * 0.24, -7.75 - texel * 0.24],
            radius,
            SHADOW_RESOLUTION,
        );
        assert_eq!(a, b);
        let c = snap_light_center([3.125 + texel * 1.0, -7.75], radius, SHADOW_RESOLUTION);
        assert_ne!(a, c);
    }
}
