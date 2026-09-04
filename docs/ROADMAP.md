# voxelforge 로드맵

각 마일스톤은 **완료 조건**과 **검증 명령**을 갖는다. 검증 명령은 지금 시점에 실패해야 정상이고(fail-first), 마일스톤이 끝나면 통과해야 한다. 순서를 건너뛰지 않는다.

시간 추정은 GPT-5.6 Sol 기준. 진헌의 플레이 확인은 별도.

---

## M0 — 스캐폴드 (완료, 2026-09-04, Claude)

- Cargo 프로젝트, 의존성 실버전 고정, wgpu 30 API 검증, 창 + 클리어 + 어댑터 로그.
- 검증(통과함):
  ```bash
  cargo build
  VF_SMOKE_FRAMES=3 cargo run     # 로그: adapter: Apple M5 | backend: Metal ... smoke: rendered 3 frames
  ```

## M1 — 정적 지형 렌더 (≈2~3h) 

**만드는 것**: `lib.rs` 모듈 구조(BLUEPRINT §2), `world/{block,coords,chunk,gen,world}`, `mesh/{vertex,mesher}`, `render/{gpu,globals,textures,chunk_pipeline,shader_watch,renderer,offscreen}`, `assets.rs`, `assets/shaders/chunk.wgsl`, `bin/snapshot.rs`. `main.rs`는 고정 카메라로 R=4 지형을 그린다(입력 없음).

**완료 조건**
- `cargo test`에 BLUEPRINT §8의 M1 테스트 10개가 존재하고 통과: `coords_floor_div_negative`, `chunk_index_roundtrip`, `chunk_set_get_and_non_air`, `vertex_pack_roundtrip`, `mesher_lone_block_has_6_faces`, `mesher_enclosed_block_has_0_faces`, `mesher_two_adjacent_blocks_10_faces`, `mesher_border_face_culled_by_padding`, `gen_is_deterministic_for_seed`, `gen_layers_match_height`.
- 스냅샷 PNG에 잔디(초록 윗면)·흙·돌·모래·물(파랑)과 하늘색 배경이 보인다. 뒤집힌 면·구멍·검은 면이 없다.
- `chunk.wgsl`을 저장하면 실행 중 게임이 1초 안에 반영한다(핫리로드). 문법 오류를 넣으면 로그에 에러가 찍히고 이전 화면이 유지된다.

**검증**
```bash
cargo test 2>&1 | tail -3                         # test result: ok. 10 passed
cargo run --release --bin snapshot -- --seed 1 --radius 4 --out /tmp/vf_m1.png && echo OK
# Claude가 /tmp/vf_m1.png를 열어 본다
VF_SMOKE_FRAMES=60 cargo run --release            # 60프레임 뒤 종료, 패닉 없음
```

## M2 — 자유 카메라·스트리밍 (≈1h)

**만드는 것**: `player/{camera,controller}`, 마우스 시선(커서 잠금), WASD/Space/Shift 비행, 스프린트, 플레이어 중심 스트리밍(R=6, 프레임 예산 gen≤4 mesh≤8, unload R+2), 창 제목 통계(1초).

**완료 조건**
- release로 5분 비행 중 프레임 최대치가 33ms를 넘지 않는다(제목의 `max`로 확인). 60fps.
- 멀리 갔다가 돌아와도 청크가 사라지거나 중복 생성되지 않는다(제목의 `chunks` 수가 R=6 범위에서 안정).
- ESC로 커서 해제, 다시 ESC로 종료.

**검증**
```bash
cargo run --release                                # 진헌/Claude 플레이 확인
cargo run --release --bin snapshot -- --pos 100,90,-60 --yaw 2.4 --pitch -0.35 --out /tmp/vf_m2.png
```

## M3 — 건축 (≈1~2h) — **완료** (기능 13:04 GPT, 리뷰 13:40 Claude, 손 플레이 13:37 진헌 통과)

**만드는 것**: `world/raycast.rs`(DDA), `render/outline.rs` + `outline.wgsl`, LMB 부수기 / RMB 놓기, `HOTBAR` 1~9 + 휠, 플레이어 AABB 겹침 거절, 편집 시 청크 + 경계 이웃 재메싱.

**완료 조건**
- 테스트 6개 추가 통과: `raycast_hits_block_in_front`, `raycast_face_normal_is_toward_origin`, `raycast_misses_when_only_air`, `raycast_passes_through_water`, `set_block_marks_neighbor_dirty_on_border`, `place_rejected_inside_player_aabb`.
- 조준 블록에 검은 와이어프레임이 뜨고, 6칸 밖은 뜨지 않는다.
- 청크 경계에 있는 블록을 부수면 이웃 청크 쪽 면이 즉시 나타난다(구멍 없음).
- 5×5 벽 + 유리창 + 나무 지붕을 실제로 지을 수 있다. 부수기/놓기 반응이 한 프레임 안이다.
- `snapshot --edits`(선택): `--edits "set 0,70,0,8; set 0,71,0,8"` 형식으로 편집을 적용한 뒤 렌더 → 회귀 확인용.

**검증**
```bash
cargo test 2>&1 | tail -3                          # 16 passed
cargo clippy --all-targets 2>&1 | grep -c warning  # 0
cargo run --release                                # 실제로 짓는다
```

**M3가 끝나면 멈추고 LOG에 기록 → Claude 리뷰.** 리뷰 전엔 M4로 가지 않는다.

