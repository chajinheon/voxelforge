# GPT-5.6 Sol 시작 프롬프트

아래를 그대로 붙여 넣는다. 작업 폴더는 `~/Projects/voxelforge`를 열어 둔 상태여야 한다.

---


당신은 `voxelforge`의 구현 담당이다. 이 저장소는 맥북(Apple M5, macOS 26)용 마인크래프트식 복셀 건축 게임을 Rust + wgpu 30(Metal)로 만든다. 설계는 이미 확정되어 있고, 당신의 일은 그 설계를 순서대로 구현해 오늘 안에 「건축 가능」(M3)까지 도달하는 것이다.

먼저 순서대로 읽어라: `AGENTS.md` → `docs/BLUEPRINT.md` → `docs/ROADMAP.md` → `docs/LOG.md` 맨 위 항목. 읽기 전에 코드를 쓰지 마라.

지켜야 할 것:

1. BLUEPRINT §1의 확정 결정과 §4 데이터 계약(이름·시그니처·비트 레이아웃), §3 면 순서·코너 표, §5 렌더 계약을 그대로 따른다. 다르게 해야 한다고 판단되면 그 마일스톤은 원안대로 끝내고 LOG에 「제안」으로 남긴다.
2. wgpu 30은 당신이 아는 버전보다 새롭다. BLUEPRINT §11의 차이표를 먼저 보고, 그 밖의 것은 `~/.cargo/registry/src/index.crates.io-*/wgpu-30.0.1/src/api/`를 읽어 확인한다. 기억으로 쓰고 컴파일 에러로 맞추는 방식은 금지. M0의 `src/main.rs`가 동작하는 실제 예다.
3. M1 → M2 → M3 순서. 각 마일스톤의 검증 명령(ROADMAP)이 전부 통과해야 다음으로 간다. 테스트 이름은 BLUEPRINT §8 그대로.
4. 순수 로직은 단위 테스트, 렌더 결과는 `cargo run --release --bin snapshot -- --out /tmp/vf_m1.png`로 PNG를 만들고 직접 열어 확인한다(뒤집힌 면, 구멍, 텍스처 방향).
5. `cargo` 명령이 샌드박스에서 `~/.cargo` 쓰기 실패로 막히면 샌드박스 밖에서 실행한다. 크레이트는 이미 받아져 있다.
6. 새 크레이트는 `cargo add`로 실제 버전을 받고 LOG에 적는다.
7. M3가 끝나면 **멈춘다**. `docs/LOG.md` 맨 위에 형식대로 기록하고, 진헌에게 「M3 완료, 리뷰 요청」이라고 보고한다. M4로 가지 않는다.
8. 중간에 세션이 끊길 수 있다. 마일스톤 하나가 끝날 때마다 LOG를 갱신하고 `git commit`한다(메시지: `M1: static terrain render` 식).

시작: M1부터. 첫 작업은 `src/lib.rs`와 `world/coords.rs` + 테스트 `coords_floor_div_negative`다.

---

# M4→M5 진행 프롬프트 (2026-09-04 13:55, M3 종료 후)

아래를 그대로 붙여 넣는다.

---

`voxelforge` 구현 담당으로 이어서 작업한다. M3는 손 플레이까지 통과해 종료됐다. 이번 세션의 목표는 **M4(성능·물리)와 M5(저장·투명)를 연속으로 끝내는 것**이다.

먼저 읽어라: `docs/LOG.md` 맨 위 두 항목(Claude 13:55 설계, 13:40 리뷰) → `docs/BLUEPRINT.md` **§14 전체**(M4·M5 상세 설계, 계약, 선결정) → `docs/ROADMAP.md`의 M3.1·M4·M5. `AGENTS.md` 규칙은 그대로다.

지켜야 할 것:

1. 순서: **M4.0**(리뷰 잔여 5개) → M4.1 스레드 스트리밍 → M4.2 greedy → M4.3 물리 → M4.4 나무·동굴 → M4.5 프러스텀 → **M5.1** 저장 → M5.2 투명 → M5.3 아레나(조건부). 단계마다 커밋(메시지는 ROADMAP에 적힌 대로). M4가 끝나면 LOG에 기록하고 **멈추지 말고** M5로.
2. §14의 계약(이름·시그니처·상수·비트)과 §14.11 선결정을 따른다. 다르게 해야 하면 원안대로 끝내고 LOG에 「제안」.
3. `main.rs`는 500줄을 넘기지 않는다. 스트리밍은 `src/stream.rs`, 이동은 `src/player/physics.rs`로.
4. 새 크레이트(`rayon`, `serde`, `serde_json`, `lz4_flex`)는 `cargo add`로 실버전을 받고 LOG에 적는다. wgpu 30은 §11 표를 먼저 보고, 없으면 레지스트리 소스를 읽는다.
5. ROADMAP의 완료 조건과 검증 명령을 전부 실제로 돌려 결과를 LOG에 숫자로 적는다(테스트 수, `settled` 초, `max` ms, 픽셀 일치율, 정점 수 비율, `saves/` 파일 수). 스냅샷 PNG는 직접 열어 본다.
6. **M5가 끝나면 멈춘다.** LOG 기록 → 「M5 완료, 리뷰 요청」 보고. M6으로 가지 않는다.

시작: M4.0의 `snapshot --edits`부터.

(결과: M4·M5 완료, 2026-09-04 20:30 리뷰 통과. 커밋 `ad8406f`.)

---

# GPT Pro 설계 확장 프롬프트 — M6~M10 (2026-09-04 20:40, Claude 작성)

ChatGPT(GPT Pro)에 그대로 붙여 넣는다. 저장소 URL은 실제 주소로 바꾼다.

---

