//! Picks the compiled-in [`Engine`] backend and lists output devices for it
//! (D-016). Both `streamboatd` and the desktop shell go through the two
//! functions here instead of naming `GstEngine`/`MpvEngine` directly, so
//! engine selection lives in one place and Cargo's lack of per-target
//! default features (see this crate's `Cargo.toml` header comment) is a
//! build-time concern, not something every caller has to re-encode.
//!
//! [`default_engine`] resolves to whichever backend is actually compiled in,
//! preferring D-016's platform pick when more than one is available (the
//! `cargo test -p streamboat-player --features mpv` CI job, for instance,
//! builds *both* `gstreamer` (default-on) and `mpv` on Linux; this still
//! returns [`crate::gst::GstEngine`] there, matching D-016's "GStreamer on
//! Linux" — [`crate::mpv::MpvEngine`] stays reachable in that build only by
//! constructing it directly, exactly as `mpv.rs`'s own tests do). A build
//! with neither engine feature compiled is a programmer error, not a
//! runtime one, hence `compile_error!` rather than an `EngineError`.

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use crate::engine::{Engine, EngineEvent, EngineResult};

/// One audio output device an engine backend can see, for a device picker.
/// `id` is what to pass back as `OutputConfig`'s `device` field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
    /// Whether this entry names a concrete piece of hardware an exclusive
    /// open can target — `false` for a driver's generic "auto"/default
    /// placeholder entry, which exclusive mode (D-017) cannot open.
    pub exclusive_capable: bool,
}

#[cfg(all(target_os = "linux", feature = "gstreamer"))]
pub fn default_engine(
    events: Sender<EngineEvent>,
    output: streamboat_core::proto::OutputConfig,
    runtime_dir: PathBuf,
) -> EngineResult<Box<dyn Engine>> {
    Ok(Box::new(crate::gst::GstEngine::new(
        events,
        output,
        runtime_dir,
    )?))
}

#[cfg(all(not(all(target_os = "linux", feature = "gstreamer")), feature = "mpv"))]
pub fn default_engine(
    events: Sender<EngineEvent>,
    output: streamboat_core::proto::OutputConfig,
    runtime_dir: PathBuf,
) -> EngineResult<Box<dyn Engine>> {
    Ok(Box::new(crate::mpv::MpvEngine::new(
        events,
        output,
        runtime_dir,
    )?))
}

/// Reached only when `mpv` is off and this is either not Linux, or Linux
/// without `gstreamer` — a fallback for a `gstreamer`-only build on a
/// platform D-016 does not target in production, kept only so that feature
/// combination still compiles to *something* rather than needing a fifth,
/// even-more-exotic predicate.
#[cfg(all(
    not(all(target_os = "linux", feature = "gstreamer")),
    not(feature = "mpv"),
    feature = "gstreamer"
))]
pub fn default_engine(
    events: Sender<EngineEvent>,
    output: streamboat_core::proto::OutputConfig,
    runtime_dir: PathBuf,
) -> EngineResult<Box<dyn Engine>> {
    Ok(Box::new(crate::gst::GstEngine::new(
        events,
        output,
        runtime_dir,
    )?))
}

// A build with neither engine feature compiled has no `default_engine` at
// all; the `compile_error!` below is what actually explains why, since the
// resulting "cannot find function" at every call site would not.
#[cfg(not(any(feature = "gstreamer", feature = "mpv")))]
compile_error!(
    "streamboat-player needs at least one engine backend compiled in: enable the `gstreamer` \
     feature (Linux, default) or the `mpv` feature (Windows/macOS, or Linux for testing — see \
     Cargo.toml's header comment for the exact invocation)."
);

/// GStreamer `DeviceMonitor` on Linux, as `streamboat-desktop`'s `devices`
/// CLI subcommand already did — moved here so `streamboatd` can list devices
/// too, without duplicating the logic (`os-integration.md` §1/§3's table:
/// `device.api=="alsa"`, `api.alsa.path` or `alsa.card`+`alsa.device` ->
/// `hw:C,D`).
#[cfg(all(target_os = "linux", feature = "gstreamer"))]
pub fn enumerate_output_devices() -> Vec<OutputDevice> {
    use gstreamer::prelude::*;

    if crate::gst::ensure_init().is_err() {
        return Vec::new();
    }
    let monitor = gstreamer::DeviceMonitor::new();
    monitor.add_filter(Some("Audio/Sink"), None);
    if monitor.start().is_err() {
        return Vec::new();
    }
    let devices = monitor
        .devices()
        .into_iter()
        .map(|d| {
            let props = d.properties();
            let alsa_path = props
                .as_ref()
                .and_then(|p| p.get::<&str>("api.alsa.path").ok())
                .map(str::to_string);
            let alsa_card_device = props.as_ref().and_then(|p| {
                let card = p.get::<i32>("alsa.card").ok()?;
                let device = p.get::<i32>("alsa.device").ok().unwrap_or(0);
                Some(format!("hw:{card},{device}"))
            });
            let hw_id = alsa_path.or(alsa_card_device);
            OutputDevice {
                id: hw_id
                    .clone()
                    .unwrap_or_else(|| d.display_name().to_string()),
                name: d.display_name().to_string(),
                exclusive_capable: hw_id.is_some(),
            }
        })
        .collect();
    monitor.stop();
    devices
}