## M3.1 — 리뷰 수정 → **M4의 0단계로 흡수** (2026-09-04 13:50)

2026-09-04 Claude 리뷰 결과. AO 결함은 Claude가 직접 고쳐 커밋했다(`d4d7e27`). 아래 5개는 M4를 시작할 때 먼저 처리한다(별도 커밋 `M4.0: review fixes`). 1번의 「시간 예산」은 M4의 워커 스레드 스트리밍(BLUEPRINT §14.2)으로 대체된다 — 개수 예산만 없애면 된다.

**만드는 것**
1. 스트리밍 예산을 개수(4/8)에서 **시간 예산 ≤10ms/프레임**으로 교체(BLUEPRINT §5). 첫 프레임 전 스폰 반경 1 동기 로딩·메싱. `is_empty()` 청크 메싱 생략. `stream: settled in X.XXs (N chunks)` 로그 1회.
2. `snapshot --edits "set x,y,z,id; set …"` 옵션(모든 청크 로딩 후 적용, dirty 재메싱 뒤 렌더). 손 플레이를 자동화할 수 없으니 이게 건축 회귀 테스트다.
3. `chunk.wgsl`의 `packed_light * 0.0` / `sky * 0u` 우회 제거 — naga는 미사용 변수를 경고하지 않는다. `light`/`sky`는 디코드만 남기거나 M6까지 주석 처리.
4. `gen.rs`: `let _ = self.seed;` 제거(필드를 쓰거나 지운다), `chunk.blocks[..] = id; chunk.non_air += 1` 직접 쓰기 대신 `Chunk::set` 또는 전용 생성자 사용.
5. `main.rs` `remove_unloaded_gpu_chunks`: `origin / CHUNK_SIZE` → `chunk_of(origin)`(지금은 32의 배수라 우연히 맞음).

**완료 조건**: M4의 완료 조건에 통합했다(아래). 손 플레이는 13:37에 이미 통과.

---

## M4 — 성능·물리 (≈3~4h) — **완료** (GPT 17:50, 리뷰 통과 Claude 20:30, 커밋 `ad8406f`)

설계는 BLUEPRINT §14.1~14.7. 순서대로, 각 단계마다 커밋.

**M4.0 리뷰 잔여** (§14.7): `snapshot --edits`, `--mesher culled|greedy`, wgsl 더미 제거, gen.rs 정리, `chunk_of`, `DEFAULT_SLOTS=16384`. 커밋 `M4.0: review fixes`.

**M4.1 워커 스레드 스트리밍** (§14.2): `cargo add rayon`(실버전 LOG에 기록). `src/stream.rs`로 스트리밍 분리, `World::{is_loaded, insert_generated, chunk_version, generator}`, 편집 청크는 메인 동기 메싱, 부트스트랩 반경 1, `stream: settled` 로그, `VF_RADIUS`. 커밋 `M4.1: threaded streaming`.

**M4.2 greedy** (§14.3): `src/mesh/greedy.rs`, 대각 뒤집기(culled에도), 게임·스냅샷 기본 greedy. 커밋 `M4.2: greedy meshing`.

**M4.3 물리** (§14.4): `src/player/physics.rs`, `Controller → MoveInput`, F 비행 토글, 스폰 안전. 커밋 `M4.3: walking physics`.

**M4.4 월드젠** (§14.5): `src/world/trees.rs`, 동굴. 커밋 `M4.4: trees and caves`.

**M4.5 프러스텀 컬링** (§14.6). 커밋 `M4.5: frustum culling`.

**완료 조건**
- 테스트 추가 통과: `greedy_quad_area_equals_culled_face_count`, `greedy_never_merges_different_keys`, `greedy_flat_slab_top_is_one_quad`, `physics_falls_and_lands_on_ground`, `physics_jump_height_about_1_25`, `physics_wall_blocks_horizontal_motion`, `physics_no_tunneling_at_low_fps`, `physics_fly_ignores_gravity`, `trees_agree_across_chunk_borders`, `caves_do_not_break_surface`, `frustum_culls_chunk_behind_camera`, `frustum_keeps_chunk_in_front`, `worldgen_is_send_sync`. 기존 25 + AO 1 유지 → **≥ 39 passed**.
- `VF_RADIUS=12 VF_SMOKE_FRAMES=900 cargo run --release` 로그: `stream: settled in X.XXs` **X < 3.0**, 이후 통계 60fps, `max` < 25ms.
- greedy/culled 스냅샷 비교: 동일 인자로 두 PNG를 뽑아 픽셀 일치율 ≥ 99.5% (아래 명령).
- greedy 정점 수: 스냅샷 로그에 총 정점 수를 찍는다(장면 의존 — 나무 장면 39%, 스폰 장면 67%. 완료 조건 아님, 기록만).
- 걷기: 스폰 후 땅에 서고, 점프 1블록 오르기, 벽에 막힘, 물에 들어가면 천천히 가라앉고 Space로 뜸. F로 비행.
- 나무가 청크 경계에서 잘리지 않는다(스냅샷으로 확인). 동굴 입구가 지표에 없다.
- `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`.

