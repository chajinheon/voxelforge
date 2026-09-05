//! Six-level HDR bloom, with a small CPU reference implementation.

use glam::Vec3;

pub const BLOOM_LEVELS: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BloomConfig {
    pub threshold: f32,
    pub knee: f32,
    pub intensity: f32,
}

impl Default for BloomConfig {
    fn default() -> Self {
        Self {
            threshold: 1.0,
            knee: 0.5,
            intensity: 0.08,
        }
    }
}

impl BloomConfig {
    pub fn threshold_color(self, color: Vec3) -> Vec3 {
        let peak = color.max_element().max(0.0);
        let knee = self.knee.max(1.0e-4);
        let soft = ((peak - self.threshold + knee) / (2.0 * knee)).clamp(0.0, 1.0);
        let contribution = (peak - self.threshold).max(0.0) + soft * soft * knee;
        color * (contribution / peak.max(1.0e-4))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BloomPyramid {
    pub levels: Vec<(u32, u32)>,
}

impl BloomPyramid {
    pub fn new(width: u32, height: u32) -> Self {
        let mut levels = Vec::with_capacity(BLOOM_LEVELS);
        let (mut w, mut h) = (width.max(1), height.max(1));
        for _ in 0..BLOOM_LEVELS {
            levels.push((w, h));
            w = w.div_ceil(2).max(1);
            h = h.div_ceil(2).max(1);
        }
        Self { levels }
    }

    pub const fn level_count(&self) -> usize {
        BLOOM_LEVELS
    }
}

/// GPU allocation for the bloom pyramid. The textures are intentionally
/// separate so each level can be sampled with a stable view and no per-frame
/// resource creation.
pub struct BloomGpu {
    pub pyramid: BloomPyramid,
    pub textures: Vec<wgpu::Texture>,
    pub views: Vec<wgpu::TextureView>,
    pub format: wgpu::TextureFormat,
}

impl BloomGpu {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        let pyramid = BloomPyramid::new(width, height);
        let mut textures = Vec::with_capacity(BLOOM_LEVELS);
        let mut views = Vec::with_capacity(BLOOM_LEVELS);
        for (index, &(level_width, level_height)) in pyramid.levels.iter().enumerate() {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("vf/m7/bloom/level-{index}")),
                size: wgpu::Extent3d {
                    width: level_width,
                    height: level_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
                view_formats: &[],
            });
            views.push(texture.create_view(&wgpu::TextureViewDescriptor::default()));
            textures.push(texture);
        }
        Self {
            pyramid,
            textures,
            views,
            format,
        }
    }
}

/// Threshold one HDR image and downsample it into a finite bloom chain.
pub fn bloom_chain(
    input: &[Vec3],
    width: usize,
    height: usize,
    config: BloomConfig,
) -> Vec<Vec<Vec3>> {
    assert_eq!(input.len(), width.saturating_mul(height));
    let mut result = Vec::with_capacity(BLOOM_LEVELS);
    let mut current = input
        .iter()
        .map(|&color| config.threshold_color(color))
        .collect::<Vec<_>>();
    let mut current_width = width.max(1);
    let mut current_height = height.max(1);
    result.push(current.clone());
    for _ in 1..BLOOM_LEVELS {
        let next_width = current_width.div_ceil(2).max(1);
        let next_height = current_height.div_ceil(2).max(1);
        let mut next = vec![Vec3::ZERO; next_width * next_height];
        for y in 0..next_height {
            for x in 0..next_width {
                let mut sum = Vec3::ZERO;
                let mut count = 0.0;
                for oy in 0..2 {
                    for ox in 0..2 {
                        let sx = (x * 2 + ox).min(current_width - 1);
                        let sy = (y * 2 + oy).min(current_height - 1);
                        sum += current[sy * current_width + sx];
                        count += 1.0;
                    }
                }
                next[y * next_width + x] = (sum / count).max(Vec3::ZERO);
            }
        }
        result.push(next.clone());
        current = next;
        current_width = next_width;
        current_height = next_height;
    }
    result
}

pub fn bloom_energy_spread(chain: &[Vec<Vec3>], far_index: usize, near_index: usize) -> f32 {
    let energy = |level: &[Vec3]| level.iter().map(|v| v.max_element().max(0.0)).sum::<f32>();
    let far = chain.get(far_index).map_or(0.0, |level| energy(level));
    let near = chain.get(near_index).map_or(0.0, |level| energy(level));
    far / near.max(1.0e-6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bloom_chain_has_six_levels() {
        assert_eq!(BloomPyramid::new(640, 360).level_count(), 6);
    }

    #[test]
    fn bloom_energy_spreads_without_nan() {
        let mut image = vec![Vec3::ZERO; 64 * 64];
        image[32 * 64 + 32] = Vec3::splat(16.0);
        let chain = bloom_chain(&image, 64, 64, BloomConfig::default());
        assert!(chain.iter().flatten().all(|v| v.is_finite()));
        assert!(bloom_energy_spread(&chain, 5, 0) > 0.0);
    }
}
