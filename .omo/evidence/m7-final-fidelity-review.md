# M7 Final Fidelity Review

Date: 2026-09-05

Scope: read-only audit of the live renderer path against `docs/BLUEPRINT.md` §20.2–§20.3 and `docs/ROADMAP.md` M7.0–M7.4. Concurrent M8/M9/LOD work was not used as evidence. Static unit tests and prior success reports were treated as untrusted.

Recommendation: REQUEST_CHANGES

## Evidence inspected

- `/Users/chajinheon/Projects/voxelforge/docs/BLUEPRINT.md` (§20.2–§20.3)
- `/Users/chajinheon/Projects/voxelforge/docs/ROADMAP.md` (M7.0–M7.4)
- `/Users/chajinheon/Projects/voxelforge/src/render/renderer.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/renderer/effects.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/deferred.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/gbuffer.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/graph.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/gtao.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/atmosphere.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/scene_bindings.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/materials.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/shadow.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/shadow_contracts.rs`
- `/Users/chajinheon/Projects/voxelforge/src/render/post/{taau,tonemap,exposure}.rs`
- `/Users/chajinheon/Projects/voxelforge/assets/shaders/{gbuffer,gtao,atmosphere,deferred,present,exposure}.wgsl`
- `/Users/chajinheon/Projects/voxelforge/src/player/camera.rs`

No screenshot or timing artifact was treated as proof of M7 fidelity in this audit; the conclusions below are from the live call graph and bound resources.

## Findings

### CRITICAL

#### C1 — Deferred lighting never tracks the live GTAO output

The first non-light frame calls `DeferredPipeline::prepare` before the newly-created `M7Effects` exists (`/Users/chajinheon/Projects/voxelforge/src/render/renderer.rs:344-392`), so the first deferred pass has no G-buffer bind group and draws nothing. On subsequent frames, `GtaoPass::encode` writes `ao_history[1-read]`, changes `read` to that slot, and `view()` returns `history[1-read]` again (`/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/gtao.rs:255-300`). That is the slot not written by the just-finished chain. Worse, `DeferredPipeline::prepare` returns early for an unchanged size and retains the first AO view forever (`/Users/chajinheon/Projects/voxelforge/src/render/deferred.rs:243-284`). The AO sampled by deferred is therefore uninitialized on setup and then stale, independent of the current GTAO result.

Correction: establish the deferred bind group after graph creation and use a stable AO output or two prebuilt slot bind groups selected by an index. The current written slot must be the value returned to deferred; slot selection must not create bind groups in the steady frame loop.

#### C2 — Atmosphere day/night updates are functionally disconnected, and the live sky cubemap only updates mip 0

`AtmosphereGpu::update` writes `sun_mu = 0.65` for every phase and the WGSL never uses the `phase` field (`/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/atmosphere.rs:137-162`, `/Users/chajinheon/Projects/voxelforge/assets/shaders/atmosphere.wgsl:1-17`). The sky-view sun integration then averages eight arbitrary sun directions rather than integrating along the live sun ray (`/Users/chajinheon/Projects/voxelforge/assets/shaders/atmosphere.wgsl:65-75`). Transmittance and multiscatter are not recomputed when the sun changes. The atmosphere-owned cube is allocated with one mip (`/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/atmosphere.rs:42-54`), and the refresh copies only mip 0 into the scene cube, which is allocated with seven mips (`/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/atmosphere.rs:190-217`, `/Users/chajinheon/Projects/voxelforge/src/render/scene_bindings.rs:83-95`). Deferred roughness samples mips 1–6 that remain at initialization values. This makes phase changes and rough environment lighting incorrect even though the cadence code runs.

Correction: pass the current sun direction/phase into the LUT uniform, refresh the dependent LUT stages in order, integrate the actual sun ray, and generate/copy all seven cube mips after each sky refresh.

#### C3 — Auto-exposure is not a live feature and its GPU state is uninitialized

