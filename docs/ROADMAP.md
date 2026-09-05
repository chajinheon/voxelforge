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

## M7~M10 공통 실행 규칙 — M6 리뷰 이후 무중단 구간

M6는 리뷰·커밋 완료 상태다. M7부터 M10까지는 하나의 연속 작업이다.

- M7, M8, M9가 끝날 때마다 해당 완료 조건을 전부 통과하고 `docs/LOG.md` 맨 위에 기록한다.
- 그 시점에 사람 리뷰를 요청하거나 멈추지 않는다. 즉시 다음 마일스톤으로 간다.
- Sol은 `git commit`을 실행하지 않는다. 각 단계 LOG에 아래 「의도한 커밋 메시지」만 적는다.
- 검증 실패를 「후속 리뷰 사항」으로 미루지 않는다. 같은 단계에서 수정·재검증한다.
- wgpu 30 API가 §11에 없으면 코드를 쓰기 전에 registry source에서 실제 타입·필드를 확인하고 확인 경로를 LOG에 남긴다.
- M10 최종 검증 뒤에만 멈추고 `M7~M10 완료, 최종 리뷰 요청`이라고 보고한다.

설계 우선순위는 BLUEPRINT §20이다. §16~§19와 충돌하면 §20을 따른다.

---

## M7 — M5 Air High HDR·PBR·GTAO·TAAU 기반 렌더러

### M7.0 — M6 기준선·공유 GPU 리소스·모듈 분리

만드는 것:

- M6 HEAD에서 전체 테스트와 기준 snapshot 재생성.
- `SceneBindings`: Globals, material arrays, sampler, material LUT, 청크 dynamic-offset uniform arena 한 벌.
- opaque/translucent가 중복 소유하던 Globals·청크 uniform 제거.
- `renderer.rs`, `main.rs`, `stream.rs`를 500줄 아래로 유지하도록 하위 모듈 분리.
- `RenderPreset::{Performance,Balanced,M5AirHigh,Cinematic}`와 내부 크기 계산.
- wgpu 30 texture array, storage texture, timestamp, query resolve, texture view layer API 사전 확인.

의도한 커밋 메시지:

```text
M7.0: consolidate renderer resources and presets
```

완료 조건:

- `m5_air_high`, 2560×1440 내부 크기 정확히 1848×1040.
- opaque·translucent가 같은 Globals buffer ID와 같은 chunk-uniform arena ID 사용.
- 한 월드 청크는 `ChunkSlot` 하나만 사용.
- 현재 M6 final snapshot과 리팩터 후 snapshot의 동일 crop 픽셀 일치율 ≥99.5%, 임계값 채널당 2.
- `main.rs`, `stream.rs`, `renderer.rs` 각각 500줄 이하.
- 렌더 루프 `unwrap/expect` 0개.
- LOG에 wgpu 30 확인한 파일 경로·실제 이름 기록.

### M7.1 — 재질 배열·mip·G버퍼·PBR

만드는 것:

- albedo/material/emission 16×16 `texture_2d_array` 3개, 5 mip.
- coverage-preserving cutout mip.
- `MaterialGpu`, POM CPU 참조, tangent basis.
- G버퍼 5장 + depth.
- motion vector에서 jitter 제거.
- Cook–Torrance GGX deferred PBR.
- 64² sky cubemap 7 mip.
- debug view `albedo|normal|depth|light|material|motion|reactive`.

의도한 커밋 메시지:

```text
M7.1: add material arrays gbuffer and ggx lighting
```

완료 조건:

- 세 배열의 layer 수·이름·mip 수가 동일.
- cutout 각 mip coverage 차이 ≤3%p.
- oct normal roundtrip 최대 각도 오차 <0.25°.
- POM CPU reference와 shader probe UV 오차 ≤1/1024.
- GGX 테스트에서 NaN/Inf 0, BRDF energy upper bound 1.05.
- motion vector static camera+jitter-only 장면의 절댓값 ≤1e-5.
- `m7-materials` fixture의 metal/stone/roughness/emissive 픽셀 조건 통과.
- G-buffer+deferred GPU p95 합 ≤2.95ms.

