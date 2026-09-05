use glam::{IVec3, Vec2, Vec3};

pub const MAX_RAYS: u32 = 6;
pub const MAX_DISTANCE: f32 = 48.0;
pub const MAX_CROSSINGS: u32 = 96;
pub const WORKGROUP_SIZE: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceConfig {
    pub rays: u32,
    pub max_distance: f32,
    pub max_crossings: u32,
    pub intensity: f32,
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            rays: 4,
            max_distance: MAX_DISTANCE,
            max_crossings: MAX_CROSSINGS,
            intensity: 0.65,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GiRay {
    pub origin: Vec3,
    pub direction: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdaHit {
    pub cell: IVec3,
    pub normal: IVec3,
    pub crossings: u32,
}

pub fn hammersley(index: u32, count: u32) -> Vec2 {
    let n = count.max(1);
    let mut bits = index;
    let mut reversed = 0_u32;
    for _ in 0..32 {
        reversed = (reversed << 1) | (bits & 1);
        bits >>= 1;
    }
    Vec2::new(index as f32 / n as f32, reversed as f32 * 2.328_306_4e-10)
}

pub fn cosine_hemisphere_sample(index: u32, count: u32, rotation: Vec2, normal: Vec3) -> Vec3 {
    let u = (hammersley(index, count) + rotation).fract();
    let phi = std::f32::consts::TAU * u.x;
    let radius = u.y.sqrt();
    let local = Vec3::new(
        radius * phi.cos(),
        radius * phi.sin(),
        (1.0 - u.y).max(0.0).sqrt(),
    );
    let n = normal.normalize_or_zero();
    let helper = if n.z.abs() < 0.999 { Vec3::Z } else { Vec3::Y };
    let tangent = helper.cross(n).normalize_or_zero();
    let bitangent = n.cross(tangent);
    (tangent * local.x + bitangent * local.y + n * local.z).normalize_or_zero()
}

pub fn dda_first_hit<F>(
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
    max_crossings: u32,
    mut occupied: F,
) -> Option<DdaHit>
where
    F: FnMut(IVec3) -> f32,
{
    let direction = direction.normalize_or_zero();
    if direction.length_squared() < f32::EPSILON || max_distance <= 0.0 || max_crossings == 0 {
        return None;
    }
    let mut cell = origin.floor().as_ivec3();
    if occupied(cell) >= 0.5 {
        return Some(DdaHit {
            cell,
            normal: IVec3::ZERO,
            crossings: 0,
        });
    }
    let step = IVec3::new(
        direction.x.signum() as i32,
        direction.y.signum() as i32,
        direction.z.signum() as i32,
    );
    let inv = Vec3::new(
        if direction.x.abs() > f32::EPSILON {
            1.0 / direction.x.abs()
        } else {
            f32::INFINITY
        },
        if direction.y.abs() > f32::EPSILON {
            1.0 / direction.y.abs()
        } else {
            f32::INFINITY
        },
        if direction.z.abs() > f32::EPSILON {
            1.0 / direction.z.abs()
        } else {
            f32::INFINITY
        },
    );
    let next_boundary = Vec3::new(
        if step.x > 0 {
            cell.x as f32 + 1.0
        } else {
            cell.x as f32
        },
        if step.y > 0 {
            cell.y as f32 + 1.0
        } else {
            cell.y as f32
        },
        if step.z > 0 {
            cell.z as f32 + 1.0
        } else {
            cell.z as f32
        },
    );
    let mut t_max = Vec3::new(
        if inv.x.is_finite() {
            (next_boundary.x - origin.x).abs() * inv.x
        } else {
            f32::INFINITY
        },
        if inv.y.is_finite() {
            (next_boundary.y - origin.y).abs() * inv.y
        } else {
            f32::INFINITY
        },
        if inv.z.is_finite() {
            (next_boundary.z - origin.z).abs() * inv.z
        } else {
            f32::INFINITY
        },
    );
    let t_delta = inv;
    for crossings in 1..=max_crossings {
        let axis = if t_max.x <= t_max.y && t_max.x <= t_max.z {
            0
        } else if t_max.y <= t_max.z {
            1
        } else {
            2
        };
        let distance = t_max[axis];
        if !distance.is_finite() || distance > max_distance {
            break;
        }
        cell[axis] += step[axis];
        t_max[axis] += t_delta[axis];
        if occupied(cell) >= 0.5 {
            let mut normal = IVec3::ZERO;
            normal[axis] = -step[axis];
            return Some(DdaHit {
                cell,
                normal,
                crossings,
            });
        }
    }
    None
}

pub fn select_level(distance: f32) -> usize {
    if distance < 8.0 {
        0
    } else if distance < 16.0 {
        1
    } else if distance < 32.0 {
        2
    } else {
        3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dda_hits_first_opaque_voxel() {
        let hit = dda_first_hit(Vec3::new(0.2, 0.5, 0.5), Vec3::X, 48.0, 96, |cell| {
            f32::from(cell.x == 3)
        });
        assert_eq!(hit.map(|h| h.cell), Some(IVec3::new(3, 0, 0)));
        assert_eq!(hit.map(|h| h.normal), Some(-IVec3::X));
    }

    #[test]
    fn dda_crosses_negative_coordinates() {
        let hit = dda_first_hit(Vec3::new(-0.2, 0.5, 0.5), -Vec3::X, 48.0, 96, |cell| {
            f32::from(cell.x == -3)
        });
        assert_eq!(hit.map(|h| h.cell), Some(IVec3::new(-3, 0, 0)));
        assert_eq!(hit.map(|h| h.normal), Some(IVec3::X));
    }

    #[test]
    fn dda_misses_empty_volume() {
        assert!(dda_first_hit(Vec3::ZERO, Vec3::Y, 48.0, 96, |_| 0.0).is_none());
    }

    #[test]
    fn cosine_hemisphere_samples_are_above_normal() {
        let n = Vec3::new(0.2, 0.8, 0.4).normalize();
        for i in 0..64 {
            assert!(cosine_hemisphere_sample(i, 64, Vec2::new(0.37, 0.11), n).dot(n) >= -1e-6);
        }
    }
}
