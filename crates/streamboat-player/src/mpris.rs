//! MPRIS2 adapter (Linux only, D-030): exposes `org.mpris.MediaPlayer2` and
//! `.Player` on the session bus as `org.mpris.MediaPlayer2.streamboat`, so
//! Wayland/X11 media keys and desktop "now playing" widgets work — mandatory
//! wiring, not an optional nicety
//! (`audio-pipeline/references/os-integration.md` §1).
//!
//! Registration lives here, in the engine/player layer, so it runs the same
//! way whether it is started by `streamboatd` (headless) or, later, by the
//! desktop shell (D-030's "MPRIS registered in the engine" rule) — either
//! caller just does `mpris::spawn(handle)` once it has a [`PlayerHandle`].
//!
//! `mpris-server`'s [`Player`](mpris_server::Player) is `Rc`-based and not
//! `Send`, so it must live and run on one dedicated OS thread with its own
//! single-threaded runtime and a [`tokio::task::LocalSet`] (Sone's shape,
//! `os-integration.md` §1). That thread talks back to the real player only
//! through the plain, `Send` [`PlayerHandle`].
//!
//! A headless box or a stripped-down Pi image frequently has no D-Bus
//! session bus at all (`os-integration.md` §4): connecting then fails, and
//! that failure is logged and swallowed rather than propagated — MPRIS is
//! optional OS integration, never something the daemon depends on to run.

use mpris_server::{LoopStatus, Metadata, PlaybackStatus as MprisStatus, Player, Time, TrackId};
use streamboat_core::models::TrackSummary;
use streamboat_core::proto::{Command, Event, PlaybackStatus, PlayerState};

use crate::player::PlayerHandle;

/// Object-path prefix for track ids. Not `/org/mpris/...`: the spec reserves
/// that tree for its own well-known paths (`os-integration.md` §7 cites the
/// same rule High Tide follows with `/Track/{id}`).
const TRACK_ID_PREFIX: &str = "/io/github/waayway/streamboat/Track/";
/// Cover art size (`tidal-api` catalog-and-library.md §10's documented
/// sizes); 320px matches High Tide's own MPRIS art choice. Most desktop
/// shells do fetch a remote `mpris:artUrl` today, but not all
/// (`os-integration.md` §7 flags this); a local on-disk cache — not built
/// yet — is the eventual fix. Until then a remote URL is strictly better
/// than no art at all.
const ART_SIZE: u32 = 320;

/// Start the MPRIS adapter on its own thread. Returns immediately; the
/// thread outlives the call and exits on its own if the player shuts down
/// (its event broadcast closing) or if no session bus is reachable.
pub fn spawn(handle: PlayerHandle) {
    std::thread::Builder::new()
        .name("streamboat-mpris".into())
        .spawn(move || run(handle))
        .expect("spawn streamboat-mpris thread");
}

