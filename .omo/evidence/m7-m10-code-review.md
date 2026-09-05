# M7-M10 Code Review

Date: 2026-09-05

## Result

- codeQualityStatus: BLOCK
- recommendation: REQUEST_CHANGES
- remove-ai-slops skill: unavailable in the exposed local skill roots; manual overfit/slop pass applied.
- programming skill: unavailable in the exposed local skill roots; manual maintainability and boundary-validation criteria applied.
- ulw-loop artifact lookup: unavailable because the `omo` runtime target is missing; this is the fallback artifact path.

The working tree was still being edited during review. `git diff --check` and `cargo fmt --all -- --check` passed at the observed snapshot, but `cargo check --all-targets` did not. Earlier, before the latest in-progress UI/graph edits, `cargo test --all-targets` compiled and failed four tests, and a runtime test reproduced the LOD WGSL validation error below. These results must be rerun after the tree settles.

## CRITICAL

### C1 — LOD shader fails wgpu validation at renderer startup

`assets/shaders/lod.wgsl:3-5` forwards `block: u32` from the vertex stage without `@interpolate(flat)`. wgpu requires integer vertex outputs to use flat interpolation. `src/render/lod_pipeline.rs:45-84` creates this shader and pipeline unconditionally and does not use an error scope. `Renderer::new` constructs the pipeline before the app/snapshot renderer is usable. A prior live `cargo test --all-targets` run reported `@interpolate(flat) must be explicitly specified for integer I/O` for this exact shader. This blocks renderer startup/snapshot and violates the project shader/pipeline validation rule.

### C2 — The current working tree is not buildable

At the last stable check, `cargo check --all-targets` failed with private UI helper re-exports and a missing `UiScale` import in `src/render/ui.rs:12,270`, `UVec3.as_uvec3()` in `src/world/world.rs:54`, and an immutable `light` buffer in `src/stream.rs:40`. A preceding in-progress snapshot also showed a temporary `src/render/m7_effects/graph.rs` delimiter/module failure. Approval requires a fresh successful all-target check after edits stop.

## HIGH

### H1 — GPU GI replaces the HDR scene instead of compositing into it

`src/render/renderer/effects.rs:17-20` prepares the runtime graph from `frame.color_view`, then replaces `RuntimeEffects::source_group` with `gpu_gi.output_view()`. `src/render/runtime_effects/stages.rs:11-40` treats that source as the first input for every GI stage, so the deferred HDR scene is discarded whenever GPU GI exists. The GI output is quarter resolution (`src/render/gi/gpu.rs:63-64`) but is sampled as the full-resolution scene. This makes the final render depend on the uninitialized/partial GI output rather than scene color.

### H2 — GPU GI is not wired to a valid world clipmap and ring readiness never completes

`src/stream.rs:32-67` uploads only the exposed regions from the stream path, while `src/render/renderer.rs:366-383` moves the clipmap again using the actual camera without uploading the returned regions. Thus moving within a chunk can shift ring origins with stale texture contents. `src/render/gi/gpu.rs:428-430` makes `mark_level_ready` a no-op, although `GpuParams.rings[*].w` is populated from `ClipmapLevel.ready` at `:441-453` and used by the shader at `assets/shaders/gi_gpu.wgsl:55`; levels remain unready. The trace shader at `assets/shaders/gi_gpu.wgsl:42-55` uses screen invocation coordinates and fixed ray directions rather than camera rays/normals, and hardcodes `MAX_DISTANCE` instead of using `params.params.x`. This is not evidence of the required world-space DDA GI.

### H3 — Toroidal GI uploads can issue out-of-bounds texture copies

`src/render/gi/gpu.rs:383-424` computes a wrapped origin but only splits uploads along Z. `SlabRegion` already exposes `physical_start` and can represent X/Y regions that cross 127 (`src/render/gi/clipmap.rs:130-157`), but the upload code ignores it. An X or Y slab with origin near 127 and extent greater than the remaining cells produces a `write_texture` extent beyond the 128³ texture, causing wgpu validation errors when the camera crosses a ring boundary.

