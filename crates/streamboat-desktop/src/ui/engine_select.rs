//! Which audio engine backs the desktop shell (D-016: GStreamer on Linux,
//! libmpv on Windows/macOS, one `Engine` trait). The choice itself lives in
//! `streamboat_player::default_engine`, shared with `streamboatd` and the
//! CLI, so this module is only the shell's call site: it keeps `ui::app`
//! from naming any concrete backend (the "only engine seam the GUI may use"
//! rule, D-010). Which backend is compiled in is decided by this crate's
//! `gstreamer`/`mpv` features (see `Cargo.toml`); `default_engine` raises a
//! `compile_error!` when neither fits the target.

use std::path::Path;
use std::sync::mpsc::Sender;

use streamboat_core::proto::OutputConfig;
use streamboat_player::{Engine, EngineEvent};

/// Builds the platform engine and returns it boxed behind the shared
/// [`Engine`] trait.
pub fn build(
    events: Sender<EngineEvent>,
    output: OutputConfig,
    runtime_dir: &Path,
) -> streamboat_player::engine::EngineResult<Box<dyn Engine>> {
    streamboat_player::default_engine(events, output, runtime_dir.to_path_buf())
}
