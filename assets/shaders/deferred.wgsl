struct Globals { view_proj: mat4x4<f32>, cam_pos: vec4<f32>, sun_dir: vec4<f32>, time_res: vec4<f32>, sky_color: vec4<f32>, };
@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var g_albedo: texture_2d<f32>;
@group(1) @binding(1) var g_normal: texture_2d<f32>;
@group(1) @binding(2) var g_light: texture_2d<f32>;
@group(1) @binding(3) var g_motion: texture_2d<f32>;
@group(1) @binding(4) var g_reactive: texture_2d<f32>;
@group(1) @binding(5) var g_depth: texture_depth_2d;
@group(1) @binding(6) var g_sampler: sampler;
@group(1) @binding(7) var sky: texture_cube<f32>;
@group(1) @binding(8) var shadow_map: texture_depth_2d_array;
@group(1) @binding(9) var shadow_sampler: sampler_comparison;
@group(1) @binding(10) var g_ao: texture_2d<f32>;
@group(1) @binding(11) var cloud_shadow: texture_2d<f32>;
struct DebugUniform { mode: u32, exposure: f32, pad0: vec2<u32>, };
@group(2) @binding(0) var<uniform> debug: DebugUniform;
struct ShadowUniform { inverse_view_proj: mat4x4<f32>, light_view_proj: array<mat4x4<f32>, 3>, splits: vec4<f32>, };
@group(3) @binding(0) var<uniform> shadow: ShadowUniform;
struct CloudShadowWorld { origin: vec2<f32>, world_size: f32, _padding: f32, };
@group(3) @binding(1) var<uniform> cloud_shadow_world: CloudShadowWorld;
struct VertexOutput { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, };
@vertex fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
  var positions = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
  var out: VertexOutput; out.position = vec4<f32>(positions[index], 0.0, 1.0); out.uv = positions[index] * 0.5 + vec2<f32>(0.5); out.uv.y = 1.0 - out.uv.y; return out;
}
fn decode_oct(encoded: vec2<f32>) -> vec3<f32> {
  var n = vec3<f32>(encoded, 1.0 - abs(encoded.x) - abs(encoded.y));
  if (n.z < 0.0) { n = vec3<f32>((1.0 - abs(n.y)) * sign(n.x), (1.0 - abs(n.x)) * sign(n.y), n.z); }
  return normalize(n);
}
fn fresnel(f0: vec3<f32>, cos_theta: f32) -> vec3<f32> { return f0 + (vec3<f32>(1.0) - f0) * pow(max(1.0 - cos_theta, 0.0), 5.0); }
fn smith_g1(ndot: f32, roughness: f32) -> f32 { let k = (roughness + 1.0) * (roughness + 1.0) / 8.0; return ndot / max(ndot * (1.0 - k) + k, 1e-4); }
fn cook_torrance(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, albedo: vec3<f32>, metallic: f32, roughness: f32) -> vec3<f32> {
  let h = normalize(v + l); let nv = max(dot(n, v), 0.0); let nl = max(dot(n, l), 0.0); let nh = max(dot(n, h), 0.0); let vh = max(dot(v, h), 0.0);
  let d = ggx(nh, roughness); let g = smith_g1(nv, roughness) * smith_g1(nl, roughness); let f = fresnel(mix(vec3<f32>(0.04), albedo, metallic), vh);
  return ((vec3<f32>(1.0) - f) * (1.0 - metallic) * albedo / PI + d * g * f / max(4.0 * nv * nl, 1e-4)) * nl;
}
const SHADOW_POISSON: array<vec2<f32>, 12> = array<vec2<f32>, 12>(
  vec2<f32>(-0.94201624, -0.39906216), vec2<f32>(0.94558610, -0.76890725),
  vec2<f32>(-0.09418410, -0.92938870), vec2<f32>(0.34495938, 0.29387760),
  vec2<f32>(-0.91588580, 0.45771432), vec2<f32>(-0.81544230, -0.87912464),
  vec2<f32>(-0.38277543, 0.27676845), vec2<f32>(0.97484400, 0.75648380),
  vec2<f32>(0.44323325, -0.97511554), vec2<f32>(0.53742980, -0.47373420),
  vec2<f32>(-0.26496910, -0.41893023), vec2<f32>(0.79197514, 0.19090188),
);
fn sample_shadow(world: vec3<f32>, cascade: u32, rotation: mat2x2<f32>) -> f32 {
  let light = shadow.light_view_proj[cascade] * vec4<f32>(world, 1.0);
  let suv = light.xy / max(light.w, 1e-5) * 0.5 + 0.5;
  if (any(suv < vec2<f32>(0.0)) || any(suv > vec2<f32>(1.0))) { return 1.0; }
  let reference = light.z / max(light.w, 1e-5) - 0.0008;
  var sum = 0.0;
  for (var tap = 0u; tap < 12u; tap++) {
    let offset = rotation * SHADOW_POISSON[tap] / 2048.0;
    sum += textureSampleCompare(shadow_map, shadow_sampler, suv + offset, i32(cascade), reference);
  }
  return sum / 12.0;
}
fn shadow_factor(uv: vec2<f32>, depth: f32) -> f32 {
  // Fullscreen texture UV points down while clip-space Y points up.
  let clip = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
  let world4 = shadow.inverse_view_proj * clip;
  let world = world4.xyz / max(world4.w, 1e-5);
  let far4 = shadow.inverse_view_proj * vec4<f32>(0.0, 0.0, 1.0, 1.0);
  let forward = normalize(far4.xyz / max(far4.w, 1e-5) - globals.cam_pos.xyz);
  let distance = max(dot(world - globals.cam_pos.xyz, forward), 0.0);
  let cascade = select(0u, select(1u, 2u, distance > shadow.splits.z), distance > shadow.splits.y);
  let angle = fract(dot(uv, vec2<f32>(91.7, 47.3)) + globals.time_res.x * 0.03125) * 6.2831853;
  let rotation = mat2x2<f32>(cos(angle), -sin(angle), sin(angle), cos(angle));
  let current = sample_shadow(world, cascade, rotation);
  if (cascade >= 2u) { return current; }
  let split_begin = select(shadow.splits.x, shadow.splits.y, cascade == 1u);
  let split_end = select(shadow.splits.y, shadow.splits.z, cascade == 1u);
  let blend_begin = mix(split_begin, split_end, 0.9);
  let blend = smoothstep(blend_begin, split_end, distance);
  return mix(current, sample_shadow(world, cascade + 1u, rotation), blend);
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
  let albedo_sample = textureSample(g_albedo, g_sampler, input.uv); let normal_sample = textureSample(g_normal, g_sampler, input.uv);
  let light_sample = textureSample(g_light, g_sampler, input.uv); let motion_sample = textureSample(g_motion, g_sampler, input.uv);
  let reactive_sample = textureSample(g_reactive, g_sampler, input.uv).r; let depth = textureSample(g_depth, g_sampler, input.uv);
  if (debug.mode == 1u) { return vec4<f32>(albedo_sample.rgb, 1.0); }
  if (debug.mode == 2u) { return vec4<f32>(decode_oct(normal_sample.rg) * 0.5 + 0.5, 1.0); }
  if (debug.mode == 3u) { return vec4<f32>(vec3<f32>(depth), 1.0); }
  if (debug.mode == 4u) { return vec4<f32>(light_sample.rgb, 1.0); }
  if (debug.mode == 5u) { return vec4<f32>(vec3<f32>(light_sample.a), 1.0); }
  if (debug.mode == 6u) { return vec4<f32>(abs(motion_sample.xy), 0.0, 1.0); }
  if (debug.mode == 7u) { return vec4<f32>(vec3<f32>(reactive_sample), 1.0); }
  if (depth >= 0.9999) { return vec4<f32>(globals.sky_color.rgb * debug.exposure, 1.0); }
  let n = decode_oct(normal_sample.rg);
  // Globals stays at the fixed 128-byte M6 contract. A normalized camera-facing
  // vector is used until the later temporal pass owns inverse-view data.
  let v = vec3<f32>(0.0, 0.0, 1.0); let l = normalize(-globals.sun_dir.xyz);
  let roughness = clamp(normal_sample.b, 0.045, 1.0); let sun_shadow = shadow_factor(input.uv, depth);
  let contact = clamp(1.0 - max(0.0, depth - textureSample(g_depth, g_sampler, input.uv + vec2<f32>(0.002, 0.0))) * 80.0, 0.55, 1.0);
  let cloud_clip = vec4<f32>(input.uv.x * 2.0 - 1.0, 1.0 - input.uv.y * 2.0, depth, 1.0);
  let cloud_world4 = shadow.inverse_view_proj * cloud_clip;
  let cloud_world = cloud_world4.xyz / max(cloud_world4.w, 1e-5);
  let cloud_uv = fract((cloud_world.xz - cloud_shadow_world.origin) / cloud_shadow_world.world_size + vec2<f32>(0.5));
  let cloud_factor = 0.55 + 0.45 * textureSample(cloud_shadow, g_sampler, cloud_uv).r;
  let sun = cook_torrance(n, v, l, albedo_sample.rgb, albedo_sample.a, roughness) * globals.sun_dir.w * sun_shadow * contact * cloud_factor;
  let sky_ambient = textureSampleLevel(sky, g_sampler, n, roughness * roughness * 6.0).rgb;
  let reflected = reflect(-v, n);
  let sky_specular = textureSampleLevel(sky, g_sampler, reflected, roughness * roughness * 6.0).rgb;
  let env_fresnel = fresnel(mix(vec3<f32>(0.04), albedo_sample.rgb, albedo_sample.a), max(dot(n, v), 0.0));
  let environment_specular = sky_specular * env_fresnel * (0.20 + 0.80 * albedo_sample.a);
  let ao = textureSampleLevel(g_ao, g_sampler, input.uv, 0.0).r;
  let ambient = (sky_ambient * 0.22 + globals.sky_color.rgb * 0.08) * albedo_sample.rgb * ao;
  let block_ambient = vec3<f32>(1.0, 0.58, 0.28) * light_sample.r * 0.18 * albedo_sample.rgb;
  let emission = vec3<f32>(0.85, 0.28, 0.04) * normal_sample.a;
  return vec4<f32>(max((sun + environment_specular + ambient * light_sample.g + block_ambient + emission) * debug.exposure, vec3<f32>(0.0)), 1.0);
}
