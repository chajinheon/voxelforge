use super::gtao::{GtaoInputs, GtaoPass};
use super::resources::*;
use super::{M7EffectInputs, M7EffectParams};
use wgpu::util::DeviceExt;
const EFFECT_WGSL: &str = r#"
struct P { exposure:f32, ao:f32, atmosphere:f32, history:f32, jitter:vec2<f32>, frame:f32, _pad:f32, auto_exposure:f32, delta_seconds:f32, phase:f32, _pad2:f32 }
@group(0) @binding(0) var src:texture_2d<f32>; @group(0) @binding(1) var depth:texture_depth_2d;
@group(0) @binding(2) var normal:texture_2d<f32>; @group(0) @binding(3) var material:texture_2d<f32>; @group(0) @binding(4) var hist:texture_2d<f32>;
@group(0) @binding(5) var motion:texture_2d<f32>; @group(0) @binding(6) var reactive:texture_2d<f32>; @group(0) @binding(7) var samp:sampler;
@group(0) @binding(8) var<uniform> p:P; @group(0) @binding(9) var<storage,read> exposure_state:array<f32>;
@group(0) @binding(10) var old_depth:texture_2d<f32>; @group(0) @binding(11) var old_normal:texture_2d<f32>; @group(0) @binding(12) var old_material:texture_2d<f32>; @group(0) @binding(13) var sky:texture_cube<f32>; @group(0) @binding(14) var sky_view_lut:texture_2d<f32>; @group(0) @binding(15) var transmittance_lut:texture_2d<f32>; @group(0) @binding(16) var multiscatter_lut:texture_2d<f32>;
struct V { @builtin(position) pos:vec4<f32>, @location(0) uv:vec2<f32> }
@vertex fn vs(@builtin(vertex_index) i:u32)->V { var q=array<vec2<f32>,3>(vec2(-1.),vec2(3.,-1.),vec2(-1.,3.)); var o:V; o.pos=vec4(q[i],0.,1.); o.uv=q[i]*.5+.5; o.uv.y=1.-o.uv.y; return o; }
@fragment fn ao_pass(i:V)->@location(0) f32 { return 1.0; }
@fragment fn exposure_pass(i:V)->@location(0) vec4<f32> { let gain=select(p.exposure,exposure_state[1],p.auto_exposure>.5); return vec4(textureSampleLevel(src,samp,i.uv,0.).rgb*gain,1.); }
@fragment fn atmosphere_pass(i:V)->@location(0) vec4<f32> { let c=textureSampleLevel(src,samp,i.uv,0.); let dp=vec2<i32>(min(i.pos.xy,vec2<f32>(textureDimensions(depth))-vec2(1.))); let d=textureLoad(depth,dp,0); let sky_lut=textureSampleLevel(sky_view_lut,samp,i.uv,0.).rgb; let trans=textureSampleLevel(transmittance_lut,samp,vec2(i.uv.x,.35),0.).rgb; let multi=textureSampleLevel(multiscatter_lut,samp,vec2(i.uv.x,.5),0.).rgb; let sky_mask=select(0.,1.,d>=.9999); let aerial=(1.-clamp(d,0.,1.))*0.06; return vec4(c.rgb*(.75+.25*textureSampleLevel(reactive,samp,i.uv,0.).r)+sky_lut*sky_mask*p.atmosphere*trans+multi*(1.-sky_mask)*aerial,c.a); }
fn ycocg(c:vec3<f32>)->vec3<f32>{return vec3(.25*c.r+.5*c.g+.25*c.b,.5*c.r-.5*c.b,-.25*c.r+.5*c.g-.25*c.b);} fn rgb(y:vec3<f32>)->vec3<f32>{return vec3(y.x+y.y-y.z,y.x+y.z,y.x-y.y-y.z);}
fn oct_decode(e:vec2<f32>)->vec3<f32>{var n=vec3(e.x,e.y,1.-abs(e.x)-abs(e.y));if(n.z<0.){let q=(vec2(1.)-abs(n.yx))*sign(n.xy);n=vec3(q,n.z);}return normalize(n);}
fn linear_depth(z:f32)->f32 { let n=.05; let f=1000.; return n*f/max(f-clamp(z,0.,1.)*(f-n),1e-5); }
fn cubic(x:f32)->f32{let a=abs(x);if(a<=1.){return 1.5*a*a*a-2.5*a*a+1.;}if(a<2.){return -.5*a*a*a+2.5*a*a-4.*a+2.;}return 0.;}
fn catmull(uv:vec2<f32>)->vec3<f32>{let size=vec2<i32>(textureDimensions(src));let q=uv*vec2<f32>(size)-.5;let base=vec2<i32>(floor(q));let f=fract(q);var sum=vec3(0.);var weight=0.;for(var y:i32=-1;y<=2;y++){for(var x:i32=-1;x<=2;x++){let w=cubic(f.x-f32(x))*cubic(f.y-f32(y));sum+=textureLoad(src,clamp(base+vec2(x,y),vec2(0),size-1),0).rgb*w;weight+=w;}}return sum/max(weight,1e-5);}
@fragment fn taau_pass(i:V)->@location(0) vec4<f32> { let uv=i.uv; let current=vec4(catmull(uv+p.jitter),1.); let os=vec2<f32>(textureDimensions(hist)); let cs=vec2<f32>(textureDimensions(src)); let cp=vec2<i32>(min((uv+p.jitter)*cs,cs-vec2(1.))); let d=linear_depth(textureLoad(depth,cp,0)); let mv=textureLoad(motion,cp,0).xy; let history_uv=uv-mv; let hp=vec2<i32>(min(max(history_uv,vec2(0.))*os,os-vec2(1.))); let old_d=linear_depth(textureLoad(old_depth,hp,0).r); let n=oct_decode(textureLoad(normal,cp,0).xy); let old_n=oct_decode(textureLoad(old_normal,hp,0).xy); let m=textureLoad(material,cp,0).a; let old_m=textureLoad(old_material,hp,0).r; let r=textureLoad(reactive,cp,0).r; let reject=p.history<=0.||any(history_uv<vec2(0.))||any(history_uv>vec2(1.))||abs(d-old_d)>.5||abs(d-old_d)/max(abs(d),1e-4)>.02||dot(n,old_n)<.90||abs(m-old_m)>.01||r>=.95; var lo=vec3(1e9);var hi=vec3(-1e9);for(var y:i32=-1;y<=1;y++){for(var x:i32=-1;x<=1;x++){let sample=textureLoad(src,clamp(vec2<i32>(uv*cs)+vec2(x,y),vec2(0),vec2<i32>(cs)-1),0).rgb;let z=ycocg(sample);lo=min(lo,z);hi=max(hi,z);}} let h=rgb(clamp(ycocg(textureLoad(hist,hp,0).rgb),lo,hi)); let motion_px=length(mv*cs); let mw=mix(.92,.78,clamp(motion_px/8.,0.,1.)); let w=select(p.history*mw*(1.-.8*clamp(r,0.,1.)),0.,reject);return vec4(max(mix(current.rgb,h,w),vec3(0.)),1.); }
struct H { @location(0) depth:f32, @location(1) normal:vec2<f32>, @location(2) material:f32 }
@fragment fn history_pass(i:V)->H { let cs=vec2<f32>(textureDimensions(depth)); let cp=vec2<i32>(min(i.uv*cs,cs-vec2(1.))); let n=textureLoad(normal,cp,0); let m=textureLoad(material,cp,0); return H(linear_depth(textureLoad(depth,cp,0)),n.xy,m.a); }
"#;
const POST_WGSL: &str = r#"
struct Q { mode:f32, threshold:f32, intensity:f32, _pad:f32 } @group(0) @binding(0) var src:texture_2d<f32>; @group(0) @binding(1) var base:texture_2d<f32>; @group(0) @binding(2) var samp:sampler; @group(0) @binding(3) var<uniform> q:Q;
struct V { @builtin(position) pos:vec4<f32>, @location(0) uv:vec2<f32> } @vertex fn vs(@builtin(vertex_index) i:u32)->V { var a=array<vec2<f32>,3>(vec2(-1.),vec2(3.,-1.),vec2(-1.,3.));var o:V;o.pos=vec4(a[i],0.,1.);o.uv=a[i]*.5+.5;o.uv.y=1.-o.uv.y;return o; }
fn threshold(c:vec3<f32>)->vec3<f32>{let peak=max(max(c.r,c.g),max(c.b,0.));let soft=clamp((peak-q.threshold+.5)/1.,0.,1.);let k=max(peak-q.threshold,0.)+soft*soft*.5;return max(c,vec3(0.))*k/max(peak,1e-4);}
@fragment fn down(i:V)->@location(0) vec4<f32>{let c=textureSampleLevel(src,samp,i.uv,0.).rgb;return vec4(select(c,threshold(c),q.mode<.5),1.);} @fragment fn up(i:V)->@location(0) vec4<f32>{return vec4(textureSampleLevel(base,samp,i.uv,0.).rgb+textureSampleLevel(src,samp,i.uv,0.).rgb*q.intensity,1.);} @fragment fn composite(i:V)->@location(0) vec4<f32>{return vec4(textureSampleLevel(base,samp,i.uv,0.).rgb+textureSampleLevel(src,samp,i.uv,0.).rgb*q.intensity,1.);}
"#;
pub struct M7Effects {
    internal: (u32, u32),
    native: (u32, u32),
    format: wgpu::TextureFormat,
    targets: Targets,
    layout: wgpu::BindGroupLayout,
    exposure_pipeline: wgpu::RenderPipeline,
    atmosphere_pipeline: wgpu::RenderPipeline,
    taau_pipeline: wgpu::RenderPipeline,
    history_pipeline: wgpu::RenderPipeline,
    post_layout: wgpu::BindGroupLayout,
    post_down: wgpu::RenderPipeline,
    post_up: wgpu::RenderPipeline,
    post_composite: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    params: wgpu::Buffer,
    post_params: wgpu::Buffer,
    exposure_params: wgpu::Buffer,
    exposure_values: wgpu::Buffer,
    groups: Option<[wgpu::BindGroup; 5]>,
    down_groups: Vec<wgpu::BindGroup>,
    up_groups: Vec<wgpu::BindGroup>,
    composite_group: Option<wgpu::BindGroup>,
    history_groups: Option<[wgpu::BindGroup; 2]>,
    exposure_layout: wgpu::BindGroupLayout,
    reduce_pipeline: wgpu::ComputePipeline,
    adapt_pipeline: wgpu::ComputePipeline,
    exposure_group: Option<wgpu::BindGroup>,
    history_read: usize,
    history_valid: bool,
    frame_index: u32,
    atmosphere_luts: AtmosphereGpu,
    gtao: GtaoPass,
}
impl M7Effects {
    pub fn ao_view(&self) -> &wgpu::TextureView {
        self.gtao.view(&self.targets.ao_history)
    }

    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        internal: (u32, u32),
        native: (u32, u32),
        format: wgpu::TextureFormat,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vf/m7/effects/layout"),
            entries: &[
                tex(0),
                depth(1),
                tex(2),
                tex(3),
                tex(4),
                tex(5),
                tex(6),
                sampler(7),
                uniform(8),
                storage(9),
                tex_unfilterable(10),
                tex(11),
                tex(12),
                tex_cube(13),
                tex(14),
                tex(15),
                tex(16),
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m7/effects/shader"),
            source: wgpu::ShaderSource::Wgsl(EFFECT_WGSL.into()),
        });
        let exposure_pipeline = pipeline(device, &layout, &shader, "exposure_pass", format);
        let atmosphere_pipeline = pipeline(device, &layout, &shader, "atmosphere_pass", format);
        let taau_pipeline = pipeline(device, &layout, &shader, "taau_pass", format);
        let history_pipeline = history_pipeline(device, &layout, &shader);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vf/m7/effects/sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vf/m7/effects/params"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vf/m7/post/layout"),
            entries: &[tex(0), tex(1), self::sampler(2), uniform(3)],
        });
        let ps = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m7/post/shader"),
            source: wgpu::ShaderSource::Wgsl(POST_WGSL.into()),
        });
        let post_down = pipeline(device, &post_layout, &ps, "down", format);
        let post_up = pipeline(device, &post_layout, &ps, "up", format);
        let post_composite = pipeline(device, &post_layout, &ps, "composite", format);
        let post_params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vf/m7/post/params"),
            contents: bytemuck::cast_slice(&[0.0_f32, 1.0, 0.08, 0.0]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let (w, h) = (internal.0.max(1), internal.1.max(1));
        let exposure_params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vf/m7/exposure/params"),
            contents: bytemuck::cast_slice(&[
                w,
                h,
                0.18_f32.to_bits(),
                0.25_f32.to_bits(),
                8.0_f32.to_bits(),
                3.0_f32.to_bits(),
                (1.0_f32 / 60.0).to_bits(),
                0_u32,
            ]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let exposure_values = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vf/m7/exposure/values"),
            size: (w as u64 * h as u64 + 4) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(
            &exposure_values,
            w as u64 * h as u64 * std::mem::size_of::<f32>() as u64,
            bytemuck::cast_slice(&[1.0_f32, 1.0_f32]),
        );
        let exposure_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vf/m7/exposure/layout"),
            entries: &[compute_tex(0), storage_rw(1), compute_uniform(2)],
        });
        let es = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vf/m7/exposure/shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/exposure.wgsl").into(),
            ),
        });
        let reduce_pipeline =
            compute_pipeline(device, &exposure_layout, &es, "reduce_log_luminance");
        let adapt_pipeline = compute_pipeline(device, &exposure_layout, &es, "adapt");
        let internal = (w, h);
        let native = (native.0.max(1), native.1.max(1));
        Self {
            internal,
            native,
            format,
            targets: Targets::new(device, internal, native, format),
            layout,
            exposure_pipeline,
            atmosphere_pipeline,
            taau_pipeline,
            history_pipeline,
            post_layout,
            post_down,
            post_up,
            post_composite,
            sampler,
            params,
            post_params,
            exposure_params,
            exposure_values,
            groups: None,
            down_groups: Vec::new(),
            up_groups: Vec::new(),
            composite_group: None,
            history_groups: None,
            exposure_layout,
            reduce_pipeline,
            adapt_pipeline,
            exposure_group: None,
            history_read: 0,
            history_valid: false,
            frame_index: 0,
            atmosphere_luts: AtmosphereGpu::new(device),
            gtao: GtaoPass::new(device, queue, internal),
        }
    }
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        internal: (u32, u32),
        native: (u32, u32),
    ) {
        let i = (internal.0.max(1), internal.1.max(1));
        let n = (native.0.max(1), native.1.max(1));
        if (i, n) == (self.internal, self.native) {
            return;
        }
        *self = Self::new(device, queue, i, n, self.format);
    }
    pub fn update(&mut self, queue: &wgpu::Queue, mut p: M7EffectParams) {
        self.atmosphere_luts.update(queue, p.phase, p.sun_mu);
        self.gtao.update(queue, self.frame_index);
        if !self.history_valid {
            p.history = 0.0;
        }
        let n = self.frame_index.saturating_add(1);
        p.jitter = [
            (halton(n, 2) - 0.5) / self.internal.0 as f32,
            (halton(n, 3) - 0.5) / self.internal.1 as f32,
        ];
        p.frame = self.frame_index as f32;
        queue.write_buffer(&self.params, 0, bytemuck::bytes_of(&p));
    }
    pub fn prepare(&mut self, device: &wgpu::Device, input: M7EffectInputs<'_>) {
        let make =
            |source: &wgpu::TextureView, history: &wgpu::TextureView, history_index: usize| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("vf/m7/effects/bind"),
                    layout: &self.layout,
                    entries: &[
                        texture_entry(0, source),
                        texture_entry(1, input.depth),
                        texture_entry(2, input.normal),
                        texture_entry(3, input.material),
                        texture_entry(4, history),
                        texture_entry(5, input.motion),
                        texture_entry(6, input.reactive),
                        sampler_entry(7, &self.sampler),
                        buffer_entry(8, self.params.as_entire_binding()),
                        buffer_entry(9, self.exposure_values.as_entire_binding()),
                        texture_entry(10, &self.targets.history_depth[history_index]),
                        texture_entry(11, &self.targets.history_normal[history_index]),
                        texture_entry(12, &self.targets.history_material[history_index]),
                        texture_entry(13, input.sky),
                        texture_entry(14, &self.atmosphere_luts.sky_view),
                        texture_entry(15, &self.atmosphere_luts.transmittance),
                        texture_entry(16, &self.atmosphere_luts.multiscatter),
                    ],
                })
            };
        self.groups = Some([
            make(input.hdr, &self.targets.history[0], 0),
            make(&self.targets.exposure, &self.targets.history[0], 0),
            make(&self.targets.atmosphere, &self.targets.history[0], 0),
            make(&self.targets.bloom, &self.targets.history[0], 0),
            make(&self.targets.bloom, &self.targets.history[1], 1),
        ]);
        self.gtao.prepare(
            device,
            GtaoInputs {
                depth: input.depth,
                normal: input.normal,
                motion: input.motion,
                old_depth: &self.targets.history_depth,
                old_normal: &self.targets.history_normal,
                inverse_view_proj: input.inverse_view_proj,
                camera_pos: input.camera_pos,
                viewport: input.viewport,
            },
        );
        self.history_groups = Some([
            make(input.hdr, &self.targets.history[0], 0),
            make(input.hdr, &self.targets.history[1], 1),
        ]);
        self.down_groups.clear();
        for i in 0..self.targets.bloom_down.len() {
            let source = if i == 0 {
                &self.targets.atmosphere
            } else {
                &self.targets.bloom_down[i - 1]
            };
            self.down_groups
                .push(self.post_group(device, source, source));
        }
        self.up_groups.clear();
        for i in (0..self.targets.bloom_up.len()).rev() {
            let source = if i + 1 == self.targets.bloom_up.len() {
                &self.targets.bloom_down[i]
            } else {
                &self.targets.bloom_up[i + 1]
            };
            self.up_groups
                .push(self.post_group(device, source, &self.targets.bloom_down[i]));
        }
        self.up_groups.reverse();
        self.composite_group =
            Some(self.post_group(device, &self.targets.bloom_up[0], &self.targets.atmosphere));
        self.exposure_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m7/exposure/bind"),
            layout: &self.exposure_layout,
            entries: &[
                texture_entry(0, input.hdr),
                buffer_entry(1, self.exposure_values.as_entire_binding()),
                buffer_entry(2, self.exposure_params.as_entire_binding()),
            ],
        }));
    }
    fn post_group(
        &self,
        device: &wgpu::Device,
        source: &wgpu::TextureView,
        base: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vf/m7/post/bind"),
            layout: &self.post_layout,
            entries: &[
                texture_entry(0, source),
                texture_entry(1, base),
                sampler_entry(2, &self.sampler),
                buffer_entry(3, self.post_params.as_entire_binding()),
            ],
        })
    }
    pub fn encode_gtao(
        &mut self,
        e: &mut wgpu::CommandEncoder,
        t: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        self.gtao.encode(e, t);
    }
    pub fn encode_exposure(
        &self,
        e: &mut wgpu::CommandEncoder,
        t: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        if let Some(g) = &self.groups {
            if let Some(x) = &self.exposure_group {
                let mut c = e.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("vf/m7/exposure-reduce"),
                    timestamp_writes: None,
                });
                c.set_pipeline(&self.reduce_pipeline);
                c.set_bind_group(0, x, &[]);
                c.dispatch_workgroups(
                    self.internal.0.div_ceil(16),
                    self.internal.1.div_ceil(16),
                    1,
                );
                c.set_pipeline(&self.adapt_pipeline);
                c.dispatch_workgroups(1, 1, 1);
            }
            pass(
                e,
                "vf/m7/exposure",
                &self.targets.exposure,
                &self.exposure_pipeline,
                &g[0],
                t,
            );
        }
    }
    pub fn encode_atmosphere(
        &mut self,
        e: &mut wgpu::CommandEncoder,
        t: Option<wgpu::RenderPassTimestampWrites<'_>>,
        sky_cubemap: &wgpu::Texture,
    ) {
        self.atmosphere_luts.encode(e, sky_cubemap);
        if let Some(g) = &self.groups {
            pass(
                e,
                "vf/m7/atmosphere",
                &self.targets.atmosphere,
                &self.atmosphere_pipeline,
                &g[1],
                t,
            );
        }
    }
    pub fn encode_bloom(
        &self,
        e: &mut wgpu::CommandEncoder,
        t: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        for (i, g) in self.down_groups.iter().enumerate() {
            const DOWN_LABELS: [&str; 6] = [
                "vf/m7/bloom/down-0",
                "vf/m7/bloom/down-1",
                "vf/m7/bloom/down-2",
                "vf/m7/bloom/down-3",
                "vf/m7/bloom/down-4",
                "vf/m7/bloom/down-5",
            ];
            pass(
                e,
                DOWN_LABELS[i],
                &self.targets.bloom_down[i],
                &self.post_down,
                g,
                None,
            );
        }
        for (i, g) in self.up_groups.iter().enumerate() {
            pass(
                e,
                [
                    "vf/m7/bloom/up-0",
                    "vf/m7/bloom/up-1",
                    "vf/m7/bloom/up-2",
                    "vf/m7/bloom/up-3",
                    "vf/m7/bloom/up-4",
                    "vf/m7/bloom/up-5",
                ][i],
                &self.targets.bloom_up[i],
                &self.post_up,
                g,
                None,
            );
        }
        if let Some(g) = &self.composite_group {
            pass(
                e,
                "vf/m7/bloom/composite",
                &self.targets.bloom,
                &self.post_composite,
                g,
                t,
            );
        }
    }
    pub fn encode_taau(
        &mut self,
        e: &mut wgpu::CommandEncoder,
        t: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        if let Some(g) = &self.groups {
            let w = 1 - self.history_read;
            pass(
                e,
                "vf/m7/taau",
                &self.targets.history[w],
                &self.taau_pipeline,
                &g[3 + self.history_read],
                t,
            );
            if let Some(group) = self
                .history_groups
                .as_ref()
                .and_then(|groups| groups.get(self.history_read))
            {
                history_pass(
                    e,
                    &self.targets.history_depth[w],
                    &self.targets.history_normal[w],
                    &self.targets.history_material[w],
                    &self.history_pipeline,
                    group,
                );
            }
            self.history_read = w;
            self.history_valid = true;
            self.frame_index = self.frame_index.saturating_add(1);
        }
    }
    pub fn history_views(&self) -> [&wgpu::TextureView; 2] {
        [&self.targets.history[0], &self.targets.history[1]]
    }
    pub fn history_index(&self) -> usize {
        self.history_read
    }
    pub fn history_reset(&mut self) {
        self.history_read = 0;
        self.history_valid = false;
        self.frame_index = 0;
        self.gtao.reset();
    }
    pub fn texture_memory_bytes(&self) -> usize {
        self.targets
            ._textures
            .iter()
            .map(crate::render::memory::texture_bytes)
            .sum()
    }
}