The live M7 update path always constructs `M7EffectParams` from `default()` without enabling `auto_exposure` (`/Users/chajinheon/Projects/voxelforge/src/render/renderer/effects.rs:187-220`); there is no renderer/CLI setting that sets the field. The reduction/adaptation compute work therefore cannot affect `exposure_pass`, which selects the fixed `p.exposure` branch in `/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/graph.rs:5-16`. The GPU adaptation buffer is created without an initial state (`/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/graph.rs:150-155`), and its `delta_seconds` parameter is hard-coded to 1/60 (`/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/graph.rs:136-148`) instead of using frame time. The M7.3 auto-exposure and 30/60/120 fps contract cannot be met by this path.

Correction: expose and propagate the mode, initialize the persistent exposure state, and upload actual bounded frame delta to the reduction/adaptation uniform each frame.

### HIGH

#### H1 — GTAO uses a stale camera, wrong standard-Z linearization, and an unprojected radius

The GTAO inverse view-projection, camera position, and viewport are captured only during graph preparation (`/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/graph.rs:268-280`) and copied into a GPU uniform from those cached fields on every later frame (`/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/gtao.rs:219-243`). Camera motion therefore leaves world reconstruction and history tests using the first-frame camera. The shader's standard-Z conversion is `near*far / ((far+near)-z*(far-near))`, which maps z=1 to far/2, not far (`/Users/chajinheon/Projects/voxelforge/assets/shaders/gtao.wgsl:37-39`); it also hard-codes far=512 while the actual camera uses far=1000 (`/Users/chajinheon/Projects/voxelforge/src/render/gtao.rs:6-7`, `/Users/chajinheon/Projects/voxelforge/src/player/camera.rs:45-53`). Finally, `screen_radius = 0.5*1.5/view_distance` ignores projection FOV and aspect (`/Users/chajinheon/Projects/voxelforge/assets/shaders/gtao.wgsl:46-55`), so the 1.5-block search footprint is not stable in screen space.

Correction: update the GTAO camera uniform per frame, use the actual projection near/far with `nf/(f-z*(f-n))`, and derive separate x/y pixel radii from the projection focal lengths.

#### H2 — Deferred PBR uses a constant view direction and a one-sample fake contact shadow

The live deferred shader sets `v = vec3(0,0,1)` for every pixel (`/Users/chajinheon/Projects/voxelforge/assets/shaders/deferred.wgsl:86-97`). Cook–Torrance view-dependent Fresnel, GGX half-vector, and specular response are thus independent of camera position and fragment location. Its `contact` term is only a single horizontal depth difference clamped to 0.55–1.0 (`/Users/chajinheon/Projects/voxelforge/assets/shaders/deferred.wgsl:90-93`), not the required half-resolution, 8-step, 2.5-block screen-space ray with `0.06+0.0015*depth` thickness. CSM shadow is multiplied by this unrelated term.

Correction: reconstruct the fragment world position from the inverse view-projection and use a normalized camera-to-fragment vector. Implement the specified 8-step contact ray and combine its result with CSM using `min`.

#### H3 — CSM cadence and sampling do not match the M7 contract

The renderer invalidates all cascades whenever the camera moves more than 0.02 blocks (`/Users/chajinheon/Projects/voxelforge/src/render/renderer.rs:356-362`), which defeats the required cascade-1/2 thresholds of 2/8 blocks and causes far cascades to redraw on ordinary motion. The deferred shader fixes PCF to 12 taps and a 2048 texel divisor (`/Users/chajinheon/Projects/voxelforge/assets/shaders/deferred.wgsl:44-54`), ignoring configured resolution and preset tap count. Its rotation is a UV/time hash, not the required 64² blue-noise plus frame-index 0..7 sequence (`/Users/chajinheon/Projects/voxelforge/assets/shaders/deferred.wgsl:64-67`). The shadow vertex shader also has no wind displacement or 0.025 normal offset (`/Users/chajinheon/Projects/voxelforge/assets/shaders/shadow.wgsl:15-22`), while the live opaque shader displaces leaves/grass (`/Users/chajinheon/Projects/voxelforge/assets/shaders/chunk.wgsl:56-66`).