당신은 `voxelforge`의 설계 확장 담당(아키텍트)이다. 이 프로젝트는 맥북(Apple M5, macOS 26)용 마인크래프트식 복셀 건축 게임을 Rust + wgpu 30(Metal)로 만든다. 현재 M0~M5(스캐폴드 → 정적 지형 → 카메라·스트리밍 → 건축 → 스레드·greedy·물리·나무·동굴·프러스텀 → 저장·투명 패스)가 구현되고 리뷰까지 통과했다. 저장소: https://github.com/chajinheon/voxelforge (브랜치 `main`).

당신의 일은 코드를 쓰는 게 아니다. **M6~M10의 설계도(BLUEPRINT §15~§19)와 로드맵(ROADMAP M6~M10), 그리고 구현 담당(GPT-5.6 Sol, ultra 추론)에게 줄 실행 지침(KICKOFF)을 문서로 만드는 것**이다. Sol은 당신의 문서만 보고 구현한다. Sol이 추측해야 하는 자리가 남으면 그 문서는 실패다.

## 1. 먼저 읽을 것 (순서대로, 전부)

1. `AGENTS.md` — 역할·절대 규칙·코드 규칙·검증 명령.
2. `docs/BLUEPRINT.md` 전체. 특히 §1 확정 결정(D1~D18), §3 좌표·면 규약, §4 데이터 계약, §5 렌더 계약, §11 wgpu 30 API 차이표, §12 선결정, **§14(M4·M5 상세 설계 — 당신이 써야 할 밀도의 기준)**, §14.11~§14.12.
3. `docs/ROADMAP.md` — M0~M5의 완료 조건·검증 명령 형식. M6~M9는 한 줄 스텁이다(이걸 당신이 채운다).
4. `docs/LOG.md` — 위에서 4개 항목(M4·M5 구현 기록과 리뷰). 무엇이 측정됐고(정착 2.17초, max 17~18ms, greedy 정점 39~67%) 무엇이 미뤄졌는지(아레나 → M7, 수중 틴트 선택).
5. 코드: `Cargo.toml`, `src/lib.rs`, `src/main.rs`, `src/stream.rs`, `src/render/renderer.rs`, `src/render/chunk_pipeline.rs`, `src/render/translucent.rs`, `src/render/globals.rs`, `src/render/textures.rs`, `src/mesh/vertex.rs`, `src/mesh/greedy.rs`, `src/mesh/mesher.rs`, `src/world/world.rs`, `src/world/block.rs`, `src/world/chunk.rs`, `src/world/gen.rs`, `src/world/save.rs`, `src/player/physics.rs`, `src/bin/snapshot.rs`, `assets/shaders/chunk.wgsl`.

## 2. 산출물 (파일 단위, 그대로 저장소에 넣을 수 있게)

**A. `docs/BLUEPRINT.md`에 덧붙일 §15~§19.** 기존 §0~§14는 건드리지 않는다.
- §15 M6 — 마크식 조명(하늘광·블록광 0~15 플러드필)·낮밤·바람.
- §16 M7 — 오프스크린 HDR·G버퍼·디퍼드, CSM 그림자, SSAO, 대기 산란 하늘, ACES 톤매핑·블룸·자동 노출, 렌더 스케일, TAA(선택).
- §17 M8 — 물(파도·SSR·굴절·코스틱), 볼류메트릭 라이트/포그, 볼류메트릭 구름, 원거리 LOD.
- §18 M9 — 복셀 GI(3D 텍스처 clipmap + 컴퓨트 DDA 레이마칭 + 색 조명 + 시간적 누적·디노이즈). 하드웨어 RT는 쓰지 않고 컴퓨트 DDA로 통일한다고 명시.
- §19 M10 — 범위를 당신이 정한다. 목표는 「게임으로 배포 가능한 마감」. 후보: HUD(비트맵 폰트 텍스트·핫바 UI·조준점), 설정(렌더 스케일·반경·감도·키 바인딩) 파일 저장, 일시정지 메뉴, 스크린샷 키, 사운드(걷기·부수기·놓기·물), 성능 프리셋(팬리스 Air용), 유저 셰이더팩 로딩(`assets/shaderpacks/<name>/`), `.app` 번들·아이콘·codesign/notarize 스크립트, MetalFX 업스케일 검토(wgpu-hal Metal 인터롭, 선택). 3~5개로 압축하고 나머지는 「하지 않는 것」에 적는다.

각 절은 §14와 같은 구조를 따른다: (1) 새 모듈 표(경로·역할·마일스톤) → (2) 알고리즘·수식 명세(참조 기법 이름 포함) → (3) Rust/WGSL 계약(시그니처·구조체·비트 레이아웃·바인드 그룹·유니폼 레이아웃·텍스처 포맷) → (4) 상수(전부 숫자) → (5) 테스트 이름과 판정 기준 → (6) 검증 방법(스냅샷 명령 + 픽셀로 잴 수 있는 기대값) → (7) 성능 예산(패스별 ms) → (8) 분기점 선결정 목록(「묻지 말고 이렇게」).

**B. `docs/ROADMAP.md`의 M6~M9 스텁을 교체하고 M10을 추가.** M4·M5 절과 같은 형식: 단계(M6.0, M6.1…)마다 만드는 것과 커밋 메시지, 완료 조건(숫자로), 검증 명령(복붙 가능), **멈춤 지점**(Claude 리뷰). M6.0은 §14.12의 사소 항목으로 시작한다: `snapshot --edits` no-op은 경고로, `MAX_GEN_CHUNK_Y` 상수화 + 테스트, 저장 경로를 프로젝트 루트 기준으로, 투명 파이프라인의 Globals 중복은 M7에서.

**C. `docs/KICKOFF.md`에 덧붙일 「Sol M6→M10 진행 프롬프트」.** 기존 「M4→M5 진행 프롬프트」 형식을 따른다. 리뷰 멈춤: M6 뒤, M7 뒤, M9 뒤(M8·M9는 연속), M10 뒤.

