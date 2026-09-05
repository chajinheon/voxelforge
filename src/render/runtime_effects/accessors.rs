use super::{RuntimeEffects, make_effect_group};

impl RuntimeEffects {
    pub fn quarter_resolution(&self) -> (u32, u32) {
        (self.quarter_width, self.quarter_height)
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn quarter_history_view(&self) -> &wgpu::TextureView {
        &self.quarter_history_views[self.quarter_read]
    }

    pub fn active_view(&self) -> Option<&wgpu::TextureView> {
        self.active.map(|index| &self.ping_views[index])
    }

    /// GI composite/fallback copy plus the atmosphere stages end in ping one.
    pub fn full_graph_view(&self) -> &wgpu::TextureView {
        &self.ping_views[1]
    }

    /// Stable scene input for the dedicated M8 composite. GI writes ping zero
    /// before M8, while ping one remains the post-chain output target.
    pub fn m8_scene_view(&self) -> &wgpu::TextureView {
        &self.ping_views[0]
    }

    pub fn depth_pyramid_view(&self) -> &wgpu::TextureView {
        &self.depth_full_view
    }

    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    /// Sum only resources actually allocated by the runtime effect graph.
    pub fn texture_memory_bytes(&self) -> usize {
        let full_resolution = self
            ._ping
            .iter()
            .map(crate::render::memory::texture_bytes)
            .sum();
        let quarter_resolution = self
            ._quarter_ping
            .iter()
            .chain(self._quarter_history.iter())
            .map(crate::render::memory::texture_bytes)
            .sum();
        crate::render::memory::texture_bytes(&self._depth)
            .saturating_add(full_resolution)
            .saturating_add(quarter_resolution)
    }

    pub(super) fn make_effect_group(
        &self,
        device: &wgpu::Device,
        source: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        make_effect_group(
            device,
            &self.effect_layout,
            source,
            &self.depth_views[0],
            &self.sampler,
            &self.params,
        )
    }
}
