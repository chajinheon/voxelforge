struct Globals { view_proj: mat4x4<f32>, cam_pos: vec4<f32>, sun_dir: vec4<f32>, time_res: vec4<f32>, sky_color: vec4<f32>, };
struct ChunkUniform { origin: vec4<i32>, };
struct MaterialGpu { base: vec4<f32>, tint: vec4<f32>, flags: vec4<u32>, };
@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var albedo: texture_2d_array<f32>;
@group(1) @binding(1) var material: texture_2d_array<f32>;
@group(1) @binding(2) var emission: texture_2d_array<f32>;
@group(1) @binding(3) var samp: sampler;
@group(1) @binding(4) var<storage, read> materials: array<MaterialGpu>;
@group(2) @binding(0) var<uniform> chunk: ChunkUniform;
struct ShadowUniform { light_view_proj: array<mat4x4<f32>, 3>, splits: vec4<f32>, };
@group(3) @binding(0) var<uniform> shadow: ShadowUniform;
struct In { @location(0) packed: vec2<u32> };
struct Out { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) @interpolate(flat) layer: u32 };
@vertex fn vs_main(input: In, @builtin(instance_index) instance: u32) -> Out {
  let x = input.packed.x & 63u; let y = (input.packed.x >> 6u) & 63u; let z = (input.packed.x >> 12u) & 63u;
  let face = (input.packed.x >> 18u) & 7u; let layer = input.packed.y & 65535u;
  let frac = vec3<f32>(f32((input.packed.x >> 24u) & 15u), f32((input.packed.x >> 28u) & 15u), f32((input.packed.y >> 24u) & 15u)) / 16.0;
  let local = vec3<f32>(f32(x), f32(y), f32(z)) + frac;
  let world = vec3<f32>(chunk.origin.xyz) + local;
  var out: Out; out.position = shadow.light_view_proj[min(instance, 2u)] * vec4<f32>(world, 1.0); out.layer = layer;
  out.uv = select(vec2<f32>(local.x, local.z), vec2<f32>(local.z, -local.y), face == 0u || face == 1u); return out;
}
@fragment fn fs_main(input: Out) {
  let layer = min(input.layer, arrayLength(&materials) - 1u);
  let gpu = materials[layer];
  if ((gpu.flags.x & 2u) != 0u && textureSampleLevel(albedo, samp, input.uv, i32(layer), 0.0).a < gpu.tint.a) { discard; }
}
