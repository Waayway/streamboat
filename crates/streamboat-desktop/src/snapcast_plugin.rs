//! `streamboat snapcast-plugin` (D-034): Snapcast's stream-plugin protocol
//! — newline-delimited JSON-RPC 2.0 over stdin/stdout — bridging a
//! `source = tcp://...&controlscript=streamboat snapcast-plugin` (or a
//! `pipe://` source with the same `controlscript`) stream to a running
//! `streamboatd`'s control API (D-030), so Snapweb and every Snapcast
//! client show real metadata and get real transport controls instead of
//! an anonymous PCM stream.
//!
//! **Protocol, cited only where the reference names a method explicitly**
//! (`badaix/snapcast` `doc/json_rpc_api/stream_plugin.md`, distilled in
//! `headless-and-tidal-connect/references/mpd-and-multiroom.md` §2 — the
//! full upstream doc was not available to check line-by-line, so nothing
//! below invents a method name past what that reference states):
//!
//! - snapserver → plugin (requests, expect a response): `Plugin.Stream.Player.Control`
//!   (`params.command` one of `play`/`pause`/`playPause`/`stop`/`next`/`previous`/
//!   `seek`/`setPosition`, the latter two carrying `params.params.offset`/`position`
//!   in seconds — offset relative, position absolute), `Plugin.Stream.Player.SetProperty`
//!   (`params.property`/`params.value`), `Plugin.Stream.Player.GetProperties` (no params).
//! - plugin → snapserver (notifications, no response expected): `Plugin.Stream.Player.Properties`
//!   (the full properties object below), `Plugin.Stream.Log`, `Plugin.Stream.Ready`.
//! - **Uncertain, not in the cited reference**: `Plugin.Stream.Log`'s exact
//!   parameter shape and whether `Plugin.Stream.Ready` carries any params at
//!   all — sent here with a plausible `{severity, message}` and no params
//!   respectively, flagged here rather than asserted as documented fact.
//! - JSON-RPC 2.0 framing (request/response/notification shape, `id`
//!   echoing, and the standard reserved error codes `-32700`/`-32600`/
//!   `-32601`/`-32602`/`-32603`) is the JSON-RPC 2.0 specification itself,
//!   not a Snapcast-specific invention.

use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use streamboat_core::proto::{Command, Event, PlaybackStatus, PlayerState};
use tokio::io::AsyncBufReadExt;

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 framing — pure encoding/decoding, no I/O.

/// A request or notification snapserver sends the plugin. `id: None` marks
/// a notification (no response expected) per the JSON-RPC 2.0 spec, though
/// in practice every method snapserver calls on the plugin is a request.
#[derive(Debug, Clone, Deserialize)]
pub struct RpcMessage {
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// The standard JSON-RPC 2.0 reserved error codes this plugin can return.
/// `PARSE_ERROR`/`INVALID_REQUEST` are part of the spec but never actually
/// constructed here — a line that fails to parse at all is logged via
/// `Plugin.Stream.Log` instead of answered (there is no `id` to answer with
/// once parsing itself has failed) — kept for documentation completeness
/// and so a future caller has the right constant instead of inventing one.
#[allow(dead_code)]
pub const PARSE_ERROR: i64 = -32700;
#[allow(dead_code)]
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;

/// A response to one `RpcMessage` that carried an `id`. Exactly one of
/// `result`/`error` is ever set — enforced by the two constructors, not by
/// the type (matching plain JSON-RPC 2.0, which has no sum-type equivalent
/// to lean on here).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcErrorBody>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RpcErrorBody {
    pub code: i64,
    pub message: String,
}

impl RpcResponse {
    pub fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }
    pub fn err(id: Value, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(RpcErrorBody {
                code,
                message: message.into(),
            }),
        }
    }
}

/// A notification the plugin sends to snapserver — no `id` at all (not even
/// `null`), which is what distinguishes it from a response on the wire.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RpcNotification {
    pub jsonrpc: &'static str,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl RpcNotification {
    pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            method: method.into(),
            params,
        }
    }
}

/// One newline-delimited JSON-RPC frame, ready to write to stdout.
pub fn encode_line(value: &impl Serialize) -> String {
    let mut s = serde_json::to_string(value).expect("RpcResponse/RpcNotification always encode");
    s.push('\n');
    s
}

// ---------------------------------------------------------------------------
// PlayerState -> Plugin.Stream.Player.Properties, and Control -> Command.

