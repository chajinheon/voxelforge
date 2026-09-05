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
    pub fn set_block(&mut self, bp: IVec3, id: BlockId) -> bool;  // 로딩된 청크만. 해당 청크 dirty, 경계면이면 이웃도 dirty.
                                                                  // **같은 id면 false(no-op)** — modified/version을 건드리지 않기 위함 (M5 리뷰에서 확정)
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

### 14.12 M4·M5 리뷰에서 확정된 것 (2026-09-04 20:30, Claude)

- 구현이 계약을 넘어 잘한 것: 저장용 `version`(내용 변경)과 재메싱용 `epoch`(이웃 변화 포함)을 분리 — stale 메시 폐기가 정확해졌다. 물리 substep 상한(8)을 두지 않은 것도 수용(터널링에 더 안전, dt는 main에서 0.1s 클램프).
- `WorldGen::generate`는 `cp.y >= 4`(블록 y ≥ 128)를 빈 청크로 즉시 반환한다. 근거: 높이 최대 64+24=88, 나무 최대 +8 → 96 < 128. **지형 높이나 나무 규칙을 바꾸면 이 상수(`MAX_GEN_CHUNK_Y`)도 함께 바꾼다.** M6.0에서 이름 붙이고 테스트로 고정.
- 투명 파이프라인은 `ChunkPipeline`을 재사용해 Globals 버퍼와 4MB 유니폼 아레나를 한 벌 더 가진다. 지금은 무해. M7 디퍼드 재구성 때 공유로 합친다.
- greedy 정점 비율은 장면 의존: 나무 장면 39%, 스폰 평지+동굴 장면 67%. `≤50%`는 목표치일 뿐 완료 조건에서 뺀다. M6 조명 값이 키에 들어가면 더 쪼개지므로, 그때 비트 평면 greedy 또는 AO 스무딩을 검토.
- `snapshot --edits`의 no-op 편집(같은 id)은 오류가 아니라 경고여야 한다(M6.0).
- 저장 경로 `saves/`는 cwd 기준. `cargo run`에서는 프로젝트 루트지만 바이너리를 다른 곳에서 실행하면 그 자리에 생긴다. M6.0에서 `assets::dir()`의 부모(프로젝트 루트) 기준으로 고정.
- 저장 엔드투엔드는 우연히 실측됐다: 리뷰 smoke 창이 전면에 뜨며 진헌의 입력을 받아 블록 하나가 부서졌고, 종료 시 청크 파일 정확히 1개, 생성 지형과의 차이 정확히 1블록((1,65,−2) GRASS→AIR)이 저장됐다. 편집 없는 실행 3회는 청크 파일 0개.

## 15. M6 상세 설계 — 마크식 조명·낮밤·바람

M6는 §4의 `Chunk`·`PaddedChunk`·정점 패킹과 §14의 `version`/메시 `epoch` 분리를 확장한다. 블록 저장 포맷은 바꾸지 않는다. 조명은 런타임 파생 데이터이며 저장하지 않는다.

### 15.1 새 모듈

| 경로                          | 역할                                                   | 마일스톤 |
| --------------------------- | ---------------------------------------------------- | ---- |
| `src/world/light.rs`        | 32×32×256 청크 열 조명 스냅샷, 하늘광·블록광 플러드필, 조명 적용           | M6.1 |
| `src/stream/lighting.rs`    | 조명 dirty 열, 조명 epoch, 워커 잡 발행·stale 결과 폐기·경계 고정점 반복  | M6.2 |
| `src/stream/jobs.rs`        | `Gen/Load/Light/Mesh` 잡과 결과 타입. `stream.rs` 500줄 방지  | M6.1 |
| `src/render/day_cycle.rs`   | 20분 낮밤 주기, 태양 궤도, 하늘색·일광 계수                          | M6.3 |
| `assets/shaders/chunk.wgsl` | 정점 light/sky 디코드, 마크식 광도표, 바람 변위                     | M6.3 |
| `src/bin/snapshot.rs`       | `--fixture`, `--view`, `--day-phase`, `--world-time` | M6.4 |

`main.rs`와 `stream.rs`에는 알고리즘을 추가하지 않는다. `stream.rs`의 조명 관련 메서드는 `src/stream/lighting.rs`의 `impl Streamer`로 둔다.

### 15.2 알고리즘·수식

#### 15.2.1 블록·조명 데이터

새 블록:

```rust
pub const TORCH: BlockId = 12;
```

`TORCH`는 현재 큐브 메셔를 그대로 쓰는 1×1×1 발광 블록이다.

* `solid = true`
* `opaque = true`
* `translucent = false`
* `emission = 14`
* 텍스처 레이어 `13`, 이름 `"torch"`
* 비큐브 횃불 모델·교차 평면·벽 부착은 하지 않는다.

`BlockDef`에 다음 필드를 추가한다.

```rust
pub emission: u8; // 0..=15
```

`TORCH` 외 모든 블록은 `emission = 0`이다. 광원 셀이 opaque여도 자기 `emission`을 시드하고, 여섯 이웃의 비opaque 셀로 전파한다.

핫바는 9칸을 유지한다.

```rust
pub const HOTBAR: [BlockId; 9] = [
    STONE, GRASS, SAND, WATER, LOG, PLANKS, GLASS, BRICK, TORCH,
];
```

`DIRT`는 핫바에서 빠지지만 ID와 텍스처는 유지한다.

청크 조명은 블록당 한 바이트다.

```text
light byte:
  bits [0..4) = block light, 0..15
  bits [4..8) = sky light,   0..15
```

`Chunk`에는 `Box<[u8; 32768]>`가 추가된다. `PaddedChunk`에는 블록과 같은 34³ 인덱스로 `Box<[u8; 34*34*34]>`가 추가된다.

* opaque 블록 셀은 하늘광 0이다.
* 비opaque 블록은 하늘광·블록광을 통과시킨다.
* WATER, GLASS, LEAVES도 아래 규칙에서는 비opaque 통로다.
* 블록광은 한 블록 이동할 때 항상 1 감소한다.
* 하늘광은 `−Y` 이동에는 감소하지 않고, `+Y`, `±X`, `±Z` 이동에는 1 감소한다.
* 값은 항상 `0..=15`로 포화한다.

조명은 저장하지 않는다. `VFC1` 청크 파일과 `world.json.version = 1`을 유지한다. 로드된 청크의 조명은 0·미초기화 상태로 시작하고, 열 조명이 완료되기 전에는 최초 메시 잡을 발행하지 않는다. 따라서 저장 포맷 버전 2는 만들지 않는다.

#### 15.2.2 조명 계산 단위

조명 잡 하나는 수평 청크 좌표 `(cx, cz)`의 전체 수직 열을 처리한다.

```text
world x = cx*32 .. cx*32+31
world y = 0 .. 255
world z = cz*32 .. cz*32+31
cells   = 32*256*32 = 262,144
```

열 안의 모든 8개 청크가 로딩된 뒤에만 잡을 만든다. 열 입력은 다음이다.

* 중앙 열의 블록 ID 262,144개.
* `−X`, `+X`, `−Z`, `+Z` 이웃 열의 맞닿은 조명면 네 장.
* 면 하나는 `32×256 = 8192` 바이트.
* 이웃 열이 미로딩이면 블록광 0, 하늘광 15인 열린 AIR 경계로 취급한다. 이는 §12의 「미로딩 이웃은 AIR」 결정을 그대로 적용한 것이다.
* 수직 경계는 월드 전체를 한 잡에서 처리하므로 별도 이웃 입력이 없다. `y < 0`은 STONE, `y >= 256`은 열린 하늘이다.

열 인덱스:

```rust
index = x + 32 * (z + 32 * y); // x,z: 0..31, y: 0..255
```

경계면 인덱스:

```rust
face_index = u + 32 * y; // X면의 u=z, Z면의 u=x
```

#### 15.2.3 하늘광 초기화

각 `(x,z)` 열을 `y=255`부터 `0`까지 스캔한다.

```text
beam = 15
for y = 255 .. 0:
    if block(x,y,z).opaque:
        sky(x,y,z) = 0
        beam = 0
    else:
        sky(x,y,z) = max(sky(x,y,z), beam)
```

직하 방향에서는 감쇠하지 않는다. 이 단계가 열린 하늘 아래의 모든 AIR·WATER·GLASS·LEAVES 셀에 하늘광 15를 만든다.

이후 네 수평 경계로부터 들어오는 하늘광은 경계를 한 번 넘었으므로 `incoming - 1`로 중앙 열에 시드한다.

하늘광 BFS 후보값:

```text
candidate =
    current                         if direction == -Y
    current.saturating_sub(1)       otherwise
```

도착 셀이 opaque면 버린다. `candidate > old`일 때만 갱신하고 큐에 넣는다.

#### 15.2.4 블록광 초기화

중앙 열의 모든 `emission > 0` 셀을 자기 emission 값으로 시드한다. 네 수평 경계에서 들어오는 블록광은 `incoming - 1`로 시드한다.

블록광 BFS 후보값:

```text
candidate = current.saturating_sub(1)
```

여섯 방향 모두 같다. 도착 셀이 opaque면 버린다. 단, opaque 광원 셀 자체의 시드는 허용한다.

BFS는 `VecDeque<u32>` 두 개를 사용한다. 하늘광과 블록광을 섞은 하나의 큐로 만들지 않는다.

#### 15.2.5 청크 경계 고정점

열 계산 결과에서 네 수평 경계면을 추출한다. 적용 전의 경계 바이트와 정확히 비교한다. 해시만 비교하지 않는다.

한 면이라도 바뀌면 그 면과 맞닿은 이웃 열을 다시 dirty로 만든다. 광원 제거 시 이웃의 오래된 경계광이 잠시 다시 들어올 수 있으나, 각 경계 왕복에서 값이 최소 1 감소하므로 최대 15회의 전파로 0에 수렴한다.

동기 경로와 비동기 경로 모두 동일한 종료 조건을 쓴다.

```text
dirty 열 없음
AND in-flight 조명 잡 없음
AND 적용 대기 결과 없음
AND 원하는 모든 수평 열의 8개 청크가 light_initialized
```

동기 스냅샷은 dirty 열 큐가 빌 때까지 거리·`x`·`z` 순으로 계산한다. 한 열이 16회보다 많이 다시 계산되면 오류로 종료한다. 정상 입력에서 16회는 도달하지 않아야 한다.

#### 15.2.6 dirty·epoch·재메싱

콘텐츠 `version`, 메시 `epoch`, 조명 `light_epoch`는 서로 다른 값이다.

* 블록을 편집하면 기존처럼 콘텐츠 `version`을 증가시키고 메시 dirty를 만든다.
* 동시에 해당 수평 열을 urgent light dirty로 만든다.
* 네 수평 이웃 열도 일반 light dirty로 만든다.
* 청크 삽입·로드·언로드도 해당 열과 네 이웃 열을 light dirty로 만든다.
* 조명 dirty는 저장 대상 `modified`에 영향을 주지 않는다.
* 조명 결과는 발행 당시 `light_epoch`와 현재 값이 같을 때만 적용한다.
* stale 결과는 조명 배열·메시 dirty·`modified`를 전혀 건드리지 않고 폐기한다.
* 적용된 조명 바이트가 실제로 달라진 청크만 메시 dirty로 만든다.
* 조명만 바뀐 청크는 콘텐츠 `version`을 올리지 않는다.
* 메시 결과의 기존 `version + mesh epoch` 검사는 유지한다.

블록 편집의 기하 변화는 기존 urgent 동기 메싱으로 즉시 보이게 해도 된다. 그 프레임에는 이전 조명이 쓰일 수 있다. 새 조명 결과가 적용되면 다시 메싱한다. 목표 조명 반응 지연은 100ms 이하이다.

조명 잡은 기존 rayon 풀을 공유한다.

* 워커 수 `w`: §14.2 그대로 최대 8.
* 조명 in-flight 상한: `w`.
* 조명 잡 발행 우선순위: urgent 편집 열 → 새로 완성된 카메라 근접 열 → 일반 경계 재전파 → 일반 메시.
* `Gen + Load + Light + Mesh` 계산 잡의 총 대기 상한: `2w`.
* 메인 스레드 조명 스냅샷 복사·결과 적용은 기존 6ms 예산 안에 포함한다.
* 한 프레임에 적용하는 조명 결과는 시간 기준이며 개수 제한을 두지 않는다.

#### 15.2.7 정점 조명

한 면의 각 코너는 면 앞쪽 층의 네 셀을 샘플한다. AO와 같은 `front`, `s1`, `s2`를 사용한다.

```text
samples = [
    front,
    front + s1,
    front + s2,
    front + s1 + s2,
]
corner_channel = (sum(samples) + 2) / 4
```

정수 반올림 평균이다. opaque 셀은 저장된 조명이 0이므로 별도 제외하지 않는다.

현재 블록이 광원이면:

```text
corner_block_light = max(corner_block_light, def(current_id).emission)
```

하늘광에는 이 보정이 없다.

greedy 셀 키는 다음 전부가 정확히 같아야 병합한다.

```rust
struct FaceKey {
    tex: u16,
    ao: [u8; 4],
    block_light: [u8; 4],
    sky_light: [u8; 4],
    lowered: [bool; 4],
}
```

조명 값을 양자화하거나 얼굴 평균 하나로 줄이지 않는다. 비트 평면 greedy도 M6에서는 하지 않는다.

정점 패킹은 기존 계약을 그대로 사용한다.

```text
a:
  x[0..6) y[6..12) z[12..18) face[18..21) ao[21..23)
  lowered[23] reserved[24..32)

b:
  tex[0..16) block_light[16..20) sky_light[20..24)
  reserved[24..32)
```

#### 15.2.8 마크식 광도표

WGSL 상수 배열은 다음 식의 16개 값을 그대로 쓴다.

```text
LIGHT_LEVEL[L] = 0.8^(15-L)
```

|  L |    선형 광도 |  L |    선형 광도 |
| -: | -------: | -: | -------: |
|  0 | 0.035184 |  8 | 0.209715 |
|  1 | 0.043980 |  9 | 0.262144 |
|  2 | 0.054976 | 10 | 0.327680 |
|  3 | 0.068719 | 11 | 0.409600 |
|  4 | 0.085899 | 12 | 0.512000 |
|  5 | 0.107374 | 13 | 0.640000 |
|  6 | 0.134218 | 14 | 0.800000 |
|  7 | 0.167772 | 15 | 1.000000 |

M6 최종 조명:

```wgsl
let block_term = light_level[block_light];
let sky_term = light_level[sky_light] * globals.sun_dir.w;
let illumination = max(block_term, sky_term);
let rgb = sampled.rgb * face_shade * ao_factor * illumination;
```

`max`는 스칼라 기준이다. 블록광과 하늘광을 더하지 않는다. M9의 RGB GI가 들어오기 전까지 블록광은 백색이다.

#### 15.2.9 낮밤

게임 하루는 정확히 1200초다. 시작 시각은 정오다. M6에서는 월드 시각을 저장하지 않는다.

```text
phase = fract(world_time_seconds / 1200)
theta = 2π * phase

sun_dir = normalize((
    0.25 * sin(theta),
   -cos(theta),
    0.9682458 * sin(theta)
))
```

`sun_dir`은 태양에서 지면으로 빛이 진행하는 방향이다.

* phase 0.00: 정오, `(0,-1,0)`
* phase 0.25: 일몰
* phase 0.50: 자정, `(0,+1,0)`
* phase 0.75: 일출

```text
sun_height = clamp(-sun_dir.y, -1, 1)
day_weight = smoothstep(-0.08, 0.12, sun_height)
sun_factor = mix(0.08, 1.00, day_weight)
```

하늘색은 다음 선형 RGB 표를 순환 보간한다. 각 구간의 로컬 보간 인자에는 `smoothstep(0,1,t)`를 적용한다.

| phase | 선형 RGB                  |
| ----: | ----------------------- |
|  0.00 | `(0.530, 0.810, 0.920)` |
|  0.18 | `(0.420, 0.580, 0.720)` |
|  0.25 | `(0.320, 0.120, 0.055)` |
|  0.32 | `(0.035, 0.055, 0.120)` |
|  0.50 | `(0.004, 0.008, 0.025)` |
|  0.68 | `(0.035, 0.055, 0.120)` |
|  0.75 | `(0.320, 0.120, 0.055)` |
|  0.82 | `(0.420, 0.580, 0.720)` |
|  1.00 | `(0.530, 0.810, 0.920)` |

렌더 패스 clear 색은 `Globals.sky_color.rgb`를 CPU에서 `wgpu::Color`로 변환해 사용한다.

#### 15.2.10 바람

M6에서 바람을 받는 것은 LEAVES 전체와 GRASS 블록의 `+Y` 면뿐이다. 별도 tall-grass 블록은 만들지 않는다.

```wgsl
let p = world_pos.xz;
let phase = dot(p, vec2<f32>(0.173, 0.127)) + globals.time_res.x * 1.7;
let gust = (sin(phase) + 0.5 * sin(phase * 2.17 + 1.3)) / 1.5;
let wind_dir = normalize(vec2<f32>(0.8, 0.6));
```

* LEAVES 진폭: `0.045` 블록.
* GRASS `+Y` 진폭: `0.012` 블록.
* 변위는 XZ에만 적용한다.
* Y 변위는 0이다.
* 위상은 청크 로컬이 아니라 `world_pos`로 계산한다.
* lowered 물 변위와 섞지 않는다.
* LEAVES와 GRASS `+Y`의 greedy 쿼드는 가로·세로 각각 최대 4블록으로 제한한다. 그 외 면은 기존 무제한 greedy다.
* M7 그림자 셰이더는 같은 식을 사용한다.

### 15.3 Rust/WGSL 계약

```rust
// world/block.rs
pub const TORCH: BlockId = 12;

pub struct BlockDef {
    pub name: &'static str,
    pub solid: bool,
    pub opaque: bool,
    pub translucent: bool,
    pub textures: [u16; 6],
    pub emission: u8,
}

// world/chunk.rs
pub const LIGHT_MAX: u8 = 15;
pub const BLOCK_LIGHT_MASK: u8 = 0x0f;
pub const SKY_LIGHT_MASK: u8 = 0xf0;

pub fn pack_light(block: u8, sky: u8) -> u8;
pub fn block_light(packed: u8) -> u8;
pub fn sky_light(packed: u8) -> u8;

impl Chunk {
    pub fn get_light(&self, l: UVec3) -> u8;
    pub fn set_light(&mut self, l: UVec3, packed: u8);
    pub fn light_initialized(&self) -> bool;
}

impl PaddedChunk {
    pub fn get_light(&self, x: i32, y: i32, z: i32) -> u8;
}

// world/light.rs
pub const LIGHT_COLUMN_CELLS: usize = 32 * 256 * 32;
pub const LIGHT_BOUNDARY_CELLS: usize = 32 * 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LightColumn {
    pub x: i32,
    pub z: i32,
}

pub struct LightColumnSnapshot {
    pub column: LightColumn,
    pub epoch: u64,
    pub blocks: Box<[BlockId; LIGHT_COLUMN_CELLS]>,
    pub incoming: [Box<[u8; LIGHT_BOUNDARY_CELLS]>; 4], // -X,+X,-Z,+Z
}

pub struct LightColumnResult {
    pub column: LightColumn,
    pub epoch: u64,
    pub light: Box<[u8; LIGHT_COLUMN_CELLS]>,
    pub boundary: [Box<[u8; LIGHT_BOUNDARY_CELLS]>; 4],
}

pub fn solve_column(input: &LightColumnSnapshot) -> LightColumnResult;

impl World {
    pub fn column_loaded(&self, column: LightColumn) -> bool;
    pub fn light_column_snapshot(
        &self,
        column: LightColumn,
        epoch: u64,
    ) -> Option<LightColumnSnapshot>;
    pub fn apply_light_column(
        &mut self,
        result: LightColumnResult,
    ) -> LightApplyResult;
    pub fn take_light_dirty(&mut self) -> Vec<LightColumn>;
}

pub struct LightApplyResult {
    pub changed_chunks: Vec<IVec3>,
    pub changed_boundaries: [bool; 4],
}

// render/day_cycle.rs
pub const DAY_LENGTH_SECONDS: f32 = 1200.0;

pub struct DayState {
    pub phase: f32,
    pub sun_dir: Vec3,
    pub sun_factor: f32,
    pub sky_color: Vec3,
}

pub fn day_state(world_time_seconds: f32) -> DayState;
```

M6 `Globals`:

```rust
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Globals {
    pub view_proj: [[f32; 4]; 4], // offset 0
    pub cam_pos: [f32; 4],        // offset 64
    pub sun_dir: [f32; 4],        // xyz direction, w sun_factor
    pub time_res: [f32; 4],       // time, output width, output height, underwater
    pub sky_color: [f32; 4],      // linear rgb, w day phase
}
```

`GLOBALS_SIZE = 128`.

WGSL은 같은 순서를 사용한다. 정점 출력에는 보간 가능한 `block_light: f32`, `sky_light: f32`를 추가한다. `tex`만 `@interpolate(flat)`이다.

`PaddedChunk`의 미로딩 수평 이웃 조명은 `(block=0, sky=15)`, `y<0`은 `(0,0)`, `y>=256`은 `(0,15)`다.

M6 이후 `snapshot` 공통 옵션:

```text
--fixture terrain|m6-light-room|m6-wind
--view final|light
--day-phase <0.0..1.0>
--world-time <seconds>
```

`--day-phase`와 `--world-time`을 동시에 주면 오류다. 둘 다 없으면 정오·시간 0이다.

### 15.4 상수

| 상수                 |         값 |
| ------------------ | --------: |
| 블록광 최대             |        15 |
| 하늘광 최대             |        15 |
| TORCH emission     |        14 |
| 블록광 이동 감쇠          |         1 |
| 하늘광 수평·상향 감쇠       |         1 |
| 하늘광 하향 감쇠          |         0 |
| 조명 열 크기            | 32×256×32 |
| 경계면 크기             |    32×256 |
| 조명 in-flight       |  워커 수 `w` |
| 경계 고정점 최대 재계산      |     열당 16 |
| 낮밤 주기              |     1200초 |
| 야간 sun factor      |      0.08 |
| 주간 sun factor      |      1.00 |
| LEAVES 바람 진폭       |     0.045 |
| GRASS 상면 바람 진폭     |     0.012 |
| 바람 각속도             | 1.7 rad/s |
| 바람 greedy 최대 크기    |       4×4 |
| 조명 반응 목표           |  100ms 이하 |
| 조명 배열 메모리          | 청크당 32KiB |
| R=12 5000청크 조명 메모리 | 156.25MiB |

### 15.5 테스트 계약

다음 이름을 그대로 사용한다.

```text
snapshot_noop_edit_is_warning
max_gen_chunk_y_covers_generated_content
save_root_is_project_root
light_pack_roundtrip
skylight_descends_without_attenuation
skylight_spreads_sideways_with_unit_falloff
block_light_falls_off_one_per_block
opaque_block_stops_both_light_channels
torch_emission_is_fourteen
light_crosses_column_boundary
removing_torch_converges_to_zero
stale_light_result_is_discarded
light_apply_does_not_mark_chunk_modified
mesher_bakes_corner_light_and_sky
greedy_never_merges_different_light_keys
wind_offset_matches_at_shared_world_vertex
save_v1_relights_instead_of_serializing_light
```

판정 기준:

* 수직 AIR 256칸은 전부 sky 15.
* 수평으로 0, 1, 2, 15칸 떨어진 값은 각각 15, 14, 13, 0.
* TORCH에서 비opaque 통로를 따라 거리가 `d`인 셀은 `max(14-d,0)`.
* TORCH 제거 후 dirty 큐가 빌 때 영향 범위의 block light가 전부 0.
* 조명 적용 전후 `modified`, 콘텐츠 `version`, 저장 파일 대상 수가 동일.
* 동일 월드 정점은 서로 다른 청크 메시에서도 바람 오프셋 차이 `< 1e-6`.
* `MAX_GEN_CHUNK_Y = 4`; `cp.y >= 4` 생성 청크는 항상 비어 있다.

### 15.6 스냅샷 검증

`m6-light-room` fixture 계약:

* 완전히 밀폐된 STONE 방.
* 정면 SAND 패널 네 곳의 중심 픽셀이 각각 block light 13, 9, 5, 1을 보도록 TORCH와 통로를 배치한다.
* 카메라와 투영은 fixture가 고정한다.
* 크기 640×360에서 프로브 중심은 `(155,180)`, `(265,180)`, `(375,180)`, `(486,180)`.
* `--view light`는 텍스처·AO·면 음영·톤매핑을 우회하고 `illumination`을 선형 회색으로 출력한다.

```bash
cargo run --release --bin snapshot -- \
  --fixture m6-light-room --size 640x360 --view light \
  --day-phase 0.5 --out /tmp/vf_m6_light.png

python3 - <<'PY'
from PIL import Image
im = Image.open("/tmp/vf_m6_light.png").convert("RGB")

def srgb_to_linear(v):
    v /= 255.0
    return v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4

def probe(x, y):
    vals = []
    for yy in range(y-2, y+3):
        for xx in range(x-2, x+3):
            r, g, b = im.getpixel((xx, yy))
            vals.append(0.2126*srgb_to_linear(r)
                        + 0.7152*srgb_to_linear(g)
                        + 0.0722*srgb_to_linear(b))
    return sum(vals) / len(vals)

p = [probe(155,180), probe(265,180), probe(375,180), probe(486,180)]
expected = [1.0, 0.4096, 0.167772, 0.068719]
ratios = [v / p[0] for v in p]
print("ratios", ratios)
for got, want in zip(ratios, expected):
    assert abs(got-want) <= 0.06, (got, want)
PY
```

낮밤:

```bash
cargo run --release --bin snapshot -- \
  --fixture terrain --seed 1 --radius 4 --size 640x360 \
  --day-phase 0.0 --out /tmp/vf_m6_day.png
cargo run --release --bin snapshot -- \
  --fixture terrain --seed 1 --radius 4 --size 640x360 \
  --day-phase 0.5 --out /tmp/vf_m6_night.png

python3 - <<'PY'
from PIL import Image
def lin(v):
    v /= 255.0
    return v/12.92 if v <= .04045 else ((v+.055)/1.055)**2.4
def y(path, box):
    im=Image.open(path).convert("RGB")
    vals=[]
    for py in range(box[1],box[3]):
        for px in range(box[0],box[2]):
            r,g,b=im.getpixel((px,py))
            vals.append(.2126*lin(r)+.7152*lin(g)+.0722*lin(b))
    return sum(vals)/len(vals)
day=y("/tmp/vf_m6_day.png",(40,20,180,80))
night=y("/tmp/vf_m6_night.png",(40,20,180,80))
print(day, night, night/day)
assert night/day <= 0.05
PY
```

`m6-wind` fixture 계약:

