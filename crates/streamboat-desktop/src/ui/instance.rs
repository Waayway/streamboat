//! Single-instance orchestration for the desktop shell (D-010, D-045):
//! decides, once at startup, whether this process becomes the instance
//! (spawns its own engine, `ui::player_link::InProcessLink`), a remote
//! client of a daemon that already holds the lock
//! (`ui::remote_link::RemoteLink`, D-030), or neither — a second GUI
//! instance with no daemon around, which just asks the running one to show
//! itself and exits.
//!
//! The mechanism is `streamboat_core::instance_lock`: a portable `fd-lock`
//! file plus two small filesystem signals (see that module's doc comment
//! for why a `control-address` file and a `show-request` file, rather than
//! a `POST /v1/show` route on the control API, which would need the GUI to
//! bind a listener and contradict D-031's "only `streamboatd` binds a
//! listener").

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use streamboat_core::config::AppDirs;
use streamboat_core::instance_lock::{self, InstanceLock};

use crate::ui::stream_ext::BoxStream;

/// What `ui::app::run` should do, decided once at startup.
pub enum Decision {
    /// This process holds the lock: spawn the engine in-process. The
    /// [`InstanceLock`] must be kept alive for the whole run — it is not
    /// read again, only held.
    Local(InstanceLock),
    /// Another process holds the lock and hosts the control API at `addr`
    /// (always `streamboatd` today, per D-031): become a remote client.
    Remote(std::net::SocketAddr),
    /// Another process holds the lock but hosts no control API — another
    /// GUI instance. A `show-request` touch has already been sent to it;
    /// there is nothing left for this process to do but exit.
    FocusedOther,
}

/// Makes the decision above. Touches the `show-request` file itself in the
/// [`Decision::FocusedOther`] case, so the caller only needs to act on the
/// result, not perform a separate step for it.
pub fn decide(dirs: &AppDirs) -> streamboat_core::Result<Decision> {
    let lock_path = dirs.instance_lock_path();
    match InstanceLock::try_acquire(&lock_path)? {
        Some(lock) => Ok(Decision::Local(lock)),
        None => match instance_lock::read_control_address(&dirs.control_address_path()) {
            Some(addr) => Ok(Decision::Remote(addr)),
            None => {
                instance_lock::request_show(&dirs.show_request_path())?;
                Ok(Decision::FocusedOther)
            }
        },
    }
}

const POLL_INTERVAL: Duration = Duration::from_millis(400);

/// A stream that yields `()` every time the `show-request` file at `path`
/// changes — the subscription `ui::app::App` runs only while it holds
/// [`Decision::Local`], to know when a second GUI instance asked to be
/// shown. `fn(&PathBuf) -> BoxStream<()>` matches
/// `iced::Subscription::run_with`'s bare-`fn`-pointer builder shape
/// directly (`PathBuf: Hash + 'static`), the same idiom
/// `ui::app::player_events` uses for [`crate::ui::player_link::LinkKey`].
///
/// A 400ms poll of one `stat()` call is deliberately simple rather than a
/// real filesystem-watch (inotify/FSEvents/ReadDirectoryChangesW): the
/// signal is latency-insensitive (a human clicking the app icon a second
/// time will not notice 400ms) and this avoids a new dependency and three
/// more platform-specific code paths for a corner case.
// The parameter must be exactly `&PathBuf`, not `&Path`:
// `iced::Subscription::run_with`'s bare-fn-pointer builder is
// `fn(&D) -> S` for `D = PathBuf` (the subscription's `data`), and
// `fn(&Path) -> _` is a different function-pointer type that does not
// satisfy it — there is no `D` to swap in to satisfy clippy's usual advice
// here.
#[allow(clippy::ptr_arg)]
pub fn show_request_events(path: &PathBuf) -> BoxStream<()> {
    let path = path.clone();
    Box::pin(futures::stream::unfold(
        None::<SystemTime>,
        move |last_seen| {
            let path = path.clone();
            async move {
                loop {
                    tokio::time::sleep(POLL_INTERVAL).await;
                    if let Some(mtime) = instance_lock::show_request_mtime(&path) {
                        if last_seen != Some(mtime) {
                            return Some(((), Some(mtime)));
                        }
                    }
                }
            }
        },
    ))
}

