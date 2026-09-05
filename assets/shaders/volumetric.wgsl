// M8 quarter-resolution checkerboard volumetric fog/light march.
struct Globals { view_proj: mat4x4<f32>, cam_pos: vec4<f32>, sun_dir: vec4<f32>, time_res: vec4<f32>, sky_color: vec4<f32>, unjittered_view_proj: mat4x4<f32>, previous_view_proj: mat4x4<f32>, };
struct VolumetricParams { max_distance: f32, density_scale: f32, history_weight: f32, underwater: f32, sun_height: f32, parity: vec2<u32>, _padding: u32, };
struct FogLight { position: vec3<f32>, radius: f32, color: vec3<f32>, _padding: f32, };
@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<uniform> params: VolumetricParams;
@group(0) @binding(2) var<storage, read> lights: array<FogLight, 8>;
@group(0) @binding(3) var cloud_shadow_map: texture_2d<f32>;
struct M8Camera { inverse_view_proj: mat4x4<f32>, previous_view_proj: mat4x4<f32>, viewport: vec4<f32>, };
@group(0) @binding(4) var<uniform> camera: M8Camera;
@group(0) @binding(5) var csm_shadow_map: texture_depth_2d_array;
@group(0) @binding(6) var csm_sampler: sampler_comparison;
@group(1) @binding(0) var blue_noise: texture_2d<f32>;
@group(1) @binding(1) var noise_sampler: sampler;
@group(1) @binding(2) var output_fog: texture_storage_2d<rgba16float, write>;
@group(1) @binding(3) var linear_depth: texture_2d<f32>;
@group(1) @binding(4) var gbuffer_normal: texture_2d<f32>;
@group(1) @binding(5) var history_fog: texture_2d<f32>;
@group(1) @binding(6) var depth_sampler: sampler;

fn phase_hg(mu: f32) -> f32 {
    let g = 0.65;
    let denominator = max(1.0 + g * g - 2.0 * g * mu, 1.0e-4);
    return (1.0 - g * g) / (12.5663706 * pow(denominator, 1.5));
}
fn decode_oct(encoded: vec2<f32>) -> vec3<f32> {
    var n = vec3<f32>(encoded, 1.0 - abs(encoded.x) - abs(encoded.y));
    if (n.z < 0.0) { n = vec3<f32>((1.0 - abs(n.y)) * sign(n.x), (1.0 - abs(n.x)) * sign(n.y), n.z); }
    return normalize(n);
}

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    let output_size = vec2<f32>(textureDimensions(output_fog));
    if (id.x >= u32(output_size.x) || id.y >= u32(output_size.y)) { return; }
    if ((id.x & 1u) != params.parity.x || (id.y & 1u) != params.parity.y) { return; }
    let uv = (vec2<f32>(id.xy) + 0.5) / output_size;
    let jitter = textureLoad(blue_noise, vec2<i32>(id.xy & vec2<u32>(63u)), 0).r;
    let depth = textureSampleLevel(linear_depth, depth_sampler, uv, 0.0).r;
    let normal = decode_oct(textureSampleLevel(gbuffer_normal, depth_sampler, uv, 0.0).rg);
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 1.0, 1.0);
    let far_world4 = camera.inverse_view_proj * ndc;
    let far_world = far_world4.xyz / max(far_world4.w, 0.0001);
    let ray = normalize(far_world - globals.cam_pos.xyz);
    let march_distance = min(params.max_distance, max(depth, 1.0));
    let reprojection_world = globals.cam_pos.xyz + ray * min(march_distance * 0.5, params.max_distance);
    let step_length = params.max_distance / 32.0;
    var transmittance = 1.0;
    var scattering = vec3<f32>(0.0);
    for (var i: u32 = 0u; i < 32u; i++) {
        let sample_distance = (f32(i) + 0.5 + jitter - 0.5) * step_length;
        let sample_position = globals.cam_pos.xyz + ray * min(f32(i) * step_length, march_distance);
        let height = sample_position.y - 63.0;
        let density = (0.0015 + 0.0060 * exp(-max(height, 0.0) * 0.025)) * params.density_scale * select(1.0, 3.0, params.underwater > 0.5);
        var local = vec3<f32>(0.0);
        for (var light_index: u32 = 0u; light_index < 8u; light_index++) {
            let light = lights[light_index];
            let radius = max(light.radius, 0.001);
            let falloff = clamp(1.0 - distance(sample_position, light.position) / radius, 0.0, 1.0);
            local += light.color * falloff * falloff;
        }
        let shadow_uv = fract(sample_position.xz / 1024.0 + vec2<f32>(0.5));
        let cloud_shadow = 0.55 + 0.45 * textureSampleLevel(cloud_shadow_map, noise_sampler, shadow_uv, 0.0).r;
        let csm_near = textureSampleCompareLevel(csm_shadow_map, csm_sampler, shadow_uv, 0, 0.5);
        let csm_far = textureSampleCompareLevel(csm_shadow_map, csm_sampler, shadow_uv, 1, 0.5);
        let csm_shadow = 0.5 * (csm_near + csm_far);
        let shadow = cloud_shadow * (0.65 + 0.35 * csm_shadow);
        let sun = select(vec3<f32>(0.0), globals.sun_dir.www * phase_hg(dot(normalize(globals.sun_dir.xyz), vec3<f32>(0.0, 0.0, 1.0))) * shadow, params.sun_height > 0.02);
        scattering += transmittance * (sun + local) * density * step_length;
        transmittance *= exp(-density * step_length);
        if (sample_distance > params.max_distance || transmittance < 1.0e-4) { break; }
    }
    let previous_clip = camera.previous_view_proj * vec4<f32>(reprojection_world, 1.0);
    let previous_uv = previous_clip.xy / max(previous_clip.w, 0.0001) * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    let history_uv = clamp(previous_uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let history = textureSampleLevel(history_fog, noise_sampler, history_uv, 0.0);
    let previous_depth = textureSampleLevel(linear_depth, depth_sampler, history_uv, 0.0).r;
    let previous_normal = decode_oct(textureSampleLevel(gbuffer_normal, depth_sampler, history_uv, 0.0).rg);
    let outside = any(previous_uv < vec2<f32>(0.0)) || any(previous_uv > vec2<f32>(1.0));
    let reject = select(params.history_weight, 0.0, outside || depth < 0.001 || abs(depth - previous_depth) > 0.35 || dot(normal, previous_normal) < 0.82);
    textureStore(output_fog, vec2<i32>(id.xy), mix(vec4<f32>(scattering, transmittance), history, reject));
}

