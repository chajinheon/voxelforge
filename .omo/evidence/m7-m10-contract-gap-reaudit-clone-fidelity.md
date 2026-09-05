# M7-M10 Runtime Contract Re-audit

Date: 2026-09-05

Scope: read-only audit of the current working tree against `VOXELFORGE_M7_M10_DESIGN_PACKAGE`, with transient compile failures excluded. The report distinguishes live render paths from CPU references and tests.

Recommendation: REQUEST_CHANGES

## Evidence inspected

- `/Users/chajinheon/Projects/voxelforge/src/render/renderer.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/renderer/effects.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/graph.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/runtime_effects.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/runtime_effects/stages.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/gbuffer.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/shadow.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/atmosphere.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/gtao.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/water_pipeline.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/lod_pipeline.rs`
- `/Users/chajinheon/Projects/voxelforge/src/stream.rs`
- `/Users/chajinheon/Projects/voxelforge/src/stream/rendering.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/gi/gpu.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/gi/gpu/runtime.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/ui/catalog.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/ui/viewmodel.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/ui.rs`
- `/Users/chajinheon/Projects/voxelforge/src/app/mod.rs`
- `/Users/chajinheon/Projects/voxelforge/src/app/events_loop.rs`
- `/Users/chajinheon/Projects/voxelforge/assets/shaders/{runtime_effects,gtao,atmosphere,gbuffer,water,viewmodel,gi_gpu}.wgsl`
- `/Users/chajinheon/Downloads/VOXELFORGE_M7_M10_DESIGN_PACKAGE/ROADMAP_M7_M10_UNINTERRUPTED_REPLACEMENT.md`
- `/Users/chajinheon/Downloads/VOXELFORGE_M7_M10_DESIGN_PACKAGE/BLUEPRINT_M7_M10_POST_M6_APPEND.md`

## Findings

### CRITICAL

None found in this snapshot after the earlier GI scene-composite/readiness fixes. The remaining issues below are nevertheless approval blockers.

### HIGH

#### H1 — GTAO is an inert/synthetic pass and is not consumed by deferred lighting

The live path invokes `M7Effects::encode_gtao` (`src/render/renderer/effects.rs:68-77`), but the pipeline is the one-line adjacent-depth formula embedded in `src/render/m7_effects/graph.rs:4-15`, rendered at internal full resolution (`src/render/m7_effects/resources.rs:344-365`). The contract requires half-resolution XeGTAO horizon search, temporal rejection, and bilateral 5x5 filtering (`BLUEPRINT §20.3.6`, lines 257-272; ROADMAP M7.2, lines 81-87). The more complete `assets/shaders/gtao.wgsl` is only exposed as a string by `src/render/gtao.rs:16` and is never made into a runtime pipeline. More importantly, the AO target is not bound in `src/render/deferred.rs:209-235` and `assets/shaders/deferred.wgsl:73-100`, so the written AO texture cannot affect ambient/GI.

#### H2 — Atmosphere LUT/sky contract is CPU-only; live atmosphere is a generic color formula

`src/render/atmosphere.rs:101-154` generates `Vec<[f32;4]>` CPU data and the WGSL is only a string constant (`:18`). The only live sky resource is a constant-filled 64²x6 texture (`src/render/scene_bindings.rs:80-94,527-564`). The actual M7 atmosphere pass is `src/render/m7_effects/graph.rs:13-15,352-365`, which adds a fixed blue color based on depth; it never samples the required transmittance 256x64, multiscatter 32x32, or sky-view 192x108 LUTs. This fails `BLUEPRINT §20.3.7` lines 274-296 and ROADMAP M7.2 lines 81-87, including cadence/update semantics. The finite CPU test is a tests-only claim.

#### H3 — Motion vectors and camera jitter are inert on the live G-buffer/TAAU path

The live G-buffer shader writes `out.motion = vec2<f32>(0.0)` unconditionally (`assets/shaders/gbuffer.wgsl:113-141`). `Globals` has only the current `view_proj` (`src/render/globals.rs:7-16`), and `M7Effects::update` only stores a Halton offset in its post uniform (`src/render/m7_effects/graph.rs:209-219`); no jittered camera matrix or previous transform is sent to geometry. The TAAU shader samples the zero motion field (`src/render/m7_effects/graph.rs:20`). Thus static-camera tests pass by construction, while moving-camera/object reprojection is missing. This conflicts with ROADMAP M7.1/M7.3 lines 51-56, 108-119 and `BLUEPRINT §20.3.8` lines 298-317.

#### H4 — CSM edit invalidation is not wired from the user edit path

