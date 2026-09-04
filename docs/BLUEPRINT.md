# voxelforge 설계도 (BLUEPRINT)

확정일 2026-09-04. 소유 Claude. 변경은 `docs/LOG.md`에 제안 → 여기 반영.

## 0. 목표와 범위

- **오늘(M3) 목표**: 창을 열면 지형이 있고, 날아다니며 조준한 블록을 부수고(LMB) 놓을 수 있다(RMB). 블록 종류는 1~9로 고른다. 이걸 「건축 가능」으로 정의한다.
- **최종 목표**: BSL/Complementary 쉐이더팩 수준의 룩(그림자·물·하늘·블룸·볼류메트릭) + 저장·물리·조명. 티어별 순서는 `ROADMAP.md`.
- **하지 않는 것**: 멀티플레이, 모드 API, 레드스톤류 시뮬레이션, 인벤토리/서바이벌 시스템.

검증된 환경: macOS 26.6.2, Apple M5(10코어), Rust 1.95.0, wgpu 30.0.1, winit 0.30.13, 서피스 `Bgra8UnormSrgb`, 창 1280×720 논리 = 2560×1440 물리(scale 2).

## 1. 확정 결정 (ADR 없이 변경 금지)

| # | 결정 | 이유 |
|---|---|---|
| D1 | Rust + wgpu(Metal 백엔드) + winit. 엔진·ECS 없음 | 에디터 없이 전부 텍스트 → 두 에이전트가 `cargo build/test`와 PNG 스냅샷으로 검증 가능. 컴파일 시간·제어권 확보 |
| D2 | 청크 **32³** 정육면체, 수직 8청크(블록 y 0..256) | 드로우 수 적음. 로컬 좌표 6비트 패킹. 패딩 메셔로 재메싱 비용 감당 가능 |
| D3 | 블록 ID `u16`, 청크는 평탄 배열 `Box<[u16; 32768]>` | 단순. 팔레트 압축은 메모리가 문제될 때(M5+) |
| D4 | 좌표계 오른손, **Y-up**. `glam::Mat4::perspective_rh`(깊이 0..1), `look_to_rh` | wgpu/Metal 깊이 범위와 일치. glam 0.33.1부터 이 둘은 deprecated(대체 `glam::camera::rh::proj::directx::perspective`, `glam::camera::rh::look_to`). 동작은 같으므로 M4까지 `#[allow(deprecated)]` 허용, M4 물리 작업 때 이관 |
| D5 | 정점 = `2 × u32` 패킹. UV는 정점에 없고 셰이더에서 로컬 좌표로 계산 | 정점 8바이트. greedy 병합 시 Repeat 샘플러로 자연 타일링 |
| D6 | 텍스처는 **`texture_2d_array`** 16×16, 아틀라스 금지 | 경계 번짐 없음, greedy 호환 |
| D7 | 메싱은 **패딩 34³ 스냅샷** 입력(`PaddedChunk`)으로 한다. 메셔는 World를 직접 보지 않는다 | 경계 면 컬링이 단순해지고 M4에서 스레드로 옮길 때 락 없이 그대로 감 |
| D8 | M1~M3은 culled 메싱 단일 스레드 + 프레임당 예산. greedy·워커 스레드는 M4 | 정확성 먼저. 정점 포맷은 이미 greedy 호환 |
| D9 | M3은 비행 카메라. 걷기/중력/점프는 M4 | 건축에 중력이 필요 없음 |
| D10 | 조명은 M3에서 면 방향 음영 + 정점 AO. 플러드필 0~15는 M6, 디퍼드/그림자는 M7 | 마크 룩의 절반은 AO. 나머지는 파이프라인이 생긴 뒤 |
| D11 | 물은 M3에서 불투명 렌더. 투명 패스는 M5 | 오늘 범위 축소 |
| D12 | 청크마다 개별 VB/IB. 아레나 할당자는 히치가 관측될 때(M5+) | 단순함 우선 |
| D13 | 깊이 `Depth32Float`, 표준 Z, near 0.05 / far 1000 | 32비트 float면 역Z 불필요 |
| D14 | 렌더는 물리 해상도 그대로(Retina 2x). 렌더 스케일 옵션은 오프스크린 HDR 타깃이 생기는 M7에 | 지금은 단순, 나중엔 필수 |
| D15 | 셰이더 **핫리로드**는 M1부터. 파일 mtime 폴링, 컴파일 실패 시 이전 파이프라인 유지 | 반복 속도 10배. 유저 쉐이더팩 구조로 확장 가능 |
| D16 | **`snapshot` 바이너리**(창 없는 오프스크린 렌더 → PNG)는 M1부터 | 에이전트가 화면을 볼 수 있는 유일한 방법. 렌더러를 창에서 분리하게 강제 |
| D17 | 텍스처는 `assets/textures/blocks/<name>.png`가 있으면 로드, 없으면 **절차적 생성**(결정적) | 오늘 에셋 의존 제거 |
| D18 | 에셋 경로 = `VF_ASSETS` 환경변수 → 없으면 `CARGO_MANIFEST_DIR/assets` | 개발 중 어디서 실행해도 동작 |

## 2. 디렉터리와 모듈 지도

