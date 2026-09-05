@group(0) @binding(0) var icons: texture_2d_array<f32>;
@group(0) @binding(1) var icon_sampler: sampler;

struct VertexOutput {
  @builtin(position) position: vec4<f32>,
  @location(0) color: vec4<f32>,
  @location(1) uv: vec2<f32>,
  @location(2) layer: f32,
};

@vertex
fn vs_main(@location(0) position: vec2<f32>, @location(1) color: vec4<f32>, @location(2) uv: vec2<f32>, @location(3) layer: f32) -> VertexOutput {
  var out: VertexOutput;
  out.position = vec4<f32>(position, 0.0, 1.0);
  out.color = color;
  out.uv = uv;
  out.layer = layer;
  return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
  if (in.layer >= 0.0) { return textureSampleLevel(icons, icon_sampler, in.uv, u32(in.layer), 0.0) * in.color; }
  return in.color;
}
