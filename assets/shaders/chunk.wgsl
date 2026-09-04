struct Globals {
    view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    sun_dir: vec4<f32>,
    time_res: vec4<f32>,
};

struct ChunkUniform {
    origin: vec4<i32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var block_tex: texture_2d_array<f32>;
@group(1) @binding(1) var block_samp: sampler;
@group(2) @binding(0) var<uniform> chunk: ChunkUniform;

struct VertexInput {
    @location(0) packed: vec2<u32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) tex: i32,
    @location(2) shade: f32,
    @location(3) ao: f32,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    let x = input.packed.x & 63u;
    let y = (input.packed.x >> 6u) & 63u;
    let z = (input.packed.x >> 12u) & 63u;
    let face = (input.packed.x >> 18u) & 7u;
    let ao = (input.packed.x >> 21u) & 3u;
    let tex = input.packed.y & 65535u;
    let light = (input.packed.y >> 16u) & 15u;
    let sky = (input.packed.y >> 20u) & 15u;
    let local = vec3<f32>(f32(x), f32(y), f32(z));
    let world_pos = vec3<f32>(chunk.origin.xyz) + local;

    var uv = vec2<f32>(local.x, local.z);
    if (face == 0u || face == 1u) {
        uv = vec2<f32>(local.z, -local.y);
    } else if (face == 2u || face == 3u) {
        uv = vec2<f32>(local.x, local.z);
    } else if (face == 4u || face == 5u) {
        uv = vec2<f32>(local.x, -local.y);
    }

    var face_shade = 0.65;
    if (face == 2u) {
        face_shade = 1.0;
    } else if (face == 3u) {
        face_shade = 0.55;
    } else if (face == 0u || face == 1u) {
        face_shade = 0.80;
    }
    // M3 carries sky/light in the packed vertex for later lighting tiers.
    let packed_light = f32(light + sky * 0u);
    let ao_factor = 0.35 + 0.65 * f32(ao) / 3.0;

    var output: VertexOutput;
    output.clip_position = globals.view_proj * vec4<f32>(world_pos, 1.0);
    output.uv = uv;
    output.tex = i32(tex);
    output.shade = face_shade + packed_light * 0.0;
    output.ao = ao_factor;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(block_tex, block_samp, input.uv, input.tex).rgb;
    return vec4<f32>(color * input.shade * input.ao, 1.0);
}