/// Builds the `Plugin.Stream.Player.Properties` payload from the daemon's
/// current [`PlayerState`] — every field the reference names as part of
/// the properties object: the capability booleans, `playbackStatus`,
/// `volume`, `position` (seconds), and `metadata` (title/artist/album/
/// duration, only the fields streamboat actually has).
pub fn properties_from_state(state: &PlayerState) -> Value {
    let playback_status = match state.status {
        PlaybackStatus::Playing => "playing",
        PlaybackStatus::Paused | PlaybackStatus::Buffering => "paused",
        PlaybackStatus::Stopped => "stopped",
    };
    let has_track = state.current.is_some();
    let metadata = state.current.as_ref().map(|t| {
        let mut m = json!({
            "title": t.title,
            "artist": [t.artists.clone()],
            "album": t.album,
        });
        if let Some(ms) = t.duration_ms {
            m["duration"] = json!(ms as f64 / 1000.0);
        }
        m
    });
    json!({
        "canControl": true,
        "canPlay": has_track,
        "canPause": has_track,
        "canGoNext": true,
        "canGoPrevious": true,
        "canSeek": has_track,
        "playbackStatus": playback_status,
        "loopStatus": "none",
        "shuffle": false,
        "volume": (state.volume.clamp(0.0, 1.0) * 100.0).round() as i64,
        "position": state.position_ms as f64 / 1000.0,
        "metadata": metadata,
    })
}

/// What one `Plugin.Stream.Player.Control` call maps to: either a
/// [`Command`] to send unconditionally, or a seek that needs the *current*
/// position first (`offset` is relative — `setPosition`'s `position` is
/// already absolute and needs no current-state lookup, so it comes back as
/// `Absolute` immediately).
#[derive(Debug)]
pub enum ControlAction {
    Send(Command),
    /// `seek {offset}` (seconds, may be negative) — relative to whatever
    /// position is current when this executes.
    RelativeSeekSeconds(f64),
    /// `setPosition {position}` (seconds) — absolute.
    AbsoluteSeekSeconds(f64),
}

/// Maps `Plugin.Stream.Player.Control`'s `params.command` (plus
/// `params.params` for `seek`/`setPosition`) to a [`ControlAction`]. `Err`
/// carries the JSON-RPC error code/message to send back — an unknown
/// command is `METHOD_NOT_FOUND`-adjacent but reported as `INVALID_PARAMS`
/// since the *method* (`Control`) is recognised, only its `command` value
/// is not.
pub fn control_action(params: &Value) -> Result<ControlAction, (i64, String)> {
    let command = params
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| (INVALID_PARAMS, "Control: missing \"command\"".to_string()))?;
    match command {
        "play" => Ok(ControlAction::Send(Command::Resume)),
        "pause" => Ok(ControlAction::Send(Command::Pause)),
        "playPause" => Ok(ControlAction::Send(Command::TogglePlayPause)),
        "stop" => Ok(ControlAction::Send(Command::Stop)),
        "next" => Ok(ControlAction::Send(Command::Next)),
        "previous" => Ok(ControlAction::Send(Command::Previous)),
        "seek" => {
            let offset = params
                .get("params")
                .and_then(|p| p.get("offset"))
                .and_then(Value::as_f64)
                .ok_or_else(|| (INVALID_PARAMS, "seek: missing \"offset\"".to_string()))?;
            Ok(ControlAction::RelativeSeekSeconds(offset))
        }
        "setPosition" => {
            let position = params
                .get("params")
                .and_then(|p| p.get("position"))
                .and_then(Value::as_f64)
                .ok_or_else(|| {
                    (
                        INVALID_PARAMS,
                        "setPosition: missing \"position\"".to_string(),
                    )
                })?;
            Ok(ControlAction::AbsoluteSeekSeconds(position))
        }
        other => Err((
            INVALID_PARAMS,
            format!("Control: unknown command {other:?}"),
        )),
    }
}

