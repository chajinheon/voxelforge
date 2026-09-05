// Add denoised indirect radiance to the deferred HDR buffer before translucency.
@group(0) @binding(0) var hdr_in: texture_2d<f32>;
@group(0) @binding(1) var gi_in: texture_2d<f32>;
@group(0) @binding(2) var albedo_in: texture_2d<f32>;
@group(0) @binding(3) var ssao_in: texture_2d<f32>;
@group(1) @binding(0) var hdr_out: texture_storage_2d<rgba16float, write>;
@group(1) @binding(1) var<uniform> intensity: f32;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let size = textureDimensions(hdr_out);
  if (gid.x >= size.x || gid.y >= size.y) { return; }
  let pixel = vec2<i32>(gid.xy);
  let hdr = textureLoad(hdr_in, pixel, 0);
  let gi = textureLoad(gi_in, vec2<i32>(gid.xy / 4u), 0).rgb;
  let albedo = textureLoad(albedo_in, pixel, 0).rgb;
  let ssao = textureLoad(ssao_in, pixel, 0).r;
  let indirect = albedo * gi * intensity * mix(1.0, ssao, 0.35);
  textureStore(hdr_out, pixel, vec4<f32>(max(hdr.rgb + indirect, vec3<f32>(0.0)), hdr.a));
}
