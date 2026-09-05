struct BloomParams { threshold: f32, knee: f32, intensity: f32, level: u32, src_size: vec2<u32>, dst_size: vec2<u32>, };
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba16float, write>;
@group(0) @binding(2) var<uniform> params: BloomParams;

fn threshold_color(color: vec3<f32>) -> vec3<f32> {
  let peak = max(max(color.r, color.g), max(color.b, 0.0));
  let knee = max(params.knee, 1e-4);
  let soft = clamp((peak - params.threshold + knee) / (2.0 * knee), 0.0, 1.0);
  let contribution = max(peak - params.threshold, 0.0) + soft * soft * knee;
  return max(color, vec3<f32>(0.0)) * (contribution / max(peak, 1e-4));
}

@compute @workgroup_size(16, 16, 1)
fn downsample(@builtin(global_invocation_id) id: vec3<u32>) {
  if (id.x >= params.dst_size.x || id.y >= params.dst_size.y) { return; }
  let base = id.xy * 2u;
  var sum = vec3<f32>(0.0);
  for (var oy: u32 = 0u; oy < 2u; oy++) {
    for (var ox: u32 = 0u; ox < 2u; ox++) {
      let p = min(base + vec2<u32>(ox, oy), params.src_size - vec2<u32>(1u));
      sum += textureLoad(src, vec2<i32>(p), 0).rgb;
    }
  }
  var value = sum * 0.25;
  if (params.level == 0u) { value = threshold_color(value); }
  textureStore(dst, vec2<i32>(id.xy), vec4<f32>(max(value, vec3<f32>(0.0)), 1.0));
}

@compute @workgroup_size(16, 16, 1)
fn upsample(@builtin(global_invocation_id) id: vec3<u32>) {
  if (id.x >= params.dst_size.x || id.y >= params.dst_size.y) { return; }
  let p = min(id.xy / 2u, params.src_size - vec2<u32>(1u));
  let value = textureLoad(src, vec2<i32>(p), 0).rgb * params.intensity;
  textureStore(dst, vec2<i32>(id.xy), vec4<f32>(max(value, vec3<f32>(0.0)), 1.0));
}