### H4 — Shape collision helpers are not used by player physics

`src/world/shape.rs:346-400` defines slab/stair/pane/fence collision boxes, including the required 24/16 fence height. `src/player/physics.rs:64-129` checks only `def(id).solid` and collides every block against `[bp,bp+1]`; `src/player/physics.rs:179-188` calls that cube-only sweep. In runtime, stairs/slabs/panes/fences therefore have full-cube collision and fences are only one block tall. Existing shape tests exercise helper output, not player movement against those shapes.

### H5 — Timing and screenshot paths block the render loop and synthesize measurements

`src/render/gpu_timing.rs:269-279` creates zero-work marker passes for missing stages, despite the timing contract forbidding synthetic pass values. `src/render/gpu_timing.rs:289-340` maps and then calls `device.poll(PollType::Wait)`/`recv`; `src/render/renderer.rs:476-483` invokes the timing finish path after submission, so the optional timing feature stalls the frame. `src/window_gpu.rs:127-130,160-218` performs screenshot copy, blocking poll, pixel extraction, and PNG encoding inline on the render path. This contradicts the required asynchronous query/readback/screenshot contract and invalidates frame-time claims.

### H6 — Screenshot staging buffers are reused across resize without size validation

`src/render/screenshot.rs:189-200` allocates a slot buffer only when `buffer.is_none()`. `release` at `:300-307` retains it, and `WindowGpu::resize` at `src/window_gpu.rs:81-89` does not invalidate screenshot buffers. A request after a larger resize can copy `width * height` rows into the old smaller MAP_READ buffer, producing a wgpu validation error or invalid readback.

### H7 — Shader packs are validated and then discarded

`src/app/events.rs:133-140` only calls `validate_wgsl` on `self.shader_pack` and drops it; `Renderer::new` at `:142` then builds all pipelines from builtin `include_str!` sources. `src/main.rs:60-74` only loads `VF_SHADERPACK`, and `settings.video.shader_pack` is never used. There are no production calls to `resolve_shader`/`resolve_texture` outside pack tests. A valid pack cannot affect rendering, and the required fallback/transactional renderer behavior is untested.

### H8 — Persisted runtime settings silently have no effect

`src/config.rs:58-75` persists view radius, TAA/sharpen, shadow resolution/distance, POM/SSR/atmosphere step counts, GI rays/distance, vsync, and shader pack, but `src/app/events.rs:151-173` applies only preset, render scale, and GI enabled. `src/render/renderer.rs:368-376` always uses `GiConfig::default()`, and `src/window_gpu.rs:29-33` always selects `AutoVsync`. `src/player/controller.rs:29-52` hardcodes WASD bindings, sensitivity, and Y sign. The save/load round trip therefore creates settings that are serialized successfully but ignored at runtime.

### H9 — Benchmark output is informational, not a gate, and can report zero GPU timing as success

`src/app/benchmark.rs:116-167` logs results and exits without checking settled time, GPU p95/max, memory, or steady-state allocation thresholds. Empty GPU samples become `gpu_p95=0`/`gpu_max=0` at `:128-134`; allocation metrics are optional at `:140-149`. This cannot substantiate the M10.5 performance contract. Existing timing/evidence files claim successful snapshot/timing runs while current checks fail and while source still synthesizes marker values; those artifacts are stale or contradictory and must not be treated as approval evidence.

### H10 — CSM freshness input omits sun and edit invalidation

`src/render/shadow.rs:193-201` passes only `frame_index` and default values to `cascade_needs_update`; `src/render/renderer.rs:282-297` invalidates only on camera displacement. Sun direction is updated every frame, but the cascade update policy never receives `sun_delta` or `edited_in_cascade`, so maps can remain stale after lighting changes and edited terrain except for periodic redraws.

### H11 — Water UVs use native dimensions for an internal-resolution pass

