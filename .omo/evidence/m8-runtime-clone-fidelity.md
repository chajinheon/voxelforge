# M8 runtime design-fidelity audit

Date: 2026-09-05 KST

Recommendation: REQUEST_CHANGES

This is a read-only audit of the current working tree. Earlier success claims and
static tests were treated as untrusted. The review follows `docs/BLUEPRINT.md`
§20.4, `docs/ROADMAP.md` M8.0–M8.5, and the supplied package copies. No code was
changed.

## Evidence inspected

- `/Users/chajinheon/Projects/voxelforge/docs/BLUEPRINT.md:4261-4330`
- `/Users/chajinheon/Projects/voxelforge/docs/ROADMAP.md:509-649`
- `/Users/chajinheon/Downloads/VOXELFORGE_M7_M10_DESIGN_PACKAGE/BLUEPRINT_M7_M10_POST_M6_APPEND.md:72-89,393-465`
- `/Users/chajinheon/Downloads/VOXELFORGE_M7_M10_DESIGN_PACKAGE/ROADMAP_M7_M10_UNINTERRUPTED_REPLACEMENT.md:202-349`
- `/Users/chajinheon/Projects/voxelforge/src/render/m8_graph.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/m8_graph/{api.rs,pipelines.rs}`
- `/Users/chajinheon/Projects/voxelforge/assets/shaders/{volumetric,clouds,cloud_shadow,water}.wgsl`
- `/Users/chajinheon/Projects/voxelforge/src/render/{renderer.rs,renderer/effects.rs,runtime_effects.rs,runtime_effects/stages.rs,water_pipeline.rs,depth_pyramid.rs,temporal.rs,deferred.rs,lod_pipeline.rs}`
- `/Users/chajinheon/Projects/voxelforge/src/stream/rendering.rs`
- `/Users/chajinheon/Projects/voxelforge/src/mesh/{mesher.rs,greedy.rs}`
- `/Users/chajinheon/Projects/voxelforge/src/render/draw_lists.rs`
- `/Users/chajinheon/Projects/voxelforge/assets/shaders/lod.wgsl`

No screenshot or timing artifact was accepted as proof of the M8 live path; the
source dataflow below is sufficient to show the blockers.

## Findings

### CRITICAL

#### C1 — Water, glass, and LOD are drawn into a ping that the M7 post chain never consumes

The M8 graph writes its composite to `runtime.full_graph_view()` (ping 1)
(`src/render/renderer/effects.rs:149-167`). The subsequent forward pass chooses
`runtime.active_view()`, which is still ping 0 after the preceding GI composite
(`src/render/renderer/effects.rs:147-167; src/render/renderer.rs:512-519`), and
draws LOD plus the sorted glass/water list there
(`src/render/renderer.rs:523-570`). M7 was prepared with ping 1 as its HDR input
(`src/render/renderer/effects.rs:49-64`), so those forward writes are absent
from exposure, TAAU, present, and the final image. This also makes the LOD debug
view ineffective even when meshes are resident.

Correction: advance the graph's active output after each write and make the
forward pass target the current graph output, or explicitly copy/rebind the
forward result before M7 post. Preserve the immutable opaque-HDR source for
water separately.

### HIGH

#### H1 — Volumetric lighting has no CSM input, no live closest-eight light update, and no depth/normal history reject

The M8 effect layout exposes globals, params, an eight-entry storage buffer, a
cloud-shadow texture, and camera matrices, but no M7 CSM texture/sampler
(`src/render/m8_graph/pipelines.rs:8-37`). The shader therefore uses the cloud
shadow map as its only sun shadow (`assets/shaders/volumetric.wgsl:8-10,60-63`);
there is no CSM sample. The eight-light buffer is initialized with defaults
(`src/render/m8_graph.rs:143-147`), and `update_local_lights` is only a helper
(`src/render/m8_graph/api.rs:49-55`) with no runtime caller in the render,
stream, or app paths, so the required closest-eight emissive contribution is
not live. The current depth and normal are read
(`assets/shaders/volumetric.wgsl:37-38`), but the history decision only checks
previous-UV bounds, near-zero depth, and `abs(normal.z)`
(`assets/shaders/volumetric.wgsl:67-72`); it never compares previous
depth/normal or motion and ignores `params.history_weight`.

Correction: bind CSM data, collect and deterministically upload the nearest
eight live emissive lights, and provide previous depth/normal/motion inputs with
the specified disocclusion and motion thresholds.

#### H2 — The cloud shadow is not world-space or sun-directional at its consumers

The shadow uniform has origin, world size, sun height, and time but no sun
direction (`src/render/m8_graph.rs:41-58`). The shader samples a vertical
180–260 layer for all 12 steps (`assets/shaders/cloud_shadow.wgsl:12-20,35-47`),
not positions advanced along the sun ray. Terrain lighting samples the map with
`fract(input.uv)` (`assets/shaders/deferred.wgsl:91-94`) without the snapped
world origin, while volumetric fog samples with camera UV
`fract(globals.cam_pos.xz / 1024.0 + .5)`
(`assets/shaders/volumetric.wgsl:60-62`). Thus a 512² map and its cadence do
not produce the required world-space terrain/fog shadow.

Correction: pass sun direction and snapped origin through the actual sample
path, map reconstructed world XZ to the 1024-block projection, and keep the
12-step light-ray integral shared by terrain and fog consumers.

