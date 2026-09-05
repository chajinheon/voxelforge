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
    @location(6) water_world: vec3<f32>,
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
    let frac = vec3<f32>(
        f32((input.packed.x >> 24u) & 15u),
        f32((input.packed.x >> 28u) & 15u),
        f32((input.packed.y >> 24u) & 15u)
    ) / 16.0;
    let local = vec3<f32>(f32(x), f32(y), f32(z)) + frac;
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
    } else if (tex == 5u && lowered) {
        // WATER top and exposed upper side corners carry the lowered bit. The
        // four deterministic Gerstner components sum to <= 0.149 blocks.
        world_pos += water_displacement(base_world_pos, globals.time_res.x);
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
    output.water_world = base_world_pos;
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

const WATER_MAX_DISPLACEMENT: f32 = 0.149;
const LOD_BAYER: array<f32, 64> = array<f32, 64>(
    0.0, 48.0, 12.0, 60.0, 3.0, 51.0, 15.0, 63.0,
    32.0, 16.0, 44.0, 28.0, 35.0, 19.0, 47.0, 31.0,
    8.0, 56.0, 4.0, 52.0, 11.0, 59.0, 7.0, 55.0,
    40.0, 24.0, 36.0, 20.0, 43.0, 27.0, 39.0, 23.0,
    2.0, 50.0, 14.0, 62.0, 1.0, 49.0, 13.0, 61.0,
    34.0, 18.0, 46.0, 30.0, 33.0, 17.0, 45.0, 29.0,
    10.0, 58.0, 6.0, 54.0, 9.0, 57.0, 5.0, 53.0,
    42.0, 26.0, 38.0, 22.0, 41.0, 25.0, 37.0, 21.0);

fn water_displacement(world_pos: vec3<f32>, time: f32) -> vec3<f32> {
    let p = world_pos.xz;
    let d0 = vec2<f32>(0.9805807, 0.1961161);
    let d1 = vec2<f32>(-0.6, 0.8);
    let d2 = vec2<f32>(0.2873479, -0.9578263);
    let d3 = vec2<f32>(-0.9363292, -0.3511234);
    let k0 = 0.3490659;
    let k1 = 0.6981317;
    let k2 = 1.3962634;
    let k3 = 2.7925268;
    let phase0 = k0 * dot(d0, p) - 0.4188790 * time;
    let phase1 = k1 * dot(d1, p) - 0.6981317 * time;
    let phase2 = k2 * dot(d2, p) - 1.1170107 * time;
    let phase3 = k3 * dot(d3, p) - 1.6755161 * time;
    var horizontal = vec2<f32>(0.0);
    horizontal += 0.35 * 0.075 * d0 * cos(phase0);
    horizontal += 0.30 * 0.040 * d1 * cos(phase1);
    horizontal += 0.22 * 0.022 * d2 * cos(phase2);
    horizontal += 0.15 * 0.012 * d3 * cos(phase3);
    let vertical = clamp(
        0.075 * sin(phase0)
            + 0.040 * sin(phase1)
            + 0.022 * sin(phase2)
            + 0.012 * sin(phase3),
        -WATER_MAX_DISPLACEMENT,
        WATER_MAX_DISPLACEMENT,
    );
    return vec3<f32>(horizontal.x, vertical, horizontal.y);
}

fn water_normal(world_pos: vec3<f32>, time: f32) -> vec3<f32> {
    let p = world_pos.xz;
    let d0 = vec2<f32>(0.9805807, 0.1961161);
    let d1 = vec2<f32>(-0.6, 0.8);
    let d2 = vec2<f32>(0.2873479, -0.9578263);
    let d3 = vec2<f32>(-0.9363292, -0.3511234);
    let k0 = 0.3490659;
    let k1 = 0.6981317;
    let k2 = 1.3962634;
    let k3 = 2.7925268;
    let phase0 = k0 * dot(d0, p) - 0.4188790 * time;
    let phase1 = k1 * dot(d1, p) - 0.6981317 * time;
    let phase2 = k2 * dot(d2, p) - 1.1170107 * time;
    let phase3 = k3 * dot(d3, p) - 1.6755161 * time;
    var tangent_x = vec3<f32>(1.0, 0.0, 0.0);
    var tangent_z = vec3<f32>(0.0, 0.0, 1.0);
    let a0 = 0.075 * k0;
    let a1 = 0.040 * k1;
    let a2 = 0.022 * k2;
    let a3 = 0.012 * k3;
    let q0 = 0.35 * a0;
    let q1 = 0.30 * a1;
    let q2 = 0.22 * a2;
    let q3 = 0.15 * a3;
    let s0 = sin(phase0);
    let s1 = sin(phase1);
    let s2 = sin(phase2);
    let s3 = sin(phase3);
    let c0 = cos(phase0);
    let c1 = cos(phase1);
    let c2 = cos(phase2);
    let c3 = cos(phase3);
    tangent_x += vec3<f32>(
        -q0 * d0.x * d0.x * s0,
        a0 * d0.x * c0,
        -q0 * d0.y * d0.x * s0,
    );
    tangent_z += vec3<f32>(
        -q0 * d0.x * d0.y * s0,
        a0 * d0.y * c0,
        -q0 * d0.y * d0.y * s0,
    );
    tangent_x += vec3<f32>(
        -q1 * d1.x * d1.x * s1,
        a1 * d1.x * c1,
        -q1 * d1.y * d1.x * s1,
    );
    tangent_z += vec3<f32>(
        -q1 * d1.x * d1.y * s1,
        a1 * d1.y * c1,
        -q1 * d1.y * d1.y * s1,
    );
    tangent_x += vec3<f32>(
        -q2 * d2.x * d2.x * s2,
        a2 * d2.x * c2,
        -q2 * d2.y * d2.x * s2,
    );
    tangent_z += vec3<f32>(
        -q2 * d2.x * d2.y * s2,
        a2 * d2.y * c2,
        -q2 * d2.y * d2.y * s2,
    );
    tangent_x += vec3<f32>(
        -q3 * d3.x * d3.x * s3,
        a3 * d3.x * c3,
        -q3 * d3.y * d3.x * s3,
    );
    tangent_z += vec3<f32>(
        -q3 * d3.x * d3.y * s3,
        a3 * d3.y * c3,
        -q3 * d3.y * d3.y * s3,
    );
    return normalize(cross(tangent_z, tangent_x));
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let sampled = textureSample(block_tex, block_samp, input.uv, input.tex);
    let illumination = light_illumination(input);
    var color = sampled.rgb * input.shade * input.ao * illumination;
    if (input.tex == 5) {
        let normal = water_normal(input.water_world, globals.time_res.x);
        let light_dir = normalize(-globals.sun_dir.xyz);
        let view_dir = normalize(globals.cam_pos.xyz - input.water_world);
        let half_dir = normalize(light_dir + view_dir);
        let diffuse = 0.72 + 0.38 * max(dot(normal, light_dir), 0.0);
        let fresnel = pow(1.0 - max(dot(normal, view_dir), 0.0), 5.0);
        let specular = pow(max(dot(normal, half_dir), 0.0), 36.0);
        let ripple = 0.5 + 0.5 * sin(dot(input.water_world.xz, vec2<f32>(0.31, -0.22)) + globals.time_res.x * 1.4);
        color = color * diffuse
            + vec3<f32>(0.015, 0.055, 0.095) * (fresnel + ripple * 0.35)
            + vec3<f32>(0.22, 0.34, 0.42) * specular;
    }
    let d = distance(input.water_world.xz, globals.cam_pos.xz);
    if (d >= 288.0 && d < 320.0) {
        let p = vec2<i32>(floor(input.clip_position.xy));
        let threshold = (LOD_BAYER[(p.y & 7) * 8 + (p.x & 7)] + 0.5) / 64.0;
        let coarse_weight = (d - 288.0) / 32.0;
        if (threshold < coarse_weight) { discard; }
    }
    return vec4<f32>(color, sampled.a);
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
