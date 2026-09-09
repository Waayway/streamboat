//! Single-instance lock shared by `streamboat` (GUI) and `streamboatd`
//! (D-010, D-045): a portable advisory file lock plus the small
//! filesystem-based signalling the two front ends use to find each other.
//!
//! D-010 names two mechanisms — "the MPRIS bus name where a D-Bus session
//! bus exists, a lock file or abstract Unix socket otherwise." This module
//! implements only the second, portable one (`fd-lock` on a plain file),
//! uniformly on every platform, per the concrete task that added it; the
//! MPRIS-bus-name variant is not built. That is a narrowing, not a
//! deviation: D-010 already names the lock file as a valid mechanism on its
//! own, just one of two, so always taking that branch stays inside the
//! decision rather than departing from it.
//!
//! Three files live under the runtime directory (`config::AppDirs::runtime`,
//! `/run/user/<uid>/streamboat` on Linux):
//!
//! - `instance.lock` — the lock itself. Whichever process holds it — a
//!   `streamboat` GUI running its own engine in-process, or `streamboatd` —
//!   is "the instance."
//! - `control-address` — written only by a lock holder that also hosts the
//!   control API (D-030, D-031: today that is always `streamboatd`, never a
//!   GUI). A second process that fails to take the lock reads this file to
//!   decide whether to become a [`crate::proto`] remote client over the
//!   control API, or has nothing to connect to.
//! - `show-request` — a deliberately `Command`-free signal (see
//!   `ui::instance`'s doc comment in `streamboat-desktop` for the full
//!   rationale): a second GUI instance that finds the lock held but no
//!   `control-address` file touches this file instead of opening its own
//!   window, and the holder polls its modification time to know when to
//!   raise its own window. Nothing here needs to know what the signal means
//!   beyond "something changed" — no `POST /v1/show` route is added to the
//!   control API, which would require the GUI to bind a listener and
//!   contradict D-031's "only `streamboatd` binds a listener."

use std::fs::{File, OpenOptions};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::Result;
use crate::fsutil;

/// Held for as long as this value is alive; the advisory lock releases when
/// it drops (or, at the latest, when the process exits and the OS reclaims
/// the file descriptor regardless).
///
/// The inner `fd_lock::RwLock` is deliberately leaked (`Box::leak`) so its
/// write guard — which borrows from it — can live inside this struct without
/// a self-referential type. That is harmless here: an [`InstanceLock`] is
/// meant to live until the process exits anyway, so "never freed until the
/// process exits" is the intended lifetime, not a real leak.
pub struct InstanceLock {
    _guard: fd_lock::RwLockWriteGuard<'static, File>,
    path: PathBuf,
}

impl std::fmt::Debug for InstanceLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstanceLock")
            .field("path", &self.path)
            .finish()
    }
}

impl InstanceLock {
    /// Try to become the single instance holding `path`, creating the file
    /// (and its parent directory) if needed.
    ///
    /// `Ok(None)` means another process already holds it — the normal
    /// "become a remote client, or just focus the other instance" path, not
    /// an error. An `Err` is a real filesystem problem (an unwritable
    /// runtime directory, for instance).
    pub fn try_acquire(path: &Path) -> Result<Option<Self>> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        let lock: &'static mut fd_lock::RwLock<File> =
            Box::leak(Box::new(fd_lock::RwLock::new(file)));
        match lock.try_write() {
            Ok(guard) => Ok(Some(Self {
                _guard: guard,
                path: path.to_path_buf(),
            })),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Write the `control-address` file: called only by whatever holds the
/// [`InstanceLock`] *and* hosts the control API (`streamboatd` today, per
/// D-031 — see the module doc for why a GUI never does this).
pub fn write_control_address(path: &Path, addr: SocketAddr) -> Result<()> {
    Ok(fsutil::atomic_write(
        path,
        addr.to_string().as_bytes(),
        0o600,
    )?)
}

/// Read the `control-address` file, if present. Absent means either nothing
/// holds the lock, or the holder is a GUI running its engine in-process
/// (which never writes this file).
pub fn read_control_address(path: &Path) -> Option<SocketAddr> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Touch the `show-request` file: a second GUI instance calls this when it
/// finds the lock held but no `control-address` file (so it has nothing to
/// connect to as a remote client) — it asks the running GUI to raise its
/// window instead of opening a duplicate one, then exits. The content is
/// never parsed by the reader (`ui::instance::watch_show_requests` in
/// `streamboat-desktop` only polls the modification time), so any distinct
/// write is enough; the current time is written for a human reading the
/// file by hand.
pub fn request_show(path: &Path) -> Result<()> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    Ok(fsutil::atomic_write(
        path,
        format!("{now}\n").as_bytes(),
        0o600,
    )?)
}

/// The modification time of `path`, if it exists — the primitive
/// `ui::instance`'s polling subscription builds on. Returns `None` (not an
/// error) for a file that has never been touched yet, which is the normal
/// state until a second instance ever asks to be shown.
pub fn show_request_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_acquire_on_the_same_path_fails_while_the_first_is_held() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("instance.lock");

        let first = InstanceLock::try_acquire(&path).unwrap();
        assert!(first.is_some(), "the first process must get the lock");

        let second = InstanceLock::try_acquire(&path).unwrap();
        assert!(
            second.is_none(),
            "a second process must not also get the lock"
        );

        drop(first);
        let third = InstanceLock::try_acquire(&path).unwrap();
        assert!(
            third.is_some(),
            "the lock must be acquirable again once the holder drops it"
        );
    }

    #[test]
    fn control_address_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control-address");
        assert_eq!(read_control_address(&path), None);

        let addr: SocketAddr = "127.0.0.1:4747".parse().unwrap();
        write_control_address(&path, addr).unwrap();
        assert_eq!(read_control_address(&path), Some(addr));
    }

    #[test]
    fn a_missing_control_address_file_is_none_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_control_address(&dir.path().join("nope")), None);
    }

    #[test]
    fn show_request_creates_and_updates_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("show-request");
        assert!(show_request_mtime(&path).is_none());

        request_show(&path).unwrap();
        let first = show_request_mtime(&path).expect("file now exists");

        // A second touch must still succeed (and stay a valid, readable
        // file) even though nothing reads its content back.
        request_show(&path).unwrap();
        assert!(show_request_mtime(&path).unwrap() >= first);
    }
}