* 왼쪽 crop `(160,60,480,270)`은 LEAVES.
* 오른쪽 crop `(500,80,620,260)`은 STONE 기준물.
* 시간 이외의 카메라·월드·노출은 동일하다.

```bash
cargo run --release --bin snapshot -- \
  --fixture m6-wind --size 640x360 --world-time 0.0 \
  --out /tmp/vf_m6_wind_a.png
cargo run --release --bin snapshot -- \
  --fixture m6-wind --size 640x360 --world-time 0.75 \
  --out /tmp/vf_m6_wind_b.png

python3 - <<'PY'
from PIL import Image, ImageChops
a=Image.open("/tmp/vf_m6_wind_a.png").convert("RGB")
b=Image.open("/tmp/vf_m6_wind_b.png").convert("RGB")
d=ImageChops.difference(a,b).convert("L")
def fraction(box):
    c=d.crop(box)
    return sum(v > 3 for v in c.getdata())/(c.width*c.height)
leaf=fraction((160,60,480,270))
stone=fraction((500,80,620,260))
print("leaf",leaf,"stone",stone)
assert 0.005 <= leaf <= 0.20
assert stone <= 0.001
PY
```

### 15.7 성능 예산

Apple M5, 2560×1440, R=12 기준이다.

| 항목                          |               예산 |
| --------------------------- | ---------------: |
| 열 solver 워커 중앙값             |        ≤ 6.0ms/열 |
| 열 solver 워커 p95             |       ≤ 12.0ms/열 |
| 메인 스레드 조명 스냅샷+적용            | 기존 6ms 스트리밍 예산 안 |
| 조명 결과 1개 적용                 |          ≤ 2.0ms |
| R=12 최초 `stream: settled`   |           < 5.0초 |
| 정착 후 opaque+transparent GPU |          ≤ 5.5ms |
| 정착 후 전체 프레임                 |      평균 ≤ 16.6ms |
| 정착 후 1초 구간 max              |           < 25ms |
| M5 대비 총 정점 수                |           ≤ 2.0배 |
| 편집→광원 변화 표시                 |          ≤ 100ms |

로그:

```text
lighting: columns N solves N median X.XXms p95 X.XXms max_requeues N
```

정착 로그는 조명 큐까지 빈 뒤에만 출력한다.

### 15.8 M6 분기 선결정 — 묻지 말고 이렇게

* 조명 저장 → 저장하지 않음. `VFC1`, `world.json.version=1` 유지.
* 조명 계산 단위 → 32×32×256 수직 열.
* 제거 처리 → 별도 remove BFS가 아니라 열 전체 재계산 + 경계 고정점.
* 미로딩 수평 이웃 → AIR, block 0, sky 15.
* 블록광 → 모든 방향 1 감쇠.
* 하늘광 → 아래 0, 나머지 방향 1 감쇠.
* WATER·GLASS·LEAVES → 조명 통과.
* greedy 키 → 텍스처·AO 4개·block light 4개·sky light 4개·lowered 4개 정확 일치.
* TORCH → ID 12, emission 14, 기존 큐브 메셔.
* 핫바 → `DIRT`를 빼고 9번에 TORCH.
* 낮밤 → 1200초, 시작 정오, 시각 저장 안 함.
* 바람 → LEAVES와 GRASS +Y만, XZ 변위.
* 비트 평면 greedy·조명 양자화 → 하지 않음.
* M6가 끝나면 LOG 기록 후 멈추고 Claude 리뷰를 받는다.

---

## 16. M7 상세 설계 — HDR·G버퍼·디퍼드·CSM·SSAO·대기·후처리

M7부터 지형은 서피스에 직접 그리지 않는다. 모든 장면 패스는 Renderer 소유 오프스크린 타깃에 그린다. 서피스에는 네이티브 해상도 최종 LDR 블릿만 그리고, M10에서 그 위에 UI를 올린다. `Renderer`는 계속 `Surface`와 `Window`를 모른다.

### 16.1 새 모듈

| 경로                                | 역할                                                   | 마일스톤 |
| --------------------------------- | ---------------------------------------------------- | ---- |
| `src/render/scene_bindings.rs`    | 공유 Globals, 블록 텍스처, 재질 LUT, 청크 dynamic uniform arena | M7.0 |
| `src/render/targets.rs`           | 렌더 스케일 기반 G버퍼·HDR·LDR·AO·블룸 타깃 생성·리사이즈               | M7.1 |
| `src/render/gbuffer.rs`           | opaque·alpha-cutout 청크 G버퍼 패스                        | M7.1 |
| `src/render/shadow.rs`            | 3-cascade CSM 행렬·안정화·섀도 패스                           | M7.2 |
| `src/render/ssao.rs`              | 16샘플 SSAO와 양방향 bilateral blur                        | M7.3 |
| `src/render/deferred.rs`          | G버퍼·CSM·SSAO·M6 조명을 HDR로 합성                          | M7.3 |
| `src/render/sky.rs`               | Rayleigh–Mie sky LUT, 태양·달                           | M7.3 |
| `src/render/post/bloom.rs`        | 5단계 bloom                                            | M7.4 |
| `src/render/post/exposure.rs`     | 평균 log luminance reduction·시간 적응                     | M7.4 |
| `src/render/post/tonemap.rs`      | ACES fitted·Catmull–Rom 업스케일                         | M7.4 |
| `src/render/debug_view.rs`        | F4·`snapshot --view` 중간 버퍼 시각화                       | M7.1 |
| `src/render/gpu_timing.rs`        | 선택적 GPU timestamp, 비동기 readback                      | M7.5 |
| `src/render/mesh_arena.rs`        | §14.10 조건을 다시 만족할 때만 VB/IB arena                     | M7.0 |
| `assets/shaders/shadow.wgsl`      | CSM depth·alpha-cutout·바람                            | M7.2 |
| `assets/shaders/ssao.wgsl`        | SSAO·blur                                            | M7.3 |
| `assets/shaders/sky_lut.wgsl`     | 대기 산란 LUT                                            | M7.3 |
| `assets/shaders/deferred.wgsl`    | HDR deferred lighting                                | M7.3 |
| `assets/shaders/translucent.wgsl` | HDR forward water·glass                              | M7.5 |
| `assets/shaders/bloom.wgsl`       | downsample·upsample                                  | M7.4 |
| `assets/shaders/exposure.wgsl`    | luminance reduction·adaptation                       | M7.4 |
| `assets/shaders/tonemap.wgsl`     | ACES·업스케일                                            | M7.4 |
| `assets/shaders/present.wgsl`     | 네이티브 LDR → 출력 타깃                                     | M7.1 |

### 16.2 알고리즘·수식

#### 16.2.1 프레임 그래프

정확한 순서:

```text
1. CSM shadow cascades 0..2
2. G-buffer opaque + alpha-cutout
3. SSAO raw
4. SSAO horizontal bilateral blur
5. SSAO vertical bilateral blur
6. sky LUT 갱신
7. deferred lighting + sky → HDR
8. forward translucent WATER/GLASS → HDR
9. outline → HDR
10. bloom downsample 0..4
11. bloom upsample 4..0
12. log luminance reduction + exposure adaptation
13. ACES + bloom + Catmull–Rom upscale → native LDR texture
14. native LDR fullscreen present → caller output view
15. M10 UI
```

M8·M9는 7~10 사이에 패스를 삽입한다.

#### 16.2.2 공유 장면 바인딩

M5의 opaque·translucent 파이프라인이 각각 소유한 Globals와 4MiB 청크 uniform arena를 제거한다.

`SceneBindings` 한 개가 다음을 소유한다.

* Globals buffer/layout/bind group 한 벌.
* 블록 `texture_2d_array`, sampler, material LUT.
* 청크 dynamic-offset uniform arena 한 벌.
* 한 월드 청크의 opaque와 translucent 메시는 같은 `ChunkSlot`을 공유한다.

```text
group 0: Globals
group 1: block texture array, sampler, material LUT
group 2: ChunkUniform dynamic offset
group 3: pass-specific lighting resources
group 4: pass-specific input textures
```

group 0~2의 의미는 geometry·shadow·forward에서 유지한다.

`ChunkUniform.origin.w`는 M7에서는 0이다. M8 LOD가 `lod_shift`로 사용한다.

#### 16.2.3 렌더 스케일

게임 기본값은 `0.75`, 범위는 `0.50..=1.00`이다. 스냅샷 기본값은 `1.00`이다.

```text
internal_width  = max(8, floor(output_width  * scale / 8) * 8)
internal_height = max(8, floor(output_height * scale / 8) * 8)
```

* G버퍼, HDR, SSAO, 블룸은 internal 크기를 기준으로 한다.
* 최종 LDR은 output 물리 크기다.
* 창 리사이즈나 scale 변경은 프레임 시작 시 타깃을 한 번 재생성한다.
* 0×0 surface 크기는 기존처럼 렌더를 건너뛴다.
* geometry를 surface에 직접 그리는 경로는 남기지 않는다.

업스케일은 separable이 아닌 16-tap Catmull–Rom bicubic이다.

```text
w0(t) = -0.5t + t² - 0.5t³
w1(t) = 1 - 2.5t² + 1.5t³
w2(t) = 0.5t + 2t² - 1.5t³
w3(t) = -0.5t² + 0.5t³
```

#### 16.2.4 G버퍼

| 타깃                        | 포맷               | 내용                                                 |
| ------------------------- | ---------------- | -------------------------------------------------- |
| `gbuffer_albedo`          | `Rgba8UnormSrgb` | 선형 albedo를 출력해 하드웨어 sRGB 인코딩, A=1                  |
| `gbuffer_normal_material` | `Rgba16Float`    | RG=oct world normal, B=roughness, A=material class |
| `gbuffer_light_ao`        | `Rgba8Unorm`     | R=block/15, G=sky/15, B=vertex AO/3, A=1           |
| `depth`                   | `Depth32Float`   | 표준 Z, near 0.05, far 1000                          |
| `hdr_scene`               | `Rgba16Float`    | 선형 HDR                                             |
| `ldr_scene`               | `Rgba8UnormSrgb` | 네이티브 해상도 최종 장면                                     |

oct normal:

```text
n = n / (|nx|+|ny|+|nz|)
p = n.xy
if n.z < 0:
    p = (1-abs(p.yx)) * sign(p.xy)
encoded = p*0.5+0.5
```

material class:

```text
0 = regular opaque
1 = alpha-cutout foliage
2 = emissive opaque
3 = reserved water
4 = reserved glass
```

재질 roughness:

| 재질     | roughness |
| ------ | --------: |
| STONE  |      0.90 |
| DIRT   |      0.95 |
| GRASS  |      0.90 |
| SAND   |      0.85 |
| LOG    |      0.80 |
| LEAVES |      0.90 |
| PLANKS |      0.75 |
| GLASS  |      0.04 |
| BRICK  |      0.85 |
| COBBLE |      0.95 |
| TORCH  |      0.55 |
| WATER  |      0.08 |

모든 재질 metallic은 0이다.

LEAVES는 M7에서 alpha-cutout으로 바꾼다.

* 절차적 leaves 텍스처의 알파 0 비율: 25~35%.
* fragment에서 `alpha < 0.5`면 discard.
* shadow 패스도 같은 alpha discard를 한다.
* `BlockDef.opaque=false`, `solid=true`, 잎-잎 면 유지 계약은 바꾸지 않는다.

#### 16.2.5 디퍼드 조명

G버퍼 픽셀에서 월드 위치를 `inv_view_proj`와 depth로 복원한다.

```text
ao_vertex = 0.35 + 0.65 * gbuffer_ao
ao_total = ao_vertex * mix(1.0, ssao, 0.75)

block_curve = LIGHT_LEVEL[round(block*15)]
sky_curve   = LIGHT_LEVEL[round(sky*15)] * sun_factor

ambient_sky   = sky_color * (0.10 + 0.35 * sky_curve)
ambient_block = vec3(1.00, 0.62, 0.35) * (0.10 + 0.55 * block_curve)
ambient       = max(ambient_sky, ambient_block)

L = -sun_dir
NdotL = max(dot(N,L),0)
direct = albedo * sun_color * 2.2 * NdotL * shadow * sky

emission = material.emission_rgb * material.emission_strength
hdr = albedo * ambient * ao_total + direct + emission
```

SSAO는 ambient에만 곱한다. direct와 emission에는 곱하지 않는다.

M6 face 상수 음영은 G버퍼 경로에서 제거한다. 법선·태양광이 그 역할을 맡는다.

#### 16.2.6 CSM

기법은 Practical Split Scheme(PSSM)이다.

* cascade 수: 3.
* shadow 거리: 192블록.
* 카메라 near: 0.05.
* λ: 0.65.

```text
log_i = n * (f/n)^(i/N)
lin_i = n + (f-n)*(i/N)
split_i = 0.65*log_i + 0.35*lin_i
```

고정 결과:

```text
cascade 0: 0.05 .. 22.9206
cascade 1: 22.9206 .. 52.7755
cascade 2: 52.7755 .. 192.0
```

섀도 텍스처:

```text
Depth32Float, 2048×2048×3, texture_depth_2d_array
```

각 cascade:

1. 분할 frustum의 8코너를 월드로 복원.
2. 중심과 bounding sphere 반경을 구한다.
3. light view에서 XY 직교 범위를 `[-r,+r]`.
4. texel 크기 `2r/2048`.
5. light-space 중심 XY를 texel 크기로 round해 안정화한다.
6. Z 범위는 분할 코너 최솟값−64부터 최댓값+64.
7. cascade별 frustum으로 청크를 CPU 컬링한다.

bias:

```text
raster constant bias = 2
raster slope scale   = 2.0
raster clamp         = 0.0
receiver depth bias  = 0.0008
normal offset        = 0.025 blocks
```

PCF:

* 3×3, 총 9 tap.
* comparison sampler.
* cascade 끝 10% 구간에서 다음 cascade와 선형 혼합.
* 160~192블록에서 shadow를 1로 fade.
* `sun_height <= 0.02`이면 shadow 패스를 생략하고 shadow=1. 달 그림자는 만들지 않는다.
* M6 바람 변위를 shadow와 G버퍼 양쪽에서 같은 식으로 계산한다.

#### 16.2.7 SSAO

기법은 Crytek형 view-space SSAO다.

* 해상도: internal의 1/2.
* kernel: 고정 16개 hemisphere sample.
* sample 반경: 1.25블록.
* depth bias: 0.025.
* intensity: 1.0.
* power: 1.5.
* 회전 노이즈: 결정적 4×4 `Rg8Snorm`.

커널 생성:

```text
u_i = Hammersley(i,16)
hemisphere direction
scale = mix(0.10,1.00,(i/15)^2)
```

raw AO 뒤 5-tap horizontal·vertical bilateral blur를 한다.

```text
spatial weights = [1,4,6,4,1] / 16
depth sigma      = 1.0 block
normal reject    = dot(Nc,Ni) < 0.80
```

AO 포맷은 `R8Unorm`이다.

#### 16.2.8 대기 산란 하늘

단순 단일 산란 Rayleigh–Mie 모델을 256×128 `Rgba16Float` equirectangular LUT에 계산한다.

단위는 km다. 월드 1블록은 1m로 본다.

| 상수                      |                               값 |
| ----------------------- | ------------------------------: |
| 지구 반경                   |                          6360km |
| 대기 반경                   |                          6460km |
| Rayleigh 높이척도           |                           8.0km |
| Mie 높이척도                |                           1.2km |
| βR                      | `(0.0058, 0.0135, 0.0331)` km⁻¹ |
| βM                      |                    `0.021` km⁻¹ |
| Mie g                   |                            0.76 |
| 태양 세기                   |                              20 |
| view ray steps          |                               8 |
| sun optical-depth steps |                               4 |
| 태양 각반경                  |                     0.00465 rad |
| 달 각반경                   |                     0.00436 rad |
| 달 radiance              |                            0.08 |

```text
rhoR(h) = exp(-h/8.0)
rhoM(h) = exp(-h/1.2)

phaseR(mu) = 3/(16π) * (1+mu²)
phaseM(mu) = (1-g²) /
             (4π * (1+g²-2gmu)^(3/2))
```

카메라 고도는 `max(cam_y,0) * 0.001km`다. LUT는 매 프레임 갱신한다. 256×128이므로 별도 temporal 처리는 하지 않는다.

태양은 `-sun_dir`, 달은 그 반대 방향이다. 지평선 아래 광선은 M6 야간색과 혼합한다. 별은 M7 범위가 아니다.

#### 16.2.9 블룸·자동 노출·ACES

블룸:

* HDR에 현재 exposure를 곱한 뒤 threshold.
* threshold `1.0`.
* soft knee `0.5`.
* 1/2, 1/4, 1/8, 1/16, 1/32의 5단계.
* 첫 downsample은 Karis average.
* 이후 13-tap downsample.
* upsample은 3×3 tent.
* 최종 bloom intensity `0.08`.

자동 노출은 histogram이 아니라 평균 log luminance를 쓴다.

```text
Y = max(dot(rgb, (0.2126,0.7152,0.0722)), 1e-4)
avg_log = mean(log2(Y))
target_exposure = clamp(0.18 / exp2(avg_log), 0.25, 8.0)
```

16×16 workgroup마다 partial sum과 count를 storage buffer에 쓴다. 256개 항목 단위로 반복 reduction해 하나가 될 때까지 줄인다. float atomic은 사용하지 않는다.

적응:

```text
speed = 1.5  if target > current  // 어두운 곳으로 이동
speed = 3.0  otherwise            // 밝은 곳으로 이동
a = 1 - exp(-speed*dt)
current = mix(current,target,a)
```

ACES는 Narkowicz fitted 식이다.

```text
x = max(hdr * exposure + bloom * 0.08, 0)
aces(x) = clamp(
    x * (2.51*x + 0.03) /
    (x * (2.43*x + 0.59) + 0.14),
    0, 1
)
```

스냅샷 기본은 고정 exposure 1.0이다. 자동 노출 검증 때만 `--auto-exposure on --warmup N`을 준다.

#### 16.2.10 TAA

M7에서는 TAA를 구현하지 않는다.

* motion vector G버퍼 없음.
* jitter는 `(0,0)`.
* history color 없음.
* M8·M9의 저해상도 temporal pass는 월드 위치 reprojection을 별도로 사용한다.
* TAA용 예약 필드는 Globals에 두지만 완료 조건에 포함하지 않는다.

### 16.3 Rust/WGSL 계약

M7 최종 `Globals`:

```rust
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Globals {
    pub view_proj: [[f32; 4]; 4],      // 0
    pub cam_pos: [f32; 4],             // 64
    pub sun_dir: [f32; 4],             // 80, w=sun_factor
    pub time_res: [f32; 4],            // 96, time,out_w,out_h,underwater
    pub sky_color: [f32; 4],           // 112, rgb, phase
    pub inv_view_proj: [[f32; 4]; 4],  // 128
    pub prev_view_proj: [[f32; 4]; 4], // 192
    pub prev_cam_pos: [f32; 4],        // 256
    pub sun_color: [f32; 4],           // 272, rgb, angular radius
    pub render_res: [f32; 4],          // 288, w,h,1/w,1/h
    pub camera_params: [f32; 4],       // 304, near,far,dt,frame_index
    pub jitter: [f32; 4],              // 320, cur.xy,prev.xy
}
```

`GLOBALS_SIZE = 336`.

```rust
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct MaterialGpu {
    pub params: [f32; 4],   // roughness, metallic, alpha_cutoff, emission_strength
    pub emission: [f32; 4], // linear rgb, reserved
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ShadowUniform {
    pub light_view_proj: [[[f32; 4]; 4]; 3],
    pub splits: [f32; 4],
    pub params: [f32; 4], // inv_resolution, normal_bias, receiver_bias, blend_fraction
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugView {
    Final,
    Albedo,
    Normal,
    Depth,
    Ao,
    Shadow,
    Light,
}

impl DebugView {
    pub const ORDER: [Self; 7];
    pub fn next(self) -> Self;
    pub fn parse(value: &str) -> anyhow::Result<Self>;
}

pub struct RenderConfig {
    pub output_size: UVec2,
    pub render_scale: f32,
    pub debug_view: DebugView,
    pub fixed_exposure: Option<f32>,
}

pub struct FrameInput<'a> {
    pub globals: &'a Globals,
    pub chunks: &'a [GpuChunkMeshes],
    pub selected_block: Option<IVec3>,
}

pub struct PassTimings {
    pub shadow_ms: f32,
    pub gbuffer_ms: f32,
    pub ssao_ms: f32,
    pub sky_ms: f32,
    pub deferred_ms: f32,
    pub translucent_ms: f32,
    pub bloom_ms: f32,
    pub exposure_ms: f32,
    pub tonemap_upscale_ms: f32,
    pub present_ms: f32,
    pub total_gpu_ms: f32,
}

pub struct RenderStats {
    pub drawn_chunks: usize,
    pub timings: Option<PassTimings>,
}

impl Renderer {
    pub fn resize(&mut self, config: &RenderConfig) -> anyhow::Result<()>;

    pub fn render_frame(
        &mut self,
        output_view: &wgpu::TextureView,
        config: &RenderConfig,
        frame: &FrameInput<'_>,
    ) -> RenderStats;
}
```

공유 청크 슬롯:

```rust
pub struct ChunkSlot {
    pub uniform_offset: u32,
    slot: u32,
}

pub struct GpuChunk {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

pub struct GpuChunkMeshes {
    pub opaque: Option<GpuChunk>,
    pub translucent: Option<GpuChunk>,
    pub origin: IVec3,
    pub slot: ChunkSlot,
}
```

M7 `snapshot` 옵션:

```text
--view albedo|normal|depth|ao|shadow|light|final
--render-scale 0.5..1.0
--fixed-exposure <f32>
--auto-exposure on|off
--warmup <u32>
--frames <u32>
--timings <json-path>
```

게임 F4는 다음 순서로 순환한다.

```text
final → albedo → normal → depth → ao → shadow → light → final
```

debug 인코딩:

* albedo: G버퍼 RGB.
* normal: `N*0.5+0.5`.
* depth: `clamp(linear_depth/192,0,1)`, sky는 1.
* ao: blurred AO 회색.
* shadow: 최종 cascade 혼합 shadow factor 회색.
* light: `max(block_curve, sky_curve*sun_factor)` 회색.
* final: ACES 결과.

wgpu 30 API 중 §11에 없는 다음 항목은 구현 전에 반드시 `~/.cargo/registry/src/index.crates.io-*/wgpu-30.0.1/src/api/`와 `wgpu-types-30.0.1/src/`에서 실제 이름·필드를 확인한다.

* timestamp query·query resolve.
* texture format feature 조회.
* `TextureViewDescriptor` array layer 필드.
* comparison sampler·depth array binding.
* storage buffer 최소 크기.
* texture copy.
* pipeline cache 관련 필드.

구버전 타입 이름을 추측해 쓰지 않는다.

### 16.4 상수

| 항목                  |                     값 |
| ------------------- | --------------------: |
| 게임 기본 render scale  |                  0.75 |
| 스냅샷 기본 render scale |                  1.00 |
| render scale 범위     |             0.50~1.00 |
| internal 정렬         |                   8픽셀 |
| HDR                 |         `Rgba16Float` |
| depth               |        `Depth32Float` |
| CSM 수               |                     3 |
| CSM 기본 해상도          |                 2048² |
| CSM 거리              |                   192 |
| PSSM λ              |                  0.65 |
| PCF                 |                   3×3 |
| cascade blend       |               마지막 10% |
| SSAO 해상도            |          internal 1/2 |
| SSAO 샘플             |                    16 |
| SSAO 반경             |                  1.25 |
| SSAO bias           |                 0.025 |
| SSAO blur           |         5×5 separable |
| sky LUT             | 256×128 `Rgba16Float` |
| 대기 view step        |                     8 |
| 대기 sun step         |                     4 |
| bloom 단계            |                     5 |
| bloom threshold     |                   1.0 |
| bloom knee          |                   0.5 |
| bloom intensity     |                  0.08 |
| exposure 범위         |              0.25~8.0 |
| exposure 기준 회색      |                  0.18 |
| TAA                 |                   미구현 |

### 16.5 테스트 계약

```text
render_scale_dimensions_align_to_eight
scene_targets_use_contract_formats
shared_scene_bindings_allocate_one_chunk_slot
debug_view_cycle_order_is_stable
pssm_splits_match_contract
shadow_texel_snapping_is_stable_under_subtexel_motion
csm_selects_expected_cascade
ssao_kernel_is_deterministic_and_hemispherical
ssao_bilateral_blur_preserves_depth_edge
oct_normal_roundtrip_error_below_threshold
gbuffer_material_encoding_roundtrip
aces_fit_matches_reference_values
bloom_chain_has_five_levels
exposure_reduction_matches_cpu_log_average
exposure_adaptation_is_frame_rate_independent
sky_lut_is_finite_at_horizon
translucent_pipeline_shares_globals_and_chunk_arena
shader_reload_is_transactional_across_changed_pipeline
debug_depth_encodes_linear_distance
```

판정 기준:

* oct normal encode/decode 최대 각도 오차 `< 0.25°`.
* split 오차 `< 1e-3`.
* 카메라가 cascade texel의 0.49배 움직일 때 snapped matrix가 bitwise 동일.
* ACES 입력 0, 0.18, 1, 4에 대한 CPU 참조와 오차 `< 1e-5`.
* exposure를 30fps와 120fps로 같은 2초 동안 적응시킨 결과 상대 오차 `< 0.5%`.
* 모든 sky LUT 값 finite, 음수 없음.
* translucent와 opaque가 같은 Globals buffer ID와 chunk arena ID를 사용.
* shader 하나가 실패하면 관련 새 파이프라인 묶음을 전부 버리고 이전 묶음을 유지.

### 16.6 스냅샷 검증

`m7-passes` fixture 계약:

* `(320,180)`은 카메라를 향한 +Z 평면, 카메라 거리 24.
* `(420,180)`은 열린 면.
* `(245,220)`은 직각 안쪽 모서리.
* `(180,235)`는 기둥 그림자.
* `(460,235)`는 같은 재질의 직사광 영역.

```bash
for v in albedo normal depth ao shadow light final; do
  cargo run --release --bin snapshot -- \
    --fixture m7-passes --size 640x360 --render-scale 1.0 \
    --day-phase 0.0 --fixed-exposure 1.0 \
    --view "$v" --out "/tmp/vf_m7_${v}.png"
done

python3 - <<'PY'
from PIL import Image
def lin(v):
    v /= 255.0
    return v/12.92 if v <= .04045 else ((v+.055)/1.055)**2.4
def rgb(path,x,y):
    p=Image.open(path).convert("RGB").getpixel((x,y))
    return tuple(lin(v) for v in p)
n=rgb("/tmp/vf_m7_normal.png",320,180)
d=rgb("/tmp/vf_m7_depth.png",320,180)[0]
ao_open=rgb("/tmp/vf_m7_ao.png",420,180)[0]
ao_corner=rgb("/tmp/vf_m7_ao.png",245,220)[0]
shadow=rgb("/tmp/vf_m7_shadow.png",180,235)[0]
lit=rgb("/tmp/vf_m7_shadow.png",460,235)[0]
print(n,d,ao_open,ao_corner,shadow,lit)
for got,want in zip(n,(0.5,0.5,1.0)):
    assert abs(got-want) <= 0.035
assert abs(d-(24.0/192.0)) <= 0.015
assert ao_open >= 0.90
assert ao_corner <= 0.65
assert shadow <= 0.35
assert lit >= 0.90
PY
```

