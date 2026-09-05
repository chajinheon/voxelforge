//! Windowed Voxelforge entrypoint for the M3 building milestone.

mod app;
mod app_config;
mod window_gpu;

use app::App;
use app_config::{load_shader_pack, parse_args, resolve_settings};
use voxelforge::audio::write_self_test_wav;
use voxelforge::world::save::SaveDir;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_core=warn,naga=warn"),
    )
    .init();
    let options = parse_args(std::env::args())?;
    if let Some(path) = options.audio_self_test.as_deref() {
        write_self_test_wav(path, 1)?;
        log::info!("audio self-test: wrote {}", path.display());
        return Ok(());
    }
    let smoke_frames = std::env::var("VF_SMOKE_FRAMES")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .or_else(|| {
            let frames = std::env::var("VF_BENCH_FRAMES")
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|value| *value > 0);
            if frames.is_some() {
                log::info!(
                    "benchmark: {} frames (autopilot={})",
                    frames.unwrap_or_default(),
                    std::env::var("VF_AUTOPILOT").unwrap_or_else(|_| "off".into())
                );
            }
            frames
        });
    let world_name = options.save_name.clone();
    let autopilot = std::env::var("VF_AUTOPILOT").ok();
    let save_dir = if autopilot.as_deref() == Some("creative-build") {
        SaveDir::open_at(&world_name, std::path::Path::new("/tmp/vf_m10_bench_save"))?
    } else {
        SaveDir::open(&world_name)?
    };
    let (settings, settings_path, runtime_overrides) = resolve_settings(&options)?;
    let shader_pack = load_shader_pack(&settings, &options)?;
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(
        smoke_frames,
        save_dir,
        settings,
        settings_path,
        shader_pack,
        runtime_overrides,
    );
    event_loop.run_app(&mut app)?;
    Ok(())
}