// The same module owns the final depth/normal-aware reconstruction. Keeping
// this entry point with the live volumetric shader prevents the runtime graph
// from silently falling back to the generic post-process placeholder.
@group(2) @binding(0) var composite_scene: texture_2d<f32>;
@group(2) @binding(1) var composite_depth: texture_2d<f32>;
@group(2) @binding(2) var composite_normal: texture_2d<f32>;
@group(2) @binding(3) var composite_fog: texture_2d<f32>;
@group(2) @binding(4) var composite_cloud: texture_2d<f32>;
@group(2) @binding(5) var composite_sampler: sampler;
@group(2) @binding(6) var composite_depth_sampler: sampler;
struct CompositeParams { debug_view: u32, _padding: vec3<u32>, };
@group(2) @binding(7) var<uniform> composite_params: CompositeParams;

struct CompositeVertex {
  @builtin(position) position: vec4<f32>,
  @location(0) uv: vec2<f32>,
};

@vertex
fn vs_composite(@builtin(vertex_index) index: u32) -> CompositeVertex {
  let positions = array<vec2<f32>, 3>(
    vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
  var out: CompositeVertex;
  out.position = vec4<f32>(positions[index], 0.0, 1.0);
  out.uv = vec2<f32>(positions[index].x * 0.5 + 0.5,
                     0.5 - positions[index].y * 0.5);
  return out;
}

fn quarter_sample(tex: texture_2d<f32>, uv: vec2<f32>) -> vec4<f32> {
  return textureSampleLevel(tex, composite_sampler, clamp(uv, vec2(0.0), vec2(1.0)), 0.0);
}

@fragment
fn fs_composite(input: CompositeVertex) -> @location(0) vec4<f32> {
  let scene = textureSampleLevel(composite_scene, composite_sampler, input.uv, 0.0);
  let texel = 1.0 / vec2<f32>(textureDimensions(composite_fog));
  let center = quarter_sample(composite_fog, input.uv);
  let c0 = quarter_sample(composite_fog, input.uv + vec2(texel.x, 0.0));
  let c1 = quarter_sample(composite_fog, input.uv - vec2(texel.x, 0.0));
  let c2 = quarter_sample(composite_fog, input.uv + vec2(0.0, texel.y));
  let c3 = quarter_sample(composite_fog, input.uv - vec2(0.0, texel.y));
  let cloud = quarter_sample(composite_cloud, input.uv);
  let average = (center + c0 + c1 + c2 + c3) * 0.2;
  let depth = textureSampleLevel(composite_depth, composite_depth_sampler, input.uv, 0.0).r;
  let depth_neighbor = textureSampleLevel(composite_depth, composite_depth_sampler,
      input.uv + vec2(0.004, 0.0), 0.0).r;
  let normal = decode_oct(textureSampleLevel(composite_normal, composite_sampler, input.uv, 0.0).rg);
  let normal_weight = clamp(dot(normal, vec3<f32>(0.0, 0.0, 1.0)) + 0.25, 0.25, 1.0);
  let depth_weight = exp(-abs(depth_neighbor - depth) * 0.05);
  let fog = mix(center, average, 0.55 * normal_weight * depth_weight);
  let transmittance = clamp(fog.a, 0.0, 1.0);
  let cloud_amount = clamp(1.0 - cloud.a, 0.0, 1.0);
  if (composite_params.debug_view == 2u) { return vec4<f32>(fog.rgb, 1.0); }
  if (composite_params.debug_view == 3u) { return vec4<f32>(cloud.rgb, 1.0); }
  let delta = fog.rgb + cloud.rgb;
  return vec4<f32>(scene.rgb * transmittance + delta * (0.65 + 0.35 * cloud_amount), 1.0);
}
