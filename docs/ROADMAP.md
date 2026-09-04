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

## M3 — 건축 (≈1~2h) ← **오늘의 목표** (기능 완료 2026-09-04 13:04, 리뷰 통과 조건부 — M3.1 참조)

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

## M3.1 — 리뷰 수정 (≈1h, GPT) ← **현재 위치**

2026-09-04 Claude 리뷰 결과. AO 결함은 Claude가 직접 고쳐 커밋했다(`d4d7e27`). 아래는 GPT 몫.

**만드는 것**
1. 스트리밍 예산을 개수(4/8)에서 **시간 예산 ≤10ms/프레임**으로 교체(BLUEPRINT §5). 첫 프레임 전 스폰 반경 1 동기 로딩·메싱. `is_empty()` 청크 메싱 생략. `stream: settled in X.XXs (N chunks)` 로그 1회.
2. `snapshot --edits "set x,y,z,id; set …"` 옵션(모든 청크 로딩 후 적용, dirty 재메싱 뒤 렌더). 손 플레이를 자동화할 수 없으니 이게 건축 회귀 테스트다.
3. `chunk.wgsl`의 `packed_light * 0.0` / `sky * 0u` 우회 제거 — naga는 미사용 변수를 경고하지 않는다. `light`/`sky`는 디코드만 남기거나 M6까지 주석 처리.
4. `gen.rs`: `let _ = self.seed;` 제거(필드를 쓰거나 지운다), `chunk.blocks[..] = id; chunk.non_air += 1` 직접 쓰기 대신 `Chunk::set` 또는 전용 생성자 사용.
5. `main.rs` `remove_unloaded_gpu_chunks`: `origin / CHUNK_SIZE` → `chunk_of(origin)`(지금은 32의 배수라 우연히 맞음).

**완료 조건**
- `VF_SMOKE_FRAMES=600 cargo run --release` 로그에 `stream: settled in` **< 3.00s**, 로딩 중 프레임 최대 < 33ms(제목·F3 통계).
- `cargo run --release --bin snapshot -- --seed 1 --radius 3 --pos 0,74,8 --yaw 3.1416 --pitch -0.35 --edits "set 0,70,0,8; set 0,71,0,8; set 0,72,0,8; set 1,70,0,9; set -1,70,0,10; set 0,69,1,0" --out /tmp/vf_m31_edits.png` → 판자 기둥 3개·유리·벽돌·구멍이 PNG에 보인다.
- `cargo test` 26 passed, clippy `-D warnings` 0, `cargo fmt --check` 통과.
- LOG 기록 후 커밋 `M3.1: review fixes`. 그 다음 **진헌의 손 플레이**(5×5 벽 + 유리창 + 나무 지붕) → 통과하면 M4.

---

## M4 — 성능·물리 (Day 2)

- greedy 메싱(비트마스크 열 기반). culled와 스냅샷 픽셀 동일, 정점 수 ≤ 50%.
- 워커 스레드: `padded()`는 메인, `mesh_chunk`/`generate`는 워커(rayon 또는 std::thread + crossbeam-channel). 메인은 업로드만.
- 걷기 물리: 중력 −28 blk/s², 점프 초속 8.5, 축별 AABB 스윕, 계단 자동 오르기 없음. `F`로 비행 토글.
- 월드젠 나무(LOG+LEAVES), 간단 동굴(3D 노이즈 임계).
- 검증: `cargo test`(greedy 테스트 추가), 스냅샷 diff, R=12에서 60fps.

## M5 — 저장·투명·아레나 (Day 2~3)

- 저장: 수정 청크만 `saves/<name>/c_x_y_z.bin`(lz4_flex) + `world.json`. 종료 시·30초마다 자동.
- 투명 패스: WATER/GLASS 별도 메시, 불투명 뒤에 청크 단위 뒤→앞 정렬, 알파 블렌드, 깊이 쓰기 off. 물은 윗면만 살짝 낮게(0.9).
- 메시 아레나 할당자(큰 VB/IB 하나 + 프리리스트)로 히치 제거.

## M6 — 마크식 조명·낮밤·바람 (Day 3~4)

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
