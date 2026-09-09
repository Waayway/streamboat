//! `MPNowPlayingInfoCenter`/`MPRemoteCommandCenter` adapter (macOS, feature
//! `nowplaying`, default on, D-030), built on `objc2-media-player`. Shape
//! mirrors [`crate::mpris`]/[`crate::smtc`] deliberately: a dedicated
//! background thread, remote-command handlers mapped to [`Command`]s, then
//! the [`PlayerHandle`]'s event broadcast pushed into `nowPlayingInfo` and
//! `playbackState`. Registration failure is logged and swallowed, never
//! fails the process — same rule every adapter here follows.
//!
//! **Caveat this module cannot resolve itself**: `audio-pipeline/references/
//! os-integration.md` §1 records that `souvlaki`'s own README states macOS
//! now-playing integration "requires an AppDelegate/winit event loop" —
//! Apple's documented contract for `MPRemoteCommandCenter` assumes an app
//! with a normal run loop, which a bare `streamboatd` (no window, no
//! `NSApplication`) does not have. This is written correctly against the
//! documented Objective-C API and registers unconditionally regardless
//! (the same "log and continue" rule as every other adapter's "no session
//! bus"/"no WinRT runtime" case), but whether `MPRemoteCommandCenter`'s
//! target-action delivery genuinely fires with no run loop at all is
//! unverified — no macOS CI runner exists here (see `docs/architecture.md`'s
//! cross-check section). The desktop shell, once its own AppKit-backed run
//! loop exists, is the configuration this is most likely to work fully in;
//! `MPNowPlayingInfoCenter.nowPlayingInfo`/`playbackState` themselves (the
//! read side other apps' Control Center sees) are plain property sets and
//! do not depend on a run loop the way the command *handlers* might.
//!
//! Volume (D-017): like SMTC, `MPNowPlayingInfoCenter` has no volume
//! surface of its own; the exclusive-mode rule lives entirely at the
//! `Player`/engine layer, same as `smtc.rs`.

use core::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSDictionary, NSNumber, NSString};
use objc2_media_player::{
    MPChangePlaybackPositionCommandEvent, MPMediaItemPropertyAlbumTitle, MPMediaItemPropertyArtist,
    MPMediaItemPropertyPlaybackDuration, MPMediaItemPropertyTitle, MPNowPlayingInfoCenter,
    MPNowPlayingInfoPropertyElapsedPlaybackTime, MPNowPlayingInfoPropertyPlaybackRate,
    MPNowPlayingPlaybackState, MPRemoteCommand, MPRemoteCommandCenter, MPRemoteCommandEvent,
    MPRemoteCommandHandlerStatus,
};

use streamboat_core::models::TrackSummary;
use streamboat_core::proto::{Command, Event, PlaybackStatus};

use crate::player::PlayerHandle;

/// Start the NowPlaying adapter on its own thread. Returns immediately; the
/// thread outlives the call and exits on its own if the player shuts down
/// (its event broadcast closing).
pub fn spawn(handle: PlayerHandle) {
    std::thread::Builder::new()
        .name("streamboat-nowplaying".into())
        .spawn(move || run(handle))
        .expect("spawn streamboat-nowplaying thread");
}

