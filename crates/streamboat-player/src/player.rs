//! The player: owns the queue, resolves streams through the API, drives one
//! [`Engine`], and speaks the protocol (`streamboat_core::proto`) and nothing
//! else. Front ends never touch the engine directly (D-010).
//!
//! Policies encoded here: an unplayable track is skipped with an `Error`
//! event, not a stop; the successor is resolved and handed to the engine as
//! soon as the current track starts (gapless); volume is refused with a
//! warning while an exclusive output is active (D-017).

use std::sync::Arc;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use streamboat_core::bootstrap::Context;
use streamboat_core::config::ReplayGainMode;
use streamboat_core::models::{Track, TrackSummary};
use streamboat_core::privileges::{PrivilegesEvent, StreamingPrivileges, hostname_display_name};
use streamboat_core::proto::{
    Command, Event, OutputConfig, PlaybackStatus, PlayerState, QueuePosition, StreamInfo,
};
use streamboat_core::reporting::{PlayEvent, PlayReporter, REPORT_THRESHOLD_MS};
use streamboat_core::scrobble::{
    LastfmScrobbler, ListenBrainzScrobbler, ScrobbleHub, ScrobbleTrack, Scrobbler,
};
use streamboat_core::{ApiClient, AudioQuality, Error as CoreError, ResolvedStream};
use tokio::sync::{broadcast, mpsc};

use crate::engine::{Engine, EngineEvent, LoadItem};
use crate::offline::OfflineCache;

/// Optional trait objects the player reports plays and claims streaming
/// privileges through. Every field defaults to `None` (used by every
/// existing test): a `Player` with no dependencies configured behaves
/// exactly as before this module existed.
#[derive(Default)]
pub struct PlayerDeps {
    /// The streaming-privileges ("Pushkin") client. `claim()` is called on
    /// genuine user intent only (D-033) — see the `Command::Play`/`Next`/
    /// `Previous`/`Resume` handlers below.
    pub privileges: Option<Arc<StreamingPrivileges>>,
    /// The event stream from the same [`StreamingPrivileges`] instance
    /// (from [`StreamingPrivileges::spawn`]); kept separate from the
    /// handle above because the receiver is not `Clone`.
    pub privileges_events: Option<mpsc::UnboundedReceiver<PrivilegesEvent>>,
    /// Reports finished/skipped plays to TIDAL (D-027).
    pub reporter: Option<Arc<PlayReporter>>,
    /// Scrobbles to Last.fm/ListenBrainz (D-037).
    pub scrobbler: Option<Arc<dyn Scrobbler>>,
    /// The pinned, encrypted offline cache (D-022). When configured,
    /// `start_from`/`prefetch` serve a pinned, valid track from it instead
    /// of resolving a live stream, and `Command::Pin`/`Unpin`/`ListPins`
    /// become live instead of failing with "offline cache is not
    /// available".
    pub offline: Option<Arc<OfflineCache>>,
}