**검증**
```bash
cargo test 2>&1 | tail -3
VF_RADIUS=12 VF_SMOKE_FRAMES=900 cargo run --release 2>&1 | grep -E "settled|stats" | head -5
cargo run --release --bin snapshot -- --seed 1 --radius 4 --mesher culled --out /tmp/vf_m4_culled.png
cargo run --release --bin snapshot -- --seed 1 --radius 4 --mesher greedy --out /tmp/vf_m4_greedy.png
python3 -c "
from PIL import Image, ImageChops
a=Image.open('/tmp/vf_m4_culled.png').convert('RGB'); b=Image.open('/tmp/vf_m4_greedy.png').convert('RGB')
d=ImageChops.difference(a,b).convert('L'); n=sum(1 for p in d.getdata() if p>4); print('same %.3f%%' % (100*(1-n/(a.width*a.height))))"
cargo run --release --bin snapshot -- --seed 1 --radius 3 --pos 0,74,8 --yaw 0 --pitch -0.35 --edits "set 0,70,0,8; set 0,71,0,8; set 0,72,0,8; set 1,70,0,9; set 1,71,0,9; set -1,70,0,10" --out /tmp/vf_m4_edits.png
# (리뷰 수정: yaw 0이 편집물을 향한다. 원안의 `set 0,69,1,0`은 그 자리가 이미 공기라 no-op → set_block false)
```

M4가 끝나면 LOG에 기록하고 **멈추지 말고 M5로** 간다.

## M5 — 저장·투명 (≈2~3h) — **완료** (GPT 19:49, 리뷰 통과 Claude 20:30, 커밋 `ad8406f`)

설계는 BLUEPRINT §14.8~14.10.

**M5.1 저장** (§14.8): `cargo add serde --features derive`, `serde_json`, `lz4_flex`(실버전 LOG). `src/world/save.rs`, `World::{modified, saved_version}`, 언로드·30초·종료 시 저장, 로딩 시 파일 우선. `VF_SAVE`/`--save`. 커밋 `M5.1: save and load`.

**M5.2 투명 패스** (§14.9): `BlockDef.translucent`, `ChunkMeshes`, 같은 id 컬링, 물 윗면 `lowered` 비트, `src/render/translucent.rs`, 뒤→앞 정렬, 텍스처 알파, (선택) 수중 틴트. 커밋 `M5.2: translucent pass`.

**M5.3 아레나** (§14.10, 조건부): M4 측정에서 업로드 히치가 확인될 때만. 아니면 LOG에 「미구현, 측정값 …」.

**완료 조건**
- 테스트 추가 통과: `save_roundtrip_chunk_bytes_equal`, `world_loads_saved_chunk_instead_of_generating`, `unmodified_chunks_are_not_written`, `world_meta_roundtrip`, `mesher_splits_translucent_blocks`, `mesher_culls_faces_between_same_translucent_blocks`, `water_top_face_sets_lowered_flag` (+ 아레나 시 2개). **≥ 46 passed**.
- 블록을 놓고 종료 → 재실행 → 그 자리에 있다. `saves/default/chunks/`에 수정 청크 수만큼만 파일이 있다. 플레이어 위치·시선 복원.
- 물 아래 모래·돌이 비치고, 유리 뒤가 보이고, 물-물·유리-유리 사이 면이 없다. 물 표면이 1/8 낮다.
- 수중에서 위를 보면 물 표면 뒷면이 보인다(cull None).
- R=12 비행 60초 `max` < 25ms 유지(투명 패스 추가 후에도).

**검증**
```bash
cargo test 2>&1 | tail -3
cargo run --release --bin snapshot -- --seed 1 --radius 4 --pos 40,66,40 --yaw 2.0 --pitch -0.5 --out /tmp/vf_m5_water.png     # 해안: 물 아래가 비침
cargo run --release --bin snapshot -- --seed 1 --radius 3 --pos 0,74,8 --yaw 0 --pitch -0.35 --edits "set 0,70,0,9; set 0,71,0,9; set 1,70,0,9; set 1,71,0,9" --out /tmp/vf_m5_glass.png
VF_SMOKE_FRAMES=120 cargo run --release && ls saves/default/chunks | wc -l    # 편집 없으면 0
```

**M5가 끝나면 멈춘다.** LOG 기록 → 진헌에게 「M5 완료, 리뷰 요청」 → Claude 리뷰 → M6.

**리뷰 결과(20:30)**: 통과. M5.3 아레나는 조건 미충족(업로드 히치 없음, max 17~18ms)으로 M7로 이월. 소소한 수정 4개는 M6.0으로: `snapshot --edits` no-op은 경고로, `MAX_GEN_CHUNK_Y` 상수화 + 테스트, 저장 경로를 프로젝트 루트 기준으로, 투명 파이프라인의 Globals 중복은 M7에서.

## M6 — 마크식 조명·낮밤·바람

설계는 BLUEPRINT §15다.

아래 「커밋 메시지」는 Claude가 리뷰할 때 사용할 메시지다. Sol은 실행 정책상 `git commit`을 하지 않는다. 각 단계가 끝날 때 `docs/LOG.md` 맨 위에 별도 항목을 추가하고 의도한 커밋 메시지를 기록한다.

**M6.0 M5 리뷰 잔여**

만드는 것:

* `snapshot --edits`에서 같은 ID no-op은 오류가 아니라 warning.
* `MAX_GEN_CHUNK_Y = 4` 상수와 `max_gen_chunk_y_covers_generated_content`.
* 저장 기본 root를 `assets::dir()`의 부모인 프로젝트 root로 고정.
* 투명 Globals 중복은 건드리지 않고 M7.0 대상으로 남김.

커밋 메시지: `M6.0: close M5 review items`

완료 조건:

