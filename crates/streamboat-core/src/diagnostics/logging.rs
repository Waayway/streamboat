//! Structured `tracing` logging to `<data dir>/logs/streamboat.log`, with
//! rotation and redaction by construction (D-029): every event that reaches
//! the file layer is formatted through [`RedactingFormat`], which redacts
//! the *complete* formatted line (fields and message together) rather than
//! trusting call sites to redact their own arguments. Also feeds
//! [`RingBuffer`] (via [`RingBufferLayer`]) so a later panic has recent
//! context to write into its crash report.

use std::fmt;
use std::path::Path;
use std::sync::{Arc, Mutex};

use file_rotate::suffix::AppendCount;
use file_rotate::{ContentLimit, FileRotate};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::FormatEvent;
use tracing_subscriber::fmt::format::{Format, Writer};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use super::crash::RingBuffer;
use super::redact::redact;

/// Sone's numbers (`streamboat-engineering-baseline/references/config-cache-logs-telemetry.md`
/// §4): rotate once the current file passes 5 MB, keep 9 rotated files —
/// roughly a 50 MB ceiling.
const ROTATE_AT_BYTES: usize = 5_000_000;
const KEEP_ROTATED_FILES: usize = 9;

/// Wraps any [`FormatEvent`] (the default `Format` here) so the *entire*
/// formatted line is redacted before it reaches the real writer — formats
/// into a scratch `String` first, since [`super::redact::redact`] operates
/// on complete lines, not on one field at a time.
struct RedactingFormat<F> {
    inner: F,
}

impl<S, N, F> FormatEvent<S, N> for RedactingFormat<F>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> tracing_subscriber::fmt::FormatFields<'a> + 'static,
    F: FormatEvent<S, N>,
{
    fn format_event(
        &self,
        ctx: &tracing_subscriber::fmt::FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let mut buf = String::new();
        {
            let scratch = Writer::new(&mut buf);
            self.inner.format_event(ctx, scratch, event)?;
        }
        writer.write_str(&redact(&buf))
    }
}

/// A `Write` implementation over a shared, mutex-guarded [`FileRotate`] —
/// `tracing_subscriber::fmt::Layer::with_writer` needs a `MakeWriter`, which
/// this satisfies via the blanket impl for `Fn() -> W where W: std::io::Write`
/// once wrapped in a `move || writer.clone()` closure at the call site.
#[derive(Clone)]
struct SharedRotatingWriter(Arc<Mutex<FileRotate<AppendCount>>>);

impl std::io::Write for SharedRotatingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}

/// Feeds every event's message into [`RingBuffer`] (already-redacted, so a
/// crash report built from it needs no second pass) — separate from the
/// file/stderr `fmt` layers below because it needs only the plain text, not
/// a fully formatted-with-timestamp line.
pub(super) struct RingBufferLayer {
    ring: Arc<RingBuffer>,
}

#[derive(Default)]
struct MessageVisitor(String);

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        } else if self.0.is_empty() {
            self.0 = format!("{}={:?}", field.name(), value);
        }
    }
}

impl<S: Subscriber> Layer<S> for RingBufferLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let line = format!(
            "{} {} {}",
            event.metadata().level(),
            event.metadata().target(),
            visitor.0
        );
        self.ring.push(redact(&line));
    }
}

/// Kept alive for the process's lifetime (`let _diagnostics = init(...)`).
/// Reserved for a future non-blocking writer; today the file writer is
/// synchronous (`file-rotate`'s own `Write` impl, behind a `Mutex`), so
/// dropping this early loses nothing — it exists so call sites read the
/// same way a `tracing-appender` guard would, in case that changes later.
pub struct DiagnosticsGuard;

/// Initializes the global `tracing` subscriber: an `EnvFilter` (`RUST_LOG`,
/// default `info`) gating three layers — stderr (unredacted, exactly what
/// both binaries printed before this module existed), the redacted rotating
/// file at `<data_dir>/logs/streamboat.log`, and the ring buffer a later
/// panic reads from. Call once, as early in `main` as possible, in both
/// binaries. Falls back to stderr-only (still redacted-on-screen is not
/// attempted — only the file layer is redacted, matching the D-029 scope)
/// if the log directory cannot be created, rather than failing to start.
pub fn init_logging(
    data_dir: &Path,
    ring: Arc<RingBuffer>,
    default_filter: &str,
) -> DiagnosticsGuard {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));
    let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
    let ring_layer = RingBufferLayer { ring };

    let logs_dir = data_dir.join("logs");
    let file_layer = match std::fs::create_dir_all(&logs_dir) {
        Ok(()) => {
            let rotate = FileRotate::new(
                logs_dir.join("streamboat.log"),
                AppendCount::new(KEEP_ROTATED_FILES),
                ContentLimit::BytesSurpassed(ROTATE_AT_BYTES),
                file_rotate::compression::Compression::None,
                None,
            );
            let writer = SharedRotatingWriter(Arc::new(Mutex::new(rotate)));
            Some(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .event_format(RedactingFormat {
                        inner: Format::default(),
                    })
                    .with_writer(move || writer.clone()),
            )
        }
        Err(e) => {
            eprintln!(
                "streamboat: could not create {}: {e}; file logging disabled, stderr only",
                logs_dir.display()
            );
            None
        }
    };

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(stderr_layer)
        .with(ring_layer)
        .with(file_layer)
        .try_init();
    DiagnosticsGuard
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_layer_captures_and_redacts_event_messages() {
        let ring = Arc::new(RingBuffer::new(10));
        let layer = RingBufferLayer { ring: ring.clone() };
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("player: started track 42");
            tracing::warn!("Authorization: Bearer leaked-secret-token");
        });
        let lines = ring.snapshot();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("started track 42"));
        assert!(!lines[1].contains("leaked-secret-token"));
        assert!(lines[1].contains("<redacted>"));
    }
}