```
voxelforge/
├── AGENTS.md
├── Cargo.toml                 # wgpu 30, winit 0.30, glam, bytemuck, pollster, image, fastnoise-lite, anyhow, log, env_logger
├── assets/
│   ├── shaders/               # chunk.wgsl, outline.wgsl  (핫리로드 대상)
│   └── textures/blocks/       # 선택. 16×16 PNG. 없으면 절차적
├── docs/                      # BLUEPRINT, ROADMAP, LOG, KICKOFF
├── src/
│   ├── lib.rs                 # pub mod world, mesh, render, player, assets
│   ├── main.rs                # 창·이벤트 루프·App (M1에서 M0 스모크를 대체)
│   ├── bin/snapshot.rs        # 오프스크린 렌더 → PNG
│   ├── assets.rs              # assets_dir(), 파일 읽기
│   ├── player/
│   │   ├── camera.rs          # Camera { pos, yaw, pitch, fov }  view/proj
│   │   └── controller.rs      # 입력 상태 → 카메라 이동(비행), M4에서 물리
│   ├── world/
│   │   ├── block.rs           # BlockId 상수, BlockDef, registry, TEXTURES
│   │   ├── coords.rs          # CHUNK_SIZE=32, 청크/로컬 변환, 면 인덱스·법선 테이블
│   │   ├── chunk.rs           # Chunk 저장·get/set, PaddedChunk
│   │   ├── gen.rs             # WorldGen (노이즈 → Chunk), 순수 함수
│   │   ├── world.rs           # World: HashMap<IVec3, Chunk>, 스트리밍, dirty 관리
│   │   └── raycast.rs         # Amanatides–Woo DDA
│   ├── mesh/
│   │   ├── vertex.rs          # ChunkVertex 패킹/언패킹
│   │   └── mesher.rs          # PaddedChunk → ChunkMesh (culled + AO). M4: greedy
│   └── render/
│       ├── gpu.rs             # Gpu: instance/adapter/device/queue/(surface)
│       ├── globals.rs         # Globals 유니폼(view_proj, cam_pos, sun_dir, time_res)
│       ├── textures.rs        # texture_2d_array 빌드(PNG 또는 절차적)
│       ├── chunk_pipeline.rs  # 청크 파이프라인 + 청크 유니폼(dynamic offset) + 드로우
│       ├── outline.rs         # 조준 블록 와이어프레임
│       ├── shader_watch.rs    # mtime 폴링 → 파이프라인 재생성(에러 스코프)
│       ├── renderer.rs        # Renderer: 위 전부를 묶어 한 프레임을 그림. 창/서피스 몰라야 함
│       └── offscreen.rs       # 텍스처 렌더 → 버퍼 복사 → RGBA 바이트 (snapshot용)
└── tests/                     # 통합 테스트(선택). 단위 테스트는 각 모듈 안 #[cfg(test)]
```

원칙: `render::Renderer`는 `wgpu::Device/Queue`와 "색 뷰 + 깊이 뷰"만 받는다. 창(`Surface`)은 `main.rs`가, 오프스크린 텍스처는 `snapshot.rs`가 만든다. 같은 Renderer를 둘이 공유한다.

M3에서 실제로 추가된 것: `src/window_gpu.rs`(창 서피스 보조, main.rs 500줄 규칙), `src/player/interaction.rs`(AABB 겹침 판정). M4·M5에서 추가되는 모듈은 **§14.1**.

## 3. 좌표·청크 규약

- 블록 위치 `IVec3 (x,y,z)`. 블록은 `[x,x+1)×[y,y+1)×[z,z+1)`를 차지한다.
- `CHUNK_SIZE = 32`, `CHUNK_BITS = 5`.
- 청크 좌표 `cp = bp >> 5` (i32 산술 시프트 → 음수도 floor). 로컬 `lp = bp & 31`.
- 청크 내부 인덱스 `idx = x + 32*(z + 32*y)` (x가 가장 빠름).
- 수직 범위: 청크 y ∈ [0, 8). 블록 y ≥ 256은 AIR, **y < 0은 STONE 취급**(월드 바닥에 면이 생기지 않게).
- 면 인덱스와 법선 (이 순서를 어디서나 쓴다):

| face | normal |
|---|---|
| 0 | +X |
| 1 | −X |
| 2 | +Y |
| 3 | −Y |
| 4 | +Z |
| 5 | −Z |

- 쿼드 코너 순서 (CCW, 바깥에서 볼 때. `front_face: Ccw`, `cull_mode: Back`). 블록 (x,y,z)에 대해:

| face | c0 | c1 | c2 | c3 |
|---|---|---|---|---|
| +X | (x+1,y,z) | (x+1,y+1,z) | (x+1,y+1,z+1) | (x+1,y,z+1) |
| −X | (x,y,z) | (x,y,z+1) | (x,y+1,z+1) | (x,y+1,z) |
| +Y | (x,y+1,z) | (x,y+1,z+1) | (x+1,y+1,z+1) | (x+1,y+1,z) |
| −Y | (x,y,z) | (x+1,y,z) | (x+1,y,z+1) | (x,y,z+1) |
| +Z | (x,y,z+1) | (x+1,y,z+1) | (x+1,y+1,z+1) | (x,y+1,z+1) |
| −Z | (x,y,z) | (x,y+1,z) | (x+1,y+1,z) | (x+1,y,z) |

인덱스: `(0,1,2), (0,2,3)`. 이 표는 검증된 외적(u×v = n) 결과다. 시행착오로 다시 찾지 않는다.

- 카메라 전방 벡터: `forward = (sin(yaw)·cos(pitch), sin(pitch), −cos(yaw)·cos(pitch))`. yaw=0 → −Z. 마우스 dx>0 → yaw 증가(오른쪽 회전). pitch는 ±89°로 클램프.

## 4. 데이터 계약 (Rust 시그니처)

구현은 자유. **이름·타입·의미**는 지킨다. 테스트가 이 계약을 부른다.