Correction: pass actual camera/sun/edit deltas into per-cascade scheduling, use the configured resolution and preset tap count, source the blue-noise rotation, and share wind/normal-offset behavior with the opaque shadow receiver.

#### H4 — Material normal maps are random RG values, not height-derived toroidal Sobel normals; POM has no shape gate

The live scene binding builds material arrays from `MaterialArrays::procedural` (`/Users/chajinheon/Projects/voxelforge/src/render/scene_bindings.rs:48-59`). Its material RG channels are independently hashed random values (`/Users/chajinheon/Projects/voxelforge/src/render/materials.rs:149-178`), while the required height-derived toroidal Sobel routine exists separately in `/Users/chajinheon/Projects/voxelforge/src/render/pbr.rs:41-59` and is not used by array generation. The G-buffer POM branch gates only on material flags, distance, view angle, and derivative footprint (`/Users/chajinheon/Projects/voxelforge/assets/shaders/gbuffer.wgsl:119-138`); no full-cube/cutout shape information is carried in the vertex contract, so slab, stair, pane, and fence faces using a POM-enabled layer can run POM despite the explicit exclusion.

Correction: generate material RG from the height channel with toroidal Sobel, and carry a shape/render-class flag (or equivalent material instance state) that disables POM for all non-full-cube and excluded translucent shapes.

#### H5 — Emission tint and metallic values are hard-coded outside the material contract

The deferred pass receives only G-buffer textures and uses a fixed orange emission color (`/Users/chajinheon/Projects/voxelforge/assets/shaders/deferred.wgsl:101-107`). The G-buffer stores only emission strength in normal alpha (`/Users/chajinheon/Projects/voxelforge/assets/shaders/gbuffer.wgsl:132-148`), so `sea_lantern` and `cold_lamp` cannot retain their blue tint from `BlockDef::emission_rgb`. The material LUT helper also marks `sand`, `metal`, and `cobble` metallic (`/Users/chajinheon/Projects/voxelforge/src/render/scene_bindings/helpers.rs:79-118`), contrary to the §20.3.4 list where non-metal materials are 0.0. This directly breaks material-array/G-buffer/PBR fidelity for the M7 materials fixture.

Correction: bind the material LUT in deferred or encode the required emission color, fetch by the G-buffer layer ID, and centralize the exact per-layer metallic/emission recipe.

#### H6 — Forward glass/water/outline never update the reactive target

The reactive attachment is written only by the opaque G-buffer pass (`/Users/chajinheon/Projects/voxelforge/assets/shaders/gbuffer.wgsl:115-150`). The forward pass has only the HDR color attachment and depth (`/Users/chajinheon/Projects/voxelforge/src/render/renderer.rs:544-563`), and the outline is drawn into the same color pass. TAAU still samples the unchanged G-buffer reactive texture (`/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/graph.rs:7-11`, `:21-23`), so the required glass=alpha*0.75, water=0.60+foam, and outline=1.0 history rejection never occurs.

Correction: add a persistent reactive target update path with Max blending for forward and outline passes, or otherwise carry those masks into the TAAU inputs before history reconstruction.

#### H7 — TAAU compares hardware depth, uses the wrong neighborhood scale, and omits motion-dependent history weight

`history_pass` writes the raw non-linear depth buffer into the R32 history target (`/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/graph.rs:22-23`), while `taau_pass` compares that value against 0.50/2% thresholds (`/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/graph.rs:20-21`). The contract requires native linear depth. The 3×3 current neighborhood divides offsets by `os = textureDimensions(hist)` even though the samples come from the internal `src`; it must use `cs` (`/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/graph.rs:20-21`). Motion is sampled only to form `history_uv`; the history weight is `p.history*(.92-.8*reactive)` and never uses `motion_px` to interpolate .92→.78. The same shader then mixes the resolved result toward the neighborhood average as `convergence` grows (`/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/graph.rs:21`), which is an uncontracted blur and undermines the edge-residual gate.

Correction: store/compare linear depth, use internal texel offsets, compute motion-pixel weight exactly, and remove the unconditional neighborhood-average convergence blend.

#### H8 — Live jitter Y sign disagrees with the fixed 8-frame contract