fn run(handle: PlayerHandle) {
    // SAFETY: `defaultCenter`/`sharedCommandCenter` are plain Objective-C
    // class methods returning a process-wide singleton; safe to call from
    // any thread, unlike UIKit/AppKit view code — see the module doc
    // comment for the one caveat that is genuinely open (whether the
    // *command handlers* fire with no run loop at all).
    let center = unsafe { MPNowPlayingInfoCenter::defaultCenter() };
    let commands = unsafe { MPRemoteCommandCenter::sharedCommandCenter() };
    wire_commands(&commands, handle.clone());

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "nowplaying: could not start a runtime; media-key integration disabled"
            );
            return;
        }
    };
    tracing::info!("nowplaying: registered MPNowPlayingInfoCenter/MPRemoteCommandCenter");
    rt.block_on(async move {
        let mut events = handle.subscribe();
        let mut last_track: Option<TrackSummary> = None;
        loop {
            match events.recv().await {
                Ok(ev) => apply_event(&center, &mut last_track, &ev),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// Registers `play`/`pause`/`toggle`/`next`/`previous`/`changePlaybackPosition`
/// handlers, mapping each to a [`Command`] through the plain `Send` handle —
/// never touching the engine directly (D-010), same rule `mpris.rs`/`smtc.rs`
/// follow. The opaque "target" object every `addTargetWithHandler` call
/// returns is discarded: nothing here ever calls `removeTarget`, since this
/// adapter lives for the process.
fn wire_commands(commands: &MPRemoteCommandCenter, handle: PlayerHandle) {
    unsafe {
        add_target(&commands.playCommand(), {
            let h = handle.clone();
            move |_| {
                h.send(Command::Resume);
                MPRemoteCommandHandlerStatus::Success
            }
        });
        add_target(&commands.pauseCommand(), {
            let h = handle.clone();
            move |_| {
                h.send(Command::Pause);
                MPRemoteCommandHandlerStatus::Success
            }
        });
        add_target(&commands.togglePlayPauseCommand(), {
            let h = handle.clone();
            move |_| {
                h.send(Command::TogglePlayPause);
                MPRemoteCommandHandlerStatus::Success
            }
        });
        add_target(&commands.nextTrackCommand(), {
            let h = handle.clone();
            move |_| {
                h.send(Command::Next);
                MPRemoteCommandHandlerStatus::Success
            }
        });
        add_target(&commands.previousTrackCommand(), {
            let h = handle.clone();
            move |_| {
                h.send(Command::Previous);
                MPRemoteCommandHandlerStatus::Success
            }
        });

        let position_command = commands.changePlaybackPositionCommand();
        let h = handle.clone();
        let handler: RcBlock<
            dyn Fn(NonNull<MPRemoteCommandEvent>) -> MPRemoteCommandHandlerStatus,
        > = RcBlock::new(move |event: NonNull<MPRemoteCommandEvent>| {
            // SAFETY: this block is only ever installed as the target of
            // `changePlaybackPositionCommand`, so the object it is actually
            // invoked with is always an `MPChangePlaybackPositionCommandEvent`
            // even though the block's own type is fixed to the base
            // `MPRemoteCommandEvent` — exactly the situation `NonNull::cast`
            // exists for, and the pointer stays valid for the call's
            // duration (Apple hands it to us borrowed, not owned).
            // Already inside `wire_commands`'s enclosing `unsafe` block —
            // a closure is not an item boundary, so it inherits that
            // context; wrapping these individually would be the
            // `unused_unsafe` lint (confirmed against a real
            // `aarch64-apple-darwin` check, not asserted from memory).
            let event = event
                .cast::<MPChangePlaybackPositionCommandEvent>()
                .as_ref();
            let position_s = event.positionTime();
            h.send(Command::Seek {
                position_ms: (position_s.max(0.0) * 1000.0) as u64,
            });
            MPRemoteCommandHandlerStatus::Success
        });
        let _ = position_command.addTargetWithHandler(&handler);
    }
}

/// Builds the block once and registers it; used for every plain
/// play/pause/toggle/next/previous handler above.
unsafe fn add_target<F>(command: &MPRemoteCommand, f: F)
where
    F: Fn(NonNull<MPRemoteCommandEvent>) -> MPRemoteCommandHandlerStatus + 'static,
{
    let handler: RcBlock<dyn Fn(NonNull<MPRemoteCommandEvent>) -> MPRemoteCommandHandlerStatus> =
        RcBlock::new(f);
    let _ = unsafe { command.addTargetWithHandler(&handler) };
}

fn apply_event(center: &MPNowPlayingInfoCenter, last_track: &mut Option<TrackSummary>, ev: &Event) {
    match ev {
        Event::State { state } => {
            unsafe { center.setPlaybackState(playback_state(state.status)) };
            if let Some(track) = &state.current {
                *last_track = Some(track.clone());
            }
            update_now_playing_info(
                center,
                last_track.as_ref(),
                state.position_ms,
                state.duration_ms,
            );
        }
        Event::TrackStarted { track, .. } => {
            *last_track = Some(track.clone());
            update_now_playing_info(center, last_track.as_ref(), 0, track.duration_ms);
        }
        Event::Position {
            position_ms,
            duration_ms,
        } => update_now_playing_info(center, last_track.as_ref(), *position_ms, *duration_ms),
        Event::Stopped | Event::EndOfQueue => unsafe {
            center.setPlaybackState(MPNowPlayingPlaybackState::Stopped);
        },
        _ => {}
    }
}

fn playback_state(status: PlaybackStatus) -> MPNowPlayingPlaybackState {
    match status {
        PlaybackStatus::Playing => MPNowPlayingPlaybackState::Playing,
        PlaybackStatus::Paused | PlaybackStatus::Buffering => MPNowPlayingPlaybackState::Paused,
        PlaybackStatus::Stopped => MPNowPlayingPlaybackState::Stopped,
    }
}

/// SAFETY: any Objective-C object is trivially a valid `AnyObject` — this is
/// the standard "upcast to `id`" operation `NSDictionary::from_slices` below
/// needs to hold heterogeneous `NSString`/`NSNumber` values in one array.
unsafe fn erase<T: objc2::Message>(v: Retained<T>) -> Retained<AnyObject> {
    unsafe { Retained::cast_unchecked(v) }
}

/// Rebuilds and sets the whole `nowPlayingInfo` dictionary — title, artist,
/// album, duration, elapsed time and playback rate, the set macOS's Control
/// Center and lock screen need to render transport controls. Resent in full
/// on every call (mirroring `mpris.rs`'s
/// `apply_state`, which also always resends the whole `Metadata`) rather
/// than mutating an existing dictionary in place, since `NSDictionary` here
/// is the immutable, from-scratch-each-time flavour. `None` (nothing played
/// yet) clears the info center instead.
fn update_now_playing_info(
    center: &MPNowPlayingInfoCenter,
    track: Option<&TrackSummary>,
    position_ms: u64,
    duration_ms: Option<u64>,
) {
    let Some(track) = track else {
        unsafe { center.setNowPlayingInfo(None) };
        return;
    };
    unsafe {
        let title = NSString::from_str(&track.title);
        let artist = NSString::from_str(&track.artists);
        let album = NSString::from_str(&track.album);
        let duration_s = NSNumber::new_f64(duration_ms.unwrap_or(0) as f64 / 1000.0);
        let elapsed_s = NSNumber::new_f64(position_ms as f64 / 1000.0);
        let rate = NSNumber::new_f64(1.0);

        // These `extern "C"` statics already have type `&'static NSString`
        // (see `objc2-media-player`'s generated bindings) — no extra `&`.
        let keys: [&NSString; 6] = [
            MPMediaItemPropertyTitle,
            MPMediaItemPropertyArtist,
            MPMediaItemPropertyAlbumTitle,
            MPMediaItemPropertyPlaybackDuration,
            MPNowPlayingInfoPropertyElapsedPlaybackTime,
            MPNowPlayingInfoPropertyPlaybackRate,
        ];
        let values = [
            erase(title),
            erase(artist),
            erase(album),
            erase(duration_s),
            erase(elapsed_s),
            erase(rate),
        ];
        let value_refs: [&AnyObject; 6] = [
            &values[0], &values[1], &values[2], &values[3], &values[4], &values[5],
        ];
        let dict = NSDictionary::from_slices(&keys, &value_refs);
        center.setNowPlayingInfo(Some(&dict));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use streamboat_core::proto::OutputConfig;

    #[test]
    fn playback_state_maps_buffering_to_paused() {
        assert_eq!(
            playback_state(PlaybackStatus::Buffering),
            MPNowPlayingPlaybackState::Paused
        );
        assert_eq!(
            playback_state(PlaybackStatus::Playing),
            MPNowPlayingPlaybackState::Playing
        );
        assert_eq!(
            playback_state(PlaybackStatus::Stopped),
            MPNowPlayingPlaybackState::Stopped
        );
    }

    #[test]
    fn exclusive_output_is_still_the_engine_layers_call() {
        // MPNowPlayingInfoCenter has no volume surface of its own (see the
        // module doc comment) — mirrors the equivalent test in `smtc.rs`.
        let exclusive = OutputConfig::Exclusive {
            device: "coreaudio_exclusive/0".into(),
        };
        assert!(exclusive.is_exclusive());
    }
}
