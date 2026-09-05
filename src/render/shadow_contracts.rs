use super::{
    CAMERA_NEAR, CASCADE_COUNT, M5_AIR_HIGH_FAR, NORMAL_OFFSET_BLOCKS, PSSM_LAMBDA, RECEIVER_BIAS,
    SHADOW_RESOLUTION,
};

/// PSSM boundaries, including near and far.  The result is `[near, split1,
/// split2, far]`, which is also the order expected by the shader uniform.
pub fn pssm_splits(near: f32, far: f32, lambda: f32) -> [f32; CASCADE_COUNT + 1] {
    let near = near.max(0.0001);
    let far = far.max(near + 0.0001);
    let lambda = lambda.clamp(0.0, 1.0);
    let mut result = [near; CASCADE_COUNT + 1];
    result[CASCADE_COUNT] = far;
    for (index, split) in result
        .iter_mut()
        .enumerate()
        .skip(1)
        .take(CASCADE_COUNT - 1)
    {
        let t = index as f32 / CASCADE_COUNT as f32;
        let logarithmic = near * (far / near).powf(t);
        let linear = near + (far - near) * t;
        *split = lambda * logarithmic + (1.0 - lambda) * linear;
    }
    result
}

pub fn m5_air_high_splits() -> [f32; CASCADE_COUNT + 1] {
    pssm_splits(CAMERA_NEAR, M5_AIR_HIGH_FAR, PSSM_LAMBDA)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowPreset {
    Performance,
    Balanced,
    M5AirHigh,
    Cinematic,
}

impl ShadowPreset {
    pub const fn far(self) -> f32 {
        match self {
            Self::Performance => 160.0,
            Self::Balanced => 192.0,
            Self::M5AirHigh => M5_AIR_HIGH_FAR,
            Self::Cinematic => 256.0,
        }
    }

    pub const fn contact_enabled(self) -> bool {
        !matches!(self, Self::Performance)
    }

    pub fn splits(self) -> [f32; CASCADE_COUNT + 1] {
        pssm_splits(CAMERA_NEAR, self.far(), PSSM_LAMBDA)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CascadeUpdateInput {
    pub frame_index: u64,
    pub camera_distance_blocks: f32,
    pub sun_delta_degrees: f32,
    pub edited_in_cascade: bool,
}

pub fn cascade_needs_update(cascade: usize, input: CascadeUpdateInput) -> bool {
    if input.edited_in_cascade {
        return true;
    }
    match cascade {
        0 => true,
        1 => {
            input.frame_index.is_multiple_of(2)
                || input.camera_distance_blocks >= 2.0
                || input.sun_delta_degrees >= 0.25
        }
        2 => {
            input.frame_index.is_multiple_of(4)
                || input.camera_distance_blocks >= 8.0
                || input.sun_delta_degrees >= 0.50
        }
        _ => false,
    }
}

pub fn snap_light_center(center_xy: [f32; 2], world_radius: f32, resolution: u32) -> [f32; 2] {
    let texel = (2.0 * world_radius.max(0.0001)) / resolution.max(1) as f32;
    [
        (center_xy[0] / texel).round() * texel,
        (center_xy[1] / texel).round() * texel,
    ]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowContract {
    pub resolution: u32,
    pub cascade_count: usize,
    pub far: f32,
    pub pcf_taps: usize,
    pub receiver_bias: f32,
    pub normal_offset: f32,
}

impl Default for ShadowContract {
    fn default() -> Self {
        Self {
            resolution: SHADOW_RESOLUTION,
            cascade_count: CASCADE_COUNT,
            far: M5_AIR_HIGH_FAR,
            pcf_taps: 12,
            receiver_bias: RECEIVER_BIAS,
            normal_offset: NORMAL_OFFSET_BLOCKS,
        }
    }
}

pub const POISSON_DISK: [[f32; 2]; 20] = [
    [-0.94201624, -0.39906216],
    [0.9455861, -0.76890725],
    [-0.0941841, -0.9293887],
    [0.34495938, 0.2938776],
    [-0.9158858, 0.45771432],
    [-0.8154423, -0.87912464],
    [-0.38277543, 0.27676845],
    [0.974_844, 0.756_483_8],
    [0.44323325, -0.97511554],
    [0.537_429_8, -0.473_734_2],
    [-0.2649691, -0.41893023],
    [0.79197514, 0.19090188],
    [-0.2418884, 0.99706507],
    [-0.81409955, 0.9143759],
    [0.19984126, 0.78641367],
    [0.14383161, -0.1410079],
    [-0.885728, 0.38645372],
    [0.45071167, 0.8898729],
    [-0.5635692, 0.8138172],
    [0.356117, -0.75680035],
];

pub fn contact_thickness(linear_depth: f32) -> f32 {
    CONTACT_THICKNESS_BASE + CONTACT_THICKNESS_SLOPE * linear_depth.max(0.0)
}

const CONTACT_THICKNESS_BASE: f32 = 0.06;
const CONTACT_THICKNESS_SLOPE: f32 = 0.0015;
