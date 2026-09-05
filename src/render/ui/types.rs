use crate::ui::{CreativeInventory, ItemRecord, ViewModelState};

/// Live lighting inputs used by the native-resolution viewmodel pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewModelLighting {
    pub player_head_sky: u8,
    pub player_head_block: u8,
    pub sun_factor: f32,
    pub sky_color: [f32; 3],
    pub sun_dir: [f32; 3],
}

impl Default for ViewModelLighting {
    fn default() -> Self {
        Self {
            player_head_sky: 15,
            player_head_block: 0,
            sun_factor: 1.0,
            sky_color: [0.53, 0.81, 0.92],
            sun_dir: [0.0, -1.0, 0.0],
        }
    }
}

impl ViewModelLighting {
    pub fn curves(self) -> [f32; 2] {
        const LIGHT_LEVEL: [f32; 16] = [
            0.035_184_4,
            0.043_980_5,
            0.054_975_6,
            0.068_719_5,
            0.085_899_3,
            0.107_374_2,
            0.134_217_7,
            0.167_772_2,
            0.209_715_2,
            0.262_144,
            0.327_68,
            0.409_6,
            0.512,
            0.64,
            0.8,
            1.0,
        ];
        [
            LIGHT_LEVEL[usize::from(self.player_head_sky.min(15))] * self.sun_factor,
            LIGHT_LEVEL[usize::from(self.player_head_block.min(15))],
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiMode {
    Hud,
    Inventory,
    Pause,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiSettingsDisplay {
    pub render_scale: f32,
    pub taa: bool,
    pub gi_enabled: bool,
    pub ui_scale: f32,
}

/// Complete frame description borrowed while `prepare` builds the batch.
pub struct UiFrame<'a> {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub mode: UiMode,
    pub inventory: &'a CreativeInventory,
    pub records: &'a [ItemRecord],
    pub viewmodel: Option<&'a ViewModelState>,
    pub viewmodel_lighting: ViewModelLighting,
    pub settings: Option<UiSettingsDisplay>,
}

#[cfg(test)]
mod tests {
    use super::ViewModelLighting;

    #[test]
    fn viewmodel_lighting_curves_use_live_light_levels_and_sun_factor() {
        let lighting = ViewModelLighting {
            player_head_sky: 12,
            player_head_block: 6,
            sun_factor: 0.5,
            ..ViewModelLighting::default()
        };
        let [sky, block] = lighting.curves();
        assert!((sky - 0.256).abs() < 1.0e-6);
        assert!((block - 0.134_217_7).abs() < 1.0e-6);
    }
}
