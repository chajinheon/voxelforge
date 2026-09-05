struct TonemapParams { exposure: f32, sharpen_amount: f32, inv_size: vec2<f32>, };
@group(0) @binding(0) var hdr: texture_2d<f32>;
@group(0) @binding(1) var bloom: texture_2d<f32>;
@group(0) @binding(2) var tex_sampler: sampler;
@group(0) @binding(3) var<uniform> params: TonemapParams;
struct VertexOutput { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, };
@vertex fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
  var positions = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
  var out: VertexOutput; out.position = vec4<f32>(positions[index], 0.0, 1.0); out.uv = positions[index] * 0.5 + vec2<f32>(0.5); out.uv.y = 1.0 - out.uv.y; return out;
}
fn aces(color: vec3<f32>) -> vec3<f32> { let x = max(color, vec3<f32>(0.0)); return clamp((x * (2.51 * x + vec3<f32>(0.03))) / (x * (2.43 * x + vec3<f32>(0.59)) + vec3<f32>(0.14)), vec3<f32>(0.0), vec3<f32>(1.0)); }
fn grade(color: vec3<f32>) -> vec3<f32> { let contrasted = (color - vec3<f32>(0.18)) * 1.06 + vec3<f32>(0.18); let y = dot(contrasted, vec3<f32>(0.2126, 0.7152, 0.0722)); return clamp((mix(vec3<f32>(y), contrasted, 1.04) - vec3<f32>(0.003)) * 1.01, vec3<f32>(0.0), vec3<f32>(1.0)); }
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
  let scene = textureSample(hdr, tex_sampler, input.uv).rgb * params.exposure + textureSample(bloom, tex_sampler, input.uv).rgb;
  let center = grade(aces(scene));
  let north = grade(aces(textureSample(hdr, tex_sampler, input.uv + vec2<f32>(0.0, -params.inv_size.y)).rgb * params.exposure));
  let south = grade(aces(textureSample(hdr, tex_sampler, input.uv + vec2<f32>(0.0, params.inv_size.y)).rgb * params.exposure));
  let east = grade(aces(textureSample(hdr, tex_sampler, input.uv + vec2<f32>(params.inv_size.x, 0.0)).rgb * params.exposure));
  let west = grade(aces(textureSample(hdr, tex_sampler, input.uv + vec2<f32>(-params.inv_size.x, 0.0)).rgb * params.exposure));
  let blur = (north + south + east + west + center * 4.0) / 8.0;
  let sharpened = clamp(center + params.sharpen_amount * (center - blur), min(min(north, south), min(east, min(west, center))), max(max(north, south), max(east, max(west, center))));
  return vec4<f32>(sharpened, 1.0);
}