### M7.2 — CSM·contact shadow·GTAO·대기

만드는 것:

- 3 cascade PSSM, preset별 far.
- 8/12/20 tap rotated Poisson PCF.
- cascade update cadence와 edit invalidation.
- half-res 8-step contact shadow.
- half-res XeGTAO 계열 horizon search, temporal·bilateral.
- transmittance 256×64, multiscatter 32×32, sky-view 192×108 LUT.
- sky cubemap update cadence.
- LEAVES alpha-cutout과 shadow alpha test.

의도한 커밋 메시지:

```text
M7.2: add high quality shadows gtao and atmosphere
```

완료 조건:

- PSSM split CPU reference 오차 <1e-3.
- cascade0 매 frame, cascade1 2-frame, cascade2 4-frame cadence 테스트.
- camera threshold/edit에서 즉시 update.
- flat plane GTAO ≥0.92, right-angle corner ≤0.62.
- depth edge 반대편 AO 누출 ≤0.05.
- shadow fixture: umbra ≤0.30, lit ≥0.90, cascade seam jump ≤0.10.
- 모든 atmosphere texel finite·0 이상.
- sky LUT/cubemap 불필요 update가 안정 camera 64 frame 중 8회 이하.
- shadow+contact p95 ≤2.00ms, GTAO ≤0.90ms, atmosphere 상각 ≤0.20ms.

### M7.3 — 블룸·노출·TAAU·ACES·샤픈

만드는 것:

- 6-level internal bloom.
- average log luminance reduction, exposure adaptation.
- native `Rgba16Float` TAAU history ping-pong.
- 8-frame Halton jitter.
- motion/depth/normal/material/reactive reject.
- YCoCg 3×3 variance clamp.
- ACES fitted·색 보정·5-tap sharpen.
- resize/scale/FOV/teleport/shaderpack history reset.

의도한 커밋 메시지:

```text
M7.3: add taau exposure bloom and final grading
```

완료 조건:

- Halton 8개 값 계약과 bitwise 동일.
- disocclusion pixel history weight 0.
- static 장면 warmup32 high-frequency residual ≤warmup1의 70%.
- native edge rise width ≤3.0px.
- render scale 0.72 출력 크기는 정확히 native, border black 비율 <0.5%.
- auto exposure 30/60/120fps 2초 결과 상대 오차 <0.5%.
- bloom halo/far 선형 밝기 비 ≥1.20.
- TAAU+ACES+sharpen p95 ≤1.10ms.

### M7.4 — M7 통합·회귀·성능 게이트

만드는 것:

- surface direct geometry 제거, caller output view에는 present copy만.
- F4 debug cycle 확정.
- transactional shader hotreload 전체 파이프라인 묶음.
- timestamp timing JSON.
- M6 조명·투명·outline 회귀.
- M7 LOG 수치 기록.

의도한 커밋 메시지:

```text
M7.4: complete m5 air high renderer foundation
```

완료 조건:

- 전체 테스트 ≥95 passed.
- `cargo clippy --all-targets -- -D warnings` 통과.
- `cargo fmt --all -- --check` 통과.
- `git diff --check` 통과.
- M7 GPU p95 ≤8.10ms.
- `VF_ALLOC_STATS=1` steady renderer allocation 중앙값 0, p95 0/frame.
- 전체 frame p95 ≤12.5ms, max <25ms.
- shader 한 개 문법 오류를 넣은 smoke에서 이전 pipeline 유지, panic 0.
- M6 `m6-light-room`, `m6-wind` 픽셀 판정 재통과.

검증 명령:

