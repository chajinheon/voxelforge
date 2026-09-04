//! View-frustum extraction and axis-aligned bounding-box intersection.
//!
//! The planes are extracted from a column-major `glam::Mat4` using the
//! Gribb-Hartmann method.  The projection used by this project is the
//! right-handed, zero-to-one depth variant, so the near plane is `z >= 0` in
//! clip space and the far plane is `z <= w`.

use glam::{Mat4, Vec3, Vec4};

#[derive(Clone, Copy, Debug)]
struct Plane {
    normal: Vec3,
    distance: f32,
}

impl Plane {
    fn from_clip_coefficients(coefficients: Vec4) -> Self {
        let normal = coefficients.truncate();
        let length = normal.length();
        if length > f32::EPSILON {
            Self {
                normal: normal / length,
                distance: coefficients.w / length,
            }
        } else {
            // A degenerate plane cannot occur for a valid view-projection
            // matrix.  Keeping it as an always-inside plane makes the
            // intersection routine robust for diagnostic/zero matrices.
            Self {
                normal: Vec3::ZERO,
                distance: 0.0,
            }
        }
    }

    fn signed_distance(self, point: Vec3) -> f32 {
        self.normal.dot(point) + self.distance
    }
}

/// The six half-spaces enclosing the camera's visible volume.
#[derive(Clone, Copy, Debug)]
pub struct Frustum {
    planes: [Plane; 6],
}

impl Frustum {
    /// Extract left, right, bottom, top, near, and far planes from a view-
    /// projection matrix.
    pub fn from_view_proj(view_proj: Mat4) -> Self {
        // `to_cols_array_2d` is indexed as [column][row].  Reconstructing the
        // rows explicitly keeps the clip-space extraction independent of
        // glam's matrix indexing helpers.
        let columns = view_proj.to_cols_array_2d();
        let row = |index: usize| {
            Vec4::new(
                columns[0][index],
                columns[1][index],
                columns[2][index],
                columns[3][index],
            )
        };
        let x = row(0);
        let y = row(1);
        let z = row(2);
        let w = row(3);

        Self {
            planes: [
                Plane::from_clip_coefficients(w + x), // left:   x + w >= 0
                Plane::from_clip_coefficients(w - x), // right:  w - x >= 0
                Plane::from_clip_coefficients(w + y), // bottom: y + w >= 0
                Plane::from_clip_coefficients(w - y), // top:    w - y >= 0
                Plane::from_clip_coefficients(z),     // near:   z >= 0
                Plane::from_clip_coefficients(w - z), // far:    w - z >= 0
            ],
        }
    }

    /// Returns whether any part of the AABB lies inside the frustum.
    ///
    /// Bounds are world-space coordinates.  Touching a plane counts as
    /// visible, which avoids popping chunks exactly on a clip boundary.
    pub fn intersects_aabb(&self, min: Vec3, max: Vec3) -> bool {
        self.planes.iter().all(|plane| {
            let positive = Vec3::new(
                if plane.normal.x >= 0.0 { max.x } else { min.x },
                if plane.normal.y >= 0.0 { max.y } else { min.y },
                if plane.normal.z >= 0.0 { max.z } else { min.z },
            );
            plane.signed_distance(positive) >= 0.0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view_proj() -> Mat4 {
        let projection =
            glam::camera::rh::proj::directx::perspective(60.0_f32.to_radians(), 1.0, 0.05, 1_000.0);
        let view = glam::camera::rh::view::look_to_mat4(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y);
        projection * view
    }

    #[test]
    fn frustum_culls_chunk_behind_camera() {
        let frustum = Frustum::from_view_proj(view_proj());
        assert!(
            !frustum.intersects_aabb(Vec3::new(-16.0, -16.0, 8.0), Vec3::new(16.0, 16.0, 40.0))
        );
    }

    #[test]
    fn frustum_keeps_chunk_in_front() {
        let frustum = Frustum::from_view_proj(view_proj());
        assert!(
            frustum.intersects_aabb(Vec3::new(-16.0, -16.0, -40.0), Vec3::new(16.0, 16.0, -8.0))
        );
    }
}
