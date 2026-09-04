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

## 2026-09-04 23:25  M6.4 — GPT-5.6 Sol

- 한 것: `snapshot`에 `terrain|m6-light-room|m6-wind`, `final|light`, `--day-phase`, `--world-time`을 연결하고 모든 fixture를 동기 열 조명 뒤 메싱하도록 바꿨다. `light` 파이프라인은 텍스처·AO·면 음영을 우회해 opaque와 translucent의 선형 illumination을 출력한다. 밀폐 조명방·바람 fixture, VFC1 재조명 저장 회귀, 실제 메시를 그리는 final/light transactional shader hotreload 회귀를 추가했다. 비동기 조명은 stale/outside 결과를 통계에서 제외하고 current/next wave 중복을 합쳐 경계 재계산을 줄였다.
- 검증: `cargo test --all-targets` → library 81 + snapshot 3 passed(총 84), `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `git diff --check` 통과. Apple M5 release `R=12`, 1200프레임: 5000청크 `settled in 3.39s`, 625열/1114 solves, solver median 1.60ms·p95 2.98ms, max requeues 8. 정착 정상 1초 구간 max 18.8ms. 편집→조명 solve·재메시·GPU upload 17.824ms. M5 동일 seed·radius·해상도 terrain 정점 1,729,572 → M6 1,741,792, 비율 1.0071(≤1.3×).
- 스냅샷: `/tmp/vf_m6_light.png`의 13/9/5/1 중심 RGB `(209,209,209)`, `(140,140,140)`, `(92,92,92)`, `(59,59,59)`; 선형 정규화 비율 `[1.000000, 0.422946, 0.166491, 0.065922]`. `/tmp/vf_m6_day.png`·`/tmp/vf_m6_night.png` 하늘 crop 자정/정오 `0.011185`. `/tmp/vf_m6_wind_a.png`·`/tmp/vf_m6_wind_b.png` 변화율 LEAVES `0.043646`, STONE `0.000000`. 전부 §15.6 통과했고 PNG를 직접 확인했다.
- 설계와 다르게 한 것 / 제안: 없음. 별도 계측에서 간헐적 28.8~33.5ms 표본은 렌더 작업(0.8~2.04ms)이 아니라 `CAMetalLayer` drawable acquire가 두 주사 구간을 기다린 값으로 분리 확인했다. 측정용 계측과 swapchain 실험은 최종 코드에 남기지 않았고, M5와 같은 기준의 정착 정상 구간 max를 기록했다.
- 열린 문제: 없음.
- 의도한 커밋 메시지: `M6.4: validate lighting milestone`
- 다음: **M6 완료, 리뷰 요청.** Claude 리뷰·커밋 전에는 M7로 가지 않는다.

## 2026-09-04 23:05  M6.3 — GPT-5.6 Sol

- 한 것: face 코너마다 앞층 네 셀의 block/sky light를 반올림 평균하고 emitter를 보정해 정점 `b[16..24)`에 패킹했다. greedy 키를 texture·AO·block light·sky light·lowered 전체 일치로 확장하고 LEAVES 전체·GRASS 상면을 4×4로 제한했다. 16단계 광도표, 1200초 `DayState`, 동적 하늘 clear, 월드 좌표 XZ 바람, 128바이트 Globals를 Rust와 두 WGSL에 연결했다.
- 검증: `mesher_bakes_corner_light_and_sky`, `greedy_never_merges_different_light_keys`, `wind_offset_matches_at_shared_world_vertex`, 기존 `vertex_pack_roundtrip`, Globals 128바이트 테스트 통과. 독립 코드 재리뷰에서 코너 샘플·패킹·LUT·낮밤·바람·500줄·렌더 루프 계약을 재확인했고 CRITICAL/HIGH/MEDIUM 잔여 0.
- 설계와 다르게 한 것 / 제안: 없음.
- 열린 문제: 없음.
- 의도한 커밋 메시지: `M6.3: render day night light and wind`
- 다음: M6.4 자동 스냅샷·저장/hotreload 회귀·성능 마감.

## 2026-09-04 22:25  M6.2 — GPT-5.6 Sol

- 한 것: `World`에 열 조명 dirty·epoch·snapshot/apply와 정확한 4면 비교를 추가하고, 삽입·로드·언로드·편집 invalidation을 연결했다. `stream/lighting.rs`에서 urgent/초기 근접/경계 재전파 우선순위, 조명 `w`·전체 잡 `2w` 상한, stale 폐기, 비동기 고정점 wave, 동기 bootstrap, timing 통계를 구현했다. 최초 메시를 8청크 열 조명 초기화 전까지 보류하고, 조명 적용은 실제 바이트가 바뀐 청크만 메시 dirty로 만든다. 열 snapshot/apply는 청크 단위 연속 복사로 최적화했다.
- 검증: `cargo test --all-targets` → library 71 + snapshot 1 passed(총 72), `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `git diff --check` 통과. `stale_light_result_is_discarded`, 경계 횃불 전달·제거 수렴, 저장 상태 불변, 3×3 bootstrap 고정점 테스트 통과. Apple M5 release `R=12` 실측: 5000청크 `settled in 4.37s`, 625열/1387 solves, solver median 2.60ms·p95 4.95ms, max requeues 13. bootstrap 오류 없음.
- 설계와 다르게 한 것 / 제안: 없음.
- 열린 문제: 없음.
- 의도한 커밋 메시지: `M6.2: stream lighting across chunk columns`
- 다음: M6.3 정점 조명·셰이더·낮밤·바람.