**D. 마지막에 짧게**: (1) 확정 결정을 바꾸자는 「ADR 제안」 목록(없으면 「없음」), (2) 진헌만 정할 수 있는 질문(각각 기본값 제안 포함, 없으면 「없음」), (3) Claude 검토 포인트 5개 이하.

출력 형식: 파일마다 `===== FILE: <경로> (<append | replace 범위>) =====` 헤더 뒤에 본문. 한 응답에 다 안 들어가면 §15~§16 + ROADMAP M6~M7을 먼저, 이어서 §17~§19 + M8~M10 + KICKOFF. 각 응답은 그 자체로 저장소에 넣을 수 있어야 한다.

## 3. 반드시 지킬 제약

- **확정 결정은 바꾸지 않는다**: §1 D1~D18, §12, §14.11, §14.12. 특히 청크 32³, `u16` 블록 ID, 정점 2×u32(비트 23 `lowered` 사용 중, 여분은 a[24..32)와 b[24..32)), `texture_2d_array`, 패딩 34³ 메셔, dynamic-offset 청크 유니폼, `Renderer`가 창을 모르는 구조(스냅샷과 공유), 셰이더 핫리로드, `snapshot` 바이너리. 바꿔야 하면 D에 「ADR 제안」으로 따로.
- **wgpu 30.0.1**은 당신의 학습 데이터보다 새 버전일 수 있다. §11 표에 실제 시그니처가 있다. 표에 없는 API를 설계에 쓸 때는 「Sol이 `~/.cargo/registry/src/*/wgpu-30.0.1/src/api/`에서 확인할 것」을 명시하고, 구버전 이름(`ImageCopyTexture`, `SurfaceTexture::present`, `push_constant_ranges`, `Instance::new(&desc)`)을 쓰지 않는다. WGSL은 naga 기준.
- **에이전트가 검증한다.** 사람이 화면을 보고 판단하는 완료 조건은 금지. 모든 시각 결과는 `cargo run --release --bin snapshot -- …`로 PNG를 만들고 픽셀 값·밝기 비율·영역 비교로 판정할 수 있어야 한다. 그래서 M7부터 `snapshot --view albedo|normal|depth|ao|shadow|light|final` 디버그 뷰를 계약에 넣어라(게임에서는 F4로 순환). 조명·그림자 판정은 「좌표 (x,y)의 픽셀이 기준색의 선형 밝기 k배 ± e」식으로 쓴다 — M3 리뷰가 AO 결함을 정확히 이 방법으로 잡았다(LOG 13:40 참조).
- **Sol은 커밋할 수 없다**(실행 정책). 단계마다 LOG 기록은 필수, 커밋은 리뷰 시점에 Claude가 한다. 그래서 리뷰 멈춤을 1~2 마일스톤마다 둔다.
- 모듈 500줄 규칙(`main.rs` 494줄, `stream.rs` 480줄 — 여유 없음. 새 기능은 새 모듈로), `unsafe` 금지, 렌더 루프 `unwrap` 금지, clippy `-D warnings`, 테스트 이름을 계약으로 명시, 새 크레이트는 `cargo add`로 실버전 + LOG 기록. 코드·주석 영어, 문서 한국어.
- 성능 기준: Apple M5(10코어, 워커 8), 서피스 2560×1440(Retina 2x, `Bgra8UnormSrgb`), 60fps = 16.6ms. M7의 렌더 스케일(0.5~1.0)이 들어가기 전까지는 물리 해상도. 예산은 패스별 ms로 적는다.
- 기존 문서 스타일(번호 절, 표, 계약 코드 블록, 「」 인용, 분기점 선결정 목록)을 그대로 따른다. 새 문서 파일은 만들지 않는다.

## 4. 각 마일스톤에서 꼭 결정해 줄 것 (Sol이 묻지 않게)

- **M6**: 광원 전파(BFS, 블록광 감쇠 1/블록, 하늘광은 아래로 무감쇠·옆으로 1 감쇠), 저장 위치(청크당 4+4비트 `u8` 배열 → `Chunk` 필드 추가, `PaddedChunk`에도 포함), 청크 경계 전파와 재메싱 트리거(dirty·epoch 활용), greedy 키에 light·sky 추가(§14.3에서 예고), 정점 b[16..24) 사용, 셰이더의 `max(sky × sun_factor, light)` 곡선과 마크식 감마 표, 낮밤 주기 길이(초)·`sun_dir` 궤도 식·하늘색 보간 표, 바람 정점 변위 식(잎·풀, `time` 사용), TORCH 블록(§4 HOTBAR 갱신, 광원 14), 저장 포맷 버전 2 여부(조명은 저장하지 않고 로딩 시 재계산 — 결정해 줄 것), 조명 재계산 비용의 스레드 배치.
- **M7**: 오프스크린 타깃·G버퍼 포맷(`Rgba16Float` HDR, normal·material 인코딩), 렌더 스케일 파이프(오프스크린 → 업스케일 블릿, 서피스는 UI만), CSM 캐스케이드 수·분할 방식·해상도·바이어스·PCF, SSAO 커널·반경·해상도·블러, 대기 산란 모델(Preetham/Hosek-Wilkie 또는 단순 Rayleigh-Mie)과 계수, ACES 피팅 식(Narkowicz 등), 블룸 다운샘플 단계 수·임계, 자동 노출(히스토그램 vs 평균, 적응 속도), TAA 여부(선택이면 조건), 투명 패스와 디퍼드의 결합 방식(포워드 투명 유지).
- **M8**: 물 파도 함수(Gerstner 합 또는 노이즈, 진폭·파장·속도), SSR 스텝 수·두께·페이드, 굴절 왜곡 크기, 코스틱 방식, 볼류메트릭 레이마칭 스텝·해상도(1/4)·블루노이즈·시간적 블러, 구름 노이즈(Worley/Perlin 조합)·레이어 고도·커버리지, LOD 단계(2×/4×/8×)와 청크 다운샘플 규칙(최빈 블록)·이음새 처리·거리 임계·메모리.
- **M9**: clipmap 해상도·레벨 수·업데이트 정책(카메라 이동 시 토로이달 갱신), DDA 레이 수/길이/코사인 가중, 색 조명 표현(RGB 강도 텍스처), 시간적 누적 알파와 리프로젝션(깊이·법선 검사), 디노이즈(à-trous 단계 수), 실패 시 폴백(M6 조명).
- **M10**: 위 후보에서 고른 범위, 폰트 소스(내장 8×8 비트맵 절차 생성 또는 PNG), 설정 파일(`settings.json` 스키마), 번들 스크립트(`scripts/bundle.sh`, `Info.plist`, 아이콘 생성), 사운드 크레이트(`cargo add`로 실버전 확인 지시), 셰이더팩 디렉터리 규약과 로딩 순서, 성능 프리셋 표.

