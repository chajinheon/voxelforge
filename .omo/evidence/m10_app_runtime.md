# M10 app runtime handoff

- Scope: `src/main.rs`, `src/app/mod.rs`, `src/app/events.rs`.
- `cargo check --all-targets` passed after app integration (before a concurrent render file disappeared).
- `cargo test --all-targets` passed: 218 library tests, 2 icon tests, 6 snapshot tests.
- App files remain below the 500-line module limit: main 69, events 266, mod 438.
- `cargo fmt`/full check is currently blocked by concurrent deletion or move of `src/render/gpu_timing.rs`, still referenced by `src/render/mod.rs`.
- Runtime hooks added: settings load/save, catalog placement, middle-click pick, inventory/pause semantics, bounded benchmark frames, deterministic audio, shaderpack fallback logging, hold-repeat edit dispatch, viewmodel action state, F2 screenshot request log.
- Remaining parent hook: pass `App.render_view` through `WindowGpu` into `Renderer::render_view`; implement actual screenshot queue capture in the window layer.
