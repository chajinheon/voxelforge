# voxelforge 세션 로그

새 항목은 **맨 위**에 추가한다. 형식:

```
## YYYY-MM-DD HH:MM  M<n> — <에이전트>
- 한 것:
- 검증: (실행한 명령과 결과 요약. 테스트 수, 스냅샷 경로)
- 설계와 다르게 한 것 / 제안: (없으면 "없음")
- 열린 문제:
- 다음:
```

---

## 2026-09-04 12:25  M2 — GPT-5.6 Sol

- 한 것: 비행 `Camera`/`Controller`, 마우스 시선과 커서 잠금, WASD·상하 이동·스프린트, 플레이어 중심 R=6 스트리밍, 프레임당 생성 4/메시 8 예산, R+2 언로드와 경계 이웃 dirty 전파, GPU 메시 제거·교체, 1초 창 제목 통계를 구현했다. `cargo run`이 게임 바이너리를 고르도록 `default-run`을 지정했다.
- 검증: `cargo test` → 17 passed. `cargo clippy --all-targets -- -D warnings` 통과. `cargo run --release --bin snapshot -- --pos 100,90,-60 --yaw 2.4 --pitch -0.35 --out /tmp/vf_m2.png` → Metal, 1280×720 PNG 생성·확인. `VF_SMOKE_FRAMES=600 cargo run --release --bin voxelforge` → 약 10초/600프레임(60fps), R=6 전체 로딩에 필요한 338프레임을 넘겨 패닉 없이 종료. 문서 그대로 `VF_SMOKE_FRAMES=60 cargo run --release`도 Metal 60프레임 통과했다. 별도 무제한 실행은 1분 이상 안정 동작 후 종료했다.
- 설계와 다르게 한 것 / 제안: 없음.
- 열린 문제: 실행 도구가 번들 없는 winit 창에 접근성 입력을 연결하지 못해 실제 마우스/키보드 5분 비행은 자동화하지 못했다. 컨트롤러 단위 테스트와 이벤트 배선 검토는 통과했으며 최종 M3 플레이 리뷰에서 함께 확인한다.
- 다음: M3 DDA 레이캐스트·outline·부수기/놓기·핫바.

## 2026-09-04 12:05  M1 — GPT-5.6 Sol

- 한 것: 좌표·블록·청크·월드젠·월드 dirty 관리, 34³ 패딩 기반 culled 메셔와 정점 AO/8바이트 패킹, 절차적 16×16 텍스처 배열, dynamic-offset 청크 유니폼 렌더러, WGSL 핫리로드, 창 없는 오프스크린 readback, 고정 카메라 창 앱과 `snapshot` 바이너리를 구현했다. 독립 리뷰에서 발견된 AO 코너 방향 반전을 수정했다.
- 검증: `cargo test` → 10 passed. `cargo clippy --all-targets -- -D warnings` 통과. `cargo run --release --bin snapshot -- --seed 1 --radius 4 --out /tmp/vf_m1.png` → Metal, 1280×720 PNG 생성(`/tmp/vf_m1.png`, 438 KiB), 지형·물·하늘·면 방향·깊이 렌더 확인. `VF_SMOKE_FRAMES=60 cargo run --release --bin voxelforge` → Metal 창 60프레임 종료, 패닉 없음. 임시 오프스크린 프로브로 정상 WGSL 재로드 성공, 문법 오류 거절, 오류 뒤 이전 파이프라인 렌더 성공을 확인했고 프로브는 제거했다.
- 설계와 다르게 한 것 / 제안: 없음. Rust 2024 예약어 때문에 `world/gen.rs` 모듈 경로는 `world::r#gen` 표기를 사용한다.
- 열린 문제: 없음.
- 다음: M2 자유 카메라·프레임 예산 스트리밍·창 제목 통계.

## 2026-09-04 11:30  M0 — Claude

- 한 것: `~/Projects/voxelforge` 생성(git init). Cargo 프로젝트, 의존성 실버전 고정(wgpu 30.0.1, winit 0.30.13, glam 0.33.6, bytemuck 1.25.2, pollster 1.0.1, image 0.25.10, fastnoise-lite 1.1.1, anyhow, log, env_logger 0.11.11). `cargo fetch` 완료. M0 스모크 `src/main.rs`(창 + 클리어 + 어댑터 로그 + `VF_SMOKE_FRAMES` 자동 종료). `AGENTS.md`, `docs/BLUEPRINT.md`, `docs/ROADMAP.md`, `docs/KICKOFF.md` 작성.
- 검증: `cargo build` 통과. `VF_SMOKE_FRAMES=3 cargo run` → `adapter: Apple M5 | backend: Metal`, `surface: Bgra8UnormSrgb 2560x1440 (scale 2)`, `smoke: rendered 3 frames, exiting`.
- wgpu 30에서 기억과 다른 API를 소스로 확인해 BLUEPRINT §11에 기록: `Instance::default()`, `RequestAdapterOptions.apply_limit_buckets`, `get_current_texture() -> CurrentSurfaceTexture`(7변형, `Validation` 포함), `queue.present(frame)`, `RenderPassDescriptor.multiview_mask`, `RenderPassColorAttachment.depth_slice`, `VertexState.buffers: &[Option<_>]`, `DepthStencilState.depth_write_enabled: Option<bool>`.
- 설계와 다르게 한 것 / 제안: 없음(설계 원본).
- 열린 문제: winit 0.31은 베타라 0.30 유지. Godot 미설치·미사용(D1). `cargo`가 `~/.cargo`에 쓰므로 샌드박스 밖 실행이 필요할 수 있음.
- 다음: GPT-5.6 Sol이 `docs/KICKOFF.md`로 시작 → M1 → M2 → M3, M3 뒤 멈추고 Claude 리뷰.
