use super::{GpuGi, LEVELS, VOXELS};
use crate::render::gi::{Clipmap, VoxelUpload};

const MAX_UPLOAD: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WrappedPart {
    source: u32,
    physical: u32,
    len: u32,
}

fn wrapped_parts(physical_start: u32, len: u32) -> ([WrappedPart; 2], usize) {
    if len == 0 {
        return (
            [WrappedPart {
                source: 0,
                physical: 0,
                len: 0,
            }; 2],
            0,
        );
    }
    let physical_start = physical_start % VOXELS;
    let first_len = len.min(VOXELS - physical_start);
    let second_len = len - first_len;
    (
        [
            WrappedPart {
                source: 0,
                physical: physical_start,
                len: first_len,
            },
            WrappedPart {
                source: first_len,
                physical: 0,
                len: second_len,
            },
        ],
        usize::from(second_len > 0) + 1,
    )
}

impl GpuGi {
    pub fn upload(&self, queue: &wgpu::Queue, clipmap: &Clipmap, upload: &VoxelUpload) {
        if !self.enabled {
            return;
        }
        let level = usize::from(upload.level);
        if level >= LEVELS {
            return;
        }
        let size = upload.extent.x as usize * upload.extent.y as usize * upload.extent.z as usize;
        if size == 0 || upload.material.len() < size * 4 || upload.light.len() < size * 4 {
            return;
        }
        let origin = clipmap.levels[level].logical_to_physical(upload.logical_origin);
        let (x_parts, x_count) = wrapped_parts(origin.x, upload.extent.x);
        let (y_parts, y_count) = wrapped_parts(origin.y, upload.extent.y);
        let (z_parts, z_count) = wrapped_parts(origin.z, upload.extent.z);
        let source_row_bytes = upload.extent.x as usize * 4;
        let source_image_bytes = source_row_bytes * upload.extent.y as usize;
        for x in x_parts.into_iter().take(x_count) {
            for y in y_parts.into_iter().take(y_count) {
                let max_depth = (MAX_UPLOAD / (x.len as usize * y.len as usize * 4)).max(1);
                for z in z_parts.into_iter().take(z_count) {
                    let mut source_z = z.source;
                    while source_z < z.source + z.len {
                        let depth = (z.source + z.len - source_z).min(max_depth as u32);
                        let source_offset = source_z as usize * source_image_bytes
                            + y.source as usize * source_row_bytes
                            + x.source as usize * 4;
                        let extent = wgpu::Extent3d {
                            width: x.len,
                            height: y.len,
                            depth_or_array_layers: depth,
                        };
                        let physical = wgpu::Origin3d {
                            x: x.physical,
                            y: y.physical,
                            z: (z.physical + source_z - z.source) % VOXELS,
                        };
                        for (texture, data) in [
                            (&self.material[level], &upload.material[source_offset..]),
                            (&self.light[level], &upload.light[source_offset..]),
                        ] {
                            queue.write_texture(
                                wgpu::TexelCopyTextureInfo {
                                    texture,
                                    mip_level: 0,
                                    origin: physical,
                                    aspect: wgpu::TextureAspect::All,
                                },
                                data,
                                wgpu::TexelCopyBufferLayout {
                                    offset: 0,
                                    bytes_per_row: Some(upload.extent.x * 4),
                                    rows_per_image: Some(upload.extent.y),
                                },
                                extent,
                            );
                        }
                        source_z += depth;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapped_parts_cover_a_wrapped_axis_without_crossing_texture_edge() {
        let (parts, count) = wrapped_parts(126, 6);
        assert_eq!(count, 2);
        assert_eq!(
            parts[0],
            WrappedPart {
                source: 0,
                physical: 126,
                len: 2
            }
        );
        assert_eq!(
            parts[1],
            WrappedPart {
                source: 2,
                physical: 0,
                len: 4
            }
        );
        assert!(
            parts[..count]
                .iter()
                .all(|part| part.physical + part.len <= VOXELS)
        );
        assert_eq!(parts[..count].iter().map(|part| part.len).sum::<u32>(), 6);
    }

    #[test]
    fn wrapped_parts_preserve_unwrapped_source_offset() {
        let (parts, count) = wrapped_parts(8, 120);
        assert_eq!(count, 1);
        assert_eq!(parts[0].source, 0);
        assert_eq!(parts[0].physical, 8);
        assert_eq!(parts[0].len, 120);
    }
}
