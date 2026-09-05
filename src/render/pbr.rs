//! CPU reference routines shared by material generation and shader probes.

use glam::{Vec2, Vec3};

pub const POM_MAX_STEPS: usize = 16;
pub const POM_MAX_REFINE: usize = 3;
pub const POM_UV_CLAMP: f32 = 0.08;

pub fn oct_encode(mut n: Vec3) -> Vec2 {
    n = n.normalize_or_zero();
    let inv = 1.0 / (n.x.abs() + n.y.abs() + n.z.abs()).max(f32::EPSILON);
    let mut p = Vec2::new(n.x * inv, n.y * inv);
    if n.z < 0.0 {
        p = Vec2::new(
            (1.0 - p.y.abs()) * p.x.signum(),
            (1.0 - p.x.abs()) * p.y.signum(),
        );
    }
    p
}

pub fn oct_decode(p: Vec2) -> Vec3 {
    let mut n = Vec3::new(p.x, p.y, 1.0 - p.x.abs() - p.y.abs());
    if n.z < 0.0 {
        n = Vec3::new(
            (1.0 - n.y.abs()) * n.x.signum(),
            (1.0 - n.x.abs()) * n.y.signum(),
            n.z,
        );
    }
    n.normalize_or_zero()
}

pub fn tangent_basis(normal: Vec3) -> (Vec3, Vec3, Vec3) {
    let n = normal.normalize_or_zero();
    let up = if n.z.abs() < 0.999 { Vec3::Z } else { Vec3::Y };
    let t = up.cross(n).normalize_or_zero();
    (t, n.cross(t).normalize_or_zero(), n)
}

pub fn toroidal_sobel(height: &[f32], width: usize, x: usize, y: usize, strength: f32) -> Vec3 {
    assert_eq!(height.len(), width * width);
    let sample = |sx: isize, sy: isize| {
        let xx = (x as isize + sx).rem_euclid(width as isize) as usize;
        let yy = (y as isize + sy).rem_euclid(width as isize) as usize;
        height[yy * width + xx]
    };
    let dx = (sample(1, -1) + 2.0 * sample(1, 0) + sample(1, 1)
        - sample(-1, -1)
        - 2.0 * sample(-1, 0)
        - sample(-1, 1))
        / 8.0;
    let dy = (sample(-1, 1) + 2.0 * sample(0, 1) + sample(1, 1)
        - sample(-1, -1)
        - 2.0 * sample(0, -1)
        - sample(1, -1))
        / 8.0;
    Vec3::new(-dx * strength, -dy * strength, 1.0).normalize_or_zero()
}

pub fn pom_intersection<F: Fn(Vec2) -> f32>(
    uv: Vec2,
    view_ts: Vec3,
    height_scale: f32,
    steps: usize,
    refine: usize,
    sample: F,
) -> Vec2 {
    if view_ts.z.abs() < f32::EPSILON || steps == 0 {
        return uv;
    }
    let layers = steps.min(POM_MAX_STEPS) as f32;
    let delta = (view_ts.truncate() / view_ts.z.abs().max(0.20)) * height_scale / layers;
    let layer_depth = 1.0 / layers;
    let mut prev_uv = uv;
    let mut prev_depth = 0.0;
    let mut prev_surface = 1.0 - sample(prev_uv);
    for i in 1..=steps.min(POM_MAX_STEPS) {
        let current_uv = uv - delta * i as f32;
        let current_depth = i as f32 * layer_depth;
        let current_surface = 1.0 - sample(current_uv);
        if current_depth >= current_surface {
            let after = current_depth - current_surface;
            let before = (prev_surface - prev_depth).max(0.0);
            let weight = (before / (after + before).max(f32::EPSILON)).clamp(0.0, 1.0);
            let mut hit = prev_uv.lerp(current_uv, weight);
            let mut lo = prev_uv;
            let mut hi = current_uv;
            let mut lo_depth = prev_depth;
            let mut hi_depth = current_depth;
            for _ in 0..refine.min(POM_MAX_REFINE) {
                let mid = lo.lerp(hi, 0.5);
                let mid_depth = (lo_depth + hi_depth) * 0.5;
                if mid_depth >= 1.0 - sample(mid) {
                    hi = mid;
                    hi_depth = mid_depth;
                } else {
                    lo = mid;
                    lo_depth = mid_depth;
                }
                hit = lo.lerp(hi, 0.5);
            }
            return hit.clamp(
                uv - Vec2::splat(POM_UV_CLAMP),
                uv + Vec2::splat(POM_UV_CLAMP),
            );
        }
        prev_uv = current_uv;
        prev_depth = current_depth;
        prev_surface = current_surface;
    }
    uv
}