`src/render/water_pipeline.rs:284-297` writes `globals.time_res[1:3]` into `internal_size`, while the water pass renders to the internal target selected in `src/render/renderer.rs:392-405`. `assets/shaders/water.wgsl:45-48` divides `input.position.xy` by that native size for scene/depth sampling. At a render scale below 1 this shifts refraction, depth, and SSR sampling relative to the actual target.

## MEDIUM

### M1 — Audio voice IDs are never finished at runtime

`src/audio.rs:66-93` requires callers to finish admitted voices. Runtime calls in `src/app/inventory.rs:98-105` and the action sites in `src/app/mod.rs` discard the returned `VoiceId`; after 16 one-shot sounds, stale entries occupy the cap and later sounds are dropped/replaced incorrectly. Tests cover the pure policy only, not the app integration.

### M2 — Held shape rendering is a flat rectangle, not the selected voxel shape

`src/render/ui/panels.rs:54-90` renders the viewmodel as an `item_color` rectangle. It does not use slab/stair/pane/fence geometry, so the M10 held-shape requirement is not met even if inventory IDs are correct.

### M3 — Inventory icon draw path tiles the entire icon repeatedly

`src/render/ui/catalog.rs:69-106` emits a 12×12 grid where each quad uses UV `[0,1]` for the whole baked texture. The bound icon shader therefore repeats the full icon atlas/image across every tile rather than selecting a single icon layer/region. Helper tests do not validate the rendered result.

### M4 — UI settings and scale paths are incomplete

The pause panel labels SETTINGS in `src/render/ui/panels.rs:30-50`, but `src/app/events.rs:231-365` has no transition or input path to `UiMode::Settings`. `src/render/ui.rs:451-455` creates a HUD `UiScale` with `settings_scale: 1.0`, ignoring `UiFrame.scale`. Persisted UI scale therefore does not apply to the HUD.

### M5 — Tests/evidence contain overfit or currently broken gates

Manual remove-ai-slops/programming review found `src/render/ui/tests.rs:11-15` tautologically compares `item_color(101)` with itself; `:17-25` derives icon-size expectations from the same constants/shape under test; `src/bin/snapshot/tests.rs:131-141` tests percentile arithmetic only on fabricated samples; and `src/shaderpack/tests.rs:57-80` tests the generic pack resolver without renderer integration. The renderer reload test at `src/render/gbuffer.rs:269-299` builds a temp asset root that omits current renderer dependencies and previously failed with `No such file or directory`. These are MEDIUM because they provide false confidence or add maintenance burden, not because the pure helpers are inherently invalid.

### M6 — LOD cache entries are all permanently protected

`src/stream/rendering.rs:70-79` inserts every LOD cache entry with `protected=true`. `src/lod/streamer.rs:245-252` defines `set_protected`, but no caller uses it, and eviction at `:270-310` only removes unprotected entries. The configured cache cap cannot evict normal LOD entries and will reject/retain stale data.

### M7 — Several modules exceed the project maintainability limit

At the observed tree, `src/render/m7_effects/graph.rs` was 735 lines and `src/render/gi/gpu.rs` 546 lines, exceeding the repository's 500-line module rule. This is a scope/maintenance issue unless the final settled tree splits them.

## LOW

No additional LOW finding is needed; the HIGH/CRITICAL issues already prevent approval.

## Blockers

1. Restore a buildable tree and fix the LOD WGSL validation error before renderer startup can be trusted.
2. Repair GI frame-graph compositing, world-space clipmap upload/readiness/ring wrapping, and fallback behavior.
3. Wire shape collision boxes into player physics.
4. Remove synchronous/synthetic timing and screenshot readbacks; fix resized staging buffers.
5. Connect shader-pack overrides and persisted settings to the actual renderer/input configuration.
6. Replace benchmark/evidence-only success claims with checks that fail on missing or stale artifacts, then rerun all tests and runtime probes after the tree settles.
