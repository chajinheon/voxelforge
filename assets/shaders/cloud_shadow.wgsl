// M8 snapped 512² world-space cloud shadow map, updated on an 8-frame cadence.
struct CloudShadowParams { origin: vec2<f32>, world_size: f32, sun_height: f32, time: f32, _padding: vec3<f32>, };
@group(0) @binding(0) var<uniform> params: CloudShadowParams;
@group(1) @binding(0) var base_noise: texture_3d<f32>;
@group(1) @binding(1) var detail_noise: texture_3d<f32>;
@group(1) @binding(2) var noise_sampler: sampler;
@group(1) @binding(3) var shadow_output: texture_storage_2d<r8unorm, write>;

@compute @workgroup_size(16, 16, 1)
fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= 512u || id.y >= 512u) { return; }
    let world = params.origin + (vec2<f32>(id.xy) / 512.0 - 0.5) * params.world_size;
    var transmittance = 1.0;
    for (var i: u32 = 0u; i < 12u; i++) {
        let height = 180.0 + (f32(i) + 0.5) / 12.0 * 80.0;
        let uvw = vec3<f32>(world / 128.0, height / 128.0);
        let base = textureSampleLevel(base_noise, noise_sampler, uvw, 0.0).r;
        let detail = textureSampleLevel(detail_noise, noise_sampler, uvw * vec3<f32>(4.0, 2.0, 4.0), 0.0).r;
        let density = clamp((base - 0.288 - detail * 0.25) * 2.0, 0.0, 1.0);
        transmittance *= exp(-density * 0.12);
    }
    textureStore(shadow_output, vec2<i32>(id.xy), vec4<f32>(transmittance, 0.0, 0.0, 1.0));
}

struct ShadowVertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, };
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> ShadowVertex {
    let positions = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    var out: ShadowVertex;
    out.position = vec4<f32>(positions[index], 0.0, 1.0);
    out.uv = positions[index] * 0.5 + vec2<f32>(0.5);
    return out;
}

@fragment
fn fs_main(input: ShadowVertex) -> @location(0) vec4<f32> {
    let world = params.origin + (input.uv - vec2<f32>(0.5)) * params.world_size;
    var transmittance = 1.0;
    for (var i: u32 = 0u; i < 12u; i++) {
        let height = 180.0 + (f32(i) + 0.5) / 12.0 * 80.0;
        let uvw = vec3<f32>(world / 128.0, height / 128.0);
        let base = textureSampleLevel(base_noise, noise_sampler, uvw, 0.0).r;
        let detail = textureSampleLevel(detail_noise, noise_sampler, uvw * vec3<f32>(4.0, 2.0, 4.0), 0.0).r;
        let density = clamp((base - 0.288 - detail * 0.25) * 2.0, 0.0, 1.0);
        transmittance *= exp(-density * 0.12);
    }
    return vec4<f32>(transmittance, 0.0, 0.0, 1.0);
}