```bash
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check

cargo run --release --bin snapshot -- \
  --fixture m7-materials --size 1280x720 --render-scale 1.0 \
  --preset m5_air_high --fixed-exposure 1.0 --warmup 16 \
  --view final --out /tmp/vf_m7_materials.png

cargo run --release --bin snapshot -- \
  --fixture m7-taau --size 1280x720 --render-scale 0.72 \
  --preset m5_air_high --fixed-exposure 1.0 --warmup 1 \
  --out /tmp/vf_m7_taau_1.png
cargo run --release --bin snapshot -- \
  --fixture m7-taau --size 1280x720 --render-scale 0.72 \
  --preset m5_air_high --fixed-exposure 1.0 --warmup 32 \
  --out /tmp/vf_m7_taau_32.png

cargo run --release --bin snapshot -- \
  --fixture m7-passes --size 2560x1440 --render-scale 0.72 \
  --preset m5_air_high --warmup 60 --frames 240 \
  --timings /tmp/vf_m7_timings.json \
  --out /tmp/vf_m7_perf.png
```

M7 검증이 통과하면 LOG에 `M7 검증 완료 — 무중단 규칙에 따라 M8 진행`을 적고 **멈추지 말고 M8.0으로 간다.**

---

## M8 — 고품질 물·볼류메트릭·구름·LOD

### M8.0 — depth pyramid·공통 temporal 기반

만드는 것:

- linear depth `R32Float` mip chain.
- 64² deterministic blue-noise rank texture.
- quarter-resolution checkerboard 좌표·history reset 공통 코드.
- temporal reject 공통 함수의 Rust reference.

의도한 커밋 메시지:

```text
M8.0: add temporal and depth hierarchy foundations
```

완료 조건:

- mip0 linear depth CPU reference 오차 <1e-4.
- 각 상위 mip이 2×2 min depth와 정확히 일치.
- blue-noise rank가 0..4095 순열, 동일 seed bitwise 동일.
- 4-frame checkerboard가 모든 parity를 한 번씩 방문.

### M8.1 — WATER mesh·Gerstner·SSR·굴절

만드는 것:

- WATER를 GLASS와 별도 mesh/pass로 분리.
- water surface greedy 최대 2×2.
- 네 Gerstner wave와 analytic normal.
- hierarchical SSR 40+5.
- Fresnel, Beer–Lambert, depth-aware refraction.
- shoreline/crest foam, caustics, underwater mode.

의도한 커밋 메시지:

```text
M8.1: add high quality animated water
```

완료 조건:

- Y 변위 절댓값 ≤0.149001.
- analytic normal 길이 오차 <1e-4.
- adjacent chunk water vertex 위치 차이 <1e-5.
- foreground refraction은 원 UV 사용.
- thickness 증가 시 RGB transmittance 단조 감소.
- SSR debug confidence 0..1, residual miss 정확.
- water crop 시간 변경률 1~35%, static crop ≤0.2%.
- water p95 ≤1.20ms.

### M8.2 — 볼류메트릭 포그·라이트

만드는 것:

- quarter-res 2×2 checkerboard 32-step march.
- HG phase, CSM 2-step cadence sample.
- closest 8 emissive light fog contribution.
- temporal reprojection·depth/normal reject·bilateral upscale.
- underwater density 3배.

의도한 커밋 메시지:

```text
M8.2: add temporal volumetric lighting and fog
```

완료 조건:

- zero density `(rgb=0,a=1)`.
- disocclusion history weight 0.
- 4-frame static convergence 후 frame difference ≤0.02.
- godray beam/shadow brightness ratio ≥1.50.
- light list는 거리 오름차순+position tie-break로 결정적.
- p95 ≤0.85ms.

### M8.3 — 볼류메트릭 구름·cloud shadow

만드는 것:

- periodic 128³ base/32³ detail Perlin–Worley.
- 180~260 cloud layer.
- quarter-res checkerboard 40/6 march.
- 512² world-space cloud shadow, 8-frame cadence.
- cloud/fog composition order.

