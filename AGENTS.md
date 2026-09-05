# voxelforge — 에이전트 공통 지침

맥북(Apple Silicon)용 마인크래프트식 복셀 건축 게임. Rust + wgpu(Metal). 에디터 없음, 전부 텍스트.
이 파일은 이 저장소에서 작업하는 **모든 에이전트(Claude, GPT-5.6 Sol, 기타)** 가 먼저 읽는다.

## 필독 순서

1. 이 파일
2. `docs/BLUEPRINT.md` — 확정 설계도. 모듈 지도·데이터 계약·렌더 계약·선결정된 분기
3. `docs/ROADMAP.md` — 마일스톤 M0~M9, 각각의 완료 조건과 검증 명령
4. `docs/LOG.md` — 마지막 항목만. 직전 세션이 어디까지 했고 무엇이 열려 있는지

## 역할

- **진헌** — 방향 결정, 플레이 테스트, 우선순위. 최종 판단.
- **Claude (설계·리뷰)** — BLUEPRINT/ROADMAP 소유(M0~M5 설계 원본). 마일스톤 결과를 `cargo test` + 스냅샷 PNG + 픽셀 측정 + 코드 읽기로 검수하고 LOG에 리뷰를 남긴다. 설계 변경은 여기서 확정한다. **커밋도 Claude가 리뷰 시점에 한다**(Sol의 실행 정책이 `git commit`을 막음).
- **GPT Pro (설계 확장, M6~M10)** — 2026-09-04 M5 이후 합류. GitHub의 이 저장소를 읽고 BLUEPRINT §15~§19와 ROADMAP M6~M10, Sol용 KICKOFF를 **§14와 같은 밀도로** 초안한다. 확정 결정(§1 D1~D18, §14.11)을 바꾸려면 「ADR 제안」으로 따로 표시한다. 초안은 Claude 검토 후 확정. 프롬프트는 `docs/KICKOFF.md` 하단.
- **GPT-5.6 Sol (구현)** — ROADMAP 순서대로 구현. 각 마일스톤마다 검증 명령을 통과시키고 LOG에 기록한다. 설계와 다르게 해야 하면 LOG에 이유를 쓰고 진행한다(막히지 않는다).

## 절대 규칙

1. `BLUEPRINT.md §1 확정 결정`은 임의로 바꾸지 않는다. 바꿔야 하면 LOG에 「제안: … 이유: …」를 남기고, 그 마일스톤 안에서는 원안대로 간다.
2. 마일스톤은 순서대로. 검증 명령(ROADMAP)을 통과하지 않은 채 다음 마일스톤으로 가지 않는다.
3. 세션을 끝낼 때 LOG에 기록하지 않고 끝내지 않는다. 형식은 `docs/LOG.md` 상단 참조.
4. **wgpu 30 API는 기억으로 쓰지 않는다.** 내 학습 데이터보다 새 버전이다. 확실치 않으면 `~/.cargo/registry/src/*/wgpu-30.0.1/src/api/*.rs`를 읽는다. 이미 확인된 차이는 `BLUEPRINT.md §11`에 있다.
5. 순수 로직(좌표, 청크, 메셔, 레이캐스트, 패킹, 월드젠)은 반드시 단위 테스트를 동반한다. 렌더 결과는 `snapshot` 바이너리로 PNG를 뽑아 확인한다.
6. 한 마일스톤에서 새 크레이트를 추가할 때는 `cargo add`로 실제 최신 버전을 받고 LOG에 적는다. 버전 추측 금지.
7. 이 저장소 밖에 파일을 만들지 않는다. 임시 산출물은 `/tmp`.

## 코드 규칙

