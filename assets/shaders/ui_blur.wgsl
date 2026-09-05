@group(0) @binding(0) var source_tex: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

struct BlurParams {
  texel_step: vec2<f32>,
  _padding: vec2<f32>,
};
@group(0) @binding(2) var<uniform> params: BlurParams;

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

@fragment
fn fs_blur(input: VertexOut) -> @location(0) vec4<f32> {
  let weights = array<f32, 5>(0.204164, 0.180174, 0.123832, 0.066282, 0.027631);
  var color = textureSampleLevel(source_tex, source_sampler, input.uv, 0.0) * weights[0];
  for (var tap = 1u; tap <= 4u; tap++) {
    let offset = params.texel_step * f32(tap);
    color += textureSampleLevel(source_tex, source_sampler, input.uv + offset, 0.0) * weights[tap];
    color += textureSampleLevel(source_tex, source_sampler, input.uv - offset, 0.0) * weights[tap];
  }
  return vec4<f32>(color.rgb, 1.0);
}

@fragment
fn fs_copy(input: VertexOut) -> @location(0) vec4<f32> {
  return vec4<f32>(textureSampleLevel(source_tex, source_sampler, input.uv, 0.0).rgb, 1.0);
}
