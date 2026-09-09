//! SMTC adapter (Windows, feature `smtc`, default on, D-030): exposes System
//! Media Transport Controls through `Windows::Media::Playback::MediaPlayer`'s
//! own `SystemMediaTransportControls` property, not
//! `SystemMediaTransportControlsInterop::GetForWindow`. That interop path is
//! the one `audio-pipeline/references/os-integration.md` §5 documents as
//! needing a real `HWND` — "a `streamboat-server` Windows service or CLI
//! daemon with no window gets no SMTC integration at all" — which does not
//! apply here: a `MediaPlayer` instance creates its own implicit
//! message-only window under the hood, so its SMTC works from a plain
//! background thread with no window of streamboat's own, headless included.
//!
//! Shape mirrors [`crate::mpris`] deliberately: a dedicated OS thread,
//! `ButtonPressed` mapped to [`Command`]s, then the [`PlayerHandle`]'s event
//! broadcast pushed into `DisplayUpdater`/`PlaybackStatus`/timeline
//! properties. Registration failure — no WinRT runtime at all (e.g. Windows
//! Server Core, or a pre-WinRT Windows 7 box) — is logged and swallowed,
//! never fails the process, same rule as `mpris.rs`'s "no D-Bus session bus"
//! case and D-030's "adapters over the control API, never load-bearing."
//!
//! Volume (D-017): SMTC has no volume control surface of its own — Windows
//! routes per-app volume through the system mixer instead — so unlike
//! `mpris.rs` there is nothing to force back to 1.0 here; the exclusive-mode
//! rule still applies at the `Player`/engine layer regardless of what SMTC
//! shows.
//!
//! **Unverified in this sandbox**: no Windows CI runner exists here (see
//! `docs/architecture.md`'s cross-check section) — this is written from the
//! `windows` 0.62.2 crate's own generated bindings (checked against its
//! published source, not against a live SMTC popup), the same "documented,
//! cfg-gated, untested here" standing `mpv.rs` already carries for its own
//! Windows/macOS AO option values.

use windows::Foundation::{TypedEventHandler, Uri};
use windows::Media::Playback::MediaPlayer;
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
    SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
    SystemMediaTransportControlsTimelineProperties,
};
use windows::Storage::Streams::RandomAccessStreamReference;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::core::HSTRING;

use streamboat_core::models::TrackSummary;
use streamboat_core::proto::{Command, Event, PlaybackStatus, PlayerState};

use crate::player::PlayerHandle;

/// Cover art size (`tidal-api` catalog-and-library.md §10's documented
/// sizes); matches `mpris.rs`'s own choice so the two adapters agree.
const ART_SIZE: u32 = 320;

/// Start the SMTC adapter on its own thread. Returns immediately; the
/// thread outlives the call and exits on its own if the player shuts down
/// (its event broadcast closing) or if no WinRT runtime is reachable.
pub fn spawn(handle: PlayerHandle) {
    std::thread::Builder::new()
        .name("streamboat-smtc".into())
        .spawn(move || run(handle))
        .expect("spawn streamboat-smtc thread");
}

