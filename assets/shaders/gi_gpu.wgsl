// Quarter-resolution world-space voxel GI. The camera ray and primary hit
// are reconstructed in world space; clipmap reads use logical origins and
// toroidal ring offsets supplied by the CPU.
const LEVEL_COUNT: u32 = 4u;
const CLIPMAP_SIZE: i32 = 128;
const MAX_RAYS: u32 = 4u;
const MAX_CROSSINGS: u32 = 96u;

struct Params {
  origins: array<vec4<i32>, 4>,
  rings: array<vec4<u32>, 4>,
  params: vec4<f32>, // max distance, ray count, intensity, enabled
  inverse_view_proj: array<vec4<f32>, 4>,
  previous_view_proj: array<vec4<f32>, 4>,
  camera_position: vec4<f32>,
  sun_direction: vec4<f32>,
  sky_color: vec4<f32>,
};

@group(0) @binding(0) var material0: texture_3d<f32>;
@group(0) @binding(1) var light0: texture_3d<f32>;
@group(0) @binding(2) var material1: texture_3d<f32>;
@group(0) @binding(3) var light1: texture_3d<f32>;
@group(0) @binding(4) var material2: texture_3d<f32>;
@group(0) @binding(5) var light2: texture_3d<f32>;
@group(0) @binding(6) var material3: texture_3d<f32>;
@group(0) @binding(7) var light3: texture_3d<f32>;
@group(0) @binding(8) var<uniform> clipmap: Params;
@group(0) @binding(9) var output_gi: texture_storage_2d<rgba16float, write>;
@group(0) @binding(10) var input_gi: texture_2d<f32>;
@group(0) @binding(11) var history_gi: texture_2d<f32>;
@group(0) @binding(12) var moments_in: texture_2d<f32>;
@group(0) @binding(13) var history_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(14) var moments_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(15) var gbuffer_depth: texture_2d<f32>;
@group(0) @binding(16) var gbuffer_normal: texture_2d<f32>;
@group(0) @binding(17) var gbuffer_material: texture_2d<f32>;
@group(0) @binding(18) var previous_surface: texture_2d<f32>;
@group(0) @binding(19) var surface_history_out: texture_storage_2d<rgba16float, write>;

fn ready(level: u32) -> bool {
  return clipmap.rings[level].w != 0u;
}

// If a fine level is still being streamed, use the nearest ready coarse level.
fn ready_level(preferred: u32) -> u32 {
  var level = min(preferred, LEVEL_COUNT - 1u);
  loop {
    if (ready(level) || level == LEVEL_COUNT - 1u) { break; }
    level += 1u;
  }
  return level;
}

fn level_for_distance(distance: f32) -> u32 {
  if (distance < 8.0) { return 0u; }
  if (distance < 16.0) { return 1u; }
  if (distance < 32.0) { return 2u; }
  return 3u;
}

fn physical_cell(level: u32, world: vec3<f32>) -> vec3<i32> {
  let cell_size = f32(1 << level);
  let world_cell = vec3<i32>(floor(world / cell_size));
  let logical = world_cell - clipmap.origins[level].xyz;
  return (logical + vec3<i32>(clipmap.rings[level].xyz)) & vec3<i32>(CLIPMAP_SIZE - 1);
}

fn load_material(level: u32, world: vec3<f32>) -> vec4<f32> {
  let cell = physical_cell(level, world);
  switch level {
    case 0u: { return textureLoad(material0, cell, 0); }
    case 1u: { return textureLoad(material1, cell, 0); }
    case 2u: { return textureLoad(material2, cell, 0); }
    default: { return textureLoad(material3, cell, 0); }
  }
}

fn load_light(level: u32, world: vec3<f32>) -> vec4<f32> {
  let cell = physical_cell(level, world);
  switch level {
    case 0u: { return textureLoad(light0, cell, 0); }
    case 1u: { return textureLoad(light1, cell, 0); }
    case 2u: { return textureLoad(light2, cell, 0); }
    default: { return textureLoad(light3, cell, 0); }
  }
}