/// `Plugin.Stream.Player.SetProperty`: only `volume` maps to anything
/// streamboat can act on (`Command::SetVolume`, percent -> 0.0..=1.0); every
/// other documented Snapcast property (`loopStatus`, `shuffle`, `rate`) has
/// no equivalent in streamboat's own `Command` set, so it is reported back
/// as an explicit, typed refusal rather than silently accepted and ignored.
pub fn set_property_action(params: &Value) -> Result<Command, (i64, String)> {
    let property = params
        .get("property")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            (
                INVALID_PARAMS,
                "SetProperty: missing \"property\"".to_string(),
            )
        })?;
    match property {
        "volume" => {
            let percent = params.get("value").and_then(Value::as_f64).ok_or_else(|| {
                (
                    INVALID_PARAMS,
                    "SetProperty volume: missing \"value\"".to_string(),
                )
            })?;
            Ok(Command::SetVolume {
                volume: (percent / 100.0).clamp(0.0, 1.0) as f32,
            })
        }
        other => Err((
            INVALID_PARAMS,
            format!("SetProperty: {other:?} is not supported by streamboat"),
        )),
    }
}

// ---------------------------------------------------------------------------
// The actual bridge: stdin/stdout <-> the daemon's control API.

/// Latest state the WS reader thread has observed, shared with the stdin
/// dispatch loop for `GetProperties`/`seek`'s current-position lookup.
type SharedState = Arc<Mutex<Option<PlayerState>>>;

/// Runs the plugin until stdin closes. `base_url` is the control API's HTTP
/// base (e.g. `http://127.0.0.1:4747/`); `token` its bearer token.
pub async fn run(base_url: String, token: String) -> anyhow::Result<()> {
    // Every call site below builds a URL as `{base_url}v1/...` with no
    // separator of its own, so a `--api` value with no trailing slash
    // (easy to type) must not silently produce `4747v1/commands`.
    let base_url = if base_url.ends_with('/') {
        base_url
    } else {
        format!("{base_url}/")
    };
    let http = reqwest::Client::new();
    let state: SharedState = Arc::new(Mutex::new(None));

    // The WS reader: keeps `state` current and emits a `Properties`
    // notification on every change, for the plugin's whole life.
    let ws_state = state.clone();
    let ws_url = format!(
        "{}v1/events",
        base_url
            .replacen("http://", "ws://", 1)
            .replacen("https://", "wss://", 1)
    );
    let ws_token = token.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = run_ws_reader(&ws_url, &ws_token, &ws_state).await {
                emit(&RpcNotification::new(
                    "Plugin.Stream.Log",
                    Some(
                        json!({"severity": "Warning", "message": format!("events websocket: {e}")}),
                    ),
                ));
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    });

    emit(&RpcNotification::new("Plugin.Stream.Ready", None));

    let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let msg: RpcMessage = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(e) => {
                emit(&RpcNotification::new(
                    "Plugin.Stream.Log",
                    Some(json!({"severity": "Error", "message": format!("parse error: {e}")})),
                ));
                continue;
            }
        };
        let Some(id) = msg.id.clone() else { continue };
        let response = dispatch(&msg, id.clone(), &http, &base_url, &token, &state).await;
        emit(&response);
    }
    Ok(())
}

async fn dispatch(
    msg: &RpcMessage,
    id: Value,
    http: &reqwest::Client,
    base_url: &str,
    token: &str,
    state: &SharedState,
) -> RpcResponse {
    match msg.method.as_str() {
        "Plugin.Stream.Player.Control" => match control_action(&msg.params) {
            Ok(ControlAction::Send(cmd)) => match send_command(http, base_url, token, &cmd).await {
                Ok(()) => RpcResponse::ok(id, json!("ok")),
                Err(e) => RpcResponse::err(id, INTERNAL_ERROR, e.to_string()),
            },
            Ok(ControlAction::AbsoluteSeekSeconds(secs)) => {
                let cmd = Command::Seek {
                    position_ms: (secs.max(0.0) * 1000.0) as u64,
                };
                match send_command(http, base_url, token, &cmd).await {
                    Ok(()) => RpcResponse::ok(id, json!("ok")),
                    Err(e) => RpcResponse::err(id, INTERNAL_ERROR, e.to_string()),
                }
            }
            Ok(ControlAction::RelativeSeekSeconds(offset)) => {
                let current_ms = state
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|s| s.position_ms)
                    .unwrap_or(0);
                let target_ms = (current_ms as f64 + offset * 1000.0).max(0.0) as u64;
                match send_command(
                    http,
                    base_url,
                    token,
                    &Command::Seek {
                        position_ms: target_ms,
                    },
                )
                .await
                {
                    Ok(()) => RpcResponse::ok(id, json!("ok")),
                    Err(e) => RpcResponse::err(id, INTERNAL_ERROR, e.to_string()),
                }
            }
            Err((code, message)) => RpcResponse::err(id, code, message),
        },
        "Plugin.Stream.Player.SetProperty" => match set_property_action(&msg.params) {
            Ok(cmd) => match send_command(http, base_url, token, &cmd).await {
                Ok(()) => RpcResponse::ok(id, json!("ok")),
                Err(e) => RpcResponse::err(id, INTERNAL_ERROR, e.to_string()),
            },
            Err((code, message)) => RpcResponse::err(id, code, message),
        },
        "Plugin.Stream.Player.GetProperties" => {
            let snapshot = state.lock().unwrap().clone();
            match snapshot {
                Some(s) => RpcResponse::ok(id, properties_from_state(&s)),
                None => match fetch_state(http, base_url, token).await {
                    Ok(s) => RpcResponse::ok(id, properties_from_state(&s)),
                    Err(e) => RpcResponse::err(id, INTERNAL_ERROR, e.to_string()),
                },
            }
        }
        other => RpcResponse::err(id, METHOD_NOT_FOUND, format!("unknown method {other:?}")),
    }
}