블룸·ACES:

```bash
cargo run --release --bin snapshot -- \
  --fixture m7-bloom --size 640x360 --render-scale 1.0 \
  --fixed-exposure 1.0 --view final --out /tmp/vf_m7_bloom.png

python3 - <<'PY'
from PIL import Image
im=Image.open("/tmp/vf_m7_bloom.png").convert("RGB")
def mean(box):
    vals=[]
    for y in range(box[1],box[3]):
        for x in range(box[0],box[2]):
            r,g,b=im.getpixel((x,y))
            vals.append((r+g+b)/3)
    return sum(vals)/len(vals)
core=mean((300,160,340,200))
halo=mean((270,130,370,230))
far=mean((80,80,160,160))
print(core,halo,far)
assert core > halo > far
assert halo >= far*1.20
assert max(max(px) for px in im.getdata()) <= 255
PY
```

자동 노출:

```bash
cargo run --release --bin snapshot -- \
  --fixture m7-exposure-dark --size 640x360 \
  --auto-exposure on --warmup 180 --frames 1 \
  --out /tmp/vf_m7_exposure_dark.png
cargo run --release --bin snapshot -- \
  --fixture m7-exposure-bright --size 640x360 \
  --auto-exposure on --warmup 180 --frames 1 \
  --out /tmp/vf_m7_exposure_bright.png
```

두 fixture의 중앙 회색 패치 최종 선형 밝기는 각각 `0.12..0.24`여야 한다.

렌더 스케일:

```bash
cargo run --release --bin snapshot -- \
  --fixture m7-passes --size 1280x720 --render-scale 0.5 \
  --fixed-exposure 1.0 --out /tmp/vf_m7_scale05.png
python3 - <<'PY'
from PIL import Image
im=Image.open("/tmp/vf_m7_scale05.png")
assert im.size == (1280,720)
rgb=im.convert("RGB")
border=[]
for x in range(im.width):
    border += [rgb.getpixel((x,0)),rgb.getpixel((x,im.height-1))]
for y in range(im.height):
    border += [rgb.getpixel((0,y)),rgb.getpixel((im.width-1,y))]
assert sum(max(p) == 0 for p in border)/len(border) < 0.05
PY
```

### 16.7 성능 예산

Apple M5, 2560×1440, render scale 0.75, R=12, F4 final 기준이다.

| 패스                           | GPU p95 예산 |
| ---------------------------- | ---------: |
| CSM 3개                       |      1.8ms |
| G버퍼                          |      1.6ms |
| SSAO+blur                    |      0.9ms |
| sky LUT                      |      0.2ms |
| deferred                     |      1.1ms |
| forward translucent+outline  |      0.7ms |
| bloom                        |      0.8ms |
| exposure                     |      0.3ms |
| ACES+upscale                 |      0.8ms |
| present                      |      0.2ms |
| GPU 합계                       |    ≤ 8.4ms |
| CPU cull·encode·stream apply |    ≤ 3.5ms |
| 전체 프레임 p95                   |   ≤ 13.0ms |
| 전체 프레임 max                   |     < 25ms |

GPU timestamp는 지원될 때만 게임에서 켠다. 미지원이면 게임은 정상 실행하되 timings를 `None`으로 둔다. Apple M5 검증에서는 timestamp가 필수이며 `snapshot --timings`가 JSON을 만든다.

JSON 키:

```text
shadow
gbuffer
ssao
sky
deferred
translucent
bloom
exposure
tonemap_upscale
present
total_gpu
```

각 키는 `{ "median_ms": f32, "p95_ms": f32 }`다.

메시 arena 재검토 게이트:

```text
VF_AUTOPILOT=stream VF_BENCH_FRAMES=3600
```

첫 600프레임을 버린다.

* p99 frame ≤20ms 또는 p99 upload CPU ≤2ms이면 arena를 구현하지 않는다.
* p99 frame >20ms이고, 가장 느린 10프레임 중 5프레임 이상에서 upload CPU가 4ms를 넘으면 §14.10 arena를 구현한다.
* arena 구현 시 VB 96MiB, IB 72MiB, 16바이트 정렬, 인접 free 병합, 가득 차면 청크 전용 버퍼 fallback.
* 결과와 선택을 LOG에 숫자로 기록한다.

### 16.8 M7 분기 선결정 — 묻지 말고 이렇게

* 렌더 경로 → 오프스크린 G버퍼/HDR/LDR, surface는 최종 블릿만.
* 기본 scale → 게임 0.75, snapshot 1.0.
* G버퍼 → albedo `Rgba8UnormSrgb`, normal/material `Rgba16Float`, light/AO `Rgba8Unorm`, depth `Depth32Float`.
* deferred → 위 식.
* 그림자 → 3 cascade, PSSM λ=0.65, 2048², 3×3 PCF.
* SSAO → 16샘플, half-resolution, 5×5 bilateral.
* 하늘 → 256×128 단일 산란 Rayleigh–Mie LUT.
* 블룸 → 5단계.
* 노출 → 평균 log luminance. histogram 아님.
* 톤매핑 → Narkowicz ACES fitted.
* 업스케일 → 16-tap Catmull–Rom.
* TAA → 구현하지 않음.
* 물·유리 → forward transparent 유지.
* alpha-cutout → LEAVES만 M7에서 적용.
* Globals·청크 uniform → opaque/translucent가 공유.
* VB/IB arena → 측정 게이트가 참일 때만.
* F4 순서 → `final, albedo, normal, depth, ao, shadow, light`.
* M7가 끝나면 LOG 기록 후 멈추고 Claude 리뷰를 받는다.

---

## 17. M8 상세 설계 — 물·볼류메트릭·구름·원거리 LOD

### 17.1 새 모듈

| 경로                               | 역할                                                  | 마일스톤      |
| -------------------------------- | --------------------------------------------------- | --------- |
| `src/mesh/water.rs`              | WATER 전용 메시 분리, 파도용 쿼드 최대 크기                        | M8.1      |
| `src/render/water.rs`            | Gerstner, SSR, 굴절, Beer–Lambert, 코스틱                | M8.1~M8.2 |
| `src/render/volumetric.rs`       | 1/4해상도 fog/light raymarch·temporal·upsample         | M8.3      |
| `src/render/clouds.rs`           | Perlin–Worley 볼류메트릭 구름                              | M8.4      |
| `src/render/noise.rs`            | periodic Perlin·Worley, 64² blue-noise rank texture | M8.3~M8.4 |
| `src/lod/grid.rs`                | 32³ 계층 LOD grid와 최빈 블록 downsample                   | M8.5      |
| `src/lod/light.rs`               | coarse-cell 하늘광·블록광                                 | M8.5      |
| `src/lod/streamer.rs`            | LOD1~3 생성·캐시·invalidations·메시 잡                     | M8.5      |
| `src/mesh/lod.rs`                | coarse grid greedy와 transition skirt                | M8.5      |
| `assets/shaders/water.wgsl`      | 파도·SSR·굴절·코스틱                                       | M8.1~M8.2 |
| `assets/shaders/volumetric.wgsl` | fog/light raymarch·temporal·upsample                | M8.3      |
| `assets/shaders/clouds.wgsl`     | 구름 raymarch·light march·temporal                    | M8.4      |

### 17.2 알고리즘·수식

#### 17.2.1 WATER 메시 분리

`ChunkMeshes`와 `GpuChunkMeshes`에 `water`를 추가한다.

* opaque: 기존 불투명·LEAVES.
* translucent: GLASS.
* water: WATER만.
* WATER-WATER 내부 면 제거는 유지.
* lowered 비트 23은 유지.
* lowered 정점에만 Gerstner 변위를 적용한다.
* +Y와 옆면 위쪽 쿼드는 폭·높이 최대 2셀.
* 물의 −Y 등 lowered가 없는 면은 최대 8셀.
* GLASS greedy 제한은 없다.

#### 17.2.2 Gerstner 파도

네 파동을 합한다.

```text
k = 2π / wavelength
omega = 2π * speed / wavelength
phase = k * dot(direction, world_xz) - omega*time

horizontal += Q * amplitude * direction * cos(phase)
vertical   += amplitude * sin(phase)
```

|  i | direction               | amplitude | wavelength | speed |    Q |
| -: | ----------------------- | --------: | ---------: | ----: | ---: |
|  0 | normalize `(1.0,0.2)`   |     0.075 |       18.0 |  1.20 | 0.35 |
|  1 | normalize `(-0.6,0.8)`  |     0.040 |        9.0 |  1.00 | 0.30 |
|  2 | normalize `(0.3,-1.0)`  |     0.022 |        4.5 |  0.80 | 0.22 |
|  3 | normalize `(-0.8,-0.3)` |     0.012 |       2.25 |  0.60 | 0.15 |

최대 이론 Y 변위는 0.149블록이다. normal은 Gerstner 편미분의 cross product로 계산하고 normalize한다.

#### 17.2.3 물 렌더 순서

deferred와 구름 합성이 끝난 HDR를 `scene_color_copy: Rgba16Float`로 복사한다.

transparent draw item을 청크 중심 거리 기준 뒤→앞으로 하나의 목록에 넣는다.

```text
Glass item → alpha blend pipeline
Water item → water replacement pipeline
```

파이프라인은 item마다 전환한다. WATER는 HDR 배경을 직접 굴절·반사해 최종 물색을 계산하므로 blend는 `None`, depth write는 off, depth compare는 `LessEqual`, cull은 `None`이다.

#### 17.2.4 SSR

view/world 위치, Gerstner normal, 카메라 벡터로 반사 광선을 만든다.

* balanced step: 48.
* binary refinement: 5.
* 최대 거리: 48블록.
* 최소 시작 offset: normal 방향 0.05블록.
* 두께: `0.18 + 0.002 * linear_depth`.
* 뒤를 향하는 광선, 화면 밖, depth 없음은 miss.
* hit 후 scene color를 샘플한다.
* miss는 sky LUT 반사로 대체한다.

fade:

```text
edge = smoothstep(0.0,0.08,min(uv.x,uv.y,1-uv.x,1-uv.y))
facing = saturate(1-dot(-V,N))
distance = 1-smoothstep(0.70,1.00,t/max_distance)
confidence = edge * facing * distance
```

SSR color와 sky fallback을 confidence로 혼합한다.

#### 17.2.5 굴절·Fresnel·흡수

```text
offset = normal_view.xy
       * 0.015
       * min(1.0, 4.0/max(water_depth,1.0))
refract_uv = screen_uv + offset
```

굴절 UV의 opaque depth가 물 표면보다 0.1블록 이상 앞이면 offset을 버리고 원래 UV를 쓴다.

Fresnel Schlick:

```text
F0 = 0.02
F = F0 + (1-F0)*(1-max(dot(N,V),0))^5
```

물 두께:

```text
thickness = max(scene_linear_depth-water_linear_depth,0)
```

Beer–Lambert:

```text
absorption = (0.150, 0.055, 0.025) per block
T = exp(-absorption * thickness)
scattering_color = (0.020, 0.160, 0.220)
refracted = scene*T + scattering_color*(1-T)
water = mix(refracted, reflection, F)
```

#### 17.2.6 화면 공간 코스틱

굴절로 얻은 배경 월드 위치에 두 개의 움직이는 주기 함수를 계산한다.

```text
a = sin(dot(p.xz,(1.7,1.1)) + time*1.6)
b = sin(dot(p.xz,(-1.3,1.9)) - time*1.2)
c = pow(saturate(1-abs(a+b)*0.5), 6)
caustic = 1 + 0.18*c*exp(-0.12*thickness)
```

`refracted *= caustic`. 물 밖 픽셀에는 적용하지 않는다.

#### 17.2.7 볼류메트릭 light/fog

internal 해상도의 1/4에서 raymarch한다.

```text
distance_i = near * (far/near)^(i/(steps-1))
```

* balanced view step 40.
* 최대 거리 192블록.
* blue-noise로 첫 step 위치를 `0..1 step` jitter.
* 밀도:

```text
height = world_y - SEA_LEVEL
density = 0.0015 + 0.0060 * exp(-max(height,0)*0.025)
```

Henyey–Greenstein:

```text
g = 0.65
phase(mu) = (1-g²)/(4π*(1+g²-2gmu)^(3/2))
```

각 step:

```text
Tr = exp(-density*ds)
Li += T * sun_color * phase * shadow * density * ds
T *= Tr
```

`sun_height <= 0.02`이면 sun scattering은 0이다.

출력:

```text
Rgba16Float: rgb = in-scattering, a = transmittance
```

temporal:

* history weight 0.90.
* 현재 월드 위치를 `prev_view_proj`로 투영.
* 이전 UV 화면 밖이면 reject.
* 상대 depth 차이 >2% 또는 absolute 차이 >0.5블록이면 reject.
* normal dot <0.90이면 reject.
* resize, render scale 변경, 카메라 순간이동 >8블록, yaw 변화 >30°면 전체 history reset.

upsample은 depth·normal bilateral 4-tap이다.

#### 17.2.8 Blue-noise

외부 PNG를 요구하지 않는다. 64×64 rank 텍스처를 시작 시 결정적으로 생성한다.

1. 4096칸 energy를 0으로 초기화.
2. 첫 픽셀은 `0x9e3779b9 % 4096`.
3. 다음 rank마다 미선택 픽셀 중 Gaussian energy가 가장 작은 픽셀 선택.
4. 동률은 `xorshift32(index ^ 0x68bc21eb)`가 작은 순.
5. 선택점 주변 toroidal 반경 6에 다음 kernel을 더한다.

```text
K(dx,dy) = exp(-(dx²+dy²)/(2*1.9²))
```

6. 값은 `round(255*rank/4095)`.
7. sampler는 Repeat·Nearest.
8. 프레임별 offset은 Halton(2,3)의 `frame_index % 64`.

#### 17.2.9 볼류메트릭 구름

고도:

```text
base = 180 blocks
top  = 260 blocks
```

주기적 custom Perlin과 Worley를 CPU에서 생성한다.

* base noise: 128³ `R8Unorm`.
* detail noise: 32³ `R8Unorm`.
* texture address: Repeat.
* periodic lattice hash를 사용해 반대 면 값이 일치해야 한다.

```text
base_shape =
    0.65 * perlin_fbm_4_octaves
  + 0.35 * (1-worley_f1)

detail_shape =
    0.50 * perlin_fbm_3_octaves
  + 0.50 * (1-worley_f1)

height_shape =
    smoothstep(0.0,0.15,h)
  * (1-smoothstep(0.70,1.0,h))

density =
    saturate(
        (base_shape - (1-coverage)*0.60 - detail_shape*0.25) * 2
    ) * height_shape
```

* coverage: 0.52.
* wind: `(0.012, 0, 0.007)` blocks/s.
* view step: 48.
* light step: 6.
* 최대 거리: 512블록.
* 해상도: internal 1/4.
* temporal history weight: 0.95.
* 구름 ray는 opaque scene depth에서 멈춘다.
* 지형 cloud shadow map은 만들지 않는다.

#### 17.2.10 LOD 계층

각 LOD grid는 항상 32³ 셀이다.

|   레벨 | 셀 크기 | grid 월드 크기 | 수직 grid 수 |
| ---: | ---: | ---------: | --------: |
| LOD0 |    1 |        32³ |         8 |
| LOD1 |    2 |        64³ |         4 |
| LOD2 |    4 |       128³ |         2 |
| LOD3 |    8 |       256³ |         1 |

LOD1은 2×2×2 base Chunk를, LOD2는 2×2×2 LOD1을, LOD3는 2×2×2 LOD2를 재귀 downsample한다.

각 부모 셀은 자식 8셀의 최빈 ID다. 동률:

1. non-AIR 우선.
2. opaque 우선.
3. 더 작은 `BlockId`.

WATER는 엄격한 최다일 때만 opaque 블록을 이긴다.

LOD grid는 `blocks: u16[32768]`와 `light: u8[32768]`를 가진다. coarse 조명은 M6 규칙을 셀 단위로 적용하되 이동 감쇠가 셀 크기다.

```text
block candidate = current.saturating_sub(cell_size)
sky horizontal/up candidate = current.saturating_sub(cell_size)
sky downward candidate = current
```

LOD 거리 `R = full-detail radius`:

```text
LOD0: 0 .. 32R
LOD1: 32(R-1) .. 64R
LOD2: 64R-32 .. 96R
LOD3: 96R-32 .. 128R
```

기본 R=10:

```text
LOD0: 0..320
LOD1: 288..640
LOD2: 608..960
LOD3: 928..1280
```

인접 링은 32블록 겹친다. 겹침 구간에서 8×8 screen-space Bayer discard로 crossfade한다. coarse와 fine의 alpha 합은 1이다.

`ChunkUniform.origin.w = lod_shift`:

```text
0,1,2,3
world_pos = origin.xyz + local * float(1 << lod_shift)
```

수준이 다른 경계의 coarse top contour에는 아래로 `2*cell_size` 내려가는 skirt를 만든다. skirt는 같은 블록 텍스처를 쓴다.

LOD source 우선순위:

1. 현재 로딩된 base Chunk.
2. 수정 청크 저장 파일.
3. `WorldGen::generate`.

base Chunk 편집은 해당 위치를 포함하는 LOD1 부모와 모든 LOD2·LOD3 조상을 invalidation한다.

캐시 상한:

| 대상                 |     상한 |
| ------------------ | -----: |
| LOD1 grid          |    768 |
| LOD2 grid          |    256 |
| LOD3 grid          |     96 |
| CPU LOD grid 총 메모리 | 128MiB |
| GPU LOD mesh 총 메모리 | 128MiB |

상한에 닿으면 카메라에서 가장 먼 grid부터 제거한다. 현재 crossfade에 필요한 grid는 제거하지 않는다.

### 17.3 Rust/WGSL 계약

```rust
// mesh/mesher.rs
pub struct ChunkMeshes {
    pub opaque: ChunkMesh,
    pub translucent: ChunkMesh, // GLASS
    pub water: ChunkMesh,       // WATER
}

// render/chunk_pipeline.rs
pub struct GpuChunkMeshes {
    pub opaque: Option<GpuChunk>,
    pub translucent: Option<GpuChunk>,
    pub water: Option<GpuChunk>,
    pub origin: IVec3,
    pub slot: ChunkSlot,
}

// render/water.rs
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GerstnerWave {
    pub dir_amp_lambda: [f32; 4], // dir.x,dir.z,amplitude,wavelength
    pub speed_q_pad: [f32; 4],    // speed,Q,0,0
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct WaterUniform {
    pub waves: [GerstnerWave; 4],
    pub absorption: [f32; 4],
    pub scattering: [f32; 4],
    pub params0: [f32; 4], // refraction,F0,max_ssr_distance,thickness_base
    pub params1: [f32; 4], // ssr_steps,binary_steps,caustic_strength,time
}

// render/volumetric.rs
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct VolumetricUniform {
    pub params0: [f32; 4], // base_density,height_density,max_distance,g
    pub params1: [f32; 4], // steps,history_weight,sea_level,frame_index
}

// render/clouds.rs
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct CloudUniform {
    pub layer: [f32; 4],   // base,top,coverage,max_distance
    pub march: [f32; 4],   // view_steps,light_steps,history_weight,time
    pub wind: [f32; 4],
}

// lod/grid.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LodKey {
    pub level: u8,
    pub coord: IVec3,
}

pub struct LodGrid {
    pub key: LodKey,
    pub blocks: Box<[BlockId; 32768]>,
    pub light: Box<[u8; 32768]>,
}

pub fn downsample_base(
    key: LodKey,
    children: [&Chunk; 8],
) -> LodGrid;

pub fn downsample_lod(
    key: LodKey,
    children: [&LodGrid; 8],
) -> LodGrid;

pub fn modal_block(ids: [BlockId; 8]) -> BlockId;
```

M8 debug view 추가:

```text
water
volumetric
cloud
lod
```

F4 순서 뒤에 추가한다.

```text
final → albedo → normal → depth → ao → shadow → light
→ water → volumetric → cloud → lod → final
```

debug 인코딩:

* water: R=SSR confidence, G=`min(thickness/16,1)`, B=Fresnel.
* volumetric: RGB=in-scattering tone 축소, A는 별도 회색 transmittance로 혼합.
* cloud: RGB=cloud radiance, A=density integral.
* lod: LOD0 회색, LOD1 초록, LOD2 주황, LOD3 자홍으로 고정 표시.

wgpu 30의 3D 텍스처, texture copy, storage texture, multisampled 여부, `copy_texture_to_texture` 실제 API는 레지스트리 소스에서 확인한다.

### 17.4 상수

| 항목                       |              값 |
| ------------------------ | -------------: |
| Gerstner 파동 수            |              4 |
| 최대 Y 진폭                  |          0.149 |
| water surface greedy     |         최대 2×2 |
| SSR balanced step        |             48 |
| SSR binary step          |              5 |
| SSR max distance         |             48 |
| SSR thickness base       |           0.18 |
| refraction strength      |          0.015 |
| water F0                 |           0.02 |
| caustic strength         |           0.18 |
| volumetric 해상도           |   internal 1/4 |
| volumetric balanced step |             40 |
| volumetric max 거리        |            192 |
| volumetric history       |           0.90 |
| fog base density         |         0.0015 |
| fog height density       |         0.0060 |
| fog height falloff       |          0.025 |
| HG g                     |           0.65 |
| blue-noise               |            64² |
| cloud base/top           |        180/260 |
| cloud coverage           |           0.52 |
| cloud 해상도                |   internal 1/4 |
| cloud view/light step    |           48/6 |
| cloud history            |           0.95 |
| cloud max 거리             |            512 |
| LOD cell size            |          2/4/8 |
| LOD ring overlap         |           32블록 |
| LOD skirt                | 2 coarse cells |

### 17.5 테스트 계약

```text
gerstner_displacement_is_periodic_and_bounded
gerstner_normal_is_unit_length
water_mesh_caps_surface_quad_span
water_side_top_matches_surface_displacement
ssr_rejects_out_of_bounds_and_backfaces
ssr_binary_refinement_reduces_depth_error
refraction_rejects_foreground_sample
beer_lambert_transmittance_decreases_with_depth
caustic_intensity_is_bounded
volumetric_integrator_preserves_zero_density
henyey_greenstein_is_finite
blue_noise_rank_texture_is_deterministic_permutation
volumetric_history_rejects_disocclusion
cloud_density_is_zero_outside_layer
cloud_noise_is_periodic
lod_mode_tie_break_is_stable
lod_recursive_downsample_is_deterministic
lod_parent_invalidation_reaches_all_levels
lod_ring_selection_has_exact_overlap
lod_chunk_uniform_scale_preserves_world_position
lod_cache_respects_memory_caps
```

판정 기준:

* Gerstner Y 절댓값 ≤0.149001.
* analytic normal 길이 오차 `<1e-4`.
* water top quad 한 변 ≤2.
* SSR binary refine 후 depth 오차가 refine 전보다 작거나 같음.
* 굴절 foreground reject 시 원 UV와 bitwise 동일.
* 두께가 증가할 때 RGB transmittance가 모두 단조 감소.
* blue-noise 값이 rank 0..4095의 순열이며 같은 seed에서 동일.
* periodic noise의 반대 면 최대 차이 `<1e-6`.
* LOD 월드 위치가 level 0~3에서 오차 `<1e-5`.
* 캐시가 hard cap을 한 항목도 넘지 않음.

### 17.6 스냅샷 검증

물:

```bash
cargo run --release --bin snapshot -- \
  --fixture m8-water --size 640x360 --world-time 0.0 \
  --view final --fixed-exposure 1.0 --out /tmp/vf_m8_water_a.png
cargo run --release --bin snapshot -- \
  --fixture m8-water --size 640x360 --world-time 1.0 \
  --view final --fixed-exposure 1.0 --out /tmp/vf_m8_water_b.png
cargo run --release --bin snapshot -- \
  --fixture m8-water --size 640x360 --world-time 0.0 \
  --view water --out /tmp/vf_m8_water_debug.png

python3 - <<'PY'
from PIL import Image, ImageChops
a=Image.open("/tmp/vf_m8_water_a.png").convert("RGB")
b=Image.open("/tmp/vf_m8_water_b.png").convert("RGB")
d=ImageChops.difference(a,b).convert("L")
water=(100,120,540,330)
static=(10,40,100,150)
def frac(box):
    c=d.crop(box)
    return sum(v>4 for v in c.getdata())/(c.width*c.height)
wf=frac(water); sf=frac(static)
print(wf,sf)
assert 0.01 <= wf <= 0.35
assert sf <= 0.002
PY
```

볼류메트릭:

```bash
cargo run --release --bin snapshot -- \
  --fixture m8-godrays --size 640x360 --day-phase 0.0 \
  --warmup 32 --view final --fixed-exposure 1.0 \
  --out /tmp/vf_m8_godrays.png

python3 - <<'PY'
from PIL import Image
im=Image.open("/tmp/vf_m8_godrays.png").convert("RGB")
def mean(box):
    s=0
    n=0
    for y in range(box[1],box[3]):
        for x in range(box[0],box[2]):
            r,g,b=im.getpixel((x,y))
            s += .2126*r+.7152*g+.0722*b
            n += 1
    return s/n
beam=mean((270,80,370,290))
shadow=mean((410,80,510,290))
print(beam,shadow,beam/shadow)
assert beam/shadow >= 1.50
PY
```

구름:

```bash
cargo run --release --bin snapshot -- \
  --fixture m8-clouds --size 640x360 --warmup 32 \
  --view cloud --out /tmp/vf_m8_cloud.png

python3 - <<'PY'
from PIL import Image
im=Image.open("/tmp/vf_m8_cloud.png").convert("RGB")
covered=sum(max(p)>20 for p in im.getdata())/(im.width*im.height)
print(covered)
assert 0.20 <= covered <= 0.75
PY
```

LOD:

```bash
cargo run --release --bin snapshot -- \
  --fixture m8-lod --size 1280x720 --view lod \
  --out /tmp/vf_m8_lod_debug.png
cargo run --release --bin snapshot -- \
  --fixture m8-lod --size 1280x720 --view final \
  --lod on --out /tmp/vf_m8_lod_on.png
cargo run --release --bin snapshot -- \
  --fixture m8-lod --size 1280x720 --view final \
  --lod off --out /tmp/vf_m8_lod_off.png
```