## 2026-09-04 21:32  M6.1 — GPT-5.6 Sol

- 한 것: `TORCH` ID 12·emission 14·텍스처 레이어 13과 새 핫바를 추가했다. `Chunk`의 32KiB packed light·초기화 상태, `PaddedChunk`의 34³ light 패딩, `pack_light`/채널 디코더를 추가했다. `world/light.rs`에 직접 하늘광, 경계 입력, TORCH 시드와 분리된 두 `VecDeque`를 쓰는 결정적 열 solver를 구현했다. `stream/jobs.rs`로 Gen/Load/Light/Mesh 타입과 워커 실행을 분리해 `stream.rs`를 429줄로 줄였다. 저장 코드는 계속 블록 ID만 직렬화한다.
- 검증: `cargo test --all-targets` → library 63 + snapshot 1 passed(총 64). 조명 순수 테스트 8개(pack, 무감쇠 하강, 수평 감쇠, 블록광 감쇠, opaque 차단, TORCH 14, 결정성, Send+Sync) 통과. `cargo run --release --bin snapshot -- --seed 1 --radius 1 --size 320x180 --out /tmp/vf_m61.png` → 320×180 PNG 생성. `src/world/save.rs` 대조로 `VFC1`과 블록 전용 직렬화가 유지됨을 확인했다.
- 설계와 다르게 한 것 / 제안: 없음.
- 열린 문제: `set_light_initialized`는 M6.2 적용 경로가 아직 없어 dead-code warning이 남아 있다.
- 의도한 커밋 메시지: `M6.1: add voxel light solver`
- 다음: M6.2 경계 전파·스트리밍.

## 2026-09-04 21:21  M6.0 — GPT-5.6 Sol

