# Renderer allocation instrumentation

- Scope: safe per-frame counters; no global allocator, `unsafe`, or process-wide hooks.
- Implementation: `src/render/allocation_stats.rs` (`AllocationStats`, `AllocationDelta`, `TrackedScratch<T>`); `DrawLists` uses tracked reusable scratch; `UiRenderer` reports vertex-buffer growth; `Renderer` exposes `last_allocation_stats()` and records graph/target creation events.
- Contract: `VF_ALLOC_STATS=1` enables counters. Each render frame resets and publishes CPU capacity-growth and GPU resource-creation deltas. Without the flag, counters stay zero and no benchmark allocation claim is emitted.

## Verification

- Invocation: `cargo test --lib allocation_stats -- --nocapture`
  - Observable: 2 tests passed (`tracked_scratch_reports_growth_once_per_capacity_change`, `disabled_stats_are_zero_and_allocation_free_contract_is_explicit`).
- Invocation: `cargo check --release`
  - Observable: exit 0; release library checked successfully (existing deprecation/dead-code warnings only).
- Invocation: `cargo test --lib`
  - Observable: 229 tests passed, 1 pre-existing/shared-tree failure in `render::gbuffer::tests::renderer_keeps_legacy_pipeline_after_invalid_reload`; failure is `assets/shaders/lod.wgsl` integer vertex output missing `@interpolate(flat)`, outside allocation instrumentation.