* 화면 중심 40%의 LOD on/off 픽셀 일치율은 임계 4 기준 99.0% 이상.
* LOD debug에는 0·1·2·3 색이 모두 100픽셀 이상 존재.
* LOD ring 경계 주변 5픽셀 띠의 평균 선형 밝기 점프는 0.08 이하.
* 카메라를 경계 앞뒤 1블록 이동한 두 final 이미지의 변경 픽셀은 전체의 12% 이하.

### 17.7 성능 예산

M7 예산에 추가되는 값이다. M5 Air 기본은 render scale 0.75, R=10이다.

| 패스                               | GPU p95 예산 |
| -------------------------------- | ---------: |
| WATER+SSR+굴절                     |      1.3ms |
| cloud raymarch+temporal          |      1.3ms |
| volumetric fog+temporal+upsample |      1.2ms |
| LOD G버퍼 추가 draw                  |      0.6ms |
| copy·transparent 전환·composite    |      0.4ms |
| M8 추가 합계                         |    ≤ 4.8ms |
| M7+M8 전체 GPU                     |   ≤ 13.2ms |
| 전체 프레임 p95                       |   ≤ 16.0ms |
| 전체 프레임 max                       |     < 25ms |

CPU:

* blue/cloud noise 시작 생성 합계 <250ms.
* LOD worker는 기존 8워커 풀 공유.
* LOD 생성·메시는 full-detail Gen/Light/Mesh보다 우선하지 않는다.
* 프레임당 LOD 결과 복사·업로드는 기존 메인 6ms 예산 안.
* CPU LOD cache ≤128MiB, GPU LOD mesh ≤128MiB.

### 17.8 M8 분기 선결정 — 묻지 말고 이렇게

* 물 파도 → 네 Gerstner 합.
* 물 top tessellation → greedy 최대 2×2.
* SSR → 48 step + 5 binary, miss는 sky LUT.
* 굴절 → 화면 공간 depth-aware offset.
* 흡수 → Beer–Lambert.
* 코스틱 → 물 fragment의 screen-space procedural projection.
* 물 렌더 → 전용 forward replacement, depth write off.
* 볼류메트릭 → 1/4해상도 40-step raymarch.
* blue-noise → 외부 파일이 아닌 결정적 64² rank texture.
* 구름 → periodic Perlin–Worley 3D 텍스처, 180~260블록.
* cloud terrain shadow → 하지 않음.
* LOD → 재귀 2× mode downsample, 2×/4×/8×.
* LOD seam → 32블록 dither overlap + coarse skirt.
* 원거리 수정 청크 → 메모리 → 저장 파일 → WorldGen 순으로 반영.
* M8 뒤에는 멈추지 않고 M9로 간다.

---

## 18. M9 상세 설계 — 컴퓨트 DDA 복셀 GI

M9 GI는 하드웨어 레이트레이싱을 사용하지 않는다. Metal acceleration structure, ray query, `wgpu-hal` Metal 인터롭을 사용하지 않는다. 모든 GI 광선은 WGSL compute shader의 Amanatides–Woo 3D DDA로 추적한다.

### 18.1 새 모듈

| 경로                                 | 역할                                     | 마일스톤 |
| ---------------------------------- | -------------------------------------- | ---- |
| `src/render/gi/clipmap.rs`         | 4레벨 128³ clipmap, 토로이달 좌표·슬랩 갱신        | M9.1 |
| `src/render/gi/voxelize.rs`        | World·LOD → material/light voxel bytes | M9.1 |
| `src/render/gi/trace.rs`           | cosine hemisphere ray 설정·DDA dispatch  | M9.2 |
| `src/render/gi/temporal.rs`        | 월드 위치 reprojection·history clamp       | M9.3 |
| `src/render/gi/denoise.rs`         | 3단계 à-trous                            | M9.3 |
| `src/stream/clipmap.rs`            | clipmap CPU gather 잡·업로드 큐             | M9.1 |
| `assets/shaders/gi_trace.wgsl`     | compute DDA 간접광                        | M9.2 |
| `assets/shaders/gi_temporal.wgsl`  | reprojection·depth/normal reject       | M9.3 |
| `assets/shaders/gi_denoise.wgsl`   | edge-aware à-trous                     | M9.3 |
| `assets/shaders/gi_composite.wgsl` | deferred HDR에 GI 합성                    | M9.4 |

### 18.2 알고리즘·수식

#### 18.2.1 Clipmap 구성

| 레벨 |  해상도 | voxel 크기 | 월드 범위 |
| -: | ---: | -------: | ----: |
|  0 | 128³ |      1블록 |  128³ |
|  1 | 128³ |      2블록 |  256³ |
|  2 | 128³ |      4블록 |  512³ |
|  3 | 128³ |      8블록 | 1024³ |

레벨마다 두 텍스처를 둔다.

```text
material: Rgba8Unorm 3D
  RGB = 선형 평균 albedo
  A   = opacity: 255 opaque, 128 LEAVES, 0 AIR/WATER/GLASS

light: Rgba8Unorm 3D
  RGB = emissive RGB / 8
  A   = sky light / 15
```

TORCH emissive 색:

```text
(1.00, 0.42, 0.12) * 8.0
```

sample 시 RGB에 8을 다시 곱한다.

clipmap source:

* level 0: 로딩된 `World` 블록·조명.
* level 1: 로딩된 World 2³ 최빈값.
* level 2~3: M8 LOD cache.
* source가 아직 준비되지 않은 셀은 opacity 0, light 0으로 두고 그 레벨의 `ready mask`를 0으로 둔다.
* ray는 준비되지 않은 level을 만나면 다음 coarse level로 즉시 전환한다.

#### 18.2.2 토로이달 갱신

각 레벨의 logical min:

```text
cell_size = 1 << level
camera_cell = floor(camera_world / cell_size)
unsnapped_min = camera_cell - 64
```

갱신 양자:

| level | cell 단위 quantum |
| ----: | --------------: |
|     0 |               4 |
|     1 |               4 |
|     2 |               2 |
|     3 |               1 |

```text
origin = floor(unsnapped_min / quantum) * quantum
```

origin 이동 `delta`:

```text
ring_offset = (ring_offset + delta) mod 128
```

logical 셀 `p`의 physical texel:

```text
physical = (p - origin + ring_offset) & 127
```

* 한 축 delta가 128 이상이면 full rebuild.
* 그 외에는 새로 노출된 X/Y/Z slab만 갱신.
* 겹치는 slab 중복 영역은 한 번만 생성.
* texture wrap 때문에 한 slab upload는 축마다 최대 2개, 총 최대 8개의 `TexelCopyTextureInfo` 영역으로 분할.
* 근거리 level 우선.
* 업로드 상한은 프레임당 4MiB.
* full rebuild 중에는 완성된 coarse level부터 사용.
* 월드 블록 편집은 네 레벨의 포함 cell과 1셀 이웃을 invalidation한다.
* clipmap 셀 10% 이상이 한 번에 교체되면 GI temporal history를 reset한다.

#### 18.2.3 DDA ray

GI 해상도는 internal 1/4이다. opaque G버퍼 픽셀만 trace한다. WATER·GLASS·sky는 GI trace 대상이 아니다.

표면 시작점:

```text
origin = world_position + normal * 0.05
```

cosine hemisphere:

```text
u = fract(Hammersley(i,N) + blue_noise_rotation)
phi = 2πu.x
r = sqrt(u.y)
local = (r*cos(phi), r*sin(phi), sqrt(1-u.y))
```

Frisvad 방식 ONB로 world normal 축에 회전한다.

balanced:

* rays/pixel: 4.
* 최대 거리: 48블록.
* 최대 DDA cell crossing: ray당 96.
* workgroup: 8×8.

거리별 clip level:

```text
0 <= t < 8   → level 0
8 <= t < 16  → level 1
16 <= t < 32 → level 2
32 <= t       → level 3
```

레벨이 바뀌면 현재 위치에서 그 cell size에 맞춰 DDA를 다시 초기화한다.

hit 조건:

```text
material.a >= 0.5
```

DDA가 통과한 마지막 축으로 hit normal을 구한다. 음수 좌표는 `floor`를 사용한다.

hit radiance:

```text
emission = clip_light.rgb * 8

sky = clip_light.a
ambient_at_hit = sky_lut(up_direction) * sky * 0.35

L = -sun_dir
direct_at_hit =
    albedo
  * sun_color
  * 2.2
  * max(dot(hit_normal,L),0)
  * sample_csm(hit_position)
  * sky

incoming = emission + ambient_at_hit + direct_at_hit
```

miss는 ray 방향의 sky LUT 값을 반환한다.

cosine-weighted sampling이므로 diffuse estimator:

```text
indirect = surface_albedo * mean(incoming_rays)
indirect *= 0.65
indirect *= mix(1.0, ssao, 0.35)
```

deferred direct·ambient가 계산된 뒤, transparent 이전에 HDR에 더한다.

#### 18.2.4 시간적 누적

버퍼:

| 버퍼              | 포맷            |
| --------------- | ------------- |
| current GI      | `Rgba16Float` |
| history GI      | `Rgba16Float` |
| moments         | `Rg16Float`   |
| previous depth  | `R32Float`    |
| previous normal | `Rg16Float`   |

현재 월드 위치를 `prev_view_proj`로 투영한다.

history 허용:

```text
prev_uv in [0,1]
relative depth difference <= 0.02
absolute depth difference <= 0.50 blocks
dot(current_normal, prev_normal) >= 0.90
material class unchanged
```

history는 현재 3×3 GI neighborhood min/max 범위에 clamp한다. 범위는 양쪽으로 10% 확장한다.

```text
history_weight = 0.90
output = mix(current, clamped_history, history_weight)
```

다음에는 0으로 바꾼다.

* 첫 프레임.
* reject.
* resize·render scale 변경.
* 카메라 순간이동 >8블록.
* clipmap 10% 이상 rebuild.
* day phase가 한 프레임에 0.01 이상 변경.
* GI on/off 전환.

#### 18.2.5 à-trous

3단계, step width `1,2,4`다. 5×5 B3-spline kernel:

```text
[1,4,6,4,1] outer product / 256
```

edge weight:

```text
w_depth  = exp(-abs(di-dc)/(0.02*dc+0.05))
w_normal = max(dot(Ni,Nc),0)^32
w_luma   = exp(-abs(Yi-Yc)/(sqrt(variance)+0.02))
weight   = kernel * w_depth * w_normal * w_luma
```

각 단계는 ping-pong `Rgba16Float`를 사용한다.

#### 18.2.6 실패 폴백

다음 중 하나면 GI를 세션 동안 비활성화한다.

* 필요한 3D 텍스처 크기·포맷 미지원.
* clipmap allocation 실패.
* GI shader/pipeline validation 실패.
* 필수 bind-group 생성 실패.
* compute dispatch validation error.

게임은 종료하지 않는다.

```text
GI disabled: <reason>; using baked light + SSAO
```

폴백 화면은 M6 light + M7 CSM/SSAO/deferred + M8 효과다. clipmap 부분 실패 시 반쪽 GI를 유지하지 않는다. 전체 GI 묶음을 끈다.

### 18.3 Rust/WGSL 계약

```rust
pub const CLIPMAP_RESOLUTION: u32 = 128;
pub const CLIPMAP_LEVELS: usize = 4;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ClipmapUniform {
    pub origin_cell_size: [[i32; 4]; 4], // xyz logical origin, w cell size
    pub ring_offset_ready: [[u32; 4]; 4], // xyz offset, w ready revision
    pub params: [f32; 4], // max_distance,rays,intensity,enabled
}

pub struct ClipmapLevel {
    pub material: wgpu::Texture,
    pub light: wgpu::Texture,
    pub origin: IVec3,
    pub ring_offset: UVec3,
    pub revision: u32,
    pub ready: bool,
}

pub struct VoxelUpload {
    pub level: u8,
    pub logical_origin: IVec3,
    pub extent: UVec3,
    pub material: Vec<u8>,
    pub light: Vec<u8>,
    pub revision: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GiMode {
    Enabled,
    Fallback,
    DisabledByUser,
}

pub struct GiConfig {
    pub rays: u32,
    pub max_distance: f32,
    pub intensity: f32,
}

pub struct GiRenderer {
    pub mode: GiMode,
    // private resources
}
```

clipmap bind group는 texture binding array를 쓰지 않는다.

```text
group 2:
  binding 0 material0 texture_3d<f32>
  binding 1 light0    texture_3d<f32>
  binding 2 material1
  binding 3 light1
  binding 4 material2
  binding 5 light2
  binding 6 material3
  binding 7 light3
  binding 8 ClipmapUniform
```

GI trace input group:

```text
depth
normal_material
albedo
ssao
shadow array
shadow sampler
sky LUT
```

output group:

```text
current GI storage texture
```

`Rgba16Float` storage texture 지원 여부와 실제 wgpu 30 format feature 조회 API는 레지스트리 소스에서 확인한다. 지원하지 않으면 §18.2.6 폴백이다.

M9 debug view 추가:

```text
gi
clipmap
```

* gi: tone 축소한 denoised indirect RGB.
* clipmap: 픽셀 ray 시작점의 선택 level을 L0 파랑, L1 초록, L2 주황, L3 자홍으로 표시.

### 18.4 상수

| 항목                    |                  값 |
| --------------------- | -----------------: |
| clipmap 해상도           |               128³ |
| clipmap level         |                  4 |
| voxel 크기              |            1,2,4,8 |
| 범위                    |   128,256,512,1024 |
| material 포맷           |       `Rgba8Unorm` |
| light 포맷              |       `Rgba8Unorm` |
| clipmap 총 texture 메모리 |            약 64MiB |
| 업로드 예산                |         4MiB/frame |
| GI 해상도                |       internal 1/4 |
| balanced ray          |            4/pixel |
| max ray 거리            |                 48 |
| max DDA crossing      |             96/ray |
| workgroup             |                8×8 |
| temporal history      |               0.90 |
| depth relative reject |               0.02 |
| depth absolute reject |               0.50 |
| normal reject         |          dot <0.90 |
| à-trous 단계            |                  3 |
| à-trous step          |              1,2,4 |
| GI intensity          |               0.65 |
| TORCH GI RGB          | `(1.00,0.42,0.12)` |
| emissive decode scale |                8.0 |

### 18.5 테스트 계약

```text
clipmap_toroidal_wrap_preserves_logical_voxels
clipmap_move_updates_only_exposed_slabs
clipmap_teleport_requests_full_rebuild
clipmap_level_selection_matches_distance_bands
dda_hits_first_opaque_voxel
dda_crosses_negative_coordinates
dda_misses_empty_volume
cosine_hemisphere_samples_are_above_normal
temporal_gi_rejects_depth_disocclusion
temporal_gi_rejects_normal_disocclusion
atrous_preserves_depth_edge
colored_emission_encodes_warm_torch
gi_failure_uses_baked_lighting
```

판정 기준:

* 한 cell 이동 시 full rebuild가 아니고 정확히 128² 새 cell/slab.
* 토로이달 이동 전후 겹치는 logical voxel 값 bitwise 동일.
* DDA 첫 hit 위치와 normal 정확 일치.
* 모든 cosine sample의 `dot(sample,N) >= 0`.
* temporal reject 결과는 current와 bitwise 동일.
* à-trous 후 깊이 경계 양쪽 평균색 혼합이 원래 대비의 20% 미만.
* 폴백 시 GI 리소스 dispatch 0회, 최종 baked light가 finite.

### 18.6 스냅샷 검증

`m9-gi-room`은 밀폐된 방으로, 왼쪽 BRICK 벽, 오른쪽 SAND 벽, 중앙 TORCH, 직접광이 막힌 바닥 영역을 가진다.

```bash
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
```

픽셀 판정:

```bash
python3 - <<'PY'
from PIL import Image
def lin(v):
    v/=255
    return v/12.92 if v<=.04045 else ((v+.055)/1.055)**2.4
def mean(path,box):
    im=Image.open(path).convert("RGB")
    s=[0,0,0]; n=0
    for y in range(box[1],box[3]):
        for x in range(box[0],box[2]):
            p=im.getpixel((x,y))
            for i in range(3): s[i]+=lin(p[i])
            n+=1
    return [v/n for v in s]
off=mean("/tmp/vf_m9_gi_off.png",(250,230,390,320))
on=mean("/tmp/vf_m9_gi_on.png",(250,230,390,320))
offY=.2126*off[0]+.7152*off[1]+.0722*off[2]
onY=.2126*on[0]+.7152*on[1]+.0722*on[2]
warm=on[0]/max((on[1]+on[2])*0.5,1e-6)
print(off,on,onY/offY,warm)
assert 1.20 <= onY/offY <= 2.50
assert warm >= 1.15
PY
```

시간 누적:

```bash
cargo run --release --bin snapshot -- \
  --fixture m9-gi-room --size 640x360 --gi on \
  --fixed-exposure 1.0 --warmup 1 --view gi \
  --out /tmp/vf_m9_gi_1.png
cargo run --release --bin snapshot -- \
  --fixture m9-gi-room --size 640x360 --gi on \
  --fixed-exposure 1.0 --warmup 32 --view gi \
  --out /tmp/vf_m9_gi_32.png

python3 - <<'PY'
from PIL import Image, ImageFilter, ImageChops
a=Image.open("/tmp/vf_m9_gi_1.png").convert("L").crop((180,100,460,320))
b=Image.open("/tmp/vf_m9_gi_32.png").convert("L").crop((180,100,460,320))
def highfreq(im):
    blur=im.filter(ImageFilter.GaussianBlur(1))
    d=ImageChops.difference(im,blur)
    return sum(d.getdata())/(d.width*d.height)
ha,hb=highfreq(a),highfreq(b)
print(ha,hb,1-hb/ha)
assert hb <= ha*0.65
PY
```

clipmap seam:

```bash
cargo run --release --bin snapshot -- \
  --fixture m9-clipmap --pos 63.5,72,0 --size 640x360 \
  --gi on --warmup 32 --view gi --out /tmp/vf_m9_clip_a.png
cargo run --release --bin snapshot -- \
  --fixture m9-clipmap --pos 64.5,72,0 --size 640x360 \
  --gi on --warmup 32 --view gi --out /tmp/vf_m9_clip_b.png
```

두 이미지 중앙 60% crop의 평균 절대 선형 RGB 차이는 0.08 이하이다.

### 18.7 성능 예산

Apple M5, 2560×1440, render scale 0.75, balanced 4 rays:

| 패스                            | GPU p95 예산 |
| ----------------------------- | ---------: |
| clipmap light/material 준비 GPU |     0.35ms |
| GI DDA trace                  |     1.80ms |
| temporal                      |     0.25ms |
| à-trous 3단계                   |     0.80ms |
| composite                     |     0.15ms |
| GI 합계                         |   ≤ 3.35ms |
| M7~M9 전체 GPU                  |   ≤ 15.5ms |
| 전체 프레임 p95                    |   ≤ 16.6ms |
| 전체 프레임 max                    |     < 25ms |

CPU:

* clipmap gather·packing p95 ≤1.5ms/worker job.
* 메인 upload encode ≤1.0ms/frame.
* texture upload ≤4MiB/frame.
* clipmap texture 약 64MiB.
* GI 2D history·ping-pong은 render scale 0.75에서 40MiB 이하.

### 18.8 M9 분기 선결정 — 묻지 말고 이렇게

* 하드웨어 RT → 사용하지 않음.
* Metal acceleration structure → 사용하지 않음.
* `wgpu-hal` Metal 인터롭 → 사용하지 않음.
* GI 방식 → 4레벨 128³ clipmap + WGSL compute DDA.
* clipmap 포맷 → material/light 모두 `Rgba8Unorm`.
* color light → RGB emissive texture, TORCH warm orange.
* balanced ray → 4, 거리 48.
* ray 분포 → cosine Hammersley + blue-noise rotation.
* temporal history → 0.90, depth·normal 검사.
* denoise → à-trous 3단계.
* 실패 → GI 전체 off, M6 baked light + M7 SSAO로 폴백.
* M8과 M9는 연속 구현한다.
* M9가 끝나면 LOG 기록 후 멈추고 Claude 리뷰를 받는다.

---

## 19. M10 상세 설계 — 배포 가능한 게임 마감

M10 범위는 다음 다섯 묶음이다.

1. HUD·핫바·조준점·일시정지·설정 UI·F2 스크린샷.
2. 설정 파일·키 바인딩·팬리스 Air 성능 프리셋.
3. 절차 생성 효과음.
4. 유저 셰이더팩·텍스처팩 오버라이드.
5. macOS `.app` 번들·아이콘·codesign·notarize 스크립트.

### 19.1 새 모듈

| 경로                         | 역할                                         | 마일스톤  |
| -------------------------- | ------------------------------------------ | ----- |
| `src/config.rs`            | 설정 스키마·경로·clamp·atomic 저장·입력 우선순위          | M10.1 |
| `src/preset.rs`            | Performance/Balanced/Quality 수치            | M10.1 |
| `src/game_clock.rs`        | pause 가능한 월드 시각                            | M10.1 |
| `src/ui/font.rs`           | 8×8 ASCII bitmap → R8 atlas                | M10.2 |
| `src/ui/geometry.rs`       | 텍스트·사각형·블록 아이콘 vertex batch                | M10.2 |
| `src/ui/menu.rs`           | HUD·pause·settings 상태와 hit testing         | M10.2 |
| `src/render/ui.rs`         | 네이티브 해상도 UI 파이프라인                          | M10.2 |
| `src/render/screenshot.rs` | 3-buffer 비동기 F2 readback·PNG 저장            | M10.2 |
| `src/audio.rs`             | rodio 출력·voice 제한·silent fallback          | M10.3 |
| `src/audio/synth.rs`       | 결정적 PCM 효과음 생성                             | M10.3 |
| `src/shaderpack.rs`        | pack manifest·파일 우선순위·transactional reload | M10.4 |
| `src/bin/icon.rs`          | image 크레이트로 1024² 복셀 아이콘 생성                | M10.5 |
| `assets/shaders/ui.wgsl`   | bitmap text·solid quad·block icon          | M10.2 |
| `assets/macos/Info.plist`  | 번들 metadata                                | M10.5 |
| `scripts/bundle.sh`        | release build·app 조립·icns·codesign         | M10.5 |
| `scripts/notarize.sh`      | zip·notarytool·staple                      | M10.5 |

`cargo add font8x8`, `cargo add rodio`로 실제 버전을 설치하고 LOG에 기록한다. 기억으로 버전을 직접 적지 않는다.

### 19.2 알고리즘·동작

#### 19.2.1 설정 파일

경로 우선순위:

1. `--settings <path>`
2. `VF_SETTINGS`
3. `$HOME/Library/Application Support/Voxelforge/settings.json`
4. HOME이 없으면 메모리 기본값만 쓰고 저장을 비활성화하며 경고 1회.

설정 입력 우선순위:

```text
CLI > environment > settings.json > Balanced 기본값
```

* `--preset`, `VF_PRESET`은 현재 실행에만 적용하고 파일을 덮어쓰지 않는다.
* UI에서 바꾸면 `preset = "custom"`으로 바꾸고 500ms debounce 뒤 저장한다.
* 저장은 같은 디렉터리의 `settings.json.tmp`에 쓴 뒤 flush·rename한다.
* 파싱 실패 파일은 `settings.json.invalid-<unix-seconds>`로 rename하고 기본값을 쓴다.
* `version != 1`이면 `settings.json.unsupported-<unix-seconds>`로 rename한다.
* 알 수 없는 JSON 필드는 무시한다.
* 숫자는 아래 범위로 clamp하고 경고한다.

스키마:

```json
{
  "version": 1,
  "video": {
    "preset": "balanced",
    "render_scale": 0.75,
    "view_radius": 10,
    "shadow_resolution": 1536,
    "shadow_distance": 192.0,
    "ssao_divisor": 2,
    "ssr_steps": 48,
    "volumetric_steps": 40,
    "cloud_view_steps": 48,
    "cloud_light_steps": 6,
    "gi_enabled": true,
    "gi_rays": 4,
    "gi_distance": 48.0,
    "bloom_levels": 5,
    "vsync": true,
    "shader_pack": "builtin"
  },
  "controls": {
    "mouse_sensitivity": 0.002,
    "invert_y": false,
    "bindings": {
      "forward": "KeyW",
      "backward": "KeyS",
      "left": "KeyA",
      "right": "KeyD",
      "jump": "Space",
      "sprint": "ControlLeft",
      "descend": "ShiftLeft",
      "toggle_fly": "KeyF",
      "pause": "Escape",
      "screenshot": "F2",
      "debug_view": "F4",
      "shader_reload": "KeyR"
    }
  },
  "audio": {
    "master": 0.8,
    "effects": 0.8
  },
  "ui": {
    "scale": 1.0
  }
}
```

범위:

| 항목                | 범위                    |
| ----------------- | --------------------- |
| render scale      | 0.50~1.00             |
| view radius       | 4~16                  |
| shadow resolution | 1024, 1536, 2048 중 하나 |
| shadow distance   | 128~256               |
| mouse sensitivity | 0.0005~0.0100         |
| audio volume      | 0~1                   |
| UI scale          | 0.75~1.50             |
| SSR step          | 16~64                 |
| volumetric step   | 16~64                 |
| GI ray            | 1~6                   |
| GI distance       | 16~64                 |

#### 19.2.2 성능 프리셋

| 항목                 | Performance | Balanced 기본 | Quality |
| ------------------ | ----------: | ----------: | ------: |
| render scale       |        0.50 |        0.75 |    1.00 |
| full-detail radius |           8 |          10 |      12 |
| shadow resolution  |        1024 |        1536 |    2048 |
| shadow distance    |         160 |         192 |     256 |
| SSAO divisor       |           4 |           2 |       2 |
| SSR step           |          24 |          48 |      64 |
| volumetric step    |          24 |          40 |      64 |
| cloud view step    |          32 |          48 |      64 |
| cloud light step   |           4 |           6 |       8 |
| GI ray             |           2 |           4 |       6 |
| GI distance        |          32 |          48 |      64 |
| bloom level        |           3 |           5 |       5 |
| LOD horizon        |      `128R` |      `128R` |  `128R` |

첫 실행은 Balanced다. 자동 동적 해상도는 만들지 않는다. 프레임에 따라 설정을 몰래 바꾸지 않는다.

#### 19.2.3 HUD·폰트

폰트는 `font8x8` 크레이트의 ASCII `BASIC_FONTS`를 사용한다.

* 지원 문자: ASCII 32..126.
* UI 문자열은 영어.
* 시작 시 16열×6행, 128×48 `R8Unorm` atlas 생성.
* 한 glyph는 8×8.
* Nearest sampler.
* glyph bitmap의 bit 0은 왼쪽 픽셀로 통일한다. 실제 크레이트 비트 방향을 테스트로 고정한다.
* 외부 TTF, CoreText, SDF font는 쓰지 않는다.

UI pixel scale:

