# M10 UI renderer evidence

- Scenario: `src/render/ui.rs` was exposed by the existing `src/render/mod.rs` and compiled through the library test target.
- Invocation: `cargo test --lib render::ui::tests::item_icon`.
- Observable: 2 library tests were selected by the filter; both exact icon tests passed (`item_icon_bake_has_122_layers`, `item_icon_alpha_background_is_clear`). The icon bake contains 122 layers, each 64x64 RGBA, with 1,998,848 bytes total; alpha-zero ratio is 75% and nontransparent bounds are x/y 20..51.
- Invocation: `cargo fmt --all -- --check`.
- Observable: exit code 0; `src/render/ui.rs` is exactly 500 lines.
- Artifacts: `src/render/ui.rs`, `assets/shaders/ui.wgsl`.