의도한 커밋 메시지:

```text
M8.3: add volumetric clouds and cloud shadows
```

완료 조건:

- periodic opposite faces 최대 차이 <1e-6.
- layer 밖 density 0.
- cloud coverage 20~75%.
- cloud shadow origin은 32블록 단위 snap.
- 안정 camera 64 frame에서 shadow update ≤8회.
- direct-light clouded/open ratio 0.55~0.90.
- clouds p95 ≤1.05ms, cloud shadow 상각 ≤0.15ms.

### M8.4 — LOD1/2/3

만드는 것:

- 2×/4×/8× 32³ grids.
- deterministic modal downsample.
- coarse M6 light.
- 32-block overlap dither와 coarse skirt.
- source priority World→save→WorldGen.
- edit ancestor invalidation.
- CPU/GPU cache caps.

의도한 커밋 메시지:

```text
M8.4: add hierarchical distant terrain lod
```

완료 조건:

- level cell size 2/4/8.
- default R=10 ring 범위 §17/§20과 정확히 일치.
- near crop LOD on/off ≥99.0% 픽셀 일치.
- ring brightness seam ≤0.08.
- CPU LOD cache ≤128MiB, GPU LOD mesh ≤128MiB.
- LOD draw p95 ≤0.45ms.

### M8.5 — M8 통합·성능

만드는 것:

- GLASS/WATER를 청크 중심 거리 기준 뒤→앞으로 하나의 목록에서 정렬하고 pipeline만 전환한다.
- underwater hand/UI 제외 확인.
- debug `water|volumetric|cloud|lod`.
- M8 timing JSON·snapshot probes·LOG.

의도한 커밋 메시지:

```text
M8.5: complete water volumetrics clouds and lod
```

완료 조건:

- 전체 테스트 ≥118 passed.
- M7 회귀 전부 통과.
- M8 추가 GPU p95 ≤3.70ms.
- M7+M8 GPU p95 ≤11.8ms.
- 전체 frame p95 ≤15.0ms, max <25ms.
- clippy/fmt/diff 통과.

검증 명령:

```bash
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check

cargo run --release --bin snapshot -- \
  --fixture m8-water --size 1280x720 --preset m5_air_high \
  --world-time 0.0 --fixed-exposure 1.0 --out /tmp/vf_m8_water_a.png
cargo run --release --bin snapshot -- \
  --fixture m8-water --size 1280x720 --preset m5_air_high \
  --world-time 1.0 --fixed-exposure 1.0 --out /tmp/vf_m8_water_b.png

cargo run --release --bin snapshot -- \
  --fixture m8-godrays --size 1280x720 --preset m5_air_high \
  --warmup 32 --fixed-exposure 1.0 --out /tmp/vf_m8_godrays.png

cargo run --release --bin snapshot -- \
  --fixture m8-clouds --size 1280x720 --preset m5_air_high \
  --warmup 32 --view cloud --out /tmp/vf_m8_clouds.png

cargo run --release --bin snapshot -- \
  --fixture m8-lod --size 2560x1440 --preset m5_air_high \
  --warmup 60 --frames 240 --timings /tmp/vf_m8_timings.json \
  --out /tmp/vf_m8_perf.png
```

M8 검증이 통과하면 LOG에 `M8 검증 완료 — 무중단 규칙에 따라 M9 진행`을 적고 **멈추지 말고 M9.0으로 간다.**

---

## M9 — 컴퓨트 DDA 복셀 GI

### M9.0 — 기능 지원 확인·폴백 틀

만드는 것:

- 3D `Rgba8Unorm` sample/storage/copy capability 실제 확인.
- `GiMode::{Enabled,Fallback,DisabledByUser}`.
- GI 리소스 생성 전체를 transactional init.
- 실패 simulation flag.

의도한 커밋 메시지:

```text
M9.0: add voxel gi capability and fallback contract
```