```rust
// world/block.rs
pub type BlockId = u16;
pub const AIR: BlockId = 0;   pub const STONE: BlockId = 1;  pub const DIRT: BlockId = 2;
pub const GRASS: BlockId = 3; pub const SAND: BlockId = 4;   pub const WATER: BlockId = 5;
pub const LOG: BlockId = 6;   pub const LEAVES: BlockId = 7; pub const PLANKS: BlockId = 8;
pub const GLASS: BlockId = 9; pub const BRICK: BlockId = 10; pub const COBBLE: BlockId = 11;
pub struct BlockDef { pub name: &'static str, pub solid: bool, pub opaque: bool, pub textures: [u16; 6] /* face 순서 */ }
pub fn def(id: BlockId) -> &'static BlockDef;      // 범위 밖이면 AIR의 def
pub const TEXTURES: &[&str];                        // 레이어 순서 = 인덱스. 예: "stone","dirt","grass_top","grass_side",...
pub const HOTBAR: [BlockId; 9];                      // 1~9 키

// world/coords.rs
pub const CHUNK_SIZE: i32 = 32;  pub const CHUNK_BITS: i32 = 5;  pub const WORLD_CHUNKS_Y: i32 = 8;
pub fn chunk_of(bp: IVec3) -> IVec3;                 // floor
pub fn local_of(bp: IVec3) -> UVec3;                 // 0..32
pub fn chunk_index(l: UVec3) -> usize;               // x + 32*(z + 32*y)
pub const FACE_NORMALS: [IVec3; 6];
pub fn face_corners(face: usize, bp: IVec3) -> [IVec3; 4];   // §3 표

// world/chunk.rs
pub struct Chunk { /* blocks: Box<[BlockId; 32768]>, non_air: u32 */ }
impl Chunk {
    pub fn new_air() -> Self;
    pub fn get(&self, l: UVec3) -> BlockId;
    pub fn set(&mut self, l: UVec3, id: BlockId);    // non_air 유지
    pub fn is_empty(&self) -> bool;                  // non_air == 0
}
pub const PADDED: usize = 34;
pub struct PaddedChunk { /* blocks: Box<[BlockId; 34*34*34]> */ }
impl PaddedChunk {
    pub fn get(&self, x: i32, y: i32, z: i32) -> BlockId;   // -1..=32 허용
}

// world/gen.rs
pub struct WorldGen { /* seed, noise */ }
impl WorldGen {
    pub fn new(seed: u64) -> Self;
    pub fn height_at(&self, x: i32, z: i32) -> i32;
    pub fn generate(&self, cp: IVec3) -> Chunk;      // 순수 함수. 같은 (seed, cp) → 같은 결과
}
pub const SEA_LEVEL: i32 = 62;

// world/world.rs
pub struct World { /* chunks: HashMap<IVec3, Chunk>, gen, dirty: HashSet<IVec3> */ }
impl World {
    pub fn new(seed: u64) -> Self;
    pub fn get_block(&self, bp: IVec3) -> BlockId;   // 미로딩/범위 밖: y<0 → STONE, 그 외 AIR
    pub fn set_block(&mut self, bp: IVec3, id: BlockId) -> bool;  // 로딩된 청크만. 해당 청크 dirty, 경계면이면 이웃도 dirty
    pub fn chunk(&self, cp: IVec3) -> Option<&Chunk>;
    pub fn padded(&self, cp: IVec3) -> PaddedChunk;  // 이웃 미로딩 → AIR(y<0면 STONE)
    pub fn ensure_loaded(&mut self, cp: IVec3) -> bool;         // 생성했으면 true. 생성 시 6이웃 dirty
    pub fn unload_outside(&mut self, center: IVec3, radius: i32);
    pub fn take_dirty(&mut self) -> Vec<IVec3>;      // 가까운 순 정렬은 호출자
}

// world/raycast.rs
pub struct RayHit { pub block: IVec3, pub normal: IVec3, pub distance: f32 }
pub fn raycast(world: &World, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<RayHit>;
// solid 블록만 맞춘다(WATER 통과). 시작 블록이 solid면 None.

// mesh/vertex.rs
#[repr(C)] #[derive(Clone, Copy, Pod, Zeroable)]
pub struct ChunkVertex { pub a: u32, pub b: u32 }
pub fn pack(x: u32, y: u32, z: u32, face: u32, ao: u32, tex: u32, light: u32, sky: u32) -> ChunkVertex;
pub fn unpack(v: ChunkVertex) -> (u32, u32, u32, u32, u32, u32, u32, u32);  // 테스트용
// a: x[0..6) y[6..12) z[12..18) face[18..21) ao[21..23) reserved[23..32)
// b: tex[0..16) light[16..20) sky[20..24) reserved[24..32)      // M3까지 light=0, sky=15

// mesh/mesher.rs
pub struct ChunkMesh { pub vertices: Vec<ChunkVertex>, pub indices: Vec<u32> }
pub fn mesh_chunk(p: &PaddedChunk) -> ChunkMesh;    // culled: 이웃이 opaque가 아니면 면 생성. 자기 자신이 AIR면 없음
// AO(0fps 방식): 가리는 블록은 **면 앞 층**에 있다. front = bp + FACE_NORMALS[face].
//   side1 = opaque(front + s1), side2 = opaque(front + s2), corner = opaque(front + s1 + s2)   (s1, s2 = 코너 쪽 ±접선축)
//   ao = if side1 && side2 { 0 } else { 3 - (side1 + side2 + corner) }
//   bp + s (블록 자신의 층)는 같은 평면이라 가리지 않는다. 이걸 섞으면 평지 전체가 ao=0으로 어두워진다(M3 리뷰에서 실측 확인).

// player/camera.rs
pub struct Camera { pub pos: Vec3, pub yaw: f32, pub pitch: f32, pub fov_y: f32, pub near: f32, pub far: f32 }
impl Camera { pub fn forward(&self) -> Vec3; pub fn view(&self) -> Mat4; pub fn proj(&self, aspect: f32) -> Mat4; }
pub const EYE_HEIGHT: f32 = 1.62;  pub const PLAYER_HALF_W: f32 = 0.3;  pub const PLAYER_H: f32 = 1.8;
```

## 5. 렌더 계약

### 바인드 그룹

| group | binding | 내용 | 갱신 |
|---|---|---|---|
| 0 | 0 | `Globals` uniform: `view_proj: mat4x4<f32>`, `cam_pos: vec4<f32>`, `sun_dir: vec4<f32>`, `time_res: vec4<f32>` (time, width, height, 0) | 프레임마다 `write_buffer` |
| 1 | 0 | `texture_2d_array<f32>` 블록 텍스처 (`Rgba8UnormSrgb`, 16×16 × N) | 시작 시 1회 |
| 1 | 1 | `sampler` Nearest/Nearest, Repeat | 1회 |
| 2 | 0 | `ChunkUniform { origin: vec4<i32> }` **dynamic offset** — 큰 uniform 버퍼 하나에 청크별 256바이트 슬롯 | 청크 업로드 시 |

정점 버퍼: `ChunkVertex` 8바이트, `VertexFormat::Uint32x2` → `@location(0) packed: vec2<u32>`. 인덱스 `Uint32`.
파이프라인: `topology TriangleList`, `front_face Ccw`, `cull_mode Some(Back)`, depth `Depth32Float` / `compare Less` / write on. 컬러 타깃 = 서피스 포맷(스냅샷은 `Rgba8UnormSrgb`).

### chunk.wgsl 계약 (디코드·UV·음영)