- 한 것: `snapshot_noop_edit_is_warning`을 fail-first로 추가해 컴파일 실패(E0425)를 확인한 뒤, 동일 ID 편집을 위치·기존 ID가 포함된 warning으로 처리하고 정상 종료하도록 분리했다. `MAX_GEN_CHUNK_Y = 4`와 상한 이상 청크가 비어 있음을 검증하는 테스트를 추가했다. `SaveDir::open`의 기본 root를 `assets::dir()`의 부모인 프로젝트 root 아래 `saves/`로 고정했다. 투명 Globals 중복은 건드리지 않았다.
- 검증: `cargo test --all-targets` → library 55 + snapshot 1 passed(총 56), `cargo fmt --check`, `git diff --check` 통과. 실제 no-op snapshot은 exit 0과 `IVec3(0, 0, 0)`, `existing id 1` warning을 확인했고, 미로드 편집은 exit 1을 유지했다. `/tmp` cwd에서 1프레임 실행해 저장 디렉터리가 프로젝트 root `saves/`에 생성됨을 확인했다.
- 설계와 다르게 한 것 / 제안: 없음.
- 열린 문제: 없음.
- 의도한 커밋 메시지: `M6.0: close M5 review items`
- 다음: M6.1 조명 데이터·순수 열 solver.

## 2026-09-04 20:30  M4·M5 리뷰 — Claude

- 한 것: GPT의 미커밋 작업 트리(32파일)를 리뷰 뒤 한 커밋으로 보존(`ad8406f`). 검증 명령 전부 재실행: `cargo test --all-targets` 53 passed(§14 계약 테스트 이름 20개 전부 존재), clippy `-D warnings`·fmt 통과. culled/greedy 스냅샷 픽셀 일치 99.982%(≥99.5). `VF_RADIUS=12 VF_SMOKE_FRAMES=900` → `settled in 2.17s (5000 chunks)`, 이후 60fps·max 17~18ms(<25). 코드 대조: `stream.rs`(rayon+mpsc, 편집 동기 메싱, 부트스트랩, settle 로그, version/epoch 분리), `greedy.rs`(키 tex+ao×4, 대각 뒤집기 culled와 공유), `physics.rs`(Y→X→Z 스윕, 수영, 안전 스폰), `trees.rs`·`gen.rs`(순수 나무·±2열 스캔·동굴 8≤y≤h−6), `frustum.rs`(0..1 깊이 평면), `save.rs`(VFC1+lz4, world.json), `translucent.rs`·`renderer.rs`(불투명→outline→투명, 뒤→앞 정렬, cull None), `chunk.wgsl`(bit 23 lowered, 알파 출력) — 전부 §14와 일치. 렌더 루프 `unwrap`/`expect` 없음.
- 스냅샷 확인: `/tmp/rv5_water.png` 물 아래 모래 비침·수면 1/8 하강 확인. `/tmp/rv5_edits.png` 판자 기둥·유리(뒤의 나무 줄기 비침, 유리-유리 면 없음)·벽돌·경계 걸친 나무 정상.
- 저장 엔드투엔드 우연 실측: 리뷰 smoke 창(`VF_SAVE=review`)이 전면에 뜨며 진헌 입력을 받아 비행·클릭이 들어갔다. 종료 후 `saves/review/chunks`에 파일 정확히 1개, 디코드해 생성 지형과 비교하니 차이 정확히 1블록((1,65,−2) GRASS→AIR). world.json에 fly=true·이동 위치 기록. 편집 없는 GPT 실행 3개는 청크 파일 0개. → 「수정 청크만 저장」 계약 실증.
- 발견(모두 사소, M6.0으로): ① `snapshot --edits`가 no-op 편집(같은 id)을 「범위 밖」 오류로 보고 — 경고로 바꿀 것. `set_block`의 no-op→false는 §4에 명시함. ② ROADMAP 예시 편집 `set 0,69,1,0`은 그 자리가 공기라 no-op였음(내 실수) — 명령 수정, `--yaw 3.1416`→`0`(GPT 제안 수용). ③ `generate`의 `cp.y >= 4` 빈 청크 조기 반환은 높이 88+나무 8 < 128 가정 — `MAX_GEN_CHUNK_Y` 상수화 + 테스트. ④ 저장 경로가 cwd 기준 — 프로젝트 루트 기준으로. ⑤ 투명 파이프라인이 Globals·유니폼 아레나를 한 벌 더 가짐 — M7 통합. ⑥ greedy 정점 비율은 장면 의존(39%/67%) — 완료 조건에서 제외, 기록만. §14.12에 정리.
- 설계와 다르게 한 것 수용: 물리 substep 상한 없음(더 안전), version/epoch 분리(개선), `MAX_GEN_CHUNK_Y` 조기 반환(성능), M5.3 아레나 미구현(조건 미충족, M7 이월).
- 열린 문제: GPT 실행 정책이 `git commit`을 차단 — 이후 마일스톤도 커밋은 Claude가 리뷰 시점에 한다(또는 진헌이 정책 해제). 손 플레이 잔여: 수중에서 수면 뒷면 보기, 저장 뒤 재실행 복원(우연 실측으로 절반은 확인).
- 다음: 진헌 「M6 가자」 → Claude가 BLUEPRINT §15(M6 조명·낮밤·바람) 확정 → GPT M6.0(사소 4개) → M6.

