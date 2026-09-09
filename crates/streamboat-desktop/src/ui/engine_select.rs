//! Which audio engine backs the desktop shell, chosen at compile time by
//! target OS (D-016: GStreamer on Linux, libmpv on Windows/macOS, one
//! `Engine` trait). The libmpv backend is being added by another agent in
//! `streamboat-player` behind that crate's own `mpv` feature, which does
//! not exist yet — this module's job is only the compile-time gate, so a
//! Windows/macOS build fails loudly and specifically today instead of
//! silently missing an engine, and so the call site here is already correct
//! once that backend lands: turn on this crate's own `mpv` feature (see
//! `Cargo.toml`) and the branch below starts referencing the real type.

#[cfg(not(any(target_os = "linux", feature = "mpv")))]
compile_error!(
    "streamboat-desktop has no audio engine for this target yet: GStreamer is Linux-only by \
     decision (D-016, D-020), and the libmpv backend for Windows/macOS is not merged. Build on \
     Linux, or enable the `mpv` feature once streamboat-player's libmpv backend lands."
);

use std::path::Path;
use std::sync::mpsc::Sender;

use streamboat_core::proto::OutputConfig;
use streamboat_player::{Engine, EngineEvent};

/// Builds the platform engine and returns it boxed behind the shared
/// [`Engine`] trait — the only thing `ui::app` depends on, per D-016's "one
/// engine trait" and the task brief's "the only engine seam the GUI may
/// use."
#[cfg(target_os = "linux")]
pub fn build(
    events: Sender<EngineEvent>,
    output: OutputConfig,
    runtime_dir: &Path,
) -> streamboat_player::engine::EngineResult<Box<dyn Engine>> {
    let engine = streamboat_player::GstEngine::new(events, output, runtime_dir.to_path_buf())?;
    Ok(Box::new(engine))
}

/// Placeholder call site for the libmpv backend (D-016). `MpvEngine` does
/// not exist in `streamboat-player` yet, so this function body is inert —
/// it only becomes reachable, and only compiles, once both that type lands
/// and this crate is built with `--features mpv` on a non-Linux target.
#[cfg(all(not(target_os = "linux"), feature = "mpv"))]
pub fn build(
    events: Sender<EngineEvent>,
    output: OutputConfig,
    runtime_dir: &Path,
) -> streamboat_player::engine::EngineResult<Box<dyn Engine>> {
    let engine = streamboat_player::MpvEngine::new(events, output, runtime_dir.to_path_buf())?;
    Ok(Box::new(engine))
}
