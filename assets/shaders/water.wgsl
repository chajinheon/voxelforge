// M8 WATER replacement pass. The scene color is immutable input.
struct Globals {
  view_proj: mat4x4<f32>, cam_pos: vec4<f32>, sun_dir: vec4<f32>,
  time_res: vec4<f32>, sky_color: vec4<f32>,
};
struct WaterParams {
  time: f32, underwater: f32, max_distance: f32, _padding0: f32,
  absorption: vec3<f32>, _padding1: f32,
  scattering: vec3<f32>, _padding_scattering: f32,
  internal_size: vec2<f32>, ssr_steps: f32, _padding2: f32,
};
struct ChunkUniform { origin: vec4<i32> };
@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var scene_color: texture_2d<f32>;
// ABI-compatible depth copy. SSR uses the linear pyramid, not this raw depth.
@group(1) @binding(1) var scene_depth: texture_depth_2d;
@group(1) @binding(2) var depth_pyramid: texture_2d<f32>;
@group(1) @binding(3) var scene_sampler: sampler;
@group(1) @binding(4) var<uniform> water: WaterParams;
@group(1) @binding(5) var depth_sampler: sampler;
@group(2) @binding(0) var<uniform> chunk: ChunkUniform;

struct VertexInput { @location(0) packed: vec2<u32> };
struct VertexOutput {
  @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>,
  @location(1) world: vec3<f32>, @location(2) normal: vec3<f32>,
  @location(3) wave_delta: f32,
};

fn wave_displacement(p: vec2<f32>, t: f32) -> vec3<f32> {
  let d0 = normalize(vec2<f32>(1.0, 0.2)); let d1 = normalize(vec2<f32>(-0.6, 0.8));
  let d2 = normalize(vec2<f32>(0.3, -1.0)); let d3 = normalize(vec2<f32>(-0.8, -0.3));
  let k0 = 6.283185307 / 18.0; let k1 = 6.283185307 / 9.0;
  let k2 = 6.283185307 / 4.5; let k3 = 6.283185307 / 2.25;
  let p0 = k0 * dot(d0, p) - 1.20 * k0 * t; let p1 = k1 * dot(d1, p) - k1 * t;
  let p2 = k2 * dot(d2, p) - 0.80 * k2 * t; let p3 = k3 * dot(d3, p) - 0.60 * k3 * t;
  return vec3<f32>(
    0.075 * 0.35 * d0.x * cos(p0) + 0.040 * 0.30 * d1.x * cos(p1) + 0.022 * 0.22 * d2.x * cos(p2) + 0.012 * 0.15 * d3.x * cos(p3),
    0.075 * sin(p0) + 0.040 * sin(p1) + 0.022 * sin(p2) + 0.012 * sin(p3),
    0.075 * 0.35 * d0.y * cos(p0) + 0.040 * 0.30 * d1.y * cos(p1) + 0.022 * 0.22 * d2.y * cos(p2) + 0.012 * 0.15 * d3.y * cos(p3));
}

// Analytic normal from the Gerstner height gradient.
fn wave_normal(p: vec2<f32>, t: f32) -> vec3<f32> {
  let d0 = normalize(vec2<f32>(1.0, 0.2)); let d1 = normalize(vec2<f32>(-0.6, 0.8));
  let d2 = normalize(vec2<f32>(0.3, -1.0)); let d3 = normalize(vec2<f32>(-0.8, -0.3));
  let k0 = 6.283185307 / 18.0; let k1 = 6.283185307 / 9.0;
  let k2 = 6.283185307 / 4.5; let k3 = 6.283185307 / 2.25;
  let q0 = k0 * dot(d0, p) - 1.20 * k0 * t; let q1 = k1 * dot(d1, p) - k1 * t;
  let q2 = k2 * dot(d2, p) - 0.80 * k2 * t; let q3 = k3 * dot(d3, p) - 0.60 * k3 * t;
  let grad = d0 * (0.075 * k0 * cos(q0)) + d1 * (0.040 * k1 * cos(q1)) + d2 * (0.022 * k2 * cos(q2)) + d3 * (0.012 * k3 * cos(q3));
  return normalize(vec3<f32>(-grad.x, 1.0, -grad.y));
}

