//! ACES fitted tone mapping, grading, and native LDR sharpen reference.

use glam::Vec3;

pub fn aces_fitted(color: Vec3) -> Vec3 {
    let color = color.max(Vec3::ZERO);
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    ((color * (a * color + Vec3::splat(b)))
        / (color * (c * color + Vec3::splat(d)) + Vec3::splat(e)))
    .clamp(Vec3::ZERO, Vec3::ONE)
}

pub fn grade_aces(color: Vec3) -> Vec3 {
    let mapped = aces_fitted(color);
    let contrasted = (mapped - Vec3::splat(0.18)) * 1.06 + Vec3::splat(0.18);
    let luma = contrasted.dot(Vec3::new(0.2126, 0.7152, 0.0722));
    let saturated = Vec3::splat(luma).lerp(contrasted, 1.04);
    ((saturated + Vec3::splat(-0.003)) * 1.01).clamp(Vec3::ZERO, Vec3::ONE)
}

pub fn sharpen_5_tap(
    center: Vec3,
    north: Vec3,
    south: Vec3,
    east: Vec3,
    west: Vec3,
    amount: f32,
) -> Vec3 {
    let blur = (north + south + east + west + center * 4.0) / 8.0;
    (center + (center - blur) * amount).clamp(
        north.min(south).min(east).min(west).min(center),
        north.max(south).max(east).max(west).max(center),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aces_fit_matches_reference_values() {
        for (input, expected) in [
            (0.0, 0.0),
            (0.18, 0.266899),
            (1.0, 0.803797),
            (4.0, 0.973417),
        ] {
            let result = aces_fitted(Vec3::splat(input)).x;
            assert!(
                (result - expected).abs() < 1.0e-5,
                "{input}: {result} != {expected}"
            );
        }
    }
}