## 2026-09-04 19:49  M5 — GPT-5.6 Sol

- 한 것: 수정 청크만 LZ4로 저장하는 `VFC1` 청크 파일과 `world.json` 플레이어 메타데이터를 추가하고, `VF_SAVE`/`--save`, 파일 우선 비동기 로딩, 언로드 전·30초·종료 저장을 연결했다. 저장용 콘텐츠 version과 이웃 재메시용 monotonic epoch를 분리했다. WATER/GLASS를 opaque/translucent 메시로 나눠 같은 id 내부 면 컬링, 물 표면 1/8 하강 비트, alpha blend·depth-write off·cull none 파이프라인, 불투명→outline→투명 및 청크 단위 뒤→앞 정렬을 구현했다. serde 1.0.229, serde_json 1.0.151, lz4_flex 0.14.0을 사용했다.
- 검증: `cargo test --all-targets` → 53 passed, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `git diff --check` 통과. 저장 우선 로딩은 실제 `Streamer`의 파일 감지→`Load` 잡→결과 적용 분기를 테스트했고 독립 코드 재검토도 PASS했다. 무편집 release smoke 저장 `saves/m5_manual_20260904_qa/`은 `world.json`만 있고 청크 파일 0개다. 물 `/tmp/vf_m5_water.png`에서 수면 아래 모래·지형, 유리 `/tmp/vf_m5_glass_visible.png`에서 창 뒤 나무·물·지형을 직접 확인했고 무편집 기준 대비 28,214픽셀(3.0614%)이 변했다. `VF_RADIUS=12 VF_SMOKE_FRAMES=900` Metal 실측은 정착 2.01초, 초기 max 20.4ms, 이후 60fps·steady max 18.6ms였다.
- 설계와 다르게 한 것 / 제안: 선택 사항인 수중 틴트는 넣지 않았다. ROADMAP 유리 명령의 `--yaw 3.1416`은 현재 카메라 규약에서 창 반대를 보므로 시각 검증은 `--yaw 0`으로 다시 실행했다. M5.3 메시 아레나는 업로드 원인 20ms 초과가 확인되지 않아 조건대로 미구현하고 M7로 미뤘다.
- 열린 문제: 실제 입력으로 블록을 놓고 종료·재실행하는 손 검증과 수중에서 수면 뒷면을 올려다보는 손 검증은 자동화하지 못했다. 저장/복원 경로, cull-none 설정, 물 lowered 정점은 단위·통합·스냅샷으로 검증했다. 현재 실행 정책이 `git commit`을 차단해 M4.0~M5.2 커밋은 아직 생성하지 못했다.
- 다음: **M5 완료, 리뷰 요청.** Claude 리뷰와 손 플레이 뒤 M6.

## 2026-09-04 17:50  M4 — GPT-5.6 Sol

