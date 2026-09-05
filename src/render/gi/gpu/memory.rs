use super::GpuGi;

impl GpuGi {
    pub fn texture_memory_bytes(&self) -> usize {
        self.clipmap_texture_memory_bytes()
            .saturating_add(self.auxiliary_texture_memory_bytes())
    }

    /// Bytes for the eight 128^3 material/light clipmap textures only.
    pub fn clipmap_texture_memory_bytes(&self) -> usize {
        self.material
            .iter()
            .chain(self.light.iter())
            .map(crate::render::memory::texture_bytes)
            .fold(0usize, usize::saturating_add)
    }

    /// Bytes for quarter-resolution GI output/history/moment textures.
    pub fn auxiliary_texture_memory_bytes(&self) -> usize {
        self._output
            .iter()
            .chain(self._moments.iter())
            .chain(self._history.iter())
            .chain(self._surface_history.iter())
            .map(crate::render::memory::texture_bytes)
            .fold(0usize, usize::saturating_add)
    }

    pub fn dispatch_dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn history_valid(&self) -> bool {
        self.history_valid
    }
}
