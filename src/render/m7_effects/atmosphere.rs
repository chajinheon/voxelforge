#[allow(dead_code)]
pub(super) struct AtmosphereGpu {
    pub(super) transmittance: wgpu::TextureView,
    pub(super) multiscatter: wgpu::TextureView,
    pub(super) sky_view: wgpu::TextureView,
    _textures: [wgpu::Texture; 3],
    _sky_cubemap: wgpu::Texture,
    pipeline: [wgpu::ComputePipeline; 3],
    binds: [wgpu::BindGroup; 3],
    cube_pipeline: wgpu::ComputePipeline,
    downsample_pipeline: wgpu::ComputePipeline,
    cube_bind: wgpu::BindGroup,
    downsample_binds: Vec<wgpu::BindGroup>,
    uniform: wgpu::Buffer,
    frame: u64,
    phase: f32,
    refresh: bool,
}

impl AtmosphereGpu {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let make = |size: (u32, u32), label: &str| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let textures = [
            make((256, 64), "vf/m7/atmosphere/transmittance"),
            make((32, 32), "vf/m7/atmosphere/multiscatter"),
            make((192, 108), "vf/m7/atmosphere/sky-view"),
        ];
        let views = textures
            .each_ref()
            .map(|t| t.create_view(&Default::default()));
        let sky_cubemap = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vf/m7/atmosphere/sky-cubemap-source"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 6,
            },
            mip_level_count: 7,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let sky_cubemap_view = sky_cubemap.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            array_layer_count: Some(6),
            mip_level_count: Some(1),
            ..Default::default()
        });
        let layout = |label, entries: &[wgpu::BindGroupLayoutEntry]| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries,
            })
        };
        let trans_layout = layout(
            "vf/m7/atmosphere/trans-layout",
            &[uniform(0), storage_texture(4)],
        );
        let multi_layout = layout(
            "vf/m7/atmosphere/multi-layout",
            &[uniform(0), texture(1), storage_texture(2)],
        );
        let sky_layout = layout(
            "vf/m7/atmosphere/sky-layout",
            &[uniform(0), texture(1), storage_texture(3), texture(5)],
        );
        let cube_layout = layout(
            "vf/m7/atmosphere/cube-layout",
            &[uniform(0), texture(6), storage_texture_array(7)],
        );
        let downsample_layout = layout(
            "vf/m7/atmosphere/downsample-layout",
            &[storage_texture_array(7), texture_array(8)],
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m7/atmosphere/lut-shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/atmosphere.wgsl").into(),
            ),
        });
        let pipeline = [
            make_pipeline(device, &shader, &trans_layout, "transmittance_main"),
            make_pipeline(device, &shader, &multi_layout, "multiscatter_main"),
            make_pipeline(device, &shader, &sky_layout, "sky_view_main"),
        ];
        let cube_pipeline = make_pipeline(device, &shader, &cube_layout, "cube_main");
        let downsample_pipeline =
            make_pipeline(device, &shader, &downsample_layout, "cube_downsample_main");
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vf/m7/atmosphere/lut-uniform"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let binds = [
            make_bind(
                device,
                "vf/m7/atmosphere/trans-bind",
                &trans_layout,
                &[
                    buffer_entry(0, uniform.as_entire_binding()),
                    texture_entry(4, &views[0]),
                ],
            ),
            make_bind(
                device,
                "vf/m7/atmosphere/multi-bind",
                &multi_layout,
                &[
                    buffer_entry(0, uniform.as_entire_binding()),
                    texture_entry(1, &views[0]),
                    texture_entry(2, &views[1]),
                ],
            ),
            make_bind(
                device,
                "vf/m7/atmosphere/sky-bind",
                &sky_layout,
                &[
                    buffer_entry(0, uniform.as_entire_binding()),
                    texture_entry(1, &views[0]),
                    texture_entry(3, &views[2]),
                    texture_entry(5, &views[1]),
                ],
            ),
        ];
        let cube_bind = make_bind(
            device,
            "vf/m7/atmosphere/cube-bind",
            &cube_layout,
            &[
                buffer_entry(0, uniform.as_entire_binding()),
                texture_entry(6, &views[2]),
                texture_entry(7, &sky_cubemap_view),
            ],
        );
        let downsample_binds = (1..7)
            .map(|mip| {
                let source = sky_cubemap.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    base_mip_level: mip - 1,
                    mip_level_count: Some(1),
                    array_layer_count: Some(6),
                    ..Default::default()
                });
                let output = sky_cubemap.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    array_layer_count: Some(6),
                    ..Default::default()
                });
                make_bind(
                    device,
                    "vf/m7/atmosphere/downsample-bind",
                    &downsample_layout,
                    &[texture_entry(7, &output), texture_entry(8, &source)],
                )
            })
            .collect();
        Self {
            transmittance: views[0].clone(),
            multiscatter: views[1].clone(),
            sky_view: views[2].clone(),
            _textures: textures,
            _sky_cubemap: sky_cubemap,
            pipeline,
            binds,
            cube_pipeline,
            downsample_pipeline,
            cube_bind,
            downsample_binds,
            uniform,
            frame: 0,
            phase: 0.0,
            refresh: true,
        }
    }

    pub(super) fn update(&mut self, queue: &wgpu::Queue, phase: f32, sun_mu: f32) {
        let values = [
            6_360_000.0_f32,
            6_460_000.0,
            sun_mu.clamp(-1.0, 1.0),
            phase,
            5.8e-6,
            13.5e-6,
            33.1e-6,
            0.0,
            21.0e-6,
            21.0e-6,
            21.0e-6,
            0.0,
            8000.0,
            1200.0,
            0.8,
            0.0,
        ];
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&values));
        self.refresh = self.frame == 0
            || (phase - self.phase).abs() >= crate::render::atmosphere::SKY_PHASE_THRESHOLD
            || self
                .frame
                .is_multiple_of(crate::render::atmosphere::SKY_UPDATE_CADENCE);
        self.phase = phase;
    }

    pub(super) fn encode(&mut self, e: &mut wgpu::CommandEncoder, destination: &wgpu::Texture) {
        let update_sky = self.frame == 0 || self.refresh;
        if self.frame == 0 {
            for ((pipeline, bind), dims) in
                self.pipeline
                    .iter()
                    .zip(&self.binds)
                    .zip([(256_u32, 64_u32), (32, 32), (192, 108)])
            {
                let mut p = e.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("vf/m7/atmosphere/lut"),
                    timestamp_writes: None,
                });
                p.set_pipeline(pipeline);
                p.set_bind_group(0, bind, &[]);
                p.dispatch_workgroups(dims.0.div_ceil(16), dims.1.div_ceil(16), 1);
            }
        } else if self.refresh {
            let mut p = e.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("vf/m7/atmosphere/sky-view-refresh"),
                timestamp_writes: None,
            });
            p.set_pipeline(&self.pipeline[2]);
            p.set_bind_group(0, &self.binds[2], &[]);
            p.dispatch_workgroups(24, 14, 1);
        }
        if update_sky {
            let mut p = e.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("vf/m7/atmosphere/sky-cubemap"),
                timestamp_writes: None,
            });
            p.set_pipeline(&self.cube_pipeline);
            p.set_bind_group(0, &self.cube_bind, &[]);
            p.dispatch_workgroups(8, 8, 6);
            drop(p);
            for (index, bind) in self.downsample_binds.iter().enumerate() {
                let size = 64_u32 >> (index + 1);
                let mut p = e.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("vf/m7/atmosphere/sky-cubemap-downsample"),
                    timestamp_writes: None,
                });
                p.set_pipeline(&self.downsample_pipeline);
                p.set_bind_group(0, bind, &[]);
                p.dispatch_workgroups(size.div_ceil(8), size.div_ceil(8), 6);
            }
            for mip in 0_u32..7 {
                let size = 64_u32 >> mip;
                e.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self._sky_cubemap,
                        mip_level: mip,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: destination,
                        mip_level: mip,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: size,
                        height: size,
                        depth_or_array_layers: 6,
                    },
                );
            }
        }
        self.refresh = false;
        self.frame = self.frame.saturating_add(1);
    }
}

fn storage_texture(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format: wgpu::TextureFormat::Rgba16Float,
            view_dimension: wgpu::TextureViewDimension::D2,
        },
        count: None,
    }
}

fn texture(binding: u32) -> wgpu::BindGroupLayoutEntry {
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

fn texture_array(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }
}

fn storage_texture_array(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format: wgpu::TextureFormat::Rgba16Float,
            view_dimension: wgpu::TextureViewDimension::D2Array,
        },
        count: None,
    }
}

fn uniform(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entry<'a>(binding: u32, view: &'a wgpu::TextureView) -> wgpu::BindGroupEntry<'a> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}

fn buffer_entry<'a>(binding: u32, buffer: wgpu::BindingResource<'a>) -> wgpu::BindGroupEntry<'a> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer,
    }
}

fn make_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::BindGroupLayout,
    entry: &str,
) -> wgpu::ComputePipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("vf/m7/atmosphere/pipeline-layout"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(entry),
        layout: Some(&pipeline_layout),
        module: shader,
        entry_point: Some(entry),
        compilation_options: Default::default(),
        cache: None,
    })
}

fn make_bind(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::BindGroupLayout,
    entries: &[wgpu::BindGroupEntry<'_>],
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries,
    })
}