fn run(handle: PlayerHandle) {
    // SAFETY: every WinRT activation needs the calling thread in an
    // apartment first; this is the standard `RoInitialize` call every
    // non-UWP WinRT sample makes once per thread before touching any WinRT
    // type. A thread that already has one (this thread is fresh, so that
    // should not happen, but a failure here just means "no media-key
    // integration," never a crash) is treated the same as any other
    // registration failure below.
    if let Err(e) = unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
        tracing::warn!(error = %e, "smtc: RoInitialize failed; media-key integration disabled");
        return;
    }

    let player = match MediaPlayer::new() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "smtc: could not create a MediaPlayer; media-key integration disabled"
            );
            return;
        }
    };
    let smtc = match player.SystemMediaTransportControls() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "smtc: SystemMediaTransportControls unavailable; media-key integration disabled"
            );
            return;
        }
    };
    if let Err(e) = configure(&smtc) {
        tracing::warn!(
            error = %e,
            "smtc: could not configure transport controls; media-key integration disabled"
        );
        return;
    }
    wire_commands(&smtc, handle.clone());

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::warn!(error = %e, "smtc: could not start a runtime; media-key integration disabled");
            return;
        }
    };
    tracing::info!("smtc: registered System Media Transport Controls");
    rt.block_on(async move {
        let mut events = handle.subscribe();
        loop {
            match events.recv().await {
                Ok(ev) => apply_event(&smtc, &ev),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    // `player`/`smtc` are dropped here, at the very end of the thread's
    // life: SMTC stops reflecting streamboat's state once the `MediaPlayer`
    // that owns it goes away, so this thread must hold both for as long as
    // the adapter should stay registered — the whole point of running the
    // event loop on this same stack frame rather than handing `smtc` off.
}

fn configure(smtc: &SystemMediaTransportControls) -> windows::core::Result<()> {
    smtc.SetIsEnabled(true)?;
    smtc.SetIsPlayEnabled(true)?;
    smtc.SetIsPauseEnabled(true)?;
    smtc.SetIsStopEnabled(true)?;
    smtc.SetIsNextEnabled(true)?;
    smtc.SetIsPreviousEnabled(true)?;
    Ok(())
}

/// Maps `ButtonPressed` to [`Command`]s through the plain `Send` handle —
/// the callback runs on whatever thread WinRT's event dispatch uses, never
/// touching the engine directly (D-010), exactly like `mpris.rs`'s
/// `wire_commands`.
fn wire_commands(smtc: &SystemMediaTransportControls, handle: PlayerHandle) {
    let handler = TypedEventHandler::<
        SystemMediaTransportControls,
        SystemMediaTransportControlsButtonPressedEventArgs,
    >::new(move |_sender, args| {
        let Some(args) = args.as_ref() else {
            return Ok(());
        };
        let Ok(button) = args.Button() else {
            return Ok(());
        };
        let cmd = match button {
            SystemMediaTransportControlsButton::Play => Some(Command::Resume),
            SystemMediaTransportControlsButton::Pause => Some(Command::Pause),
            SystemMediaTransportControlsButton::Stop => Some(Command::Stop),
            SystemMediaTransportControlsButton::Next => Some(Command::Next),
            SystemMediaTransportControlsButton::Previous => Some(Command::Previous),
            _ => None,
        };
        if let Some(cmd) = cmd {
            handle.send(cmd);
        }
        Ok(())
    });
    if let Err(e) = smtc.ButtonPressed(&handler) {
        tracing::warn!(error = %e, "smtc: could not subscribe to ButtonPressed");
    }
}

fn apply_event(smtc: &SystemMediaTransportControls, ev: &Event) {
    match ev {
        Event::State { state } => apply_state(smtc, state),
        Event::TrackStarted { track, .. } => update_display(smtc, track),
        Event::Position {
            position_ms,
            duration_ms,
        } => update_timeline(smtc, *position_ms, *duration_ms),
        Event::Stopped | Event::EndOfQueue => {
            let _ = smtc.SetPlaybackStatus(MediaPlaybackStatus::Stopped);
        }
        _ => {}
    }
}

fn apply_state(smtc: &SystemMediaTransportControls, state: &PlayerState) {
    let _ = smtc.SetPlaybackStatus(playback_status(state.status));
    if let Some(track) = &state.current {
        update_display(smtc, track);
    }
    update_timeline(smtc, state.position_ms, state.duration_ms);
    let can_next = state.current_index.is_some_and(|i| i + 1 < state.queue_len);
    let can_previous = state.current_index.is_some_and(|i| i > 0);
    let _ = smtc.SetIsNextEnabled(can_next);
    let _ = smtc.SetIsPreviousEnabled(can_previous);
    // D-017: nothing to force here — SMTC carries no volume property of its
    // own (see the module doc comment); the exclusive-mode volume rule
    // lives entirely at the `Player`/engine layer.
}

fn playback_status(status: PlaybackStatus) -> MediaPlaybackStatus {
    match status {
        PlaybackStatus::Playing => MediaPlaybackStatus::Playing,
        PlaybackStatus::Paused | PlaybackStatus::Buffering => MediaPlaybackStatus::Paused,
        PlaybackStatus::Stopped => MediaPlaybackStatus::Stopped,
    }
}

/// Sets `MusicProperties` (title/artist/album) and, when the track has a
/// cover, a thumbnail built from the album-cover URL via
/// `RandomAccessStreamReference::CreateFromUri` — SMTC's own artwork
/// contract needs a stream reference, never a bare URL string
/// (`os-integration.md` §7).
fn update_display(smtc: &SystemMediaTransportControls, track: &TrackSummary) {
    let Ok(updater) = smtc.DisplayUpdater() else {
        return;
    };
    let _ = updater.SetType(MediaPlaybackType::Music);
    if let Ok(music) = updater.MusicProperties() {
        let _ = music.SetTitle(&HSTRING::from(track.title.as_str()));
        let _ = music.SetArtist(&HSTRING::from(track.artists.as_str()));
        let _ = music.SetAlbumTitle(&HSTRING::from(track.album.as_str()));
    }
    if let Some(cover) = &track.cover {
        if let Ok(url) = streamboat_core::api::images::album_cover_url(cover, ART_SIZE) {
            if let Ok(uri) = Uri::CreateUri(&HSTRING::from(url.as_str())) {
                if let Ok(stream_ref) = RandomAccessStreamReference::CreateFromUri(&uri) {
                    let _ = updater.SetThumbnail(&stream_ref);
                }
            }
        }
    }
    let _ = updater.Update();
}

/// `SystemMediaTransportControlsTimelineProperties` is a plain value object
/// built fresh and handed to `UpdateTimelineProperties` each time — nothing
/// to keep alive between calls, unlike `DisplayUpdater` (owned by `smtc`
/// itself). `TimeSpan::Duration` is 100 ns ticks, hence `* 10_000` from ms.
fn update_timeline(
    smtc: &SystemMediaTransportControls,
    position_ms: u64,
    duration_ms: Option<u64>,
) {
    let Ok(timeline) = SystemMediaTransportControlsTimelineProperties::new() else {
        return;
    };
    let _ = timeline.SetStartTime(ms_to_timespan(0));
    let _ = timeline.SetMinSeekTime(ms_to_timespan(0));
    let _ = timeline.SetPosition(ms_to_timespan(position_ms));
    let end = duration_ms.unwrap_or(position_ms);
    let _ = timeline.SetEndTime(ms_to_timespan(end));
    let _ = timeline.SetMaxSeekTime(ms_to_timespan(end));
    let _ = smtc.UpdateTimelineProperties(&timeline);
}

fn ms_to_timespan(ms: u64) -> windows::Foundation::TimeSpan {
    windows::Foundation::TimeSpan {
        Duration: (ms as i64).saturating_mul(10_000),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use streamboat_core::proto::OutputConfig;

    #[test]
    fn playback_status_maps_buffering_to_paused() {
        assert_eq!(
            playback_status(PlaybackStatus::Buffering),
            MediaPlaybackStatus::Paused
        );
        assert_eq!(
            playback_status(PlaybackStatus::Playing),
            MediaPlaybackStatus::Playing
        );
        assert_eq!(
            playback_status(PlaybackStatus::Stopped),
            MediaPlaybackStatus::Stopped
        );
    }

    #[test]
    fn ms_to_timespan_uses_100ns_ticks() {
        assert_eq!(ms_to_timespan(1500).Duration, 15_000_000);
    }

    #[test]
    fn exclusive_output_is_still_the_engine_layers_call() {
        // SMTC has no volume surface of its own (see the module doc
        // comment) — this test exists to document that the D-017 rule
        // lives at the `Player`/engine layer, not here, mirroring the
        // equivalent test in `mpris.rs`.
        let exclusive = OutputConfig::Exclusive {
            device: "wasapi/{0}".into(),
        };
        assert!(exclusive.is_exclusive());
    }
}