```wgsl
struct Globals { view_proj: mat4x4<f32>, cam_pos: vec4<f32>, sun_dir: vec4<f32>, time_res: vec4<f32> }
struct ChunkUniform { origin: vec4<i32> }
@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var block_tex: texture_2d_array<f32>;
@group(1) @binding(1) var block_samp: sampler;
@group(2) @binding(0) var<uniform> chunk: ChunkUniform;

// decode
let x = packed.x & 63u;  let y = (packed.x >> 6u) & 63u;  let z = (packed.x >> 12u) & 63u;
let face = (packed.x >> 18u) & 7u;  let ao = (packed.x >> 21u) & 3u;
let tex = packed.y & 65535u;  let light = (packed.y >> 16u) & 15u;  let sky = (packed.y >> 20u) & 15u;
let local = vec3<f32>(f32(x), f32(y), f32(z));
let world_pos = vec3<f32>(chunk.origin.xyz) + local;

// uv: 로컬 좌표 두 축. 세로는 -y로 뒤집어 텍스처 위가 위. Repeat 샘플러라 음수·1 초과 모두 타일링됨(greedy 대비)
//  face 0,1 (±X): uv = (local.z, -local.y)    face 2,3 (±Y): uv = (local.x, local.z)    face 4,5 (±Z): uv = (local.x, -local.y)

// shade: face별 상수  +Y 1.0 / −Y 0.55 / ±X 0.80 / ±Z 0.65
// ao_f = 0.35 + 0.65 * f32(ao) / 3.0
// color = textureSample(block_tex, block_samp, uv, i32(tex)).rgb * shade * ao_f
```

서피스가 sRGB 포맷이므로 셰이더는 **선형** 값을 쓰고 인코딩은 하드웨어가 한다. 텍스처도 sRGB 포맷으로 만들어 샘플 시 선형으로 풀린다.

### outline.wgsl

조준 블록의 12 모서리. `LineList`, 정점 `vec3<f32>` 월드 좌표(블록 AABB를 1.002배). group(0)만 사용. 색 `(0.05,0.05,0.05)`. 깊이 비교 `LessEqual`, 쓰기 off.

### 절차적 텍스처 (D17)

16×16 RGBA. 결정적 해시 노이즈로 밝기 ±8%. 기본색: stone `(125,125,125)`, dirt `(134,96,67)`, grass_top `(95,159,53)`, grass_side = dirt 위 4줄 grass, sand `(219,211,160)`, water `(63,118,228)`, log_side `(109,85,50)` 세로 줄, log_top 동심 사각, leaves `(60,120,40)`, planks `(157,127,78)` 가로 줄 4개, glass `(200,235,240)` + 테두리, brick `(150,84,66)` + 줄눈, cobble 회색 조각. PNG가 있으면 이름으로 덮어쓴다.

### 프레임 루프 (main.rs)

`ControlFlow::Poll`. `RedrawRequested`마다:

1. `dt` 계산(clamp 0.1s)
2. 입력 → 카메라(비행): 속도 12 blk/s, 스프린트(LCtrl) ×3, Space 상승, LShift 하강. WASD는 yaw 기준 수평
3. 스트리밍: 플레이어 청크 기준 수평 반경 `R=6`, 수직 전체(0..8). 가까운 순으로 `ensure_loaded`, `unload_outside(R+2)`
4. `take_dirty()` → 가까운 순(편집 청크 최우선) 정렬 → `padded → mesh_chunk → 업로드`. 결과가 빈 메시면 GPU 리소스 제거

   **예산(2026-09-04 리뷰에서 개수→시간으로 변경)**: 3·4를 합쳐 **프레임당 ≤ 10ms**(`Instant`로 재고, 청크 하나 끝날 때마다 확인). 개수 상한 없음. 큐가 비면 0ms. `chunk.is_empty()`인 청크는 메싱하지 않는다(절반이 공기). 첫 프레임 전에 스폰 반경 1(3×3×8)은 동기 로딩·메싱한다. 원하는 집합이 전부 로딩·메싱된 순간 `stream: settled in {:.2}s ({} chunks)`를 1회 로그로 남긴다 — §7의 초기 로딩 기준은 이 로그다.
5. 레이캐스트(max 6.0) → outline 위치
6. `Renderer::render(...)`
7. 1초마다 창 제목 갱신: `voxelforge | 60 fps | max 17.2ms | pos 12.3 71.0 -4.5 | chunks 812 drawn 214 | meshq 0`

입력 맵: 마우스 이동 = 시선(감도 0.002 rad/px, `DeviceEvent::MouseMotion`), 클릭으로 커서 잠금(`CursorGrabMode::Locked`, 실패 시 `Confined`) + 숨김, ESC = 잠금 해제, 해제 상태에서 ESC = 종료. LMB 부수기, RMB 놓기(`hit.block + hit.normal`, AIR 또는 WATER 위치만, 플레이어 AABB와 겹치면 거절). 1~9 = `HOTBAR`, 휠 = 순환. R = 셰이더 강제 리로드. F3 = 콘솔 디버그 로그 토글.

### 셰이더 핫리로드 (D15)

`ShaderWatcher`가 0.5초마다 `assets/shaders/*.wgsl` mtime 확인. 변경 시:

```rust
device.push_error_scope(wgpu::ErrorFilter::Validation);
let module = device.create_shader_module(desc);
let pipeline = device.create_render_pipeline(&desc_using(&module));
if let Some(err) = pollster::block_on(device.pop_error_scope()) { log::error!("{err}"); /* 이전 파이프라인 유지 */ }
else { self.pipeline = pipeline; log::info!("shader reloaded"); }
```

기본 uncaptured error 핸들러는 패닉이므로 반드시 에러 스코프로 감싼다.

### snapshot 바이너리 (D16)

```
cargo run --release --bin snapshot -- [--seed 1] [--pos 0,80,0] [--yaw 0.6] [--pitch -0.3] [--radius 4] [--size 1280x720] [--out /tmp/vf.png]
```

창 없이: `Instance::default()` → `request_adapter(compatible_surface: None)` → 동일 `Renderer` → `Rgba8UnormSrgb` 텍스처 + 깊이 → `copy_texture_to_buffer`(bytes_per_row 256 정렬) → `map_async` + `device.poll` → `image`로 PNG. 스트리밍 예산 없이 반경 내 전 청크 생성·메싱 후 1프레임 렌더. `--pos` 생략 시 `(0, height_at(0,0)+3, 0)`.

## 6. 월드 생성 (gen.rs)