The renderer applies the Y clip offset as `+2*(halton_y-0.5)/height` (`/Users/chajinheon/Projects/voxelforge/src/render/renderer.rs:225-231`). The M7 contract defines `jitter_clip_y = -2*jitter.y/internal_h`; `M7Effects` uses the contract's signed Halton values for its reconstruction uniform (`/Users/chajinheon/Projects/voxelforge/src/render/m7_effects/graph.rs:220-232`), so geometry and TAAU sample positions have opposite Y conventions. Static-camera tests cannot expose this because the motion vector intentionally removes jitter.

Correction: apply the exact contract sign and use one shared Halton/jitter helper for geometry and reconstruction.

#### H9 — ACES/sharpen order in the live present pass is reversed

`FinalPass` loads `assets/shaders/present.wgsl` as its live source (`/Users/chajinheon/Projects/voxelforge/src/render/final_pass.rs:27-42`). That shader computes the 5-tap sharpen from HDR samples and only then applies ACES and grading (`/Users/chajinheon/Projects/voxelforge/assets/shaders/present.wgsl:20-31`). §20.3.9 requires ACES + color grade first, followed by native-LDR 5-tap unsharp, with hand/UI excluded. The separate CPU contract and `tonemap.wgsl` do not change this live path.

Correction: tone-map and grade the center/neighbors first, then apply the clamped 5-tap sharpen in native LDR.

### MEDIUM

#### M1 — The Light debug view bypasses the required present-copy-only surface path

`RenderView::Light` directly opens a color/depth pass on the caller's `color_view` and draws opaque geometry (`/Users/chajinheon/Projects/voxelforge/src/render/renderer.rs:276-300`). M7.4 requires caller output to receive a present copy; the exception is not documented in the frame-graph contract. This is isolated to that debug view but means the surface-independence rule is not global.

Correction: render Light into an internal debug target and use the same caller-output present copy, or explicitly scope the contract exception.

#### M2 — M7 quality presets do not drive the live GTAO/PCF/POM variant

The live GTAO shader is fixed at 8 directions × 4 steps (`/Users/chajinheon/Projects/voxelforge/assets/shaders/gtao.wgsl:50-61`) and the deferred shadow shader is fixed at 12 taps (`/Users/chajinheon/Projects/voxelforge/assets/shaders/deferred.wgsl:50-54`). `Renderer::configure_render_quality` changes POM step count but has no GTAO direction/step or PCF-tap configuration (`/Users/chajinheon/Projects/voxelforge/src/render/renderer/api.rs:352-396`). Consequently performance/balanced/cinematic rows in the preset contract do not select their declared quality variants.

Correction: pass a bounded quality variant into the live uniforms/pipelines and ensure the preset values are reflected without creating resources in the frame loop.

#### M3 — The CPU depth-pyramid reference and live depth consumers disagree on far-plane mapping

The live depth-linear shader uses a hard-coded far=1000 formula (`/Users/chajinheon/Projects/voxelforge/assets/shaders/runtime_depth_linear.wgsl:8-12`), while the CPU reference uses the far/2-at-z=1 formula (`/Users/chajinheon/Projects/voxelforge/src/render/depth_pyramid.rs:8-13`) and GTAO repeats the same wrong mapping. This makes CPU probes and live SSR/GTAO depth semantics non-comparable and hides the M7 edge/history error.

Correction: make one standard-Z linearization contract with explicit near/far, use it in CPU and GPU paths, and upload the camera values rather than hard-coding 1000.

### LOW

No additional low-severity finding is needed. The critical/high findings are sufficient to block M7 approval.

## Blockers

1. Repair the GTAO slot/bind-group lifecycle and update its camera/depth math before trusting any AO or deferred image.
2. Wire live atmosphere sun/LUT/mip updates and a real auto-exposure path.
3. Correct deferred PBR view/contact/CSM data and material/shape/emission contracts.
4. Correct TAAU depth, motion weighting, jitter, reactive accumulation, and ACES/sharpen order.
5. Rerun Metal offscreen frame probes and numeric M7 gates only after the above are fixed.