#### H3 — M8 temporal history has no explicit reset/validity path

The two volumetric and two cloud history textures are created without a clear or
valid-history state (`src/render/m8_graph.rs:65-88,253-255`). Each frame only
writes the selected checkerboard parity and then copies that partial result to
the opposite texture (`src/render/m8_graph.rs:440-494`); there is no reset when
the camera teleports, render scale changes, GI toggles, underwater changes, or
the shader graph is rebuilt. Renderer teleport detection only changes the
global previous matrix (`src/render/renderer.rs:227-237`), and the shared
`HistoryKey::needs_reset` reference is not used by M8
(`src/render/temporal.rs:100-117`).

Correction: add explicit history clear/validity and wire it to the same reset
events as TAA/GI; reject history until the four checkerboard parities have a
valid current sample.

#### H4 — Water violates replacement-pass and underwater contracts, and its GPU shading omits required terms

The dedicated pipeline correctly has depth writes off and `LessEqual`, but its
color target is alpha blended (`src/render/water_pipeline.rs:147-163`) even
though the M8 contract requires a replacement pass with `blend=None`; the
shader returns alpha `.78` (`assets/shaders/water.wgsl:61-68`). The renderer
propagates underwater state, but the live water shader never branches on
`water.underwater` and has no 64-block underwater absorption/distortion mode
(`assets/shaders/water.wgsl:4,61-68`). The vertex path applies only vertical
`wave_height` displacement (`assets/shaders/water.wgsl:35-42`), while the CPU
Gerstner reference includes horizontal displacement (`src/render/water.rs:47-75`),
and the live foam is only a transmittance threshold rather than the required
shore/crest/noise formula (`assets/shaders/water.wgsl:65-68`).

Correction: use replacement blending, consume the underwater state in the water
shader, apply the full four-wave displacement/normal contract, and implement
shore/crest/noise foam and the specified caustic/refraction terms on the live
path.

#### H5 — M8 debug views are wired to an unused generic parameter block

`encode_runtime_effects` writes `debug_view` values for water/volumetric/cloud/LOD
(`src/render/renderer/effects.rs:117-145`), but when `M8Graph` exists it bypasses
the generic atmospheric stages and calls only the dedicated graph
(`src/render/renderer/effects.rs:147-177`). `M8Graph` has no debug-view field or
debug branch, `water.wgsl` has no debug branch, and `debug_for_view` maps these
views to `DeferredDebug::Final` (`src/render/final_pass.rs:6-15`). The required
water confidence/thickness/Fresnel and volumetric/cloud diagnostics therefore
cannot appear in the live snapshots. LOD additionally suffers from C1.

Correction: add explicit debug outputs to the actual M8 graph and route the
requested view through the final chain; prove each diagnostic with fresh Metal
snapshots.

#### H6 — The M8 depth CPU reference disagrees with the live GPU linearization

The CPU reference uses `(far + near) - z * (far - near)` and a fixed far value
of 512 (`src/render/depth_pyramid.rs:8-12,74-79`), which maps standard DirectX
depth 1.0 to far/2 instead of far. The live shader uses
`far - depth * (far - near)` with far 1000
(`assets/shaders/runtime_depth_linear.wgsl:9-12`), matching the camera
projection (`src/player/camera.rs:47-53`). The CPU tests only compare the
reference to itself, so they do not certify the M8.0 live contract.

Correction: share one DirectX-Z linearization formula and near/far constants
between CPU reference, GPU shader, and test vectors.

#### H7 — Runtime LOD bypasses the G-buffer/deferred lighting path

The only LOD draw is inside the forward translucent pass
(`src/render/renderer.rs:523-549`), while the G-buffer pass draws only opaque
chunk meshes (`src/render/renderer.rs:413-429`). `LodPipeline` targets HDR
directly and its shader emits a hardcoded block/LOD palette, not G-buffer
material/light attributes (`src/render/lod_pipeline.rs:55-82; assets/shaders/lod.wgsl:6-22`).
Even after C1 is corrected, distant terrain will bypass M7 PBR/CSM/GTAO and
cannot satisfy the near-crop parity requirement.

Correction: add a real LOD G-buffer/deferred path (including world-space
material/light attributes and shadow participation) and render it before the
deferred pass, with the final output target fixed as in C1.

### MEDIUM

#### M1 — The greedy mesher does not provide a distinct water surface greedy pass

`mesh_chunk_greedy_all` uses greedy meshing for opaque and glass but delegates
water to the non-greedy face mesher (`src/mesh/greedy.rs:100-105`), so a flat
water surface is emitted one face per block. This is within the geometric
maximum but does not implement the intended water-surface greedy path and will
inflate the dedicated water draw cost.

## Blockers before approval

1. Repair the ping/output dataflow so forward water/glass/LOD reaches M7 post
   and present (C1), then verify with a pixel-changing water snapshot.
2. Add live CSM, emissive-light upload, world-space shadow mapping, temporal
   reset/reject, and actual M8 debug outputs (H1–H3, H5).
3. Bring water replacement/underwater/Gerstner/foam behavior and the depth CPU
   reference into the authoritative contract (H4, H6).
4. Move LOD into the lit G-buffer path and re-run LOD on/off and debug gates
   (H7).

