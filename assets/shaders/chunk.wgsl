struct Globals {
    view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    sun_dir: vec4<f32>,
    time_res: vec4<f32>,
    sky_color: vec4<f32>,
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
    @location(4) block_light: f32,
    @location(5) sky_light: f32,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    let x = input.packed.x & 63u;
    let y = (input.packed.x >> 6u) & 63u;
    let z = (input.packed.x >> 12u) & 63u;
    let face = (input.packed.x >> 18u) & 7u;
    let ao = (input.packed.x >> 21u) & 3u;
    let lowered = (input.packed.x & 0x00800000u) != 0u;
    let tex = input.packed.y & 65535u;
    let block_light = (input.packed.y >> 16u) & 15u;
    let sky_light = (input.packed.y >> 20u) & 15u;
    let local = vec3<f32>(f32(x), f32(y), f32(z));
    var world_local = local;
    if (lowered) {
        world_local.y -= 0.125;
    }
    let base_world_pos = vec3<f32>(chunk.origin.xyz) + world_local;
    var world_pos = base_world_pos;
    if (tex == 8u) {
        let offset = wind_offset(base_world_pos, 0.045);
        world_pos.x += offset.x;
        world_pos.z += offset.y;
    } else if (tex == 2u && face == 2u) {
        let offset = wind_offset(base_world_pos, 0.012);
        world_pos.x += offset.x;
        world_pos.z += offset.y;
    }

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
    let ao_factor = 0.35 + 0.65 * f32(ao) / 3.0;

    var output: VertexOutput;
    output.clip_position = globals.view_proj * vec4<f32>(world_pos, 1.0);
    output.uv = uv;
    output.tex = i32(tex);
    output.shade = face_shade;
    output.ao = ao_factor;
    output.block_light = f32(block_light);
    output.sky_light = f32(sky_light);
    return output;
}

const LIGHT_LEVEL: array<f32, 16> = array<f32, 16>(
    0.0351844, 0.0439805, 0.0549756, 0.0687195,
    0.0858993, 0.1073742, 0.1342177, 0.1677722,
    0.2097152, 0.2621440, 0.3276800, 0.4096000,
    0.5120000, 0.6400000, 0.8000000, 1.0000000
);

fn wind_offset(world_pos: vec3<f32>, amplitude: f32) -> vec2<f32> {
    let phase = dot(world_pos.xz, vec2<f32>(0.173, 0.127)) + globals.time_res.x * 1.7;
    let gust = (sin(phase) + 0.5 * sin(phase * 2.17 + 1.3)) / 1.5;
    return normalize(vec2<f32>(0.8, 0.6)) * (gust * amplitude);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let sampled = textureSample(block_tex, block_samp, input.uv, input.tex);
    let illumination = light_illumination(input);
    return vec4<f32>(sampled.rgb * input.shade * input.ao * illumination, sampled.a);
}

fn light_illumination(input: VertexOutput) -> f32 {
    let block_curve = LIGHT_LEVEL[u32(round(input.block_light))];
    let sky_curve = LIGHT_LEVEL[u32(round(input.sky_light))] * globals.sun_dir.w;
    return max(block_curve, sky_curve);
}

@fragment
fn fs_light(input: VertexOutput) -> @location(0) vec4<f32> {
    let illumination = light_illumination(input);
    return vec4<f32>(illumination, illumination, illumination, 1.0);
}
