//! A bounded, thread-safe ring of recent log lines, and the panic hook that
//! writes a structured crash report to `<data dir>/crashes/` using it
//! (D-029). Never writes anywhere else — no hosted crash service, no
//! ingest key, nothing leaves the machine.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;

use super::redact::redact;
use crate::fsutil;

/// The last `capacity` redacted log lines, oldest first — installed as a
/// `tracing` layer ([`super::logging::RingBufferLayer`]) and read back by
/// [`install_panic_hook`] to give a crash report some context for what led
/// up to it.
pub struct RingBuffer {
    lines: Mutex<VecDeque<String>>,
    capacity: usize,
}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            lines: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity: capacity.max(1),
        }
    }

    /// Appends one line, already expected to be pre-redacted by the caller
    /// (the logging layer redacts before this is ever called) — this
    /// function does not redact again, so it stays usable for lines that
    /// are already known-safe without a second, wasted pass.
    pub fn push(&self, line: String) {
        let mut lines = self.lines.lock().unwrap();
        if lines.len() >= self.capacity {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    /// A snapshot, oldest first.
    pub fn snapshot(&self) -> Vec<String> {
        self.lines.lock().unwrap().iter().cloned().collect()
    }
}

/// One crash report, written as pretty JSON to
/// `<data dir>/crashes/crash-<unix_ms>.json`.
#[derive(Debug, Serialize)]
pub struct CrashReport {
    /// RFC 2822-ish, from `httpdate` — human-legible without adding a date
    /// dependency this crate did not already have.
    pub timestamp: String,
    pub timestamp_unix_ms: u128,
    pub app: String,
    pub version: String,
    pub os: String,
    pub arch: String,
    pub message: String,
    pub location: Option<String>,
    /// `Some` only when `RUST_BACKTRACE` was set — this hook honours that
    /// variable itself rather than always capturing one.
    pub backtrace: Option<String>,
    /// The last N log lines before the panic, oldest first, already
    /// redacted by the logging layer that fed [`RingBuffer`].
    pub recent_log_lines: Vec<String>,
}

fn now() -> (String, u128) {
    let now = std::time::SystemTime::now();
    let human = httpdate::fmt_http_date(now);
    let ms = now
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    (human, ms)
}

/// Builds and writes one crash report for `info` into `crashes_dir` (created
/// if missing). Returns the path written, for a caller (tests, or a
/// deliberately-triggered "report a bug" flow) that wants it.
pub fn write_report(
    crashes_dir: &Path,
    app: &str,
    version: &str,
    ring: &RingBuffer,
    info: &std::panic::PanicHookInfo<'_>,
) -> std::io::Result<PathBuf> {
    let (timestamp, timestamp_unix_ms) = now();
    let location = info.location().map(|l| l.to_string());
    let message = redact(&info.to_string());
    let backtrace = std::env::var_os("RUST_BACKTRACE")
        .filter(|v| v != "0")
        .map(|_| redact(&std::backtrace::Backtrace::force_capture().to_string()));
    let report = CrashReport {
        timestamp,
        timestamp_unix_ms,
        app: app.to_string(),
        version: version.to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        message,
        location,
        backtrace,
        recent_log_lines: ring.snapshot(),
    };
    std::fs::create_dir_all(crashes_dir)?;
    let path = crashes_dir.join(format!("crash-{timestamp_unix_ms}.json"));
    let json = serde_json::to_vec_pretty(&report).unwrap_or_else(|e| {
        format!("{{\"error\":\"could not serialize crash report: {e}\"}}").into_bytes()
    });
    fsutil::atomic_write(&path, &json, 0o600)?;
    Ok(path)
}

/// Installs a panic hook that chains to whatever hook was previously
/// installed (so the panic still prints to stderr the normal way) and then
/// writes a [`CrashReport`] to `<data dir>/crashes/`. Call once, as early in
/// `main` as possible, in both binaries (`streamboat`, `streamboatd`).
pub fn install_panic_hook(
    crashes_dir: PathBuf,
    app: &'static str,
    version: &'static str,
    ring: Arc<RingBuffer>,
) {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_hook(info);
        if let Err(e) = write_report(&crashes_dir, app, version, &ring, info) {
            eprintln!("streamboat: could not write a crash report to {crashes_dir:?}: {e}");
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `install_panic_hook` replaces *process-global* state; the two tests
    /// below each install one and immediately trigger it, so they must not
    /// run concurrently with each other (both live in this one test binary,
    /// which runs its tests on multiple threads by default) or one test's
    /// synthetic panic could be caught by the other's hook mid-installation.
    static PANIC_HOOK_TEST_SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn ring_buffer_evicts_the_oldest_line_past_capacity() {
        let ring = RingBuffer::new(3);
        for i in 0..5 {
            ring.push(format!("line {i}"));
        }
        assert_eq!(
            ring.snapshot(),
            vec!["line 2".to_string(), "line 3".into(), "line 4".into()]
        );
    }

    #[test]
    fn a_panic_on_a_spawned_thread_writes_a_crash_report_with_the_expected_fields() {
        let _serial = PANIC_HOOK_TEST_SERIAL.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let crashes_dir = dir.path().join("crashes");
        let ring = Arc::new(RingBuffer::new(50));
        ring.push("player: started track 42".into());
        install_panic_hook(crashes_dir.clone(), "streamboat-test", "9.9.9", ring);

        let handle = std::thread::spawn(|| {
            panic!("synthetic panic for the crash-report test");
        });
        let _ = handle.join();

        let mut entries: Vec<_> = std::fs::read_dir(&crashes_dir)
            .expect("crashes dir must exist")
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries.len(), 1, "exactly one crash report expected");
        let path = entries.remove(0).path();
        let body = std::fs::read_to_string(&path).unwrap();
        let report: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(report["app"], "streamboat-test");
        assert_eq!(report["version"], "9.9.9");
        assert!(
            report["message"]
                .as_str()
                .unwrap()
                .contains("synthetic panic for the crash-report test")
        );
        assert!(report["location"].as_str().unwrap().contains("crash.rs"));
        assert!(report["os"].as_str().unwrap() == std::env::consts::OS);
        assert_eq!(
            report["recent_log_lines"].as_array().unwrap()[0],
            "player: started track 42"
        );
    }

    #[test]
    fn crash_report_redacts_a_token_that_leaked_into_the_panic_message() {
        let _serial = PANIC_HOOK_TEST_SERIAL.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let crashes_dir = dir.path().join("crashes");
        let ring = Arc::new(RingBuffer::new(10));
        install_panic_hook(crashes_dir.clone(), "streamboat-test", "0.0.1", ring);

        let handle = std::thread::spawn(|| {
            panic!("unexpected header Authorization: Bearer leaked-secret-token-value");
        });
        let _ = handle.join();

        let entry = std::fs::read_dir(&crashes_dir)
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        let body = std::fs::read_to_string(entry.path()).unwrap();
        assert!(!body.contains("leaked-secret-token-value"));
        assert!(body.contains("<redacted>"));
    }
}