* no-op 편집 뒤 snapshot exit code 0.
* warning에 위치와 기존 ID가 포함.
* 범위 밖 편집은 여전히 non-zero exit.
* `cp.y >= MAX_GEN_CHUNK_Y` 생성 청크 0 non-air.
* cwd를 `/tmp`로 바꿔 실행해도 저장은 프로젝트 root `saves/`에 생성.
* LOG에 네 항목 결과 기록.

**M6.1 조명 데이터·순수 solver**

만드는 것:

* TORCH ID 12, emission 14, texture layer 13.
* HOTBAR 교체.
* `Chunk`·`PaddedChunk` packed light.
* `src/world/light.rs`.
* 열 skylight/blocklight solver.
* `src/stream/jobs.rs`로 잡 타입 분리.

커밋 메시지: `M6.1: add voxel light solver`

완료 조건:

* §15.5의 pack·전파·opaque·TORCH 테스트 통과.
* 열 solver 단일 입력이 결정적.
* worker thread에서 `Send + Sync`.
* 청크 파일 byte 포맷 `VFC1 + 65536 block bytes compressed` 유지.
* 조명 배열이 저장 파일에 포함되지 않음.

**M6.2 경계 전파·스트리밍**

만드는 것:

* `src/stream/lighting.rs`.
* light dirty·epoch·stale 폐기.
* 열 경계 고정점.
* 삽입·로드·언로드·편집 invalidation.
* 최초 열 조명 완료 전 메시 발행 보류.
* 조명 적용 시 콘텐츠 version·modified 불변.

커밋 메시지: `M6.2: stream lighting across chunk columns`

완료 조건:

* TORCH가 청크 열 경계를 넘어 정확히 1씩 감소.
* 경계 TORCH 제거 뒤 최대 16회 안에 block light 0.
* stale 결과가 새 편집을 덮지 않음.
* 조명 적용만으로 저장 청크 파일 수가 늘지 않음.
* R=12 정착 로그가 조명 큐가 빈 뒤 한 번만 출력.
* 열 solver p95 ≤12ms.

**M6.3 메시·셰이더·낮밤·바람**

만드는 것:

* 코너 4셀 light 평균.
* greedy 키에 block/sky 4개.
* `b[16..24)` 실제 사용.
* 마크식 16단계 광도표.
* `DayState`, 1200초 궤도, 하늘색.
* LEAVES·GRASS +Y 바람.
* TORCH 절차 텍스처.

커밋 메시지: `M6.3: render day night light and wind`

완료 조건:

* `vertex_pack_roundtrip` 포함 기존 정점 계약 유지.
* `greedy_never_merges_different_light_keys`.
* 조명 정점 수가 M5 동일 snapshot의 2.0배 이하.
* 공유 월드 정점의 바람 오차 `<1e-6`.
* 정오·자정 하늘 픽셀 비율 §15.6 통과.
* sealed light room의 13/9/5/1 비율 통과.

**M6.4 자동 스냅샷·성능 마감**

만드는 것:

* `--fixture`, `--view final|light`, `--day-phase`, `--world-time`.
* M6 fixture 3개.
* 조명 timing 로그.
* shader hotreload 회귀.
* LOG에 테스트 수·정착·solver·정점·프레임 수치.

커밋 메시지: `M6.4: validate lighting milestone`

완료 조건:

* 전체 테스트 **≥70 passed**.
* `cargo clippy --all-targets -- -D warnings`.
* `cargo fmt --all -- --check`.
* `git diff --check`.
* R=12 `settled < 5.0s`.
* 정착 후 max `<25ms`.
* 조명 반응 `<100ms`.
* 아래 검증 명령 전부 통과.

검증:

```bash
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check

cargo run --release --bin snapshot -- \
  --fixture m6-light-room --size 640x360 --view light \
  --day-phase 0.5 --out /tmp/vf_m6_light.png

cargo run --release --bin snapshot -- \
  --fixture terrain --seed 1 --radius 4 --size 640x360 \
  --day-phase 0.0 --out /tmp/vf_m6_day.png

cargo run --release --bin snapshot -- \
  --fixture terrain --seed 1 --radius 4 --size 640x360 \
  --day-phase 0.5 --out /tmp/vf_m6_night.png

cargo run --release --bin snapshot -- \
  --fixture m6-wind --size 640x360 --world-time 0.0 \
  --out /tmp/vf_m6_wind_a.png

cargo run --release --bin snapshot -- \
  --fixture m6-wind --size 640x360 --world-time 0.75 \
  --out /tmp/vf_m6_wind_b.png

VF_RADIUS=12 VF_SMOKE_FRAMES=1200 \
  cargo run --release 2>&1 | tee /tmp/vf_m6_perf.log

grep -E "settled|lighting:|stats:" /tmp/vf_m6_perf.log
```

각 PNG에는 BLUEPRINT §15.6의 Python 판정을 그대로 실행한다.

**M6가 끝나면 멈춘다.** LOG에 `M6 완료, 리뷰 요청`을 기록하고 진헌에게 같은 문구로 보고한다. Claude가 리뷰·커밋하기 전에는 M7로 가지 않는다.

---

## M7 — HDR·디퍼드·그림자·SSAO·대기·후처리

설계는 BLUEPRINT §16이다.

**M7.0 공유 리소스·arena 게이트**

만드는 것:

* `SceneBindings`.
* opaque/translucent Globals와 청크 uniform arena 통합.
* 월드 청크당 `ChunkSlot` 하나.
* M5에서 이월한 VB/IB arena 측정 게이트.
* 기존 render 결과가 리팩터 전과 99.5% 이상 일치하는 회귀 snapshot.