완료 조건:

- 지원 장치에서 Enabled.
- 강제 format/pipeline failure에서 게임 정상, `GI disabled: ...; using baked light + GTAO` 정확히 1회.
- fallback에서 GI dispatch 0.
- `unsafe`, `wgpu-hal`, ray query, acceleration structure 문자열이 소스에 없음.

### M9.1 — 4-level clipmap·토로이달 slab

만드는 것:

- 128³×4 material/light textures.
- level 0/1 World, level 2/3 LOD source.
- logical origin·ring offset.
- exposed slab 분할 upload.
- ready revision과 coarse fallback.
- edit invalidation.

의도한 커밋 메시지:

```text
M9.1: add toroidal voxel gi clipmaps
```

완료 조건:

- one-cell move 정확히 128² new cells.
- overlapping logical voxels bitwise 동일.
- delta≥128 full rebuild.
- frame upload ≤4MiB.
- texture memory 약 64MiB ±2MiB.
- level priority 0→1→2→3.

### M9.2 — WGSL compute DDA trace

만드는 것:

- quarter-res 8×8 compute.
- 4 cosine Hammersley rays.
- max 48, crossing 96.
- distance band level switch.
- first-hit material, CSM sun, sky miss.
- six emissive block RGB.
- indirect composite.

의도한 커밋 메시지:

```text
M9.2: trace colored voxel gi with compute dda
```

완료 조건:

- positive/negative-coordinate DDA first hit 정확.
- cosine samples dot(N,d)≥0.
- all outputs finite/nonnegative.
- m9 room GI on/off shadow brightness ratio 1.20~2.50.
- warm torch R/mean(GB) ≥1.15.
- trace p95 ≤1.75ms.

### M9.3 — temporal·moments·à-trous

만드는 것:

- native world reprojection to quarter history.
- depth/normal/material reject.
- 3×3 YCoCg/radiance clamp.
- moments variance.
- à-trous 1,2,4.
- resize/teleport/clipmap/history reset.

의도한 커밋 메시지:

```text
M9.3: temporally stabilize and denoise voxel gi
```

완료 조건:

- reject 결과 current와 bitwise 동일.
- warmup32 high-frequency ≤warmup1의 65%.
- depth edge contrast 80% 이상 보존.
- temporal+denoise+composite p95 ≤1.15ms.

### M9.4 — M9 통합·성능

만드는 것:

- debug `gi|clipmap`.
- clipmap seam fixtures.
- full M7~M9 timing JSON.
- baked-light fallback snapshot.
- LOG 수치.

의도한 커밋 메시지:

```text
M9.4: complete compute voxel gi
```

완료 조건:

- 전체 테스트 ≥134 passed.
- GI GPU 합계 p95 ≤3.20ms.
- M7~M9 GPU p95 ≤15.0ms.
- 전체 frame p95 ≤16.6ms, max <25ms.
- clipmap seam 평균 선형 RGB 차이 ≤0.08.
- R=10 settled <5.0s.
- clippy/fmt/diff 통과.

검증 명령:

```bash
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check

cargo run --release --bin snapshot -- \
  --fixture m9-gi-room --size 1280x720 --preset m5_air_high \
  --gi off --fixed-exposure 1.0 --warmup 32 \
  --out /tmp/vf_m9_gi_off.png
cargo run --release --bin snapshot -- \
  --fixture m9-gi-room --size 1280x720 --preset m5_air_high \
  --gi on --fixed-exposure 1.0 --warmup 32 \
  --out /tmp/vf_m9_gi_on.png
cargo run --release --bin snapshot -- \
  --fixture m9-gi-room --size 1280x720 --preset m5_air_high \
  --gi on --fixed-exposure 1.0 --warmup 32 --view gi \
  --out /tmp/vf_m9_gi_debug.png

cargo run --release --bin snapshot -- \
  --fixture m9-clipmap --pos 63.5,72,0 --size 1280x720 \
  --gi on --warmup 32 --view gi --out /tmp/vf_m9_clip_a.png
cargo run --release --bin snapshot -- \
  --fixture m9-clipmap --pos 64.5,72,0 --size 1280x720 \
  --gi on --warmup 32 --view gi --out /tmp/vf_m9_clip_b.png

cargo run --release --bin snapshot -- \
  --fixture m9-gi-room --size 2560x1440 --preset m5_air_high \
  --gi on --warmup 60 --frames 240 \
  --timings /tmp/vf_m9_timings.json --out /tmp/vf_m9_perf.png
```