/// Reads the bearer token `ui::remote_link::RemoteLink` authenticates
/// with, from the same file `streamboatd` generates
/// (`AppDirs::control_token_path`, D-030). A remote client only ever reads
/// this file — generating one is the holder's job
/// (`streamboat_core::config::load_or_create_control_token`), never a
/// client's.
pub fn read_control_token(path: &Path) -> streamboat_core::Result<String> {
    let token = std::fs::read_to_string(path)?.trim().to_string();
    if token.is_empty() {
        return Err(streamboat_core::Error::Config(format!(
            "control token file {} is empty",
            path.display()
        )));
    }
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decide_is_local_when_nothing_else_holds_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = AppDirs {
            config: dir.path().join("config"),
            data: dir.path().join("data"),
            cache: dir.path().join("cache"),
            runtime: dir.path().join("run"),
        };
        match decide(&dirs).unwrap() {
            Decision::Local(_lock) => {}
            _ => panic!("expected Local"),
        }
    }

    #[test]
    fn decide_is_remote_when_a_control_address_file_is_present() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = AppDirs {
            config: dir.path().join("config"),
            data: dir.path().join("data"),
            cache: dir.path().join("cache"),
            runtime: dir.path().join("run"),
        };
        // Simulate a daemon already holding the lock and hosting the
        // control API: take the lock ourselves (standing in for the
        // daemon) and write the address file, then decide from a second,
        // independent path handle (the lock is per-path, not per-`AppDirs`
        // value, so a second `decide` call against the same `dirs` still
        // observes the first's held lock).
        let held = InstanceLock::try_acquire(&dirs.instance_lock_path())
            .unwrap()
            .expect("first acquire must succeed");
        let addr: std::net::SocketAddr = "127.0.0.1:4747".parse().unwrap();
        instance_lock::write_control_address(&dirs.control_address_path(), addr).unwrap();

        match decide(&dirs).unwrap() {
            Decision::Remote(got) => assert_eq!(got, addr),
            other => panic!(
                "expected Remote, got a different decision: {}",
                debug_name(&other)
            ),
        }
        drop(held);
    }

    #[test]
    fn decide_focuses_the_other_gui_and_touches_show_request_when_no_daemon_is_present() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = AppDirs {
            config: dir.path().join("config"),
            data: dir.path().join("data"),
            cache: dir.path().join("cache"),
            runtime: dir.path().join("run"),
        };
        let held = InstanceLock::try_acquire(&dirs.instance_lock_path())
            .unwrap()
            .expect("first acquire must succeed");
        // No control-address file: the holder is a GUI, not a daemon.
        assert!(instance_lock::show_request_mtime(&dirs.show_request_path()).is_none());

        match decide(&dirs).unwrap() {
            Decision::FocusedOther => {}
            other => panic!(
                "expected FocusedOther, got a different decision: {}",
                debug_name(&other)
            ),
        }
        assert!(
            instance_lock::show_request_mtime(&dirs.show_request_path()).is_some(),
            "deciding FocusedOther must touch the show-request file"
        );
        drop(held);
    }

    #[test]
    fn read_control_token_reads_back_what_was_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control-token");
        std::fs::write(&path, "abc123\n").unwrap();
        assert_eq!(read_control_token(&path).unwrap(), "abc123");
    }

    #[test]
    fn read_control_token_rejects_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_control_token(&dir.path().join("nope")).is_err());
    }

    fn debug_name(d: &Decision) -> &'static str {
        match d {
            Decision::Local(_) => "Local",
            Decision::Remote(_) => "Remote",
            Decision::FocusedOther => "FocusedOther",
        }
    }
}
