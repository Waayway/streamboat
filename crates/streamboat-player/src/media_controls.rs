//! Picks the OS media-integration adapter by `cfg`, mirroring
//! [`crate::platform::default_engine`]'s "one function, several bodies"
//! shape (D-030: "MPRIS/SMTC are adapters over the control API, MPRIS
//! registered in the engine"). Every adapter registers itself the same way
//! from here on: `media_controls::spawn(handle)`, called once by whichever
//! process owns the [`PlayerHandle`] (`streamboatd` today; the desktop shell
//! once it exists — see `docs/architecture.md`).
//!
//! Each adapter logs and returns on registration failure — no D-Bus session
//! bus, no WinRT runtime, whatever — and never fails the process; see
//! `mpris.rs`'s module doc comment for the reasoning this mirrors.

use crate::player::PlayerHandle;

#[cfg(all(target_os = "linux", feature = "mpris"))]
pub fn spawn(handle: PlayerHandle) {
    crate::mpris::spawn(handle);
}

#[cfg(all(target_os = "windows", feature = "smtc"))]
pub fn spawn(handle: PlayerHandle) {
    crate::smtc::spawn(handle);
}

#[cfg(all(target_os = "macos", feature = "nowplaying"))]
pub fn spawn(handle: PlayerHandle) {
    crate::nowplaying::spawn(handle);
}

/// Reached on Linux without `mpris`, Windows without `smtc`, macOS without
/// `nowplaying`, or any other target: no OS media-key integration, logged
/// once so a build that dropped one of those features on purpose (see
/// `Cargo.toml`'s header comment) does not look like it silently forgot to
/// wire this up.
#[cfg(not(any(
    all(target_os = "linux", feature = "mpris"),
    all(target_os = "windows", feature = "smtc"),
    all(target_os = "macos", feature = "nowplaying"),
)))]
pub fn spawn(_handle: PlayerHandle) {
    tracing::debug!(
        "media_controls: no OS media-integration adapter compiled in for this target/feature set"
    );
}
