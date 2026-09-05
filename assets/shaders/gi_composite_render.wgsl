// Composite the quarter-resolution GPU GI result into the deferred HDR scene.
// The scene and GI textures are deliberately separate bindings so a GI debug
// view cannot replace the lighting buffer by accident.
struct Params {
  time: f32,
  water: f32,
  clouds: f32,
  volumetric: f32,
  gi: f32,
  gi_mode: f32,
  exposure: f32,
  debug_view: f32,
};

@group(0) @binding(0) var scene_tex: texture_2d<f32>;
@group(0) @binding(1) var gi_tex: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;
@group(0) @binding(3) var<uniform> params: Params;

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
  var output: VertexOut;
  output.position = vec4<f32>(positions[index], 0.0, 1.0);
  output.uv = vec2<f32>(positions[index].x * 0.5 + 0.5, 0.5 - positions[index].y * 0.5);
  return output;
}

@fragment
fn gi_composite(input: VertexOut) -> @location(0) vec4<f32> {
  let scene = textureSampleLevel(scene_tex, linear_sampler, input.uv, 0.0).rgb;
  let gi = textureSampleLevel(gi_tex, linear_sampler, input.uv, 0.0).rgb;
  if (params.gi_mode < 0.5) {
    return vec4<f32>(scene, 1.0);
  }
  if (params.debug_view == 5.0 || params.debug_view == 6.0) {
    return vec4<f32>(max(gi, vec3<f32>(0.0)), 1.0);
  }
  return vec4<f32>(max(scene + gi, vec3<f32>(0.0)), 1.0);
}
