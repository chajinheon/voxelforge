//! Stable render-quality presets and output-size calculations.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum RenderPreset {
    Performance,
    Balanced,
    #[default]
    M5AirHigh,
    Cinematic,
}

impl RenderPreset {
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "performance" => Ok(Self::Performance),
            "balanced" => Ok(Self::Balanced),
            "m5_air_high" => Ok(Self::M5AirHigh),
            "cinematic" => Ok(Self::Cinematic),
            _ => anyhow::bail!("--preset must be performance, balanced, m5_air_high, or cinematic"),
        }
    }

    pub const fn render_scale(self) -> f32 {
        match self {
            Self::Performance => 0.58,
            Self::Balanced => 0.67,
            Self::M5AirHigh => 0.72,
            Self::Cinematic => 1.0,
        }
    }

    pub const fn sharpen(self) -> f32 {
        match self {
            Self::Performance => 0.12,
            Self::Balanced => 0.16,
            Self::M5AirHigh => 0.18,
            Self::Cinematic => 0.12,
        }
    }

    /// Return the internal size, aligned to the renderer's 8-pixel tile.
    pub fn internal_size(self, output_width: u32, output_height: u32) -> (u32, u32) {
        self.internal_size_with_scale(output_width, output_height, self.render_scale())
    }

    pub fn internal_size_with_scale(
        self,
        output_width: u32,
        output_height: u32,
        scale: f32,
    ) -> (u32, u32) {
        let scale = scale.clamp(0.5, 1.0);
        let height = ((output_height as f32 * scale / 8.0).round() as u32 * 8).max(8);
        let aspect_height = output_height.max(1) as f32;
        let width =
            ((height as f32 * output_width as f32 / aspect_height / 8.0).round() as u32 * 8).max(8);
        (width.max(8), height.max(8))
    }
}

#[cfg(test)]
mod tests {
    use super::RenderPreset;

    #[test]
    fn m5_air_high_internal_size_is_1848x1040() {
        assert_eq!(
            RenderPreset::M5AirHigh.internal_size(2560, 1440),
            (1848, 1040)
        );
    }
}