```text
pixel_scale =
    max(1, round(output_height / 720 * settings.ui.scale * 2))
```

HUD:

* 조준점: 화면 중앙, 흰색 1×5/5×1 막대 + 1픽셀 검은 외곽.
* 핫바: 화면 아래 중앙, 9칸.
* 슬롯 외곽 크기: `20 * pixel_scale`.
* 슬롯 간격: `1 * pixel_scale`.
* 아래 여백: `8 * pixel_scale`.
* 선택 슬롯 외곽 2픽셀 흰색, 비선택 1픽셀 회색.
* 각 슬롯은 해당 블록의 +Y texture layer를 표시.
* 선택 블록 이름은 핫바 위에 8×8 bitmap text로 표시.
* F3는 계속 콘솔 통계만 토글한다. F3 텍스트 디버그 오버레이는 만들지 않는다.

#### 19.2.4 일시정지·설정 메뉴

ESC 동작을 바꾼다.

```text
Playing + ESC → Paused, 커서 해제·표시
Paused + ESC  → Playing, 커서 잠금
```

두 번째 ESC로 종료하던 기존 동작은 제거한다. 종료는 pause 메뉴의 Quit 또는 창 닫기다.

pause 중:

* 물리·입력·월드 시각 정지.
* 스트리밍 결과 회수·저장은 계속.
* shader hotreload 계속.
* 렌더·UI 계속.
* 발소리·물 소리 정지.
* `GameClock`의 elapsed는 증가하지 않는다.

메뉴:

```text
Resume
Settings
Quit
```

Settings:

```text
Preset
Render Scale
View Radius
Mouse Sensitivity
Master Volume
Effects Volume
Key Bindings
Back
```

슬라이더는 키보드 좌우와 마우스 클릭을 지원한다. key binding 항목을 선택하면 다음 `PhysicalKey::Code` 한 번을 받는다. `Escape`를 재바인딩하려는 시도는 거부한다.

#### 19.2.5 F2 스크린샷

F2는 UI까지 포함된 네이티브 최종 프레임을 저장한다.

경로:

1. `VF_SCREENSHOTS`
2. `$HOME/Pictures/Voxelforge`
3. HOME이 없으면 기능 비활성화·경고.

파일명:

```text
vf_YYYYMMDD_HHMMSS_mmm.png
```

스크린샷 readback:

* 재사용 buffer 3개.
* `copy_texture_to_buffer` 뒤 `map_async`.
* 인코딩은 별도 std thread 한 개.
* queue가 3개 모두 사용 중이면 요청을 버리고 경고.
* 렌더 스레드에서 map 완료를 기다리지 않는다.
* PNG는 `image` 크레이트.
* row 256바이트 정렬은 기존 offscreen 코드와 같은 계산.

#### 19.2.6 사운드

외부 오디오 파일을 넣지 않는다. 48kHz mono PCM을 시작 시 생성하고, 재생 시 equal-power stereo pan을 적용한다.

| 효과               |    길이 | 기본 amplitude | 생성                                                 |
| ---------------- | ----: | -----------: | -------------------------------------------------- |
| footstep         |  90ms |         0.18 | xorshift noise, one-pole α=0.18, `(1-t)²` envelope |
| break            | 180ms |         0.28 | noise α=0.35, 0/35/70ms impulse                    |
| place            |  70ms |         0.20 | 180Hz sine, `exp(-35t)`                            |
| water enter/exit | 250ms |         0.15 | noise α=0.05, attack 20ms, decay                   |

xorshift seed:

```text
world_seed XOR event_counter XOR block_id*0x9e3779b9
```

발소리:

* 지면 위에서 걸은 수평 누적 거리 1.8블록마다.
* sprint는 1.3블록마다.
* 공중·비행·pause 중에는 없음.
* WATER 안에서는 footstep 대신 0.9블록마다 water 효과를 0.6배로 재생.

동시 voice 최대 16개. 17번째는 가장 오래된 non-water voice를 제거한다. 최종 PCM은 `[-0.95,+0.95]`로 clamp한다.

rodio 초기화 실패는 게임 실패가 아니다.

```text
audio unavailable: <reason>; continuing silently
```

#### 19.2.7 셰이더팩

기본 개발 경로:

```text
assets/shaderpacks/<name>/
├── pack.json
├── shaders/
└── textures/blocks/
```

배포 앱의 사용자 경로는 launcher가 `VF_SHADERPACKS`로 `$HOME/Library/Application Support/Voxelforge/shaderpacks`를 지정한다.

manifest:

```json
{
  "version": 1,
  "contract_version": 1,
  "name": "Example",
  "author": "Name",
  "shaders": ["deferred.wgsl", "water.wgsl"],
  "textures": ["stone.png"]
}
```

`<name>`과 manifest 파일명은 하나의 normal path component만 허용한다. `/`, `\`, `.`, `..`를 거부한다.

override 가능한 shader:

```text
chunk.wgsl
shadow.wgsl
ssao.wgsl
sky_lut.wgsl
deferred.wgsl
translucent.wgsl
water.wgsl
volumetric.wgsl
clouds.wgsl
bloom.wgsl
exposure.wgsl
tonemap.wgsl
present.wgsl
gi_trace.wgsl
gi_temporal.wgsl
gi_denoise.wgsl
gi_composite.wgsl
```

`ui.wgsl`은 override 불가다.

로딩 우선순위:

```text
shader:
  selected user pack
  → assets/shaderpacks/<name>
  → assets/shaders

block texture:
  selected user pack
  → assets/shaderpacks/<name>
  → assets/textures/blocks
  → procedural
```

셰이더팩은 Rust 측 bind-group layout, texture format, entry point 이름을 바꿀 수 없다. `contract_version=1` 계약을 그대로 구현해야 한다. include·매크로·별도 전처리기는 없다.

팩 교체·핫리로드:

1. 모든 override 소스를 읽는다.
2. 모든 shader module과 pipeline을 임시 `RenderPack`에 만든다.
3. error scope를 모두 pop한다.
4. 하나라도 실패하면 임시 묶음을 전부 버린다.
5. 성공할 때만 프레임 경계에서 전체 묶음을 교체한다.
6. 실패 시 이전 팩을 유지한다.

#### 19.2.8 `.app` 번들

출력:

```text
target/dist/Voxelforge.app/
└── Contents/
    ├── Info.plist
    ├── MacOS/
    │   ├── voxelforge-launcher
    │   └── voxelforge-bin
    └── Resources/
        ├── assets/
        └── Voxelforge.icns
```

`Info.plist` 필수값:

```xml
CFBundleIdentifier      io.github.chajinheon.voxelforge
CFBundleName            Voxelforge
CFBundleDisplayName     Voxelforge
CFBundleExecutable      voxelforge-launcher
CFBundlePackageType     APPL
CFBundleShortVersionString  0.1.0
CFBundleVersion         1
LSMinimumSystemVersion  26.0
NSHighResolutionCapable true
```

launcher:

```sh
#!/bin/sh
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
RESOURCES="$HERE/../Resources"
SUPPORT="$HOME/Library/Application Support/Voxelforge"

mkdir -p "$SUPPORT/saves" "$SUPPORT/shaderpacks"
mkdir -p "$HOME/Pictures/Voxelforge"

export VF_ASSETS="$RESOURCES/assets"
export VF_SAVE_ROOT="$SUPPORT/saves"
export VF_SETTINGS="$SUPPORT/settings.json"
export VF_SHADERPACKS="$SUPPORT/shaderpacks"
export VF_SCREENSHOTS="$HOME/Pictures/Voxelforge"

exec "$HERE/voxelforge-bin" "$@"
```

D18은 바꾸지 않는다. launcher가 기존 최우선 경로인 `VF_ASSETS`를 설정한다.

M10에서 `SaveDir`은 다음 root 우선순위를 추가한다.

```text
VF_SAVE_ROOT → assets::project_root()/saves
```

아이콘:

* `src/bin/icon.rs`가 1024×1024 PNG를 절차 생성.
* 배경 `(28,32,42)`.
* 중앙에 잔디·흙·돌 3층 isometric voxel cube.
* alpha 255.
* `scripts/bundle.sh`가 `sips`로 16, 32, 64, 128, 256, 512, 1024 크기 iconset을 만들고 `iconutil -c icns`.

codesign:

* `VF_CODESIGN_IDENTITY`가 없으면 `-`로 ad-hoc sign.
* 있으면 `--options runtime --timestamp`.
* 내부 binary → launcher → app 순으로 sign.
* `codesign --verify --strict --verbose=2` 실행.

notarize:

* `VF_NOTARY_PROFILE` 필수.
* `ditto -c -k --keepParent`.
* `xcrun notarytool submit ... --keychain-profile "$VF_NOTARY_PROFILE" --wait`.
* 성공 후 `xcrun stapler staple`.
* profile이 없으면 명확한 오류로 종료한다.

### 19.3 Rust 계약

```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub version: u32,
    pub video: VideoSettings,
    pub controls: ControlSettings,
    pub audio: AudioSettings,
    pub ui: UiSettings,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PerformancePreset {
    Performance,
    Balanced,
    Quality,
    Custom,
}

impl Settings {
    pub fn load(path: &Path) -> anyhow::Result<Self>;
    pub fn clamp_and_warn(&mut self);
    pub fn write_atomic(&self, path: &Path) -> anyhow::Result<()>;
}

pub struct GameClock {
    elapsed: f64,
    paused: bool,
}

impl GameClock {
    pub fn update(&mut self, real_dt: f32);
    pub fn set_paused(&mut self, paused: bool);
    pub fn seconds(&self) -> f32;
}

pub enum UiScreen {
    Hud,
    Pause,
    Settings,
    Rebinding(BindingAction),
}

pub struct UiFrame {
    pub vertices: Vec<UiVertex>,
    pub indices: Vec<u32>,
}

pub struct ScreenshotQueue {
    // exactly three reusable slots
}

pub enum AudioEvent {
    Footstep { block: BlockId, sprint: bool },
    Break { block: BlockId },
    Place { block: BlockId },
    Water,
}

pub struct ShaderPackManifest {
    pub version: u32,
    pub contract_version: u32,
    pub name: String,
    pub author: String,
    pub shaders: Vec<String>,
    pub textures: Vec<String>,
}
```

CLI 추가:

```text
--settings <path>
--preset performance|balanced|quality
--shader-pack <name>
--audio-self-test <wav-path>
```

### 19.4 상수

| 항목                        |                값 |
| ------------------------- | ---------------: |
| font glyph                |              8×8 |
| font atlas                | 128×48 `R8Unorm` |
| hotbar slot               |     20 UI pixels |
| hotbar count              |                9 |
| screenshot buffer         |                3 |
| screenshot encoder thread |                1 |
| audio sample rate         |            48000 |
| max audio voices          |               16 |
| settings debounce         |            500ms |
| shader poll               |             0.5초 |
| shader contract version   |                1 |
| 설정 version                |                1 |
| app 최소 macOS              |             26.0 |
| 기본 preset                 |         Balanced |

### 19.5 테스트 계약

```text
settings_roundtrip_and_clamp
settings_unknown_fields_are_ignored
settings_atomic_write_leaves_no_temp_file
binding_names_roundtrip
preset_values_match_contract
pause_stops_game_clock
ui_bitmap_atlas_contains_ascii_glyphs
ui_hotbar_geometry_is_centered
ui_crosshair_is_centered_at_even_and_odd_sizes
screenshot_queue_drops_when_full_without_panic
procedural_audio_is_deterministic
audio_mix_stays_below_full_scale
shaderpack_rejects_path_traversal
shaderpack_texture_precedence_matches_contract
shaderpack_reload_is_transactional
bundle_launcher_sets_asset_and_save_overrides
info_plist_has_required_keys
```

판정 기준:

* clamp 후 모든 필드가 §19.2.1 범위 안.
* atomic 저장 성공 후 `.tmp` 없음.
* pause 10초 update 후 `GameClock.seconds()` 변화 0.
* ASCII `"Voxelforge 0123456789"` glyph가 atlas에서 모두 1픽셀 이상.
* 핫바 중심 오차 ≤0.5 physical pixel.
* screenshot 4번째 요청은 `Dropped`, panic 없음.
* 같은 seed·event PCM bitwise 동일.
* PCM 최대 절댓값 ≤0.95.
* invalid shaderpack 뒤 active pack ID가 이전과 같음.
* launcher에 다섯 `VF_*` export가 모두 존재.
* `plutil -lint` 통과.

### 19.6 검증

HUD·pause:

```bash
cargo run --release --bin snapshot -- \
  --fixture m10-hud --size 1280x720 --ui hud \
  --out /tmp/vf_m10_hud.png
cargo run --release --bin snapshot -- \
  --fixture m10-hud --size 1280x720 --ui pause \
  --out /tmp/vf_m10_pause.png

python3 - <<'PY'
from PIL import Image, ImageChops
hud=Image.open("/tmp/vf_m10_hud.png").convert("RGB")
pause=Image.open("/tmp/vf_m10_pause.png").convert("RGB")
cx,cy=hud.width//2,hud.height//2
cross=sum(max(hud.getpixel((x,y)))>180
          for y in range(cy-8,cy+9)
          for x in range(cx-8,cx+9))
assert cross >= 8
hotbar=hud.crop((350,600,930,715))
assert sum(max(p)>80 for p in hotbar.getdata()) > 2000
diff=ImageChops.difference(hud,pause).convert("L")
assert sum(v>4 for v in diff.getdata())/(hud.width*hud.height) >= 0.15
PY
```

설정:

```bash
rm -f /tmp/vf_settings.json /tmp/vf_settings.json.tmp
VF_SETTINGS=/tmp/vf_settings.json VF_SMOKE_FRAMES=3 \
  cargo run --release
python3 - <<'PY'
import json
p="/tmp/vf_settings.json"
d=json.load(open(p))
assert d["version"] == 1
assert d["video"]["preset"] == "balanced"
assert 0.5 <= d["video"]["render_scale"] <= 1.0
PY
```

오디오:

```bash
cargo run --release -- \
  --audio-self-test /tmp/vf_audio.wav

python3 - <<'PY'
import wave, struct, math
with wave.open("/tmp/vf_audio.wav","rb") as w:
    assert w.getframerate() == 48000
    assert w.getnchannels() == 1
    data=w.readframes(w.getnframes())
