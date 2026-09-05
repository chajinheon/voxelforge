//! CPU reference and GPU descriptors for the linear-depth hierarchy.

/// The largest mip chain used by the M8 SSR contract (mip 0 through 5).
pub const DEPTH_PYRAMID_MAX_MIPS: usize = 16;
pub const DEPTH_NEAR: f32 = 0.05;
pub const DEPTH_FAR: f32 = 512.0;

/// Convert standard-Z depth to a positive view-space distance.
pub fn linearize_depth(depth: f32, near: f32, far: f32) -> f32 {
    let z = depth.clamp(0.0, 1.0);
    let denominator = (far + near) - z * (far - near);
    (near * far / denominator.max(1.0e-6)).max(0.0)
}

#[derive(Clone, Debug, PartialEq)]
pub struct DepthLevel {
    pub width: usize,
    pub height: usize,
    pub values: Vec<f32>,
}

impl DepthLevel {
    pub fn at(&self, x: usize, y: usize) -> f32 {
        self.values[y.min(self.height.saturating_sub(1)) * self.width
            + x.min(self.width.saturating_sub(1))]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DepthPyramid {
    pub levels: Vec<DepthLevel>,
}

impl DepthPyramid {
    /// Build mip 0 from linear depth. Every next level is exactly a 2x2 min.
    pub fn build(width: usize, height: usize, linear_depth: &[f32]) -> Self {
        assert!(width > 0 && height > 0);
        assert_eq!(linear_depth.len(), width * height);
        let mut levels = vec![DepthLevel {
            width,
            height,
            values: linear_depth.iter().map(|value| value.max(0.0)).collect(),
        }];
        while levels.len() < DEPTH_PYRAMID_MAX_MIPS
            && (levels.last().unwrap().width > 1 || levels.last().unwrap().height > 1)
        {
            let previous = levels.last().unwrap();
            let next_width = previous.width.div_ceil(2);
            let next_height = previous.height.div_ceil(2);
            let mut values = vec![f32::INFINITY; next_width * next_height];
            for y in 0..next_height {
                for x in 0..next_width {
                    let x0 = x * 2;
                    let y0 = y * 2;
                    values[y * next_width + x] = [
                        previous.at(x0, y0),
                        previous.at(x0 + 1, y0),
                        previous.at(x0, y0 + 1),
                        previous.at(x0 + 1, y0 + 1),
                    ]
                    .into_iter()
                    .fold(f32::INFINITY, f32::min);
                }
            }
            levels.push(DepthLevel {
                width: next_width,
                height: next_height,
                values,
            });
        }
        Self { levels }
    }

    pub fn from_standard_z(width: usize, height: usize, standard_z: &[f32]) -> Self {
        let linear = standard_z
            .iter()
            .map(|depth| linearize_depth(*depth, DEPTH_NEAR, DEPTH_FAR))
            .collect::<Vec<_>>();
        Self::build(width, height, &linear)
    }

    pub fn level(&self, mip: usize) -> Option<&DepthLevel> {
        self.levels
            .get(mip.min(self.levels.len().saturating_sub(1)))
    }

    pub fn min_depth(&self, mip: usize, x: usize, y: usize) -> f32 {
        self.level(mip)
            .map_or(f32::INFINITY, |level| level.at(x, y))
    }
}

/// Parameters shared by the depth reduction compute pipeline.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DepthPyramidParams {
    pub source_size: [u32; 2],
    pub destination_size: [u32; 2],
    pub source_mip: u32,
    pub _padding: [u32; 3],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip0_linear_depth_matches_reference() {
        let input = [0.0, 0.5, 0.9, 1.0];
        let pyramid = DepthPyramid::from_standard_z(2, 2, &input);
        for (actual, depth) in pyramid.levels[0].values.iter().zip(input) {
            assert!((actual - linearize_depth(depth, DEPTH_NEAR, DEPTH_FAR)).abs() < 1.0e-4);
        }
    }

    #[test]
    fn every_parent_is_exact_2x2_min() {
        let pyramid = DepthPyramid::build(
            5,
            3,
            &[
                8.0, 4.0, 9.0, 3.0, 7.0, 6.0, 5.0, 2.0, 1.0, 4.0, 3.0, 9.0, 8.0, 7.0, 6.0,
            ],
        );
        for mip in 1..pyramid.levels.len() {
            let parent = &pyramid.levels[mip];
            let child = &pyramid.levels[mip - 1];
            for y in 0..parent.height {
                for x in 0..parent.width {
                    let expected = [
                        child.at(2 * x, 2 * y),
                        child.at(2 * x + 1, 2 * y),
                        child.at(2 * x, 2 * y + 1),
                        child.at(2 * x + 1, 2 * y + 1),
                    ]
                    .into_iter()
                    .fold(f32::INFINITY, f32::min);
                    assert_eq!(parent.at(x, y), expected);
                }
            }
        }
    }
}
