//! Crash dumps, redacted structured logging, and the debug bundle (D-029).
//! Both binaries (`streamboat`, `streamboatd`) call [`init`] once, as early
//! in `main` as possible, and keep the returned guard alive for the
//! process's life.
//!
//! Nothing here ever leaves the machine unprompted: no hosted crash
//! service, no ingest key, no telemetry. `<data dir>/logs/` and
//! `<data dir>/crashes/` are the only two directories this module writes
//! to; `streamboat debug-bundle` ([`bundle::create`]) reads only those two
//! plus whatever the caller passes in directly.

pub mod bundle;
pub mod crash;
mod logging;
mod redact;

use std::path::Path;
use std::sync::Arc;

pub use crash::{CrashReport, RingBuffer};
pub use logging::DiagnosticsGuard;
pub use redact::redact;

/// Installs the panic hook and structured logging described in the module
/// doc comment. `app`/`version` should be `'static` string literals (e.g.
/// `"streamboat"`, `env!("CARGO_PKG_VERSION")`) — both binaries call this
/// exactly once, so a `'static` bound costs nothing and keeps
/// `install_panic_hook`'s boxed closure simple. `default_filter` is the
/// `RUST_LOG` fallback when that variable is unset — each binary's own
/// previous default (`"warn"` for the desktop CLI, `"info"` for the daemon)
/// so this is a redaction/rotation change, not a verbosity change.
pub fn init(
    data_dir: &Path,
    app: &'static str,
    version: &'static str,
    default_filter: &str,
) -> DiagnosticsGuard {
    let ring = Arc::new(RingBuffer::new(200));
    crash::install_panic_hook(data_dir.join("crashes"), app, version, ring.clone());
    logging::init_logging(data_dir, ring, default_filter)
}
