struct ExposureParams { src_size: vec2<u32>, middle_gray: f32, min_exposure: f32, max_exposure: f32, adaptation: f32, delta_seconds: f32, pad: f32, };
@group(0) @binding(0) var hdr: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> result: array<f32>;
@group(0) @binding(2) var<uniform> params: ExposureParams;

fn luminance(color: vec3<f32>) -> f32 { return max(dot(max(color, vec3<f32>(0.0)), vec3<f32>(0.2126, 0.7152, 0.0722)), 1e-4); }

var<workgroup> partial: array<f32, 256>;
var<workgroup> valid: array<u32, 256>;

@compute @workgroup_size(16, 16, 1)
fn reduce_log_luminance(@builtin(global_invocation_id) id: vec3<u32>) {
  let local = id.x % 16u + (id.y % 16u) * 16u;
  let inside = id.x < params.src_size.x && id.y < params.src_size.y;
  partial[local] = select(0.0, log2(luminance(textureLoad(hdr, vec2<i32>(id.xy), 0).rgb)), inside);
  valid[local] = select(0u, 1u, inside);
  workgroupBarrier();
  var stride = 128u;
  loop {
    if (local < stride) { partial[local] += partial[local + stride]; }
    if (local < stride) { valid[local] += valid[local + stride]; }
    workgroupBarrier();
    if (stride == 1u) { break; }
    stride /= 2u;
  }
  if (local == 0u) {
    let group = (id.y / 16u) * ((params.src_size.x + 15u) / 16u) + id.x / 16u;
    let groups = (params.src_size.x + 15u) / 16u * (params.src_size.y + 15u) / 16u;
    result[group] = partial[0];
    result[groups + group] = f32(valid[0]);
  }
}

@compute @workgroup_size(1, 1, 1)
fn adapt(@builtin(global_invocation_id) id: vec3<u32>) {
  if (id.x != 0u || id.y != 0u) { return; }
  let partial_count = ((params.src_size.x + 15u) / 16u) * ((params.src_size.y + 15u) / 16u);
  var sum = 0.0;
  var count = 0.0;
  for (var index = 0u; index < partial_count; index++) { sum += result[index]; count += result[partial_count + index]; }
  let average_log = sum / max(count, 1.0);
  let target_exposure = clamp(params.middle_gray / exp2(average_log), params.min_exposure, params.max_exposure);
  let alpha = 1.0 - exp(-max(params.adaptation, 0.0) * max(params.delta_seconds, 0.0));
  let state = params.src_size.x * params.src_size.y;
  let previous = result[state];
  result[state] = clamp(previous + (target_exposure - previous) * alpha, params.min_exposure, params.max_exposure);
  result[state + 1u] = target_exposure;
  result[1u] = result[state];
}
