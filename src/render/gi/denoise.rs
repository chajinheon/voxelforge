use glam::Vec3;

const KERNEL: [f32; 5] = [1.0, 4.0, 6.0, 4.0, 1.0];

#[derive(Clone, Debug, PartialEq)]
pub struct AtrousImage {
    pub width: usize,
    pub height: usize,
    pub radiance: Vec<Vec3>,
    pub depth: Vec<f32>,
    pub normals: Vec<Vec3>,
    pub variance: Vec<f32>,
}

impl AtrousImage {
    pub fn new(
        width: usize,
        height: usize,
        radiance: Vec<Vec3>,
        depth: Vec<f32>,
        normals: Vec<Vec3>,
    ) -> Self {
        assert_eq!(radiance.len(), width * height);
        assert_eq!(depth.len(), radiance.len());
        assert_eq!(normals.len(), radiance.len());
        Self {
            width,
            height,
            radiance,
            depth,
            normals,
            variance: vec![0.0; width * height],
        }
    }
}

pub fn atrous_filter(mut image: AtrousImage) -> AtrousImage {
    for step in [1_usize, 2, 4] {
        let mut filtered = vec![Vec3::ZERO; image.radiance.len()];
        for y in 0..image.height {
            for x in 0..image.width {
                let center = y * image.width + x;
                let dc = image.depth[center];
                let nc = image.normals[center].normalize_or_zero();
                let yc = luminance(image.radiance[center]);
                let mut sum = Vec3::ZERO;
                let mut total = 0.0;
                for (ky, ky_weight) in KERNEL.iter().enumerate() {
                    for (kx, kx_weight) in KERNEL.iter().enumerate() {
                        let dx = (kx as isize - 2) * step as isize;
                        let dy = (ky as isize - 2) * step as isize;
                        let sx = (x as isize + dx).clamp(0, image.width as isize - 1) as usize;
                        let sy = (y as isize + dy).clamp(0, image.height as isize - 1) as usize;
                        let sample = sy * image.width + sx;
                        let depth_weight =
                            (-((image.depth[sample] - dc).abs() / (0.02 * dc.abs() + 0.05))).exp();
                        let normal_weight = nc
                            .dot(image.normals[sample].normalize_or_zero())
                            .max(0.0)
                            .powi(32);
                        let luma_weight = (-((luminance(image.radiance[sample]) - yc).abs()
                            / (image.variance[center].sqrt() + 0.02)))
                            .exp();
                        let weight = kx_weight * ky_weight / 256.0
                            * depth_weight
                            * normal_weight
                            * luma_weight;
                        sum += image.radiance[sample] * weight;
                        total += weight;
                    }
                }
                filtered[center] = if total > f32::EPSILON {
                    sum / total
                } else {
                    image.radiance[center]
                };
            }
        }
        image.radiance = filtered;
    }
    image
}

fn luminance(value: Vec3) -> f32 {
    value.dot(Vec3::new(0.2126, 0.7152, 0.0722))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atrous_preserves_depth_edge() {
        let width = 10;
        let height = 4;
        let mut radiance = vec![Vec3::ZERO; width * height];
        let mut depth = vec![1.0; width * height];
        let normals = vec![Vec3::Z; width * height];
        for y in 0..height {
            for x in 5..width {
                radiance[y * width + x] = Vec3::ONE;
                depth[y * width + x] = 8.0;
            }
        }
        let result = atrous_filter(AtrousImage::new(width, height, radiance, depth, normals));
        let left = result.radiance[width + 4].x;
        let right = result.radiance[width + 5].x;
        assert!(left < 0.20 && right > 0.80, "edge mixed: {left} {right}");
    }
}
