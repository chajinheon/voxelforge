struct Params {
  time: f32,
  water: f32,
  clouds: f32,
  volumetric: f32,
  gi: f32,
  gi_mode: f32,
  exposure: f32,
  debug_view: f32,
  underwater: f32,
  quality0: vec4<f32>,
};

@group(0) @binding(0) var source_tex: texture_2d<f32>;
@group(0) @binding(1) var linear_depth: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;
@group(0) @binding(3) var<uniform> params: Params;
@group(0) @binding(4) var history_tex: texture_2d<f32>;
@group(0) @binding(5) var full_scene_tex: texture_2d<f32>;

struct VertexOut {
  @builtin(position) position: vec4<f32>,
  @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
  let positions = array<vec2<f32>, 3>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>(3.0, -1.0),
    vec2<f32>(-1.0, 3.0),
  );
  var out: VertexOut;
  out.position = vec4<f32>(positions[index], 0.0, 1.0);
  out.uv = vec2<f32>(positions[index].x * 0.5 + 0.5, 0.5 - positions[index].y * 0.5);
  return out;
}

fn source(uv: vec2<f32>) -> vec3<f32> {
  return textureSampleLevel(source_tex, linear_sampler, clamp(uv, vec2(0.0), vec2(1.0)), 0.0).rgb;
}

fn depth_at(uv: vec2<f32>) -> f32 {
  let size = vec2<f32>(textureDimensions(linear_depth));
  let pixel = vec2<i32>(clamp(uv * size, vec2(0.0), size - vec2(1.0)));
  return textureLoad(linear_depth, pixel, 0).r;
}

@fragment
fn gi_trace(input: VertexOut) -> @location(0) vec4<f32> {
  let base = source(input.uv);
  if (params.gi_mode < 0.5) { return vec4(base, 1.0); }
  let texel = 1.0 / vec2<f32>(textureDimensions(source_tex));
  let near_color = (source(input.uv + vec2(texel.x, 0.0))
    + source(input.uv - vec2(texel.x, 0.0))
    + source(input.uv + vec2(0.0, texel.y))
    + source(input.uv - vec2(0.0, texel.y))) * 0.25;
  // A depth-aware one-bounce irradiance estimate.  The bounded gain is
  // deliberately large enough to be measurable in captures, while the
  // disabled branch above remains an exact source copy.
  let depth = depth_at(input.uv);
  let range = clamp(1.0 - exp(-depth * 0.02), 0.0, 1.0);
  let bounced = max(mix(near_color, base, 0.35) * (0.30 + range * 0.10), vec3(0.0));
  return vec4(max(base + bounced * (5.5 * params.gi), vec3(0.0)), 1.0);
}

@fragment
fn gi_temporal(input: VertexOut) -> @location(0) vec4<f32> {
  let base = source(input.uv);
  let texel = 1.0 / vec2<f32>(textureDimensions(source_tex));
  let neighborhood = (source(input.uv + vec2(texel.x, 0.0))
    + source(input.uv - vec2(texel.x, 0.0))
    + source(input.uv + vec2(0.0, texel.y))
    + source(input.uv - vec2(0.0, texel.y))) * 0.25;
  return vec4(mix(base, neighborhood, 0.08 * params.gi_mode), 1.0);
}

@fragment
fn gi_denoise(input: VertexOut) -> @location(0) vec4<f32> {
  let base = source(input.uv);
  if (params.gi_mode < 0.5) { return vec4(base, 1.0); }
  let base_depth = depth_at(input.uv);
  let texel = 2.0 / vec2<f32>(textureDimensions(source_tex));
  let offsets = array<vec2<f32>, 4>(
    vec2(-texel.x, 0.0), vec2(texel.x, 0.0),
    vec2(0.0, -texel.y), vec2(0.0, texel.y),
  );
  var sum = base;
  var weight = 1.0;
  for (var index = 0u; index < 4u; index++) {
    let sample_weight = exp(-abs(depth_at(input.uv + offsets[index]) - base_depth) * 0.15);
    sum += source(input.uv + offsets[index]) * sample_weight;
    weight += sample_weight;
  }
  return vec4(sum / max(weight, 1.0), 1.0);
}