- `fastnoise_lite::FastNoiseLite`, `NoiseType::OpenSimplex2`, `FractalType::FBm`, octaves 4, lacunarity 2.0, gain 0.5, frequency 0.008, seed 하위 32비트.
- `height = 64 + round(24 * noise2d(x, z))` (noise ∈ [-1,1]) → 40..88.
- 층: `y < 1` STONE(바닥) / `y <= h-4` STONE / `h-4 < y < h` DIRT / `y == h` → `h >= SEA_LEVEL+1 ? GRASS : SAND` / `h < y <= SEA_LEVEL` WATER / 그 외 AIR.
- 나무·동굴은 M4. `generate`는 World를 보지 않는 순수 함수(스레드 이관 대비).

## 7. 성능 예산 (M3 기준, Apple M5, 2560×1440)

- 60 fps 유지, 프레임 최대 < 33ms(히치 없음). 초기 로딩(R=6, release) < 3초 = `stream: settled` 로그 기준. 개수 예산(4/프레임)으로는 하한이 5.6초라 §5의 시간 예산으로 바꿨다. 단일 스레드 10ms 예산으로도 미달이면 M4 워커 스레드에서 확정한다.
- 청크 32³ culled 메싱 release < 1ms/청크. 메모리: 로딩 청크 ≤ ~1400개 × 64KB ≈ 90MB + GPU 메시.
- 플레이는 `--release`. dev 프로필도 의존성은 `opt-level 3`(Cargo.toml).

## 8. 테스트 계약 (M1/M3에서 반드시 존재하는 이름)

`coords_floor_div_negative`, `chunk_index_roundtrip`, `chunk_set_get_and_non_air`, `vertex_pack_roundtrip`, `mesher_lone_block_has_6_faces`(24 vert/36 idx, AO 전부 3), `mesher_ao_reads_layer_in_front_of_face`(리뷰에서 추가), `mesher_enclosed_block_has_0_faces`, `mesher_two_adjacent_blocks_10_faces`, `mesher_border_face_culled_by_padding`, `gen_is_deterministic_for_seed`, `gen_layers_match_height`, `raycast_hits_block_in_front`, `raycast_face_normal_is_toward_origin`, `raycast_misses_when_only_air`, `raycast_passes_through_water`, `set_block_marks_neighbor_dirty_on_border`, `place_rejected_inside_player_aabb`.

## 9. 에셋 경로 (D18)

`assets::dir()` = `VF_ASSETS` → 없으면 `concat!(env!("CARGO_MANIFEST_DIR"), "/assets")`. 셰이더는 `dir()/shaders/<name>.wgsl`, 텍스처는 `dir()/textures/blocks/<name>.png`.

## 10. 에러·로그 정책

- 초기화: `anyhow::Result`, 실패 시 메시지 후 종료.
- 렌더 루프: `CurrentSurfaceTexture` 6개 변형을 모두 `match`. `Outdated | Lost` → 재구성, `Timeout | Occluded | Validation` → 프레임 스킵.
- `env_logger`, 기본 필터 `info,wgpu_core=warn,wgpu_hal=warn,naga=warn`.

## 11. wgpu 30 API — 소스로 확인된 차이 (기억과 다르면 이쪽이 맞다)

| 항목 | wgpu 30 실제 |
|---|---|
| 인스턴스 | `wgpu::Instance::default()` (= `Instance::new(InstanceDescriptor::new_without_display_handle())`). `Instance::new`는 **값** 전달. `from_env_or_default`는 InstanceDescriptor에 없음 |
| 어댑터 | `request_adapter(&RequestAdapterOptions{ power_preference, force_fallback_adapter, compatible_surface, apply_limit_buckets })` → `Future<Result<Adapter, RequestAdapterError>>` |
| 디바이스 | `request_device(&DeviceDescriptor{ label, required_features, required_limits, experimental_features, memory_hints, trace })` → `Result<(Device, Queue), _>`. `DeviceDescriptor`는 `Default` 있음 |
| 프레임 획득 | `surface.get_current_texture() -> CurrentSurfaceTexture` **열거형**: `Success(t) | Suboptimal(t) | Timeout | Occluded | Outdated | Lost | Validation`. `Result`가 아님 |
| 표시 | `queue.present(surface_texture)`. `SurfaceTexture::present`는 **없음** |
| 렌더 패스 | `RenderPassDescriptor{ label, color_attachments, depth_stencil_attachment, timestamp_writes, occlusion_query_set, multiview_mask }` — `Default` 없음, 전부 명시 |
| 컬러 어태치 | `RenderPassColorAttachment{ view, depth_slice: None, resolve_target, ops }` |
| 파이프라인 | `RenderPipelineDescriptor{ label, layout, vertex, primitive, depth_stencil, multisample, fragment, multiview_mask, cache }` |
| VertexState | `buffers: &[Option<VertexBufferLayout>]` (Option 래핑) |
| DepthStencilState | `depth_write_enabled: Option<bool>`, `depth_compare: Option<CompareFunction>` |
| SurfaceConfiguration | `color_space` 필드 추가. `get_default_config`가 채워 줌 |
| 서피스 수명 | `Surface<'static>`는 `instance.create_surface(Arc<Window>)`로 |
| 파이프라인 레이아웃 | `PipelineLayoutDescriptor{ label, bind_group_layouts: &[Option<&BindGroupLayout>], immediate_size: 0 }` (push_constant_ranges 대신 `immediate_size`) |
| 에러 스코프 | `let scope = device.push_error_scope(ErrorFilter::Validation); … ; pollster::block_on(scope.pop())` — 스코프 객체를 반환하고 그 객체의 `pop()`을 기다린다 |
| 텍스처 복사 타입 | `TexelCopyTextureInfo`, `TexelCopyBufferLayout` (구 `ImageCopyTexture`/`ImageDataLayout`). 샘플러 `mipmap_filter: MipmapFilterMode` |
| 소스 위치 | `~/.cargo/registry/src/index.crates.io-*/wgpu-30.0.1/src/api/`, 타입은 `wgpu-types-30.0.1/src/` |

winit 0.30: `ApplicationHandler` 트레이트(`resumed`, `window_event`, `device_event`, `about_to_wait`), `event_loop.create_window(Window::default_attributes())`, `EventLoop::new()?.run_app(&mut app)?`. 실제 동작 예는 M0 `src/main.rs`(git 첫 커밋)에 있다.

