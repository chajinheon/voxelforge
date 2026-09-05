//! Cell-sized light helpers for hierarchical terrain.

use crate::world::block::def;
use crate::world::chunk::{block_light, pack_light, sky_light};

/// Merge child light samples without introducing a brighter value than a
/// source already present in the represented volume.
pub fn downsample_light(samples: [u8; 8]) -> u8 {
    let block = samples.into_iter().map(block_light).max().unwrap_or(0);
    let sky = samples.into_iter().map(sky_light).max().unwrap_or(0);
    pack_light(block, sky)
}

/// Collapse a child cell using the coarse M6 transport step.  A raw maximum
/// would incorrectly preserve a point light at every distant level; each
/// source is propagated across the represented cell before taking the max.
pub fn downsample_light_step(samples: [u8; 8], cell_size: u8) -> u8 {
    let mut result = 0;
    for sample in samples {
        // Horizontal propagation attenuates block light across the coarse
        // cell.  Sky light is a vertical column signal: retain its source
        // value here and let the column solver attenuate only blocked steps.
        let propagated = neighbor_light(0, sample, LightDirection::Horizontal, cell_size);
        let block = block_light(result).max(block_light(propagated));
        let sky = sky_light(result).max(sky_light(sample));
        result = pack_light(block, sky);
    }
    result
}

pub fn apply_cell_blocking(light: u8, block: u16) -> u8 {
    if def(block).light_blocking {
        pack_light(block_light(light), 0)
    } else {
        light
    }
}

/// Direction used when applying the coarse M6 light transport rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LightDirection {
    Horizontal,
    Up,
    Down,
}

/// Propagate one packed light value over a cell-sized step.
///
/// Block light always loses one unit per represented cell. Sky light loses one
/// unit for horizontal/up movement and is unchanged for downward movement.
pub fn propagate(current: u8, candidate: u8, direction: LightDirection, cell_size: u8) -> u8 {
    let attenuation = cell_size.min(15);
    let block = block_light(candidate).saturating_sub(attenuation);
    let sky = match direction {
        LightDirection::Down => sky_light(current),
        LightDirection::Horizontal | LightDirection::Up => {
            sky_light(candidate).saturating_sub(attenuation)
        }
    };
    pack_light(block, sky)
}

/// The conservative light value for a coarse cell and one neighboring sample.
pub fn neighbor_light(current: u8, neighbor: u8, direction: LightDirection, cell_size: u8) -> u8 {
    let propagated = propagate(current, neighbor, direction, cell_size);
    let block = block_light(current).max(block_light(propagated));
    let sky = sky_light(current).max(sky_light(propagated));
    pack_light(block, sky)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::pack_light;

    #[test]
    fn coarse_light_uses_maximum_child_channels() {
        let samples = [
            pack_light(1, 2),
            pack_light(8, 3),
            pack_light(2, 13),
            pack_light(0, 0),
            pack_light(4, 4),
            pack_light(3, 5),
            pack_light(5, 6),
            pack_light(6, 7),
        ];
        assert_eq!(downsample_light(samples), pack_light(8, 13));
    }

    #[test]
    fn downward_sky_light_does_not_attenuate() {
        let current = pack_light(12, 11);
        let neighbor = pack_light(12, 15);
        assert_eq!(
            propagate(current, neighbor, LightDirection::Down, 4),
            pack_light(8, 11)
        );
    }

    #[test]
    fn coarse_step_attenuates_child_sources() {
        let source = pack_light(12, 15);
        assert_eq!(downsample_light_step([source; 8], 4), pack_light(8, 15));
    }

    #[test]
    fn open_vertical_sky_stays_full_across_lod_levels() {
        let source = pack_light(0, 15);
        let l1 = downsample_light_step([source; 8], 2);
        let l2 = downsample_light_step([l1; 8], 4);
        let l3 = downsample_light_step([l2; 8], 8);
        assert_eq!(sky_light(l1), 15);
        assert_eq!(sky_light(l2), 15);
        assert_eq!(sky_light(l3), 15);
    }
}
