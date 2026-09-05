// Dedicated native 128px -> 64px icon reduction.  Each pass targets one
// array-layer view, avoiding format-incompatible texture copies.
struct QuadOut { @builtin(position) position: vec4<f32> }
@group(0) @binding(0) var native_icon: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vertex: u32) -> QuadOut {
  var positions = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
  var out: QuadOut;
  out.position = vec4(positions[vertex], 0.0, 1.0);
  return out;
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
  let base = vec2<i32>(floor(position.xy)) * 2;
  return (textureLoad(native_icon, base, 0)
    + textureLoad(native_icon, base + vec2<i32>(1, 0), 0)
    + textureLoad(native_icon, base + vec2<i32>(0, 1), 0)
    + textureLoad(native_icon, base + vec2<i32>(1, 1), 0)) * 0.25;
}