@vertex fn vs_main(input: VertexInput) -> VertexOutput {
  let x = f32(input.packed.x & 63u); let y = f32((input.packed.x >> 6u) & 63u); let z = f32((input.packed.x >> 12u) & 63u);
  let face = (input.packed.x >> 18u) & 7u; let lowered = (input.packed.x & 0x00800000u) != 0u;
  let fx = f32((input.packed.x >> 24u) & 15u) / 16.0; let fy = f32((input.packed.x >> 28u) & 15u) / 16.0; let fz = f32((input.packed.y >> 24u) & 15u) / 16.0;
  var local = vec3<f32>(x + fx, y + fy, z + fz); if (lowered) { local.y -= 0.125; }
  var world = vec3<f32>(chunk.origin.xyz) + local; var delta = 0.0;
  if (lowered) { let wave = wave_displacement(world.xz, water.time); world += wave; delta = wave.y; }
  var uv = vec2<f32>(local.x, local.z);
  if (face == 0u || face == 1u) { uv = vec2<f32>(local.z, -local.y); }
  if (face == 4u || face == 5u) { uv = vec2<f32>(local.x, -local.y); }
  var out: VertexOutput; out.position = globals.view_proj * vec4<f32>(world, 1.0); out.uv = uv;
  let face_normal = array<vec3<f32>, 6>(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(-1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, -1.0, 0.0), vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(0.0, 0.0, -1.0));
  out.world = world; out.normal = select(face_normal[face], wave_normal(world.xz, water.time), lowered); out.wave_delta = delta; return out;
}

fn project_uv(world: vec3<f32>) -> vec3<f32> {
  let clip = globals.view_proj * vec4<f32>(world, 1.0); if (clip.w <= 0.0001) { return vec3<f32>(-2.0, -2.0, 0.0); }
  let ndc = clip.xy / clip.w; return vec3<f32>(ndc.x * 0.5 + 0.5, 1.0 - (ndc.y * 0.5 + 0.5), clip.z / clip.w);
}
struct SsrResult { color: vec3<f32>, confidence: f32, residual: f32 };
fn ssr(surface_uv: vec2<f32>, world: vec3<f32>, normal: vec3<f32>, view_dir: vec3<f32>, water_depth: f32) -> SsrResult {
  var result = SsrResult(globals.sky_color.rgb, 0.0, 1.0); let reflected = normalize(reflect(-view_dir, normal));
  let thickness = 0.18 + 0.002 * water_depth; let steps = clamp(water.ssr_steps, 40.0, 64.0); let max_distance = min(water.max_distance, 48.0);
  var previous_t = 0.05; var hit = false; var hit_uv = surface_uv; var hit_residual = 1.0;
  for (var i = 0u; i < 64u; i++) {
    if (f32(i) >= steps || hit) { break; }
    let t = max_distance * (f32(i) + 1.0) / steps; let projected = project_uv(world + reflected * t);
    if (projected.x < 0.0 || projected.x > 1.0 || projected.y < 0.0 || projected.y > 1.0) { break; }
    let mip = min(5.0, floor(f32(i) / 8.0)); let scene_depth = textureSampleLevel(depth_pyramid, depth_sampler, projected.xy, mip).r;
    let ray_depth = distance(world + reflected * t, globals.cam_pos.xyz); let residual = abs(scene_depth - ray_depth);
    if (scene_depth > water_depth && scene_depth <= ray_depth + thickness && residual <= thickness) {
      var lo = previous_t; var hi = t;
      for (var refine = 0u; refine < 5u; refine++) {
        let mid = (lo + hi) * 0.5; let mid_world = world + reflected * mid; let mid_projected = project_uv(mid_world);
        let mid_depth = textureSampleLevel(depth_pyramid, depth_sampler, mid_projected.xy, 0.0).r;
        if (mid_depth <= distance(mid_world, globals.cam_pos.xyz) + thickness) { hi = mid; } else { lo = mid; }
      }
      hit_uv = project_uv(world + reflected * ((lo + hi) * 0.5)).xy; hit_residual = min(1.0, residual / max(thickness, 0.0001)); hit = true;
    }
    previous_t = t;
  }
  if (hit) {
    result.color = textureSampleLevel(scene_color, scene_sampler, hit_uv, 0.0).rgb;
    let edge = smoothstep(0.0, 0.08, min(min(hit_uv.x, hit_uv.y), min(1.0 - hit_uv.x, 1.0 - hit_uv.y)));
    let facing = clamp(1.0 - dot(-view_dir, normal), 0.0, 1.0); let distance_fade = 1.0 - smoothstep(0.70, 1.00, previous_t / max_distance);
    result.confidence = edge * facing * distance_fade * (1.0 - hit_residual); result.residual = hit_residual;
  }
  return result;
}
fn fbm(p: vec2<f32>) -> f32 {
  var value = 0.0; var amplitude = 0.5; var frequency = 1.0;
  for (var i = 0u; i < 4u; i++) { value += amplitude * (0.5 + 0.5 * sin(dot(p * frequency, vec2<f32>(12.9898, 78.233)))); frequency *= 2.03; amplitude *= 0.5; }
  return value;
}

