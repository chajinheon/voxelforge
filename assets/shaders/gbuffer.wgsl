struct Globals {
  view_proj: mat4x4<f32>, cam_pos: vec4<f32>, sun_dir: vec4<f32>,
  time_res: vec4<f32>, sky_color: vec4<f32>,
  unjittered_view_proj: mat4x4<f32>, previous_view_proj: mat4x4<f32>,
};
struct ChunkUniform { origin: vec4<i32>, };
struct MaterialGpu { base: vec4<f32>, tint: vec4<f32>, flags: vec4<u32>, };
@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var pbr_albedo: texture_2d_array<f32>;
@group(1) @binding(1) var pbr_material: texture_2d_array<f32>;
@group(1) @binding(2) var pbr_emission: texture_2d_array<f32>;
@group(1) @binding(3) var pbr_sampler: sampler;
@group(1) @binding(4) var<storage, read> materials: array<MaterialGpu>;
@group(2) @binding(0) var<uniform> chunk: ChunkUniform;

struct VertexInput { @location(0) packed: vec2<u32>, };
struct VertexOutput {
  @builtin(position) clip_position: vec4<f32>, @location(0) uv: vec2<f32>,
  @location(1) @interpolate(flat) tex: i32, @location(2) @interpolate(flat) face: u32,
  @location(3) @interpolate(flat) normal: vec3<f32>,
  @location(4) @interpolate(flat) tangent: vec3<f32>, @location(5) ao: f32,
  @location(6) block_light: f32, @location(7) sky_light: f32, @location(8) world_pos: vec3<f32>,
};
fn face_normal(face: u32) -> vec3<f32> {
  switch face {
    case 0u: { return vec3<f32>(1.0, 0.0, 0.0); }
    case 1u: { return vec3<f32>(-1.0, 0.0, 0.0); }
    case 2u: { return vec3<f32>(0.0, 1.0, 0.0); }
    case 3u: { return vec3<f32>(0.0, -1.0, 0.0); }
    case 4u: { return vec3<f32>(0.0, 0.0, 1.0); }
    default: { return vec3<f32>(0.0, 0.0, -1.0); }
  }
}
fn face_tangent(face: u32) -> vec3<f32> {
  if (face == 0u || face == 1u) { return vec3<f32>(0.0, 0.0, 1.0); }
  return vec3<f32>(1.0, 0.0, 0.0);
}
fn face_bitangent(face: u32) -> vec3<f32> {
  if (face == 0u || face == 1u || face == 4u || face == 5u) { return vec3<f32>(0.0, -1.0, 0.0); }
  return vec3<f32>(0.0, 0.0, 1.0);
}
@vertex fn vs_main(input: VertexInput) -> VertexOutput {
  let x = input.packed.x & 63u; let y = (input.packed.x >> 6u) & 63u;
  let z = (input.packed.x >> 12u) & 63u; let face = (input.packed.x >> 18u) & 7u;
  let ao = (input.packed.x >> 21u) & 3u; let lowered = (input.packed.x & 0x00800000u) != 0u;
  let tex = input.packed.y & 65535u; let block_light = (input.packed.y >> 16u) & 15u;
  let sky_light = (input.packed.y >> 20u) & 15u;
  let frac = vec3<f32>(
    f32((input.packed.x >> 24u) & 15u),
    f32((input.packed.x >> 28u) & 15u),
    f32((input.packed.y >> 24u) & 15u)
  ) / 16.0;
  let local = vec3<f32>(f32(x), f32(y), f32(z)) + frac;
  var world_local = local; if (lowered) { world_local.y -= 0.125; }
  let base_world = vec3<f32>(chunk.origin.xyz) + world_local; var world_pos = base_world;
  if (tex == 8u) {
    let phase = dot(base_world.xz, vec2<f32>(0.173, 0.127)) + globals.time_res.x * 1.7;
    let gust = (sin(phase) + 0.5 * sin(phase * 2.17 + 1.3)) / 1.5;
    let wind = normalize(vec2<f32>(0.8, 0.6)) * (gust * 0.045);
    world_pos.x += wind.x; world_pos.z += wind.y;
  } else if (tex == 2u && face == 2u) {
    let phase = dot(base_world.xz, vec2<f32>(0.173, 0.127)) + globals.time_res.x * 1.7;
    let gust = (sin(phase) + 0.5 * sin(phase * 2.17 + 1.3)) / 1.5;
    let wind = normalize(vec2<f32>(0.8, 0.6)) * (gust * 0.012);
    world_pos.x += wind.x; world_pos.z += wind.y;
  }
  var uv = vec2<f32>(local.x, local.z);
  if (face == 0u || face == 1u) { uv = vec2<f32>(local.z, -local.y); }
  else if (face == 4u || face == 5u) { uv = vec2<f32>(local.x, -local.y); }
  let normal = face_normal(face); let tangent = normalize(face_tangent(face) - normal * dot(normal, face_tangent(face)));
  var output: VertexOutput; let world4 = vec4<f32>(world_pos, 1.0);
  output.clip_position = globals.view_proj * world4;
  output.uv = uv; output.tex = i32(tex); output.face = face; output.normal = normal;
  output.tangent = tangent; output.ao = 0.35 + 0.65 * f32(ao) / 3.0;
  output.block_light = f32(block_light); output.sky_light = f32(sky_light); output.world_pos = world_pos; return output;
}
fn pom_uv(uv: vec2<f32>, layer: i32, view_ts: vec3<f32>, scale: f32) -> vec2<f32> {
  if (abs(view_ts.z) < 0.15 || scale <= 0.0) { return uv; }
  let layers = i32(clamp(globals.time_res.w, 1.0, 32.0)); let delta = view_ts.xy / max(abs(view_ts.z), 0.20) * scale / f32(layers);
  let layer_depth = 1.0 / f32(layers);
  var previous = uv; var previous_depth = 0.0;
  var previous_surface = 1.0 - textureSampleLevel(pbr_material, pbr_sampler, previous, layer, 0.0).a;
  for (var i = 1; i <= 16; i++) {
    if (i > layers) { break; }
    let current = uv - delta * f32(i);
    let depth = f32(i) * layer_depth;
    let surface = 1.0 - textureSampleLevel(pbr_material, pbr_sampler, current, layer, 0.0).a;
    if (depth >= surface) {
      let after = depth - surface;
      let before = max(previous_surface - previous_depth, 0.0);
      let weight = clamp(before / max(after + before, 1e-4), 0.0, 1.0);
      var lo = previous; var hi = current;
      var lo_depth = previous_depth; var hi_depth = depth;
      var hit = mix(lo, hi, weight);
      for (var refine = 0; refine < 3; refine++) {
        if (refine >= 2) { break; }
        let mid = mix(lo, hi, 0.5);
        let mid_depth = (lo_depth + hi_depth) * 0.5;
        let mid_surface = 1.0 - textureSampleLevel(pbr_material, pbr_sampler, mid, layer, 0.0).a;
        if (mid_depth >= mid_surface) { hi = mid; hi_depth = mid_depth; }
        else { lo = mid; lo_depth = mid_depth; }
        hit = mix(lo, hi, 0.5);
      }
      return clamp(hit, uv - vec2<f32>(0.08), uv + vec2<f32>(0.08));
    }
    previous = current; previous_depth = depth; previous_surface = surface;
  }
  return uv;
}
fn oct_encode(n: vec3<f32>) -> vec2<f32> {
  var p = n.xy / max(abs(n.x) + abs(n.y) + abs(n.z), 1e-8);
  if (n.z < 0.0) { p = (vec2<f32>(1.0) - abs(p.yx)) * sign(p); }
  return p;
}
struct GBufferOut {
  @location(0) albedo: vec4<f32>, @location(1) normal: vec4<f32>, @location(2) light: vec4<f32>,
  @location(3) motion: vec2<f32>, @location(4) reactive: f32,
};
@fragment fn fs_main(input: VertexOutput) -> GBufferOut {
  let raw_layer = u32(max(input.tex, 0));
  let layer = select(0u, raw_layer, raw_layer < arrayLength(&materials));
  let layer_i = i32(layer);
  let gpu = materials[layer]; let base_material = textureSampleLevel(pbr_material, pbr_sampler, input.uv, layer_i, 0.0);
  let view = normalize(globals.cam_pos.xyz - input.world_pos); let bitangent = face_bitangent(input.face);
  let view_ts = vec3<f32>(dot(view, input.tangent), dot(view, bitangent), dot(view, input.normal));
  var uv = input.uv;
  let footprint_texels = max(length(dpdx(input.uv)), length(dpdy(input.uv))) * 16.0;
  let linear_distance = length(globals.cam_pos.xyz - input.world_pos);
  if ((gpu.flags.x & 1u) != 0u && dot(input.normal, view) >= 0.15 && linear_distance <= 48.0 && footprint_texels <= 0.25) {
    uv = pom_uv(input.uv, layer_i, view_ts, gpu.base.z);
  }
  let albedo_sample = textureSampleLevel(pbr_albedo, pbr_sampler, uv, layer_i, 0.0);
  var material_sample = base_material;
  if (any(abs(uv - input.uv) > vec2<f32>(1e-6))) { material_sample = textureSampleLevel(pbr_material, pbr_sampler, uv, layer_i, 0.0); }
  let emission_mask = textureSampleLevel(pbr_emission, pbr_sampler, uv, layer_i, 0.0).r;
  if ((gpu.flags.x & 2u) != 0u && albedo_sample.a < gpu.tint.a) { discard; }
  let tangent_normal = normalize(vec3<f32>((material_sample.r * 2.0 - 1.0) * gpu.base.y, (material_sample.g * 2.0 - 1.0) * gpu.base.y, 1.0));
  let world_normal = normalize(input.tangent * tangent_normal.x + bitangent * tangent_normal.y + input.normal * tangent_normal.z);
  let oct = oct_encode(world_normal);
  let current_no_jitter = globals.unjittered_view_proj * vec4<f32>(input.world_pos, 1.0);
  let previous_no_jitter = globals.previous_view_proj * vec4<f32>(input.world_pos, 1.0);
  let current_uv = current_no_jitter.xy / max(abs(current_no_jitter.w), 1e-5);
  let previous_uv = previous_no_jitter.xy / max(abs(previous_no_jitter.w), 1e-5);
  let motion_uv = vec2<f32>((current_uv.x - previous_uv.x) * 0.5,
    (previous_uv.y - current_uv.y) * 0.5);
  var out: GBufferOut; out.albedo = vec4<f32>(albedo_sample.rgb, gpu.base.x);
  out.normal = vec4<f32>(oct, material_sample.b, gpu.base.w * emission_mask);
  out.light = vec4<f32>(input.block_light / 15.0, input.sky_light / 15.0, input.ao, f32(layer) / 255.0);
  out.motion = select(vec2<f32>(0.0), motion_uv, current_no_jitter.w > 0.0 && previous_no_jitter.w > 0.0);
  out.reactive = max(emission_mask, select(0.0, 1.0, (gpu.flags.x & 2u) != 0u)); return out;
}
