// Edge-aware 3-stage à-trous GI denoiser, steps 1, 2 and 4.
const KERNEL: array<f32, 5> = array<f32, 5>(1.0, 4.0, 6.0, 4.0, 1.0);

@group(0) @binding(0) var input_gi: texture_2d<f32>;
@group(0) @binding(1) var depth: texture_2d<f32>;
@group(0) @binding(2) var normal: texture_2d<f32>;
@group(0) @binding(3) var variance: texture_2d<f32>;
@group(1) @binding(0) var output_gi: texture_storage_2d<rgba16float, write>;
@group(1) @binding(1) var<uniform> step_uniform: u32;

fn luminance(c: vec3<f32>) -> f32 {
  return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let size = textureDimensions(output_gi);
  if (gid.x >= size.x || gid.y >= size.y) { return; }
  let pixel = vec2<i32>(gid.xy);
  let center_depth = textureLoad(depth, pixel, 0).r;
  let center_normal = normalize(textureLoad(normal, pixel, 0).xyz);
  let center = textureLoad(input_gi, pixel, 0);
  let center_luma = luminance(center.rgb);
  let center_variance = textureLoad(variance, pixel, 0).r;
  var sum = vec3<f32>(0.0);
  var total = 0.0;
  for (var y = -2; y <= 2; y++) {
    for (var x = -2; x <= 2; x++) {
      let offset = vec2<i32>(x, y) * i32(step_uniform);
      let sample_pixel = clamp(pixel + offset, vec2<i32>(0), vec2<i32>(size) - 1);
      let sample = textureLoad(input_gi, sample_pixel, 0);
      let sample_depth = textureLoad(depth, sample_pixel, 0).r;
      let sample_normal = normalize(textureLoad(normal, sample_pixel, 0).xyz);
      let depth_weight = exp(-abs(sample_depth - center_depth) / (0.02 * abs(center_depth) + 0.05));
      let normal_weight = pow(max(dot(sample_normal, center_normal), 0.0), 32.0);
      let luma_weight = exp(-abs(luminance(sample.rgb) - center_luma) / (sqrt(max(center_variance, 0.0)) + 0.02));
      let weight = KERNEL[x + 2] * KERNEL[y + 2] / 256.0 * depth_weight * normal_weight * luma_weight;
      sum += sample.rgb * weight;
      total += weight;
    }
  }
  textureStore(output_gi, pixel, vec4<f32>(sum / max(total, 1e-5), center.a));
}
