// Quarter-resolution voxel GI.  All traversal is software Amanatides-Woo DDA.
const MAX_RAYS: u32 = 6u;
const MAX_CROSSINGS: u32 = 96u;
const MAX_DISTANCE: f32 = 48.0;

struct ClipmapUniform {
  origin_cell_size: array<vec4<i32>, 4>,
  ring_offset_ready: array<vec4<u32>, 4>,
  params: vec4<f32>,
};

@group(2) @binding(0) var material0: texture_3d<f32>;
@group(2) @binding(1) var light0: texture_3d<f32>;
@group(2) @binding(2) var material1: texture_3d<f32>;
@group(2) @binding(3) var light1: texture_3d<f32>;
@group(2) @binding(4) var material2: texture_3d<f32>;
@group(2) @binding(5) var light2: texture_3d<f32>;
@group(2) @binding(6) var material3: texture_3d<f32>;
@group(2) @binding(7) var light3: texture_3d<f32>;
@group(2) @binding(8) var<uniform> clipmap: ClipmapUniform;

@group(3) @binding(0) var depth_in: texture_2d<f32>;
@group(3) @binding(1) var normal_material: texture_2d<f32>;
@group(3) @binding(2) var albedo_in: texture_2d<f32>;
@group(3) @binding(3) var ssao_in: texture_2d<f32>;
@group(3) @binding(4) var shadow_array: texture_depth_2d_array;
@group(3) @binding(5) var shadow_sampler: sampler_comparison;
@group(3) @binding(6) var sky_lut: texture_2d<f32>;
@group(4) @binding(0) var current_gi: texture_storage_2d<rgba16float, write>;

fn level_for_distance(distance: f32) -> u32 {
  if (distance < 8.0) { return 0u; }
  if (distance < 16.0) { return 1u; }
  if (distance < 32.0) { return 2u; }
  return 3u;
}

fn load_material(level: u32, cell: vec3<i32>) -> vec4<f32> {
  let p = cell & vec3<i32>(127);
  switch level {
    case 0u: { return textureLoad(material0, p, 0); }
    case 1u: { return textureLoad(material1, p, 0); }
    case 2u: { return textureLoad(material2, p, 0); }
    default: { return textureLoad(material3, p, 0); }
  }
}

fn load_light(level: u32, cell: vec3<i32>) -> vec4<f32> {
  let p = cell & vec3<i32>(127);
  switch level {
    case 0u: { return textureLoad(light0, p, 0); }
    case 1u: { return textureLoad(light1, p, 0); }
    case 2u: { return textureLoad(light2, p, 0); }
    default: { return textureLoad(light3, p, 0); }
  }
}

fn radical_inverse(bits_in: u32) -> f32 {
  var bits = bits_in;
  bits = (bits << 16u) | (bits >> 16u);
  bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
  bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
  bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
  return f32(bits) * 2.3283064365386963e-10;
}

fn hammersley(i: u32, count: u32) -> vec2<f32> {
  return vec2<f32>(f32(i) / f32(max(count, 1u)), radical_inverse(i));
}

fn cosine_ray(i: u32, count: u32, normal: vec3<f32>) -> vec3<f32> {
  let u = fract(hammersley(i, count) + vec2<f32>(0.37, 0.11));
  let phi = 6.28318530718 * u.x;
  let r = sqrt(u.y);
  let local = vec3<f32>(r * cos(phi), r * sin(phi), sqrt(max(1.0 - u.y, 0.0)));
  let n = normalize(normal);
  let helper = select(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(0.0, 1.0, 0.0), abs(n.z) >= 0.999);
  let tangent = normalize(cross(helper, n));
  let bitangent = cross(n, tangent);
  return normalize(tangent * local.x + bitangent * local.y + n * local.z);
}

fn trace_ray(origin: vec3<f32>, direction: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
  var cell = vec3<i32>(floor(origin));
  let step = vec3<i32>(sign(direction));
  var distance = 0.0;
  for (var crossing = 0u; crossing < MAX_CROSSINGS; crossing++) {
    let sample = load_material(level_for_distance(distance), cell);
    if (sample.a >= 0.5) {
      let light = load_light(level_for_distance(distance), cell);
      return sample.rgb * (light.rgb * 8.0 + light.a * 0.35);
    }
    let boundary = vec3<f32>(cell) + select(vec3<f32>(0.0), vec3<f32>(1.0), step > vec3<i32>(0));
    let t = (boundary - origin) / direction;
    let valid_t = select(vec3<f32>(1e30), t, abs(direction) > vec3<f32>(1e-5));
    let next = min(min(valid_t.x, valid_t.y), valid_t.z);
    if (next >= 1e29 || next > MAX_DISTANCE) { break; }
    distance = next;
    cell += vec3<i32>(select(vec3<i32>(0), step, abs(t - vec3<f32>(next)) < vec3<f32>(1e-4)));
  }
  let sky_uv = vec2<f32>(0.5 + direction.x * 0.5, 0.5 - direction.y * 0.5);
  return textureLoad(sky_lut, vec2<i32>(sky_uv * vec2<f32>(textureDimensions(sky_lut))), 0).rgb;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let size = textureDimensions(current_gi);
  if (gid.x >= size.x || gid.y >= size.y) { return; }
  let pixel = vec2<i32>(gid.xy);
  let depth = textureLoad(depth_in, pixel, 0).r;
  if (depth >= 0.9999 || clipmap.params.w < 0.5) {
    textureStore(current_gi, pixel, vec4<f32>(0.0));
    return;
  }
  let encoded = textureLoad(normal_material, pixel, 0).rg * 2.0 - 1.0;
  let normal = normalize(vec3<f32>(encoded, sqrt(max(1.0 - dot(encoded, encoded), 0.0))));
  let origin = vec3<f32>(f32(gid.x), depth * 48.0, f32(gid.y)) + normal * 0.05;
  var indirect = vec3<f32>(0.0);
  for (var ray = 0u; ray < MAX_RAYS; ray++) {
    if (ray >= u32(clipmap.params.y)) { break; }
    indirect += trace_ray(origin, cosine_ray(ray, u32(clipmap.params.y), normal), normal);
  }
  let count = max(clipmap.params.y, 1.0);
  textureStore(current_gi, pixel, vec4<f32>(max(indirect / count * clipmap.params.z, vec3<f32>(0.0)), 1.0));
}
