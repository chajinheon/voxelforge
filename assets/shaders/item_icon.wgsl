// Fixed-layer icon sampler used by the M10 native icon atlas.
@group(0) @binding(0) var icons: texture_2d_array<f32>;
@group(0) @binding(1) var icon_sampler: sampler;

struct IconVertex {
  @builtin(position) position: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) @interpolate(flat) layer: u32,
};

@fragment
fn fs_main(input: IconVertex) -> @location(0) vec4<f32> {
  return textureSampleLevel(icons, icon_sampler, input.uv, input.layer, 0.0);
}
