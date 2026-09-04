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