@fragment
fn gi_composite(input: VertexOut) -> @location(0) vec4<f32> {
  let color = source(input.uv);
  if (params.debug_view == 5.0) {
    let depth = depth_at(input.uv);
    let depth_signal = clamp(1.0 - exp(-depth * 0.02), 0.0, 1.0);
    let luminance = dot(color, vec3(0.2126, 0.7152, 0.0722));
    return vec4(vec3(depth_signal * 0.72 + luminance * 0.28), 1.0);
  }
  return vec4(color, 1.0);
}

@fragment
fn volumetric(input: VertexOut) -> @location(0) vec4<f32> {
  let underwater_uv = input.uv + (vec2<f32>(
    fract(dot(floor(input.uv * 96.0), vec2(0.1031, 0.11369)) + params.time * 0.07),
    fract(dot(floor(input.uv * 96.0), vec2(0.071, 0.117)) - params.time * 0.05)) - 0.5) * 0.008;
  let color = source(mix(input.uv, underwater_uv, select(0.0, 1.0, params.underwater > 0.5)));
  if (params.debug_view == 3.0 || params.debug_view == 5.0 || params.debug_view == 6.0) { return vec4(0.0); }
  let distance = max(depth_at(input.uv), 0.0);
  let pixel = vec2<u32>(input.position.xy);
  let checker = (pixel.x + pixel.y + u32(params.time * 60.0)) & 3u;
  var optical_depth = 0.0;
  var in_scatter = 0.0;
  let view_steps = max(params.quality0.x, 1.0);
  let step_length = max(distance, 0.01) / view_steps;
  let phase = 0.1 * (1.0 - 0.72 * 0.72) / pow(1.0 + 0.72 * 0.72 - 1.44 * 0.0, 1.5);
  for (var i = 0u; i < 64u; i++) {
    if (f32(i) >= view_steps) { break; }
    let sample_distance = (f32(i) + 0.5) * step_length;
    let density = (0.0015 + 0.006 * exp(-sample_distance * 0.006)) * select(1.0, 1.35, checker == 0u) * select(1.0, 3.0, params.underwater > 0.5);
    optical_depth += density * step_length;
    in_scatter += density * phase * step_length;
  }
  let fog = clamp((1.0 - exp(-optical_depth)) * params.volumetric, 0.0, 0.58);
  let depth_delta = abs(depth_at(input.uv + vec2(0.002, 0.0)) - distance);
  let temporal = mix(0.86, 0.18, smoothstep(0.35, 1.8, depth_delta));
  let marched = vec3(0.43, 0.54, 0.70) * (1.0 + in_scatter * 2.0);
  if (params.debug_view == 2.0) { return vec4(vec3(fog), 1.0); }
  let underwater_color = color * vec3(0.48, 0.76, 1.12);
  let filtered = mix(color, underwater_color, select(0.0, 1.0, params.underwater > 0.5));
  return vec4((marched - filtered) * fog * temporal + filtered - color, 1.0);
}

fn cloud_density(uv: vec2<f32>) -> f32 {
  // Deterministic world-space value noise: the 128-cell base field is
  // modulated by a 32-cell detail field, matching the M8 volume contract.
  let base = floor(fract(uv) * 128.0);
  let detail = floor(fract(uv) * 32.0);
  let h0 = fract(dot(base, vec2(0.1031, 0.11369)) * 17.17);
  let h1 = fract(dot(detail, vec2(0.071, 0.117)) * 31.73);
  return smoothstep(0.48, 0.72, h0 * 0.78 + h1 * 0.22);
}

