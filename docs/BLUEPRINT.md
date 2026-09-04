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