impl PlayerDeps {
    /// The production wiring every front end shares (`streamboatd`, the
    /// `streamboat` CLI and the desktop shell): the play reporter from
    /// `Settings::play_reporting` (D-027), the scrobble backends whose
    /// credentials are complete (D-037), and the streaming-privileges client
    /// (D-033), and the pinned offline cache (D-022; unavailable — say, no
    /// keyring reachable with `key_storage = keyring` — means stream-only,
    /// logged and never fatal) — all persisted under the data dir. Must be
    /// called inside a tokio runtime: the privileges client spawns its
    /// socket task and the cache opens its loopback server.
    pub async fn for_context(ctx: &Context) -> streamboat_core::Result<Self> {
        let reporter = Arc::new(PlayReporter::open(
            ctx.api.clone(),
            ctx.dirs.data.join("play_reports.json"),
            ctx.settings.play_reporting,
        )?);

        let mut backends: Vec<Arc<dyn Scrobbler>> = Vec::new();
        if let Some(lastfm) = LastfmScrobbler::open(
            &ctx.settings.scrobble.lastfm,
            ctx.dirs.data.join("scrobble_lastfm.json"),
            ctx.api.user_agent(),
        )? {
            backends.push(Arc::new(lastfm));
        }
        if let Some(listenbrainz) = ListenBrainzScrobbler::open(
            &ctx.settings.scrobble.listenbrainz,
            ctx.dirs.data.join("scrobble_listenbrainz.json"),
            ctx.api.user_agent(),
        )? {
            backends.push(Arc::new(listenbrainz));
        }
        let scrobbler: Option<Arc<dyn Scrobbler>> = if backends.is_empty() {
            None
        } else {
            Some(Arc::new(ScrobbleHub::new(backends)))
        };

        let offline =
            match OfflineCache::open(&ctx.dirs, &ctx.settings, &ctx.device.client_unique_key).await
            {
                Ok(cache) => Some(cache),
                Err(e) => {
                    tracing::warn!(%e, "offline cache unavailable; streaming only");
                    None
                }
            };

        let (privileges, privileges_events) =
            StreamingPrivileges::spawn(ctx.api.clone(), hostname_display_name());
        Ok(Self {
            privileges: Some(Arc::new(privileges)),
            privileges_events: Some(privileges_events),
            reporter: Some(reporter),
            scrobbler,
            offline,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PlayerConfig {
    pub quality_ceiling: AudioQuality,
    pub output: OutputConfig,
    pub volume: f32,
    /// ReplayGain mode (D-019): off / album / track. `Command::SetReplayGainMode`
    /// changes this live; see [`select_replay_gain`] for exactly how each
    /// mode picks between TIDAL's album and track numbers.
    pub replay_gain_mode: ReplayGainMode,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            quality_ceiling: AudioQuality::HiResLossless,
            output: OutputConfig::default(),
            volume: 1.0,
            replay_gain_mode: ReplayGainMode::default(),
        }
    }
}

/// Picks the ReplayGain (dB) and peak (linear) values [`load_item`] feeds
/// into a [`LoadItem`], per `mode` (D-019, `tidal-api/references/playback.md`
/// §8): `Off` applies neither (no gain stage is a valid, requested state);
/// `Album` prefers TIDAL's album-context numbers, falling back to the track
/// numbers only when TIDAL did not report an album value at all; `Track`
/// always uses the track numbers, with no album fallback. The gain *formula*
/// (`min(10^((rg+4)/20), 1/peak)`) is unchanged and stays in the engines
/// (`gst.rs`'s `volume` filter, `mpv.rs`'s `volume` property) — this
/// function only chooses which pair of numbers reaches it.
pub(crate) fn select_replay_gain(
    mode: ReplayGainMode,
    info: &StreamInfo,
) -> (Option<f64>, Option<f64>) {
    match mode {
        ReplayGainMode::Off => (None, None),
        ReplayGainMode::Album => {
            if info.album_replay_gain_db.is_some() {
                (info.album_replay_gain_db, info.album_peak_amplitude)
            } else {
                (info.replay_gain_db, info.peak_amplitude)
            }
        }
        ReplayGainMode::Track => (info.replay_gain_db, info.peak_amplitude),
    }
}

/// Cheap, cloneable way to talk to a running player.
#[derive(Clone)]
pub struct PlayerHandle {
    cmd_tx: mpsc::UnboundedSender<Command>,
    events: broadcast::Sender<Event>,
}

impl PlayerHandle {
    /// `false` if the player has shut down.
    pub fn send(&self, cmd: Command) -> bool {
        self.cmd_tx.send(cmd).is_ok()
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }
    /// Publish an event the player itself did not originate (headless login's
    /// `AuthRequired`/`AuthOk`, for instance) on the same broadcast every
    /// subscriber already listens on, so every front end — the control API,
    /// the stdio protocol, MPRIS — sees one unified event stream.
    pub fn publish(&self, ev: Event) {
        let _ = self.events.send(ev);
    }
}

struct Entry {
    item_id: u64,
    track: Track,
    summary: TrackSummary,
    resolved: Option<ResolvedStream>,
    /// One client-generated streaming session id per playback of this entry.
    session_id: String,
}

pub struct Player {
    api: ApiClient,
    engine: Box<dyn Engine>,
    engine_rx: mpsc::UnboundedReceiver<EngineEvent>,
    cmd_rx: mpsc::UnboundedReceiver<Command>,
    events: broadcast::Sender<Event>,
    queue: Vec<Entry>,
    index: Option<usize>,
    status: PlaybackStatus,
    volume: f32,
    ceiling: AudioQuality,
    output: OutputConfig,
    replay_gain_mode: ReplayGainMode,
    stream: Option<StreamInfo>,
    position_ms: u64,
    duration_ms: Option<u64>,
    next_item_id: u64,
    prefetched_for: Option<u64>,
    buffering_paused: bool,
    privileges: Option<Arc<StreamingPrivileges>>,
    privileges_events: Option<mpsc::UnboundedReceiver<PrivilegesEvent>>,
    reporter: Option<Arc<PlayReporter>>,
    scrobbler: Option<Arc<dyn Scrobbler>>,
    offline: Option<Arc<OfflineCache>>,
    /// Server-anchored start time of the currently loaded entry, taken as
    /// soon as it starts (`EngineEvent::Started`) and consumed the moment
    /// it stops being current — by a natural `EngineEvent::Finished` or by
    /// a command that skips it — so a play is reported/scrobbled exactly
    /// once. `None` whenever there is no [`PlayReporter`] configured.
    track_report_start_ms: Option<u64>,
}

impl Player {
    /// Start the player on the current tokio runtime. `engine_rx` is the
    /// channel the engine was constructed with; `deps` are the optional
    /// streaming-privileges/reporting/scrobbling dependencies (all `None`
    /// keeps today's behaviour exactly, which is what every existing test
    /// relies on).
    pub fn spawn(
        api: ApiClient,
        engine: Box<dyn Engine>,
        engine_rx: std_mpsc::Receiver<EngineEvent>,
        cfg: PlayerConfig,
        deps: PlayerDeps,
    ) -> PlayerHandle {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (events, _) = broadcast::channel(512);
        let (etx, erx) = mpsc::unbounded_channel();
        std::thread::Builder::new()
            .name("streamboat-engine-events".into())
            .spawn(move || {
                while let Ok(e) = engine_rx.recv() {
                    if etx.send(e).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn engine event bridge");
        let player = Player {
            api,
            engine,
            engine_rx: erx,
            cmd_rx,
            events: events.clone(),
            queue: Vec::new(),
            index: None,
            status: PlaybackStatus::Stopped,
            volume: cfg.volume,
            ceiling: cfg.quality_ceiling,
            output: cfg.output,
            replay_gain_mode: cfg.replay_gain_mode,
            stream: None,
            position_ms: 0,
            duration_ms: None,
            next_item_id: 1,
            prefetched_for: None,
            buffering_paused: false,
            privileges: deps.privileges,
            privileges_events: deps.privileges_events,
            reporter: deps.reporter,
            scrobbler: deps.scrobbler,
            offline: deps.offline,
            track_report_start_ms: None,
        };
        tokio::spawn(player.run());
        PlayerHandle { cmd_tx, events }
    }

    /// Send `USER_ACTION` on the privileges socket, if one is configured.
    /// Call only from a handler for a command that is genuine user intent
    /// (D-033) — never from an internal transition like gapless hand-over,
    /// resume-after-buffering, or an error-driven skip.
    fn claim_privileges(&self) {
        if let Some(p) = &self.privileges {
            p.claim();
        }
    }

    /// Reports a play and scrobbles the track that just stopped being
    /// current — either it finished naturally (`EngineEvent::Finished`,
    /// still `self.index`-current at that point) or a command is about to
    /// skip it. Must be called before `self.index`/`self.position_ms` are
    /// changed by the caller. A no-op whenever nothing was ever started
    /// (`track_report_start_ms` is `None`) or no `PlayReporter`/`Scrobbler`
    /// is configured.
    async fn note_play_ended(&mut self) {
        let Some(start_ms) = self.track_report_start_ms.take() else {
            return;
        };
        let Some(i) = self.index else { return };
        let Some(entry) = self.queue.get(i) else {
            return;
        };
        let Some(resolved) = entry.resolved.clone() else {
            return;
        };
        let played_ms = self.position_ms;
        let track_id = entry.track.id;
        let session_id = entry.session_id.clone();
        let summary = entry.summary.clone();
        if let Some(r) = self.reporter.clone() {
            let event = PlayEvent {
                track_id,
                streaming_session_id: session_id,
                asset_presentation: if resolved.info.preview {
                    "PREVIEW"
                } else {
                    "FULL"
                }
                .to_string(),
                audio_quality: resolved.info.quality,
                audio_mode: resolved.info.audio_mode,
                start_timestamp_ms: start_ms,
                end_timestamp_ms: start_ms.saturating_add(played_ms),
                start_position_s: 0.0,
                end_position_s: played_ms as f64 / 1000.0,
                source: None,
            };
            r.record(event).await;
        }
        if let Some(s) = self.scrobbler.clone() {
            // No dedicated scrobble threshold is documented for streamboat;
            // reusing TIDAL's own 30s play-reporting threshold (D-027) is a
            // deliberate, conservative stand-in rather than a made-up rule.
            if played_ms > REPORT_THRESHOLD_MS {
                let track = ScrobbleTrack {
                    artist: summary.artists.clone(),
                    title: summary.title.clone(),
                    album: Some(summary.album.clone()).filter(|a| !a.is_empty()),
                    duration_s: summary.duration_ms.map(|d| (d / 1000) as u32),
                    track_number: None,
                    mbid: None,
                };
                s.scrobble(&track, start_ms / 1000).await;
            }
        }
    }

    /// Resolves to the next privileges event, or never, when none is
    /// configured — lets `run`'s `select!` treat the channel as optional.
    async fn recv_privileges(
        rx: &mut Option<mpsc::UnboundedReceiver<PrivilegesEvent>>,
    ) -> Option<PrivilegesEvent> {
        match rx {
            Some(r) => r.recv().await,
            None => std::future::pending().await,
        }
    }

    async fn run(mut self) {
        if let Err(e) = self.engine.set_output(&self.output) {
            self.emit(Event::Error {
                message: format!("output: {e}"),
                track_id: None,
            });
        }
        if !self.output.is_exclusive() {
            let _ = self.engine.set_volume(self.volume);
        }
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                cmd = self.cmd_rx.recv() => match cmd {
                    None | Some(Command::Shutdown) => {
                        let _ = self.engine.stop();
                        self.emit(Event::Stopped);
                        break;
                    }
                    Some(cmd) => self.handle_command(cmd).await,
                },
                ev = self.engine_rx.recv() => match ev {
                    None => break,
                    Some(ev) => self.handle_engine_event(ev).await,
                },
                priv_ev = Self::recv_privileges(&mut self.privileges_events) => {
                    match priv_ev {
                        Some(ev) => self.handle_privileges_event(ev).await,
                        None => self.privileges_events = None,
                    }
                },
                _ = tick.tick() => self.tick(),
            }
        }
    }

    /// Reacts to the streaming-privileges socket (D-033): pause and
    /// announce a takeover, never re-claim automatically. Every other
    /// event is informational only — nothing here needs to react to
    /// `Connected`/`Reconnect`/`Disconnected` beyond what
    /// `StreamingPrivileges` itself already logs.
    async fn handle_privileges_event(&mut self, ev: PrivilegesEvent) {
        if let PrivilegesEvent::Revoked {
            client_display_name,
        } = ev
        {
            if matches!(
                self.status,
                PlaybackStatus::Playing | PlaybackStatus::Buffering
            ) {
                match self.engine.pause() {
                    Ok(()) => self.status = PlaybackStatus::Paused,
                    Err(e) => self.emit(Event::Warning {
                        message: format!("pausing after a privileges takeover: {e}"),
                    }),
                }
            }
            self.emit(Event::Warning {
                message: format!("playback paused: playback started on {client_display_name}"),
            });
            self.emit(Event::PlaybackTakenOver {
                by: client_display_name,
            });
            self.emit_state();
        }
    }

    fn emit(&self, ev: Event) {
        let _ = self.events.send(ev);
    }

    fn state(&self) -> PlayerState {
        let current = self
            .index
            .and_then(|i| self.queue.get(i))
            .map(|e| e.summary.clone());
        // The engines report *whether* ReplayGain was applied
        // (`replaygain_applied`) from the numbers they were actually handed;
        // only `Player` knows *which mode* chose those numbers (D-019), so
        // it is stamped on here rather than threaded through the `Engine`
        // trait.
        let mut signal_path = self.engine.signal_path();
        if let Some(sp) = signal_path.as_mut() {
            sp.replaygain_mode = self.replay_gain_mode.as_str().to_string();
        }
        PlayerState {
            status: self.status,
            current,
            current_index: self.index,
            queue_len: self.queue.len(),
            position_ms: self.position_ms,
            duration_ms: self.duration_ms,
            volume: self.volume,
            quality_ceiling: self.ceiling,
            output: self.output.clone(),
            stream: self.stream.clone(),
            signal_path,
        }
    }

    fn emit_state(&self) {
        self.emit(Event::State {
            state: self.state(),
        });
    }

    fn emit_queue(&self) {
        self.emit(Event::QueueChanged {
            queue: self.queue.iter().map(|e| e.summary.clone()).collect(),
            current_index: self.index,
        });
    }

    fn tick(&mut self) {
        if matches!(
            self.status,
            PlaybackStatus::Playing | PlaybackStatus::Paused | PlaybackStatus::Buffering
        ) {
            if let Some((pos, dur)) = self.engine.position() {
                self.position_ms = pos;
                if dur.is_some() {
                    self.duration_ms = dur;
                }
                self.emit(Event::Position {
                    position_ms: pos,
                    duration_ms: self.duration_ms,
                });
            }
        }
    }

    async fn fetch_entries(&mut self, track_ids: &[u64]) -> Vec<Entry> {
        let mut out = Vec::with_capacity(track_ids.len());
        for &id in track_ids {
            match self.api.track(id).await {
                Ok(track) => {
                    let summary = TrackSummary::from(&track);
                    let item_id = self.next_item_id;
                    self.next_item_id += 1;
                    out.push(Entry {
                        item_id,
                        track,
                        summary,
                        resolved: None,
                        session_id: uuid::Uuid::new_v4().to_string(),
                    });
                }
                Err(e) => self.emit(Event::Error {
                    message: format!("track {id}: {e}"),
                    track_id: Some(id),
                }),
            }
        }
        out
    }

    async fn handle_command(&mut self, cmd: Command) {
        match cmd {
            Command::Play { items } => {
                self.note_play_ended().await;
                let _ = self.engine.stop();
                self.prefetched_for = None;
                let ids: Vec<u64> = items.iter().map(|i| i.track_id).collect();
                self.queue = self.fetch_entries(&ids).await;
                self.index = None;
                self.emit_queue();
                if self.queue.is_empty() {
                    self.status = PlaybackStatus::Stopped;
                    self.emit(Event::EndOfQueue);
                    self.emit_state();
                } else {
                    // User-issued `Play`: genuine intent (D-033).
                    self.claim_privileges();
                    self.start_from(0).await;
                }
            }
            Command::Enqueue { items, position } => {
                let ids: Vec<u64> = items.iter().map(|i| i.track_id).collect();
                let entries = self.fetch_entries(&ids).await;
                let was_empty = self.queue.is_empty();
                let at = match (position, self.index) {
                    (QueuePosition::Next, Some(i)) => i + 1,
                    _ => self.queue.len(),
                };
                let at = at.min(self.queue.len());
                let n = entries.len();
                for (k, e) in entries.into_iter().enumerate() {
                    self.queue.insert(at + k, e);
                }
                self.emit_queue();
                if was_empty && n > 0 {
                    self.start_from(0).await;
                } else if let Some(i) = self.index {
                    // The successor may have changed.
                    self.prefetched_for = None;
                    self.engine.set_next(None);
                    self.prefetch(i).await;
                }
            }
            Command::Pause => {
                if self.status == PlaybackStatus::Playing {
                    match self.engine.pause() {
                        Ok(()) => self.status = PlaybackStatus::Paused,
                        Err(e) => self.emit(Event::Warning {
                            message: e.to_string(),
                        }),
                    }
                }
                self.emit_state();
            }
            Command::Resume => {
                if self.status == PlaybackStatus::Paused {
                    match self.engine.play() {
                        // A user pressing resume: genuine intent (D-033) —
                        // never called from the buffering-pause path below.
                        Ok(()) => {
                            self.status = PlaybackStatus::Playing;
                            self.claim_privileges();
                        }
                        Err(e) => self.emit(Event::Warning {
                            message: e.to_string(),
                        }),
                    }
                } else if self.status == PlaybackStatus::Stopped && !self.queue.is_empty() {
                    let i = self.index.unwrap_or(0);
                    self.claim_privileges();
                    self.start_from(i).await;
                    return;
                }
                self.emit_state();
            }
            Command::TogglePlayPause => {
                let next = match self.status {
                    PlaybackStatus::Playing => Command::Pause,
                    _ => Command::Resume,
                };
                Box::pin(self.handle_command(next)).await;
            }
            Command::Stop => {
                self.note_play_ended().await;
                let _ = self.engine.stop();
                self.prefetched_for = None;
                self.status = PlaybackStatus::Stopped;
                self.position_ms = 0;
                self.emit(Event::Stopped);
                self.emit_state();
            }
            Command::Next => {
                self.note_play_ended().await;
                let next = self.index.map(|i| i + 1).unwrap_or(0);
                if next < self.queue.len() {
                    // User-issued `Next`: genuine intent (D-033).
                    self.claim_privileges();
                    self.start_from(next).await;
                } else {
                    let _ = self.engine.stop();
                    self.status = PlaybackStatus::Stopped;
                    self.emit(Event::EndOfQueue);
                    self.emit_state();
                }
            }
            Command::Previous => match self.index {
                Some(i) if self.position_ms > 3000 || i == 0 => {
                    if let Err(e) = self.engine.seek(0) {
                        self.emit(Event::Warning {
                            message: e.to_string(),
                        });
                    }
                }
                Some(i) => {
                    self.note_play_ended().await;
                    // User-issued `Previous`: genuine intent (D-033).
                    self.claim_privileges();
                    self.start_from(i - 1).await;
                }
                None => {}
            },
            Command::Seek { position_ms } => {
                if let Err(e) = self.engine.seek(position_ms) {
                    self.emit(Event::Warning {
                        message: format!("seek: {e}"),
                    });
                } else {
                    self.position_ms = position_ms;
                    self.emit(Event::Position {
                        position_ms,
                        duration_ms: self.duration_ms,
                    });
                }
            }
            Command::SetVolume { volume } => {
                let v = volume.clamp(0.0, 1.0);
                if self.output.is_exclusive() {
                    self.emit(Event::Warning {
                        message: "volume is fixed at 100% while an exclusive output is active"
                            .into(),
                    });
                } else {
                    match self.engine.set_volume(v) {
                        Ok(()) => self.volume = v,
                        Err(e) => self.emit(Event::Warning {
                            message: e.to_string(),
                        }),
                    }
                }
                self.emit_state();
            }
            Command::SetQualityCeiling { ceiling } => {
                self.ceiling = ceiling;
                // Any prefetched successor was resolved at the old ceiling.
                self.prefetched_for = None;
                self.engine.set_next(None);
                if let Some(i) = self.index {
                    if let Some(e) = self.queue.get_mut(i + 1) {
                        e.resolved = None;
                    }
                    self.prefetch(i).await;
                }
                self.emit_state();
            }
            Command::SetOutput { output } => {
                if output == self.output {
                    self.emit_state();
                    return;
                }
                self.output = output.clone();
                let was_playing = matches!(
                    self.status,
                    PlaybackStatus::Playing | PlaybackStatus::Buffering
                );
                let resume_at = self.position_ms;
                match self.engine.set_output(&output) {
                    Ok(()) => {
                        if !output.is_exclusive() {
                            let _ = self.engine.set_volume(self.volume);
                        }
                        if was_playing {
                            if let Some(i) = self.index {
                                self.start_from(i).await;
                                if resume_at > 0 {
                                    let _ = self.engine.seek(resume_at);
                                }
                                return;
                            }
                        }
                    }
                    Err(e) => self.emit(Event::Error {
                        message: format!("output: {e}"),
                        track_id: None,
                    }),
                }
                self.emit_state();
            }
            Command::ClearQueue => {
                self.note_play_ended().await;
                let _ = self.engine.stop();
                self.queue.clear();
                self.index = None;
                self.prefetched_for = None;
                self.status = PlaybackStatus::Stopped;
                self.emit_queue();
                self.emit_state();
            }
            Command::MoveQueueItem { from, to } => {
                if from < self.queue.len() && to < self.queue.len() && from != to {
                    let playing_id = self
                        .index
                        .and_then(|i| self.queue.get(i))
                        .map(|e| e.item_id);
                    let entry = self.queue.remove(from);
                    self.queue.insert(to, entry);
                    self.index = playing_id.and_then(|id| self.index_of_item(id));
                    self.prefetched_for = None;
                    self.engine.set_next(None);
                    if let Some(i) = self.index {
                        self.prefetch(i).await;
                    }
                    self.emit_queue();
                }
                self.emit_state();
            }
            Command::RemoveQueueItem { index } => {
                if index < self.queue.len() {
                    let playing_id = self
                        .index
                        .and_then(|i| self.queue.get(i))
                        .map(|e| e.item_id);
                    if playing_id == Some(self.queue[index].item_id) {
                        self.emit(Event::Warning {
                            message:
                                "cannot remove the track that is currently playing; skip to it first"
                                    .into(),
                        });
                    } else {
                        let _ = self.queue.remove(index);
                        self.index = playing_id.and_then(|id| self.index_of_item(id));
                        self.prefetched_for = None;
                        self.engine.set_next(None);
                        if let Some(i) = self.index {
                            self.prefetch(i).await;
                        }
                        self.emit_queue();
                    }
                }
                self.emit_state();
            }
            Command::GetState => self.emit_state(),
            Command::Shutdown => {}
            Command::Pin { kind, id } => {
                let Some(cache) = self.offline.clone() else {
                    self.emit(Event::PinFailed {
                        kind,
                        id,
                        message: "the offline cache is not available".into(),
                    });
                    return;
                };
                let api = self.api.clone();
                let ceiling = self.ceiling;
                let events = self.events.clone();
                let id_for_task = id.clone();
                tokio::spawn(async move {
                    let progress_events = events.clone();
                    let progress_id = id_for_task.clone();
                    let result = cache
                        .pin(
                            &api,
                            kind,
                            id_for_task.clone(),
                            ceiling,
                            |completed, total| {
                                let _ = progress_events.send(Event::PinProgress {
                                    kind,
                                    id: progress_id.clone(),
                                    completed,
                                    total,
                                });
                            },
                        )
                        .await;
                    match result {
                        Ok(()) => {
                            let _ = events.send(Event::PinReady {
                                kind,
                                id: id_for_task,
                            });
                            let _ = events.send(Event::PinsChanged);
                        }
                        Err(e) => {
                            let _ = events.send(Event::PinFailed {
                                kind,
                                id: id_for_task,
                                message: e.to_string(),
                            });
                        }
                    }
                });
            }
            Command::Unpin { kind, id } => {
                let Some(cache) = self.offline.clone() else {
                    self.emit(Event::PinFailed {
                        kind,
                        id,
                        message: "the offline cache is not available".into(),
                    });
                    return;
                };
                match cache.unpin(kind, &id) {
                    Ok(_) => self.emit(Event::PinsChanged),
                    Err(e) => self.emit(Event::PinFailed {
                        kind,
                        id,
                        message: e.to_string(),
                    }),
                }
            }
            Command::ListPins => {
                // Front ends in this process (the CLI) query the offline
                // cache directly; a remote control surface re-fetches its
                // list on this signal (proto.rs's doc comment on the
                // variant).
                self.emit(Event::PinsChanged);
            }
            Command::SetReplayGainMode { mode } => {
                self.replay_gain_mode = mode;
                // The currently-playing entry keeps whatever gain its own
                // `load()` already applied (the engines' documented
                // once-per-load boundary, D-019) — but a prefetched
                // successor is still just sitting in `engine.set_next`, so
                // recompute and re-hand it over with the new mode, from the
                // `ResolvedStream` already cached on the queue entry (no
                // extra network round trip).
                if let Some(i) = self.index {
                    self.prefetched_for = None;
                    self.engine.set_next(None);
                    self.prefetch(i).await;
                }
                self.emit_state();
            }
            Command::Logout => {
                self.note_play_ended().await;
                let _ = self.engine.stop();
                self.queue.clear();
                self.index = None;
                self.prefetched_for = None;
                self.status = PlaybackStatus::Stopped;
                self.position_ms = 0;
                self.duration_ms = None;
                self.stream = None;
                if let Err(e) = self.api.logout().await {
                    self.emit(Event::Warning {
                        message: format!("logout: could not revoke the stored session: {e}"),
                    });
                }
                // D-022: the offline cache is a subscriber feature, never a
                // downloader — nothing pinned should outlive this session.
                if let Some(cache) = self.offline.clone() {
                    match cache.wipe_all() {
                        Ok(()) => self.emit(Event::PinsChanged),
                        Err(e) => tracing::warn!(%e, "logout: failed to wipe the offline cache"),
                    }
                }
                self.emit(Event::Stopped);
                self.emit(Event::Warning {
                    message: "logged out".into(),
                });
                self.emit_state();
            }
        }
    }

    /// Resolve and start entry `from`; on failure emit and try the next one
    /// (skip-with-event policy) until something plays or the queue ends.
    async fn start_from(&mut self, from: usize) {
        self.prefetched_for = None;
        self.engine.set_next(None);
        let mut i = from;
        while i < self.queue.len() {
            let (item_id, track_id, session_id, cached) = {
                let e = &self.queue[i];
                (
                    e.item_id,
                    e.track.id,
                    e.session_id.clone(),
                    e.resolved.clone(),
                )
            };
            let resolved = match cached {
                Some(r) => r,
                None => match self.resolve_track(track_id, &session_id).await {
                    Ok(r) => r,
                    Err(e) => {
                        self.emit(Event::Error {
                            message: format!("{}: {e}", self.queue[i].summary.title),
                            track_id: Some(track_id),
                        });
                        i += 1;
                        continue;
                    }
                },
            };
            for w in &resolved.warnings {
                self.emit(Event::Warning {
                    message: format!("{}: {w}", self.queue[i].summary.title),
                });
            }
            let item = load_item(item_id, &resolved, self.replay_gain_mode);
            self.queue[i].resolved = Some(resolved.clone());
            self.index = Some(i);
            self.status = PlaybackStatus::Buffering;
            self.position_ms = 0;
            self.duration_ms = self.queue[i].summary.duration_ms;
            self.stream = Some(resolved.info.clone());
            self.emit_state();
            match self.engine.load(item) {
                Ok(()) => return,
                Err(e) => {
                    self.emit(Event::Error {
                        message: format!("{}: {e}", self.queue[i].summary.title),
                        track_id: Some(track_id),
                    });
                    i += 1;
                }
            }
        }
        self.status = PlaybackStatus::Stopped;
        self.stream = None;
        self.emit(Event::EndOfQueue);
        self.emit_state();
    }

    /// Resolve the successor of entry `i` and hand it to the engine.
    async fn prefetch(&mut self, i: usize) {
        let Some(next) = self.queue.get(i + 1) else {
            return;
        };
        if self.prefetched_for == Some(next.item_id) {
            return;
        }
        let (item_id, track_id, session_id, cached, title) = (
            next.item_id,
            next.track.id,
            next.session_id.clone(),
            next.resolved.clone(),
            next.summary.title.clone(),
        );
        let resolved = match cached {
            Some(r) => r,
            None => match self.resolve_track(track_id, &session_id).await {
                Ok(r) => r,
                Err(e) => {
                    // Not fatal: start_from will retry at end of stream.
                    self.emit(Event::Warning {
                        message: format!("prefetch of {title} failed: {e}"),
                    });
                    return;
                }
            },
        };
        let item = load_item(item_id, &resolved, self.replay_gain_mode);
        if let Some(e) = self.queue.get_mut(i + 1) {
            e.resolved = Some(resolved);
        }
        self.engine.set_next(Some(item));
        self.prefetched_for = Some(item_id);
    }

    fn index_of_item(&self, item_id: u64) -> Option<usize> {
        self.queue.iter().position(|e| e.item_id == item_id)
    }

    /// Resolves a stream for `track_id`: a pinned, valid offline copy when
    /// one is configured and served (D-022 — never a written playable
    /// file, see `OfflineCache::serve_track`), else the ordinary live
    /// cascade. A terminal subscription sub-status from the live path
    /// wipes the offline cache (D-022's "wiped on ... a terminal
    /// subscription substatus"), since it means the account can no longer
    /// stream at all, offline pins included.
    async fn resolve_track(
        &mut self,
        track_id: u64,
        session_id: &str,
    ) -> Result<ResolvedStream, CoreError> {
        if let Some(cache) = self.offline.clone() {
            if let Some(served) = cache.serve_track(&self.api, track_id).await {
                return Ok(served.into_resolved_stream(track_id));
            }
        }
        let result = self
            .api
            .resolve_stream(track_id, self.ceiling, session_id)
            .await;
        if let Err(e) = &result {
            if let Some(cache) = &self.offline {
                if crate::offline::is_subscription_terminal(e) {
                    tracing::warn!(
                        %e,
                        "subscription no longer serves this account; wiping the offline cache"
                    );
                    match cache.wipe_all() {
                        Ok(()) => self.emit(Event::PinsChanged),
                        Err(we) => tracing::warn!(%we, "failed to wipe the offline cache"),
                    }
                }
            }
        }
        result
    }

    async fn handle_engine_event(&mut self, ev: EngineEvent) {
        match ev {
            EngineEvent::Started { id } => {
                let Some(i) = self.index_of_item(id) else {
                    return;
                };
                self.index = Some(i);
                self.status = PlaybackStatus::Playing;
                self.buffering_paused = false;
                self.position_ms = 0;
                self.duration_ms = self.queue[i].summary.duration_ms;
                let stream = self.queue[i]
                    .resolved
                    .as_ref()
                    .map(|r| r.info.clone())
                    .unwrap_or_default();
                self.stream = Some(stream.clone());
                self.track_report_start_ms = match &self.reporter {
                    Some(r) => Some(r.now_ms().await),
                    None => None,
                };
                if let Some(s) = self.scrobbler.clone() {
                    let summary = self.queue[i].summary.clone();
                    let track = ScrobbleTrack {
                        artist: summary.artists,
                        title: summary.title,
                        album: Some(summary.album).filter(|a| !a.is_empty()),
                        duration_s: summary.duration_ms.map(|d| (d / 1000) as u32),
                        track_number: None,
                        mbid: None,
                    };
                    s.now_playing(&track).await;
                }
                self.emit(Event::TrackStarted {
                    track: self.queue[i].summary.clone(),
                    stream,
                    index: i,
                });
                self.emit_state();
                self.prefetch(i).await;
            }
            EngineEvent::AboutToFinish { .. } => {}
            EngineEvent::Finished { id } => {
                if let Some(i) = self.index_of_item(id) {
                    self.emit(Event::TrackFinished {
                        track_id: self.queue[i].track.id,
                    });
                }
                // Still `self.index`-current here: a gapless hand-over's
                // `Started` for the successor has not been processed yet.
                self.note_play_ended().await;
            }
            EngineEvent::EndOfStream => {
                let next = self.index.map(|i| i + 1).unwrap_or(0);
                if next < self.queue.len() {
                    // Gapless did not happen (no successor was handed over):
                    // start it the ordinary way.
                    self.start_from(next).await;
                } else {
                    self.status = PlaybackStatus::Stopped;
                    self.stream = None;
                    self.emit(Event::EndOfQueue);
                    self.emit_state();
                }
            }
            EngineEvent::Buffering { percent } => {
                self.emit(Event::Buffering { percent });
                if percent < 100 && self.status == PlaybackStatus::Playing && !self.buffering_paused
                {
                    let _ = self.engine.pause();
                    self.buffering_paused = true;
                    self.status = PlaybackStatus::Buffering;
                } else if percent >= 100 && self.buffering_paused {
                    let _ = self.engine.play();
                    self.buffering_paused = false;
                    self.status = PlaybackStatus::Playing;
                }
            }
            EngineEvent::Format { id, description } => {
                if let Some(i) = self.index_of_item(id) {
                    if let Some(src_rate) = self.queue[i]
                        .resolved
                        .as_ref()
                        .and_then(|r| r.info.sample_rate)
                    {
                        if self.output.is_exclusive()
                            && !description.contains(&format!(" {src_rate} Hz"))
                        {
                            self.emit(Event::Warning {
                                message: format!(
                                    "exclusive output is running at {description} while the source is {src_rate} Hz: not bit-perfect"
                                ),
                            });
                        }
                    }
                }
                self.emit_state();
            }
            EngineEvent::Warning { message } => self.emit(Event::Warning { message }),
            EngineEvent::Error { id, message } => {
                let track_id = id
                    .and_then(|id| self.index_of_item(id))
                    .map(|i| self.queue[i].track.id);
                self.emit(Event::Error { message, track_id });
                let next = self.index.map(|i| i + 1).unwrap_or(0);
                if next < self.queue.len() {
                    self.start_from(next).await;
                } else {
                    let _ = self.engine.stop();
                    self.status = PlaybackStatus::Stopped;
                    self.emit(Event::EndOfQueue);
                    self.emit_state();
                }
            }
        }
    }
}

fn load_item(item_id: u64, r: &ResolvedStream, replay_gain_mode: ReplayGainMode) -> LoadItem {
    let (replay_gain_db, peak_amplitude) = select_replay_gain(replay_gain_mode, &r.info);
    LoadItem {
        id: item_id,
        source: r.source.clone(),
        replay_gain_db,
        peak_amplitude,
        codec: r.info.codec.clone(),
        sample_rate: r.info.sample_rate,
        bit_depth: r.info.bit_depth,
    }
}