M9 검증이 통과하면 LOG에 `M9 검증 완료 — 무중단 규칙에 따라 M10 진행`을 적고 **멈추지 말고 M10.0으로 간다.**

---

## M10 — 122개 건축 아이템·I 인벤토리·손·배포 마감

### M10.0 — 안정 ID 레지스트리·재질 recipe·ItemId

만드는 것:

- BLUEPRINT §20.6.2 BlockId 0~88와 상태 range.
- ItemId 0~122와 7 categories.
- 122 `ItemDef` 정확한 순서.
- `TextureRecipe` patterns·palette.
- 기존 alias `LOG/LEAVES/PLANKS/GLASS/BRICK/COBBLE` 유지.
- emission RGB와 light-blocking 분리.

의도한 커밋 메시지:

```text
M10.0: add stable creative block and item catalog
```

완료 조건:

- ItemId 1..122 unique·gap 없음.
- 모든 item name unique.
- 모든 placement가 유효 concrete BlockId를 생성.
- 기존 ID 0..12 의미 bitwise 동일.
- 기존 M6 full opaque lighting snapshot 동일.
- 모든 texture recipe가 16×16 albedo/material/emission layer 생성.
- NaN/Inf 0, alpha·roughness·height 범위 0..1.

### M10.1 — 1/16 shape·정점·메셔

만드는 것:

- `ShapeMask`, cached templates, collision boxes.
- axis logs, bottom/top slabs, 8 stair states, 16 pane states, 16 fence states.
- frac_x/y/z vertex bits.
- partial-neighbor coverage subtraction.
- micro AO.
- opaque/cutout/glass/water pass routing.

의도한 커밋 메시지:

```text
M10.1: add subvoxel building shapes and meshing
```

완료 조건:

- bottom slab occupancy 2048.
- pane mask0 occupancy 64.
- fence mask0 occupancy 256.
- template quads와 exposed microface 집합 bitwise 동일.
- vertex fraction roundtrip all 0..15.
- old cube vertices frac 0, M9 terrain regression ≥99.5%.
- partial face area 보존 정확히 256 subpixels.
- shape gallery probes 전부 통과.
- M10 shape mesh CPU p95 ≤기존 full-cube mesh의 2.5배/동일 visible block 4096 fixture.

### M10.2 — 물리·shape raycast·배치 규칙

만드는 것:

- shape collision AABB 순회.
- fence 1.5 collision.
- DDA cell+shape AABB raycast.
- axis log placement.
- slab top/bottom/merge.
- stair facing/upside-down.
- pane/fence five-cell connection update.
- middle-click pick block.
- hand action event ID 생성 지점.

의도한 커밋 메시지:

```text
M10.2: add shaped collision raycast and placement
```

완료 조건:

- pane empty region ray가 뒤 블록을 맞춤.
- stair step hit normal·local_hit 정확.
- 8 stair state placement fixture 정확.
- same-material slab pair full block merge.
- pane/fence edit 후 center+4 neighbor state 정확.
- 사용자 edit 하나당 각 chunk version 최대 1 증가.
- player가 bottom slab·stair를 올라가고 pane를 통과하지 않음.
- fence 위로 일반 jump로 넘지 못함.
- pick block existing slot 선택/absent replace 계약 통과.
- hold repeat가 LMB 0.22/0.10, RMB 0.25/0.12초 계약과 일치하고 frame당 edit 1회 이하.