커밋 메시지: `M7.0: consolidate scene GPU resources`

완료 조건:

* opaque/translucent Globals buffer 한 벌.
* 청크 uniform arena 4MiB 한 벌.
* 같은 청크 두 패스가 같은 dynamic offset.
* `VF_AUTOPILOT=stream VF_BENCH_FRAMES=3600` 수치로 arena 여부 자동 결정.
* 선택 이유와 p99 frame/upload를 LOG에 기록.
* arena를 만들었다면 §14.10 테스트 2개 통과.

**M7.1 타깃·G버퍼·렌더 스케일·debug view**

만드는 것:

* `SceneTargets`.
* G버퍼 3장, depth, HDR, native LDR.
* `Renderer::render_frame`.
* surface direct geometry 제거.
* render scale 0.5~1.0.
* F4와 `--view`.
* M7 Globals 확장.
* `MaterialGpu`.

커밋 메시지: `M7.1: add scaled HDR gbuffer`

완료 조건:

* 모든 texture format가 §16.2.4와 일치.
* 2560×1440 scale 0.75 → internal 1920×1080.
* 1280×720 scale 0.5 → internal 640×360.
* output PNG 크기는 항상 요청한 native 크기.
* `albedo|normal|depth|ao|shadow|light|final` parse·cycle.
* normal·depth 프로브 §16.6 통과.

**M7.2 CSM**

만드는 것:

* 3-cascade PSSM.
* 2048² `Depth32Float` array.
* stable texel snapping.
* 3×3 PCF·bias·cascade blend.
* alpha-cutout·바람 shadow.
* 야간 shadow skip.

커밋 메시지: `M7.2: add cascaded sun shadows`

완료 조건:

* split `[22.9206, 52.7755, 192.0]` 오차 `<1e-3`.
* sub-texel 카메라 이동 matrix bitwise 동일.
* shadow/lit probe 각각 ≤0.35, ≥0.90.
* cascade 경계 밝기 점프 ≤0.12.
* shadow GPU p95 ≤1.8ms.

**M7.3 SSAO·deferred·대기 하늘**

만드는 것:

* 16-sample half-res SSAO.
* 5×5 bilateral blur.
* deferred lighting.
* 256×128 Rayleigh–Mie LUT.
* 태양·달.
* LEAVES alpha-cutout.

커밋 메시지: `M7.3: add deferred lighting atmosphere and ssao`

완료 조건:

* AO open ≥0.90, corner ≤0.65.
* depth edge 반대편 AO 누출 ≤0.05.
* sky LUT NaN·Inf 0.
* leaves alpha coverage 65~75%.
* transparent 전까지 HDR finite.
* SSAO p95 ≤0.9ms, sky+deferred ≤1.3ms.

**M7.4 블룸·노출·ACES·업스케일**

만드는 것:

* 5단계 bloom.
* 평균 log luminance reduction.
* exposure adaptation.
* ACES fitted.
* Catmull–Rom native upscale.
* snapshot warmup/frames/fixed exposure.

커밋 메시지: `M7.4: add HDR post processing`

완료 조건:

* bloom 5단계 크기 정확.
* halo/far 밝기 ≥1.20.
* 자동 노출 두 fixture 회색 패치 0.12~0.24.
* ACES CPU 참조 오차 `<1e-5`.
* scale 0.5 출력에 검은 border 없음.
* bloom+exposure+tone/upscale p95 ≤1.9ms.

**M7.5 forward 통합·timings·마감**

만드는 것:

* WATER/GLASS HDR forward.
* outline HDR.
* timestamp JSON.
* 모든 shader transactional hotreload.
* M7 fixture.
* 전체 성능·LOG.

커밋 메시지: `M7.5: complete deferred renderer`

완료 조건:

* 물·유리 뒤 지형이 보임.
* forward depth write off.
* surface에는 present pass만 접근.
* GPU timings JSON 키 10개 존재.
* 전체 테스트 **≥90 passed**.
* render scale 0.75 전체 p95 ≤13.0ms.
* max `<25ms`.
* clippy/fmt/diff 통과.

검증:

```bash
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check

for v in albedo normal depth ao shadow light final; do
  cargo run --release --bin snapshot -- \
    --fixture m7-passes --size 640x360 --render-scale 1.0 \
    --day-phase 0.0 --fixed-exposure 1.0 \
    --view "$v" --out "/tmp/vf_m7_${v}.png"
done

cargo run --release --bin snapshot -- \
  --fixture m7-bloom --size 640x360 \
  --fixed-exposure 1.0 --out /tmp/vf_m7_bloom.png

cargo run --release --bin snapshot -- \
  --fixture m7-exposure-dark --size 640x360 \
  --auto-exposure on --warmup 180 \
  --out /tmp/vf_m7_exposure_dark.png

cargo run --release --bin snapshot -- \
  --fixture m7-passes --size 2560x1440 --render-scale 0.75 \
  --warmup 60 --frames 180 \
  --timings /tmp/vf_m7_timings.json \
  --out /tmp/vf_m7_perf.png

python3 - <<'PY'
import json
d=json.load(open("/tmp/vf_m7_timings.json"))
required=["shadow","gbuffer","ssao","sky","deferred","translucent",
          "bloom","exposure","tonemap_upscale","present","total_gpu"]
for k in required:
    assert k in d, k
assert d["shadow"]["p95_ms"] <= 1.8
assert d["gbuffer"]["p95_ms"] <= 1.6
assert d["ssao"]["p95_ms"] <= 0.9
assert d["total_gpu"]["p95_ms"] <= 8.4
PY
```