## 5. 품질 기준

Sol이 §14만 보고 M4·M5를 구현해 첫 리뷰에서 결함 0(사소 4개)으로 통과했다. 당신의 §15~§19도 같은 결과가 나와야 한다. 문장마다 「Sol이 이 줄만 보고 코드를 쓸 수 있나」를 물어라. 「적절히」, 「자연스럽게」, 「필요에 따라」 같은 말 대신 숫자·식·이름을 쓴다.

# Sol M6→M10 진행 프롬프트 (2026-09-04, M5 리뷰 통과 후)

아래를 그대로 붙여 넣는다.

---

당신은 `voxelforge`의 구현 담당이다. 현재 `main`은 M0~M5 구현과 Claude 리뷰를 통과했다. 이번 프롬프트는 M6~M10의 장기 진행 지침이지만, 리뷰 멈춤선을 절대로 건너뛰지 않는다.

먼저 순서대로 읽어라.

1. `AGENTS.md`
2. `docs/LOG.md` 맨 위 M4·M5 리뷰 및 구현 기록 4개
3. `docs/BLUEPRINT.md` §1, §3~§5, §11~§12, §14.11~§14.12
4. 현재 허가된 마일스톤의 상세 절:

   * M6: §15
   * M7: §16
   * M8·M9: §17~§18
   * M10: §19
5. `docs/ROADMAP.md`의 해당 마일스톤

읽기 전에 코드를 수정하지 마라.

절대 규칙:

1. D1~D18, §12, §14.11, §14.12를 바꾸지 않는다. 필요하다고 생각해도 원안을 구현한 뒤 LOG의 「ADR 제안」에만 적는다.
2. 청크 32³, `u16` ID, 34³ `PaddedChunk`, 정점 2×u32, `a[23] lowered`, `b[16..20] block light`, `b[20..24] sky light`, `texture_2d_array`, dynamic-offset 청크 uniform을 유지한다.
3. `Renderer`에 `Window`, `Surface`, winit 타입을 넣지 않는다. 창과 snapshot은 같은 Renderer를 쓴다.
4. `main.rs`와 `stream.rs`는 이미 한계에 가깝다. 500줄을 넘기지 말고 BLUEPRINT의 새 모듈로 분리한다.
5. 우리 코드의 `unsafe`는 금지한다. 렌더 루프에 `unwrap`·`expect`를 넣지 않는다. 초기화 실패는 `anyhow::Result`, 프레임 오류는 로그·skip·기능 폴백으로 처리한다.
6. wgpu 30은 기억으로 쓰지 않는다. §11 표에 없는 API를 쓰기 전에 `~/.cargo/registry/src/index.crates.io-*/wgpu-30.0.1/src/api/`와 `wgpu-types-30.0.1/src/`를 직접 읽는다. 구버전 이름 `ImageCopyTexture`, `SurfaceTexture::present`, `push_constant_ranges`, `Instance::new(&desc)`를 쓰지 않는다.
7. WGSL은 naga가 검증하는 문법만 쓴다. shader/pipeline 생성·핫리로드는 error scope로 감싸고 실패 시 이전 묶음을 유지한다.
8. 새 크레이트는 `cargo add`로 실제 버전을 설치하고 그 명령·버전을 LOG에 기록한다.
9. 코드와 주석은 영어, 문서는 한국어다.
10. 모든 순수 로직은 BLUEPRINT에 적힌 이름 그대로 단위 테스트를 만든다.
11. 시각 완료 조건은 사람 눈으로 판정하지 않는다. ROADMAP의 snapshot 명령과 Python 픽셀 판정을 실제 실행해 숫자를 LOG에 남긴다.
12. 성능은 Apple M5, 2560×1440, release, ROADMAP의 render scale·preset으로 측정한다. GPU timing은 지원될 때 비동기 timestamp를 쓰며 렌더 루프에서 readback을 기다리지 않는다.
13. Sol은 `git commit`을 실행하지 않는다. 각 단계가 끝날 때 LOG에 의도한 커밋 메시지를 기록한다. 실제 커밋은 리뷰 시 Claude가 한다.
14. 단계 하나마다 `cargo test --all-targets`, 영향 범위 snapshot, LOG 기록을 한다. 마일스톤 끝에서는 clippy·fmt·diff까지 전부 실행한다.
15. 설계에서 선택으로 남긴 것은 없다. 다른 방식을 임의로 택하지 않는다.

진행 순서와 멈춤선:

## 첫 세션 — M6만

순서:

```text
M6.0 → M6.1 → M6.2 → M6.3 → M6.4
```

M6.0 첫 작업:

1. `snapshot --edits` no-op을 warning으로 변경.
2. `MAX_GEN_CHUNK_Y = 4`와 테스트.
3. 저장 기본 root를 프로젝트 root로 변경.
4. 투명 Globals 중복은 건드리지 않고 M7 대상으로 남김.