### M10.3 — item icon·HUD·I 인벤토리

만드는 것:

- `cargo add font8x8` 실제 버전 기록.
- ASCII 8×8 atlas.
- 122-layer item icon bake.
- hotbar·crosshair.
- `I` modal inventory.
- category, search, 9×6 grid, scroll, tooltip.
- click·number hotbar assignment.
- quarter-res modal blur.
- snapshot `--ui inventory|hud|pause`, inventory fixture args.

의도한 커밋 메시지:

```text
M10.3: add creative inventory hud and item icons
```

완료 조건:

- icon layer 정확히 122, bake ≤500ms.
- 모든 icon nontransparent bbox 20..60px.
- inventory all 화면 54 occupied cells.
- `Shapes + stair` filter 정확히 12 items.
- hotbar center error ≤0.5px.
- I open에서 cursor unlock·world time/physics 정지.
- streaming apply/save/shader reload는 open 중 계속.
- Escape는 inventory를 먼저 닫고 pause를 열지 않음.
- inventory blur+UI p95 ≤0.45ms.
- BLUEPRINT §20.12.3 픽셀 스크립트 통과.

### M10.4 — 1인칭 손·상호작용·설정·사운드

만드는 것:

- native LDR right-hand viewmodel.
- held item mesh cache.
- idle/walk/break/place/switch animations.
- settings version 2 migration·creative hotbar.
- `cargo add rodio` 실제 버전 기록.
- 기존 4 sound + inventory/switch sound.
- screenshot 3-buffer.
- pause/settings UI.

의도한 커밋 메시지:

```text
M10.4: add first person hand settings audio and screenshots
```

완료 조건:

- break 0.26, place 0.18, switch 0.20초 reference 정확.
- action priority Place>Break>Switch>Idle.
- pause/inventory 1초 전후 transform bitwise 동일.
- hand crop 변경률 break/place 2~30%, world crop ≤0.1%.
- settings v1→v2 migrate, hotbar roundtrip.
- invalid JSON preservation·atomic temp cleanup.
- PCM same seed bitwise 동일, peak ≤0.95.
- audio device failure silent fallback.
- screenshot 4번째 queue request drop, panic 0.
- viewmodel p95 ≤0.15ms, HUD p95 ≤0.18ms.

### M10.5 — 셰이더팩 contract 2·`.app`·최종 검증

만드는 것:

- shaderpack contract 2.
- albedo/material/emission/hand overrides.
- whole-pack transactional compile/swap.
- path traversal 방지.
- app launcher·Info.plist·icon·bundle/codesign/notarize scripts.
- full debug cycle.
- 최종 M5 Air benchmark 3600 frames.
- LOG 최종 기록.

의도한 커밋 메시지:

```text
M10.5: finish voxelforge creative build and macos release
```

완료 조건:

- 전체 테스트 ≥185 passed.
- contract1 거부, contract2 broken shader에서 이전/builtin 유지.
- UI shader·registry·shape override 거부.
- `plutil -lint` 통과.
- ad-hoc `codesign --verify --strict` 통과.
- bundle launcher smoke 3 frames.
- `m5_air_high`, 2560×1440, R=10:
  - settled <5.0s.
  - GPU p95 ≤15.40ms HUD, ≤15.75ms inventory.
  - full frame p95 ≤16.6ms.
  - max <25ms.
  - memory <1.35GiB.
- clippy/fmt/diff 통과.
- render loop panic·validation error 0.
- renderer steady allocations 중앙값 0, p95 0/frame.

최종 검증 명령:

