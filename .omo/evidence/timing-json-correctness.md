# Timing JSON correctness evidence

- Scenario: offscreen `m7-passes` render with optional stages absent and GPU timestamps enabled.
- Invocation: `cargo run --release --bin snapshot -- --fixture m7-passes --size 320x180 --warmup 1 --frames 2 --timings /tmp/vf_timing_correctness.json --out /tmp/vf_timing_correctness.png`
- Binary observables: exit 0; `/tmp/vf_timing_correctness.json` exists and reports `gpu_supported: true`, all 20 `TimedPass` keys, per-pass `median_ms`/`p95_ms`/`max_ms`, and nonzero measured values for active stages. Absent `shadow`, `water`, `exposure`, `ui`, and `viewmodel` stages are initialized and report measured zero markers rather than missing/uninitialized query data. `total_gpu` is measured from each frame's full pass set (`median_ms: 1.719374`, `max_ms: 1.781542`). PNG artifact: `/tmp/vf_timing_correctness.png`.
- Unit verification: `cargo test --bin snapshot` → 7 passed, including `snapshot_timing_values_report_real_statistics`.
- Source constraints: `cargo fmt --all` completed; `src/render/gpu_timing.rs` is 415 lines and `src/bin/snapshot.rs` is 415 lines.