그 뒤 §15의 열 조명 solver부터 구현한다.

M6 완료 조건과 BLUEPRINT §15.6 픽셀 검증을 전부 통과시키고 LOG에 다음 수치를 기록한다.

```text
test count
R=12 settled seconds
light column median/p95 ms
max boundary requeue
M5 대비 정점 비율
steady max frame ms
각 snapshot 경로와 픽셀 비율
```

M6가 끝나면 반드시 멈추고 다음 한 줄로 보고한다.

```text
M6 완료, 리뷰 요청
```

Claude의 명시적 승인 전에는 M7 파일을 만들지 않는다.

## 두 번째 세션 — 승인 뒤 M7만

먼저 갱신된 LOG와 Claude 리뷰를 읽는다. 순서:

```text
M7.0 → M7.1 → M7.2 → M7.3 → M7.4 → M7.5
```

M7.0에서 먼저 opaque/translucent Globals와 청크 uniform arena를 통합한다. VB/IB arena는 ROADMAP의 수치 게이트를 실제 실행한 결과로만 결정한다.

M7에서 지형을 surface에 직접 그리는 경로를 제거한다. surface에는 native LDR present만 한다. `Renderer`는 surface를 소유하지 않는다.

TAA는 구현하지 않는다. F4 순서와 snapshot view 이름을 계약대로 고정한다.

M7 완료 시 다음을 LOG에 기록한다.

```text
arena gate p99 frame/upload와 선택
각 GPU pass median/p95
normal/depth/AO/shadow 픽셀 값
auto exposure 결과
render scale 내부 크기
전체 p95/max
```

완료 뒤 멈추고:

```text
M7 완료, 리뷰 요청
```

Claude 승인 전에는 M8로 가지 않는다.

## 세 번째 세션 — 승인 뒤 M8·M9 연속

먼저 갱신된 LOG와 M7 리뷰를 읽는다.

순서:

```text
M8.1 → M8.2 → M8.3 → M8.4 → M8.5
→ M9.1 → M9.2 → M9.3 → M9.4
```

M8 종료 시 LOG는 기록하지만 리뷰 요청으로 멈추지 않는다.

M8 주의:

* WATER는 GLASS와 분리한다.
* lowered bit를 유지한다.
* 물 surface greedy는 최대 2×2.
* 볼류메트릭과 구름은 internal 1/4.
* blue-noise는 외부 파일이 아니라 §17 알고리즘으로 생성한다.
* LOD는 2×/4×/8× 재귀 최빈값과 32블록 overlap·skirt다.
* LOD cache hard cap을 넘기지 않는다.

M9 주의:

* 하드웨어 RT, Metal acceleration structure, ray query, `wgpu-hal`을 사용하지 않는다.
* WGSL compute DDA만 사용한다.
* 128³×4 clipmap.
* balanced 4 rays, 48블록, 96 crossing.
* temporal history 0.90, à-trous 3단계.
* 필수 기능 실패 시 GI 전체를 끄고 M6 baked light + SSAO로 폴백한다.

M9 완료 시 LOG:

```text
water/volumetric/cloud/LOD pass p95
LOD CPU/GPU memory
clipmap memory와 upload bytes/frame
GI trace/temporal/denoise p95
GI room 밝기·색 비율
temporal noise 감소율
전체 p95/max
fallback simulation 결과
```

완료 뒤 멈추고:

```text
M8·M9 완료, 리뷰 요청
```

Claude 승인 전에는 M10으로 가지 않는다.

## 네 번째 세션 — 승인 뒤 M10

먼저 갱신된 LOG와 M9 리뷰를 읽는다.

순서:

```text
M10.1 → M10.2 → M10.3 → M10.4 → M10.5
```

M10 주의:

* `cargo add font8x8`, `cargo add rodio`의 실제 버전을 LOG에 적는다.
* F3는 콘솔 통계다. 디버그 텍스트 오버레이로 바꾸지 않는다.
* ESC는 pause/resume이며 두 번째 ESC 종료를 제거한다.
* 설정은 atomic JSON.
* 사운드는 절차 PCM이고 장치 실패는 silent fallback.
* 셰이더팩 교체는 전체 transactional swap.
* `.app` launcher가 `VF_ASSETS` 등 경로를 설정해 D18을 유지한다.
* MetalFX는 구현하지 않는다.
* signing identity·notary profile이 없으면 ad-hoc bundle 검증까지만 수행한다. notarize 성공을 거짓으로 기록하지 않는다.

M10 완료 시 LOG:

```text
test count
HUD/pause pixel 판정
settings roundtrip/atomic 결과
audio WAV RMS/peak
shaderpack failure fallback
bundle path
plutil/codesign 결과
Balanced p95/max
메모리 추정
notarize 실행 여부와 실제 결과
```

완료 뒤 멈추고:

```text
M10 완료, 최종 리뷰 요청
```

Claude가 최종 검증·커밋하기 전에는 프로젝트 완료라고 선언하지 않는다.

시작: M6.0의 `snapshot_noop_edit_is_warning` 테스트부터 fail-first로 작성하라.

---

## ADR 제안

없음.

M10 `.app`의 asset 경로는 D18을 바꾸지 않고 launcher가 `VF_ASSETS`를 설정한다. MetalFX는 현행 `unsafe` 금지와 Renderer 계약을 유지하기 위해 범위에서 제외했다.

## 진헌만 정할 수 있는 질문

없음.

서명·notarize에 필요한 Apple Developer identity와 keychain profile은 설계 결정이 아니라 배포 시 환경 입력이다. 기본은 ad-hoc 서명이며, 값이 제공됐을 때만 정식 서명·notarize를 실행한다.

## Claude 검토 포인트

