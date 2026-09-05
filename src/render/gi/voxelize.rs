use glam::IVec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoxelKind {
    Air,
    Stone,
    Leaves,
    Water,
    Glass,
    Torch,
    Glowstone,
    SeaLantern,
    WarmLamp,
    ColdLamp,
    GlowPanel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoxelCell {
    pub material: [u8; 4],
    pub light: [u8; 4],
    pub ready: bool,
}

impl VoxelCell {
    pub const AIR: Self = Self {
        material: [0, 0, 0, 0],
        light: [0, 0, 0, 0],
        ready: true,
    };

    pub fn occupied(self) -> bool {
        self.material[3] >= 128
    }
}

pub fn emission_rgb(kind: VoxelKind) -> [f32; 3] {
    match kind {
        VoxelKind::Torch => [8.0, 3.36, 0.96],
        VoxelKind::Glowstone => [6.0, 3.4, 1.5],
        VoxelKind::SeaLantern => [3.0, 5.5, 6.5],
        VoxelKind::WarmLamp => [8.0, 4.4, 1.8],
        VoxelKind::ColdLamp => [4.5, 6.5, 8.0],
        VoxelKind::GlowPanel => [6.5, 6.8, 7.0],
        _ => [0.0; 3],
    }
}

fn albedo(kind: VoxelKind) -> [u8; 3] {
    match kind {
        VoxelKind::Stone => [128, 128, 128],
        VoxelKind::Leaves => [74, 144, 66],
        VoxelKind::Torch | VoxelKind::WarmLamp => [240, 90, 30],
        VoxelKind::Glowstone => [236, 196, 82],
        VoxelKind::SeaLantern | VoxelKind::ColdLamp | VoxelKind::GlowPanel => [140, 210, 224],
        VoxelKind::Water => [45, 100, 180],
        VoxelKind::Glass => [190, 220, 235],
        VoxelKind::Air => [0; 3],
    }
}

fn opacity(kind: VoxelKind) -> u8 {
    match kind {
        VoxelKind::Air | VoxelKind::Water | VoxelKind::Glass => 0,
        VoxelKind::Leaves => 128,
        _ => 255,
    }
}

#[derive(Clone, Debug)]
pub struct VoxelPacker {
    pub origin: IVec3,
    pub extent: [u32; 3],
}

impl VoxelPacker {
    pub fn new(origin: IVec3, extent: [u32; 3]) -> Self {
        Self { origin, extent }
    }

    pub fn pack<F>(&self, mut source: F) -> (Vec<u8>, Vec<u8>)
    where
        F: FnMut(IVec3) -> VoxelKind,
    {
        let count: usize = self.extent.iter().map(|v| *v as usize).product();
        let mut material = Vec::with_capacity(count * 4);
        let mut light = Vec::with_capacity(count * 4);
        for z in 0..self.extent[2] {
            for y in 0..self.extent[1] {
                for x in 0..self.extent[0] {
                    let kind = source(self.origin + IVec3::new(x as i32, y as i32, z as i32));
                    let rgb = albedo(kind);
                    material.extend_from_slice(&[rgb[0], rgb[1], rgb[2], opacity(kind)]);
                    let emission = emission_rgb(kind);
                    let encoded =
                        emission.map(|v| (v / 8.0 * 255.0).round().clamp(0.0, 255.0) as u8);
                    light.extend_from_slice(&[encoded[0], encoded[1], encoded[2], 0]);
                }
            }
        }
        (material, light)
    }

    pub fn lod_equivalent(cells: &[VoxelCell]) -> VoxelCell {
        if cells.is_empty() {
            return VoxelCell::AIR;
        }
        let mut result = VoxelCell::AIR;
        let mut opaque = 0usize;
        let mut light = [0u32; 3];
        let mut sky = 0u32;
        for cell in cells {
            if cell.material[3] >= 128 {
                opaque += 1;
                for (sum, value) in light.iter_mut().zip(cell.light[..3].iter()) {
                    *sum += *value as u32;
                }
            }
            sky += cell.light[3] as u32;
        }
        result.material[3] = ((opaque * 255) / cells.len()).min(255) as u8;
        result.light[3] = (sky / cells.len() as u32) as u8;
        if opaque > 0 {
            for (channel, value) in light.iter().take(3).enumerate() {
                result.light[channel] = (*value / opaque as u32) as u8;
            }
        }
        result.ready = true;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colored_emission_encodes_warm_torch() {
        let rgb = emission_rgb(VoxelKind::Torch);
        assert!((rgb[0] / ((rgb[1] + rgb[2]) * 0.5)) > 1.15);
        let packer = VoxelPacker::new(IVec3::ZERO, [1, 1, 1]);
        let (_, light) = packer.pack(|_| VoxelKind::Torch);
        assert_eq!(&light[..3], &[255, 107, 31]);
    }
}