`ShadowPipeline::record` receives default `camera_distance_blocks`, `sun_delta_degrees`, and `edited_in_cascade` (`src/render/shadow.rs:222-242`); renderer-side invalidation is only camera movement (`src/render/renderer.rs:296-302`). User edits immediately drain `World::take_dirty` and call `Streamer::mark_urgent` (`src/app/mod.rs:351-376`), but `mark_urgent` only bumps mesh/light state (`src/stream.rs:264-268`) and never invalidates the shadow maps. The pure `cascade_needs_update` test therefore does not represent runtime edit freshness. This violates `BLUEPRINT §20.3.5` lines 235-255 and ROADMAP M7.2 completion line 100.

#### H5 — M8 volumetric fog/cloud/cloud-shadow runtime is a generic screen-space approximation, not the dedicated contract implementation

The live graph creates one `runtime_effects.wgsl` module and maps all seven effect stages to it (`src/render/runtime_effects.rs:157-175`); its bind group contains only source/depth/sampler/params/history (`src/render/runtime_effects/resources.rs:34-60`). The live fog shader uses a scalar fixed-color march without CSM sampling or local-light array (`assets/shaders/runtime_effects.wgsl:108-133`), its “checker” is a density multiplier based on `time*60` rather than a 2x2 frame-index parity update, and its temporal path only compares a neighboring depth (`:200-209`). Clouds are 2D `sin/cos` UV density (`:135-164`), and cloud shadow is six 2D samples (`:167-174`). The dedicated compute assets (`assets/shaders/volumetric.wgsl`, `clouds.wgsl`, `cloud_shadow.wgsl`) are not referenced by any runtime constructor. Required 32-step/CSM/8-lights/normal reject/3x-underwater and periodic 128³+32³/512² shadow cadence are specified in ROADMAP M8.2/M8.3 lines 252-301 and `BLUEPRINT §20.4.2-20.4.3` lines 427-452.

#### H6 — M8 underwater mode is permanently disabled

`WaterPipeline::update` writes `underwater: 0.0` for every frame (`src/render/water_pipeline.rs:303-325`). The live `assets/shaders/water.wgsl:4-12,61-69` declares the uniform but never branches on `water.underwater` or applies underwater absorption/distortion/fog. The physics state has `Body::in_water` (`src/player/physics.rs:59-67`), but it is never passed into the water or volumetric render params. This fails `BLUEPRINT §20.4.1` lines 417-425 and ROADMAP M8.1 lines 228-250 / M8.2 line 260.

#### H7 — M8 LOD ring coverage and source priority exist as helpers, not as a complete distant-world runtime

Streaming loads only the configured radius (`src/stream/scheduling.rs:18-38`), and each loaded base chunk rebuilds only its three local ancestors when meshed (`src/stream/rendering.rs:41-68`). The renderer draws only already-uploaded LOD meshes (`src/render/lod_pipeline.rs:147-164`). There is no ring scheduler that creates the required LOD1/2/3 grids across the R=10 32-block-overlap ranges. The runtime GI/LOD fallback path uses World or procedural `fallback_block` only (`src/stream.rs:84-95`), while the required `World→save→WorldGen` helper (`src/lod/streamer.rs:77-94`) has no runtime caller. This conflicts with ROADMAP M8.4 lines 303-328.

#### H8 — M9 GI temporal pass has no world reprojection or depth/normal/material reject, and history is never reset

The live GPU GI creates all pipelines from `assets/shaders/gi_gpu.wgsl` (`src/render/gi/gpu.rs:267-293`). Its temporal function only blends same-pixel history using one global validity bit (`assets/shaders/gi_gpu.wgsl:240-254`); no previous view matrix, depth, normal, or material bindings exist in `src/render/gi/gpu.rs:18-26,148-259`. The dedicated `assets/shaders/gi_temporal.wgsl` with those checks is never included. `GpuGi::record` marks history valid forever after the first dispatch (`src/render/gi/gpu/runtime.rs:119-121`), while clipmap updates expose `history_reset` (`src/render/gi/clipmap.rs:27-34,117-124`) and the streamer drops it (`src/stream.rs:340-352`). There is also no GI resize path despite GI output dimensions being fixed at construction (`src/render/gi/gpu.rs:54-67`; `src/window_gpu.rs:95-103`). This violates ROADMAP M9.3 lines 464-486 and the M9.2 CSM/sky requirements in lines 437-447.

#### H9 — M9 clipmap upload uses procedural fallback directly and omits LOD/save semantics