1. M6 광원 제거가 열 경계 고정점에서 15회 이내 실제로 0에 수렴하며 조명 변경이 저장 `modified/version`을 오염시키지 않는지.
2. M7의 공유 `SceneBindings`, G버퍼 포맷, wgpu 30 bind-group·timestamp API가 실소스와 맞고 surface direct geometry가 완전히 제거됐는지.
3. M8 WATER의 HDR read/write feedback 회피, 볼류메트릭 temporal reject, LOD overlap·skirt·메모리 cap이 계약대로인지.
4. M9가 하드웨어 RT 없이 compute DDA만 사용하고 clipmap toroidal wrap·RGB emission·전체 GI 폴백이 정확한지.
5. M10 launcher가 D18과 배포 쓰기 경로를 동시에 지키며 셰이더팩·설정·스크린샷·오디오 실패가 게임 종료로 이어지지 않는지.
# Sol M7→M10 무중단 구현 프롬프트 — M6 리뷰·커밋 완료 후

아래를 GPT-5.6 Sol ultra에 그대로 붙여 넣는다.

---

당신은 `voxelforge`의 구현 담당이다. M6는 구현·자동 검증·Claude 리뷰·커밋까지 완료됐다. 지금부터의 목표는 **M7, M8, M9, M10을 사람 승인 대기 없이 한 번에 연속 구현하고, M10 최종 검증 뒤에만 멈추는 것**이다.

당신은 설계자가 아니다. 설계 선택, 범위 재협상, 사용자 질문, 대안 비교를 하지 않는다. `docs/BLUEPRINT.md §20`과 `docs/ROADMAP.md M7~M10`이 구현 명세다. §16~§19와 충돌하면 §20이 우선한다. 문서에 적힌 숫자·형식·ID·수식·입력·검증을 그대로 실행한다.

## 0. 시작 전에 반드시 읽을 것

순서:

1. `AGENTS.md` 전체.
2. `docs/LOG.md` 위에서 최소 6개 항목. M6 구현·리뷰·커밋 결과와 현재 HEAD를 확인한다.
3. `docs/BLUEPRINT.md` §1, §3~§5, §11~§12, §14.11~§14.12, §15 전체.
4. `docs/BLUEPRINT.md` §16~§20 전체. 최종 우선권은 §20.
5. `docs/ROADMAP.md` M7~M10 전체.
6. 현재 코드 중 `Cargo.toml`, `src/lib.rs`, `src/main.rs`, `src/stream.rs`, `src/render/**`, `src/mesh/**`, `src/world/**`, `src/player/**`, `src/bin/snapshot.rs`, `assets/shaders/**`.

읽기 전에 파일을 수정하지 않는다. 읽은 뒤 첫 LOG 항목에 다음을 기록한다.

```text
Sol M7→M10 무중단 실행 시작
base HEAD: <sha>
M6 tests: <count>
M6 settled/perf: <LOG 수치>
설계 우선순위: BLUEPRINT §20
중간 리뷰 대기: 없음
최종 정지: M10 이후
```

## 1. 절대 실행 규칙

1. **M7→M8→M9→M10을 연속으로 간다.** M7, M8, M9 완료 시 LOG와 검증은 남기지만 절대로 멈추거나 리뷰를 요청하지 않는다.
2. M10의 모든 최종 검증이 끝난 뒤에만 멈춘다. 최종 보고 문구는 정확히 `M7~M10 완료, 최종 리뷰 요청`이다.
3. 중간에 설계가 크거나 시간이 오래 걸린다는 이유로 범위를 줄이지 않는다. 컨텍스트가 압축되면 LOG와 ROADMAP을 다시 읽고 다음 미완료 단계부터 계속한다.
4. 사용자에게 질문하지 않는다. 문서의 「묻지 말고 이렇게」 결정을 따른다.
5. Sol은 `git commit`, `git push`, branch 생성, reset, rebase를 실행하지 않는다. 각 단계 LOG에 의도한 커밋 메시지만 기록한다.
6. 단계 검증이 실패하면 다음 단계로 넘어가지 않는다. 원인을 수정하고 같은 검증을 다시 실행해 통과시킨 뒤 계속한다.
7. D1~D18, §12, §14.11~§14.12, §15를 임의 변경하지 않는다. 정말 충돌한다고 판단해도 현재 마일스톤에서는 원안대로 구현하고 LOG의 `ADR 제안`에만 적는다.
8. `Chunk`는 32³, block storage는 `u16`, `PaddedChunk`는 34³, vertex는 2×u32, `a[23] lowered`, M6 light bits, dynamic-offset chunk uniform, `texture_2d_array`를 유지한다.
9. `Renderer`는 `Window`, `Surface`, winit event를 소유하거나 import하지 않는다. 창과 `snapshot`이 같은 Renderer를 사용한다.
10. `main.rs`, `stream.rs`, `renderer.rs`를 포함해 한 모듈 500줄 이하. 400줄에 도달하면 다음 기능을 넣기 전에 하위 모듈로 분리한다.
11. 우리 코드에 `unsafe` 금지. 렌더 루프의 `unwrap/expect` 금지. 초기화는 `anyhow::Result`, 프레임 오류는 로그 후 기능 폴백 또는 해당 프레임 skip.
12. wgpu 30 API를 기억으로 쓰지 않는다. §11에 없는 texture array layer view, storage texture feature, timestamp query, query resolve, map/readback, 3D texture copy API는 먼저 `~/.cargo/registry/src/index.crates.io-*/wgpu-30.0.1/src/api/`와 `wgpu-types-30.0.1/src/`에서 확인한다. 확인한 파일·타입을 LOG에 적는다.
13. 구버전 이름 `ImageCopyTexture`, `SurfaceTexture::present`, `push_constant_ranges`, `Instance::new(&desc)`를 쓰지 않는다.
14. WGSL은 naga 기준. 임의 include 문법을 만들지 않는다. 공통 WGSL은 Rust가 파일 문자열을 순서대로 결합한다.
15. shader/pipeline 생성과 hotreload는 error scope로 검증한다. 하나라도 실패하면 새 묶음 전체를 버리고 이전 묶음을 유지한다.
16. 새 크레이트는 `cargo add`로 실버전을 받아야 한다. M10에서 `font8x8`, `rodio`가 필요하다. 실제 명령과 resolved version을 LOG에 적는다.
17. 순수 로직은 BLUEPRINT에 적힌 테스트 이름을 그대로 만든다. 테스트 이름을 생략·통합·개명하지 않는다.
18. 시각 결과를 사람 눈으로 완료 처리하지 않는다. `snapshot` PNG, fixture probe, Python 픽셀 판정을 실제 실행하고 수치를 LOG에 적는다.
19. 성능 수치를 debug build로 재지 않는다. Apple M5, release, 2560×1440, `m5_air_high`, R=10에서 ROADMAP의 warmup/측정 구간을 쓴다.
20. GPU timing readback을 렌더 스레드에서 기다리지 않는다. 비동기 ring을 사용하고 미지원 시 게임은 실행하되 명확한 폴백을 기록한다. 대상 Apple M5 최종 검증에서는 timing이 실제로 있어야 한다.
21. 품질 예산을 넘는다고 render scale, ray 수, shadow 해상도, 아이템 수, 효과 범위를 임의 하향하지 않는다. BLUEPRINT §20.13의 최적화 순서로 병목을 제거한다.
22. M10 건축 UX는 선택 기능이 아니다. 122개 item, I 인벤토리, 1/16 shape, hand/viewmodel, hotbar, icon이 하나라도 빠지면 완료가 아니다.

