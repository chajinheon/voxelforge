@group(0) @binding(0) var source_depth: texture_depth_2d;
@group(0) @binding(1) var destination: texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
  let size = textureDimensions(destination);
  if (any(id.xy >= size)) { return; }
  let depth = textureLoad(source_depth, vec2<i32>(id.xy), 0);
  let near = 0.05;
  let far = 1000.0;
  let linear = near * far / max(far - depth * (far - near), 1e-6);
  textureStore(destination, vec2<i32>(id.xy), vec4<f32>(linear, 0.0, 0.0, 0.0));
}
