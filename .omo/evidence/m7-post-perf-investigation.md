# M7 post-performance investigation

## Invocation

The supplied release timing artifact is `/tmp/vf_m7_timings.json`, generated for the M7 TAAU fixture at 2560x1440 with render scale 0.72, `m5_air_high`, warmup 60, and 240 measured frames.

## Observations

| Observable | p95 |
| --- | ---: |
| CPU frame | 16.915 ms |
| exposure | 12.162 ms |
| atmosphere | 12.174 ms |
| bloom | 1.590 ms |
| taau_tonemap | 3.841 ms |
| total_gpu | 70.090 ms |

`src/render/m7_effects/graph.rs` shows exposure as one fullscreen sample, atmosphere as two color samples plus depth, bloom as a six-level down/up chain, and TAAU as the required 16-tap Catmull-Rom reconstruction plus 3x3 neighborhood. The exposure/atmosphere/cloud/glass samples all clustering around 11-12 ms while CPU p95 is 16.915 ms and total GPU p95 is 70.090 ms is inconsistent with the shader work and indicates pass timestamp/queue timing contamination rather than an isolated M7 shader bottleneck.

## Verification status

No M7 shader/resource edit was made: changing the required 16-tap TAAU, six-level bloom, or HDR exposure/atmosphere contracts would be a quality regression, and the timing implementation is outside this assignment's allowed files. A fresh focused release run was blocked by a concurrent compile error: `src/render/renderer/api.rs:130` calls missing `UiRenderer::apply_hand_override`.