## 12. 분기점 선결정 (묻지 말고 이렇게)

- 청크 16³ vs 32³ → **32³** (D2)
- 텍스처 아틀라스 vs 배열 → **배열** (D6)
- 정점에 UV 저장 vs 셰이더 계산 → **셰이더 계산** (D5)
- 메셔가 World 참조 vs 패딩 스냅샷 → **패딩 34³** (D7)
- 청크 원점 전달: push constant vs dynamic-offset uniform vs 인스턴스 버퍼 → **dynamic-offset uniform** (§5)
- 스레드 도입 시점 → **M4**, rayon 또는 std::thread + crossbeam-channel 중 GPT 선택, LOG에 기록
- 미로딩 이웃 청크 경계면 → **AIR 취급(면 그림)**, 이웃 로딩 시 dirty로 재메싱
- 물 조준 → 통과(부수기·놓기 대상 아님). 물 위치에 놓기 → 허용(물을 대체)
- 시작 위치가 solid 안 → 레이캐스트 None
- 텍스트 오버레이(F3) → 하지 않음(글리프 아틀라스 공수). 창 제목 + 콘솔 로그로 대체
- 저장 포맷(M5) → 수정된 청크만 `saves/<name>/c_<x>_<y>_<z>.bin`(lz4) + `world.json{seed}`. 리전 파일은 필요해질 때
- 엔진 크레이트(bevy) → 사용 안 함
- Retina 렌더 스케일 → M7까지 물리 해상도 그대로
- 스트리밍 예산 → **시간 예산**, 개수 예산 아님 (§5). 빈 청크는 메싱 생략. M4부터는 워커 스레드 구조(§14.2)가 이를 대체
- 청크 유니폼 아레나 슬롯 → **16384 고정**(4MB, R=16까지 충분). 성장 로직 없음
- M4·M5의 추가 선결정은 **§14.11**

## 13. 이후 티어 요약 (상세는 ROADMAP)

M4 greedy·스레드·물리 → M5 저장·투명·아레나 → M6 플러드필 조명·낮밤·바람 → M7 오프스크린 HDR·CSM 그림자·SSAO·대기 하늘·ACES/블룸·렌더 스케일 → M8 물(파도·SSR·굴절)·볼류메트릭·구름·LOD → M9 복셀 GI(3D 텍스처 clipmap + 컴퓨트 DDA + 시간적 누적).

---

## 14. M4·M5 상세 설계 (2026-09-04 13:50 확정, Claude)

M3 손 플레이 통과(13:37). M3.1 잔여 항목은 M4의 0단계다. 아래 계약은 §4·§5를 **확장**한다(기존 이름은 유지).

### 14.1 새 모듈

```
src/stream.rs                 # Streamer: 원하는 집합, 잡 발행(rayon), 결과 회수(mpsc), 시간 예산, settled 로그   (M4)
src/mesh/greedy.rs            # mesh_chunk_greedy: 키(tex, ao[4]) 동일 셀 병합. 면/AO 헬퍼는 mesher.rs와 공유         (M4)
src/player/physics.rs         # Body, MoveInput, step(), sweep_axis()                                            (M4)
src/render/frustum.rs         # Frustum::from_view_proj, intersects_aabb                                          (M4)
src/world/trees.rs            # tree_at, tree_blocks (순수 함수, gen.rs가 호출)                                    (M4)
src/world/save.rs             # SaveDir: world.json, 청크 파일 lz4 읽기/쓰기                                       (M5)
src/render/translucent.rs     # 투명 파이프라인(블렌드, 깊이 쓰기 off, cull None)                                  (M5)
src/render/mesh_arena.rs      # 큰 VB/IB + 프리리스트 (M5, 조건부 — 14.9)                                          (M5)
```

`main.rs`는 이미 497줄이다. M4에서 스트리밍은 `stream.rs`, 이동은 `physics.rs`로 빠져야 500줄 규칙을 지킨다.

### 14.2 스트리밍 — 워커 스레드 (M4)

- 풀: **rayon** `ThreadPoolBuilder::num_threads(w)`, `w = available_parallelism().saturating_sub(2).clamp(2, 8)`. 결과는 `std::sync::mpsc::channel`. (분기 선결정: crossbeam·수제 풀 아님)
- 잡 두 종류. `Gen { cp, gen: Arc<WorldGen> } → Chunk`, `Mesh { cp, padded: Box<PaddedChunk>, version: u64 } → ChunkMesh`. `WorldGen: Send + Sync`(FastNoiseLite는 숫자 필드뿐)를 `static_assertions` 없이 `fn assert_send_sync<T: Send + Sync>()` 테스트로 고정.
- `padded()`는 메인 스레드(월드 읽기), 메싱·생성은 워커. 메인은 결과를 받아 업로드만.
- 프레임 순서: ① 결과 채널 전부 회수(gen → `World::insert_generated`, mesh → `version` 일치할 때만 업로드, 불일치·언로드면 폐기) ② 원하는 집합과 비교해 새 잡 발행(가까운 순, 종류별 in-flight ≤ `2w`) ③ 언로드 ④ 편집으로 dirty된 청크(urgent)는 **메인 스레드에서 동기 메싱**(같은 프레임 반영, 프레임당 ≤ 3개, 넘치면 다음 프레임).
- 메인 스레드 예산: `padded()` 복사 + 업로드 합산 ≤ 6ms/프레임(`Instant`). 개수 상한 없음.
- 부트스트랩: 첫 프레임 전 스폰 반경 1(3×3×8)은 동기 생성·메싱(플레이어가 떨어지지 않게).
- `stream: settled in {:.2}s ({} chunks, {} drawn)`: 원하는 집합 전부 로딩 + in-flight 0 + 큐 0인 첫 순간 1회. **목표 R=12에서 < 3.0s**(예상 1~1.5s).
- 반경: `STREAM_RADIUS` 기본 **10**, 환경변수 `VF_RADIUS`(4..=16). 성능 검증은 12.
- `World` 추가 계약:

