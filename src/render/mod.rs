//! Surface-independent GPU rendering primitives.

pub mod chunk_pipeline;
pub mod globals;
pub mod gpu;
pub mod offscreen;
pub mod renderer;
pub mod shader_watch;
pub mod textures;

pub use chunk_pipeline::{ChunkPipeline, GpuChunk};
pub use globals::Globals;
pub use gpu::Gpu;
pub use offscreen::{OffscreenTarget, read_pixels};
pub use renderer::Renderer;
pub use shader_watch::ShaderWatcher;
pub use textures::BlockTextures;
