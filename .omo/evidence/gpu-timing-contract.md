# GPU timing contract evidence

- Source: `src/render/gpu_timing.rs` (385 lines; below the 500-line module limit).
- Scenario: full pass-contract unit tests.
- Invocation: `cargo test --all-targets render::gpu_timing`
- Binary observable: 4 timing tests passed, 0 failed.
- Scenario: compile check after the timing API expansion.
- Invocation: `cargo check --all-targets`
- Binary observable: `Finished dev profile` with no timing-module errors.
- Scenario: formatting and whitespace validation.
- Invocation: `cargo fmt --all -- src/render/gpu_timing.rs && git diff --check -- src/render/gpu_timing.rs`
- Binary observable: both commands exited 0.

The contract uses one 40-query timestamp set (two timestamps for each of the 20
named passes), one reusable resolve buffer, and one reusable readback buffer.
The render and compute timestamp helpers return `None` unless timing is both
supported and enabled; they do not synthesize pass values.