fn incident_light(level: u32, world: vec3<f32>) -> vec4<f32> {
  let radius = f32(1 << level) * 0.75;
  var result = load_light(level, world);
  result = max(result, load_light(level, world + vec3<f32>( radius, 0.0, 0.0)));
  result = max(result, load_light(level, world + vec3<f32>(-radius, 0.0, 0.0)));
  result = max(result, load_light(level, world + vec3<f32>(0.0,  radius, 0.0)));
  result = max(result, load_light(level, world + vec3<f32>(0.0, -radius, 0.0)));
  result = max(result, load_light(level, world + vec3<f32>(0.0, 0.0,  radius)));
  result = max(result, load_light(level, world + vec3<f32>(0.0, 0.0, -radius)));
  return result;
}

fn radical_inverse(bits_in: u32) -> f32 {
  var bits = bits_in;
  bits = (bits << 16u) | (bits >> 16u);
  bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
  bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
  bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
  return f32(bits) * 2.3283064365386963e-10;
}

fn hammersley(index: u32, count: u32) -> vec2<f32> {
  return vec2<f32>(f32(index) / f32(max(count, 1u)), radical_inverse(index));
}

fn cosine_direction(index: u32, count: u32, normal: vec3<f32>) -> vec3<f32> {
  let sample = fract(hammersley(index, count) + vec2<f32>(0.37, 0.11));
  let phi = 6.28318530718 * sample.x;
  let radius = sqrt(sample.y);
  let local = vec3<f32>(radius * cos(phi), radius * sin(phi), sqrt(max(1.0 - sample.y, 0.0)));
  let n = normalize(normal);
  let helper = select(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(0.0, 1.0, 0.0), abs(n.z) >= 0.999);
  let tangent = normalize(cross(helper, n));
  let bitangent = cross(n, tangent);
  return normalize(tangent * local.x + bitangent * local.y + n * local.z);
}

fn crossing_normal(axis: u32, step: vec3<i32>, fallback: vec3<f32>) -> vec3<f32> {
  if (axis == 0u) { return vec3<f32>(-f32(step.x), 0.0, 0.0); }
  if (axis == 1u) { return vec3<f32>(0.0, -f32(step.y), 0.0); }
  if (axis == 2u) { return vec3<f32>(0.0, 0.0, -f32(step.z)); }
  return fallback;
}

struct Hit {
  found: bool,
  position: vec3<f32>,
  normal: vec3<f32>,
  albedo: vec3<f32>,
};

fn trace_primary(origin: vec3<f32>, direction: vec3<f32>, max_distance: f32) -> Hit {
  var result = Hit(false, origin, -normalize(direction), vec3<f32>(0.0));
  var position = origin;
  var distance = 0.0;
  for (var crossing = 0u; crossing < MAX_CROSSINGS; crossing++) {
    if (distance >= max_distance) { break; }
    let level = ready_level(level_for_distance(distance));
    if (ready(level)) {
      let material = load_material(level, position);
      if (material.a >= 0.5) {
        result.found = true;
        result.position = position;
        result.albedo = material.rgb;
        return result;
      }
    }
    let cell_size = f32(1 << level);
    let cell = floor(position / cell_size);
    let step = vec3<i32>(sign(direction));
    let boundary = (cell + select(vec3<f32>(0.0), vec3<f32>(1.0), direction > vec3<f32>(0.0))) * cell_size;
    let t = (boundary - position) / direction;
    let valid_t = select(vec3<f32>(1.0e30), t, abs(direction) > vec3<f32>(1.0e-5));
    let next = min(min(valid_t.x, valid_t.y), valid_t.z);
    if (next >= 1.0e29) { break; }
    let axis = select(select(2u, 1u, valid_t.y <= valid_t.z), 0u, valid_t.x <= min(valid_t.y, valid_t.z));
    let advance = max(next, 0.01);
    position += direction * advance;
    distance += advance;
    result.normal = crossing_normal(axis, step, result.normal);
  }
  return result;
}

