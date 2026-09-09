//! The GUI's one seam into playback (D-010): every screen sends
//! [`Command`]s and reacts to [`Event`]s through a [`PlayerLink`], never by
//! touching [`streamboat_player::Engine`] or [`streamboat_player::Player`]
//! directly. [`InProcessLink`] is the implementation this wave ships,
//! wrapping a [`PlayerHandle`] from a `Player` spawned in the same process
//! (`Player::spawn`) — the only two `streamboat-player` APIs this crate is
//! allowed to touch, per the task brief.
//!
//! [`RemoteLink`] is a documented stub for the future control-API client
//! (D-010's "GUI becomes a remote client" path, D-030's HTTP+WebSocket
//! control API): when `streamboatd` already holds the single-instance lock,
//! the desktop shell is supposed to become a plain client of that daemon's
//! Command/Event surface over the network instead of spawning its own
//! engine. That daemon and its control API do not exist yet
//! (`docs/architecture.md` "not yet built"), so there is nothing to connect
//! to — this type exists only so the call site in `ui/app.rs` already
//! branches on "local lock held" vs "remote," and so the next agent adds an
//! HTTP/WebSocket client here instead of inventing a second seam.

use std::sync::Arc;

use streamboat_core::proto::{Command, Event};
use streamboat_player::PlayerHandle;

use crate::ui::stream_ext::BoxStream;

/// Everything the GUI needs from "the player," regardless of whether it is
/// in-process ([`InProcessLink`]) or, eventually, a daemon over the network
/// ([`RemoteLink`]).
pub trait PlayerLink: Send + Sync {
    /// `false` only if the player has already shut down.
    fn send(&self, cmd: Command) -> bool;

    /// A fresh stream of every [`Event`] from this point on. Called once by
    /// the app's `subscription()`; iced keeps the returned stream alive for
    /// as long as the subscription is active.
    fn events(&self) -> BoxStream<Event>;
}

/// The in-process implementation: a thin wrapper over [`PlayerHandle`].
#[derive(Clone)]
pub struct InProcessLink {
    handle: PlayerHandle,
}

impl InProcessLink {
    pub fn new(handle: PlayerHandle) -> Self {
        Self { handle }
    }
}

impl PlayerLink for InProcessLink {
    fn send(&self, cmd: Command) -> bool {
        self.handle.send(cmd)
    }

    fn events(&self) -> BoxStream<Event> {
        let rx = self.handle.subscribe();
        Box::pin(futures::stream::unfold(rx, |mut rx| async move {
            loop {
                match rx.recv().await {
                    Ok(ev) => return Some((ev, rx)),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                }
            }
        }))
    }
}

/// A future remote-client implementation over `streamboatd`'s control API
/// (D-010, D-030). Deliberately unimplemented: no HTTP/WebSocket code here
/// yet, only the seam. Constructing one today always fails, loudly, rather
/// than silently pretending to be connected.
///
/// Nothing constructs this today (`ui::app::run` always spawns an in-process
/// `Player`, since the single-instance lock and the daemon it would defer to
/// do not exist yet either — D-010, D-045) — `#[allow(dead_code)]` marks
/// that as intentional rather than an oversight to clean up.
#[allow(dead_code)]
pub struct RemoteLink {
    endpoint: String,
}

#[allow(dead_code)]
impl RemoteLink {
    /// `endpoint` will be the daemon's `http://127.0.0.1:<port>` base URL
    /// once the control API (D-030) exists. Always returns an error today.
    pub fn connect(endpoint: impl Into<String>) -> Result<Self, RemoteLinkError> {
        let _ = Self {
            endpoint: endpoint.into(),
        };
        Err(RemoteLinkError::NotImplemented)
    }
}

#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum RemoteLinkError {
    #[error(
        "streamboatd's control API (D-030) is not built yet; the desktop shell cannot become a \
         remote client of another instance's daemon"
    )]
    NotImplemented,
}

impl PlayerLink for RemoteLink {
    fn send(&self, _cmd: Command) -> bool {
        false
    }

    fn events(&self) -> BoxStream<Event> {
        Box::pin(futures::stream::empty::<Event>())
    }
}

/// Type-erased handle used by [`crate::ui::app::App`] so it does not care
/// which [`PlayerLink`] implementation backs it.
pub type SharedLink = Arc<dyn PlayerLink>;

/// Wraps a [`SharedLink`] as `iced::Subscription::run_with` data. The app
/// holds exactly one link for its whole lifetime, so the `Arc`'s pointer
/// identity is a stable, unique subscription id — cheaper than giving
/// `PlayerLink` its own id scheme just for this.
#[derive(Clone)]
pub struct LinkKey(pub SharedLink);

impl std::hash::Hash for LinkKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let ptr = Arc::as_ptr(&self.0) as *const () as usize;
        ptr.hash(state);
    }
}