```rust
impl World {
    pub fn is_loaded(&self, cp: IVec3) -> bool;
    pub fn insert_generated(&mut self, cp: IVec3, chunk: Chunk);   // ensure_loaded의 삽입 절반. dirty + 6이웃 dirty
    pub fn chunk_version(&self, cp: IVec3) -> Option<u64>;         // 삽입 시 1, set_block마다 +1
    pub fn generator(&self) -> &Arc<WorldGen>;
}
```

### 14.3 Greedy 메싱 (M4)

- 면 6개 × 법선축 층 32개 × 32×32 격자. 셀이 보이면(현재 culled 규칙) 키 `key = tex << 8 | ao0<<6 | ao1<<4 | ao2<<2 | ao3`. 같은 키의 인접 셀을 u 방향으로 늘리고, 그 행 전체가 같으면 v 방향으로 늘린다(Lysenko 방식). 비트 평면 최적화는 하지 않는다(필요해지면 M7 이후).
- 정점은 기존 `pack()`. 쿼드 4정점 위치는 병합 사각형의 코너(§3 코너 순서 유지). UV는 셰이더가 로컬 좌표로 계산하므로 변경 없음(D5).
- 대각 뒤집기: `ao0 + ao2 > ao1 + ao3`면 인덱스 `(1,2,3),(1,3,0)`. culled 메셔에도 같이 적용해 둘의 픽셀 결과가 같게 한다.
- `mesh_chunk`(culled)는 남긴다 — 테스트와 `snapshot --mesher culled` 비교 기준. 게임과 스냅샷 기본은 greedy.
- 계약:

```rust
// mesh/greedy.rs
pub fn mesh_chunk_greedy(p: &PaddedChunk) -> ChunkMesh;
// 테스트: greedy_quad_area_equals_culled_face_count (병합 사각형 넓이 합 == culled 면 수, 랜덤 시드 3개)
//        greedy_never_merges_different_keys, greedy_flat_slab_top_is_one_quad (32×32 평판 윗면 = 4정점)
```

### 14.4 물리 (M4)

- `Body { pos: Vec3 /* 발 중심 */, vel: Vec3, on_ground: bool, fly: bool, in_water: bool }`. 카메라 `pos = body.pos + (0, EYE_HEIGHT, 0)`. AABB = `pos ± (0.3, 0, 0.3)`, 높이 1.8 (§4 상수 재사용).
- `MoveInput { wish: Vec3 /* yaw 기준 수평, 정규화 */, jump: bool, sprint: bool, up: bool, down: bool }`. `Controller`는 이제 카메라를 직접 움직이지 않고 `MoveInput`을 만든다(시선 회전은 그대로 카메라에).
- 상수: 중력 −28, 종단 −78, 점프 초속 8.5(높이 ≈1.29), 걷기 4.3, 스프린트 5.6, 비행 12(스프린트 ×3). 수중: 중력 ×0.4, 수직 속도 클램프 ±4, Space는 +4 상승, 수평 ×0.6.
- `step(world, body, input, dt)`: `steps = ceil(dt / (1/120)).clamp(1, 8)`, `h = dt/steps`, 각 substep에서 **Y → X → Z** 순 축별 스윕. 축에서 막히면 그 축 속도 0, Y 하강 중 막힘이면 `on_ground = true`.
- `sweep_axis(world, min, max, delta, axis) -> (moved, hit)`: 이동 범위를 덮는 블록들 중 `def(id).solid`이고 다른 두 축이 겹치는(ε=1e-4) 블록으로 허용 거리를 줄인다. 계단 자동 오르기 없음.
- 플레이어 청크가 미로딩이면 물리를 멈춘다(떨어지지 않음). 스폰 열이 물이면 나선형으로 64블록까지 `h > SEA_LEVEL` 열을 찾는다.
- 키: `F` 비행 토글(켤 때 `vel.y = 0`), Space 점프/상승, LShift 하강(비행), LCtrl 스프린트.
- 테스트: `physics_falls_and_lands_on_ground`(2초 후 on_ground, `pos.y == 윗면` ±1e-3), `physics_jump_height_about_1_25`(최고점 1.1~1.4), `physics_wall_blocks_horizontal_motion`(벽면에서 정확히 멈춤, 침투 0), `physics_no_tunneling_at_low_fps`(dt 0.25, 속도 40에서도 착지), `physics_fly_ignores_gravity`.

### 14.5 월드젠 — 나무·동굴 (M4)

- **나무**(`world/trees.rs`, 순수): `tree_at(seed, x, z, h) -> Option<Tree>`: `hash01(seed, x, z, TREE_SALT) < 0.004 && h > SEA_LEVEL + 1`. `trunk_h ∈ 4..=6`(같은 해시), `base_y = h + 1`. 블록:
  - 줄기: `(x, base_y + i, z) LOG`, `i ∈ 0..trunk_h`
  - 잎: `dy ∈ {trunk_h-2, trunk_h-1}`: `dx,dz ∈ [-2,2]`에서 네 모서리(|dx|=|dz|=2) 제외 → 21셀. `dy = trunk_h`: 3×3. `dy = trunk_h+1`: 십자 5셀.
  - 규칙: LOG가 LEAVES를 이긴다. 둘 다 지형이 AIR인 곳에만 놓는다.
- `generate(cp)`는 `x ∈ [cx·32−2, cx·32+34)`, `z` 같은 범위의 열마다 `tree_at`을 보고, **이 청크 안에 떨어지는 블록만** 쓴다. 이웃 청크가 같은 나무를 같은 자리에서 계산하므로 경계에서 이어진다(순수 함수라 스레드 안전).
- **동굴**: 3D OpenSimplex2(프랙털 없음), 주파수 0.045, 시드 `seed+1`. `noise > 0.62 && 8 <= y <= h - 6`이면 AIR. 표면(h..h-5)은 절대 뚫지 않는다.
- 테스트: `trees_agree_across_chunk_borders`(시드 고정, 경계를 걸치는 나무를 찾아 두 청크의 블록이 `tree_blocks` 집합과 일치), `caves_do_not_break_surface`(모든 열에서 y=h..h-5가 non-air), 기존 `gen_layers_match_height` 유지.

### 14.6 프러스텀 컬링 (M4)

`Frustum::from_view_proj(Mat4)` → 6평면(Gribb-Hartmann). 청크 AABB `origin .. origin+32` 판정. 컬링된 청크는 draw 안 함. 제목 `drawn`은 컬링 후 수. 테스트 `frustum_culls_chunk_behind_camera`, `frustum_keeps_chunk_in_front`.

