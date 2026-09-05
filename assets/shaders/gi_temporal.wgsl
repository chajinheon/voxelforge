// World-position reprojection and stable GI history.
const HISTORY_WEIGHT: f32 = 0.90;
const RELATIVE_DEPTH_REJECT: f32 = 0.02;
const ABSOLUTE_DEPTH_REJECT: f32 = 0.50;
const NORMAL_REJECT_DOT: f32 = 0.90;

@group(0) @binding(0) var current_gi: texture_2d<f32>;
@group(0) @binding(1) var history_gi: texture_2d<f32>;
@group(0) @binding(2) var current_depth: texture_2d<f32>;
@group(0) @binding(3) var previous_depth: texture_2d<f32>;
@group(0) @binding(4) var current_normal: texture_2d<f32>;
@group(0) @binding(5) var previous_normal: texture_2d<f32>;
@group(0) @binding(6) var current_material: texture_2d<u32>;
@group(0) @binding(7) var previous_material: texture_2d<u32>;
@group(0) @binding(8) var<uniform> prev_view_proj: mat4x4<f32>;
@group(1) @binding(0) var temporal_out: texture_storage_2d<rgba16float, write>;

fn history_allowed(uv: vec2<f32>, depth: f32, old_depth: f32, normal: vec3<f32>, old_normal: vec3<f32>, material: u32, old_material: u32) -> bool {
  let delta = abs(depth - old_depth);
  return all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0))
    && delta / max(abs(depth), 1e-4) <= RELATIVE_DEPTH_REJECT
    && delta <= ABSOLUTE_DEPTH_REJECT
    && dot(normalize(normal), normalize(old_normal)) >= NORMAL_REJECT_DOT
    && material == old_material;
}

struct Bounds {
  lo: vec4<f32>,
  hi: vec4<f32>,
};

fn neighborhood_bounds(pixel: vec2<i32>) -> Bounds {
  var lo = vec4<f32>(1e30);
  var hi = vec4<f32>(-1e30);
  for (var y = -1; y <= 1; y++) {
    for (var x = -1; x <= 1; x++) {
      let sample = textureLoad(current_gi, pixel + vec2<i32>(x, y), 0);
      lo = min(lo, sample);
      hi = max(hi, sample);
    }
  }
  return Bounds(lo, hi);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let size = textureDimensions(temporal_out);
  if (gid.x >= size.x || gid.y >= size.y) { return; }
  let pixel = vec2<i32>(gid.xy);
  let current = textureLoad(current_gi, pixel, 0);
  let depth = textureLoad(current_depth, pixel, 0).r;
  let normal = textureLoad(current_normal, pixel, 0).xyz;
  let material = textureLoad(current_material, pixel, 0).r;
  let clip = vec4<f32>(vec2<f32>(gid.xy) / vec2<f32>(size), depth, 1.0);
  let previous_clip = prev_view_proj * clip;
  let previous_uv = previous_clip.xy / max(previous_clip.w, 1e-4) * 0.5 + 0.5;
  let previous_pixel = vec2<i32>(previous_uv * vec2<f32>(size));
  let old_depth = textureLoad(previous_depth, previous_pixel, 0).r;
  let old_normal = textureLoad(previous_normal, previous_pixel, 0).xyz;
  let old_material = textureLoad(previous_material, previous_pixel, 0).r;
  if (!history_allowed(previous_uv, depth, old_depth, normal, old_normal, material, old_material)) {
    textureStore(temporal_out, pixel, current);
    return;
  }
  let history = textureLoad(history_gi, previous_pixel, 0);
  let bounds = neighborhood_bounds(pixel);
  let span = (bounds.hi - bounds.lo) * 0.10;
  let clamped = clamp(history, bounds.lo - span, bounds.hi + span);
  textureStore(temporal_out, pixel, mix(current, clamped, HISTORY_WEIGHT));
}