s=struct.unpack("<%dh"%(len(data)//2),data)
rms=math.sqrt(sum(x*x for x in s)/len(s))/32768
peak=max(abs(x) for x in s)/32768
print(rms,peak)
assert 0.01 <= rms <= 0.40
assert peak <= 0.95
PY
```

셰이더팩 실패 폴백:

```bash
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
printf 'this is not wgsl\n' >/tmp/vf_shaderpacks/broken/shaders/deferred.wgsl

VF_SHADERPACKS=/tmp/vf_shaderpacks VF_SHADERPACK=broken \
VF_SMOKE_FRAMES=3 cargo run --release 2>&1 | \
tee /tmp/vf_shaderpack.log

grep -q "shader pack.*rejected" /tmp/vf_shaderpack.log
grep -q "using builtin" /tmp/vf_shaderpack.log
```

번들:

```bash
bash scripts/bundle.sh
plutil -lint target/dist/Voxelforge.app/Contents/Info.plist
codesign --verify --strict --verbose=2 target/dist/Voxelforge.app
VF_SMOKE_FRAMES=3 \
  target/dist/Voxelforge.app/Contents/MacOS/voxelforge-launcher
```

notarize dry validation:

```bash
bash -n scripts/notarize.sh
test -x scripts/bundle.sh
test -x scripts/notarize.sh
```

성능:

```bash
VF_PRESET=balanced VF_AUTOPILOT=stream VF_BENCH_FRAMES=3600 \
  cargo run --release 2>&1 | tee /tmp/vf_m10_bench.log
```

첫 600프레임 제외:

* p95 전체 프레임 ≤16.6ms.
* max <25ms.
* 정착 <5.0초.
* GPU 메모리 추정 로그 <1.25GiB.
* screenshot·audio·UI를 켠 상태에서도 같은 기준.

### 19.7 성능 예산

| 항목                       |                     예산 |
| ------------------------ | ---------------------: |
| UI GPU                   |                ≤0.25ms |
| UI geometry CPU          |                ≤0.20ms |
| audio callback           |                ≤0.20ms |
| screenshot 평상시           |                    0ms |
| screenshot 요청 프레임 CPU 추가 |                 ≤1.0ms |
| 설정 저장                    | 렌더 스레드 blocking ≤0.5ms |
| shaderpack 평상시 overhead  |         mtime poll 외 0 |
| Balanced 전체 p95          |                ≤16.6ms |
| Balanced max             |                  <25ms |
| 전체 추정 GPU+CPU 렌더 메모리     |               <1.25GiB |

스크린샷 PNG 인코딩·설정 JSON 실제 파일 쓰기는 렌더 스레드에서 하지 않는다. 설정은 500ms debounce 뒤 전용 짧은 writer thread에서 처리한다.

### 19.8 M10 분기 선결정 — 묻지 말고 이렇게

* M10 범위 → UI, 설정·프리셋, 사운드, 셰이더팩, `.app` 배포.
* 폰트 → `font8x8` ASCII 8×8, 시작 시 atlas 생성.
* UI 언어 → 영어.
* F3 → 콘솔 통계 유지, 디버그 텍스트 오버레이 없음.
* ESC → pause/resume. 두 번째 ESC 종료 제거.
* 스크린샷 → F2, UI 포함, 비동기 3-buffer.
* 사운드 → rodio + 절차 PCM, 외부 WAV 없음.
* 오디오 장치 실패 → silent fallback.
* 설정 → Application Support, atomic JSON version 1.
* 첫 preset → Balanced.
* 자동 dynamic resolution → 하지 않음.
* 셰이더팩 → 전체 파일 override, bind/layout 계약 고정, transactional swap.
* 배포 asset 경로 → launcher가 `VF_ASSETS` 지정.
* codesign → identity 환경변수, 없으면 ad-hoc.
* MetalFX → 구현하지 않음. `wgpu-hal` 인터롭과 `unsafe`가 필요하므로 현재 절대 규칙과 맞지 않는다.
* 비큐브 TORCH, 인벤토리, 서바이벌, 멀티플레이, 모드 API, 리플레이, 게임패드, 크로스플랫폼 번들 → 하지 않음.
* M10이 끝나면 LOG 기록 후 멈추고 Claude 최종 리뷰를 받는다.
## 20. M6 이후 통합 보강 설계 — M7~M10 무중단 실행·M5 Air High·크리에이티브 건축 UX

확정 시점은 M6 구현·리뷰·커밋 이후다. 이 절은 §16~§19를 삭제하지 않고 보강한다. 같은 항목이 충돌하면 **§20의 수치·계약·진행 방식이 우선**한다. §1 D1~D18, §12, §14.11~§14.12, §15의 M6 조명 계약은 바꾸지 않는다.

M7부터 M10까지는 하나의 연속 구현 구간이다. M7·M8·M9 종료 시 검증과 LOG 기록은 하되 사람 리뷰를 기다리지 않는다. M10 최종 검증 뒤에만 멈춘다. 중간 마일스톤의 검증이 실패하면 그 자리에서 원인을 수정하고 같은 검증을 다시 통과시킨 뒤 다음 단계로 간다.

### 20.1 최종 제품 목표와 고정 범위

M10 완료 시 다음이 동시에 성립해야 한다.

1. 2560×1440 물리 출력의 Apple M5 팬리스 MacBook Air에서 기본 `m5_air_high` 프리셋이 60fps 목표를 지킨다.
2. 오프스크린 HDR·PBR G버퍼·CSM·GTAO·대기·TAAU·블룸·자동 노출·고품질 물·볼류메트릭 포그·볼류메트릭 구름·LOD·컴퓨트 DDA GI가 하나의 프레임 그래프로 동작한다.
3. `I`로 크리에이티브 인벤토리를 열고 122개 건축 아이템을 검색·분류·핫바 배치할 수 있다.
4. 큐브뿐 아니라 통나무 축, 반블록, 계단, 유리판, 울타리를 배치·파괴·선택할 수 있다.
5. 화면 오른쪽 아래에 1인칭 손과 선택 아이템이 보이고, 걷기·파괴·설치·아이템 전환 애니메이션이 결정적으로 동작한다.
6. 설정·핫바가 저장되고, 스크린샷·사운드·셰이더팩·`.app` 번들이 동작한다.
7. 모든 완료 판정은 테스트·로그·PNG 픽셀·GPU/CPU 수치로 자동 검증된다.

하지 않는 것:

- 도어, 트랩도어, 사다리, 레드스톤, 상자 인벤토리, 아이템 수량, 제작, 생존, 몹, 멀티플레이.
- 곡면 블록, 임의 회전 메시, glTF 모델, 스킨 편집기.
- MetalFX, `wgpu-hal`, 하드웨어 레이트레이싱, Metal acceleration structure.
- 자동 동적 해상도. 프리셋은 사용자가 고르고 실행 중 몰래 바뀌지 않는다.

### 20.2 M5 Air High 렌더 품질 계약

#### 20.2.1 프리셋

`m5_air_high`가 모든 첫 실행의 기본값이다. 대상 제품이 Apple M5용이므로 adapter 이름으로 품질을 자동 하향하지 않는다. `AdapterInfo`는 LOG와 진단에만 기록한다. 사용자가 저장한 설정이 있으면 저장값이 우선한다.

| 항목 | performance | balanced | m5_air_high 기본 | cinematic |
|---|---:|---:|---:|---:|
| render scale | 0.58 | 0.67 | 0.72 | 1.00 |
| TAAU | on | on | on | on |
| sharpen | 0.12 | 0.16 | 0.18 | 0.12 |
| CSM 해상도 | 1536 | 1792 | 2048 | 2048 |
| shadow PCF tap | 8 | 8 | 12 | 20 |
| shadow distance | 160 | 192 | 224 | 256 |
| GTAO 방향×step | 4×3 | 6×3 | 8×4 | 8×6 |
| POM step+refine | off | 6+2 | 8+2 | 16+3 |
| SSR step+refine | 24+4 | 32+5 | 40+5 | 64+6 |
| volumetric view step | 24 | 28 | 32 | 48 |
| cloud view/light step | 28/4 | 32/5 | 40/6 | 64/8 |
| GI ray/pixel | 2 | 3 | 4 | 6 |
| GI max distance | 32 | 40 | 48 | 64 |
| bloom level | 4 | 5 | 6 | 6 |
| full-detail radius | 8 | 9 | 10 | 12 |

`cinematic`은 자동 스냅샷과 정지 장면용이다. 60fps 완료 조건은 `m5_air_high`에만 적용한다.

내부 렌더 크기는 출력 종횡비를 보존하며 높이에서 계산한다.

```text
internal_h = max(8, round(output_h * scale / 8) * 8)
internal_w = max(8, round((internal_h * output_w / output_h) / 8) * 8)
```

2560×1440, scale 0.72의 계약값은 1848×1040이다. 스냅샷 기본은 명시하지 않으면 `m5_air_high`; 렌더 패스의 픽셀 기준 검증은 항상 `--render-scale 1.0`과 고정 노출을 명시한다.

#### 20.2.2 프레임 그래프

M10 최종 순서는 다음으로 고정한다.

```text
00 clipmap CPU 결과 적용·GPU slab upload
01 CSM cascade update 선택·shadow draw
02 opaque/cutout G-buffer
03 linear-depth pyramid
04 half-res GTAO raw
05 GTAO spatial+temporal
06 atmosphere LUT 갱신 선택
07 deferred PBR + sky → internal HDR
08 quarter-res voxel GI trace
09 GI temporal + à-trous
10 GI composite → internal HDR
11 quarter-res volumetric fog/light
12 quarter-res volumetric clouds
13 cloud/fog temporal + depth-aware composite
14 HDR scene copy
15 distance-sorted GLASS/WATER forward list; WATER samples immutable opaque-HDR copy
16 selection outline
17 internal bloom pyramid
18 average log luminance + exposure
19 native-resolution TAAU HDR reconstruction of `internal HDR + bloom`
20 ACES + color grade + sharpen → native LDR
21 native-resolution first-person hand/viewmodel
22 modal background blur when inventory/pause is open
23 native-resolution HUD/inventory/pause UI
24 optional screenshot copy
25 present copy to caller output view
```

`Renderer`는 여전히 `Surface`와 `Window`를 모른다. 25번 present copy의 대상은 호출자가 넘긴 `TextureView`다.

#### 20.2.3 WGSL·GPU 최적화 계약

- 모든 render/compute pipeline, bind-group layout, sampler, texture view는 init 또는 resize/settings change 때만 만든다. frame loop에서 생성 금지.
- 단순 image kernel(depth mip, bloom, exposure reduction)은 `@workgroup_size(16,16,1)`. depth/normal 의존 분기(GTAO, TAAU, water, volumetric, cloud, GI)는 `@workgroup_size(8,8,1)`.
- workgroup shared memory는 GTAO/TAAU의 10×10 depth/normal tile에만 사용하고 group당 16KiB 이하.
- shader loop의 최대 반복은 preset uniform이 아니라 compile-time upper bound로 고정하고 `if i>=active_steps { break; }`를 쓴다. upper bound: POM16, contact8, GTAO48, SSR64, volumetric48, cloud64, cloud-light8, GI ray6, DDA96, à-trous25 taps.
- `textureDimensions`를 fragment마다 호출하지 않는다. inverse dimensions를 uniform에 둔다.
- 동일 pixel에서 같은 texture/UV를 두 번 sample하지 않는다. albedo/material/emission 결과를 local variable에 유지한다.
- fullscreen triangle은 vertex buffer 없이 `vertex_index` 0..2로 만든다.
- f16은 필수 feature로 요청하지 않는다. 모든 필수 경로는 f32다. `SHADER_F16` 전용 변형은 M10 범위 밖이다.
- NaN 방지: normalize 입력 길이 최소 `1e-8`, division denominator 최소 `1e-4`, `pow` base 0 이상, log luminance 최소 `1e-4`.
- frame당 `queue.write_buffer`는 Globals 1회, dirty chunk-uniform 연속 range 최대 1회, pass uniform ring 최대 1회로 batch한다.
- `Vec`/`HashMap` scratch는 Renderer 필드에서 재사용한다. global allocator hook은 `unsafe`가 필요하므로 쓰지 않는다. 대신 모든 renderer-owned scratch collection을 `TrackedScratch<T>`로 감싸 capacity 증가 때 `cpu_capacity_growths`를, GPU buffer/texture/bind-group/pipeline 생성 때 `gpu_resource_creations`를 증가시킨다. `VF_ALLOC_STATS=1` 600 steady frames에서 두 값의 합은 중앙값 0, p95 0이어야 한다.
- resource label은 `vf/<milestone>/<module>/<name>` 형식으로 붙인다. validation error에 unlabeled resource가 없어야 한다.

`m5_air_high` 최대 texture/comparison sample 수의 정적 상한:

| pass | pixel당 최대 sample |
|---|---:|
| G-buffer POM 포함 | 13 |
| CSM+contact | 20 |
| GTAO raw | 34 |
| deferred | 9 |
| TAAU current/history/clamp | 30 |
| WATER SSR/refraction | 52 |
| volumetric | 48 |
| cloud | 64 view + 48 light-density |
| GI | 4 rays×96 occupancy fetch 상한 |

### 20.3 M7 보강 — 재질 배열·PBR·GTAO·TAAU

#### 20.3.1 새 모듈

| 경로 | 역할 |
|---|---|
| `src/render/materials.rs` | albedo/material/emission `texture_2d_array`, 재질 LUT, 절차 재질 생성 |
| `src/render/pbr.rs` | CPU 참조 GGX·oct normal·POM 상수와 테스트 |
| `src/render/depth_pyramid.rs` | linear depth mip compute |
| `src/render/gtao.rs` | GTAO raw·spatial·temporal |
| `src/render/taau.rs` | native HDR history, reprojection, YCoCg clamp, sharpen 입력 |
| `src/render/atmosphere.rs` | transmittance·multi-scatter·sky-view LUT와 sky cubemap |
| `assets/shaders/gbuffer.wgsl` | normal map·POM·motion/reactive 출력 |
| `assets/shaders/gtao.wgsl` | horizon GTAO·temporal |
| `assets/shaders/taau.wgsl` | native 해상도 TAAU |
| `assets/shaders/pbr_common.wgsl` | GGX·oct·색공간 공통 함수. 텍스트 include는 없으므로 빌드 시 Rust가 문자열 결합 |

`main.rs`, `stream.rs`, `renderer.rs`가 500줄에 닿기 전에 하위 모듈로 분리한다. shader source 결합은 `shader_source.rs` 한 곳에서 `pbr_common.wgsl + pass.wgsl` 순서로 한다. 임의 include 문법을 WGSL에 만들지 않는다.

#### 20.3.2 재질 텍스처 배열

D6을 유지하며 세 배열 모두 같은 layer 순서와 16×16 크기를 쓴다. 정점의 기존 `tex: u16`이 곧 material-layer ID다. 모든 face별 재질은 `BlockDef.textures[face]`로 선택하고 deferred의 `MaterialGpu`도 이 layer ID로 인덱스한다. material layer hard cap은 256이며 M10 builtin layer 수는 128 이하로 유지한다.

| 배열 | 포맷 | 채널 |
|---|---|---|
| albedo | `Rgba8UnormSrgb` | RGB base color, A cutout/translucency mask |
| material | `Rgba8Unorm` | RG tangent normal XY, B roughness, A height |
| emission | `R8Unorm` | texel emission mask |

모든 배열은 `texture_2d_array`, mip level은 5개(16,8,4,2,1)다. mip은 CPU box filter로 결정적으로 만든다. alpha-cutout mip의 alpha는 평균이 아니라 coverage-preserving threshold 보정으로 만든다. 원본 layer의 `alpha >= 0.5` coverage와 각 mip coverage 차이는 3%p 이하다.

`MaterialGpu`:

```rust
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct MaterialGpu {
    pub base: [f32; 4],      // metallic, normal_strength, height_scale, emission_strength
    pub tint: [f32; 4],      // emission tint rgb, alpha_cutoff
    pub flags: [u32; 4],     // bit0 POM, bit1 cutout, bit2 translucent, bit3 double_sided
}
```

G버퍼의 A 채널에서 복원한 material-layer ID는 `u32(round(a*255.0))`다. `MaterialGpu`, emission tint, flags는 이 ID로 조회한다. 범위 밖이면 `STONE_MATERIAL_LAYER = def(STONE).textures[0]`를 사용한다.

POM은 full cube의 opaque/cutout face에서만 쓴다. slab/stair/pane/fence, WATER, GLASS, LEAVES에는 쓰지 않는다.

```text
view_ts = tangent-space view direction
layers = preset step count
layer_depth = 1/layers
uv_delta = view_ts.xy / max(abs(view_ts.z), 0.20)
         * material.height_scale / layers
```

높이 맵 A는 0=깊은 홈, 1=표면이다. 현재 layer depth가 `1-height(uv)`를 처음 넘은 지점에서 멈추고, 앞·뒤 두 샘플을 선형 보간한 뒤 지정된 refine 횟수만큼 이분 탐색한다. UV 변위 절댓값은 각 축 0.08 이하로 clamp한다. 시선 `N·V < 0.15`, 선형 depth >48, derivative footprint >0.25 texel이면 POM을 끄고 원 UV를 쓴다.

normal map은 height의 toroidal Sobel로 생성한다.

```text
dx = (h(x+1,y-1)+2h(x+1,y)+h(x+1,y+1)
    - h(x-1,y-1)-2h(x-1,y)-h(x-1,y+1)) / 8
dy = 같은 방식의 y 미분
n = normalize((-dx*normal_strength, -dy*normal_strength, 1))
RG = n.xy*0.5+0.5
```

#### 20.3.3 G버퍼

| 타깃 | 포맷 | 내용 |
|---|---|---|
| `gbuffer_albedo` | `Rgba8UnormSrgb` | RGB albedo, A metallic |
| `gbuffer_normal` | `Rgba16Float` | RG world oct normal, B roughness, A emission strength |
| `gbuffer_light` | `Rgba8Unorm` | R block light/15, G sky/15, B vertex AO/3, A material-layer ID/255 |
| `gbuffer_motion` | `Rg16Float` | current UV - previous UV |
| `gbuffer_reactive` | `R8Unorm` | cutout/emissive/animation history 감쇠 마스크 |
| `depth` | `Depth32Float` | 표준 Z |

motion은 jitter를 제거한 현재·이전 clip 좌표로 계산한다. 새로 생성된 청크, teleport, resize, shaderpack swap, render-scale 변경은 해당 픽셀 motion=0, reactive=1로 둔다. reactive target은 프레임 시작에 0으로 clear하고 G-buffer cutout/emissive가 `max(alpha_edge, emission_mask)`를 쓴다. forward GLASS는 `max(old, alpha*0.75)`, WATER는 `max(old, 0.60+0.40*foam)`, outline은 1.0을 쓴다. 여러 pass가 같은 픽셀을 덮을 때 blend operation은 `Max`다.

#### 20.3.4 PBR 직접광·환경광

직접광은 Cook–Torrance GGX다.

```text
a = max(roughness², 0.0025)
a2 = a²
D = a2 / (π * ((NdotH²*(a2-1)+1)²))
k = (roughness+1)² / 8
G1(x) = x / (x*(1-k)+k)
G = G1(NdotV)*G1(NdotL)
F0 = mix(vec3(0.04), albedo, metallic)
F = F0 + (1-F0)*(1-VdotH)^5
specular = D*G*F / max(4*NdotV*NdotL, 1e-4)
diffuse = (1-F)*(1-metallic)*albedo/π
direct = (diffuse+specular) * sun_radiance * NdotL * shadow * sky_visibility
```

금속값:

- GOLD_BLOCK 1.0
- METAL_PANEL 0.90
- RUSTED_METAL 0.75
- COPPER, WEATHERED_COPPER 0.90
- 나머지 0.0

sky specular는 64×64×6 `Rgba16Float` cubemap 7 mip을 쓴다. mip 0은 sky-view LUT에서 생성하고, 다음 mip은 2×2 box filter다. `lod = roughness²*6`. diffuse ambient는 `sample_sky(normal)*0.22 + zenith_color*0.08`이다. M6 block light ambient는 warm tint `(1.0,0.58,0.28)`를 쓴다. M9 GI가 켜지면 이 ambient는 유지하되 GI가 추가된다.

#### 20.3.5 CSM 업데이트 cadence와 contact shadow

§16의 3 cascade와 PSSM을 유지하되 `m5_air_high` split far는 224다. split은 런타임에 같은 PSSM λ=0.65 식으로 계산한다.

업데이트:

- cascade 0: 매 프레임.
- cascade 1: 짝수 프레임 또는 카메라 2블록 이동 또는 태양 방향 0.25° 변화.
- cascade 2: `frame_index % 4 == 0` 또는 카메라 8블록 이동 또는 태양 방향 0.50° 변화.
- 청크 edit가 cascade 범위 안이면 해당 cascade를 즉시 갱신.

PCF는 고정 Poisson disk를 회전해 쓴다. `m5_air_high` 12 tap, 회전각은 64² blue-noise와 frame index 0..7로 정하며 TAAU가 누적한다. receiver bias 0.0008, normal offset 0.025는 유지한다.

screen-space contact shadow:

- internal half-resolution.
- 태양 방향으로 view-space 최대 2.5블록.
- 8 step.
- thickness 0.06+0.0015*depth.
- CSM shadow에 `min`으로 결합.
- Performance에서는 off, 나머지는 on.

#### 20.3.6 GTAO

Crytek SSAO 대신 XeGTAO 계열 horizon search를 쓴다.

- half-resolution.
- radius 1.5블록.
- falloff start 0.6*radius.
- thickness 0.20.
- 방향·step은 프리셋 표.
- 각 방향은 blue-noise로 회전.
- depth discontinuity absolute 0.5 또는 relative 2%에서 history reject.
- normal dot <0.85에서 reject.
- temporal history 0.90 static, 0.75 motion >1px.
- spatial bilateral 5×5 separable, depth sigma 0.75, normal power 32.

출력은 `R8Unorm`, 1=unoccluded다. deferred에서는 ambient와 GI에만 곱하고 direct·emission에는 곱하지 않는다.

#### 20.3.7 대기와 sky LUT

기법은 Hillaire 2020 계열 LUT 분해를 단순화해 사용한다.

| LUT | 크기·포맷 | 갱신 |
|---|---|---|
| transmittance | 256×64 `Rgba16Float` | 시작·계수 변경 시 |
| multiscatter | 32×32 `Rgba16Float` | transmittance 뒤 |
| sky view | 192×108 `Rgba16Float` | 8프레임마다 또는 phase 0.0005 변화 |
| sky cubemap | 64²×6, 7 mip `Rgba16Float` | sky view 갱신 뒤 |

대기 상수는 §16의 지구/대기 반경, βR, βM, 높이척도, Mie g를 유지한다. LUT 좌표는 다음으로 고정한다.

```text
h = clamp((r-R_ground)/(R_atmosphere-R_ground),0,1)
mu = dot(ray, radial_up)
transmittance uv = ((mu+1)*0.5, sqrt(h))
multiscatter uv = ((sun_mu+1)*0.5, sqrt(h))
sky-view u = fract(atan2(dir.z,dir.x)/(2π)+0.5)
sky-view v = clamp(0.5-asin(dir.y)/π,0,1)
```

ray-sphere 교점은 작은 양의 해를 선택하고, ground를 먼저 맞으면 적분을 그 지점에서 끝낸다. transmittance는 40 step, multiscatter는 texel당 16방향×8 step, sky view는 24 view step×8 sun step이다. multiscatter 보정은 `single_scatter + multiscatter * (1-transmittance)`로 합친다. 모든 LUT 값은 finite·0 이상이어야 한다.

#### 20.3.8 TAAU

TAAU는 M7 필수다. native 해상도 `Rgba16Float` history 두 장, previous native linear depth `R32Float`, normal `Rg16Float`를 사용한다.

8-frame Halton jitter는 다음 순서를 반복한다.

```text
( 0.0000000,-0.1666667)
(-0.2500000, 0.1666667)
( 0.2500000,-0.3888889)
(-0.3750000,-0.0555556)
( 0.1250000, 0.2777778)
(-0.1250000,-0.2777778)
( 0.3750000, 0.0555556)
(-0.4375000, 0.3888889)
```

jitter clip offset는 `(2*jitter.x/internal_w, -2*jitter.y/internal_h)`다.

exposure reduction은 bloom을 더하기 전 internal HDR만 샘플한다. TAAU 현재 색 입력은 `internal_hdr + bloom*intensity`이며 scene-referred HDR라 exposure 변화의 영향을 history에 축적하지 않는다. 현재 샘플은 native pixel의 jittered 위치에서 16-tap Catmull–Rom으로 재구성한다. history UV는 motion을 빼서 구한다. native current depth·normal·material-layer history는 같은 UV에서 가장 가까운 internal point sample을 선택해 기록하고, motion/reactive는 bilinear sample한다.

reject:

- history UV outside.
- absolute depth diff >0.50.
- relative depth diff >0.02.
- normal dot <0.90.
- material-layer ID 변화.
- reactive >=0.95.
- resize, scale, teleport >8블록, FOV 변화 >1°, shaderpack swap.

YCoCg neighborhood clamp:

1. internal current 3×3을 YCoCg로 변환.
2. min/max와 평균·표준편차를 구한다.
3. 허용 범위는 `max(min, mean-1.25σ)`부터 `min(max, mean+1.25σ)`.
4. history를 범위에 clamp.

history weight:

```text
motion_px = length(motion * output_size)
w_motion = mix(0.92, 0.78, saturate(motion_px/8))
w = w_motion * (1 - 0.80*reactive)
output = mix(current, history_clamped, w)
```

hand와 UI는 TAAU 뒤에 그려 history에 들어가지 않는다.

#### 20.3.9 톤매핑·색 보정·샤픈

ACES fitted는 §16 식을 유지한다. 그 뒤 선형 공간에서 다음을 적용한다.

```text
contrast pivot = 0.18
contrast = 1.06
saturation = 1.04
lift = -0.003
gain = 1.01
```

샤픈은 native LDR의 5-tap unsharp다.

```text
blur = (N+S+E+W+4*C)/8
sharp = clamp(C + amount*(C-blur), min(N,S,E,W,C), max(N,S,E,W,C))
```

amount는 프리셋 표다. UI·hand에는 적용하지 않는다.

#### 20.3.10 M7 성능 예산

Apple M5, 2560×1440, `m5_air_high`, R=10, 정착 후 GPU p95:

| 패스 | p95 |
|---|---:|
| CSM+contact | 2.00ms |
| G-buffer+POM | 1.75ms |
| depth pyramid | 0.20ms |
| GTAO 전체 | 0.90ms |
| atmosphere 갱신 상각 | 0.20ms |
| deferred PBR | 1.20ms |
| bloom+exposure | 0.75ms |
| TAAU+ACES+sharpen | 1.10ms |
| M7 GPU 합계 | 8.10ms |

M7 종료 시 p95 전체 프레임 ≤12.5ms, max <25ms다. 실패하면 품질 수치를 낮추지 말고 다음 순서로 최적화한다.

1. bind group·pipeline 전환 제거.
2. far cascade cadence 확인.
3. LUT 불필요 갱신 제거.
4. fullscreen pass 합치기.
5. storage texture read/write 중복 제거.
6. CPU draw list·uniform write batch.

### 20.4 M8 보강 — 고품질 물·볼류메트릭·구름

#### 20.4.1 물

§17의 네 Gerstner 파동을 유지한다. 다음을 추가한다.

- linear-depth pyramid를 SSR에 사용.
- `m5_air_high`: 40 coarse step + 5 binary refine, max 48블록.
- 첫 8 step은 mip0, 이후 projected footprint에 따라 mip 1..5.
- hit confidence는 edge·distance·normal facing·depth residual 네 항의 곱.
- residual >thickness면 miss.
- SSR miss는 sky cubemap roughness mip.

shore foam:

```text
shore = 1-smoothstep(0.15,1.25,water_thickness)
crest = smoothstep(0.06,0.13,abs(gerstner_height_delta))
noise = smoothstep(0.45,0.70,fbm(world_xz*0.35 + time*vec2(0.08,0.04)))
foam = saturate(shore*0.85 + crest*0.45) * noise
```

foam color `(0.80,0.92,0.95)`, roughness 0.85. water는 replacement pass, depth write off를 유지한다.

수중:

- camera eye가 WATER 셀 안이면 `underwater=1`.
- 최대 시야 64블록.
- absorption `(0.16,0.065,0.028)`.
- scattering `(0.015,0.12,0.18)`.
- distortion `0.0025*sin(uv.y*90+time*1.6)`.
- volumetric density 3배.
- UI·hand는 tint하지 않는다.

#### 20.4.2 볼류메트릭 포그/라이트

quarter-resolution, checkerboard 2×2다. 한 프레임에 parity `(frame_index&1, (frame_index>>1)&1)` 픽셀만 full march하고 나머지는 history reprojection한다. 4프레임 안에 전 픽셀이 갱신된다.

`m5_air_high`:

- 32 view step.
- 최대 224블록.
- HG g=0.65.
- CSM sample은 2 step마다 한 번, 중간 step은 선형 보간.
- local TORCH fog glow는 가장 가까운 8개 광원만 CPU에서 uniform 배열로 전달하고, 거리 16블록에서 0.
- temporal static weight 0.93, motion >2px 0.80.

#### 20.4.3 구름과 구름 그림자

§17 Perlin–Worley를 유지한다. `m5_air_high`는 quarter-resolution checkerboard, 40 view step, 6 light step다.

cloud shadow map:

- 512×512 `R8Unorm`.
- 카메라 중심 1024×1024블록 정사영.
- texel 2블록.
- 8프레임마다 또는 카메라 16블록 이동 시 갱신.
- 태양 방향으로 cloud layer를 12 step 적분.
- terrain direct sun에 `mix(0.55,1.0,shadow)`를 곱한다.
- 32블록 이동 단위로 shadow map origin을 snap한다.

#### 20.4.4 M8 성능 예산

| 패스 | `m5_air_high` p95 |
|---|---:|
| WATER+SSR | 1.20ms |
| volumetric fog | 0.85ms |
| clouds | 1.05ms |
| cloud shadow 상각 | 0.15ms |
| LOD 추가 draw | 0.45ms |
| M8 추가 합계 | 3.70ms |
| M7+M8 GPU 합계 | ≤11.8ms |
| 전체 프레임 p95 | ≤15.0ms |

### 20.5 M9 보강 — GI 품질·스케줄

§18의 4×128³ clipmap, WGSL compute DDA, 하드웨어 RT 금지를 유지한다.

`m5_air_high` 고정값:

- quarter-resolution.
- pixel당 4 cosine ray.
- ray max 48블록.
- ray당 max 96 crossing.
- temporal history 0.90.
- à-trous 3단계 1,2,4.
- intensity 0.65.

추가 스케줄:

- clipmap upload는 프레임당 4MiB hard cap.
- level 0/1 slab가 있으면 2/3보다 우선.
- 카메라 속도 >12 blocks/s이면 level 0의 ray max를 32로 줄이지 않는다. 대신 upload backlog를 허용하고 ready mask로 coarse level fallback한다.
- GI trace는 checkerboard하지 않는다. quarter-resolution 모든 픽셀을 매 프레임 계산한다.
- TORCH, GLOWSTONE, SEA_LANTERN, WARM_LAMP, COLD_LAMP, GLOW_PANEL의 RGB emission을 clipmap light에 기록한다.

| 블록 | RGB radiance |
|---|---|
| TORCH | `(8.0,3.36,0.96)` |
| GLOWSTONE | `(6.0,3.4,1.5)` |
| SEA_LANTERN | `(3.0,5.5,6.5)` |
| WARM_LAMP | `(8.0,4.4,1.8)` |
| COLD_LAMP | `(4.5,6.5,8.0)` |
| GLOW_PANEL | `(6.5,6.8,7.0)` |

M6 flood-fill emission level은 TORCH 14, GLOWSTONE·SEA_LANTERN·WARM_LAMP·COLD_LAMP·GLOW_PANEL 15다. 나머지는 0이다.

M9 추가 GPU p95는 3.2ms, M7~M9 전체 GPU p95는 15.0ms 이하, 전체 프레임 p95는 16.6ms 이하, max는 25ms 미만이다.

### 20.6 M10 크리에이티브 건축 시스템 — 블록·아이템·형상

M10의 최우선은 배포 스크립트가 아니라 건축 UX다. 순서는 「레지스트리·형상 → 메시·물리·레이캐스트 → I 인벤토리·아이콘 → 손·상호작용 → 설정·사운드·셰이더팩·번들」이다.

#### 20.6.1 새 모듈

| 경로 | 역할 |
|---|---|
| `src/world/material.rs` | 재질 ID·절차 texture recipe·PBR 계수 |
| `src/world/shape.rs` | 1/16 occupancy, AABB, boundary coverage, cached template |
| `src/world/catalog.rs` | 안정된 BlockId·ItemId·카테고리·배치 규칙 |
| `src/world/connect.rs` | pane/fence 연결 mask 재계산 |
| `src/mesh/shaped.rs` | partial-neighbor face subtraction·template emission |
| `src/player/place.rs` | slab merge·stair facing·axis log·connection placement |
| `src/player/pick.rs` | middle-click block → ItemId |
| `src/ui/inventory.rs` | I 창 상태·검색·카테고리·scroll·hotbar assignment |
| `src/ui/item_icons.rs` | 122개 64² icon array bake·cache |
| `src/render/viewmodel.rs` | 1인칭 손·held item·애니메이션 |
| `src/gameplay/interaction_anim.rs` | swing/place/switch state machine |
| `assets/shaders/item_icon.wgsl` | 고정 isometric item icon |
| `assets/shaders/viewmodel.wgsl` | native LDR viewmodel PBR·ACES |
| `assets/shaders/ui_blur.wgsl` | inventory/pause 배경 blur |

#### 20.6.2 BlockId 안정 레지스트리

0~12는 M6 값을 그대로 유지한다. 아래 ID는 save compatibility 계약이다. 이름을 바꾸거나 순서를 재배치하지 않는다.

```text
  0 AIR
  1 STONE
  2 DIRT
  3 GRASS
  4 SAND
  5 WATER
  6 OAK_LOG_Y              // 기존 LOG alias
  7 OAK_LEAVES             // 기존 LEAVES alias
  8 OAK_PLANKS             // 기존 PLANKS alias
  9 CLEAR_GLASS            // 기존 GLASS alias
 10 RED_BRICK              // 기존 BRICK alias
 11 COBBLESTONE            // 기존 COBBLE alias
 12 TORCH
 13 DIRT_PATH
 14 GRAVEL
 15 RED_SAND
 16 CLAY
 17 MUD
 18 SNOW
 19 MOSS
 20 POLISHED_STONE
 21 STONE_BRICKS
 22 MOSSY_STONE_BRICKS
 23 CHISELED_STONE
 24 SLATE
 25 POLISHED_SLATE
 26 BASALT
 27 POLISHED_BASALT
 28 LIMESTONE
 29 LIMESTONE_BRICKS
 30 MARBLE
 31 MARBLE_TILES
 32 SANDSTONE
 33 CUT_SANDSTONE
 34 RED_SANDSTONE
 35 QUARTZ
 36 QUARTZ_TILES
 37 OBSIDIAN
 38 BIRCH_LOG_Y
 39 SPRUCE_LOG_Y
 40 DARK_OAK_LOG_Y
 41 BIRCH_PLANKS
 42 SPRUCE_PLANKS
 43 DARK_OAK_PLANKS
 44 BIRCH_LEAVES
 45 SPRUCE_LEAVES
 46 DARK_OAK_LEAVES
 47 BOOKSHELF
 48 CRATE
 49 BARREL
 50 HAY_BALE
 51 WHITE_CONCRETE
 52 LIGHT_GRAY_CONCRETE
 53 GRAY_CONCRETE
 54 BLACK_CONCRETE
 55 BROWN_CONCRETE
 56 RED_CONCRETE
 57 ORANGE_CONCRETE
 58 YELLOW_CONCRETE
 59 LIME_CONCRETE
 60 GREEN_CONCRETE
 61 CYAN_CONCRETE
 62 LIGHT_BLUE_CONCRETE
 63 BLUE_CONCRETE
 64 PURPLE_CONCRETE
 65 MAGENTA_CONCRETE
 66 PINK_CONCRETE
 67 WHITE_GLASS
 68 RED_GLASS
 69 GREEN_GLASS
 70 CYAN_GLASS
 71 BLUE_GLASS
 72 GLOWSTONE
 73 SEA_LANTERN
 74 WARM_LAMP
 75 COLD_LAMP
 76 GLOW_PANEL
 77 ROOF_TILE
 78 CHECKER_TILE
 79 CERAMIC_TILE
 80 METAL_PANEL
 81 RUSTED_METAL
 82 COPPER
 83 WEATHERED_COPPER
 84 GOLD_BLOCK
 85 PRISMARINE
 86 DARK_PRISMARINE
 87 ICE
 88 PACKED_ICE
```

상태 ID 범위:

```text
128 OAK_LOG_X
129 OAK_LOG_Z
130 BIRCH_LOG_X
131 BIRCH_LOG_Z
132 SPRUCE_LOG_X
133 SPRUCE_LOG_Z
134 DARK_OAK_LOG_X
135 DARK_OAK_LOG_Z

SLAB_BASE = 256
  material_index 0..11, state 0=bottom, 1=top
  id = 256 + material_index*2 + state

STAIR_BASE = 512
  material_index 0..11
  facing 0=North(-Z), 1=East(+X), 2=South(+Z), 3=West(-X)
  state = facing | (upside_down ? 4 : 0)
  id = 512 + material_index*8 + state

PANE_BASE = 768
  material_index 0..5
  mask bits: North=1, East=2, South=4, West=8
  id = 768 + material_index*16 + mask

FENCE_BASE = 896
  material_index 0..3
  mask bits: North=1, East=2, South=4, West=8
  id = 896 + material_index*16 + mask
```

ID 89~127, 136~255, 280~511, 608~767, 864~895, 960 이상은 현재 예약이다. 범위 밖 `def()`는 AIR를 반환하는 기존 계약을 유지한다.

slab/stair material_index:

```text
0 STONE
1 STONE_BRICKS
2 COBBLESTONE
3 SLATE
4 MARBLE
5 LIMESTONE_BRICKS
6 SANDSTONE
7 RED_BRICK
8 OAK_PLANKS
9 BIRCH_PLANKS
10 SPRUCE_PLANKS
11 DARK_OAK_PLANKS
```

pane material_index:

```text
0 CLEAR_GLASS
1 WHITE_GLASS
2 RED_GLASS
3 GREEN_GLASS
4 CYAN_GLASS
5 BLUE_GLASS
```

fence material_index:

```text
0 OAK_PLANKS
1 BIRCH_PLANKS
2 SPRUCE_PLANKS
3 DARK_OAK_PLANKS
```

#### 20.6.3 ItemId와 122개 카탈로그

`ItemId = u16`, 0은 EMPTY다. 블록 ID와 item ID가 같다고 가정하지 않는다.

```rust
pub type ItemId = u16;
pub const EMPTY_ITEM: ItemId = 0;

pub enum ItemCategory {
    Terrain,
    Masonry,
    WoodNature,
    Color,
    GlassLight,
    DetailUtility,
    Shapes,
}

pub enum PlacementKind {
    Block(BlockId),
    AxisLog { y: BlockId, x: BlockId, z: BlockId },
    Slab { material: u8, full: BlockId },
    Stair { material: u8 },
    Pane { material: u8 },
    Fence { material: u8 },
}

pub struct ItemDef {
    pub id: ItemId,
    pub name: &'static str,
    pub category: ItemCategory,
    pub placement: PlacementKind,
    pub icon_block: BlockId,
    pub search_terms: &'static [&'static str],
}
```

카탈로그 순서와 ID:

```text
Terrain
  1 Grass             -> Block(3)
  2 Dirt              -> Block(2)
  3 Dirt Path         -> Block(13)
  4 Stone             -> Block(1)
  5 Cobblestone       -> Block(11)
  6 Gravel            -> Block(14)
  7 Sand              -> Block(4)
  8 Red Sand          -> Block(15)
  9 Clay              -> Block(16)
 10 Mud               -> Block(17)
 11 Snow              -> Block(18)
 12 Moss              -> Block(19)

Masonry
 13 Polished Stone       -> Block(20)
 14 Stone Bricks         -> Block(21)
 15 Mossy Stone Bricks   -> Block(22)
 16 Chiseled Stone       -> Block(23)
 17 Slate                -> Block(24)
 18 Polished Slate       -> Block(25)
 19 Basalt               -> Block(26)
 20 Polished Basalt      -> Block(27)
 21 Limestone            -> Block(28)
 22 Limestone Bricks     -> Block(29)
 23 Marble               -> Block(30)
 24 Marble Tiles         -> Block(31)
 25 Sandstone            -> Block(32)
 26 Cut Sandstone        -> Block(33)
 27 Red Sandstone        -> Block(34)
 28 Quartz               -> Block(35)
 29 Quartz Tiles         -> Block(36)
 30 Obsidian             -> Block(37)

WoodNature
 31 Oak Log        -> AxisLog{6,128,129}
 32 Birch Log      -> AxisLog{38,130,131}
 33 Spruce Log     -> AxisLog{39,132,133}
 34 Dark Oak Log   -> AxisLog{40,134,135}
 35 Oak Planks     -> Block(8)
 36 Birch Planks   -> Block(41)
 37 Spruce Planks  -> Block(42)
 38 Dark Oak Planks-> Block(43)
 39 Oak Leaves     -> Block(7)
 40 Birch Leaves   -> Block(44)
 41 Spruce Leaves  -> Block(45)
 42 Dark Oak Leaves-> Block(46)
 43 Bookshelf      -> Block(47)
 44 Crate          -> Block(48)
 45 Barrel         -> Block(49)
 46 Hay Bale       -> Block(50)

Color
 47 White Concrete      -> Block(51)
 48 Light Gray Concrete -> Block(52)
 49 Gray Concrete       -> Block(53)
 50 Black Concrete      -> Block(54)
 51 Brown Concrete      -> Block(55)
 52 Red Concrete        -> Block(56)
 53 Orange Concrete     -> Block(57)
 54 Yellow Concrete     -> Block(58)
 55 Lime Concrete       -> Block(59)
 56 Green Concrete      -> Block(60)
 57 Cyan Concrete       -> Block(61)
 58 Light Blue Concrete -> Block(62)
 59 Blue Concrete       -> Block(63)
 60 Purple Concrete     -> Block(64)
 61 Magenta Concrete    -> Block(65)
 62 Pink Concrete       -> Block(66)

GlassLight
 63 Clear Glass -> Block(9)
 64 White Glass -> Block(67)
 65 Red Glass   -> Block(68)
 66 Green Glass -> Block(69)
 67 Cyan Glass  -> Block(70)
 68 Blue Glass  -> Block(71)
 69 Glowstone   -> Block(72)
 70 Sea Lantern -> Block(73)
 71 Warm Lamp   -> Block(74)
 72 Cold Lamp   -> Block(75)
 73 Glow Panel  -> Block(76)
 74 Torch       -> Block(12)

DetailUtility
 75 Red Brick          -> Block(10)
 76 Roof Tile          -> Block(77)
 77 Checker Tile       -> Block(78)
 78 Ceramic Tile       -> Block(79)
 79 Metal Panel        -> Block(80)
 80 Rusted Metal       -> Block(81)
 81 Copper             -> Block(82)
 82 Weathered Copper   -> Block(83)
 83 Gold Block         -> Block(84)
 84 Prismarine         -> Block(85)
 85 Dark Prismarine    -> Block(86)
 86 Ice                -> Block(87)
 87 Packed Ice         -> Block(88)
 88 Water              -> Block(5)

Shapes — slabs
 89 Stone Slab           -> Slab{0,1}
 90 Stone Brick Slab     -> Slab{1,21}
 91 Cobblestone Slab     -> Slab{2,11}
 92 Slate Slab           -> Slab{3,24}
 93 Marble Slab          -> Slab{4,30}
 94 Limestone Brick Slab -> Slab{5,29}
 95 Sandstone Slab       -> Slab{6,32}
 96 Red Brick Slab       -> Slab{7,10}
 97 Oak Slab             -> Slab{8,8}
 98 Birch Slab           -> Slab{9,41}
 99 Spruce Slab          -> Slab{10,42}
100 Dark Oak Slab        -> Slab{11,43}

Shapes — stairs
101 Stone Stairs           -> Stair{0}
102 Stone Brick Stairs     -> Stair{1}
103 Cobblestone Stairs     -> Stair{2}
104 Slate Stairs           -> Stair{3}
105 Marble Stairs          -> Stair{4}
106 Limestone Brick Stairs -> Stair{5}
107 Sandstone Stairs       -> Stair{6}
108 Red Brick Stairs       -> Stair{7}
109 Oak Stairs             -> Stair{8}
110 Birch Stairs           -> Stair{9}
111 Spruce Stairs          -> Stair{10}
112 Dark Oak Stairs        -> Stair{11}

Shapes — panes
113 Clear Glass Pane -> Pane{0}
114 White Glass Pane -> Pane{1}
115 Red Glass Pane   -> Pane{2}
116 Green Glass Pane -> Pane{3}
117 Cyan Glass Pane  -> Pane{4}
118 Blue Glass Pane  -> Pane{5}

Shapes — fences
119 Oak Fence      -> Fence{0}
120 Birch Fence    -> Fence{1}
121 Spruce Fence   -> Fence{2}
122 Dark Oak Fence -> Fence{3}
```

기본 hotbar item ID:

```rust
pub const DEFAULT_HOTBAR_ITEMS: [ItemId; 9] = [4, 5, 14, 35, 63, 89, 101, 113, 74];
```

M6의 `world::block::HOTBAR: [BlockId; 9]`는 회귀 테스트와 save migration을 위해 삭제·개명하지 않는다. M10 runtime input/UI는 그것을 사용하지 않고 `DEFAULT_HOTBAR_ITEMS`와 settings의 `creative.hotbar`만 사용한다.

`search_terms`는 임의 동의어를 넣지 않는다. 각 item에 다음 토큰만 붙인다.

```text
공통: category의 영문 표시명을 lowercase
Block: "block"
AxisLog: "log wood axis"
Slab: "shape slab half"
Stair: "shape stair steps"
Pane: "shape pane glass"
Fence: "shape fence wood"
emission>0: "light glowing emissive"
translucent: "transparent"
```

`icon_block`은 Block placement면 그 BlockId, AxisLog면 Y variant, Slab면 bottom state, Stair면 South·bottom state, Pane/Fence면 mask 15다. icon array layer는 `ItemId-1`이다.

#### 20.6.4 BlockDef 확장

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderClass {
    Opaque,
    Cutout,
    Translucent,
    Water,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Cube,
    Slab { top: bool },
    Stair { facing: Facing, upside_down: bool },
    Pane { mask: u8 },
    Fence { mask: u8 },
}

pub struct BlockDef {
    pub name: &'static str,
    pub solid: bool,
    pub opaque: bool,             // full cube visibility semantics only
    pub translucent: bool,
    pub textures: [u16; 6],
    pub emission: u8,
    pub emission_rgb: [f32; 3],
    pub light_blocking: bool,
    pub render_class: RenderClass,
    pub shape: ShapeKind,
    pub pick_item: ItemId,
}
```

render/light/collision 분류는 다음으로 고정한다.

| 대상 | solid | render_class | light_blocking | opaque/translucent |
|---|---|---|---|---|
| AIR | false | 렌더 안 함 | false | false/false |
| WATER | false | Water | false | false/true |
| full GLASS·ICE·PACKED_ICE | true | Translucent | false | false/true |
| glass pane | true | Translucent | false | false/true |
| leaves 4종 | true | Cutout | false | false/false |
| slab·stair·fence | true | Opaque | false | false/false |
| 그 외 full cube | true | Opaque | true | true/false |

`light_blocking=true`인 것은 opaque full cube뿐이다. slab, stair, pane, fence, leaves, glass, ice, water는 false다. M6 solver는 `opaque`가 아니라 `light_blocking`을 사용하도록 바꾼다. 기존 full opaque 블록의 결과는 bitwise 동일해야 한다. `MAX_BLOCK_ID = 959`이며 registry는 0..=959를 포함하고 예약 ID에는 AIR definition을 넣는다.

#### 20.6.5 1/16 형상 표현

모든 non-cube 형상은 16³ occupancy로 정의한다.

```rust
pub const SUBVOXEL: i32 = 16;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ShapeMask {
    pub bits: [u64; 64], // 4096 bits, index x + 16*(z + 16*y)
}

pub struct ShapeTemplate {
    pub mask: ShapeMask,
    pub quads: Vec<TemplateQuad>,
    pub boundary: [[u16; 16]; 6], // 각 face의 16행 coverage
    pub collision: SmallAabbList,
}
```

새 크레이트를 추가하지 않는다. `SmallAabbList`는 `[Aabb; 12] + len` 고정 배열로 구현한다.

occupancy:

- Cube: x,y,z 0..16.
- bottom slab: y 0..8.
- top slab: y 8..16.
- bottom stair: y 0..8 전체 + y 8..16의 high half.
- upside-down stair: y 8..16 전체 + y 0..8의 high half.
- North high half: z 0..8.
- East high half: x 8..16.
- South high half: z 8..16.
- West high half: x 0..8.
- pane core: x 7..9, y 0..16, z 7..9.
- pane arm N: x 7..9, z 0..7; E: x 9..16,z 7..9; S: x 7..9,z 9..16; W: x 0..7,z 7..9.
- fence post: x 6..10,y 0..16,z 6..10.
- fence rail N: x 6..10,z 0..6,y 5..8 및 y 11..14. E/S/W는 회전.

ShapeTemplate quads는 occupancy 16³에 3축 greedy를 한 번 실행해 만든다. 텍스처 UV는 월드 블록 단위이므로 1/16 좌표를 그대로 16으로 나눈다. 서로 다른 재질은 같은 기하 template을 공유한다.

#### 20.6.6 정점 패킹의 남은 비트 사용

D5와 M6 light 비트를 유지한다.

```text
a:
  x[0..6) y[6..12) z[12..18) face[18..21) ao[21..23)
  lowered[23] frac_x[24..28) frac_y[28..32)

b:
  tex[0..16) block_light[16..20) sky_light[20..24)
  frac_z[24..28) reserved[28..32)
```

```text
position = vec3(integer) + vec3(frac_x,frac_y,frac_z)/16
position.y -= lowered ? 0.125 : 0
```

정수 좌표 32에서 frac은 반드시 0이다. pack 함수는 `integer==32 && frac!=0`을 오류로 본다. 기존 cube 정점은 frac 0이므로 M6 snapshot과 동일하다.

#### 20.6.7 partial face culling

full-cube greedy 셀의 이웃 face coverage가:

- 0이면 기존 greedy에 포함.
- 0xffff×16이고 occluding이면 제거.
- 일부이면 일반 greedy에서 제외하고 `full_mask - neighbor_coverage`를 2D greedy해 subface quads를 만든다.

non-cube boundary face는 `shape_coverage - neighbor_coverage`를 2D greedy한다. 내부 plane face는 그대로 출력한다.

occluding 규칙:

1. Opaque full/partial shape는 occupancy가 덮는 부분을 occlude.
2. Cutout leaves는 이웃을 occlude하지 않는다.
3. 같은 glass family끼리는 겹치는 coverage를 occlude.
4. 같은 pane family끼리는 겹치는 coverage를 occlude.
5. WATER-WATER는 기존처럼 내부 면 제거.
6. fence끼리는 같은 material 여부와 관계없이 겹치는 rail/post face를 occlude.

micro AO는 face 밖 1 subvoxel의 side1·side2·corner occupancy를 읽고 기존 0..3 공식을 쓴다. 이 샘플은 현재 블록과 26 이웃의 `ShapeMask`를 조회한다. `PaddedChunk`의 블록 ID로 shape mask를 찾으며 새 World lock을 만들지 않는다. partial vertex의 block/sky light는 vertex에서 face normal 방향으로 `1/32`블록 이동한 점을 기준으로 주변 네 block cell을 선택해 M6의 정수 반올림 평균 `(sum+2)/4`를 쓴다. 같은 geometric vertex를 공유하는 template quad는 동일 light 값을 가져야 한다.

#### 20.6.8 충돌·레이캐스트

렌더 occupancy와 충돌 AABB:

- cube/slab/stair/pane는 occupancy bounding boxes와 동일.
- fence 렌더 높이는 1.0블록, collision은 각 post/arm AABB의 max_y를 1.5블록으로 확장한다.
- pane collision은 렌더 box와 동일.

기존 player swept AABB는 대상 블록의 `collision_boxes(id)`를 순회한다. 한 tick에서 검사하는 AABB 상한은 broad-phase block 64개 × block당 12개다. 상한 초과 시 panic하지 않고 추가 empty box를 무시하지도 않는다. broad-phase 범위를 축별로 쪼개 전체를 처리한다.

raycast:

1. Amanatides–Woo로 block cell 순회.
2. cell이 AIR가 아니면 해당 shape의 local AABB들을 slab ray test.
3. `t`가 현재 cell entry 이상이고 cell exit 이하인 가장 가까운 hit를 선택.
4. shape AABB를 모두 miss하면 DDA를 계속한다.
5. 반환 normal은 맞은 AABB face normal, `local_hit`은 0..1 위치다.

```rust
pub struct RayHit {
    pub block: IVec3,
    pub normal: IVec3,
    pub distance: f32,
    pub local_hit: Vec3,
    pub block_id: BlockId,
}
```

#### 20.6.9 배치 규칙

AxisLog:

```text
clicked normal ±X → x variant
clicked normal ±Y → y variant
clicked normal ±Z → z variant
```

Slab:

- clicked cell에 같은 material bottom slab가 있고 `normal=+Y` 또는 local_hit.y>0.5이면 full block으로 merge.
- 같은 material top slab가 있고 `normal=-Y` 또는 local_hit.y<0.5이면 full merge.
- 새 adjacent cell:
  - normal=-Y → top.
  - normal=+Y → bottom.
  - side face → local_hit.y>0.5이면 top, 아니면 bottom.

Stair:

```text
camera horizontal forward의 절댓값이 큰 축을 고른다.
high-side facing = cardinal(-camera_forward_xz)
upside_down = normal==-Y OR (normal.y==0 AND local_hit.y>0.5)
```

Pane/Fence 연결:

- 같은 family state와 연결.
- pane는 opaque full-face cube 및 모든 pane family와 연결.
- fence는 opaque full-face cube 및 모든 fence family와 연결.
- slab/stair에는 연결하지 않는다.
- 배치·파괴 뒤 edited cell과 N/E/S/W 4셀의 mask를 재계산한다.
- 자동 state 변화도 save 대상 modified로 기록하고 콘텐츠 version을 올린다.
- 한 사용자 edit에서 같은 청크 version은 모든 state 변화가 끝난 뒤 한 번만 증가한다.

Middle mouse pick:

- `pick_item`이 hotbar에 있으면 그 slot 선택.
- 없으면 현재 선택 slot을 `pick_item`으로 교체.
- AIR는 아무 동작 없음.

M8 LOD 입력에는 다음 `lod_equivalent(id)`를 먼저 적용한다.

```text
full cube → 자기 ID
axis log X/Z → 같은 family Y log
slab/stair → 해당 material의 full block
pane/fence → AIR
예약/invalid → AIR
```

M9 level-0 clipmap에서 shape material alpha는 `occupied_subvoxels/4096`이다. DDA hit threshold 0.5를 유지하므로 slab(0.5)와 stair(0.75)는 GI occluder, pane/fence는 GI 비차폐다. level 1 이상은 `lod_equivalent` 결과를 쓴다. shape edit는 기존 네 clipmap level을 모두 invalidation한다.

저장 포맷은 계속 `VFC1`의 `u16` block 배열이다. concrete state BlockId를 그대로 저장하며 version 2 chunk format을 만들지 않는다.

### 20.7 절차 재질 사양

#### 20.7.0 material-layer 이름과 추가 순서

현재 M6 `TEXTURES`의 기존 이름·숫자 layer는 immutable prefix다. 어떤 항목도 재배치하지 않는다. 새 layer는 아래 이름을 **줄 순서대로** append한다. 이미 prefix에 같은 이름이 있으면 중복 추가하지 않고 기존 layer를 재사용한다. shape state는 새 layer를 만들지 않고 base material layer를 재사용한다.

```text
dirt_path
gravel
red_sand
clay
mud
snow
moss
polished_stone
stone_bricks
mossy_stone_bricks
chiseled_stone
slate
polished_slate
basalt
polished_basalt
limestone
limestone_bricks
marble
marble_tiles
sandstone
cut_sandstone
red_sandstone
quartz
quartz_tiles
obsidian
birch_log_side
birch_log_top
spruce_log_side
spruce_log_top
dark_oak_log_side
dark_oak_log_top
birch_planks
spruce_planks
dark_oak_planks
birch_leaves
spruce_leaves
dark_oak_leaves
bookshelf_side
crate_side
crate_top
barrel_side
barrel_top
hay_side
hay_top
white_concrete
light_gray_concrete
gray_concrete
black_concrete
brown_concrete
red_concrete
orange_concrete
yellow_concrete
lime_concrete
green_concrete
cyan_concrete
light_blue_concrete
blue_concrete
purple_concrete
magenta_concrete
pink_concrete
white_glass
red_glass
green_glass
cyan_glass
blue_glass
glowstone
sea_lantern
warm_lamp
cold_lamp
glow_panel
roof_tile
checker_tile
ceramic_tile
metal_panel
rusted_metal
copper
weathered_copper
gold_block
prismarine
dark_prismarine
ice
packed_ice
```

face mapping:

- axis log: 축에 수직인 두 face는 `<wood>_log_top`, 나머지 네 face는 `<wood>_log_side`. Oak는 기존 log top/side layer를 재사용한다.
- BOOKSHELF: ±X/±Z `bookshelf_side`, ±Y oak planks.
- CRATE: +Y/-Y `crate_top`, 나머지 `crate_side`.
- BARREL: +Y/-Y `barrel_top`, 나머지 `barrel_side`.
- HAY_BALE: +Y/-Y `hay_top`, 나머지 `hay_side`.
- GRASS·기존 Oak log 등 prefix block은 기존 mapping 유지.
- slab/stair는 대응 full block의 face mapping.
- pane는 대응 full glass의 같은 layer를 여섯 face에 사용.
- fence는 대응 plank layer를 여섯 face에 사용.
- GLOW_PANEL은 여섯 face 모두 `glow_panel`.

append 뒤 전체 layer 수는 128 이하여야 한다. `TEXTURES`, albedo/material/emission 배열, `MaterialGpu` LUT가 이 순서를 공유한다.

#### 20.7.1 공통 생성기

모든 texture layer는 외부 PNG가 없을 때 `TextureRecipe`로 생성한다.

```rust
pub enum TexturePattern {
    StoneNoise,
    Cobble,
    Brick,
    Tile,
    Concrete,
    Plank,
    LogSide,
    LogTop,
    Leaves,
    Glass,
    MetalPanel,
    Organic,
    Ice,
    EmissiveGrid,
    Bookshelf,
    Crate,
    Barrel,
    Hay,
}

pub struct TextureRecipe {
    pub name: &'static str,
    pub pattern: TexturePattern,
    pub base_srgb: [u8; 3],
    pub accent_srgb: [u8; 3],
    pub mortar_srgb: [u8; 3],
    pub roughness: u8,
    pub metallic: u8,
    pub normal_strength: f32,
    pub height_scale: f32,
    pub emission: u8,
}
```

hash:

```text
v = x*0x1f123bb5 ^ y*0x05491333 ^ seed*0x9e3779b9
v ^= v>>16; v *= 0x7feb352d; v ^= v>>15; v *= 0x846ca68b; v ^= v>>16
noise = (v & 0xffff)/65535
```

pattern 규칙:

- StoneNoise: `t=0.65+0.35*(0.6*n(x,y)+0.3*n(x/2,y/2)+0.1*n(x/4,y/4))`, height=t.
- Cobble: 4×4 Voronoi cell, cell border 거리 <0.11이면 mortar, cell마다 hash tint ±12%, height mortar 0.15/stone 0.65~1.
- Brick: row height 4, brick width 8, 홀수 row x offset 4, 1px mortar, height mortar 0.10/brick 0.75+noise*0.20.
- Tile: 4×4 또는 8×4 grid를 seed bit로 선택, 1px joint, height joint 0.15/tile 0.80.
- Concrete: base와 accent를 noise 0.35 이내로 혼합, height 0.48~0.55.
- Plank: 4px board, 1px seam, grain `sin((x+noise*2)*1.7)`, seam height 0.12, wood 0.65~0.90.
- LogSide: 세로 grain, 4px마다 dark line; LogTop: 중심 `(7.5,7.5)` 반경 ring `sin(r*2.4+noise)`.
- Leaves: alpha=0 if hash<0.30, 아니면 255; height 0.4~1.0.
- Glass: interior alpha 48, 1px border alpha 128, height 0.50, roughness recipe.
- MetalPanel: 8×8 panel seam 1px, corner bolt at `(1,1),(6,1),(1,6),(6,6)`, metallic recipe.
- EmissiveGrid: 1px dark frame, 안쪽 emission mask recipe.emission.
- Ice: diagonal 1px crack 두 개, alpha 160, normal strength 0.20.
- Bookshelf/Crate/Barrel/Hay는 이름 그대로 4px 반복 구조를 사용하고 seed로 색만 ±8% 변동.

#### 20.7.2 재질 팔레트

RGB는 sRGB 0..255다. `mortar`가 의미 없는 pattern은 base의 70%로 둔다.

| 재질군 | pattern | base | accent | mortar | rough | metal | normal | height | emit |
|---|---|---|---|---|---:|---:|---:|---:|---:|
| stone | StoneNoise | 118,122,126 | 151,154,157 | 82,84,88 | 218 | 0 | 1.3 | .025 | 0 |
| cobble | Cobble | 112,116,118 | 148,151,152 | 70,72,73 | 238 | 0 | 1.8 | .045 | 0 |
| polished stone | Tile | 132,136,140 | 155,158,161 | 104,106,109 | 165 | 0 | .8 | .018 | 0 |
| stone brick | Brick | 119,123,126 | 146,149,151 | 76,78,80 | 215 | 0 | 1.5 | .040 | 0 |
| mossy stone brick | Brick | 103,119,99 | 132,142,120 | 68,74,66 | 225 | 0 | 1.6 | .040 | 0 |
| slate | Tile | 61,69,78 | 90,98,107 | 42,46,52 | 205 | 0 | 1.2 | .025 | 0 |
| basalt | StoneNoise | 45,47,52 | 72,74,79 | 28,29,32 | 220 | 0 | 1.5 | .032 | 0 |
| limestone | StoneNoise | 188,178,145 | 220,208,169 | 143,134,108 | 210 | 0 | 1.0 | .025 | 0 |
| marble | StoneNoise | 218,216,207 | 166,176,184 | 190,188,180 | 135 | 0 | .9 | .018 | 0 |
| sandstone | StoneNoise | 201,174,112 | 229,204,143 | 158,132,83 | 224 | 0 | 1.2 | .030 | 0 |
| red sandstone | StoneNoise | 177,91,48 | 213,124,68 | 132,63,33 | 225 | 0 | 1.2 | .030 | 0 |
| red brick | Brick | 151,66,48 | 190,91,65 | 91,85,78 | 215 | 0 | 1.7 | .045 | 0 |
| quartz | Tile | 221,217,205 | 244,241,231 | 178,174,165 | 135 | 0 | .8 | .016 | 0 |
| obsidian | StoneNoise | 24,18,35 | 55,37,76 | 15,12,22 | 90 | 0 | .7 | .012 | 0 |
| oak plank | Plank | 153,108,59 | 196,145,82 | 91,62,34 | 175 | 0 | 1.2 | .025 | 0 |
| birch plank | Plank | 202,181,132 | 230,211,161 | 151,131,91 | 170 | 0 | 1.0 | .022 | 0 |
| spruce plank | Plank | 105,72,44 | 141,101,62 | 63,42,26 | 185 | 0 | 1.2 | .026 | 0 |
| dark oak plank | Plank | 67,45,31 | 98,67,43 | 39,27,19 | 190 | 0 | 1.2 | .026 | 0 |
| concrete white | Concrete | 208,210,208 | 225,226,224 | 151,153,151 | 230 | 0 | .25 | .004 | 0 |
| concrete light gray | Concrete | 151,155,158 | 168,172,175 | 109,112,114 | 230 | 0 | .25 | .004 | 0 |
| concrete gray | Concrete | 91,96,101 | 111,116,121 | 64,68,72 | 230 | 0 | .25 | .004 | 0 |
| concrete black | Concrete | 31,34,38 | 49,53,58 | 20,22,25 | 225 | 0 | .25 | .004 | 0 |
| concrete brown | Concrete | 112,69,46 | 139,88,58 | 79,47,32 | 230 | 0 | .25 | .004 | 0 |
| concrete red | Concrete | 172,48,45 | 204,67,60 | 120,32,30 | 230 | 0 | .25 | .004 | 0 |
| concrete orange | Concrete | 216,112,36 | 239,137,53 | 157,78,23 | 230 | 0 | .25 | .004 | 0 |
| concrete yellow | Concrete | 229,190,47 | 246,214,72 | 166,136,29 | 230 | 0 | .25 | .004 | 0 |
| concrete lime | Concrete | 121,183,47 | 147,210,69 | 84,128,30 | 230 | 0 | .25 | .004 | 0 |
| concrete green | Concrete | 48,126,57 | 66,153,75 | 31,88,38 | 230 | 0 | .25 | .004 | 0 |
| concrete cyan | Concrete | 38,146,159 | 57,174,187 | 25,102,112 | 230 | 0 | .25 | .004 | 0 |
| concrete light blue | Concrete | 77,151,209 | 105,177,231 | 52,105,148 | 230 | 0 | .25 | .004 | 0 |
| concrete blue | Concrete | 51,76,174 | 69,99,205 | 34,50,122 | 230 | 0 | .25 | .004 | 0 |
| concrete purple | Concrete | 111,61,171 | 137,80,201 | 76,40,120 | 230 | 0 | .25 | .004 | 0 |
| concrete magenta | Concrete | 181,62,166 | 208,83,193 | 128,40,117 | 230 | 0 | .25 | .004 | 0 |
| concrete pink | Concrete | 226,126,159 | 244,153,184 | 171,88,116 | 230 | 0 | .25 | .004 | 0 |
| clear glass | Glass | 190,224,232 | 226,246,250 | 133,176,187 | 20 | 0 | .25 | .002 | 0 |
| white glass | Glass | 225,228,226 | 248,250,249 | 173,178,176 | 26 | 0 | .25 | .002 | 0 |
| red glass | Glass | 204,74,65 | 239,113,102 | 147,45,40 | 28 | 0 | .25 | .002 | 0 |
| green glass | Glass | 71,179,91 | 106,215,125 | 43,125,60 | 28 | 0 | .25 | .002 | 0 |
| cyan glass | Glass | 55,187,205 | 91,223,238 | 34,131,145 | 25 | 0 | .25 | .002 | 0 |
| blue glass | Glass | 69,111,211 | 103,146,239 | 43,73,153 | 25 | 0 | .25 | .002 | 0 |
| metal panel | MetalPanel | 113,121,128 | 159,166,171 | 70,75,80 | 92 | 230 | 1.0 | .020 | 0 |
| rusted metal | MetalPanel | 132,74,42 | 174,105,59 | 76,47,31 | 172 | 191 | 1.3 | .030 | 0 |
| copper | MetalPanel | 183,105,61 | 222,142,86 | 110,61,37 | 104 | 230 | .9 | .018 | 0 |
| weathered copper | MetalPanel | 70,139,120 | 104,174,151 | 43,91,79 | 150 | 230 | 1.0 | .020 | 0 |
| gold | MetalPanel | 224,170,42 | 249,210,80 | 156,111,25 | 82 | 255 | .7 | .015 | 0 |
| glowstone | Organic | 213,151,62 | 255,211,111 | 116,74,33 | 145 | 0 | 1.2 | .025 | 220 |
| sea lantern | Tile | 154,220,211 | 222,255,244 | 81,139,136 | 90 | 0 | .8 | .015 | 235 |
| warm lamp | EmissiveGrid | 235,160,72 | 255,219,135 | 67,47,31 | 80 | 0 | .7 | .012 | 255 |
| cold lamp | EmissiveGrid | 155,210,247 | 222,244,255 | 43,59,72 | 75 | 0 | .7 | .012 | 255 |
| glow panel | EmissiveGrid | 207,217,219 | 255,255,255 | 58,62,65 | 70 | 0 | .5 | .008 | 245 |
| ice | Ice | 142,205,232 | 206,238,250 | 89,155,188 | 35 | 0 | .3 | .006 | 0 |

표에 없는 세부 블록은 가장 가까운 재질군을 사용한다. 정확한 매핑:

- DIRT, DIRT_PATH, MUD, CLAY, GRAVEL, SAND, RED_SAND, SNOW, MOSS는 `Organic` 또는 `StoneNoise`에 이름별 base color만 적용한다.
- 각 log의 side/top은 같은 wood palette의 `LogSide`/`LogTop`.
- leaves는 wood별 base `(72,132,61)`, `(112,156,73)`, `(55,103,50)`, `(48,84,44)`.
- BOOKSHELF, CRATE, BARREL, HAY_BALE는 해당 전용 pattern과 oak/hay palette.
- ROOF_TILE `(124,49,39)/(171,69,52)`, CHECKER_TILE `(218,216,207)/(49,53,59)`, CERAMIC_TILE `(184,204,207)/(225,237,238)`.
- PRISMARINE `(73,155,145)/(111,190,177)`, DARK_PRISMARINE `(42,73,68)/(59,101,93)`.
- PACKED_ICE는 ice base를 15% 밝게 하고 alpha 220.

### 20.8 I 인벤토리·HUD·아이콘

#### 20.8.1 상태와 입력

```rust
pub struct CreativeInventory {
    pub open: bool,
    pub category: Option<ItemCategory>, // None=All
    pub query: String,                  // ASCII lowercase, max 32 bytes
    pub scroll_row: u16,
    pub hovered: Option<ItemId>,
    pub hotbar: [ItemId; 9],
    pub selected_slot: u8,
}
```

입력:

- `I`: inventory 열기/닫기.
- inventory open 중 `Escape`: inventory 닫기. pause menu를 열지 않는다.
- inventory closed 중 `Escape`: pause menu.
- inventory open 동안 cursor unlock·visible, player physics·world time 정지, streaming 결과 apply·save·shader reload는 계속.
- inventory closed에서 숫자 1..9는 slot 선택, mouse wheel은 slot을 mod 9로 순환한다. 둘 다 ItemId가 실제로 바뀌면 Switch action을 시작한다.
- `Q` drop과 world item entity는 구현하지 않는다.
- 글자 입력은 winit text event의 ASCII 32..126만 받아 lowercase로 저장. 한글 IME는 M10 범위가 아니다.
- `Backspace`: query 마지막 Unicode scalar 제거. 실제 입력이 ASCII라 1바이트다.
- `Ctrl+F`: search field focus.
- mouse wheel 한 notch: 3행 이동.
- item left click: 현재 selected hotbar slot에 배치, inventory 유지.
- hovered item 위 숫자 1..9: 해당 hotbar slot에 배치.
- search field focus 중 숫자 키는 pointer가 item 위에 있을 때 hotbar 배치가 우선하고, item 위가 아니면 query에 입력한다.
- hotbar slot left click: selected slot 변경.
- empty grid click: 동작 없음.
- tooltip: hover 0.35초 뒤 item English name.

검색은 `name + search_terms`에 대한 ASCII lowercase substring이다. category filter 뒤 검색한다. 결과 순서는 ItemId 오름차순이다.

#### 20.8.2 레이아웃

기준 canvas 1280×720. 실제 출력에는

```text
ui_scale = clamp(settings.ui.scale,0.75,1.50)
S = max(1, round(output_h/720 * ui_scale))
```

을 곱한다. design 좌표의 실제 위치는 `offset_x=(output_w/S-1280)/2`, `offset_y=(output_h/S-720)/2`를 더한 뒤 S를 곱하고 physical pixel로 round한다. offset이 음수이면 0으로 clamp하고 panel에는 clip rect를 적용한다.

Inventory panel:

```text
panel x=174, y=66, w=932, h=588
background rgba=(18,22,30,235)
border outer #0b0d12 2px, inner #5f6878 1px
```

- title `(202,88)`: `BUILD INVENTORY`.
- search `(568,82,w=502,h=34)`.
- category column `(198,132,w=148)`.
- category button `148×42`, gap 6, 순서 All/Terrain/Masonry/Wood/Color/Glass & Light/Detail/Shapes.
- grid origin `(374,132)`.
- 9 columns ×6 rows.
- cell 66×66, gap 6.
- visible item 54개.
- scroll bar x=1029,y=132,w=12,h=426.
- hotbar preview y=584.

HUD hotbar는 inventory closed/open 모두 보인다.

```text
slot outer size = 44*S
slot gap = 4*S
bottom margin = 18*S
selected border = 3*S, rgba(245,245,245,255)
normal border = 1*S, rgba(105,112,125,255)
background = rgba(12,14,19,205)
```

crosshair는 중앙 9×9 design pixel, 가운데 1px hole, white core와 black 1px outline다.

inventory/pause open 시 world LDR를 quarter-resolution으로 downsample하고 9-tap separable Gaussian blur sigma 2.0을 적용한 뒤 black alpha 0.35를 덮는다. blur GPU p95 ≤0.25ms.

#### 20.8.3 item icon bake

시작 시 122개 item icon을 64×64×122 `Rgba8UnormSrgb` array에 굽는다.

1. item별 대표 BlockId/ShapeTemplate mesh.
2. 128×128 `Rgba16Float` 임시 target, `Depth32Float`.
3. orthographic camera yaw -45°, pitch 30°, scale 1.25.
4. key light normalize `(-0.4,-1,-0.3)`, intensity 2.5.
5. fill `(0.22,0.27,0.34)`.
6. alpha 0 clear.
7. 2×2 box downsample render pass로 64×64 array layer에 기록.
8. 모든 122 layer가 끝난 뒤 임시 target 제거.

icon은 world time, day/night, GI 영향을 받지 않는다. emissive block은 exposure 1.0 ACES 후 core가 255를 넘지 않는다.

icon bake wall time Apple M5 release ≤500ms, array memory 약 2MiB다.

### 20.9 1인칭 손·held item

#### 20.9.1 렌더 위치

viewmodel은 native LDR world 결과 뒤, UI 앞에 그린다. TAAU history·depth와 분리한다.

- target: native `Rgba8UnormSrgb` LDR.
- depth: native `Depth32Float`, 매 프레임 1.0 clear.
- viewmodel FOV 68°.
- near 0.01, far 10.
- depth write on, compare Less.
- cull Back.
- world depth와 비교하지 않는다.
- viewmodel shader 내부에서 linear PBR → exposure 1.0 → 같은 ACES·color grade를 적용한다.

#### 20.9.2 손 모델

오른팔 하나를 6면 cuboid로 만든다.

```text
size = (0.26,0.72,0.26)
arm pivot = top center
base translation camera space = (0.58,-0.52,-0.88)
base rotation degrees = (-22,-28,-8)
```

기본 texture 경로 `assets/textures/player/hand.png`, 16×16 RGBA. 없으면 절차 생성:

- skin base `(196,139,101)`.
- light `(224,169,126)`.
- shadow `(145,91,65)`.
- sleeve는 아래 42%를 `(54,78,122)`.
- 1px noise ±6, alpha 255.

held item:

```text
base translation = (0.38,-0.34,-0.66)
base rotation degrees = (18,-38,8)
scale cube/full item = 0.34
scale slab/stair = 0.38
scale pane/fence = 0.46
```

item mesh는 world ShapeTemplate를 재사용하되 별도 f32 `ViewVertex`로 변환해 작은 buffer cache에 저장한다. 122개 전체 GPU cache 상한 4MiB.

viewmodel lighting:

```text
sky_curve = LIGHT_LEVEL[player_head_sky] * sun_factor
block_curve = LIGHT_LEVEL[player_head_block]
ambient = sky_color*(0.18+0.32*sky_curve)
        + vec3(1.0,0.58,0.28)*(0.08+0.28*block_curve)
direct = GGX(N,V,-sun_dir) * sun_color * 1.6 * sky_curve
color = material*ambient + direct + emission
```

viewmodel은 CSM을 샘플하지 않는다. 손이 갑자기 검게 되는 것을 막기 위해 최종 ambient 각 채널은 최소 0.06이다. inventory 또는 pause menu가 open이면 viewmodel draw를 완전히 생략한다.

#### 20.9.3 애니메이션

```rust
pub enum HandAction {
    Idle,
    Break { elapsed: f32 },
    Place { elapsed: f32 },
    Switch { elapsed: f32, from: ItemId, to: ItemId },
}
```

idle:

```text
translation += (sin(t*0.8)*0.008, sin(t*1.6)*0.006,0)
rotation.z += sin(t*0.8)*0.8°
```

walk phase는 지면 수평 누적 거리다.

```text
phase += horizontal_distance * π/0.90
translation.x += sin(phase)*0.025
translation.y += abs(cos(phase))*0.018
rotation.z += sin(phase)*2.5°
```

sprint 계수 1.35. 공중·비행·inventory/pause 중 walk bob은 0.15초 half-life로 0에 감쇠한다.

Break duration 0.26초:

```text
p=clamp(elapsed/.26,0,1)
s=sin(π*p)
translation += (-0.10*s,-0.05*s,0.06*s)
rotation += (-72*s, 18*s, 24*s) degrees
```

Place duration 0.18초:

```text
p=elapsed/.18
s=sin(π*p)
translation += (0,-0.03*s,-0.16*s)
rotation += (18*s,0,-8*s)
```

Switch duration 0.20초:

```text
p=elapsed/.20
y_offset = -0.48 * (1-(2p-1)^2) for p<0.5/after item swap
item changes at p=0.5
```

새 action 우선순위: Place > Break > Switch > Idle. 같은 프레임에 place와 break가 발생할 수 없도록 input dispatcher가 보장한다. animation time은 `GameClock`이 아니라 pause에 영향받는 `interaction_time`; pause/inventory 중 고정한다. Break는 실제 block 제거 성공 때만, Place는 collision/규칙 검사를 통과한 실제 설치 성공 때만 시작한다. Switch는 selected ItemId가 달라질 때만 시작한다. 실패한 edit는 손 action과 sound를 만들지 않는다.

hold repeat:

```text
LMB: press frame 즉시 1회, 0.22초 뒤부터 0.10초 간격
RMB: press frame 즉시 1회, 0.25초 뒤부터 0.12초 간격
한 frame 최대 world edit 1회
LMB/RMB 동시 hold면 RMB Place 우선
```

반복 성공마다 Break/Place animation elapsed를 0으로 다시 시작한다.

### 20.10 설정·사운드·셰이더팩·번들 보강

#### 20.10.1 settings version 2

키·마우스 binding은 하나의 enum으로 저장한다.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputBinding {
    Key(winit::keyboard::KeyCode),
    Mouse(winit::event::MouseButton),
}
```

`Serialize`/`Deserialize`는 derive하지 않고 문자열로 수동 구현한다. JSON 문자열 `KeyI`, `KeyW`, `Space`, `F2`, `MouseMiddle`를 명시적으로 parse/serialize한다. 알 수 없는 문자열은 해당 action의 기본 binding으로 되돌리고 warning한다. `pause=Escape`는 재바인딩할 수 없다.

기존 §19 schema를 다음으로 교체한다.

```json
{
  "version": 2,
  "video": {
    "preset": "m5_air_high",
    "render_scale": 0.72,
    "view_radius": 10,
    "taa": true,
    "sharpen": 0.18,
    "shadow_resolution": 2048,
    "shadow_distance": 224.0,
    "pom_steps": 8,
    "ssr_steps": 40,
    "volumetric_steps": 32,
    "cloud_view_steps": 40,
    "cloud_light_steps": 6,
    "gi_enabled": true,
    "gi_rays": 4,
    "gi_distance": 48.0,
    "vsync": true,
    "shader_pack": "builtin"
  },
  "controls": {
    "mouse_sensitivity": 0.002,
    "invert_y": false,
    "bindings": {
      "inventory": "KeyI",
      "forward": "KeyW",
      "backward": "KeyS",
      "left": "KeyA",
      "right": "KeyD",
      "jump": "Space",
      "sprint": "ControlLeft",
      "descend": "ShiftLeft",
      "toggle_fly": "KeyF",
      "pause": "Escape",
      "screenshot": "F2",
      "debug_view": "F4",
      "shader_reload": "KeyR",
      "pick_block": "MouseMiddle"
    }
  },
  "audio": { "master": 0.8, "effects": 0.8 },
  "ui": { "scale": 1.0 },
  "creative": {
    "hotbar": [4,5,14,35,63,89,101,113,74],
    "selected_slot": 0
  }
}
```

version 1 설정이 있으면 필드별 migrate하고 hotbar 기본값을 넣어 version 2로 atomic 저장한다. unsupported version은 기존 보존 규칙을 쓴다.

#### 20.10.2 사운드 추가

§19 절차 PCM을 유지하고 다음만 추가한다.

- inventory open/close: 55ms, 640→420Hz sine chirp, amplitude 0.08.
- hotbar switch: 35ms, 880Hz sine, amplitude 0.05.
- pane/fence place는 place PCM high-pass 계수 0.35.
- slab/stair place는 base place PCM.
- viewmodel action과 sound event는 같은 interaction event ID를 공유해 중복 재생을 막는다.

#### 20.10.3 셰이더팩 contract version 2

팩 경로에 다음을 허용한다.

```text
textures/blocks/<name>.png                 16×16 RGBA sRGB
textures/materials/<name>_material.png     16×16 RGBA linear
textures/emission/<name>_emission.png      16×16 grayscale
textures/player/hand.png                   16×16 RGBA sRGB
```

`pack.json.contract_version`은 2다. contract 1은 명확한 오류로 거부하고 builtin을 유지한다. shaderpack은 item/block registry와 shape geometry를 바꿀 수 없다.

#### 20.10.4 번들

§19 `.app` 구조를 유지한다. launcher는 `VF_ASSETS`, `VF_SAVE_ROOT`, `VF_SETTINGS`, `VF_SHADERPACKS`, `VF_SCREENSHOTS`를 설정한다. default save root 프로젝트 경로 계약은 개발 실행에서 유지되고, 앱 launcher 실행 때만 환경변수 우선순위로 Application Support를 쓴다.

### 20.11 테스트 계약

다음 이름을 그대로 추가한다.

```text
m5_air_high_internal_size_is_1848x1040
material_arrays_share_layer_order_and_mips
cutout_mips_preserve_coverage
pom_reference_intersection_matches_shader_contract
ggx_reference_is_finite_and_energy_bounded
motion_vectors_exclude_jitter
taau_rejects_disocclusion
taau_history_clamps_in_ycocg
taau_static_sequence_reduces_high_frequency_error
csm_update_cadence_matches_contract
gtao_flat_plane_is_unoccluded
gtao_corner_is_occluded
atmosphere_luts_are_finite

block_registry_ids_are_stable
item_catalog_has_exactly_122_unique_items
every_item_has_valid_icon_and_placement
shape_mask_counts_match_contract
shape_template_quads_cover_mask_surface
vertex_fraction_pack_roundtrip
cube_vertices_keep_zero_fraction
partial_neighbor_subtracts_only_covered_face_area
micro_ao_reads_shape_occupancy
stair_collision_matches_orientation
fence_collision_height_is_one_point_five
raycast_passes_through_empty_pane_region
raycast_hits_stair_step
axis_log_uses_clicked_axis
slab_pair_merges_to_full_block
stair_facing_is_opposite_camera_forward
pane_connections_update_five_cells
fence_connections_update_five_cells
connection_update_increments_chunk_version_once
pick_block_selects_existing_hotbar_slot
pick_block_replaces_selected_slot_when_absent

inventory_key_toggles_modal_state
inventory_escape_closes_before_pause
inventory_filter_order_is_stable
inventory_scroll_clamps
inventory_click_assigns_selected_hotbar_slot
inventory_number_assigns_requested_slot
settings_v1_migrates_to_v2
creative_hotbar_roundtrips
item_icon_bake_has_122_layers
item_icon_alpha_background_is_clear
hotbar_is_centered_at_1280x720
inventory_panel_matches_contract_bounds
viewmodel_action_priority_is_stable
viewmodel_break_duration_is_point_two_six
viewmodel_place_duration_is_point_one_eight
viewmodel_pauses_with_inventory
shaderpack_contract_one_is_rejected
shaderpack_contract_two_is_transactional
```

판정:

- 122개 ItemId가 1..122를 빈틈 없이 한 번씩 사용.
- 모든 item placement가 유효 BlockId 또는 상태 범위를 생성.
- bottom slab occupancy 2048, pane mask 0 occupancy 64, fence mask 0 occupancy 256.
- ShapeTemplate 표면 microface 집합과 occupancy의 exposed microface 집합이 bitwise 동일.
- partial cube face의 출력 면적 + neighbor coverage 면적 =256 subpixels.
- raycast pane 빈 영역은 다음 cell hit를 반환.
- connection edit 하나당 관련 각 chunk version 증가는 최대 1.
- icon layer alpha 0 픽셀 비율 30~80%, icon nontransparent bbox는 20×20 이상 60×60 이하.
- viewmodel animation transform은 pause 1초 전후 bitwise 동일.

### 20.12 스냅샷 검증

#### 20.12.1 M7 quality

```bash
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
```

`m7-materials` fixture는 stone, brick, wood, metal, gold, emissive의 정면 패널을 둔다. 각 패널 80×120 crop에서:

- metal/gold의 specular highlight max 선형 밝기는 stone의 1.25배 이상.
- rough stone highlight 면적은 polished stone의 1.50배 이상.
- emissive night crop은 emission off reference의 2.0배 이상.
- 모든 픽셀 finite, 검정 NaN 대체색 `(255,0,255)` 0개.

TAAU warmup32의 high-frequency residual은 warmup1의 70% 이하, 정적 edge의 10~90% rise width는 3.0 native pixel 이하.

#### 20.12.2 shape gallery

```bash
cargo run --release --bin snapshot -- \
  --fixture m10-shapes --size 1280x720 --render-scale 1.0 \
  --preset m5_air_high --fixed-exposure 1.0 --warmup 16 \
  --view final --out /tmp/vf_m10_shapes.png

cargo run --release --bin snapshot -- \
  --fixture m10-shapes --size 1280x720 --view normal \
  --out /tmp/vf_m10_shapes_normal.png
```

fixture는 bottom/top slab, 네 facing stair, pane mask 0/5/15, fence mask 0/5/15, X/Y/Z log를 고정 위치에 둔다. 프로브 계약은 fixture 코드의 `SnapshotProbe` JSON을 `/tmp/vf_m10_shapes.probes.json`에 함께 쓴다. 각 probe에는 `name`, `x`, `y`, `expected_linear_rgb`, `epsilon`이 있다. snapshot 바이너리가 PNG 저장 뒤 probe를 자체 검사하고 하나라도 실패하면 exit 1이다. 외부 사람 판정은 없다.

#### 20.12.3 inventory

```bash
cargo run --release --bin snapshot -- \
  --fixture m10-inventory --size 1280x720 --ui inventory \
  --inventory-category all --inventory-query "" \
  --out /tmp/vf_m10_inventory.png

cargo run --release --bin snapshot -- \
  --fixture m10-inventory --size 1280x720 --ui inventory \
  --inventory-category shapes --inventory-query "stair" \
  --out /tmp/vf_m10_inventory_stair.png
```

픽셀 판정:

```bash
python3 - <<'PY'
from PIL import Image
all_im=Image.open('/tmp/vf_m10_inventory.png').convert('RGBA')
stair=Image.open('/tmp/vf_m10_inventory_stair.png').convert('RGBA')
assert all_im.size==(1280,720)
# panel corner and center opacity
assert all_im.getpixel((174,66))[3] >= 220
assert all_im.getpixel((640,360))[3] >= 220
# grid region must contain many non-background icon colors
region=all_im.crop((374,132,1026,558)).convert('RGB')
unique=len(set(region.getdata()))
assert unique >= 512, unique
# filtered result shows 12 stair icons: count occupied cells by variance
cells=0
for row in range(6):
  for col in range(9):
    x=374+col*72; y=132+row*72
    c=stair.crop((x,y,x+66,y+66)).convert('RGB')
    vals=list(c.getdata())
    lo=min(sum(p) for p in vals); hi=max(sum(p) for p in vals)
    if hi-lo>80: cells+=1
assert cells==12, cells
PY
```

#### 20.12.4 hand

```bash
cargo run --release --bin snapshot -- \
  --fixture m10-viewmodel --size 1280x720 --hand-action idle \
  --held-item 101 --world-time 0.0 --out /tmp/vf_hand_idle.png
cargo run --release --bin snapshot -- \
  --fixture m10-viewmodel --size 1280x720 --hand-action break:0.13 \
  --held-item 101 --world-time 0.0 --out /tmp/vf_hand_break.png
cargo run --release --bin snapshot -- \
  --fixture m10-viewmodel --size 1280x720 --hand-action place:0.09 \
  --held-item 101 --world-time 0.0 --out /tmp/vf_hand_place.png
```

오른쪽 아래 crop `(700,310,1280,720)`에서 idle 대비 break/place 변경 픽셀 비율은 각각 2~30%, 왼쪽 위 crop `(0,0,500,300)` 변경 비율은 0.1% 이하다.

### 20.13 최종 성능 예산

Apple M5, 2560×1440, `m5_air_high`, R=10, 600 warmup + 3000 측정 frame:

| 항목 | p95 예산 |
|---|---:|
| M7 기반 렌더 | 8.10ms |
| M8 water/volumetric/cloud/LOD | 3.70ms |
| M9 GI | 3.20ms |
| viewmodel | 0.15ms |
| HUD | 0.18ms |
| inventory blur+UI(open) | 0.45ms |
| 최종 GPU, HUD 상태 | ≤15.40ms |
| 최종 GPU, inventory open | ≤15.75ms |
| CPU update+stream+draw encode | ≤4.0ms |
| 전체 frame p95 | ≤16.6ms |
| 전체 frame max | <25ms |
| 정착 | <5.0초 |
| 총 렌더·월드 추정 메모리 | <1.35GiB |

패스 timing JSON은 최소 다음 키를 가진다.

```text
shadow, gbuffer, depth_pyramid, gtao, atmosphere, deferred,
gi_trace, gi_temporal, gi_denoise, volumetric, clouds, cloud_shadow,
glass, water, bloom, exposure, taau_tonemap, viewmodel, ui, present,
total_gpu
```

budget 초과 시 기능을 제거하거나 preset 수치를 바꾸지 않는다. 다음 순서로 원인을 제거한다.

1. 잘못된 full-resolution intermediate.
2. 매 프레임 texture/pipeline/bind-group 생성.
3. 중복 Globals·chunk uniform.
4. 불필요한 LUT/cascade/cloud-shadow 갱신.
5. readback 대기·map blocking.
6. draw item 정렬·allocation.
7. shader texture sample 중복.
8. compute workgroup occupancy와 bounds branch.

### 20.14 M7~M10 분기 선결정 — 묻지 말고 이렇게

- 실행 흐름 → M7→M8→M9→M10 연속. 중간 사람 리뷰 대기 없음.
- M7 TAA → 선택이 아니라 TAAU 필수.
- 기본 대상 품질 → `m5_air_high`, 0.72 scale, 2560×1440 60fps.
- PBR → normal/roughness/height/emission texture arrays + GGX.
- SSAO → GTAO.
- high-quality 최적화 → temporal·cadence·quarter/half resolution. 품질 수치 임의 하향 금지.
- building catalog → 정확히 122 item.
- block storage → 계속 `u16`, 새 state도 concrete BlockId.
- sub-block precision → 정점 spare bits의 1/16 frac.
- shape → axis-aligned occupancy 16³; 임의 mesh 없음.
- shapes → axis log, slab, stair, pane, fence만.
- inventory → `I`, 9×6 grid, 검색·카테고리·scroll·hotbar assignment.
- inventory open → gameplay/world time pause, streaming apply/save/reload 계속.
- hand → 오른손 1개 + held item, native LDR, TAA history 제외.
- item quantities/crafting → 없음.
- settings → version 2, hotbar 포함.
- shaderpack → contract 2, item/shape registry 변경 불가.
- 중간 commit → Sol은 하지 않음. LOG에 의도한 메시지만 기록.
- 최종 정지 → M10 검증·LOG 완료 뒤 한 번만 `M7~M10 완료, 최종 리뷰 요청`.
