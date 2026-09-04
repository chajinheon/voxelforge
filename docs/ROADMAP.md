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

## M6 — 마크식 조명·낮밤·바람 ← **다음** (상세 설계는 M6 시작 시 Claude가 BLUEPRINT §15로 확정)

- 하늘광·블록광 0~15 플러드필(청크 경계 전파 포함), 정점 `b`의 light/sky에 굽기. 광원 블록(TORCH) 추가.
- 낮밤 주기(`sun_dir` 회전, 하늘색 보간), 잎·풀 정점 흔들기(`time`).
- 셰이더: `color *= max(sky_f(sun), light_f)`.

## M7 — 디퍼드 파이프라인·쉐이더 1차 (Week 2)

- 오프스크린 HDR(`Rgba16Float`) + G버퍼(albedo, normal, material, depth). 렌더 스케일 옵션(0.5~1.0).
- 캐스케이드 섀도맵 3장, SSAO 절반 해상도, 대기 산란 하늘(Rayleigh/Mie 근사) + 태양/달, ACES 톤매핑 + 블룸 + 자동 노출.
- TAA(선택). 이 단계부터 「쉐이더 켠 마크」 룩.

## M8 — 쉐이더 2차 (Week 3)

- 물: 정점 파도 + 노멀 + SSR 반사 + 굴절 + 코스틱. 볼류메트릭 라이트/포그(1/4 해상도). 볼류메트릭 구름. 원거리 LOD(2×/4×/8× 다운샘플 메시).

## M9 — 복셀 GI (Week 4+)

- 월드 3D 텍스처 clipmap, 컴퓨트 DDA 레이마칭 GI + 색 조명, 시간적 누적·디노이즈. 필요 시 wgpu-hal Metal 인터롭으로 MetalFX 검토.

---

## 리뷰 체크리스트 (Claude, 각 마일스톤 뒤)

1. 검증 명령 전부 직접 재실행.
2. 스냅샷 PNG 열어 확인(뒤집힌 면, 구멍, 텍스처 방향, 색).
3. BLUEPRINT 계약(이름·시그니처·비트 레이아웃·면 순서)과 코드 대조.
4. 렌더 루프 `unwrap` 검색, `CurrentSurfaceTexture` 전 변형 처리 확인.
5. LOG의 「설계와 다르게 한 것」 검토 → 수용이면 BLUEPRINT 갱신, 아니면 수정 요청.
