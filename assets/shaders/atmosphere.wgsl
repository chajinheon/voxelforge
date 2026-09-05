// M7 atmospheric LUT chain. Transmittance is integrated first, multiscatter
// reads it (16 directions x 8 steps), and sky-view reads both LUTs.
struct AtmosphereUniform {
  ground_radius: f32, atmosphere_radius: f32, sun_mu: f32, phase: f32,
  beta_r: vec3<f32>, beta_m: vec3<f32>,
};
@group(0) @binding(0) var<uniform> atmosphere: AtmosphereUniform;
@group(0) @binding(1) var transmittance_tex: texture_2d<f32>;
@group(0) @binding(2) var multiscatter_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(3) var sky_view_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(4) var transmittance_out: texture_storage_2d<rgba16float, write>;
// Storage textures are write-only in WGSL. Bind the same multiscatter
// allocation as a sampled read view for the sky-view stage.
@group(0) @binding(5) var multiscatter_tex: texture_2d<f32>;
@group(0) @binding(6) var sky_view_tex: texture_2d<f32>;
@group(0) @binding(7) var sky_cube_out: texture_storage_2d_array<rgba16float, write>;
@group(0) @binding(8) var sky_cube_in: texture_2d_array<f32>;
const PI: f32 = 3.14159265359; const TAU: f32 = 6.28318530718;
fn safe_exp(value: f32) -> f32 { return exp(clamp(value, -80.0, 0.0)); }
fn transmittance_value(uv: vec2<f32>) -> vec4<f32> {
  let mu = uv.x * 2.0 - 1.0; let altitude = uv.y * uv.y;
  var optical = vec3<f32>(0.); let step_length = 100000.0 / 40.0;
  for (var step = 0u; step < 40u; step++) {
    let t = (f32(step) + .5) / 40.; let density = exp(-t * (1. - altitude) * 4.);
    optical += (atmosphere.beta_r + atmosphere.beta_m) * step_length * density * max(1. - abs(mu) * .35, .08);
  }
  return vec4(safe_exp(-optical.x), safe_exp(-optical.y), safe_exp(-optical.z), 1.);
}
@compute @workgroup_size(16, 16, 1)
fn transmittance_main(@builtin(global_invocation_id) id: vec3<u32>) {
  let size = textureDimensions(transmittance_out); if (id.x >= size.x || id.y >= size.y) { return; }
  let uv = (vec2<f32>(id.xy) + .5) / vec2<f32>(size); textureStore(transmittance_out, vec2<i32>(id.xy), transmittance_value(uv));
}
@compute @workgroup_size(16, 16, 1)
fn multiscatter_main(@builtin(global_invocation_id) id: vec3<u32>) {
  let size = textureDimensions(multiscatter_out); if (id.x >= size.x || id.y >= size.y) { return; }
  let uv = (vec2<f32>(id.xy) + .5) / vec2<f32>(size); let ts = vec2<i32>(textureDimensions(transmittance_tex));
  var integrated = vec3<f32>(0.);
  for (var direction = 0u; direction < 16u; direction++) {
    for (var step = 0u; step < 8u; step++) {
      let du = (f32(direction) + .5) / 16.; let sv = (f32(step) + .5) / 8.;
      let coord = clamp(vec2<i32>(vec2(du, uv.y * .65 + sv * .35) * vec2<f32>(ts)), vec2(0), ts - vec2(1));
      integrated += textureLoad(transmittance_tex, coord, 0).rgb;
    }
  }
  textureStore(multiscatter_out, vec2<i32>(id.xy), vec4(integrated / 128. * (0.5 + 0.5 * uv.x), 1.));
}
fn ray_sphere(origin: vec3<f32>, direction: vec3<f32>, radius: f32) -> f32 {
  let b = dot(origin, direction); let c = dot(origin, origin) - radius * radius; let discriminant = b * b - c;
  if (discriminant < 0.) { return -1.; } let root = sqrt(discriminant); let near = -b - root; let far = -b + root;
  return select(far, near, near > .001);
}
@compute @workgroup_size(8, 8, 1)
fn sky_view_main(@builtin(global_invocation_id) id: vec3<u32>) {
  let size = textureDimensions(sky_view_out); if (id.x >= size.x || id.y >= size.y) { return; }
  let uv = (vec2<f32>(id.xy) + .5) / vec2<f32>(size); let azimuth = (uv.x - .5) * TAU; let elevation = (.5 - uv.y) * PI;
  let direction = normalize(vec3(cos(elevation) * cos(azimuth), sin(elevation), cos(elevation) * sin(azimuth)));
  let origin = vec3(0., atmosphere.ground_radius + 1000., 0.); let atmo_hit = ray_sphere(origin, direction, atmosphere.atmosphere_radius);
  let ground_hit = ray_sphere(origin, direction, atmosphere.ground_radius); let segment = select(atmo_hit, min(atmo_hit, ground_hit), ground_hit > .001);
  var view_transmittance = vec3(1.); var radiance = vec3(0.); let step_length = max(segment, 0.) / 24.;
  for (var view_step = 0u; view_step < 24u; view_step++) {
    let distance = (f32(view_step) + .5) * step_length; let point = origin + direction * distance;
    let height = clamp((length(point) - atmosphere.ground_radius) / (atmosphere.atmosphere_radius - atmosphere.ground_radius), 0., 1.);
    let mu = dot(direction, normalize(point)); let trans_uv = vec2(clamp(mu * .5 + .5, 0., 1.), sqrt(height));
    let ts = vec2<i32>(textureDimensions(transmittance_tex)); let trans_coord = clamp(vec2<i32>(trans_uv * vec2<f32>(ts)), vec2(0), ts - vec2(1));
    let sun_elevation = clamp(atmosphere.sun_mu, -1., 1.); var sun_transmittance = vec3(0.);
    for (var sun_step = 0u; sun_step < 8u; sun_step++) {
      let sun_mu = mix(-1., 1., (f32(sun_step) + .5) / 8.); let sun_uv = vec2(clamp(sun_mu * .5 + .5, 0., 1.), sqrt(height));
      sun_transmittance += textureLoad(transmittance_tex, clamp(vec2<i32>(sun_uv * vec2<f32>(ts)), vec2(0), ts - vec2(1)), 0).rgb;
    }
    sun_transmittance /= 8.; let ms = vec2<i32>(textureDimensions(multiscatter_tex)); let multi_uv = vec2(clamp(sun_elevation * .5 + .5, 0., 1.), sqrt(height));
    let multi = textureLoad(multiscatter_tex, clamp(vec2<i32>(multi_uv * vec2<f32>(ms)), vec2(0), ms - vec2(1)), 0).rgb;
    let density = exp(-height * 4.); let rayleigh = atmosphere.beta_r * density; let mie = atmosphere.beta_m * density;
    let single_scatter = rayleigh * (0.75 + 0.25 * sun_elevation) + mie * (0.5 + 0.5 * sun_elevation);
    let merged = single_scatter * sun_transmittance + multi * (vec3(1.) - sun_transmittance);
    radiance += view_transmittance * merged * step_length * 1000.; view_transmittance *= textureLoad(transmittance_tex, trans_coord, 0).rgb;
  }
  textureStore(sky_view_out, vec2<i32>(id.xy), vec4(max(radiance, vec3(0.)), 1.));
}
fn cube_direction(face: u32, uv: vec2<f32>) -> vec3<f32> {
  let p = uv * 2. - 1.;
  switch face {
    case 0u: { return normalize(vec3(1., -p.y, -p.x)); }
    case 1u: { return normalize(vec3(-1., -p.y, p.x)); }
    case 2u: { return normalize(vec3(p.x, 1., p.y)); }
    case 3u: { return normalize(vec3(p.x, -1., -p.y)); }
    case 4u: { return normalize(vec3(p.x, -p.y, 1.)); }
    default: { return normalize(vec3(-p.x, -p.y, -1.)); }
  }
}
@compute @workgroup_size(8, 8, 1)
fn cube_main(@builtin(global_invocation_id) id: vec3<u32>) {
  let size = textureDimensions(sky_cube_out); if (id.x >= size.x || id.y >= size.y || id.z >= 6u) { return; }
  let direction = cube_direction(id.z, (vec2<f32>(id.xy) + .5) / vec2<f32>(size.xy));
  let uv = vec2(atan2(direction.z, direction.x) / TAU + .5, .5 - asin(clamp(direction.y, -1., 1.)) / PI);
  let sky_size = vec2<i32>(textureDimensions(sky_view_tex));
  let sample_center = uv * vec2<f32>(sky_size) - vec2<f32>(0.5);
  let base = vec2<i32>(floor(sample_center));
  let max_coord = sky_size - vec2<i32>(1);
  let footprint = max(sky_size / vec2<i32>(size.xy), vec2<i32>(1));
  var box = vec4<f32>(0.);
  for (var oy = 0; oy < 2; oy++) {
    for (var ox = 0; ox < 2; ox++) {
      box += textureLoad(sky_view_tex, clamp(base + vec2<i32>(ox, oy) * footprint, vec2<i32>(0), max_coord), 0);
    }
  }
  textureStore(sky_cube_out, vec2<i32>(id.xy), i32(id.z), box * 0.25);
}
@compute @workgroup_size(8, 8, 1)
fn cube_downsample_main(@builtin(global_invocation_id) id: vec3<u32>) {
  let size = textureDimensions(sky_cube_out);
  if (id.x >= size.x || id.y >= size.y || id.z >= 6u) { return; }
  let source_size = vec2<i32>(textureDimensions(sky_cube_in));
  let base = vec2<i32>(id.xy) * 2;
  var color = vec4<f32>(0.0);
  for (var y = 0; y < 2; y++) {
    for (var x = 0; x < 2; x++) {
      color += textureLoad(sky_cube_in, min(base + vec2<i32>(x, y), source_size - vec2<i32>(1)), i32(id.z), 0);
    }
  }
  textureStore(sky_cube_out, vec2<i32>(id.xy), i32(id.z), color * 0.25);
}
