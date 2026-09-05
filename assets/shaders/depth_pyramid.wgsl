// M8 linear-depth min pyramid. Dispatch one invocation per destination texel.
struct DepthPyramidParams {
    source_size: vec2<u32>,
    destination_size: vec2<u32>,
    source_mip: u32,
    _padding: vec3<u32>,
};
@group(0) @binding(0) var source_depth: texture_2d<f32>;
@group(0) @binding(1) var destination: texture_storage_2d<r32float, write>;
@group(0) @binding(2) var<uniform> params: DepthPyramidParams;

@compute @workgroup_size(16, 16, 1)
fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.destination_size.x || id.y >= params.destination_size.y) { return; }
    let origin = vec2<i32>(id.xy * 2u);
    let source_size = vec2<i32>(params.source_size);
    let p0 = min(origin, source_size - vec2<i32>(1));
    let p1 = min(origin + vec2<i32>(1, 0), source_size - vec2<i32>(1));
    let p2 = min(origin + vec2<i32>(0, 1), source_size - vec2<i32>(1));
    let p3 = min(origin + vec2<i32>(1, 1), source_size - vec2<i32>(1));
    let value = min(min(textureLoad(source_depth, p0, i32(params.source_mip)).r, textureLoad(source_depth, p1, i32(params.source_mip)).r), min(textureLoad(source_depth, p2, i32(params.source_mip)).r, textureLoad(source_depth, p3, i32(params.source_mip)).r));
    textureStore(destination, vec2<i32>(id.xy), vec4<f32>(max(value, 0.0), 0.0, 0.0, 1.0));
}
