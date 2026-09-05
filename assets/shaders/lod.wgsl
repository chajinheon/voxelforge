struct Globals { view_proj: mat4x4<f32>, cam_pos: vec4<f32>, sun_dir: vec4<f32>, time_res: vec4<f32>, sky_color: vec4<f32> };
@group(0) @binding(0) var<uniform> globals: Globals;
struct In { @location(0) position: vec3<f32>, @location(1) block: u32, @location(2) light: f32, @location(3) level: u32 };
struct Out { @builtin(position) clip: vec4<f32>, @location(0) @interpolate(flat) block: u32, @location(1) light: f32, @location(2) @interpolate(flat) level: u32, @location(3) world: vec3<f32> };
@vertex fn vs_main(input: In) -> Out { var out: Out; out.clip = globals.view_proj * vec4(input.position, 1.0); out.block = input.block; out.light = input.light; out.level = input.level; out.world = input.position; return out; }
fn bayer8(p: vec2<i32>) -> f32 {
  let matrix = array<f32, 64>(
    0., 48., 12., 60., 3., 51., 15., 63.,
    32., 16., 44., 28., 35., 19., 47., 31.,
    8., 56., 4., 52., 11., 59., 7., 55.,
    40., 24., 36., 20., 43., 27., 39., 23.,
    2., 50., 14., 62., 1., 49., 13., 61.,
    34., 18., 46., 30., 33., 17., 45., 29.,
    10., 58., 6., 54., 9., 57., 5., 53.,
    42., 26., 38., 22., 41., 25., 37., 21.);
  return (matrix[(p.y & 7) * 8 + (p.x & 7)] + 0.5) / 64.0;
}
@fragment fn fs_main(input: Out) -> @location(0) vec4<f32> {
  let lod_palette = array<vec3<f32>, 4>(vec3(0.16,0.82,0.28), vec3(0.20,0.48,0.95), vec3(0.86,0.28,0.82), vec3(0.98,0.62,0.16));
  let block_palette = array<vec3<f32>, 6>(vec3(0.42,0.42,0.45), vec3(0.30,0.52,0.25), vec3(0.55,0.36,0.18), vec3(0.75,0.70,0.40), vec3(0.25,0.55,0.75), vec3(0.60,0.60,0.62));
  // Material colours are the normal path.  The palette is intentionally
  // debug-only so distant terrain does not advertise its implementation LOD.
  let DEBUG_LOD = globals.time_res.w < 0.0;
  let material = block_palette[input.block % 6u];
  let debug = lod_palette[input.level % 4u];
  let c = select(material, debug, DEBUG_LOD) * (0.35 + 0.65 * input.light);
  let d = distance(input.world.xz, globals.cam_pos.xz);
  let t = bayer8(vec2<i32>(floor(input.clip.xy)));
  var keep = true;
  if (input.level == 1u && d >= 288.0 && d < 320.0) { keep = t < (d - 288.0) / 32.0; }
  else if (input.level == 1u && d >= 608.0 && d < 640.0) { keep = t >= (d - 608.0) / 32.0; }
  else if (input.level == 2u && d >= 608.0 && d < 640.0) { keep = t < (d - 608.0) / 32.0; }
  else if (input.level == 2u && d >= 928.0 && d < 960.0) { keep = t >= (d - 928.0) / 32.0; }
  else if (input.level == 3u && d >= 928.0 && d < 960.0) { keep = t < (d - 928.0) / 32.0; }
  if (!keep) { discard; }
  return vec4(c, 1.0);
}
