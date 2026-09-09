//! The GUI's one seam into playback (D-010): every screen sends
//! [`Command`]s and reacts to [`Event`]s through a [`PlayerLink`], never by
//! touching [`streamboat_player::Engine`] or [`streamboat_player::Player`]
//! directly. [`InProcessLink`] is the implementation for when this process
//! holds the single-instance lock and runs its own engine, wrapping a
//! [`PlayerHandle`] from a `Player` spawned in the same process
//! (`Player::spawn`) — one of only two `streamboat-player` APIs this crate
//! touches at all, the seam D-010 draws.
//!
//! [`crate::ui::remote_link::RemoteLink`] is the other implementation, for
//! when another process already holds the lock and hosts the control API
//! (`ui::instance` decides which one `ui::app::run` builds).

use std::sync::Arc;

use streamboat_core::proto::{Command, Event};
use streamboat_player::PlayerHandle;

use crate::ui::stream_ext::BoxStream;

/// Everything the GUI needs from "the player," regardless of whether it is
/// in-process ([`InProcessLink`]) or a remote daemon over the control API
/// ([`crate::ui::remote_link::RemoteLink`]).
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