`upload_slice` samples `world.get_block` for loaded chunks and otherwise calls `fallback_block` (`src/stream.rs:80-99`). The fallback reads only `world.generator()` (`src/stream.rs:42-60`), so modified save chunks and hierarchical LOD source are not considered. This is a live data-source mismatch with ROADMAP M9.1 lines 411-420 and `BLUEPRINT §20.6` shape/LOD rule lines 1102-1112; the existence of `choose_source` is not runtime evidence.

#### H10 — M10 item icon “bake” is a procedural diamond atlas, not representative mesh rendering

`bake_item_icons` simply maps each ID to `item_icon_pixels` (`src/render/ui/catalog.rs:32-36`), which fills the same x=10..54/y=12..56 diamond mask with seeded colors/noise (`:38-77`). `UiRenderer::new` uploads those CPU pixels directly (`src/render/ui.rs:89-130`) and merely creates an unused `_icon_shader` module (`:136-141`). It does not allocate the required 128² Rgba16Float + Depth32Float target, orthographic camera/lights, or 2x2 downsample. This violates `BLUEPRINT §20.8.3` lines 1436-1451 and ROADMAP M10.3 lines 643-674; the current tests only check layer count/alpha bounds.

#### H11 — M10 viewmodel is CPU-projected 2D UI geometry with no native depth or PBR/ACES

The viewmodel cache does reuse shape faces, but `project_mesh` converts them to screen-space points and sorts triangles by CPU depth (`src/render/ui/viewmodel.rs:148-187`); `item_color`/`face_shade` are fixed seeded colors (`:299-317`). The live shader accepts `position: vec2<f32>` and forces z=0 (`assets/shaders/viewmodel.wgsl:13-25`), while both UI render passes have no depth attachment (`src/render/renderer/effects.rs:154-180`) and the viewmodel pipeline declares `depth_stencil: None` (`src/render/ui.rs:309-333`). This is not the required native LDR 3D viewmodel with Depth32Float, FOV68, near/far, Less depth, and linear PBR→ACES from `BLUEPRINT §20.9.1` lines 1455-1466 / ROADMAP M10.4 lines 676-706.

#### H12 — Settings changes in the live settings UI do not reconfigure the renderer

The UI exposes only four settings rows (`src/render/ui/panels.rs:54-116`), and `adjust_settings` mutates only `self.settings` (`src/app/mod.rs:484-498`). Renderer configuration is applied once during startup (`src/app/events_loop.rs:72-99`); no equivalent call follows an in-session render-scale, TAA, or GI change. The panel therefore saves values that do not affect the current frame. This is a runtime gap against the settings/pause requirement in ROADMAP M10.4 lines 676-706 and the full schema contract in `BLUEPRINT §20.10.1` lines 1582-1649.

#### H13 — Shader hot reload is not whole-pipeline transactional and water reload is explicitly inert

The watcher reloads only chunk/translucent/water/outline (`src/render/renderer.rs:197-204`), omitting G-buffer, deferred, M7 post, runtime effects, UI, and GI. `WaterPipeline::reload_shader` always logs a rebuild warning and returns `false` (`src/render/water_pipeline.rs:384-389`). This is weaker than the whole-pipeline transactional hot-reload/compile-swap requirement in ROADMAP M7.4 lines 138-165 and M10.5 lines 708-743. The generic `TransactionalPack<T>` tests do not prove live renderer rollback.

#### H14 — Place audio does not select pane/fence variant or share an interaction event ID with the action

Every successful RMB edit starts `ActionRequest::Place` and synthesizes `Effect::Place { pane_or_fence: false }` (`src/app/mod.rs:351-373`), even when `place_item` returns a pane/fence state. `ViewModelState::start_action` accepts only the request and stores no event ID (`src/ui/viewmodel.rs:95-117`), while audio uses the frame number as its unrelated `started_at`. This misses the pane/fence high-pass 0.35 and shared action/sound event identity in `BLUEPRINT §20.10.2` lines 1651-1659.

### MEDIUM

No additional medium finding is needed for this re-audit. Several earlier review items are now fixed or materially changed (for example, GI scene compositing and clipmap readiness); they are intentionally not repeated here.

### LOW

None.

## Blockers

1. Wire the actual M7 GTAO, contact shadow, atmosphere LUT, motion-vector, and edit-invalidation resources into the live frame graph.
2. Replace the generic M8 screen-space placeholders with the specified water-underwater, volumetric, cloud-shadow, and complete LOD runtime paths.
3. Finish M9 world-reprojected GI temporal/reset/resize and correct World→save→WorldGen/LOD data sourcing.
4. Replace CPU-procedural icons and 2D viewmodel projection with the specified live mesh/depth/PBR paths.
5. Make settings, hot reload, and interaction audio semantics affect the live runtime, then rerun the package’s runtime probes after the tree settles.