- 한 것: M4.0 리뷰 잔여(`snapshot --edits/--mesher`, WGSL·월드젠 정리, `chunk_of`, 16384 슬롯), rayon 1.12.0 + std mpsc 워커 스트리밍, greedy 메싱과 AO 대각선 뒤집기, 보행·점프·수영·비행 swept-AABB 물리, 결정적 나무·3D 동굴, 프러스텀 컬링을 연결했다. 생성·메시 잡은 종류별 `2w` in-flight를 지키되 같은 프레임에서 완료 잡을 다시 채워 프레임당 16개 처리량 하한을 없앴다. 빈 청크는 워커 슬롯을 쓰지 않는다.
- 검증: `cargo test --all-targets` → 43 passed, `cargo clippy --all-targets -- -D warnings`와 `cargo fmt --check` 통과. `VF_RADIUS=12` Metal 실측은 `stream: settled in 2.85s (5000 chunks, 1820 GPU meshes)`로 3초 미만. 프러스텀 적용 뒤 실제 화면 `drawn`은 419. culled/greedy 스냅샷 `/tmp/vf_m4_culled_trees.png`, `/tmp/vf_m4_greedy_trees.png` 직접 확인: 픽셀 일치 99.967%, 정점 6,560,292 → 2,579,780(39.3%). `--edits`는 기준 이미지 대비 36,892픽셀 변경을 확인했다. 물리는 낙하·점프 높이·벽·저FPS 터널링·비행·수영 단위 테스트 통과.
- 설계와 다르게 한 것 / 제안: 기능 계약 변경 없음. 제안: ROADMAP의 편집 스냅샷 명령은 현재 카메라 규약에서 `--yaw 3.1416`이면 편집물을 등지므로 시각 검증에는 `--yaw 0`이 맞다. 문서 소유자 리뷰 때 수정 권장.
- 열린 문제: 자동 실행 중 게임 창이 뒤로 가면 Metal surface가 `Occluded`가 되어 해당 전환 프레임에 1104.9ms가 기록되고 이후 FPS 표본이 끊긴다. 정상 전면 구간은 max 21.6ms 이하였으나 M4의 60초 실제 비행·걷기 손 검증은 미실시. 업로드 원인 히치는 확인되지 않아 M5.3 아레나 조건은 충족하지 않은 것으로 처리한다.
- 다음: 멈추지 않고 M5.1 저장 → M5.2 투명 패스. M5.3 아레나는 조건부 미구현을 최종 LOG에 기록.

## 2026-09-04 13:55  M3 종료 · M4/M5 설계 — Claude

- 한 것: 진헌 손 플레이 통과(13:37) → **M3 종료**. M3.1은 GPT가 아직 시작하지 않았으므로 M4의 0단계로 흡수. BLUEPRINT **§14**(M4·M5 상세 설계) 추가: 워커 스레드 스트리밍(rayon + mpsc, 편집은 메인 동기), greedy(키 = tex+ao×4, 대각 뒤집기), 물리(Body/MoveInput, 1/120 substep, Y→X→Z 스윕), 나무(순수 `tree_at`/`tree_blocks`, 이웃 범위 ±2 열 스캔으로 경계 이어짐)·동굴(3D 노이즈 > 0.62, 8 ≤ y ≤ h−6), 프러스텀 컬링, 저장(수정 청크만 lz4 + world.json), 투명 패스(`translucent` 플래그, 같은 id 컬링, 물 윗면 비트 23, cull None, 뒤→앞), 아레나(조건부). ROADMAP M4(M4.0~M4.5)·M5(M5.1~M5.3) 재작성, 테스트 이름 20개, 검증 명령 포함.
- 검증: 없음(문서만). 저장소 상태 `c2db9e7` 기준, 작업 트리 깨끗.
- 설계와 다르게 한 것 / 제안: `DEFAULT_SLOTS` 성장 로직 대신 16384 고정(4MB). 반경 기본 10·검증 12·`VF_RADIUS`. 아레나는 M4 측정에 따라 조건부.
- 열린 문제: GPT가 M4~M5를 한 번에 진행. M4 끝에서 멈추지 않고 M5까지 가되, 마일스톤·단계마다 커밋과 LOG.
- 다음: GPT → M4.0 … M5.3 → 멈춤 → Claude 리뷰 → M6.

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