## 2. LOG 방식

각 `Mx.y` 시작과 종료에 LOG 맨 위 항목을 추가한다. 종료 항목 형식:

```text
## <날짜 시각> — Sol — Mx.y 완료

- 만든 것:
- 변경 모듈과 줄 수:
- 새 테스트와 전체 테스트 수:
- 실행한 명령:
- snapshot 경로와 픽셀/fixture probe 결과:
- CPU/GPU/메모리 수치:
- wgpu 30 실제 API 확인:
- 의도한 커밋 메시지:
- 다음 단계: Mx.z
- ADR 제안: 없음 또는 제안 내용
```

M7, M8, M9 마지막 항목에는 다음 문구만 쓰고 바로 진행한다.

```text
검증 완료 — 무중단 규칙에 따라 다음 마일스톤 진행
```

`리뷰 요청`, `사용자 확인 필요`, `잠시 중단`, `다음 세션`을 쓰지 않는다.

## 3. 실행 순서

정확히 다음 순서다.

```text
M7.0 → M7.1 → M7.2 → M7.3 → M7.4
→ M8.0 → M8.1 → M8.2 → M8.3 → M8.4 → M8.5
→ M9.0 → M9.1 → M9.2 → M9.3 → M9.4
→ M10.0 → M10.1 → M10.2 → M10.3 → M10.4 → M10.5
```

### M7 핵심

M7은 M5 Air용 렌더 기반을 만든다.

- duplicated Globals/chunk uniform부터 통합한다.
- 세 개의 16×16 material arrays와 5 mip을 만든다.
- G-buffer는 albedo, normal/roughness/emission, light/AO/material, motion, reactive, depth다.
- POM은 full opaque/cutout에만 preset 수치대로 적용한다.
- 직접광은 GGX PBR다.
- CSM cascade cadence를 구현한다.
- SSAO가 아니라 GTAO다.
- atmosphere LUT 3종과 sky cubemap을 만든다.
- TAA는 선택이 아니다. native HDR TAAU와 Halton 8 sequence를 구현한다.
- hand와 UI는 아직 world TAA history에 넣지 않는다.
- `m5_air_high` 내부 크기는 1848×1040이다.

M7.4 검증이 통과하면 LOG 후 즉시 M8.0.

### M8 핵심

- linear depth pyramid와 deterministic blue-noise부터 만든다.
- WATER는 GLASS와 분리한다.
- Gerstner 네 파동, SSR 40+5, 굴절, Beer–Lambert, foam, caustic, underwater를 구현한다.
- fog와 cloud는 quarter-resolution checkerboard temporal이다.
- cloud shadow 512², 8-frame cadence.
- LOD는 2×/4×/8× recursive modal, 32-block overlap, skirt, cache cap이다.
- M8 효과를 제거해 성능을 맞추지 않는다.

M8.5 검증이 통과하면 LOG 후 즉시 M9.0.

### M9 핵심

- 하드웨어 RT, `wgpu-hal`, Metal acceleration structure를 사용하지 않는다.
- 4×128³ material/light clipmap.
- 토로이달 slab update, 4MiB/frame hard cap.
- WGSL compute DDA, quarter-resolution, 4 rays, 48 blocks, 96 crossings.
- six emissive materials의 RGB radiance는 §20 표 그대로.
- temporal 0.90, à-trous 1/2/4.
- 필수 기능 실패 시 GI 전체를 끄고 M6 light+GTAO로 폴백한다. 반쯤 초기화된 GI를 유지하지 않는다.

M9.4 검증이 통과하면 LOG 후 즉시 M10.0.

### M10 핵심 — 우선순위가 가장 높다

M10은 먼저 건축 시스템을 완성하고, 그 다음 배포 마감을 한다.

#### M10.0 레지스트리

- BlockId 0~88과 상태 range를 한 글자도 임의 변경하지 않는다.
- ItemId 1~122를 정확히 만든다.
- ItemId와 BlockId를 같은 것으로 취급하지 않는다.
- 기존 ID 0~12와 save compatibility를 유지한다.
- 122개 모두 icon·placement·category·English name이 있어야 한다.
- texture recipe와 PBR palette를 §20대로 구현한다.

#### M10.1 shape

