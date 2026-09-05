// M8 quarter-resolution Perlin-Worley cloud march.
struct Globals { view_proj: mat4x4<f32>, cam_pos: vec4<f32>, sun_dir: vec4<f32>, time_res: vec4<f32>, sky_color: vec4<f32>, unjittered_view_proj: mat4x4<f32>, previous_view_proj: mat4x4<f32>, };
struct CloudParams { base_altitude: f32, top_altitude: f32, coverage: f32, max_distance: f32, history_weight: f32, parity: vec2<u32>, _padding: u32, };
@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<uniform> params: CloudParams;
struct M8Camera { inverse_view_proj: mat4x4<f32>, previous_view_proj: mat4x4<f32>, viewport: vec4<f32>, };
@group(0) @binding(4) var<uniform> camera: M8Camera;
@group(0) @binding(5) var csm_shadow_map: texture_depth_2d_array;
@group(0) @binding(6) var csm_sampler: sampler_comparison;
@group(1) @binding(0) var base_noise: texture_3d<f32>;
@group(1) @binding(1) var detail_noise: texture_3d<f32>;
@group(1) @binding(2) var noise_sampler: sampler;
@group(1) @binding(3) var cloud_output: texture_storage_2d<rgba16float, write>;
@group(1) @binding(4) var linear_depth: texture_2d<f32>;
@group(1) @binding(5) var gbuffer_normal: texture_2d<f32>;
@group(1) @binding(6) var cloud_history: texture_2d<f32>;
@group(1) @binding(7) var depth_sampler: sampler;

fn cloud_height(world_y: f32) -> f32 { return clamp((world_y - params.base_altitude) / max(params.top_altitude - params.base_altitude, 1.0e-4), 0.0, 1.0); }
fn smoothstep01(value: f32) -> f32 { let x = clamp(value, 0.0, 1.0); return x * x * (3.0 - 2.0 * x); }
fn decode_oct(encoded: vec2<f32>) -> vec3<f32> {
    var n = vec3<f32>(encoded, 1.0 - abs(encoded.x) - abs(encoded.y));
    if (n.z < 0.0) { n = vec3<f32>((1.0 - abs(n.y)) * sign(n.x), (1.0 - abs(n.x)) * sign(n.y), n.z); }
    return normalize(n);
}

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    let output_size = vec2<f32>(textureDimensions(cloud_output));
    if (id.x >= u32(output_size.x) || id.y >= u32(output_size.y)) { return; }
    if ((id.x & 1u) != params.parity.x || (id.y & 1u) != params.parity.y) { return; }
    let uv = (vec2<f32>(id.xy) + 0.5) / output_size;
    let depth = textureSampleLevel(linear_depth, depth_sampler, uv, 0.0).r;
    let normal = decode_oct(textureSampleLevel(gbuffer_normal, depth_sampler, uv, 0.0).rg);
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 1.0, 1.0);
    let far_world4 = camera.inverse_view_proj * ndc;
    let far_world = far_world4.xyz / max(far_world4.w, 0.0001);
    let ray = normalize(far_world - globals.cam_pos.xyz);
    var transmittance = 1.0;
    var color = vec3<f32>(0.0);
    for (var i: u32 = 0u; i < 40u; i++) {
        let t = (f32(i) + 0.5) / 40.0;
        let world_y = globals.cam_pos.y + ray.y * t * params.max_distance;
        let h = cloud_height(world_y);
        let shape = smoothstep01(h / 0.15) * (1.0 - smoothstep01((h - 0.70) / 0.30));
        let world_xz = globals.cam_pos.xz + ray.xz * t * params.max_distance;
        let uvw = vec3<f32>(world_xz / 128.0, h);
        let base = textureSampleLevel(base_noise, noise_sampler, uvw, 0.0).r;
        let detail = textureSampleLevel(detail_noise, noise_sampler, uvw * vec3<f32>(4.0, 2.0, 4.0), 0.0).r;
        let density = clamp((base - (1.0 - params.coverage) * 0.60 - detail * 0.25) * 2.0, 0.0, 1.0) * shape;
        var sun_transmittance = 1.0;
        for (var sun_step: u32 = 0u; sun_step < 6u; sun_step++) {
            let sun_position = vec3<f32>(world_xz, world_y) + normalize(globals.sun_dir.xyz) * (f32(sun_step) + 0.5) * 16.0;
            let sun_h = cloud_height(sun_position.y);
            let sun_uvw = vec3<f32>(sun_position.xz / 128.0, sun_h);
            let sun_base = textureSampleLevel(base_noise, noise_sampler, sun_uvw, 0.0).r;
            let sun_detail = textureSampleLevel(detail_noise, noise_sampler, sun_uvw * vec3<f32>(4.0, 2.0, 4.0), 0.0).r;
            let sun_density = clamp((sun_base - (1.0 - params.coverage) * 0.60 - sun_detail * 0.25) * 2.0, 0.0, 1.0);
            sun_transmittance *= exp(-sun_density * 0.04);
        }
        let csm_uv = fract(world_xz / 1024.0 + vec2<f32>(0.5));
        let csm_near = textureSampleCompareLevel(csm_shadow_map, csm_sampler, csm_uv, 0, 0.5);
        let csm_far = textureSampleCompareLevel(csm_shadow_map, csm_sampler, csm_uv, 1, 0.5);
        let csm_shadow = 0.5 * (csm_near + csm_far);
        color += transmittance * globals.sun_dir.www * vec3<f32>(1.0, 0.96, 0.88) * density * sun_transmittance * (0.65 + 0.35 * csm_shadow) * 0.025;
        transmittance *= exp(-density * 0.025);
        if (depth > 0.0 && t * params.max_distance > depth) { break; }
        if (transmittance < 1.0e-4) { break; }
    }
    let reprojection_clip = camera.previous_view_proj * vec4<f32>(globals.cam_pos.xyz + ray * params.max_distance * 0.5, 1.0);
    let previous_uv = reprojection_clip.xy / max(reprojection_clip.w, 0.0001) * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    let history_uv = clamp(previous_uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let history = textureSampleLevel(cloud_history, noise_sampler, history_uv, 0.0);
    let previous_depth = textureSampleLevel(linear_depth, depth_sampler, history_uv, 0.0).r;
    let previous_normal = decode_oct(textureSampleLevel(gbuffer_normal, depth_sampler, history_uv, 0.0).rg);
    let outside = any(previous_uv < vec2<f32>(0.0)) || any(previous_uv > vec2<f32>(1.0));
    let reject = select(params.history_weight, 0.0, outside || depth < 0.001 || abs(depth - previous_depth) > 0.35 || dot(normal, previous_normal) < 0.82);
    textureStore(cloud_output, vec2<i32>(id.xy), mix(vec4<f32>(color, transmittance), history, reject));
}
