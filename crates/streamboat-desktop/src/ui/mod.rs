//! The iced desktop shell (task brief): app architecture, startup, screens,
//! the persistent sidebar/playback bar, and the `PlayerLink` seam over
//! `streamboat-player`'s `PlayerHandle`/`Player::spawn` (D-010).

pub mod actions;
pub mod app;
pub mod banner;
pub mod design;
pub mod engine_select;
pub mod format;
pub mod images;
pub mod nav;
pub mod playback_bar;
pub mod player_link;
pub mod screens;
pub mod signal_path;
pub mod stream_ext;
pub mod widgets;

pub use app::run;

/// Opens `url` in the OS's default browser. Best-effort: a failure is
/// logged, never fatal (mirrors the existing CLI's `open_browser` in
/// `main.rs`, which this duplicates rather than sharing across a bin/lib
/// split that would otherwise exist only for this one helper).
pub fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let cmd = ("open", vec![url]);
    #[cfg(target_os = "windows")]
    let cmd = ("cmd", vec!["/C", "start", "", url]);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let cmd = ("xdg-open", vec![url]);
    match std::process::Command::new(cmd.0)
        .args(&cmd.1)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_) => {}
        Err(e) => tracing::warn!("could not open a browser automatically: {e}"),
    }
}