BLUEPRINT §16.6의 픽셀 판정을 모두 실행한다.

**M7가 끝나면 멈춘다.** LOG에 `M7 완료, 리뷰 요청`. Claude 리뷰·커밋 전에는 M8로 가지 않는다.

---

## M8 — 물·볼류메트릭·구름·LOD

설계는 BLUEPRINT §17이다.

**M8.1 WATER 메시·Gerstner**

만드는 것:

* `ChunkMeshes.water`.
* WATER와 GLASS 분리.
* water surface greedy 2×2.
* 네 Gerstner wave와 analytic normal.
* side top seam.

커밋 메시지: `M8.1: add animated water surface`

완료 조건:

* 최대 Y 변위 ≤0.149001.
* water top quad 최대 2×2.
* 청크 경계 같은 정점 위치 오차 `<1e-5`.
* 기존 lowered bit 23 유지.
* 물 이외 정점은 시간에 따라 움직이지 않음.

**M8.2 SSR·굴절·코스틱**

만드는 것:

* HDR scene copy.
* transparent 통합 거리 정렬.
* 48-step SSR, 5-step refine.
* depth-aware refraction.
* Beer–Lambert.
* Fresnel·screen-space caustics.

커밋 메시지: `M8.2: add reflective refractive water`

완료 조건:

* SSR miss는 sky fallback.
* foreground refraction reject.
* thickness 증가 시 RGB 투과율 단조 감소.
* water debug RGB가 모두 0..1.
* 물 시간차 crop 변경률 1~35%.
* static crop 변경률 ≤0.2%.
* water GPU p95 ≤1.3ms.

**M8.3 볼류메트릭 fog/light**

만드는 것:

* 64² blue-noise.
* 1/4해상도 40-step fog.
* CSM ray sample.
* HG phase.
* temporal reprojection·bilateral upsample.

커밋 메시지: `M8.3: add volumetric fog and light`

완료 조건:

* zero density 결과 `(rgb=0, transmittance=1)`.
* disocclusion history reject.
* godray beam/shadow 밝기 비 ≥1.50.
* p95 ≤1.2ms.

**M8.4 볼류메트릭 구름**

만드는 것:

* periodic Perlin·Worley.
* 128³ base, 32³ detail.
* 180~260 layer.
* 48 view·6 light step.
* temporal.

커밋 메시지: `M8.4: add volumetric clouds`

완료 조건:

* 반대 texture face 차이 `<1e-6`.
* layer 밖 density 0.
* cloud debug coverage 20~75%.
* 고정 카메라 warmup 32 뒤 프레임간 평균 차이 ≤0.02.
* p95 ≤1.3ms.

**M8.5 원거리 LOD**

만드는 것:

* LOD1~3 grid·재귀 mode.
* coarse 조명.
* worker cache.
* dynamic uniform `origin.w=lod_shift`.
* dither overlap·skirt.
* save/WorldGen source 우선순위.
* memory cap.

커밋 메시지: `M8.5: add hierarchical terrain lod`

완료 조건:

* level 1/2/3 셀 크기 2/4/8.
* 기본 ring 거리 §17.2.10 정확.
* near crop LOD on/off 99.0% 일치.
* ring 밝기 점프 ≤0.08.
* CPU LOD cache ≤128MiB.
* GPU LOD mesh ≤128MiB.
* 전체 테스트 **≥111 passed**.
* 전체 프레임 p95 ≤16.0ms.
* clippy/fmt/diff 통과.

검증:

```bash
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check

cargo run --release --bin snapshot -- \
  --fixture m8-water --size 640x360 --world-time 0.0 \
  --fixed-exposure 1.0 --out /tmp/vf_m8_water_a.png

cargo run --release --bin snapshot -- \
  --fixture m8-water --size 640x360 --world-time 1.0 \
  --fixed-exposure 1.0 --out /tmp/vf_m8_water_b.png

cargo run --release --bin snapshot -- \
  --fixture m8-godrays --size 640x360 --warmup 32 \
  --fixed-exposure 1.0 --out /tmp/vf_m8_godrays.png

cargo run --release --bin snapshot -- \
  --fixture m8-clouds --size 640x360 --warmup 32 \
  --view cloud --out /tmp/vf_m8_cloud.png

cargo run --release --bin snapshot -- \
  --fixture m8-lod --size 1280x720 --view lod \
  --out /tmp/vf_m8_lod_debug.png

cargo run --release --bin snapshot -- \
  --fixture m8-lod --size 2560x1440 --render-scale 0.75 \
  --warmup 60 --frames 180 \
  --timings /tmp/vf_m8_timings.json \
  --out /tmp/vf_m8_perf.png
```

BLUEPRINT §17.6의 픽셀 판정을 모두 실행한다.

**M8 뒤에는 멈추지 않는다.** LOG에 M8 단계별 기록을 남기고 바로 M9.1로 간다. M8 단독 리뷰 요청을 하지 않는다.

---

## M9 — 컴퓨트 DDA 복셀 GI

설계는 BLUEPRINT §18이다.

**M9.1 Clipmap**

만드는 것:

* 128³×4 level.
* material/light RGB textures.
* World·LOD voxelize.
* 토로이달 origin·ring offset.
* slab update·4MiB/frame.
* edit invalidation.
* allocation 실패 폴백.

커밋 메시지: `M9.1: add voxel gi clipmaps`

완료 조건:

* 한 cell 이동 시 정확히 128² slab.
* teleport는 full rebuild.
* 겹치는 logical voxel bitwise 보존.
* 네 level material/light ready.
* clipmap 약 64MiB.
* 미지원 simulation 테스트에서 fallback.

**M9.2 DDA trace**

만드는 것:

* 1/4해상도 compute.
* cosine Hammersley 4 rays.
* 48블록·96 crossing.
* 거리별 level 전환.
* RGB TORCH emission.
* sky·CSM hit radiance.
* HDR GI composite.

커밋 메시지: `M9.2: trace diffuse gi with compute dda`

완료 조건:

* DDA 양·음 좌표 테스트.
* 첫 opaque voxel hit 정확.
* 모든 ray가 normal hemisphere 위.
* GI room shadow crop 밝기 1.20~2.50배.
* warm color ratio ≥1.15.
* trace p95 ≤1.8ms.

**M9.3 temporal·à-trous·fallback**

만드는 것:

* history GI/moments/depth/normal.
* world position reprojection.
* depth·normal reject.
* 3×3 clamp.
* 3단계 à-trous.
* 전체 GI transactional fallback.

커밋 메시지: `M9.3: stabilize and denoise voxel gi`

완료 조건:

* warmup 32 high-frequency ≤warmup 1의 65%.
* depth edge 대비 80% 이상 유지.
* invalid shader/format simulation 후 게임 정상 렌더.
* fallback dispatch 0.
* temporal+denoise p95 ≤1.05ms.

**M9.4 통합·성능·회귀**

만드는 것:

* M7~M9 최종 프레임 그래프.
* `--view gi|clipmap`.
* 모든 resize/pause/debug reset.
* timing JSON.
* LOG에 clipmap upload·GI GPU·전체 frame·메모리.
* M6 light fallback 회귀.

커밋 메시지: `M9.4: complete voxel gi milestone`

완료 조건:

* 전체 테스트 **≥124 passed**.
* GPU GI 합계 p95 ≤3.35ms.
* 전체 p95 ≤16.6ms.
* max `<25ms`.
* R=12 정착 `<5.0s`.
* clipmap seam 평균 차이 ≤0.08.
* clippy/fmt/diff 통과.

검증:

```bash
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check

cargo run --release --bin snapshot -- \
  --fixture m9-gi-room --size 640x360 --gi off \
  --fixed-exposure 1.0 --warmup 32 \
  --out /tmp/vf_m9_gi_off.png

cargo run --release --bin snapshot -- \
  --fixture m9-gi-room --size 640x360 --gi on \
  --fixed-exposure 1.0 --warmup 32 \
  --out /tmp/vf_m9_gi_on.png

cargo run --release --bin snapshot -- \
  --fixture m9-gi-room --size 640x360 --gi on \
  --fixed-exposure 1.0 --warmup 32 --view gi \
  --out /tmp/vf_m9_gi_debug.png

cargo run --release --bin snapshot -- \
  --fixture m9-clipmap --pos 63.5,72,0 --size 640x360 \
  --gi on --warmup 32 --view gi \
  --out /tmp/vf_m9_clip_a.png

cargo run --release --bin snapshot -- \
  --fixture m9-clipmap --pos 64.5,72,0 --size 640x360 \
  --gi on --warmup 32 --view gi \
  --out /tmp/vf_m9_clip_b.png

cargo run --release --bin snapshot -- \
  --fixture m9-gi-room --size 2560x1440 \
  --render-scale 0.75 --gi on --warmup 60 --frames 180 \
  --timings /tmp/vf_m9_timings.json \
  --out /tmp/vf_m9_perf.png

python3 - <<'PY'
import json
d=json.load(open("/tmp/vf_m9_timings.json"))
gi=sum(d[k]["p95_ms"] for k in
       ["gi_clipmap","gi_trace","gi_temporal","gi_denoise","gi_composite"])
print("gi p95 sum",gi)
assert gi <= 3.35
assert d["total_gpu"]["p95_ms"] <= 15.5
PY
```

BLUEPRINT §18.6의 모든 픽셀 판정을 실행한다.

**M9가 끝나면 멈춘다.** LOG에 `M8·M9 완료, 리뷰 요청`을 기록하고 진헌에게 같은 문구로 보고한다. Claude 리뷰·커밋 전에는 M10으로 가지 않는다.

---

## M10 — 배포 가능한 게임 마감

설계는 BLUEPRINT §19다.

**M10.1 설정·프리셋·GameClock**

만드는 것:

* `cargo add font8x8`, `cargo add rodio`; 실제 버전 LOG.
* `Settings` JSON version 1.
* CLI/env/settings 우선순위.
* atomic write.
* Performance/Balanced/Quality.
* pause 가능한 GameClock.
* Streamer radius runtime 변경.
* render target·shadow 설정 안전 재생성.

커밋 메시지: `M10.1: add settings and performance presets`

완료 조건:

* 설정 roundtrip·clamp·unknown field·atomic 테스트.
* 기본값 Balanced.
* pause 중 월드 시각 변화 0.
* radius 변경 뒤 새 desired set으로 정착.
* invalid JSON은 `.invalid-*`로 보존.

**M10.2 HUD·pause·스크린샷**