- Rust 1.95 stable, edition 2024. nightly 금지. `unsafe` 금지(wgpu가 요구하는 곳 없음).
- 렌더 루프 안에서 `unwrap()`/`expect()` 금지 — `match` + `log::error!`. 초기화 단계는 `anyhow::Result`.
- 모듈 하나 500줄 이하. 넘으면 나눈다.
- 코드·주석·식별자는 영어. 문서(`docs/`, `AGENTS.md`, LOG)는 한국어.
- `cargo fmt` 적용, `cargo clippy --all-targets`에 경고 0을 목표로 한다.
- 엔진 크레이트(bevy 등)·ECS 도입 금지. 평범한 구조체와 함수로 간다(선결정, BLUEPRINT §12).

## 검증 명령 (복붙용)

```bash
cd ~/Projects/voxelforge
cargo build                                     # 컴파일
cargo test                                      # 순수 로직 테스트
cargo clippy --all-targets                      # 경고 0 목표
VF_SMOKE_FRAMES=3 cargo run                     # 창 열고 3프레임 뒤 자동 종료, 로그에 "backend: Metal"
cargo run --release --bin snapshot -- --out /tmp/vf.png   # 오프스크린 렌더 → PNG (M1부터 존재)
cargo run --release                             # 실제 플레이
```

샌드박스 메모: `cargo`는 `~/.cargo`에 쓰기 때문에 샌드박스 안에서 실패할 수 있다. 그럴 때만 샌드박스 밖(all 권한)으로 실행한다. 크레이트는 이미 다 받아져 있다(`cargo fetch` 완료).

## 문서 지도

- `docs/BLUEPRINT.md` — 설계도. §0~§14 Claude 원본(M0~M5), §15~ GPT Pro 초안 → Claude 확정
- `docs/ROADMAP.md` — 마일스톤·완료 조건·검증 명령. M0~M5 완료, M6~M10 확장 중
- `docs/LOG.md` — 세션 로그, 위에 새 항목 추가 (모두 기록)
- `docs/KICKOFF.md` — 세션 시작용 프롬프트 모음 (Sol 구현용, GPT Pro 설계 확장용)

## M6 이후 실행 예외 (M7~M10)

M6 리뷰·커밋 이후 M7~M10은 하나의 연속 구현 구간이다.

- Sol은 M7, M8, M9 종료 시 ROADMAP 검증과 LOG 기록을 완료하되 사람 리뷰를 기다리지 않고 다음 마일스톤으로 간다.
- M7→M8→M9→M10 순서는 바꾸지 않으며, 각 마일스톤 검증이 실패하면 통과할 때까지 같은 단계에서 수정한다.
- 중간 커밋은 만들지 않는다. 각 단계의 의도한 커밋 메시지만 LOG에 적는다.
- Claude 리뷰·실제 커밋은 M10 전체 종료 뒤 한 번 수행한다.
- 이 실행 예외는 검증 생략을 뜻하지 않는다. 테스트·clippy·fmt·snapshot·픽셀 판정·성능 측정은 ROADMAP대로 모두 실행한다.
- 설계 우선순위는 `BLUEPRINT §20 > §16~§19`다. §1 D1~D18, §12, §14.11~§14.12, §15는 그대로 유지한다.

## 현재 상태 (2026-09-05, M6 리뷰·커밋 이후)

M0~M6 완료·리뷰 통과·커밋 완료. 정확한 M6 HEAD SHA, 테스트 수, 조명 solver·정착·프레임 수치는 `docs/LOG.md` 맨 위 M6 리뷰 항목을 source of truth로 사용한다.

다음 작업은 M7~M10 무중단 구현이다.

- M7: M5 Air High HDR/PBR/GTAO/TAAU 렌더 기반.
- M8: 물·볼류메트릭 포그·구름·LOD.
- M9: 4-level clipmap + WGSL compute DDA voxel GI.
- M10: 122개 건축 아이템, 1/16 형상, `I` 인벤토리, 핫바·아이콘, 1인칭 손, 설정·사운드·셰이더팩·`.app`.

M10 최종 검증 전에는 리뷰를 요청하지 않는다. 최종 보고 문구는 `M7~M10 완료, 최종 리뷰 요청`이다.
