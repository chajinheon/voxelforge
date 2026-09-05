// M7 half-resolution GTAO: reconstructed world-space 1.5-block horizons,
// separable 5x5 bilateral filtering, and motion-reprojected R8 history.
struct Params {
  inverse_view_proj: mat4x4<f32>, camera_pos: vec4<f32>, viewport: vec4<f32>,
  depth_range: vec4<f32>, frame: u32, static_weight: f32, motion_weight: f32, _pad: f32,
};
@group(0) @binding(0) var depth_tex: texture_depth_2d;
@group(0) @binding(1) var normal_tex: texture_2d<f32>;
@group(0) @binding(2) var motion_tex: texture_2d<f32>;
@group(0) @binding(3) var previous_ao: texture_2d<f32>;
@group(0) @binding(4) var previous_depth: texture_2d<f32>;
@group(0) @binding(5) var previous_normal: texture_2d<f32>;
@group(0) @binding(6) var raw_ao: texture_2d<f32>;
@group(0) @binding(7) var point_sampler: sampler;
@group(0) @binding(8) var<uniform> params: Params;
@group(0) @binding(9) var horizontal_ao: texture_2d<f32>;
@group(0) @binding(10) var blue_noise: texture_2d<f32>;
struct Vertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs_main(@builtin(vertex_index) index: u32) -> Vertex {
  var p = array<vec2<f32>, 3>(vec2(-1., -1.), vec2(3., -1.), vec2(-1., 3.));
  var out: Vertex; out.position = vec4(p[index], 0., 1.); out.uv = p[index] * .5 + .5; out.uv.y = 1. - out.uv.y; return out;
}
fn decode_oct(encoded: vec2<f32>) -> vec3<f32> {
  var n = vec3(encoded, 1. - abs(encoded.x) - abs(encoded.y));
  if (n.z < 0.) { let folded = (vec2(1.) - abs(n.yx)) * sign(n.xy); n.x = folded.x; n.y = folded.y; }
  return normalize(n);
}
fn full_coord(uv: vec2<f32>) -> vec2<i32> {
  let size = vec2<i32>(textureDimensions(depth_tex)); return clamp(vec2<i32>(uv * vec2<f32>(size)), vec2(0), size - vec2(1));
}
fn depth_at(uv: vec2<f32>) -> f32 { return textureLoad(depth_tex, full_coord(uv), 0); }
fn normal_at(uv: vec2<f32>) -> vec3<f32> { return decode_oct(textureLoad(normal_tex, full_coord(uv), 0).xy); }
fn world_at(uv: vec2<f32>, depth: f32) -> vec3<f32> {
  let clip = vec4(uv.x * 2. - 1., 1. - uv.y * 2., depth, 1.);
  let world = params.inverse_view_proj * clip; return world.xyz / max(abs(world.w), .00001);
}
fn linear_depth(depth: f32) -> f32 {
  let n = params.depth_range.x; let f = params.depth_range.y;
  return n * f / max(f - clamp(depth, 0., 1.) * (f - n), .00001);
}
fn blue_rotation(pixel: vec2<f32>) -> f32 {
  let size = vec2<i32>(textureDimensions(blue_noise)); let p = vec2<i32>(pixel) % size;
  return textureLoad(blue_noise, p, 0).r * 6.2831853 + f32(params.frame % 8u) * .78539816;
}
// Eight directions and four samples per direction, with radius in world blocks.
fn raw_horizon(uv: vec2<f32>) -> f32 {
  let center_depth = depth_at(uv); let center = world_at(uv, center_depth); let n = normal_at(uv);
  let pixel = uv * params.viewport.xy; let rotation = blue_rotation(pixel); var occlusion = 0.;
  let view_distance = max(length(center - params.camera_pos.xyz), .05);
  let focal_y = params.viewport.y * 0.714705;
  let focal_x = focal_y;
  let screen_radius_x = 1.5 * focal_x / max(view_distance * params.viewport.x, 1.0);
  let screen_radius_y = 1.5 * focal_y / max(view_distance * params.viewport.y, 1.0);
  for (var direction = 0u; direction < 8u; direction++) {
    let angle = rotation + f32(direction) * .78539816; let axis = vec2(cos(angle), sin(angle)); var horizon = 0.;
    for (var step = 1u; step <= 4u; step++) {
      let distance = 1.5 * f32(step) / 4.; let sample_uv = uv + vec2(axis.x * screen_radius_x, axis.y * screen_radius_y) * f32(step) / 4.;
      let sample = world_at(sample_uv, depth_at(sample_uv)); let separation = length(sample - center);
      let elevation = max(dot(sample - center, n) - .20, 0.); let falloff = select(1., clamp((1.5 - separation) / (1.5 - .9), 0., 1.), separation > .9);
      horizon = max(horizon, elevation / max(separation, .0001) * falloff);
    }
    occlusion += clamp(horizon, 0., 1.);
  }
  return clamp(1. - occlusion / 8., 0., 1.);
}
@fragment fn raw_main(input: Vertex) -> @location(0) f32 { return raw_horizon(input.uv); }
fn bilateral_weight(a: f32, b: f32, na: vec3<f32>, nb: vec3<f32>) -> f32 {
  let d = (a - b) / .75; return exp(-.5 * d * d) * pow(max(dot(na, nb), 0.), 32.);
}
@fragment fn horizontal_main(input: Vertex) -> @location(0) f32 {
  let size = vec2<i32>(textureDimensions(raw_ao)); let p = vec2<i32>(input.position.xy); let uv = (vec2<f32>(p) + .5) / vec2<f32>(size);
  let cd = linear_depth(depth_at(uv)); let cn = normal_at(uv); var sum = 0.; var weights = 0.;
  for (var offset = -2; offset <= 2; offset++) { let q = clamp(p + vec2<i32>(offset, 0), vec2(0), size - vec2(1)); let quv = (vec2<f32>(q) + .5) / vec2<f32>(size); let w = bilateral_weight(cd, linear_depth(depth_at(quv)), cn, normal_at(quv)); sum += textureLoad(raw_ao, q, 0).r * w; weights += w; }
  return sum / max(weights, .0001);
}
@fragment fn temporal_main(input: Vertex) -> @location(0) f32 {
  let size = vec2<i32>(textureDimensions(horizontal_ao)); let p = vec2<i32>(input.position.xy); let uv = (vec2<f32>(p) + .5) / vec2<f32>(size);
  let cd = linear_depth(depth_at(uv)); let cn = normal_at(uv); var filtered = 0.; var weights = 0.;
  for (var offset = -2; offset <= 2; offset++) { let q = clamp(p + vec2<i32>(0, offset), vec2(0), size - vec2(1)); let quv = (vec2<f32>(q) + .5) / vec2<f32>(size); let w = bilateral_weight(cd, linear_depth(depth_at(quv)), cn, normal_at(quv)); filtered += textureLoad(horizontal_ao, q, 0).r * w; weights += w; }
  filtered /= max(weights, .0001); let full = full_coord(uv); let motion = textureLoad(motion_tex, full, 0).xy; let motion_pixels = length(motion * params.viewport.xy); let history_uv = uv - motion;
  let old_size = vec2<i32>(textureDimensions(previous_depth)); let old_coord = clamp(vec2<i32>(history_uv * vec2<f32>(old_size)), vec2(0), old_size - vec2(1));
  let old_depth = linear_depth(textureLoad(previous_depth, old_coord, 0).r); let old_normal = decode_oct(textureLoad(previous_normal, old_coord, 0).xy);
  let reject = params.frame == 0u || any(history_uv < vec2(0.)) || any(history_uv > vec2(1.)) || abs(cd - old_depth) > .5 || abs(cd - old_depth) / max(abs(cd), .0001) > .02 || dot(cn, old_normal) < .85;
  let history = textureSampleLevel(previous_ao, point_sampler, history_uv, 0.).r; let weight = select(params.static_weight, params.motion_weight, motion_pixels > 1.);
  return select(mix(filtered, history, weight), filtered, reject);
}
