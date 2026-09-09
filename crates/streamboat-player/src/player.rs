//! The player: owns the queue, resolves streams through the API, drives one
//! [`Engine`], and speaks the protocol (`streamboat_core::proto`) and nothing
//! else. Front ends never touch the engine directly (D-010).
//!
//! Policies encoded here: an unplayable track is skipped with an `Error`
//! event, not a stop; the successor is resolved and handed to the engine as
//! soon as the current track starts (gapless); volume is refused with a
//! warning while an exclusive output is active (D-017).

use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use streamboat_core::models::{Track, TrackSummary};
use streamboat_core::proto::{
    Command, Event, OutputConfig, PlaybackStatus, PlayerState, QueuePosition, StreamInfo,
};
use streamboat_core::{ApiClient, AudioQuality, ResolvedStream};
use tokio::sync::{broadcast, mpsc};

use crate::engine::{Engine, EngineEvent, LoadItem};

#[derive(Debug, Clone)]
pub struct PlayerConfig {
    pub quality_ceiling: AudioQuality,
    pub output: OutputConfig,
    pub volume: f32,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            quality_ceiling: AudioQuality::HiResLossless,
            output: OutputConfig::default(),
            volume: 1.0,
        }
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
    stream: Option<StreamInfo>,
    position_ms: u64,
    duration_ms: Option<u64>,
    next_item_id: u64,
    prefetched_for: Option<u64>,
    buffering_paused: bool,
}

impl Player {
    /// Start the player on the current tokio runtime. `engine_rx` is the
    /// channel the engine was constructed with.
    pub fn spawn(
        api: ApiClient,
        engine: Box<dyn Engine>,
        engine_rx: std_mpsc::Receiver<EngineEvent>,
        cfg: PlayerConfig,
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
            stream: None,
            position_ms: 0,
            duration_ms: None,
            next_item_id: 1,
            prefetched_for: None,
            buffering_paused: false,
        };
        tokio::spawn(player.run());
        PlayerHandle { cmd_tx, events }
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
                _ = tick.tick() => self.tick(),
            }
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
            signal_path: self.engine.signal_path(),
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
                        Ok(()) => self.status = PlaybackStatus::Playing,
                        Err(e) => self.emit(Event::Warning {
                            message: e.to_string(),
                        }),
                    }
                } else if self.status == PlaybackStatus::Stopped && !self.queue.is_empty() {
                    let i = self.index.unwrap_or(0);
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
                let _ = self.engine.stop();
                self.prefetched_for = None;
                self.status = PlaybackStatus::Stopped;
                self.position_ms = 0;
                self.emit(Event::Stopped);
                self.emit_state();
            }
            Command::Next => {
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
            Command::Previous => match self.index {
                Some(i) if self.position_ms > 3000 || i == 0 => {
                    if let Err(e) = self.engine.seek(0) {
                        self.emit(Event::Warning {
                            message: e.to_string(),
                        });
                    }
                }
                Some(i) => self.start_from(i - 1).await,
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
                None => match self
                    .api
                    .resolve_stream(track_id, self.ceiling, &session_id)
                    .await
                {
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
            let item = load_item(item_id, &resolved);
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
            None => match self
                .api
                .resolve_stream(track_id, self.ceiling, &session_id)
                .await
            {
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
        let item = load_item(item_id, &resolved);
        if let Some(e) = self.queue.get_mut(i + 1) {
            e.resolved = Some(resolved);
        }
        self.engine.set_next(Some(item));
        self.prefetched_for = Some(item_id);
    }

    fn index_of_item(&self, item_id: u64) -> Option<usize> {
        self.queue.iter().position(|e| e.item_id == item_id)
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

fn load_item(item_id: u64, r: &ResolvedStream) -> LoadItem {
    LoadItem {
        id: item_id,
        source: r.source.clone(),
        replay_gain_db: r.info.replay_gain_db,
        peak_amplitude: r.info.peak_amplitude,
        codec: r.info.codec.clone(),
        sample_rate: r.info.sample_rate,
        bit_depth: r.info.bit_depth,
    }
}