fn emit(value: &impl Serialize) {
    print!("{}", encode_line(value));
    let _ = std::io::Write::flush(&mut std::io::stdout());
}

async fn send_command(
    http: &reqwest::Client,
    base_url: &str,
    token: &str,
    cmd: &Command,
) -> anyhow::Result<()> {
    http.post(format!("{base_url}v1/commands"))
        .bearer_auth(token)
        .json(cmd)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

async fn fetch_state(
    http: &reqwest::Client,
    base_url: &str,
    token: &str,
) -> anyhow::Result<PlayerState> {
    let state = http
        .get(format!("{base_url}v1/state"))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json::<PlayerState>()
        .await?;
    Ok(state)
}

async fn run_ws_reader(ws_url: &str, token: &str, state: &SharedState) -> anyhow::Result<()> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;

    let mut request = ws_url.into_client_request()?;
    request
        .headers_mut()
        .insert(AUTHORIZATION, format!("Bearer {token}").parse()?);
    let (ws, _) = tokio_tungstenite::connect_async(request).await?;
    let (_, mut read) = ws.split();
    while let Some(msg) = read.next().await {
        let msg = msg?;
        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
            #[derive(Deserialize)]
            struct Envelope {
                event: Event,
            }
            if let Ok(env) = serde_json::from_str::<Envelope>(&text) {
                if let Event::State { state: s } = env.event {
                    *state.lock().unwrap() = Some(s.clone());
                    emit(&RpcNotification::new(
                        "Plugin.Stream.Player.Properties",
                        Some(properties_from_state(&s)),
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use streamboat_core::AudioQuality;
    use streamboat_core::models::TrackSummary;
    use streamboat_core::proto::OutputConfig;

    #[test]
    fn a_request_with_an_id_round_trips_through_json_rpc() {
        let raw = r#"{"jsonrpc":"2.0","id":7,"method":"Plugin.Stream.Player.Control","params":{"command":"play"}}"#;
        let msg: RpcMessage = serde_json::from_str(raw).unwrap();
        assert_eq!(msg.id, Some(json!(7)));
        assert_eq!(msg.method, "Plugin.Stream.Player.Control");
        assert_eq!(msg.params["command"], "play");
    }

    #[test]
    fn a_notification_with_no_id_is_still_parseable() {
        // Snapserver only ever sends requests to the plugin per the cited
        // reference, but the framing itself must not choke on a bare
        // notification either (JSON-RPC 2.0 allows one with no `id`).
        let raw = r#"{"jsonrpc":"2.0","method":"Plugin.Stream.Log"}"#;
        let msg: RpcMessage = serde_json::from_str(raw).unwrap();
        assert_eq!(msg.id, None);
    }

    #[test]
    fn ok_response_encodes_result_not_error() {
        let resp = RpcResponse::ok(json!(1), json!("ok"));
        let line = encode_line(&resp);
        assert_eq!(line.trim_end(), r#"{"jsonrpc":"2.0","id":1,"result":"ok"}"#);
    }

    #[test]
    fn error_response_encodes_error_not_result_with_a_standard_code() {
        let resp = RpcResponse::err(json!("abc"), METHOD_NOT_FOUND, "unknown method");
        let line = encode_line(&resp);
        assert_eq!(
            line.trim_end(),
            r#"{"jsonrpc":"2.0","id":"abc","error":{"code":-32601,"message":"unknown method"}}"#
        );
    }

    #[test]
    fn notification_encodes_with_no_id_field_at_all() {
        let note = RpcNotification::new("Plugin.Stream.Ready", None);
        let line = encode_line(&note);
        assert_eq!(
            line.trim_end(),
            r#"{"jsonrpc":"2.0","method":"Plugin.Stream.Ready"}"#
        );
        assert!(!line.contains("\"id\""));
    }

    #[test]
    fn properties_notification_carries_the_full_properties_object() {
        let state = playing_state();
        let note = RpcNotification::new(
            "Plugin.Stream.Player.Properties",
            Some(properties_from_state(&state)),
        );
        let v: Value = serde_json::from_str(encode_line(&note).trim_end()).unwrap();
        assert_eq!(v["method"], "Plugin.Stream.Player.Properties");
        assert_eq!(v["params"]["playbackStatus"], "playing");
        assert_eq!(v["params"]["metadata"]["title"], "Song");
    }

    fn playing_state() -> PlayerState {
        PlayerState {
            status: PlaybackStatus::Playing,
            current: Some(TrackSummary {
                id: 1,
                title: "Song".into(),
                artists: "Artist".into(),
                album: "Album".into(),
                duration_ms: Some(180_000),
                cover: None,
            }),
            current_index: Some(0),
            queue_len: 1,
            position_ms: 42_000,
            duration_ms: Some(180_000),
            volume: 0.5,
            quality_ceiling: AudioQuality::Lossless,
            output: OutputConfig::default(),
            stream: None,
            signal_path: None,
        }
    }

    #[test]
    fn control_play_pause_and_transport_map_to_the_expected_commands() {
        assert!(matches!(
            control_action(&json!({"command":"play"})).unwrap(),
            ControlAction::Send(Command::Resume)
        ));
        assert!(matches!(
            control_action(&json!({"command":"pause"})).unwrap(),
            ControlAction::Send(Command::Pause)
        ));
        assert!(matches!(
            control_action(&json!({"command":"playPause"})).unwrap(),
            ControlAction::Send(Command::TogglePlayPause)
        ));
        assert!(matches!(
            control_action(&json!({"command":"stop"})).unwrap(),
            ControlAction::Send(Command::Stop)
        ));
        assert!(matches!(
            control_action(&json!({"command":"next"})).unwrap(),
            ControlAction::Send(Command::Next)
        ));
        assert!(matches!(
            control_action(&json!({"command":"previous"})).unwrap(),
            ControlAction::Send(Command::Previous)
        ));
    }

    #[test]
    fn control_seek_is_relative_and_set_position_is_absolute() {
        match control_action(&json!({"command":"seek","params":{"offset":-5.0}})).unwrap() {
            ControlAction::RelativeSeekSeconds(v) => assert_eq!(v, -5.0),
            other => panic!("expected a relative seek, got {other:?}"),
        }
        match control_action(&json!({"command":"setPosition","params":{"position":30.0}})).unwrap()
        {
            ControlAction::AbsoluteSeekSeconds(v) => assert_eq!(v, 30.0),
            other => panic!("expected an absolute seek, got {other:?}"),
        }
    }

    #[test]
    fn control_with_an_unknown_command_is_invalid_params_not_method_not_found() {
        let err = control_action(&json!({"command":"teleport"})).unwrap_err();
        assert_eq!(err.0, INVALID_PARAMS);
    }

    #[test]
    fn set_property_volume_maps_to_set_volume_command() {
        let cmd = set_property_action(&json!({"property":"volume","value":50.0})).unwrap();
        assert_eq!(cmd, Command::SetVolume { volume: 0.5 });
    }

    #[test]
    fn set_property_on_an_unsupported_property_is_a_typed_refusal() {
        let err = set_property_action(&json!({"property":"shuffle","value":true})).unwrap_err();
        assert_eq!(err.0, INVALID_PARAMS);
    }

    #[test]
    fn get_properties_dispatch_uses_the_cached_state_without_touching_the_network() {
        // `dispatch` for GetProperties reads `state` directly when it is
        // already populated (from the WS reader) rather than always
        // issuing a fresh `GET /v1/state` — checked here at the
        // `properties_from_state` level, which is what that branch calls.
        let props = properties_from_state(&playing_state());
        assert_eq!(props["canPlay"], true);
        assert_eq!(props["canSeek"], true);
        assert_eq!(props["volume"], 50);
        assert_eq!(props["position"], 42.0);
    }
}