fn trace_secondary(origin: vec3<f32>, direction: vec3<f32>, max_distance: f32) -> vec3<f32> {
  var position = origin;
  var distance = 0.0;
  for (var crossing = 0u; crossing < MAX_CROSSINGS; crossing++) {
    if (distance >= max_distance) { break; }
    let level = ready_level(level_for_distance(distance));
    if (ready(level)) {
      let material = load_material(level, position);
      if (material.a >= 0.5) {
        // Sample the incident field immediately outside the occupied cell;
        // block light is stored in traversable voxels, not inside opaque ones.
        let light = incident_light(level, position - direction * 0.1);
        return material.rgb * (light.rgb * 8.0 + light.a * 0.35);
      }
    }
    let cell_size = f32(1 << level);
    let cell = floor(position / cell_size);
    let boundary = (cell + select(vec3<f32>(0.0), vec3<f32>(1.0), direction > vec3<f32>(0.0))) * cell_size;
    let t = (boundary - position) / direction;
    let valid_t = select(vec3<f32>(1.0e30), t, abs(direction) > vec3<f32>(1.0e-5));
    let next = min(min(valid_t.x, valid_t.y), valid_t.z);
    if (next >= 1.0e29) { break; }
    let advance = max(next, 0.01);
    position += direction * advance;
    distance += advance;
  }
  let sky = clipmap.sky_color.rgb * (0.35 + 0.65 * max(direction.y, 0.0));
  let sun = pow(max(dot(direction, -normalize(clipmap.sun_direction.xyz)), 0.0), 32.0)
    * vec3<f32>(1.0, 0.92, 0.72) * clipmap.sun_direction.w;
  return sky + sun * 0.35;
}

fn unproject(clip: vec4<f32>) -> vec3<f32> {
  // Rust uploads glam's column-major array, so multiply the four columns by
  // the corresponding clip components rather than dotting each column.
  let world = clipmap.inverse_view_proj[0] * clip.x
    + clipmap.inverse_view_proj[1] * clip.y
    + clipmap.inverse_view_proj[2] * clip.z
    + clipmap.inverse_view_proj[3] * clip.w;
  return world.xyz / max(abs(world.w), 1.0e-5);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let size = textureDimensions(output_gi);
  if (any(gid.xy >= size) || clipmap.params.w < 0.5) { return; }
  let uv = (vec2<f32>(gid.xy) + vec2<f32>(0.5)) / vec2<f32>(size);
  let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
  let near_world = unproject(vec4<f32>(ndc, 0.0, 1.0));
  let far_world = unproject(vec4<f32>(ndc, 1.0, 1.0));
  let primary_direction = normalize(far_world - near_world);
  let hit = trace_primary(clipmap.camera_position.xyz, primary_direction, clipmap.params.x);
  if (!hit.found) {
    textureStore(output_gi, vec2<i32>(gid.xy), vec4<f32>(0.0));
    return;
  }
  let rays = min(MAX_RAYS, u32(max(clipmap.params.y, 1.0)));
  var indirect = vec3<f32>(0.0);
  for (var ray = 0u; ray < MAX_RAYS; ray++) {
    if (ray >= rays) { break; }
    let direction = cosine_direction(ray, rays, hit.normal);
    indirect += trace_secondary(hit.position + hit.normal * 0.05, direction, clipmap.params.x);
  }
  // The scalar flood-fill already identifies low-frequency incident block
  // light in the free voxel next to the primary surface. Its RGB encoding
  // preserves the source hue while the four DDA rays add directional bounce.
  let hit_level = ready_level(level_for_distance(distance(hit.position, clipmap.camera_position.xyz)));
  let local_light = incident_light(hit_level, hit.position + hit.normal * 0.1).rgb * 8.0;
  let irradiance = hit.albedo
    * (indirect / f32(rays) + local_light * 0.95 + vec3<f32>(0.20))
    * clipmap.params.z;
  textureStore(output_gi, vec2<i32>(gid.xy), vec4<f32>(max(irradiance, vec3<f32>(0.0)), 1.0));
}