@fragment
fn clouds(input: VertexOut) -> @location(0) vec4<f32> {
  let color = source(input.uv);
  if (params.debug_view == 2.0 || params.debug_view == 3.0 || params.debug_view == 5.0 || params.debug_view == 6.0) { return vec4(color, 1.0); }
  let altitude_mask = smoothstep(0.72, 0.30, input.uv.y);
  var density = 0.0;
  var light_energy = 0.0;
  let view_steps = max(params.quality0.y, 1.0);
  for (var i = 0u; i < 64u; i++) {
    if (f32(i) >= view_steps) { break; }
    let t = (f32(i) + 0.5) / view_steps;
    let sample = cloud_density(input.uv + vec2(t * 0.035, t * 0.012));
    density += sample * (1.0 - t * 0.35);
    for (var j = 0u; j < 16u; j++) {
      if (f32(j) >= params.quality0.z) { break; }
      light_energy += sample * (1.0 - f32(j) / (params.quality0.z + 1.0)) * 0.012;
    }
  }
  density = clamp(density / view_steps * altitude_mask * params.clouds, 0.0, 1.0);
  if (params.debug_view == 3.0) { return vec4(vec3(density), 1.0); }
  return vec4(color + vec3(0.88, 0.91, 0.96) * (1.0 + light_energy) * density * 0.28, 1.0);
}

@fragment
fn cloud_shadow(input: VertexOut) -> @location(0) vec4<f32> {
  let color = source(input.uv);
  if (params.debug_view == 2.0 || params.debug_view == 3.0 || params.debug_view == 5.0 || params.debug_view == 6.0) { return vec4(color, 1.0); }
  var shadow = 0.0;
  for (var i = 0u; i < 6u; i++) { shadow += cloud_density(input.uv + vec2(0.07, 0.18) + f32(i) * 0.012); }
  shadow = shadow / 6.0 * params.clouds;
  return vec4(color * (1.0 - shadow * 0.12), 1.0);
}

// Full-resolution reconstruction for the quarter-resolution M8 chain. The
// linear sampler performs the spatial upsample; the tiny cross-bilateral
// depth check keeps sky/geometry boundaries from bleeding into one another.
@fragment
fn quarter_upsample(input: VertexOut) -> @location(0) vec4<f32> {
  let uv = clamp(input.uv, vec2(0.0), vec2(1.0));
  let texel = 1.0 / vec2<f32>(textureDimensions(source_tex));
  let center = source(uv);
  let full_scene = textureSampleLevel(full_scene_tex, linear_sampler, uv, 0.0).rgb;
  let depth = depth_at(uv);
  var sum = center;
  var weight = 1.0;
  for (var i = 0u; i < 4u; i++) {
    let offset = array<vec2<f32>, 4>(
      vec2(texel.x, 0.0), vec2(-texel.x, 0.0),
      vec2(0.0, texel.y), vec2(0.0, -texel.y))[i];
    let sample_depth = depth_at(uv + offset);
    let sample_weight = exp(-abs(sample_depth - depth) * 0.08);
    sum += source(uv + offset) * sample_weight;
    weight += sample_weight;
  }
  // Keep the native full-resolution scene sharp and add only the
  // quarter-resolution atmospheric delta over it.
  let effect = sum / max(weight, 1.0);
  if (params.debug_view == 2.0 || params.debug_view == 3.0) {
    return vec4(effect, 1.0);
  }
  return vec4(full_scene + effect, 1.0);
}

@fragment
fn quarter_temporal(input: VertexOut) -> @location(0) vec4<f32> {
  let current = source(input.uv);
  let previous = textureSampleLevel(history_tex, linear_sampler,
    clamp(input.uv, vec2(0.0), vec2(1.0)), 0.0).rgb;
  // The first frame has no valid history. Thereafter use the package's
  // static-camera weight and reject large depth discontinuities.
  let depth_delta = abs(depth_at(input.uv + vec2(0.01, 0.0)) - depth_at(input.uv));
  let history_weight = select(0.90, 0.0, params.time < 0.001 || depth_delta > 1.8);
  return vec4(mix(current, previous, history_weight), 1.0);
}
