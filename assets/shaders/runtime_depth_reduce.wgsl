@group(0) @binding(0) var source_depth: texture_2d<f32>;
@group(0) @binding(1) var destination: texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
  let size = textureDimensions(destination);
  if (any(id.xy >= size)) { return; }
  let source_size = vec2<i32>(textureDimensions(source_depth));
  let base = vec2<i32>(id.xy) * 2;
  let a = textureLoad(source_depth, min(base, source_size - 1), 0).r;
  let b = textureLoad(source_depth, min(base + vec2<i32>(1, 0), source_size - 1), 0).r;
  let c = textureLoad(source_depth, min(base + vec2<i32>(0, 1), source_size - 1), 0).r;
  let d = textureLoad(source_depth, min(base + vec2<i32>(1, 1), source_size - 1), 0).r;
  textureStore(destination, vec2<i32>(id.xy), vec4<f32>(min(min(a, b), min(c, d)), 0.0, 0.0, 0.0));
}
