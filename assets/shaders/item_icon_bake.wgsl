// Native item bake target. Geometry is supplied by the canonical ShapeTemplate
// cache; this pass deliberately uses a fixed orthographic camera and three
// point-light weights so the reduced atlas remains representative at any UI
// scale. The output is linear RGBA16F and is later reduced 2x2 into the atlas.
struct BakeParams {
  model: mat4x4<f32>,
  color: vec4<f32>,
  emission: vec4<f32>,
}
@group(0) @binding(0) var<uniform> params: BakeParams;

struct VertexOut { @builtin(position) position: vec4<f32>, @location(0) color: vec4<f32>, };

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>) -> VertexOut {
  var out: VertexOut;
  let world = (params.model * vec4<f32>(position, 1.0)).xyz;
  let key = max(dot(normalize(normal), normalize(vec3<f32>(-0.4, -1.0, -0.3))), 0.0) * 2.5;
  let fill = vec3<f32>(0.22, 0.27, 0.34);
  out.position = vec4<f32>(world.xy, world.z, 1.0);
  let linear = params.color.rgb * (vec3<f32>(0.08) + fill + key) + params.emission.rgb;
  let x = max(linear, vec3<f32>(0.0));
  let mapped = (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14);
  out.color = vec4<f32>(mapped, params.color.a);
  return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> { return input.color; }
