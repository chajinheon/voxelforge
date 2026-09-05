@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
struct FinalUniform { inv_source_size: vec2<f32>, exposure: f32, sharpen: f32, };
@group(0) @binding(2) var<uniform> final_uniform: FinalUniform;
struct VertexOutput { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, };
@vertex fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
  var positions = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
  var out: VertexOutput; out.position = vec4<f32>(positions[index], 0.0, 1.0); out.uv = positions[index] * 0.5 + vec2<f32>(0.5); out.uv.y = 1.0 - out.uv.y; return out;
}
fn aces_fitted(color: vec3<f32>) -> vec3<f32> {
  let a = 2.51; let b = 0.03; let c = 2.43; let d = 0.59; let e = 0.14;
  return clamp((color * (a * color + vec3<f32>(b))) / max(color * (c * color + vec3<f32>(d)) + vec3<f32>(e), vec3<f32>(1e-4)), vec3<f32>(0.0), vec3<f32>(1.0));
}
fn grade(color: vec3<f32>) -> vec3<f32> {
  var graded = (color - vec3<f32>(0.18)) * 1.06 + vec3<f32>(0.18);
  let luminance = dot(graded, vec3<f32>(0.2126, 0.7152, 0.0722));
  graded = mix(vec3<f32>(luminance), graded, 1.04);
  return max((graded - vec3<f32>(0.003)) * 1.01, vec3<f32>(0.0));
}
fn tone(c: vec3<f32>, exposure: f32) -> vec3<f32> {
  return grade(aces_fitted(max(c * exposure, vec3<f32>(0.0))));
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
  let texel = final_uniform.inv_source_size;
  let c = tone(textureSample(source, source_sampler, input.uv).rgb, final_uniform.exposure);
  let n = tone(textureSample(source, source_sampler, input.uv + vec2<f32>(0.0, -texel.y)).rgb, final_uniform.exposure);
  let s = tone(textureSample(source, source_sampler, input.uv + vec2<f32>(0.0, texel.y)).rgb, final_uniform.exposure);
  let e = tone(textureSample(source, source_sampler, input.uv + vec2<f32>(texel.x, 0.0)).rgb, final_uniform.exposure);
  let w = tone(textureSample(source, source_sampler, input.uv + vec2<f32>(-texel.x, 0.0)).rgb, final_uniform.exposure);
  let blur = (n + s + e + w + 4.0 * c) / 8.0;
  let lo = min(c, min(min(n, s), min(e, w)));
  let hi = max(c, max(max(n, s), max(e, w)));
  return vec4<f32>(clamp(c + final_uniform.sharpen * (c - blur), lo, hi), 1.0);
}