만드는 것:

* 8×8 ASCII atlas.
* HUD·9칸 hotbar·crosshair.
* pause/settings UI.
* key rebinding.
* F2 3-buffer screenshot.
* snapshot `--ui hud|pause|settings`.

커밋 메시지: `M10.2: add hud menus and screenshots`

완료 조건:

* 핫바 중심 오차 ≤0.5px.
* crosshair 홀·짝 크기 모두 중심.
* ESC pause/resume.
* 두 번째 ESC 종료 없음.
* screenshot 4번째 요청 drop, panic 없음.
* HUD·pause 픽셀 판정 통과.
* UI GPU ≤0.25ms.

**M10.3 절차 사운드**

만드는 것:

* rodio output.
* footstep/break/place/water PCM.
* 거리 기반 발소리.
* max 16 voices.
* silent fallback.
* `--audio-self-test`.

커밋 메시지: `M10.3: add procedural game audio`

완료 조건:

* PCM deterministic.
* peak ≤0.95.
* self-test WAV 48kHz mono.
* RMS 0.01~0.40.
* 장치 초기화 실패 simulation 후 게임 계속 실행.

**M10.4 셰이더팩**

만드는 것:

* manifest version/contract.
* user/builtin 경로.
* shader·texture precedence.
* path traversal 방지.
* 전체 팩 transactional compile/swap.
* hotreload.

커밋 메시지: `M10.4: add user shader packs`

완료 조건:

* invalid name·filename 거부.
* broken WGSL 후 builtin/이전 팩 유지.
* texture override 정확.
* UI shader override 거부.
* 모든 pipeline hotreload 실패 시 panic 없음.

**M10.5 `.app`·아이콘·서명·최종 성능**

만드는 것:

* procedural icon.
* `assets/macos/Info.plist`.
* launcher.
* bundle/notarize scripts.
* app resource copy.
* codesign.
* 최종 Balanced benchmark.
* 최종 LOG.

커밋 메시지: `M10.5: package voxelforge for macOS`

완료 조건:

* 전체 테스트 **≥141 passed**.
* `target/dist/Voxelforge.app` 구조 정확.
* `plutil -lint` 통과.
* ad-hoc `codesign --verify --strict` 통과.
* launcher에서 `VF_ASSETS`, `VF_SAVE_ROOT`, `VF_SETTINGS`, `VF_SHADERPACKS`, `VF_SCREENSHOTS` 설정.
* launcher smoke 3프레임 정상.
* Balanced 2560×1440 p95 ≤16.6ms, max `<25ms`.
* 추정 렌더 메모리 `<1.25GiB`.
* clippy/fmt/diff 통과.

검증:

```bash
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check

cargo run --release --bin snapshot -- \
  --fixture m10-hud --size 1280x720 --ui hud \
  --out /tmp/vf_m10_hud.png

cargo run --release --bin snapshot -- \
  --fixture m10-hud --size 1280x720 --ui pause \
  --out /tmp/vf_m10_pause.png

cargo run --release -- \
  --audio-self-test /tmp/vf_audio.wav

rm -rf /tmp/vf_shaderpacks
mkdir -p /tmp/vf_shaderpacks/broken/shaders
cat >/tmp/vf_shaderpacks/broken/pack.json <<'JSON'
{
  "version": 1,
  "contract_version": 1,
  "name": "Broken",
  "author": "test",
  "shaders": ["deferred.wgsl"],
  "textures": []
}
JSON
printf 'this is not wgsl\n' \
  >/tmp/vf_shaderpacks/broken/shaders/deferred.wgsl

VF_SHADERPACKS=/tmp/vf_shaderpacks VF_SHADERPACK=broken \
VF_SMOKE_FRAMES=3 cargo run --release 2>&1 | \
tee /tmp/vf_shaderpack.log

grep -q "shader pack.*rejected" /tmp/vf_shaderpack.log
grep -q "using builtin" /tmp/vf_shaderpack.log

bash scripts/bundle.sh
plutil -lint target/dist/Voxelforge.app/Contents/Info.plist
codesign --verify --strict --verbose=2 target/dist/Voxelforge.app

VF_SMOKE_FRAMES=3 \
  target/dist/Voxelforge.app/Contents/MacOS/voxelforge-launcher

bash -n scripts/notarize.sh

VF_PRESET=balanced VF_AUTOPILOT=stream VF_BENCH_FRAMES=3600 \
  cargo run --release 2>&1 | tee /tmp/vf_m10_bench.log
```

BLUEPRINT §19.6의 HUD·설정·WAV 픽셀/수치 판정을 전부 실행한다.

**M10이 끝나면 멈춘다.** LOG에 `M10 완료, 최종 리뷰 요청`을 기록하고 진헌에게 같은 문구로 보고한다. Claude 최종 리뷰·커밋 전에는 완료로 선언하지 않는다.

## 리뷰 체크리스트 (Claude, 각 마일스톤 뒤)

1. 검증 명령 전부 직접 재실행.
2. 스냅샷 PNG 열어 확인(뒤집힌 면, 구멍, 텍스처 방향, 색).
3. BLUEPRINT 계약(이름·시그니처·비트 레이아웃·면 순서)과 코드 대조.
4. 렌더 루프 `unwrap` 검색, `CurrentSurfaceTexture` 전 변형 처리 확인.
5. LOG의 「설계와 다르게 한 것」 검토 → 수용이면 BLUEPRINT 갱신, 아니면 수정 요청.
