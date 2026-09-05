# Shader/UI hygiene evidence

- Scenario: GI-disabled runtime effect path.
  - Invocation: inspected `assets/shaders/runtime_effects.wgsl` after the patch.
  - Observable: `gi_denoise` returns `source(input.uv)` before any depth/neighbor samples when `params.gi_mode < 0.5`.
  - Artifact: `assets/shaders/runtime_effects.wgsl` lines 67-82.

- Scenario: Rust unit and compile checks for the owned renderer/UI surface.
  - Invocation: `cargo test --lib`.
  - Observable: 226 passed, 0 failed.
  - Artifact: command output captured in the current execution log.
  - Invocation: `cargo check --all-targets`.
  - Observable: finished successfully; existing warnings are in `src/app/mod.rs` for unfinished benchmark fields/method.
  - Artifact: command output captured in the current execution log.

- Scenario: source module size audit.
  - Invocation: `find src -type f -name '*.rs' ...` per-file line-count audit.
  - Observable: `src/render/ui.rs` is 500 lines; helper modules are 53, 49, and 42 lines. No renderer/UI module exceeds 500. Current unrelated concurrent `src/app/mod.rs` is 523 lines and needs owner follow-up.
  - Artifact: `.omo/evidence/shader-ui-hygiene.md`.