### 14.7 M3.1 잔여 (M4 0단계)

`snapshot --edits "set x,y,z,id; …"`, `snapshot --mesher culled|greedy`(기본 greedy), `chunk.wgsl` 더미 연산 제거, `gen.rs` `let _ = self.seed`·`Chunk` 내부 직접 쓰기 정리, `origin / CHUNK_SIZE → chunk_of`, `DEFAULT_SLOTS = 16384`(4MB, R=16까지 충분 — 성장 로직 대신 상한).

### 14.8 저장·불러오기 (M5)

- 위치 `saves/<name>/` (기본 `default`, `VF_SAVE` 또는 `--save`). `.gitignore`에 이미 있음.
- `world.json` (serde + serde_json): `{ "version": 1, "seed": u64, "player": { "pos": [f32;3], "yaw": f32, "pitch": f32, "fly": bool } }`.
- 청크 파일 `chunks/c_{x}_{y}_{z}.bin`: 매직 `b"VFC1"` + `lz4_flex::compress_prepend_size(blocks as LE u16 bytes)`. **수정된 청크만** 쓴다.
- `World`: `modified: HashSet<IVec3>`(set_block 시 삽입), `saved_version: HashMap<IVec3, u64>`. 저장 대상 = modified ∧ `version != saved_version`. 언로드 직전·30초마다·종료 시(CloseRequested, ESC 종료) 저장. 로딩 시 파일이 있으면 생성 대신 읽고 `modified`에 넣는다(스레드 잡 `Gen`은 파일 존재를 메인에서 먼저 검사해 `Load`로 분기).
- 계약:

```rust
// world/save.rs
pub struct SaveDir { root: PathBuf }
impl SaveDir {
    pub fn open(name: &str) -> anyhow::Result<Self>;                 // 디렉터리 생성
    pub fn chunk_path(&self, cp: IVec3) -> PathBuf;
    pub fn write_chunk(&self, cp: IVec3, chunk: &Chunk) -> anyhow::Result<()>;
    pub fn read_chunk(&self, cp: IVec3) -> anyhow::Result<Option<Chunk>>;  // 없으면 None, 손상이면 Err(로그 후 재생성)
    pub fn write_meta(&self, meta: &WorldMeta) -> anyhow::Result<()>;
    pub fn read_meta(&self) -> anyhow::Result<Option<WorldMeta>>;
}
// 테스트: save_roundtrip_chunk_bytes_equal, world_loads_saved_chunk_instead_of_generating,
//        unmodified_chunks_are_not_written, world_meta_roundtrip
```

### 14.9 투명 패스 (M5)

- `BlockDef`에 `translucent: bool`(WATER, GLASS) 추가. 메셔는 `ChunkMeshes { opaque: ChunkMesh, translucent: ChunkMesh }`를 내는 `mesh_chunk_all`/`mesh_chunk_greedy_all`을 추가한다(기존 함수는 opaque만 반환, 테스트 유지).
- 컬링 규칙 확장: 면을 지우는 조건 = 이웃이 opaque **또는 이웃 id가 같고 translucent**(물-물, 유리-유리 사이 면 제거). 잎은 그대로(잎-잎 면 유지).
- **물 윗면 낮추기**: 정점 `a` 비트 23 = `lowered`(셰이더에서 y −0.125). 물 블록 위가 물이 아니면 +Y 면 4정점과 옆면의 위쪽 2정점에 세운다. 정수 좌표 정점 포맷을 바꾸지 않기 위한 결정.
- 파이프라인(`render/translucent.rs`): 같은 `chunk.wgsl`(프래그먼트가 텍스처 알파를 내보냄), `blend: ALPHA_BLENDING`, `depth_write_enabled: false`, `depth_compare: Less`, `cull_mode: None`. 절차적 텍스처 알파: water 150, glass 90, 나머지 255.
- 순서: 불투명 → outline → 투명. 투명 청크는 카메라와 청크 중심 거리로 **뒤→앞** 정렬(프레임마다, 청크 단위).
- 수중 틴트(선택): 카메라 위치 블록이 WATER면 `time_res.w = 1.0` → 프래그먼트에서 `mix(color, vec3(0.1,0.3,0.6), 0.45)`.
- 테스트: `mesher_splits_translucent_blocks`, `mesher_culls_faces_between_same_translucent_blocks`, `water_top_face_sets_lowered_flag`.
- 스냅샷: 해안선 위에서 물 아래 모래가 비친다. `--edits`로 유리창을 세우면 뒤가 보인다.

### 14.10 메시 아레나 (M5, 조건부)

M4 끝의 측정에서 R=12 비행 60초 동안 `max`가 20ms를 넘고 그 원인이 청크 업로드(버퍼 생성/해제)일 때만 구현한다. 아니면 M7로 미룬다(LOG에 측정값 기록).
구현 시: 정점 96MB + 인덱스 72MB 버퍼 하나씩, 오프셋 정렬 프리리스트(인접 병합), `draw_indexed(range, base_vertex, 0..1)`로 인덱스는 청크 로컬 유지. 가득 차면 그 청크만 전용 버퍼로 폴백하고 1회 경고. 테스트 `arena_alloc_free_coalesces`, `arena_full_falls_back`.

### 14.11 M4·M5 분기 선결정 (묻지 말고 이렇게)

- 스레드 풀 → rayon + std mpsc. 편집 청크는 메인에서 동기 메싱
- greedy 키 → `(tex, ao×4)` 정확 일치. 라이트(M6)는 그때 키에 추가
- 물 윗면 → 비트 23 플래그(정점 포맷 유지)
- 투명 컬 → `cull_mode: None`, 청크 단위 뒤→앞 정렬만
- 저장 → 수정 청크만, lz4, world.json에 플레이어. 리전 파일 없음
- 슬롯 → 16384 고정
- 반경 → 기본 10, 검증 12, `VF_RADIUS`
- 물리 → 고정 1/120 substep, Y→X→Z, 계단 오르기 없음
- 잎 → solid, opaque=false 유지(잎-잎 면 유지). 알파 컷아웃 텍스처는 M7 텍스처 작업 때
- 아레나 → 조건부(14.10)
- M5가 끝나면 **멈춘다**. Claude 리뷰 → M6