pub fn ggx_brdf(n: Vec3, v: Vec3, l: Vec3, albedo: Vec3, metallic: f32, roughness: f32) -> Vec3 {
    let n = n.normalize_or_zero();
    let v = v.normalize_or_zero();
    let l = l.normalize_or_zero();
    let h = (v + l).normalize_or_zero();
    let nv = n.dot(v).max(0.0);
    let nl = n.dot(l).max(0.0);
    let nh = n.dot(h).max(0.0);
    let vh = v.dot(h).max(0.0);
    let r = roughness.clamp(0.0, 1.0);
    let a = (r * r).max(0.0025);
    let a2 = a * a;
    let d = a2 / (std::f32::consts::PI * (nh * nh * (a2 - 1.0) + 1.0).powi(2));
    let k = (r + 1.0).powi(2) / 8.0;
    let g1 = |x: f32| x / (x * (1.0 - k) + k).max(f32::EPSILON);
    let f0 = Vec3::splat(0.04).lerp(albedo, metallic.clamp(0.0, 1.0));
    let f = f0 + (Vec3::ONE - f0) * (1.0 - vh).powi(5);
    let spec = f * (d * g1(nv) * g1(nl) / (4.0 * nv * nl).max(1e-4));
    let diffuse =
        (Vec3::ONE - f) * (1.0 - metallic.clamp(0.0, 1.0)) * albedo / std::f32::consts::PI;
    (diffuse + spec).max(Vec3::ZERO)
}

pub fn motion_without_jitter(current_no_jitter: Vec2, previous_no_jitter: Vec2) -> Vec2 {
    current_no_jitter - previous_no_jitter
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oct_roundtrip_error_is_sub_quarter_degree() {
        let mut max = 0.0_f32;
        for i in 0..4096 {
            let z = (i as f32 / 4095.0) * 2.0 - 1.0;
            let a = i as f32 * 0.61803395;
            let n = Vec3::new(a.cos(), a.sin(), z).normalize();
            max = max.max(n.angle_between(oct_decode(oct_encode(n))).to_degrees());
        }
        assert!(max < 0.25, "max oct error {max}°");
    }
    #[test]
    fn pom_reference_intersection_matches_shader_contract() {
        let hit = pom_intersection(
            Vec2::splat(0.5),
            Vec3::new(0.2, -0.1, 0.9),
            0.04,
            8,
            2,
            |uv| uv.x.clamp(0.0, 1.0),
        );
        // Golden result from the WGSL probe's fixed 8-step + 2-refine loop.
        // Keeping this independent constant catches interpolation/refinement
        // drift between the CPU reference and shader contract.
        let shader_probe = Vec2::new(0.495_416_67, 0.502_291_7);
        let error = (hit - shader_probe).abs().max_element();
        assert!(hit.is_finite());
        assert!(error <= 1.0 / 1024.0, "POM UV error {error}");
        assert!((hit - Vec2::splat(0.5)).abs().max_element() <= POM_UV_CLAMP);
        let shader = include_str!("../../assets/shaders/gbuffer.wgsl");
        assert!(shader.contains("clamp(globals.time_res.w, 1.0, 32.0)"));
        assert!(shader.contains("refine < 3"));
        assert!(shader.contains("vec2<f32>(0.08)"));
    }
    #[test]
    fn ggx_reference_is_finite_and_energy_bounded() {
        let n = Vec3::Z;
        let v = Vec3::Z;
        let mut energy = 0.0;
        for i in 0..128 {
            let theta = (i as f32 + 0.5) / 128.0 * std::f32::consts::FRAC_PI_2;
            let l = Vec3::new(theta.sin(), 0.0, theta.cos());
            let value = ggx_brdf(n, v, l, Vec3::splat(0.8), 0.0, 0.5);
            assert!(value.is_finite());
            energy += value.x * l.z * theta.sin();
        }
        energy *= std::f32::consts::PI / 128.0 * 2.0;
        assert!(energy <= 1.05, "energy={energy}");
    }
    #[test]
    fn motion_vectors_exclude_jitter() {
        assert_eq!(
            motion_without_jitter(Vec2::splat(0.4), Vec2::splat(0.4)),
            Vec2::ZERO
        );
    }
}