/// mpv's `audio-device-list` property (a read-only `MPV_FORMAT_NODE`,
/// rendered here the same way `mpv.rs`'s `describe_node_json` reads other
/// node properties): a short-lived, otherwise-idle `Mpv` instance is enough
/// to ask, without opening any device or affecting a real [`MpvEngine`].
/// Prefixed entries not matching this platform's own AO driver (`wasapi` on
/// Windows, `coreaudio`/`coreaudio_exclusive` on macOS) are dropped, since a
/// libmpv build can list every AO driver it was compiled with, not only the
/// one this platform's `mpv.rs` actually selects.
#[cfg(all(not(all(target_os = "linux", feature = "gstreamer")), feature = "mpv"))]
pub fn enumerate_output_devices() -> Vec<OutputDevice> {
    let Ok(mpv) = libmpv2::Mpv::with_initializer(|init| {
        init.set_option("vid", "no")?;
        init.set_option("audio-display", "no")?;
        init.set_option("terminal", "no")?;
        init.set_option("idle", "yes")?;
        Ok(())
    }) else {
        return Vec::new();
    };
    let Ok(raw) = mpv.get_property::<String>("audio-device-list") else {
        return Vec::new();
    };
    parse_mpv_device_list(&raw)
}

// Deliberately *not* gated on the same "am I the preferred backend"
// predicate as the `enumerate_output_devices` above: this is pure JSON
// mapping, so it stays compiled (and tested — see the `parses_mpv_device_
// list_json` test below) under plain `--features mpv` on Linux too, even
// though GStreamer wins there and this particular helper goes unused in
// that one build combination (hence the narrow `allow`, not a blanket one).
#[cfg(feature = "mpv")]
#[cfg_attr(all(target_os = "linux", feature = "gstreamer"), allow(dead_code))]
fn platform_ao_prefix() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "wasapi/"
    }
    #[cfg(target_os = "macos")]
    {
        "coreaudio"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        ""
    }
}

#[cfg(feature = "mpv")]
#[cfg_attr(all(target_os = "linux", feature = "gstreamer"), allow(dead_code))]
fn parse_mpv_device_list(json: &str) -> Vec<OutputDevice> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(entries) = value.as_array() else {
        return Vec::new();
    };
    let prefix = platform_ao_prefix();
    entries
        .iter()
        .filter_map(|entry| {
            let id = entry.get("name")?.as_str()?.to_string();
            if id != "auto" && !prefix.is_empty() && !id.starts_with(prefix) {
                return None;
            }
            let name = entry
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or(id.as_str())
                .to_string();
            Some(OutputDevice {
                exclusive_capable: id != "auto",
                id,
                name,
            })
        })
        .collect()
}

/// Reached when device enumeration has no backend to ask (neither
/// `gstreamer` on Linux nor `mpv` compiled): an empty list, not a build
/// failure — unlike [`default_engine`], not being able to *list* devices
/// still leaves manual `OutputConfig` device strings usable.
#[cfg(not(any(all(target_os = "linux", feature = "gstreamer"), feature = "mpv")))]
pub fn enumerate_output_devices() -> Vec<OutputDevice> {
    tracing::debug!(
        "enumerate_output_devices: no device-enumeration backend compiled in for this target"
    );
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "mpv")]
    #[test]
    fn parses_mpv_device_list_json() {
        let json = r#"[
            {"name": "auto", "description": "Autoselect device"},
            {"name": "wasapi/{guid}", "description": "Speakers"},
            {"name": "coreaudio/AppleHDAEngineOutput:1", "description": "Built-in Output"}
        ]"#;
        let devices = parse_mpv_device_list(json);
        assert!(
            devices
                .iter()
                .any(|d| d.id == "auto" && !d.exclusive_capable)
        );
    }

    #[cfg(all(target_os = "linux", feature = "gstreamer"))]
    #[test]
    fn linux_device_enumeration_does_not_panic() {
        // No real audio hardware is guaranteed in CI; this only checks the
        // GStreamer DeviceMonitor round trip completes without panicking,
        // the same guarantee `streamboat-desktop`'s prior `devices` command
        // relied on informally.
        let _ = enumerate_output_devices();
    }

    #[cfg(all(target_os = "linux", feature = "gstreamer"))]
    #[test]
    fn default_engine_constructs_the_gstreamer_backend() {
        unsafe {
            std::env::set_var("STREAMBOAT_GST_SINK", "fakesink");
        }
        let (tx, _rx) = std::sync::mpsc::channel();
        let dir = std::env::temp_dir().join("streamboat-platform-test");
        let engine = default_engine(tx, streamboat_core::proto::OutputConfig::default(), dir)
            .expect("default_engine should build the GStreamer backend with fakesink");
        assert_eq!(engine.name(), "gstreamer");
    }
}
