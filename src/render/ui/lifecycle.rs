use super::UiRenderer;

impl UiRenderer {
    /// Wall time spent encoding the one-shot 122-layer GPU icon bake.
    pub fn icon_bake_ms(&self) -> u64 {
        self.icon_bake.bake_ms()
    }

    pub(crate) fn encode_icon_bake(&mut self, encoder: &mut wgpu::CommandEncoder) {
        self.icon_bake.encode(encoder);
    }
}
