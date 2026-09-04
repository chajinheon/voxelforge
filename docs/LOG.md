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

## 2026-09-04 13:40  M3 리뷰 — Claude

- 한 것: 리뷰 체크리스트 5항목 수행. 검증 명령 전부 재실행(테스트 25 passed, clippy `-D warnings` 통과, fmt 통과, 스냅샷 `/tmp/rv_m3_default.png` 190청크). 계약 대조: `vertex.rs` 비트 레이아웃, `coords.rs` 면 코너 표 6개, `chunk.wgsl` 디코드·UV·음영, `gen.rs` 층 규칙, `raycast.rs` DDA·법선, `world.rs` dirty 전파, `chunk_pipeline.rs` dynamic offset·에러 스코프, `window_gpu.rs` `CurrentSurfaceTexture` 7변형 — 전부 일치. 렌더 루프에 `unwrap`/`expect` 없음(테스트 제외).
- **결함 1 (수정함)**: 메셔 AO가 가리는 블록을 `bp + s`(블록 자신의 층)에서 읽어 평지 전체가 ao=0. 스냅샷 픽셀 실측 (61,103,34) = 기준색 (95,159,53)의 선형 밝기 0.35배. 원인의 절반은 BLUEPRINT §4가 「이웃」의 층을 명시하지 않은 것. 수정: `neighbor(=bp+normal) + s`. 테스트 `mesher_ao_reads_layer_in_front_of_face` 추가(동일 평면 이웃은 3, 대각 위 블록은 해당 코너만 2, 평지 3). 수정 후 실측 (91,149,54). 커밋 `d4d7e27`. §4·§8 갱신.
- **결함 2 (GPT에 요청, M3.1)**: 초기 로딩. GPT 분석이 맞다 — 개수 예산 4/프레임은 하한 5.63초. 결정: 개수 예산 폐기, **시간 예산 ≤10ms/프레임** + 빈 청크 메싱 생략 + 스폰 반경 1 동기 부트스트랩 + `stream: settled` 로그. `<3초` 계약은 이 로그 기준으로 유지, 단일 스레드로 미달이면 M4에서 확정. §5·§7·§12 갱신.
- 소소한 것(M3.1): `snapshot --edits` 미구현(선택이었지만 손 플레이 대체 수단이라 필수로 올림), `chunk.wgsl` 더미 연산, `gen.rs` `let _ = self.seed`와 `Chunk` 내부 직접 쓰기, `origin / CHUNK_SIZE`. `DEFAULT_SLOTS 4096`은 M4 R=12(5000청크)에서 부족 — §12에 적음. glam 0.33.1이 `perspective_rh`/`look_to_rh`를 deprecated — `#[allow(deprecated)]` 유지 결정, D4에 기록.
- 잘된 것: 렌더러가 창을 모른다(D16 의도대로 스냅샷과 공유). 핫리로드 에러 스코프 처리. `padded()` 27회 청크 조회 최적화. 500줄 규칙 지킴(`window_gpu.rs` 분리).
- 다음: GPT → M3.1(ROADMAP 참조) → 진헌 손 플레이 → M3 종료 → M4.

## 2026-09-04 13:04  M3 — GPT-5.6 Sol

- 한 것: Amanatides–Woo DDA 레이캐스트, 12모서리 조준 outline, LMB 부수기/RMB 놓기, 물 통과·물 위치 대체, 플레이어 AABB 겹침 거절, 1~9·휠 핫바, F3 통계, 편집 청크·경계 이웃 우선 재메싱을 구현했다. `main.rs` 500줄 제한을 지키기 위해 창/서피스 보조 코드를 `window_gpu.rs`로 분리했다. 측정된 로딩 히치를 줄이려고 월드젠 높이를 열당 1회 계산하고 패딩의 청크 조회를 최대 27회로 줄였다.
- 검증: `cargo test` → 25 passed(요구된 M3 테스트 6개 포함). `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` 통과. Metal `VF_SMOKE_FRAMES=900 cargo run --release` → 로딩 후 60fps, steady max 17~21ms, 1352 chunks/371 drawn/meshq 0. `/tmp/vf_m3.png`와 최적화 후 `/tmp/vf_m3_optimized.png`는 1280×720 PNG이며 SHA-256 `89a85b58ba936f34d144ca845923e25aade283be93b5ab38ed862290c6a7ca04`로 픽셀 동일. 실행 중 `outline.wgsl` 정상 수정은 1초 안에 재로드됐고 문법 오류는 거절한 채 프로세스·이전 파이프라인을 유지했으며 원복 재로드도 확인했다. 독립 M3 재리뷰와 성능 편집 감사 PASS.
- 설계와 다르게 한 것 / 제안: 원안은 유지했다. 제안: R=6×수직 8 = 1352청크를 생성 ≤4/프레임으로 제한하면 60fps에서도 하한이 338프레임=5.63초라 §7의 초기 로딩 `<3초`와 양립하지 않는다. 실제 가시 창 측정도 완전 로딩 약 7초였다. 리뷰에서 초기 부트스트랩 예산을 별도로 늘리거나 `<3초` 기준을 바꾸는 결정을 BLUEPRINT에 반영해야 한다.
- 열린 문제: 실행 도구가 번들 없는 winit 창에 마우스/키보드 입력을 연결하지 못해 5×5 벽·유리창·나무 지붕의 실제 손 플레이는 자동화하지 못했다. 진헌/Claude 플레이 리뷰가 필요하다. 초기 로딩 `<3초`는 위 계약 충돌로 미충족이다.
- 다음: **M3에서 멈춤. M3 기능 완료, 리뷰 요청. M4로 가지 않는다.**

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
