struct TaauParams { output_size: vec2<u32>, current_size: vec2<u32>, reset: u32, pad: u32, };
@group(0) @binding(0) var current_color: texture_2d<f32>;
@group(0) @binding(1) var history_color: texture_2d<f32>;
@group(0) @binding(2) var current_depth: texture_2d<f32>;
@group(0) @binding(3) var history_depth: texture_2d<f32>;
@group(0) @binding(4) var current_normal: texture_2d<f32>;
@group(0) @binding(5) var history_normal: texture_2d<f32>;
@group(0) @binding(6) var current_material: texture_2d<f32>;
@group(0) @binding(7) var history_material: texture_2d<f32>;
@group(0) @binding(8) var motion_reactive: texture_2d<f32>;
@group(0) @binding(9) var output_color: texture_storage_2d<rgba16float, write>;
@group(0) @binding(10) var<uniform> params: TaauParams;

fn rgb_to_ycocg(rgb: vec3<f32>) -> vec3<f32> { return vec3<f32>(0.25 * rgb.r + 0.5 * rgb.g + 0.25 * rgb.b, 0.5 * rgb.r - 0.5 * rgb.b, -0.25 * rgb.r + 0.5 * rgb.g - 0.25 * rgb.b); }
fn ycocg_to_rgb(value: vec3<f32>) -> vec3<f32> { return vec3<f32>(value.x + value.y - value.z, value.x + value.z, value.x - value.y - value.z); }

@compute @workgroup_size(8, 8, 1)
fn reconstruct(@builtin(global_invocation_id) id: vec3<u32>) {
  if (id.x >= params.output_size.x || id.y >= params.output_size.y) { return; }
  let uv = (vec2<f32>(id.xy) + vec2<f32>(0.5)) / vec2<f32>(params.output_size);
  let current_uv = uv * vec2<f32>(params.current_size);
  let current_point = min(vec2<u32>(current_uv), params.current_size - vec2<u32>(1u));
  let current = textureLoad(current_color, vec2<i32>(current_point), 0).rgb;
  let old = textureLoad(history_color, vec2<i32>(id.xy), 0).rgb;
  let depth = textureLoad(current_depth, vec2<i32>(current_point), 0).r;
  let old_depth = textureLoad(history_depth, vec2<i32>(id.xy), 0).r;
  let normal = normalize(textureLoad(current_normal, vec2<i32>(current_point), 0).xyz);
  let old_normal = normalize(textureLoad(history_normal, vec2<i32>(id.xy), 0).xyz);
  let material = textureLoad(current_material, vec2<i32>(current_point), 0).r;
  let old_material = textureLoad(history_material, vec2<i32>(id.xy), 0).r;
  let reactive = textureLoad(motion_reactive, vec2<i32>(current_point), 0).a;
  let reject = params.reset != 0u || abs(depth - old_depth) > 0.50 || abs(depth - old_depth) / max(abs(depth), 1e-4) > 0.02 || dot(normal, old_normal) < 0.90 || material != old_material || reactive >= 0.95;
  let history_weight = select(0.92 * (1.0 - 0.80 * reactive), 0.0, reject);
  let clamped_history = ycocg_to_rgb(clamp(rgb_to_ycocg(old), rgb_to_ycocg(current - vec3<f32>(0.2)), rgb_to_ycocg(current + vec3<f32>(0.2))));
  textureStore(output_color, vec2<i32>(id.xy), vec4<f32>(max(mix(current, clamped_history, history_weight), vec3<f32>(0.0)), 1.0));
}