@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
  let screen_uv = input.position.xy / water.internal_size; let view_dir = normalize(globals.cam_pos.xyz - input.world);
  let water_depth = distance(globals.cam_pos.xyz, input.world); let scene_depth = textureSampleLevel(depth_pyramid, depth_sampler, screen_uv, 0.0).r;
  if (scene_depth + 0.0005 < water_depth) { discard; }
  let thickness = max(scene_depth - water_depth, 0.0); let offset = input.normal.xz * 0.015 * min(1.0, 4.0 / max(water_depth, 1.0));
  let distortion = water.underwater * 0.0025 * sin(screen_uv.y * 90.0 + water.time * 1.6);
  let candidate_uv = clamp(screen_uv + offset + vec2<f32>(distortion, 0.0), vec2<f32>(0.001), vec2<f32>(0.999)); let candidate_depth = textureSampleLevel(depth_pyramid, depth_sampler, candidate_uv, 0.0).r;
  let refract_uv = select(candidate_uv, screen_uv, candidate_depth < water_depth - 0.1); var refracted = textureSample(scene_color, scene_sampler, refract_uv).rgb;
  let transmittance = exp(-water.absorption * thickness); refracted = refracted * transmittance + water.scattering * (1.0 - transmittance);
  let ca = sin(dot(input.world.xz, vec2<f32>(1.7, 1.1)) + water.time * 1.6); let cb = sin(dot(input.world.xz, vec2<f32>(-1.3, 1.9)) - water.time * 1.2);
  refracted *= 1.0 + 0.18 * pow(clamp(1.0 - abs(ca + cb) * 0.5, 0.0, 1.0), 6.0) * exp(-0.12 * thickness);
  let ssr_result = ssr(screen_uv, input.world, input.normal, view_dir, water_depth); let fresnel = 0.02 + 0.98 * pow(1.0 - clamp(dot(input.normal, view_dir), 0.0, 1.0), 5.0);
  let noise = smoothstep(0.45, 0.70, fbm(input.world.xz * 0.35 + water.time * vec2<f32>(0.08, 0.04))); let shore = 1.0 - smoothstep(0.15, 1.25, thickness); let crest = smoothstep(0.06, 0.13, abs(input.wave_delta));
  let foam = clamp(shore * 0.85 + crest * 0.45, 0.0, 1.0) * noise; let color = mix(refracted, ssr_result.color, fresnel * ssr_result.confidence);
  return vec4<f32>(mix(color, vec3<f32>(0.80, 0.92, 0.95), foam), 1.0);
}