fn luminance(color: vec3<f32>) -> f32 {
  return dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn decode_oct(encoded: vec2<f32>) -> vec3<f32> {
  var n = vec3<f32>(encoded, 1.0 - abs(encoded.x) - abs(encoded.y));
  if (n.z < 0.0) {
    let folded = (vec2<f32>(1.0) - abs(n.yx)) * sign(n.xy);
    n.x = folded.x;
    n.y = folded.y;
  }
  return normalize(n);
}

fn ycgco(color: vec3<f32>) -> vec3<f32> {
  return vec3<f32>(0.25 * color.r + 0.5 * color.g + 0.25 * color.b,
    0.5 * color.r - 0.5 * color.b, -0.25 * color.r + 0.5 * color.g - 0.25 * color.b);
}

fn from_ycgco(value: vec3<f32>) -> vec3<f32> {
  return vec3<f32>(value.x + value.y - value.z, value.x - value.z, value.x - value.y - value.z);
}

@compute @workgroup_size(8, 8, 1)
fn temporal(@builtin(global_invocation_id) gid: vec3<u32>) {
  let size = textureDimensions(output_gi);
  if (any(gid.xy >= size)) { return; }
  let pixel = vec2<i32>(gid.xy);
  let current = textureLoad(input_gi, pixel, 0);
  let size_i = vec2<i32>(size);
  // Reproject the quarter-resolution world point into the previous camera.
  // A conservative z slice is used because the GI trace is itself the
  // quarter-resolution depth query; large motion is rejected below.
  let uv = (vec2<f32>(gid.xy) + vec2<f32>(0.5)) / vec2<f32>(size);
  let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
  let gbuffer_size = vec2<f32>(textureDimensions(gbuffer_depth));
  let current_full = vec2<i32>(floor(uv * gbuffer_size));
  let current_depth = textureLoad(gbuffer_depth, current_full, 0).r;
  let world = unproject(vec4<f32>(ndc, current_depth, 1.0));
  let previous_clip = clipmap.previous_view_proj[0] * world.x
    + clipmap.previous_view_proj[1] * world.y
    + clipmap.previous_view_proj[2] * world.z
    + clipmap.previous_view_proj[3];
  let previous_uv = vec2<f32>(previous_clip.x / max(abs(previous_clip.w), 1.0e-5) * 0.5 + 0.5,
    0.5 - previous_clip.y / max(abs(previous_clip.w), 1.0e-5) * 0.5);
  let previous_pixel = vec2<i32>(floor(previous_uv * vec2<f32>(size)));
  let valid_reprojection = all(previous_pixel >= vec2<i32>(0))
    && all(previous_pixel < size_i)
    && distance(previous_uv, uv) < 0.20;
  let safe_previous_pixel = clamp(previous_pixel, vec2<i32>(0), size_i - 1);
  let previous_full = vec2<i32>(floor(previous_uv * gbuffer_size));
  let safe_current_full = clamp(current_full, vec2<i32>(0), vec2<i32>(gbuffer_size) - 1);
  let safe_previous_full = clamp(previous_full, vec2<i32>(0), vec2<i32>(gbuffer_size) - 1);
  let old_surface = textureLoad(previous_surface, safe_previous_pixel, 0);
  let old_depth = old_surface.r;
  let current_normal = decode_oct(textureLoad(gbuffer_normal, safe_current_full, 0).xy);
  let old_normal_xy = old_surface.gb;
  let old_normal = decode_oct(old_normal_xy);
  // G-buffer light alpha is the stable material layer/class ID.  Comparing
  // this scalar avoids rejecting history because albedo lighting changed.
  let current_material = textureLoad(gbuffer_material, safe_current_full, 0).a;
  let previous_material = old_surface.a;
  // Compare camera-space linear distances. Raw depth-buffer values are
  // non-linear and would reject nearly every far surface.
  let current_linear_depth = distance(world, clipmap.camera_position.xyz);
  let depth_delta = abs(current_linear_depth - old_depth);
  let depth_relative = depth_delta / max(max(abs(current_linear_depth), abs(old_depth)), 1.0e-3);
  let depth_reject = depth_delta > 0.50 || depth_relative > 0.02;
  let normal_reject = dot(current_normal, old_normal) < 0.90;
  let material_reject = abs(current_material - previous_material) > (0.5 / 255.0);
  let surface_match = !(depth_reject || normal_reject || material_reject);
  let valid_surface = valid_reprojection && surface_match;
  let history = select(vec4<f32>(0.0), textureLoad(history_gi, safe_previous_pixel, 0), valid_surface);
  var lo = vec3<f32>(1.0e20);
  var hi = vec3<f32>(-1.0e20);
  for (var oy = -1; oy <= 1; oy++) {
    for (var ox = -1; ox <= 1; ox++) {
      let sample_pixel = clamp(pixel + vec2<i32>(ox, oy), vec2<i32>(0), size_i - 1);
      let neighborhood = ycgco(textureLoad(input_gi, sample_pixel, 0).rgb);
      lo = min(lo, neighborhood);
      hi = max(hi, neighborhood);
    }
  }
  let range = hi - lo;
  let history_ycgco = clamp(ycgco(history.rgb), lo - range * 0.10, hi + range * 0.10);
  let clamped_history = vec4<f32>(max(from_ycgco(history_ycgco), vec3<f32>(0.0)), history.a);
  let previous_moments = textureLoad(moments_in, pixel, 0).rg;
  let history_weight = select(0.0, 0.90, clipmap.camera_position.w > 0.5 && valid_surface);
  let resolved = mix(current, clamped_history, history_weight);
  let luma = luminance(resolved.rgb);
  let moments = mix(vec2<f32>(luma, luma * luma), previous_moments, history_weight);
  textureStore(output_gi, pixel, resolved);
  textureStore(history_out, pixel, resolved);
  textureStore(moments_out, pixel, vec4<f32>(moments, 0.0, 1.0));
  let current_oct = textureLoad(gbuffer_normal, safe_current_full, 0).xy;
  textureStore(surface_history_out, pixel, vec4<f32>(current_linear_depth, current_oct, textureLoad(gbuffer_material, safe_current_full, 0).a));
}

fn denoise_pixel(pixel: vec2<i32>, step_width: i32) -> vec4<f32> {
  let size = vec2<i32>(textureDimensions(input_gi));
  let center = textureLoad(input_gi, pixel, 0);
  let center_luma = luminance(center.rgb);
  let gbuffer_size = vec2<f32>(textureDimensions(gbuffer_depth));
  let center_full = clamp(vec2<i32>(floor((vec2<f32>(pixel) + 0.5) / vec2<f32>(size) * gbuffer_size)),
    vec2<i32>(0), vec2<i32>(gbuffer_size) - 1);
  let center_depth = textureLoad(gbuffer_depth, center_full, 0).r;
  let center_normal = decode_oct(textureLoad(gbuffer_normal, center_full, 0).xy);
  let center_material = textureLoad(gbuffer_material, center_full, 0).a;
  let variance = max(textureLoad(moments_in, pixel, 0).g
    - textureLoad(moments_in, pixel, 0).r * textureLoad(moments_in, pixel, 0).r, 0.0);
  let offsets = array<vec2<i32>, 8>(
    vec2<i32>(-1, 0), vec2<i32>(1, 0), vec2<i32>(0, -1), vec2<i32>(0, 1),
    vec2<i32>(-1, -1), vec2<i32>(1, -1), vec2<i32>(-1, 1), vec2<i32>(1, 1),
  );
  var sum = center.rgb * 4.0;
  var total = 4.0;
  for (var index = 0u; index < 8u; index++) {
    let sample_pixel = clamp(pixel + offsets[index] * step_width, vec2<i32>(0), size - 1);
    let sample = textureLoad(input_gi, sample_pixel, 0).rgb;
    let sample_full = clamp(vec2<i32>(floor((vec2<f32>(sample_pixel) + 0.5) / vec2<f32>(size) * gbuffer_size)),
      vec2<i32>(0), vec2<i32>(gbuffer_size) - 1);
    let sample_depth = textureLoad(gbuffer_depth, sample_full, 0).r;
    let sample_normal = decode_oct(textureLoad(gbuffer_normal, sample_full, 0).xy);
    let sample_material = textureLoad(gbuffer_material, sample_full, 0).a;
    // Radiance and luma variance jointly preserve high-contrast edges.
    let depth_weight = exp(-abs(sample_depth - center_depth) / 0.02);
    let normal_weight = pow(max(dot(sample_normal, center_normal), 0.0), 32.0);
    let material_weight = select(0.0, 1.0, abs(sample_material - center_material) <= (0.5 / 255.0));
    let radiance_weight = exp(-abs(luminance(sample) - center_luma) / (sqrt(variance) + 0.02));
    let weight = depth_weight * normal_weight * material_weight * radiance_weight;
    sum += sample * weight;
    total += weight;
  }
  return vec4<f32>(max(sum / max(total, 1.0), vec3<f32>(0.0)), center.a);
}

fn denoise_store(gid: vec3<u32>, step_width: i32) {
  let size = textureDimensions(output_gi);
  if (any(gid.xy >= size)) { return; }
  textureStore(output_gi, vec2<i32>(gid.xy), denoise_pixel(vec2<i32>(gid.xy), step_width));
}

@compute @workgroup_size(8, 8, 1)
fn denoise_1(@builtin(global_invocation_id) gid: vec3<u32>) { denoise_store(gid, 1); }

@compute @workgroup_size(8, 8, 1)
fn denoise_2(@builtin(global_invocation_id) gid: vec3<u32>) { denoise_store(gid, 2); }

@compute @workgroup_size(8, 8, 1)
fn denoise_4(@builtin(global_invocation_id) gid: vec3<u32>) { denoise_store(gid, 4); }

@compute @workgroup_size(8, 8, 1)