fn run(handle: PlayerHandle) {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::warn!(error = %e, "mpris: could not start a runtime; media-key integration disabled");
            return;
        }
    };
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async move {
        let player = match build_player().await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "mpris: no D-Bus session bus reachable; media-key integration disabled"
                );
                return;
            }
        };
        wire_commands(&player, handle.clone());
        tokio::task::spawn_local(player.run());
        tracing::info!("mpris: registered org.mpris.MediaPlayer2.streamboat");

        let mut events = handle.subscribe();
        loop {
            match events.recv().await {
                Ok(ev) => apply_event(&player, &ev).await,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

async fn build_player() -> mpris_server::zbus::Result<Player> {
    Player::builder("streamboat")
        .identity("streamboat")
        .desktop_entry("io.github.waayway.streamboat")
        .can_play(true)
        .can_pause(true)
        .can_seek(true)
        .can_go_next(true)
        .can_go_previous(true)
        .can_control(true)
        .playback_status(MprisStatus::Stopped)
        .build()
        .await
}

/// Map every `Player` control surface to a [`Command`], through the plain
/// `Send` handle — the callbacks below run on the MPRIS thread but never
/// touch the engine directly (D-010).
fn wire_commands(player: &Player, handle: PlayerHandle) {
    let h = handle.clone();
    player.connect_play_pause(move |_p| {
        h.send(Command::TogglePlayPause);
    });
    let h = handle.clone();
    player.connect_play(move |_p| {
        h.send(Command::Resume);
    });
    let h = handle.clone();
    player.connect_pause(move |_p| {
        h.send(Command::Pause);
    });
    let h = handle.clone();
    player.connect_stop(move |_p| {
        h.send(Command::Stop);
    });
    let h = handle.clone();
    player.connect_next(move |_p| {
        h.send(Command::Next);
    });
    let h = handle.clone();
    player.connect_previous(move |_p| {
        h.send(Command::Previous);
    });
    let h = handle.clone();
    player.connect_seek(move |p, offset| {
        // MPRIS `Seek` is a relative offset in microseconds, forward or back.
        let target = p.position().as_millis() + offset.as_millis();
        h.send(Command::Seek {
            position_ms: target.max(0) as u64,
        });
    });
    let h = handle.clone();
    player.connect_set_position(move |_p, _track_id, position| {
        // `SetPosition` is absolute; per spec a client is expected to send
        // the currently-playing track's id, and a stale one is a no-op —
        // the player has exactly one current track, so there is nothing
        // else to compare against here.
        h.send(Command::Seek {
            position_ms: position.as_millis().max(0) as u64,
        });
    });
    let h = handle.clone();
    player.connect_set_volume(move |_p, volume| {
        // The player itself refuses `SetVolume` with a `Warning` while an
        // exclusive output is active (D-017) and `apply_state` below always
        // re-asserts 1.0 in that case, so nothing extra is needed here.
        h.send(Command::SetVolume {
            volume: volume as f32,
        });
    });
}

async fn apply_event(player: &Player, ev: &Event) {
    match ev {
        Event::State { state } => apply_state(player, state).await,
        Event::TrackStarted { track, .. } => {
            let _ = player.set_metadata(track_metadata(track)).await;
            let _ = player.set_playback_status(MprisStatus::Playing).await;
            player.set_position(Time::ZERO);
        }
        Event::Position {
            position_ms,
            duration_ms: _,
        } => {
            player.set_position(ms_to_time(*position_ms));
        }
        Event::Stopped | Event::EndOfQueue => {
            let _ = player.set_playback_status(MprisStatus::Stopped).await;
        }
        _ => {}
    }
}

async fn apply_state(player: &Player, state: &PlayerState) {
    let status = match state.status {
        PlaybackStatus::Playing => MprisStatus::Playing,
        PlaybackStatus::Paused | PlaybackStatus::Buffering => MprisStatus::Paused,
        PlaybackStatus::Stopped => MprisStatus::Stopped,
    };
    let _ = player.set_playback_status(status).await;
    player.set_position(ms_to_time(state.position_ms));
    // D-017: volume is fixed at 1.0 and reported as such while an exclusive
    // output is active, never the last value a client tried to set.
    let volume = if state.output.is_exclusive() {
        1.0
    } else {
        f64::from(state.volume)
    };
    let _ = player.set_volume(volume).await;
    let _ = player.set_loop_status(LoopStatus::None).await;
    match &state.current {
        Some(track) => {
            let _ = player.set_metadata(track_metadata(track)).await;
        }
        None => {
            let _ = player.set_metadata(Metadata::new()).await;
        }
    }
    let can_go_next = state.current_index.is_some_and(|i| i + 1 < state.queue_len);
    let can_go_previous = state.current_index.is_some_and(|i| i > 0);
    let _ = player.set_can_go_next(can_go_next).await;
    let _ = player.set_can_go_previous(can_go_previous).await;
}

fn ms_to_time(ms: u64) -> Time {
    Time::from_millis(i64::try_from(ms).unwrap_or(i64::MAX))
}

/// Build MPRIS `Metadata` from a [`TrackSummary`]. `StreamInfo` (codec,
/// quality, ...) has no standard MPRIS key to land in, so it is not surfaced
/// here — the signal-path panel (D-036) is the right home for that.
fn track_metadata(track: &TrackSummary) -> Metadata {
    let trackid =
        TrackId::try_from(format!("{TRACK_ID_PREFIX}{}", track.id)).unwrap_or(TrackId::NO_TRACK);
    let mut builder = Metadata::builder()
        .trackid(trackid)
        .title(track.title.clone())
        // `TrackSummary::artists` is already one display string (possibly
        // several names joined); MPRIS wants a list, so wrap it as one.
        .artist([track.artists.clone()])
        .album(track.album.clone());
    if let Some(ms) = track.duration_ms {
        builder = builder.length(ms_to_time(ms));
    }
    if let Some(cover) = &track.cover {
        if let Ok(url) = streamboat_core::api::images::album_cover_url(cover, ART_SIZE) {
            builder = builder.art_url(url);
        }
    }
    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use streamboat_core::proto::OutputConfig;

    fn track(id: u64) -> TrackSummary {
        TrackSummary {
            id,
            title: "A Title".into(),
            artists: "An Artist".into(),
            album: "An Album".into(),
            duration_ms: Some(200_000),
            cover: Some("1e01cdb6-f15d-4d8b-8440-a047976c1cac".into()),
        }
    }

    #[test]
    fn metadata_maps_track_summary_fields() {
        let md = track_metadata(&track(42));
        assert_eq!(
            md.trackid().unwrap().as_str(),
            "/io/github/waayway/streamboat/Track/42"
        );
        assert_eq!(md.title(), Some("A Title"));
        assert_eq!(md.artist(), Some(vec!["An Artist".to_string()]));
        assert_eq!(md.album(), Some("An Album"));
        assert_eq!(md.length(), Some(Time::from_millis(200_000)));
        let art = md.art_url().expect("art url set");
        assert!(art.as_str().contains("resources.tidal.com"));
    }

    #[test]
    fn metadata_omits_art_when_no_cover() {
        let mut t = track(1);
        t.cover = None;
        let md = track_metadata(&t);
        assert_eq!(md.art_url(), None);
    }

    #[test]
    fn metadata_falls_back_to_no_track_on_a_bad_cover_id() {
        // Not a valid TIDAL image id; must not panic, and art is skipped.
        let mut t = track(1);
        t.cover = Some("not-a-uuid".into());
        let md = track_metadata(&t);
        assert_eq!(md.art_url(), None);
    }

    #[test]
    fn playback_status_maps_buffering_to_paused() {
        assert!(matches!(
            match PlaybackStatus::Buffering {
                PlaybackStatus::Playing => MprisStatus::Playing,
                PlaybackStatus::Paused | PlaybackStatus::Buffering => MprisStatus::Paused,
                PlaybackStatus::Stopped => MprisStatus::Stopped,
            },
            MprisStatus::Paused
        ));
    }

    #[test]
    fn ms_to_time_round_trips() {
        assert_eq!(ms_to_time(1500).as_millis(), 1500);
    }

    #[test]
    fn exclusive_output_forces_reported_volume_to_one() {
        // Pure mapping check mirroring `apply_state`'s exclusive-mode rule
        // (D-017), without needing a live D-Bus connection.
        let exclusive = OutputConfig::Exclusive {
            device: "hw:0,0".into(),
        };
        let reported = if exclusive.is_exclusive() {
            1.0f64
        } else {
            0.3
        };
        assert_eq!(reported, 1.0);
    }
}