```bash
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check

cargo run --release --bin snapshot -- \
  --fixture m10-shapes --size 1280x720 --render-scale 1.0 \
  --preset m5_air_high --fixed-exposure 1.0 --warmup 16 \
  --view final --out /tmp/vf_m10_shapes.png

cargo run --release --bin snapshot -- \
  --fixture m10-inventory --size 1280x720 --ui inventory \
  --inventory-category all --inventory-query "" \
  --out /tmp/vf_m10_inventory.png
cargo run --release --bin snapshot -- \
  --fixture m10-inventory --size 1280x720 --ui inventory \
  --inventory-category shapes --inventory-query "stair" \
  --out /tmp/vf_m10_inventory_stair.png

cargo run --release --bin snapshot -- \
  --fixture m10-viewmodel --size 1280x720 --hand-action idle \
  --held-item 101 --world-time 0.0 --out /tmp/vf_hand_idle.png
cargo run --release --bin snapshot -- \
  --fixture m10-viewmodel --size 1280x720 --hand-action break:0.13 \
  --held-item 101 --world-time 0.0 --out /tmp/vf_hand_break.png
cargo run --release --bin snapshot -- \
  --fixture m10-viewmodel --size 1280x720 --hand-action place:0.09 \
  --held-item 101 --world-time 0.0 --out /tmp/vf_hand_place.png

cargo run --release -- --audio-self-test /tmp/vf_audio.wav

rm -rf /tmp/vf_shaderpacks
mkdir -p /tmp/vf_shaderpacks/broken/shaders
cat >/tmp/vf_shaderpacks/broken/pack.json <<'JSON'
{
  "version": 1,
  "contract_version": 2,
  "name": "Broken",
  "author": "test",
  "shaders": ["deferred.wgsl"],
  "textures": []
}
JSON
printf 'this is not wgsl\n' >/tmp/vf_shaderpacks/broken/shaders/deferred.wgsl
VF_SHADERPACKS=/tmp/vf_shaderpacks VF_SHADERPACK=broken \
VF_SMOKE_FRAMES=3 cargo run --release 2>&1 | tee /tmp/vf_shaderpack.log
grep -q "shader pack.*rejected" /tmp/vf_shaderpack.log
grep -Eq "using builtin|keeping previous" /tmp/vf_shaderpack.log

bash scripts/bundle.sh
plutil -lint target/dist/Voxelforge.app/Contents/Info.plist
codesign --verify --strict --verbose=2 target/dist/Voxelforge.app
VF_SMOKE_FRAMES=3 \
  target/dist/Voxelforge.app/Contents/MacOS/voxelforge-launcher
bash -n scripts/notarize.sh

VF_PRESET=m5_air_high VF_RADIUS=10 \
VF_AUTOPILOT=creative-build VF_BENCH_FRAMES=3600 \
  cargo run --release 2>&1 | tee /tmp/vf_m10_bench.log
```

`VF_AUTOPILOT=creative-build` 계약:

- first 600 frame warmup.
- 600~1200 walk/fly and stream.
- 1200~1800 place/break cube/slab/stair/pane/fence.
- 1800~2400 water/cloud/GI camera route.
- 2400~3000 inventory open, search `stair`, hotbar assignment, close.
- 3000~3600 viewmodel action and settled scene.
- 실제 save는 `/tmp/vf_m10_bench_save`로 격리.

최종 LOG 필수 수치:

```text
HEAD/base M6 commit
final test count
list of every source module whose line count is >=400
all cargo add commands and resolved versions
M7/M8/M9/M10 timing JSON paths
pass median/p95 and total p95/max
settled seconds
world/render/LOD/clipmap/icon memory estimates
122 item catalog validation
shape mesh vertex counts
inventory pixel results
viewmodel crop-diff results
audio RMS/peak
shaderpack failure fallback
bundle path and codesign output
notarize executed/skipped and truthful reason
```

M10 검증을 전부 통과한 뒤에만 멈춘다. LOG 맨 위에 `M7~M10 완료, 최종 리뷰 요청`을 기록하고 같은 문구로 진헌에게 보고한다.