- non-cube는 16³ occupancy다.
- 정점 spare bits에 frac_x/y/z를 넣는다.
- old cube frac은 0.
- full cube가 partial neighbor와 맞닿을 때 z-fighting이 없도록 face coverage subtraction을 한다.
- micro AO는 shape occupancy를 읽는다.
- shape는 axis log, slab, stair, pane, fence만 구현한다. 임의 모델 시스템으로 범위를 넓히지 않는다.

#### M10.2 상호작용

- player collision이 shape AABB를 순회한다.
- fence collision 1.5 blocks.
- DDA cell hit 뒤 shape AABB 정확 hit를 한다.
- slab merge, stair orientation, pane/fence connection state를 계약대로 처리한다.
- connection state 자동 변경까지 save modified/version에 반영하되 한 사용자 edit당 chunk version 한 번.
- middle mouse pick block을 구현한다.

#### M10.3 인벤토리

- I 키로 modal inventory.
- 9×6 grid, 54 visible items.
- 7 category + All, ASCII search, scroll, tooltip.
- item click은 selected hotbar slot에 배치.
- hovered item+number 1..9는 해당 slot에 배치.
- hotbar와 crosshair는 항상 보인다.
- 122 icon layer를 시작 시 500ms 안에 bake한다.
- inventory open 중 world time/physics는 멈추지만 renderer, streaming result apply, save, shader reload는 계속한다.
- Escape는 inventory를 먼저 닫는다.

#### M10.4 손

- native LDR 오른손과 held item.
- world TAA history에 포함하지 않는다.
- FOV 68, transform·duration·priority는 §20 그대로.
- idle/walk/break/place/switch 전부 구현한다.
- held item은 cube뿐 아니라 slab/stair/pane/fence shape를 보여준다.
- settings version 2, hotbar persistence, sound, screenshot도 이 단계에서 구현한다.

#### M10.5 마감

- shaderpack contract 2.
- material/emission/hand override.
- pack 실패 시 이전/builtin 유지.
- `.app`, launcher, Info.plist, icon, ad-hoc codesign.
- notary credential이 없으면 실행하지 않았다고 LOG에 정확히 적는다. 성공했다고 꾸미지 않는다.
- 3600-frame creative-build autopilot 최종 측정.

## 4. 오류·폴백 정책

아래는 질문하거나 멈출 이유가 아니다.

- timestamp 미지원: 게임은 실행. 단, Apple M5 최종 장치에서 재확인하고 timing이 없으면 해당 단계 미완료로 수정한다.
- GI format/pipeline 실패: GI 전체 fallback 후 M10까지 계속. fallback 테스트와 로그 필수.
- audio device 실패: silent fallback 후 계속.
- shaderpack 실패: previous/builtin 유지 후 계속.
- user asset 없음: procedural asset 사용.
- notary credential 없음: ad-hoc bundle/codesign까지 검증하고 notarize skipped를 기록.
- save/settings directory 없음: 생성. HOME 없음: 개발 root 또는 memory fallback을 문서 계약대로 사용.
- screenshot ring full: 네 번째 요청 drop+warning, 렌더 계속.

컴파일 오류를 API 탐색 방식으로 해결하지 않는다. wgpu 관련이면 먼저 실제 registry source를 읽고 한 번에 올바른 API로 고친다.

## 5. 성능 최적화 순서

`m5_air_high` p95가 예산을 넘으면 다음 순서를 지킨다.

1. 매 프레임 생성되는 texture/view/bind-group/pipeline/buffer 제거.
2. duplicated Globals·chunk uniform·scene copy 제거.
3. CSM far cascade, atmosphere, cloud shadow cadence가 계약대로 작동하는지 확인.
4. fullscreen pass fusion: 가능한 경우 tone/grade/sharpen, temporal/composite를 한 pass로 결합.
5. workgroup 8×8/16×16 비교를 실제 timestamp로 측정.
6. storage texture 왕복·texture sampling 중복 제거.
7. draw list Vec 재사용, sort key packed u64, queue.write_buffer batch.
8. clipmap slab 중복·LOD duplicate generation 제거.
9. shader divergent branch를 material flags와 `select`로 축소.
10. CPU profile에서 snapshot copy, mesh upload, UI geometry allocation 제거.

설정 수치 자체를 낮추는 것은 금지다. `cinematic`만 60fps 예산 밖이다.

## 6. 최종 완료 조건

다음이 모두 참이어야 종료한다.

- ROADMAP M7.0~M10.5의 각 완료 조건 통과.
- 전체 테스트 ≥185.
- clippy `-D warnings`, fmt check, diff check 통과.
- 122 item exact.
- shape gallery probe 전부 통과.
- I inventory all 및 `Shapes + stair` 픽셀 검증 통과.
- viewmodel idle/break/place crop 검증 통과.
- M6 lighting regression 통과.
- shaderpack broken-pack fallback 통과.
- `.app` plutil/codesign/launcher smoke 통과.
- `m5_air_high`, 2560×1440, R=10:
  - settled <5.0s.
  - HUD GPU p95 ≤15.40ms.
  - inventory GPU p95 ≤15.75ms.
  - full frame p95 ≤16.6ms.
  - max <25ms.
  - memory <1.35GiB.
- LOG에 모든 timing·pixel·memory·version·bundle 결과 기록.

완료 후:

1. `git status --short`를 LOG에 복사한다.
2. `git diff --stat`를 LOG에 복사한다.
3. commit은 하지 않는다.
4. LOG 맨 위에 `M7~M10 완료, 최종 리뷰 요청`을 기록한다.
5. 진헌에게 정확히 같은 문구와 검증 수치 요약만 보고한다.

시작하라. 첫 작업은 M7.0의 M6 baseline 전체 검증과 현재 renderer resource ownership 지도 작성이다. 그 뒤 M10까지 멈추지 않는다.

---
