@group(0) @binding(0) var icons: texture_2d_array<f32>;
@group(0) @binding(1) var icon_sampler: sampler;
@group(1) @binding(0) var hand_texture: texture_2d<f32>;
@group(1) @binding(1) var hand_sampler: sampler;

struct Params {
  model_view_projection: mat4x4<f32>,
  normal_matrix: mat4x4<f32>,
  color: vec4<f32>,
  material: vec4<f32>, // roughness, metallic, alpha, reserved
  lighting: vec4<f32>, // sky curve, block curve, sun factor, reserved
  sky_color: vec4<f32>,
  sun_dir: vec4<f32>,
  emission: vec4<f32>,
}
@group(2) @binding(0) var<uniform> params: Params;

struct VertexOut {
  @builtin(position) position: vec4<f32>,
  @location(0) normal: vec3<f32>,
  @location(1) uv: vec2<f32>,
  @location(2) color: vec4<f32>,
  @location(3) @interpolate(flat) layer: i32,
};

@vertex
fn vs_main(
  @location(0) position: vec3<f32>,
  @location(1) normal: vec3<f32>,
  @location(2) uv: vec2<f32>,
  @location(3) color: vec4<f32>,
  @location(4) layer: f32,
) -> VertexOut {
  var out: VertexOut;
  out.position = params.model_view_projection * vec4<f32>(position, 1.0);
  out.normal = normalize((params.normal_matrix * vec4<f32>(normal, 0.0)).xyz);
  out.uv = uv;
  out.color = color * params.color;
  out.layer = i32(round(layer));
  return out;
}

fn aces(color: vec3<f32>) -> vec3<f32> {
  let x = max(color, vec3<f32>(0.0));
  return (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14);
}

fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
  let a = roughness * roughness;
  let a2 = a * a;
  let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
  return a2 / max(3.14159265 * d * d, 0.0001);
}

fn geometry_schlick(n_dot: f32, roughness: f32) -> f32 {
  let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
  return n_dot / max(n_dot * (1.0 - k) + k, 0.0001);
}

fn fresnel_schlick(cosine: f32, f0: vec3<f32>) -> vec3<f32> {
  return f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - cosine, 5.0);
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
  var base = input.color;
  if (input.layer < -1) {
    base *= textureSampleLevel(hand_texture, hand_sampler, input.uv, 0.0);
  }
  let normal = normalize(input.normal);
  let view = normalize(vec3<f32>(0.0, 0.0, 1.0));
  let light = normalize(-params.sun_dir.xyz);
  let halfway = normalize(view + light);
  let n_dot_l = max(dot(normal, light), 0.0);
  let n_dot_v = max(dot(normal, view), 0.0);
  let n_dot_h = max(dot(normal, halfway), 0.0);
  let v_dot_h = max(dot(view, halfway), 0.0);
  let roughness = clamp(params.material.x, 0.04, 1.0);
  let metallic = clamp(params.material.y, 0.0, 1.0);
  let f0 = mix(vec3<f32>(0.04), base.rgb, metallic);
  let fresnel = fresnel_schlick(v_dot_h, f0);
  let specular = distribution_ggx(n_dot_h, roughness) * geometry_schlick(n_dot_v, roughness) * geometry_schlick(n_dot_l, roughness) * fresnel / max(4.0 * n_dot_v * n_dot_l, 0.0001);
  let diffuse = (vec3<f32>(1.0) - fresnel) * (1.0 - metallic) * base.rgb / 3.14159265;
  let sky_curve = params.lighting.x;
  let block_curve = params.lighting.y;
  let ambient = max(
    params.sky_color.rgb * (0.18 + 0.32 * sky_curve)
      + vec3<f32>(1.0, 0.58, 0.28) * (0.08 + 0.28 * block_curve),
    vec3<f32>(0.06),
  );
  let direct = (diffuse + specular) * n_dot_l * params.sky_color.rgb * 1.6 * sky_curve;
  let lit = base.rgb * ambient + direct + params.emission.rgb;
  let exposure_one = lit;
  return vec4<f32>(aces(exposure_one), base.a * params.material.z);
}
