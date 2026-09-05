// Composed by src/render/shader_source.rs. Keep this file as the canonical pass contract.
const PI: f32 = 3.14159265359;
fn saturate(x: f32) -> f32 { return clamp(x, 0.0, 1.0); }
fn oct_decode(p: vec2<f32>) -> vec3<f32> {
  var n = vec3<f32>(p, 1.0 - abs(p.x) - abs(p.y));
  if (n.z < 0.0) { n = vec3<f32>((1.0-abs(n.y))*sign(n.x), (1.0-abs(n.x))*sign(n.y), n.z); }
  return normalize(n);
}
fn ggx(n_dot_h: f32, roughness: f32) -> f32 {
  let a = max(roughness * roughness, 0.0025);
  let a2 = a * a;
  let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
  return a2 / max(PI * d * d, 1e-4);
}
