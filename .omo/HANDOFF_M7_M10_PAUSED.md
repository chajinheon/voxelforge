# Voxelforge M7-M10 paused handoff

Paused at Jinheon's request on 2026-09-05 08:08 KST.

## Ground truth

- Repository: `/Users/chajinheon/Projects/voxelforge`
- Branch: `main`
- Base HEAD: `870cd4cc4be61b652767588293fda890edd168a6`
- No commit, push, branch, reset, rebase, deploy, or notarization was performed.
- Design reference: `/Users/chajinheon/Downloads/VOXELFORGE_M7_M10_DESIGN_PACKAGE/`
- All subagents are stopped/interrupted. No Cargo or snapshot process remains running.
- Worktree is intentionally dirty and large (140 porcelain entries). Preserve it; do not use broad reset/checkout/staging.

## Last verified good points before the pause

- `cargo test --all-targets` passed once with 290 tests total:
  - library 267
  - icon 2
  - snapshot 8
  - app binary 13
- `cargo check --all-targets` passed before the final interrupted renderer split.
- A real Metal frame rendered successfully on Apple M5 after fixing the deferred group count, M8 WGSL/layout errors, GTAO attachment aliasing, and atmosphere resource aliasing:

```bash
cargo run --release --bin snapshot -- \
  --fixture m7-materials --size 320x180 --render-scale 1.0 \
  --preset m5_air_high --fixed-exposure 1.0 --warmup 2 --frames 2 \
  --view final --out /tmp/vf_runtime_smoke2.png
```

- Root added stable GTAO history-copy wiring in:
  - `src/render/m7_effects/gtao.rs`
  - `src/render/m7_effects/gtao_gpu.rs`
- Root split atmosphere LUT bind groups by actual resource use and added true cubemap mip 1-6 downsampling in:
  - `src/render/m7_effects/atmosphere.rs`
  - `assets/shaders/atmosphere.wgsl`

These successes predate the last interrupted parallel edits and must be reverified.

## Exact current blocker

The current tree does **not** compile because the renderer split agent was interrupted after moving the render loop to `src/render/renderer/frame.rs`.

`cargo check --all-targets` currently reports seven bad module-relative paths in that file:

- line 14: `super::ui::UiFrame`
- line 177: `super::m8_graph::M8Graph`
- line 188: `super::final_pass::debug_for_view`
- lines 201 and 207: `super::clouds::*`
- lines 321 and 327: `super::runtime_effects::RuntimeEffects::*`

Use `crate::render::{ui, m8_graph, final_pass, clouds, runtime_effects}` imports or the equivalent `super::super::*` paths. Also remove the current unused `Facing` import in `src/mesh/shaped.rs` if it remains after the shape work is reconciled.

First resume commands:

```bash
cd /Users/chajinheon/Projects/voxelforge
cargo fmt --all
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

## Known incomplete work, in priority order

1. **Stabilize interrupted edits.** Inspect the partial changes in `renderer/frame.rs`, M10 shape/interaction/UI/material/icon files, and shaderpack APIs before modifying them. Do not assume interrupted workers finished atomic changes.
2. **M7 visual correctness.** Exact `m7-materials` at warmup 16 becomes nearly all white (mean RGB about 254; 919,217 of 921,600 pixels pure white). GI off behaves the same. Warmup 1 mean was about 61 and warmup 8 about 212, proving frame-to-frame accumulation/saturation in the M7 post path. The likely audit surface is `src/render/m7_effects/graph.rs`: exposure currently creates a pre-exposed texture before TAAU, bloom/TAA source selection must remain scene-referred, and atmosphere radiance must be bounded. Run exact §20.12.1 snapshots and numeric crop checks after fixing.
3. **Module size gate.** Current known failures include `src/render/renderer/api.rs` at 556 lines and `src/render/m7_effects/graph.rs` at 501 lines. Hard limit is 500 lines; 400 is the soft split point.
4. **M10 shape contract.** A pre-pause exact 1280x720 `m10-shapes` run failed `shape probe slab_bottom at (150,430)`. Reconcile partial work for bidirectional cube/shape face subtraction, exact fence rails, Y logs, light blocking, micro AO/light, shape collision/raycast, and complete fixtures/probes.
5. **M10 runtime/UI contract.** Reconcile partial work for break connection updates, shape-aware placement collision, slab mutation result, hotbar/viewmodel/sound events, actual walking distance, persisted held item, inventory hover/tooltip/crosshair/exact labels and y=584 preview, CLI precedence, and benchmark use of the real interaction path.
6. **LOD/material/icon contract.** Verify `lod_equivalent` is applied before coarse modal selection, exact material recipes/names exist, and icon bake creates no per-frame GPU resources and transforms normals correctly.
7. **Shaderpack contract 2.** Loader alias/texture prevalidation exists, but the full allowed M7-M9 shader set still needs proven live staged construction and atomic swap. Compile-only storage is not acceptance.
8. **M8/M9 fidelity.** M8 Metal validation reached a rendered frame, and GI-focused unit tests passed, but CSM direct GI binding, quality pixel gates, temporal resets, seams, settle, and release timings remain unverified.
9. **Final gates.** Run every exact ROADMAP M7/M8/M9/M10 snapshot, pixel/probe script, 2560x1440 timing capture, 3600-frame benchmark, allocation/memory gate, audio WAV, broken shaderpack fallback, app bundle/plutil/codesign/launcher smoke, and truthful notarization skip/run. Update `docs/LOG.md` only with actually measured results.

## Current evidence and reports

- `.omo/evidence/m7-final-fidelity-review.md`
- `.omo/evidence/m8-runtime-clone-fidelity.md`
- `.omo/evidence/m9-gi-source-final.md`
- `.omo/evidence/gpu-timing-check.txt`

Keep `.omo` while the work is paused. Delete it only after every final gate passes, as required by the original execution cleanup plan.

## Final completion rule

Do not commit. Do not report completion until every gate above is real and current. Only then update the top of `docs/LOG.md` and use the exact final phrase:

`M7~M10 완료, 최종 리뷰 요청`
