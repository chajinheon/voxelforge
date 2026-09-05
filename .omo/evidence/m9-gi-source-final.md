# M9 GI source verification

Date: 2026-09-05 KST

Scenario: Rust GI contracts and source-level runtime hooks.

Invocation:

```text
cargo test --lib render::gi
wc -l src/render/gi/gpu.rs
git diff --check -- src/render/gi src/render/renderer.rs
rg -n "unsafe|wgpu-hal|ray query|acceleration structure" src/render/gi assets/shaders/gi*.wgsl
```

Captured observables:

- 18 GI tests passed, 0 failed.
- `src/render/gi/gpu.rs` is 498 lines; all other GI Rust modules are below 500 lines.
- forbidden-token scan returned no matches in the GI sources/shaders.
- edit invalidation now resets GPU temporal history explicitly; phase delta >0.01, camera teleport, revision change, and GI mode change reject history.
- history surface stores linear camera distance, oct-encoded normal, and G-buffer light-alpha material ID.
- GI Params carries live sun direction/factor and sky color for DDA sky misses.

Runtime snapshot gate remains unverified in this attempt because the shared M7/M8 renderer currently stops before GI at deferred bind-group validation (`max_bind_groups` group 4 in the deferred shader). Existing older M9 PNG reports were not reused as evidence after these source changes.
