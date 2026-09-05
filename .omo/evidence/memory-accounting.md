# Live memory accounting evidence

Scope: renderer/world/streamer memory accounting added during the M7-M10 run.

## Structural observables

- `World::block_light_memory_bytes()` computes `chunk_count * CHUNK_VOLUME * 3` from loaded chunks.
- `GpuChunk::buffer_memory_bytes()` and `GpuChunkMeshes::buffer_memory_bytes()` read live `wgpu::Buffer::size()` for vertex/index buffers.
- `Renderer::uploaded_buffer_memory_bytes()` covers chunk meshes, live LOD meshes, and the UI vertex buffer.
- `Renderer::render_target_memory_bytes()` walks live GBuffer, HDR frame, M7, runtime (full/quarter/history), and shadow textures.
- `GpuGi::texture_memory_bytes()` walks live clipmap/output/history/moment textures; it is separate from render targets to avoid double counting. Split clipmap/auxiliary hooks are also available for the 64 MiB clipmap contract.
- `MemoryAccounting::measure()` reports six components and `total_bytes()` uses saturating addition.

## Verification invocations

- `cargo fmt --all` — exit 0.
- `cargo check --all-targets` — exit 0.
- `cargo clippy --all-targets -- -D warnings` — exit 0.
- `git diff --check` — exit 0.
- `cargo test --lib memory -- --nocapture` — 3 tests passed, including live-chunk `*3` arithmetic and LOD cache accounting.
- `cargo test --bin voxelforge -- memory -- --nocapture` — 2 `MemoryAccounting` tests passed, including total/log fields and overflow saturation.

Artifact: `/Users/chajinheon/Projects/voxelforge/.omo/evidence/memory-accounting.md`
