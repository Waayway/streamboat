//! The top-level `App`: startup (task item 2), the split top-level
//! `Message` enum wrapping each screen's own (D-013), the navigation stack,
//! the persistent sidebar and bottom playback bar, the keyboard shortcuts
//! (task item 4), and — this wave — the multi-window daemon runtime
//! (D-036), the mini-player window, the tray (D-014), and single-instance/
//! remote-client startup (D-010, D-030, D-045).
//!
//! ## Multi-window (D-036)
//!
//! `ui::app::run` builds the shell with `iced::daemon(...)` instead of
//! `iced::application(...)`: a `Daemon` opens no window on its own and
//! never exits when its last window closes (see that function's own doc
//! comment in `iced-0.14.0/src/daemon.rs`, verified against the pinned
//! source per the task brief) — exactly the two properties D-014's "closing
//! the window keeps playing, quitting is explicit" needs, so this crate no
//! longer has to fight the single-window shell's default exit-on-close
//! behaviour. `boot` opens the main window itself via `window::open`, which
//! returns the new `window::Id` synchronously (the `Task` it also returns
//! is only for the *effect* of actually opening it) — that `Id` is stored
//! in `App` before the window exists on screen, so `view`/`title` can
//! dispatch on it immediately. The mini-player window is opened/closed the
//! same way, on demand.
//!
//! ## Window lifecycle (D-014)
//!
//! The main window is created with `exit_on_close_request: false`
//! (`window::Settings`), so pressing its native close button does *not*
//! close it — it only delivers a `window::Event::CloseRequested` (verified
//! against `iced_winit-0.14.0/src/conversion.rs`: that conversion happens
//! unconditionally, before the shell's own "close and maybe exit" special
//! case even looks at `exit_on_close_request`) through
//! `window::close_requests()`'s subscription, which `update` answers by
//! hiding the window (`window::set_mode(id, window::Mode::Hidden)`) instead
//! of closing it — the lock and the engine (and the audio device, once
//! playback starts) stay held exactly as before. Quitting is explicit: the
//! tray's "Quit" item or Ctrl+Q sends `Command::Shutdown` and waits
//! (briefly, with a timeout) for `Event::Stopped` before returning
//! `iced::exit()`, per the task brief.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use iced::widget::{column, container, row, stack};
use iced::{Element, Length, Subscription, Task, Theme, window};

use streamboat_core::api::images::{ContentLink, parse_content_link};
use streamboat_core::bootstrap::Context;
use streamboat_core::config::ThemePreference;
use streamboat_core::instance_lock::InstanceLock;
use streamboat_core::models::TrackSummary;
use streamboat_core::proto::{Command, Event, PlayItem, PlayerState, QueuePosition};
use streamboat_player::{DecoderSupport, Player, PlayerConfig, PlayerDeps};

use crate::ui::actions;
use crate::ui::banner;
use crate::ui::design::Tokens;
use crate::ui::engine_select;
use crate::ui::format::cover_url;
use crate::ui::images::ImageCache;
use crate::ui::instance;
use crate::ui::mini_player;
use crate::ui::nav::{EntityRef, Nav, Screen};
use crate::ui::player_link::{InProcessLink, LinkKey, SharedLink};
use crate::ui::remote_link::RemoteLink;
use crate::ui::tray::{self, TrayEvent};
use crate::ui::{playback_bar, screens, signal_path};

const IMAGE_CACHE_CAPACITY: usize = 512;

// --- Entity/Collection/Lyrics wave: screen state and cross-screen effects ---
// Everything in this block through `impl From<...> for EntityEffect` is new
// this wave (task items 1-4): one enum holding whichever entity screen is
// currently open (constructed fresh per navigation by `App::sync_entity_screen`),
// and one small effect type every entity screen's own `Effect` converts into,
// so `update_album`/`update_artist`/... below share one `apply_entity_effect`
// instead of five near-identical copies of "send Play, navigate, prefetch
// artwork, open the add-to-playlist picker."

/// Whichever entity page (task item 1) is currently open, or `None` before
/// the first one loads. Rebuilt by [`App::sync_entity_screen`] whenever
/// navigation lands on a different [`Screen::Entity`]/id.
enum EntityScreen {
    None,
    Album(screens::album::State),
    Artist(screens::artist::State),
    Playlist(screens::playlist::State),
    Mix(screens::mix::State),
    Track(screens::track::State),
    Video(screens::video::State),
}

/// The effects every entity screen's own `Effect` type converts into, so
/// `App` has one place that knows how to turn "play these tracks" or "open
/// the add-to-playlist picker" into real `Command`s/state changes.
enum EntityEffect {
    PlayTracks(Vec<u64>),
    Enqueue(u64, QueuePosition),
    Navigate(EntityRef),
    ImagesNeeded(Vec<String>),
    OpenAddToPlaylist(u64),
    /// The user deleted their own playlist (`screens::playlist::Effect::Deleted`).
    PlaylistDeleted,
}

impl From<screens::album::Effect> for EntityEffect {
    fn from(e: screens::album::Effect) -> Self {
        match e {
            screens::album::Effect::PlayTracks(ids) => EntityEffect::PlayTracks(ids),
            screens::album::Effect::Enqueue(id, pos) => EntityEffect::Enqueue(id, pos),
            screens::album::Effect::Navigate(entity) => EntityEffect::Navigate(entity),
            screens::album::Effect::ImagesNeeded(ids) => EntityEffect::ImagesNeeded(ids),
            screens::album::Effect::OpenAddToPlaylist(id) => EntityEffect::OpenAddToPlaylist(id),
        }
    }
}

impl From<screens::artist::Effect> for EntityEffect {
    fn from(e: screens::artist::Effect) -> Self {
        match e {
            screens::artist::Effect::PlayTracks(ids) => EntityEffect::PlayTracks(ids),
            screens::artist::Effect::Enqueue(id, pos) => EntityEffect::Enqueue(id, pos),
            screens::artist::Effect::Navigate(entity) => EntityEffect::Navigate(entity),
            screens::artist::Effect::ImagesNeeded(ids) => EntityEffect::ImagesNeeded(ids),
            screens::artist::Effect::OpenAddToPlaylist(id) => EntityEffect::OpenAddToPlaylist(id),
        }
    }
}

impl From<screens::playlist::Effect> for EntityEffect {
    fn from(e: screens::playlist::Effect) -> Self {
        match e {
            screens::playlist::Effect::PlayTracks(ids) => EntityEffect::PlayTracks(ids),
            screens::playlist::Effect::Enqueue(id, pos) => EntityEffect::Enqueue(id, pos),
            screens::playlist::Effect::Navigate(entity) => EntityEffect::Navigate(entity),
            screens::playlist::Effect::ImagesNeeded(ids) => EntityEffect::ImagesNeeded(ids),
            screens::playlist::Effect::OpenAddToPlaylist(id) => EntityEffect::OpenAddToPlaylist(id),
            screens::playlist::Effect::Deleted => EntityEffect::PlaylistDeleted,
        }
    }
}

impl From<screens::mix::Effect> for EntityEffect {
    fn from(e: screens::mix::Effect) -> Self {
        match e {
            screens::mix::Effect::PlayTracks(ids) => EntityEffect::PlayTracks(ids),
            screens::mix::Effect::Enqueue(id, pos) => EntityEffect::Enqueue(id, pos),
            screens::mix::Effect::Navigate(entity) => EntityEffect::Navigate(entity),
            screens::mix::Effect::OpenAddToPlaylist(id) => EntityEffect::OpenAddToPlaylist(id),
        }
    }
}

impl From<screens::track::Effect> for EntityEffect {
    fn from(e: screens::track::Effect) -> Self {
        match e {
            screens::track::Effect::PlayTracks(ids) => EntityEffect::PlayTracks(ids),
            screens::track::Effect::Enqueue(id, pos) => EntityEffect::Enqueue(id, pos),
            screens::track::Effect::Navigate(entity) => EntityEffect::Navigate(entity),
            screens::track::Effect::ImagesNeeded(ids) => EntityEffect::ImagesNeeded(ids),
            screens::track::Effect::OpenAddToPlaylist(id) => EntityEffect::OpenAddToPlaylist(id),
        }
    }
}

impl From<screens::video::Effect> for EntityEffect {
    fn from(e: screens::video::Effect) -> Self {
        match e {
            screens::video::Effect::Navigate(entity) => EntityEffect::Navigate(entity),
            screens::video::Effect::ImagesNeeded(ids) => EntityEffect::ImagesNeeded(ids),
        }
    }
}

impl From<screens::collection::Effect> for EntityEffect {
    fn from(e: screens::collection::Effect) -> Self {
        match e {
            screens::collection::Effect::PlayTracks(ids) => EntityEffect::PlayTracks(ids),
            screens::collection::Effect::Enqueue(id, pos) => EntityEffect::Enqueue(id, pos),
            screens::collection::Effect::Navigate(entity) => EntityEffect::Navigate(entity),
            screens::collection::Effect::ImagesNeeded(ids) => EntityEffect::ImagesNeeded(ids),
            screens::collection::Effect::OpenAddToPlaylist(id) => {
                EntityEffect::OpenAddToPlaylist(id)
            }
        }
    }
}
// --- end Entity/Collection/Lyrics wave block ---

/// How long to wait for `Event::Stopped` after `Command::Shutdown` before
/// exiting anyway (task item 2: "wait for `Event::Stopped` briefly").
const SHUTDOWN_GRACE: Duration = Duration::from_millis(800);

pub struct App {
    ctx: Context,
    link: SharedLink,
    http: reqwest::Client,

    /// The main window's id, known synchronously from `window::open` in
    /// `boot` (task item 1) — before it necessarily exists on screen.
    main_window: window::Id,
    /// `Some` only while the mini-player window is open (task item 1);
    /// toggled by the playback bar and a keyboard shortcut.
    mini_window: Option<window::Id>,
    /// The `show-request` file to poll for a second GUI instance asking to
    /// be shown (`ui::instance`) — `None` when this process is a remote
    /// client rather than the lock holder, since nothing else would ever
    /// touch that file expecting *this* process to react to it.
    show_request_path: Option<PathBuf>,
    /// The startup decoder probe result (D-003). Stored so a future
    /// Settings-screen change can grey out an unreachable quality tier from
    /// it; this wave already uses it once, at startup, to cap the
    /// requested ceiling (see `ui::app::run_as_local_instance`).
    #[allow(dead_code)]
    decoder_support: DecoderSupport,

    logged_in: bool,
    nav: Nav,
    tokens: Tokens,

    images: ImageCache,
    queue: Vec<TrackSummary>,
    player_state: PlayerState,
    show_signal_path: bool,

    login: screens::login::State,
    home: screens::home::State,
    explore: screens::explore::State,
    search: screens::search::State,
    settings: screens::settings::State,

    /// Task item 1: whichever entity page is open, rebuilt on navigation by
    /// [`App::sync_entity_screen`].
    entity: EntityScreen,
    /// Task item 2: My Collection persists across visits like `home`/
    /// `explore`/`search` above, rather than being rebuilt per navigation
    /// like `entity` — it has no per-visit id to key a fresh load on.
    collection: screens::collection::State,
    /// Task item 4: the currently (pre)fetched track's lyrics.
    lyrics: screens::lyrics::State,
    /// Task item 6: dismissible, auto-expiring notification banners.
    banner: banner::State,
    /// Task item 3: the "add to playlist" picker, open over whichever
    /// screen triggered it.
    add_to_playlist: Option<actions::PickerState>,
    /// Task item 5: a deep link passed on the command line (`streamboat
    /// open <url>` or a bare URL as `argv[1]`), applied once the user is
    /// confirmed logged in.
    pending_open: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Nav(NavAction),
    Login(screens::login::Message),
    Home(screens::home::Message),
    Explore(screens::explore::Message),
    Search(screens::search::Message),
    NowPlaying(screens::now_playing::Message),
    Settings(screens::settings::Message),
    PlaybackBar(playback_bar::Message),
    MiniPlayer(mini_player::Message),
    Tray(TrayEvent),
    /// Boxed: `Event::State` carries a full `PlayerState` snapshot, which
    /// makes this by far the largest variant — boxing it keeps every other
    /// `Message` variant (and every `Task`/`Subscription` built from them)
    /// from paying that size on the stack.
    PlayerEvent(Box<Event>),
    LoginCheck(bool),
    ImageFetched(String, Option<iced::widget::image::Handle>),
    KeyShortcut(Shortcut),

    // --- Entity/Collection/Lyrics wave (task items 1-6) ---
    Album(screens::album::Message),
    Artist(screens::artist::Message),
    Playlist(screens::playlist::Message),
    Mix(screens::mix::Message),
    Track(screens::track::Message),
    Video(screens::video::Message),
    Collection(screens::collection::Message),
    Lyrics(screens::lyrics::Message),
    Banner(banner::Message),
    /// Task item 3: the "add to playlist" picker overlay, opened by any
    /// screen's `EntityEffect::OpenAddToPlaylist` (via `App::open_add_to_playlist`,
    /// called directly rather than through a `Message` — there is no UI
    /// element outside a screen's own track rows that opens this, so there
    /// is nothing that would ever construct a top-level `Message` for it).
    AddToPlaylist(actions::PickerMessage),
    /// A shared playlist link resolved to its track ids (task item 5,
    /// D-039 "shared playlist links open and play") — `Err` surfaces as a
    /// banner rather than a silent failure.
    DeepLinkPlaylistLoaded(Result<Vec<u64>, String>),
    /// A window's native close button was pressed (D-014).
    WindowCloseRequested(window::Id),
    /// A window actually closed — used only to notice the mini-player
    /// window disappearing by some path other than the toggle handler
    /// (e.g. a platform gesture this crate does not otherwise intercept).
    WindowClosed(window::Id),
    /// A second GUI instance asked to be shown (`ui::instance`).
    ShowRequested,
    /// `Command::Shutdown` was sent and either `Event::Stopped` arrived or
    /// the grace period elapsed — safe to call `iced::exit()` now.
    ReadyToExit,
}

/// A subset of [`Screen`] with no payload, for the sidebar's fixed entries
/// (entity/lyrics screens are only ever reached by clicking a card, which
/// carries its own id straight into `Screen::Entity`/`Screen::Lyrics`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavTarget {
    Home,
    Explore,
    Search,
    NowPlaying,
    Collection,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavAction {
    Go(NavTarget),
    Back,
    Forward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    TogglePlayPause,
    FocusSearch,
    Back,
    /// Ctrl+M (task item 1: "toggled from the playback bar and a keyboard
    /// shortcut").
    ToggleMiniPlayer,
    /// Ctrl+Q (D-014: "quitting is explicit").
    Quit,
}

impl App {
    fn boot(
        ctx: Context,
        link: SharedLink,
        open_url: Option<String>,
        decoder_support: DecoderSupport,
        show_request_path: Option<PathBuf>,
    ) -> (Self, Task<Message>) {
        let http = reqwest::Client::builder()
            .user_agent(format!(
                "streamboat/{} (+{})",
                streamboat_core::VERSION,
                streamboat_core::PROJECT_URL
            ))
            .build()
            .unwrap_or_default();
        let tokens = Tokens::for_preference(ctx.settings.theme);
        let key_location = ctx.store.key_location().to_string();
        let settings = screens::settings::State::from_settings(&ctx.settings, key_location)
            .with_decoder_support(decoder_support);
        let api = ctx.api.clone();
        let (main_window, open_main) = window::open(main_window_settings());
        let app = Self {
            ctx,
            link,
            http,
            main_window,
            mini_window: None,
            show_request_path,
            decoder_support,
            logged_in: false,
            nav: Nav::new(Screen::Login),
            tokens,
            images: ImageCache::new(IMAGE_CACHE_CAPACITY),
            queue: Vec::new(),
            player_state: PlayerState::default(),
            show_signal_path: false,
            login: screens::login::State::default(),
            home: screens::home::State::default(),
            explore: screens::explore::State::default(),
            search: screens::search::State::default(),
            settings,
            entity: EntityScreen::None,
            collection: screens::collection::State::default(),
            lyrics: screens::lyrics::State::default(),
            banner: banner::State::default(),
            add_to_playlist: None,
            pending_open: open_url,
        };
        let check = Task::perform(async move { api.is_logged_in().await }, Message::LoginCheck);
        (app, Task::batch([open_main.discard(), check]))
    }

    fn title(&self, window: window::Id) -> String {
        if Some(window) == self.mini_window {
            "streamboat mini".to_string()
        } else {
            format!("streamboat — {}", self.nav.current().title())
        }
    }

    fn theme(&self, _window: window::Id) -> Theme {
        self.tokens.iced_theme(match self.ctx.settings.theme {
            ThemePreference::Dark => "streamboat dark",
            ThemePreference::Light => "streamboat light",
        })
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            Subscription::run_with(LinkKey(self.link.clone()), player_events)
                .map(|event| Message::PlayerEvent(Box::new(event))),
            iced::event::listen_with(keyboard_shortcut).map(Message::KeyShortcut),
            window::close_requests().map(Message::WindowCloseRequested),
            window::close_events().map(Message::WindowClosed),
            Subscription::run_with(LinkKey(self.link.clone()), tray_events).map(Message::Tray),
        ];
        if let Some(path) = self.show_request_path.clone() {
            subs.push(
                Subscription::run_with(path, instance::show_request_events)
                    .map(|()| Message::ShowRequested),
            );
        }
        Subscription::batch(subs)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::LoginCheck(true) => {
                self.logged_in = true;
                self.nav = Nav::new(Screen::Home);
                Task::batch([self.load_home_and_explore(), self.apply_pending_open()])
            }
            Message::LoginCheck(false) => Task::none(),
            Message::Nav(action) => self.apply_nav(action),
            Message::Login(inner) => self.update_login(inner),
            Message::Home(inner) => self.update_home(inner),
            Message::Explore(inner) => self.update_explore(inner),
            Message::Search(inner) => self.update_search(inner),
            Message::NowPlaying(inner) => self.update_now_playing(inner),
            Message::Settings(inner) => self.update_settings(inner),
            Message::PlaybackBar(inner) => self.update_playback_bar(inner),
            Message::MiniPlayer(inner) => self.update_mini_player(inner),
            Message::Tray(event) => self.update_tray(event),
            Message::PlayerEvent(event) => self.handle_player_event(*event),
            Message::ImageFetched(url, handle) => {
                if let Some(handle) = handle {
                    self.images.insert(url, handle);
                }
                Task::none()
            }
            Message::KeyShortcut(shortcut) => self.handle_shortcut(shortcut),

            // --- Entity/Collection/Lyrics wave (task items 1-6) ---
            Message::Album(inner) => self.update_album(inner),
            Message::Artist(inner) => self.update_artist(inner),
            Message::Playlist(inner) => self.update_playlist(inner),
            Message::Mix(inner) => self.update_mix(inner),
            Message::Track(inner) => self.update_track(inner),
            Message::Video(inner) => self.update_video(inner),
            Message::Collection(inner) => self.update_collection(inner),
            Message::Lyrics(inner) => self.update_lyrics(inner),
            Message::Banner(inner) => self.update_banner(inner),
            Message::AddToPlaylist(inner) => self.update_add_to_playlist(inner),
            Message::DeepLinkPlaylistLoaded(result) => self.deep_link_playlist_loaded(result),
            Message::WindowCloseRequested(id) => self.handle_close_requested(id),
            Message::WindowClosed(id) => {
                if Some(id) == self.mini_window {
                    self.mini_window = None;
                }
                Task::none()
            }
            Message::ShowRequested => self.show_main_window(),
            Message::ReadyToExit => iced::exit(),
        }
    }

    fn load_home_and_explore(&mut self) -> Task<Message> {
        Task::batch([
            screens::home::State::load(&self.ctx.api).map(Message::Home),
            screens::explore::State::load(&self.ctx.api).map(Message::Explore),
        ])
    }

    fn apply_nav(&mut self, action: NavAction) -> Task<Message> {
        if !self.logged_in {
            return Task::none();
        }
        match action {
            NavAction::Go(target) => self.nav.go_to(target.into()),
            NavAction::Back => {
                let _ = self.nav.back();
            }
            NavAction::Forward => {
                let _ = self.nav.forward();
            }
        }
        self.sync_entity_screen()
    }

    /// Navigate to `screen` and make sure whatever state it needs (an
    /// entity page's data, My Collection's first tab, this track's lyrics)
    /// is loading — the one path every navigation in this module should go
    /// through instead of calling `self.nav.go_to` directly, so nothing new
    /// this wave adds is ever shown stale or empty.
    fn navigate(&mut self, screen: Screen) -> Task<Message> {
        self.nav.go_to(screen);
        self.sync_entity_screen()
    }

    /// Rebuilds [`EntityScreen`]/loads My Collection's first tab/(re)requests
    /// lyrics when navigation just landed on a screen that needs it — a
    /// no-op for every screen that already manages its own persistent state
    /// (Home, Explore, Search, Now Playing, Settings).
    fn sync_entity_screen(&mut self) -> Task<Message> {
        match self.nav.current().clone() {
            Screen::Entity(EntityRef::Album(id)) => {
                if !matches!(&self.entity, EntityScreen::Album(s) if s.album_id() == id) {
                    self.entity = EntityScreen::Album(screens::album::State::new(id));
                    return screens::album::State::load(&self.ctx.api, id).map(Message::Album);
                }
            }
            Screen::Entity(EntityRef::Artist(id)) => {
                if !matches!(&self.entity, EntityScreen::Artist(s) if s.artist_id() == id) {
                    self.entity = EntityScreen::Artist(screens::artist::State::new(id));
                    return screens::artist::State::load(&self.ctx.api, id).map(Message::Artist);
                }
            }
            Screen::Entity(EntityRef::Playlist(uuid)) => {
                if !matches!(&self.entity, EntityScreen::Playlist(s) if s.uuid() == uuid.as_str()) {
                    self.entity =
                        EntityScreen::Playlist(screens::playlist::State::new(uuid.clone()));
                    return screens::playlist::State::load(&self.ctx.api, uuid)
                        .map(Message::Playlist);
                }
            }
            Screen::Entity(EntityRef::Mix(id)) => {
                if !matches!(&self.entity, EntityScreen::Mix(s) if s.mix_id() == id.as_str()) {
                    self.entity = EntityScreen::Mix(screens::mix::State::new(id.clone()));
                    return screens::mix::State::load(&self.ctx.api, id).map(Message::Mix);
                }
            }
            Screen::Entity(EntityRef::Track(id)) => {
                if !matches!(&self.entity, EntityScreen::Track(s) if s.track_id() == id) {
                    self.entity = EntityScreen::Track(screens::track::State::new(id));
                    return screens::track::State::load(&self.ctx.api, id).map(Message::Track);
                }
            }
            Screen::Entity(EntityRef::Video(id)) => {
                if !matches!(&self.entity, EntityScreen::Video(s) if s.video_id() == id) {
                    self.entity = EntityScreen::Video(screens::video::State::new(id));
                    return screens::video::State::load(&self.ctx.api, id).map(Message::Video);
                }
            }
            Screen::Collection => {
                if !self.collection.is_loaded() {
                    return screens::collection::State::load(&self.ctx.api)
                        .map(Message::Collection);
                }
            }
            Screen::Lyrics(id) => {
                let (task, _effect) = self
                    .lyrics
                    .update(screens::lyrics::Message::Requested(id), &self.ctx.api);
                return task.map(Message::Lyrics);
            }
            Screen::Login
            | Screen::Home
            | Screen::Explore
            | Screen::Search
            | Screen::NowPlaying
            | Screen::Settings => {}
        }
        Task::none()
    }

    /// Applies the effects every entity screen's own `Effect` converts into
    /// via `From` — see the block above [`App`]'s struct definition.
    fn apply_entity_effect(&mut self, effect: EntityEffect) -> Task<Message> {
        match effect {
            EntityEffect::PlayTracks(ids) => {
                if ids.is_empty() {
                    return Task::none();
                }
                self.link.send(Command::Play {
                    items: ids
                        .into_iter()
                        .map(|track_id| PlayItem { track_id })
                        .collect(),
                });
                self.navigate(Screen::NowPlaying)
            }
            EntityEffect::Enqueue(id, position) => {
                self.link.send(Command::Enqueue {
                    items: vec![PlayItem { track_id: id }],
                    position,
                });
                Task::none()
            }
            EntityEffect::Navigate(entity) => self.navigate(Screen::Entity(entity)),
            EntityEffect::ImagesNeeded(ids) => self.prefetch_images(ids),
            EntityEffect::OpenAddToPlaylist(id) => self.open_add_to_playlist(id),
            EntityEffect::PlaylistDeleted => self.navigate(Screen::Collection),
        }
    }

    fn open_add_to_playlist(&mut self, track_id: u64) -> Task<Message> {
        let (state, task) = actions::PickerState::open(track_id, &self.ctx.api);
        self.add_to_playlist = Some(state);
        task.map(Message::AddToPlaylist)
    }

    fn update_add_to_playlist(&mut self, message: actions::PickerMessage) -> Task<Message> {
        let Some(state) = self.add_to_playlist.as_mut() else {
            return Task::none();
        };
        let (task, effect) = state.update(message, &self.ctx.api);
        if matches!(effect, Some(actions::PickerEffect::Close)) {
            self.add_to_playlist = None;
        }
        task.map(Message::AddToPlaylist)
    }

    fn update_album(&mut self, inner: screens::album::Message) -> Task<Message> {
        let EntityScreen::Album(state) = &mut self.entity else {
            return Task::none();
        };
        let (task, effects) = state.update(inner, &self.ctx.api);
        let mut tasks = vec![task.map(Message::Album)];
        for effect in effects {
            tasks.push(self.apply_entity_effect(effect.into()));
        }
        Task::batch(tasks)
    }

    fn update_artist(&mut self, inner: screens::artist::Message) -> Task<Message> {
        let EntityScreen::Artist(state) = &mut self.entity else {
            return Task::none();
        };
        let (task, effects) = state.update(inner, &self.ctx.api);
        let mut tasks = vec![task.map(Message::Artist)];
        for effect in effects {
            tasks.push(self.apply_entity_effect(effect.into()));
        }
        Task::batch(tasks)
    }

    fn update_playlist(&mut self, inner: screens::playlist::Message) -> Task<Message> {
        let EntityScreen::Playlist(state) = &mut self.entity else {
            return Task::none();
        };
        let (task, effects) = state.update(inner, &self.ctx.api);
        let mut tasks = vec![task.map(Message::Playlist)];
        for effect in effects {
            tasks.push(self.apply_entity_effect(effect.into()));
        }
        Task::batch(tasks)
    }

    fn update_mix(&mut self, inner: screens::mix::Message) -> Task<Message> {
        let EntityScreen::Mix(state) = &mut self.entity else {
            return Task::none();
        };
        let (task, effects) = state.update(inner, &self.ctx.api);
        let mut tasks = vec![task.map(Message::Mix)];
        for effect in effects {
            tasks.push(self.apply_entity_effect(effect.into()));
        }
        Task::batch(tasks)
    }

    fn update_track(&mut self, inner: screens::track::Message) -> Task<Message> {
        let EntityScreen::Track(state) = &mut self.entity else {
            return Task::none();
        };
        let (task, effects) = state.update(inner, &self.ctx.api);
        let mut tasks = vec![task.map(Message::Track)];
        for effect in effects {
            tasks.push(self.apply_entity_effect(effect.into()));
        }
        Task::batch(tasks)
    }

    fn update_video(&mut self, inner: screens::video::Message) -> Task<Message> {
        let EntityScreen::Video(state) = &mut self.entity else {
            return Task::none();
        };
        let (task, effects) = state.update(inner, &self.ctx.api);
        let mut tasks = vec![task.map(Message::Video)];
        for effect in effects {
            tasks.push(self.apply_entity_effect(effect.into()));
        }
        Task::batch(tasks)
    }

    fn update_collection(&mut self, inner: screens::collection::Message) -> Task<Message> {
        let (task, effects) = self.collection.update(inner, &self.ctx.api);
        let mut tasks = vec![task.map(Message::Collection)];
        for effect in effects {
            tasks.push(self.apply_entity_effect(effect.into()));
        }
        Task::batch(tasks)
    }

    fn update_lyrics(&mut self, inner: screens::lyrics::Message) -> Task<Message> {
        let (task, effect) = self.lyrics.update(inner, &self.ctx.api);
        if let Some(screens::lyrics::Effect::Seek(position_ms)) = effect {
            self.link.send(Command::Seek { position_ms });
        }
        task.map(Message::Lyrics)
    }

    fn update_banner(&mut self, inner: banner::Message) -> Task<Message> {
        if let Some(banner::Effect::Resume) = self.banner.update(inner) {
            self.link.send(Command::Resume);
        }
        Task::none()
    }

    /// Task item 5: apply a deep link once parsed — navigate immediately,
    /// and for a shared playlist link, also fetch its tracks and start
    /// playback (D-039).
    fn open_deep_link(&mut self, link: ContentLink) -> Task<Message> {
        let screen = Screen::from_content_link(link.clone());
        let mut tasks = vec![self.navigate(screen)];
        if let ContentLink::Playlist(uuid) = link {
            let api = self.ctx.api.clone();
            tasks.push(Task::perform(
                load_playlist_track_ids(api, uuid),
                Message::DeepLinkPlaylistLoaded,
            ));
        }
        Task::batch(tasks)
    }

    fn deep_link_playlist_loaded(&mut self, result: Result<Vec<u64>, String>) -> Task<Message> {
        match result {
            Ok(ids) if !ids.is_empty() => {
                self.link.send(Command::Play {
                    items: ids
                        .into_iter()
                        .map(|track_id| PlayItem { track_id })
                        .collect(),
                });
                Task::none()
            }
            Ok(_) => Task::none(),
            Err(e) => {
                let task = self.banner.push(
                    format!("couldn't load this shared playlist: {e}"),
                    banner::Tone::Danger,
                    false,
                );
                task.map(Message::Banner)
            }
        }
    }

    /// A CLI-provided deep link (`streamboat open <url>` or a bare URL as
    /// `argv[1]`), applied once — parsed and consumed from `pending_open`
    /// after the first successful login check, whether that's an existing
    /// session or a fresh login.
    fn apply_pending_open(&mut self) -> Task<Message> {
        let Some(url) = self.pending_open.take() else {
            return Task::none();
        };
        let Some(link) = parse_content_link(&url) else {
            let task = self.banner.push(
                format!("couldn't recognise this link: {url}"),
                banner::Tone::Warning,
                false,
            );
            return task.map(Message::Banner);
        };
        self.open_deep_link(link)
    }

    fn update_login(&mut self, inner: screens::login::Message) -> Task<Message> {
        let (task, effect) =
            self.login
                .update(inner, &self.ctx.api, &self.ctx.device.client_unique_key);
        if matches!(effect, Some(screens::login::Effect::LoggedIn)) {
            self.logged_in = true;
            self.nav = Nav::new(Screen::Home);
            return Task::batch([
                task.map(Message::Login),
                self.load_home_and_explore(),
                self.apply_pending_open(),
            ]);
        }
        task.map(Message::Login)
    }

    fn update_home(&mut self, inner: screens::home::Message) -> Task<Message> {
        let (task, effects) = self.home.update(inner, &self.ctx.api);
        let mut tasks = vec![task.map(Message::Home)];
        for effect in effects {
            match effect {
                screens::home::Effect::Navigate(entity) => {
                    tasks.push(self.navigate(Screen::Entity(entity)))
                }
                screens::home::Effect::ImagesNeeded(ids) => tasks.push(self.prefetch_images(ids)),
            }
        }
        Task::batch(tasks)
    }

    fn update_explore(&mut self, inner: screens::explore::Message) -> Task<Message> {
        let (task, effects) = self.explore.update(inner, &self.ctx.api);
        let mut tasks = vec![task.map(Message::Explore)];
        for effect in effects {
            match effect {
                screens::explore::Effect::Navigate(entity) => {
                    tasks.push(self.navigate(Screen::Entity(entity)))
                }
                screens::explore::Effect::ImagesNeeded(ids) => {
                    tasks.push(self.prefetch_images(ids))
                }
            }
        }
        Task::batch(tasks)
    }

    fn update_search(&mut self, inner: screens::search::Message) -> Task<Message> {
        let (task, effects) = self.search.update(inner, &self.ctx.api);
        let mut tasks = vec![task.map(Message::Search)];
        for effect in effects {
            match effect {
                screens::search::Effect::PlayTrack(id) => {
                    self.link.send(Command::Play {
                        items: vec![PlayItem { track_id: id }],
                    });
                    tasks.push(self.navigate(Screen::NowPlaying));
                }
                screens::search::Effect::Enqueue(id, position) => {
                    self.link.send(Command::Enqueue {
                        items: vec![PlayItem { track_id: id }],
                        position,
                    });
                }
                screens::search::Effect::Navigate(entity) => {
                    tasks.push(self.navigate(Screen::Entity(entity)))
                }
                screens::search::Effect::ImagesNeeded(ids) => tasks.push(self.prefetch_images(ids)),
                screens::search::Effect::OpenDeepLink(link) => {
                    tasks.push(self.open_deep_link(link))
                }
            }
        }
        Task::batch(tasks)
    }

    fn update_now_playing(&mut self, inner: screens::now_playing::Message) -> Task<Message> {
        match inner {
            screens::now_playing::Message::SeekChanged(ms) => {
                self.player_state.position_ms = u64::from(ms);
            }
            screens::now_playing::Message::SeekReleased => {
                self.link.send(Command::Seek {
                    position_ms: self.player_state.position_ms,
                });
            }
            screens::now_playing::Message::MoveUp(index) => {
                if index > 0 {
                    self.link.send(Command::MoveQueueItem {
                        from: index,
                        to: index - 1,
                    });
                }
            }
            screens::now_playing::Message::MoveDown(index) => {
                self.link.send(Command::MoveQueueItem {
                    from: index,
                    to: index + 1,
                });
            }
            screens::now_playing::Message::Remove(index) => {
                self.link.send(Command::RemoveQueueItem { index });
            }
            screens::now_playing::Message::OpenLyrics(track_id) => {
                return self.navigate(Screen::Lyrics(track_id));
            }
        }
        Task::none()
    }

    fn update_settings(&mut self, inner: screens::settings::Message) -> Task<Message> {
        let (task, effect) = self.settings.update(inner, &self.ctx.api);
        match effect {
            Some(screens::settings::Effect::Save) => {
                let output = self.settings.apply_to(&mut self.ctx.settings);
                if let Err(e) = self.ctx.settings.save(&self.ctx.dirs.settings_path()) {
                    tracing::warn!("could not save settings: {e}");
                }
                self.link.send(Command::SetOutput { output });
                self.link.send(Command::SetQualityCeiling {
                    ceiling: self.ctx.settings.quality_ceiling(),
                });
            }
            Some(screens::settings::Effect::ThemeChanged(pref)) => {
                self.ctx.settings.theme = pref;
                self.tokens = Tokens::for_preference(pref);
            }
            Some(screens::settings::Effect::LoggedOut) => {
                self.logged_in = false;
                self.nav = Nav::new(Screen::Login);
                self.login = screens::login::State::default();
                self.queue.clear();
                self.player_state = PlayerState::default();
            }
            None => {}
        }
        task.map(Message::Settings)
    }

    fn update_playback_bar(&mut self, inner: playback_bar::Message) -> Task<Message> {
        match inner {
            playback_bar::Message::PlayPause => {
                self.link.send(Command::TogglePlayPause);
            }
            playback_bar::Message::Previous => {
                self.link.send(Command::Previous);
            }
            playback_bar::Message::Next => {
                self.link.send(Command::Next);
            }
            playback_bar::Message::SeekChanged(ms) => {
                self.player_state.position_ms = u64::from(ms);
            }
            playback_bar::Message::SeekReleased => {
                self.link.send(Command::Seek {
                    position_ms: self.player_state.position_ms,
                });
            }
            playback_bar::Message::VolumeChanged(v) => {
                if !self.player_state.output.is_exclusive() {
                    self.link.send(Command::SetVolume { volume: v });
                }
            }
            playback_bar::Message::ToggleQueue => self.nav.go_to(Screen::NowPlaying),
            playback_bar::Message::ToggleSignalPath => {
                self.show_signal_path = !self.show_signal_path
            }
            playback_bar::Message::ToggleMiniPlayer => return self.toggle_mini_player(),
        }
        Task::none()
    }

    fn update_mini_player(&mut self, inner: mini_player::Message) -> Task<Message> {
        match inner {
            mini_player::Message::PlayPause => {
                self.link.send(Command::TogglePlayPause);
            }
            mini_player::Message::Previous => {
                self.link.send(Command::Previous);
            }
            mini_player::Message::Next => {
                self.link.send(Command::Next);
            }
            mini_player::Message::SeekChanged(ms) => {
                self.player_state.position_ms = u64::from(ms);
            }
            mini_player::Message::SeekReleased => {
                self.link.send(Command::Seek {
                    position_ms: self.player_state.position_ms,
                });
            }
            mini_player::Message::Restore => return self.show_main_window(),
        }
        Task::none()
    }

    /// Tray clicks (task item 2). `Show`/`Hide`/`Quit` are window/process
    /// lifecycle, handled here directly against iced's own `window`/`exit`
    /// `Task`s; everything else goes through
    /// `tray::tray_event_to_command`'s pure mapping, the same as any other
    /// input this shell turns into a `Command`.
    fn update_tray(&mut self, event: TrayEvent) -> Task<Message> {
        match event {
            TrayEvent::ShowMain => return self.show_main_window(),
            TrayEvent::HideMain => return self.hide_main_window(),
            TrayEvent::Quit => return self.quit(),
            TrayEvent::PlayPause | TrayEvent::Next | TrayEvent::Previous => {
                if let Some(cmd) = tray::tray_event_to_command(event) {
                    self.link.send(cmd);
                }
            }
        }
        Task::none()
    }

    /// D-014's close-intercept: the main window hides instead of closing;
    /// the mini-player window (which has no state worth preserving hidden)
    /// closes for real.
    fn handle_close_requested(&mut self, id: window::Id) -> Task<Message> {
        if id == self.main_window {
            self.hide_main_window()
        } else if Some(id) == self.mini_window {
            self.mini_window = None;
            window::close(id)
        } else {
            Task::none()
        }
    }

    fn hide_main_window(&self) -> Task<Message> {
        window::set_mode(self.main_window, window::Mode::Hidden)
    }

    fn show_main_window(&self) -> Task<Message> {
        window::set_mode(self.main_window, window::Mode::Windowed)
            .chain(window::gain_focus(self.main_window))
    }

    fn toggle_mini_player(&mut self) -> Task<Message> {
        match self.mini_window.take() {
            Some(id) => window::close(id),
            None => {
                let (id, open) = window::open(mini_window_settings());
                self.mini_window = Some(id);
                open.discard()
            }
        }
    }

    /// D-014: send `Command::Shutdown`, wait briefly for `Event::Stopped`,
    /// then exit regardless (never hang the quit on a player that never
    /// answers).
    fn quit(&self) -> Task<Message> {
        self.link.send(Command::Shutdown);
        let link = self.link.clone();
        Task::perform(wait_for_shutdown(link), |()| Message::ReadyToExit)
    }

    fn handle_player_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::State { state } => {
                self.player_state = state;
                Task::none()
            }
            Event::QueueChanged { queue, .. } => {
                self.queue = queue;
                Task::none()
            }
            Event::Position {
                position_ms,
                duration_ms,
            } => {
                self.player_state.position_ms = position_ms;
                self.player_state.duration_ms = duration_ms;
                // Task item 4: keep the Lyrics screen's highlighted line
                // (and its auto-scroll) in sync even while the screen isn't
                // the one currently shown, so it's correct the instant the
                // user opens it.
                let (task, _effect) = self.lyrics.update(
                    screens::lyrics::Message::PositionChanged(position_ms),
                    &self.ctx.api,
                );
                task.map(Message::Lyrics)
            }
            // --- Banner wave (task item 6): replaces the previous silent
            // arms for these three events with a dismissible, auto-expiring
            // banner (`ui::banner`). `PlaybackTakenOver` additionally offers
            // a resume button (D-033: only ever a real button press, never
            // an automatic retry).
            Event::Warning { message } => self
                .banner
                .push(message, banner::Tone::Warning, false)
                .map(Message::Banner),
            Event::Error { message, .. } => self
                .banner
                .push(message, banner::Tone::Danger, false)
                .map(Message::Banner),
            Event::PlaybackTakenOver { by } => self
                .banner
                .push(
                    format!("Playback started on {by}."),
                    banner::Tone::Info,
                    true,
                )
                .map(Message::Banner),
            // --- Task item 4: prefetch this track's lyrics as soon as it
            // starts, so the Lyrics screen (reached from Now Playing) opens
            // instantly instead of showing "Loading lyrics…" every time.
            Event::TrackStarted { track, .. } => {
                let (task, _effect) = self
                    .lyrics
                    .update(screens::lyrics::Message::Requested(track.id), &self.ctx.api);
                task.map(Message::Lyrics)
            }
            Event::TrackFinished { .. }
            | Event::Buffering { .. }
            | Event::AuthRequired { .. }
            | Event::AuthOk { .. }
            | Event::PinProgress { .. }
            | Event::PinReady { .. }
            | Event::PinFailed { .. }
            | Event::PinsChanged
            | Event::EndOfQueue
            | Event::Stopped => Task::none(),
        }
    }

    fn handle_shortcut(&mut self, shortcut: Shortcut) -> Task<Message> {
        match shortcut {
            Shortcut::Quit => return self.quit(),
            Shortcut::ToggleMiniPlayer => return self.toggle_mini_player(),
            _ if !self.logged_in => return Task::none(),
            Shortcut::TogglePlayPause => {
                self.link.send(Command::TogglePlayPause);
            }
            Shortcut::FocusSearch => self.nav.go_to(Screen::Search),
            // Task item 3: escape closes the add-to-playlist overlay first,
            // rather than navigating the screen underneath it — the modal
            // has no other keyboard dismissal and this is the one place
            // `Shortcut::Back` is already wired globally.
            Shortcut::Back if self.add_to_playlist.is_some() => {
                self.add_to_playlist = None;
            }
            Shortcut::Back => {
                let _ = self.nav.back();
            }
        }
        Task::none()
    }

    fn prefetch_images(&mut self, ids: Vec<String>) -> Task<Message> {
        let mut tasks = Vec::new();
        for id in ids {
            let Some(url) = cover_url(&id) else { continue };
            if self.images.contains(&url) {
                continue;
            }
            tasks.push(
                crate::ui::images::fetch(self.http.clone(), url)
                    .map(|(url, handle)| Message::ImageFetched(url, handle)),
            );
        }
        Task::batch(tasks)
    }

    fn view(&self, window: window::Id) -> Element<'_, Message> {
        if Some(window) == self.mini_window {
            let art = self.current_art();
            return mini_player::view(self.tokens, &self.player_state, art)
                .map(Message::MiniPlayer);
        }
        self.main_view()
    }

    fn main_view(&self) -> Element<'_, Message> {
        if !self.logged_in {
            return self.login.view(self.tokens).map(Message::Login);
        }

        let body: Element<'_, Message> = match self.nav.current() {
            Screen::Login => self.login.view(self.tokens).map(Message::Login),
            Screen::Home => self.home.view(self.tokens, &self.images).map(Message::Home),
            Screen::Explore => self
                .explore
                .view(self.tokens, &self.images)
                .map(Message::Explore),
            Screen::Search => self
                .search
                .view(self.tokens, &self.images)
                .map(Message::Search),
            Screen::NowPlaying => {
                let art = self.current_art();
                screens::now_playing::view(self.tokens, &self.player_state, &self.queue, art)
                    .map(Message::NowPlaying)
            }
            Screen::Settings => self.settings.view(self.tokens).map(Message::Settings),
            Screen::Entity(_) => self.entity_view(),
            Screen::Collection => self
                .collection
                .view(self.tokens, &self.images)
                .map(Message::Collection),
            Screen::Lyrics(_) => self.lyrics.view(self.tokens).map(Message::Lyrics),
        };

        let mut main_row = row![self.sidebar(), body]
            .height(Length::Fill)
            .width(Length::Fill);
        if self.show_signal_path {
            main_row = main_row
                .push(signal_path::view(self.tokens, &self.player_state).map(|m| match m {}));
        }

        let art = self.current_art();
        let bar = playback_bar::view(
            self.tokens,
            &self.player_state,
            art,
            self.nav.current() == &Screen::NowPlaying,
            self.show_signal_path,
            self.mini_window.is_some(),
        )
        .map(Message::PlaybackBar);

        let mut page = column![main_row, bar].height(Length::Fill);
        if let Some(banner_area) = self.banner.view(self.tokens) {
            page = column![banner_area.map(Message::Banner), page].height(Length::Fill);
        }

        let base: Element<'_, Message> = container(page)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme: &Theme| iced::widget::container::Style {
                background: Some(self.tokens.background.into()),
                text_color: Some(self.tokens.text),
                ..iced::widget::container::Style::default()
            })
            .into();

        // Task item 3: the "add to playlist" picker overlays whatever
        // screen opened it, dimming the background rather than replacing
        // it — iced 0.14 has no dedicated modal widget, so `stack!` plus a
        // full-size translucent backdrop is the mechanism (§6 of the
        // `iced-ui` skill has no stock "no wrap layout" surprises here,
        // just none of this needing one).
        match &self.add_to_playlist {
            Some(picker) => stack![
                base,
                container(picker.view(self.tokens).map(Message::AddToPlaylist))
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .style(move |_theme: &Theme| iced::widget::container::Style {
                        background: Some(
                            iced::Color {
                                a: 0.55,
                                ..iced::Color::BLACK
                            }
                            .into()
                        ),
                        ..iced::widget::container::Style::default()
                    }),
            ]
            .into(),
            None => base,
        }
    }

    /// The currently open entity page (task item 1), read from [`EntityScreen`]
    /// rather than [`Screen::Entity`]'s own payload — `sync_entity_screen`
    /// keeps the two in lockstep, and this avoids a second id-based match.
    fn entity_view(&self) -> Element<'_, Message> {
        match &self.entity {
            EntityScreen::None => crate::ui::widgets::page(
                self.tokens,
                iced::widget::text("Loading…").color(self.tokens.muted),
            ),
            EntityScreen::Album(state) => state.view(self.tokens, &self.images).map(Message::Album),
            EntityScreen::Artist(state) => {
                state.view(self.tokens, &self.images).map(Message::Artist)
            }
            EntityScreen::Playlist(state) => {
                state.view(self.tokens, &self.images).map(Message::Playlist)
            }
            EntityScreen::Mix(state) => state.view(self.tokens).map(Message::Mix),
            EntityScreen::Track(state) => state.view(self.tokens, &self.images).map(Message::Track),
            EntityScreen::Video(state) => state.view(self.tokens, &self.images).map(Message::Video),
        }
    }

    fn current_art(&self) -> Option<iced::widget::image::Handle> {
        let cover = self.player_state.current.as_ref()?.cover.as_deref()?;
        let url = cover_url(cover)?;
        self.images.peek(&url)
    }

    fn sidebar(&self) -> Element<'_, Message> {
        use crate::ui::widgets::nav_item;
        let current = self.nav.current();
        column![
            history_buttons(
                self.tokens,
                self.nav.can_go_back(),
                self.nav.can_go_forward()
            ),
            nav_item(
                self.tokens,
                "Home",
                current == &Screen::Home,
                nav_go(NavTarget::Home)
            ),
            nav_item(
                self.tokens,
                "Explore",
                current == &Screen::Explore,
                nav_go(NavTarget::Explore)
            ),
            nav_item(
                self.tokens,
                "Search",
                current == &Screen::Search,
                nav_go(NavTarget::Search)
            ),
            nav_item(
                self.tokens,
                "Now Playing",
                current == &Screen::NowPlaying,
                nav_go(NavTarget::NowPlaying),
            ),
            nav_item(
                self.tokens,
                "My Collection",
                current == &Screen::Collection,
                nav_go(NavTarget::Collection),
            ),
            iced::widget::space::vertical(),
            nav_item(
                self.tokens,
                "Settings",
                current == &Screen::Settings,
                nav_go(NavTarget::Settings)
            ),
        ]
        .spacing(self.tokens.space_xs)
        .padding(self.tokens.space_md)
        .width(Length::Fixed(200.0))
        .height(Length::Fill)
        .into()
    }
}

fn nav_go(target: NavTarget) -> Message {
    Message::Nav(NavAction::Go(target))
}

/// The sidebar's back/forward pair over the navigation stack (task item 1).
fn history_buttons(tokens: Tokens, can_back: bool, can_forward: bool) -> Element<'static, Message> {
    use iced::widget::{button, row, text};
    let arrow = |label: &'static str, enabled: bool, on_press: Message| {
        let mut b = button(text(label).size(tokens.text_sm))
            .padding([tokens.space_xs, tokens.space_sm])
            .style(move |_theme: &Theme, status| {
                let hovered = matches!(status, iced::widget::button::Status::Hovered);
                iced::widget::button::Style {
                    background: hovered.then_some(tokens.elevated.into()),
                    text_color: if enabled { tokens.text } else { tokens.muted },
                    border: iced::Border {
                        radius: tokens.radius_sm.into(),
                        ..iced::Border::default()
                    },
                    ..iced::widget::button::Style::default()
                }
            });
        if enabled {
            b = b.on_press(on_press);
        }
        b
    };
    row![
        arrow("◀", can_back, Message::Nav(NavAction::Back)),
        arrow("▶", can_forward, Message::Nav(NavAction::Forward)),
    ]
    .spacing(tokens.space_xs)
    .into()
}

impl From<NavTarget> for Screen {
    fn from(target: NavTarget) -> Self {
        match target {
            NavTarget::Home => Screen::Home,
            NavTarget::Explore => Screen::Explore,
            NavTarget::Search => Screen::Search,
            NavTarget::NowPlaying => Screen::NowPlaying,
            NavTarget::Collection => Screen::Collection,
            NavTarget::Settings => Screen::Settings,
        }
    }
}

fn player_events(link: &LinkKey) -> crate::ui::stream_ext::BoxStream<Event> {
    link.0.events()
}

/// Builder for the tray's event subscription (task item 2) — same
/// `fn(&LinkKey) -> BoxStream<_>` idiom as [`player_events`] above, reusing
/// `LinkKey` rather than inventing a second wrapper: the app holds exactly
/// one link for its whole lifetime either way.
fn tray_events(link: &LinkKey) -> crate::ui::stream_ext::BoxStream<TrayEvent> {
    tray::spawn(link.0.clone())
}

/// Waits for `Event::Stopped`/`Event::EndOfQueue` on a fresh subscription
/// to `link`, capped at [`SHUTDOWN_GRACE`] — used by [`App::quit`] so
/// quitting never hangs on a player that does not answer.
async fn wait_for_shutdown(link: SharedLink) {
    use futures::StreamExt as _;
    let mut events = link.events();
    let wait_for_stop = async {
        while let Some(ev) = events.next().await {
            if matches!(ev, Event::Stopped | Event::EndOfQueue) {
                break;
            }
        }
    };
    let _ = tokio::time::timeout(SHUTDOWN_GRACE, wait_for_stop).await;
}

/// Keyboard shortcuts (task item 4): space play/pause, ctrl+f focuses
/// search, escape goes back one step, ctrl+m toggles the mini-player
/// (task item 1), ctrl+q quits (D-014). Media keys arrive via MPRIS later,
/// not here.
fn keyboard_shortcut(
    event: iced::Event,
    status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Shortcut> {
    use iced::keyboard::{self, Key, Modifiers, key::Named};

    if status == iced::event::Status::Captured {
        return None;
    }
    let iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event else {
        return None;
    };
    match key {
        Key::Named(Named::Space) => Some(Shortcut::TogglePlayPause),
        Key::Named(Named::Escape) => Some(Shortcut::Back),
        Key::Character(c) if c == "f" && modifiers.contains(Modifiers::CTRL) => {
            Some(Shortcut::FocusSearch)
        }
        Key::Character(c) if c == "m" && modifiers.contains(Modifiers::CTRL) => {
            Some(Shortcut::ToggleMiniPlayer)
        }
        Key::Character(c) if c == "q" && modifiers.contains(Modifiers::CTRL) => {
            Some(Shortcut::Quit)
        }
        _ => None,
    }
}

/// Task item 5 (D-039 "shared playlist links open and play"): every track
/// id of a shared playlist, for `App::open_deep_link` to hand straight to
/// `Command::Play`.
async fn load_playlist_track_ids(
    api: streamboat_core::ApiClient,
    uuid: String,
) -> Result<Vec<u64>, String> {
    let items = api
        .playlist_items_all(&uuid, 10_000)
        .await
        .map_err(|e| e.to_string())?;
    Ok(items
        .iter()
        .filter_map(|i| i.as_track())
        .map(|t| t.id)
        .collect())
}

/// The main window's settings: `exit_on_close_request: false` is the
/// mechanism D-014's "closing the window hides it" relies on — see this
/// module's top-level doc comment.
fn main_window_settings() -> window::Settings {
    window::Settings {
        size: iced::Size::new(1180.0, 760.0),
        min_size: Some(iced::Size::new(860.0, 560.0)),
        exit_on_close_request: false,
        ..window::Settings::default()
    }
}

/// The mini-player window's settings (task item 1): compact, fixed-size,
/// always-on-top. Left at the default `exit_on_close_request: true` — a
/// native close on this window is exactly the same as the toggle button,
/// there is no state to preserve by hiding it instead.
fn mini_window_settings() -> window::Settings {
    window::Settings {
        size: mini_player::SIZE,
        min_size: Some(mini_player::SIZE),
        max_size: Some(mini_player::SIZE),
        resizable: false,
        level: window::Level::AlwaysOnTop,
        ..window::Settings::default()
    }
}

/// Runs the desktop shell. Decides once, at startup, whether this process
/// becomes the single instance (spawns its own engine), a remote client of
/// a daemon that already holds the lock, or neither (D-010, D-045,
/// `ui::instance`); the rest of `App` never needs to know which — every
/// screen keeps working unchanged behind [`crate::ui::player_link::PlayerLink`].
pub fn run(open_url: Option<String>) -> anyhow::Result<()> {
    let ctx = Context::load()?;
    match instance::decide(&ctx.dirs)? {
        instance::Decision::FocusedOther => {
            if let Some(url) = open_url {
                // The running instance was asked to show itself; handing it
                // the link is not built yet (see docs/architecture.md).
                tracing::warn!(%url, "another streamboat instance is running; open the link there");
            }
            Ok(())
        }
        instance::Decision::Local(lock) => run_as_local_instance(ctx, lock, open_url),
        instance::Decision::Remote(addr) => run_as_remote_client(ctx, addr, open_url),
    }
}

/// This process holds the instance lock: build the platform engine
/// (D-016 via `engine_select`), probe which quality tiers it can actually
/// decode and cap the requested ceiling if it exceeds them (D-003), spawn
/// [`Player`], register MPRIS on Linux (task item 5), and run the shell
/// with an in-process [`InProcessLink`]. If the stored tokens are not
/// valid, the boot task flips to the Login screen instead of failing.
fn run_as_local_instance(
    ctx: Context,
    lock: InstanceLock,
    open_url: Option<String>,
) -> anyhow::Result<()> {
    let (etx, erx) = std_mpsc::channel();
    let output = ctx.settings.output.clone().unwrap_or_default();
    let engine = engine_select::build(etx, output.clone(), &ctx.dirs.runtime)
        .map_err(|e| anyhow::anyhow!("starting the audio engine: {e}"))?;

    let decoder_support = streamboat_player::probe::probe();
    let requested = ctx.settings.quality_ceiling();
    let effective = decoder_support.cap(requested).unwrap_or(requested);
    let cfg = PlayerConfig {
        quality_ceiling: effective,
        output,
        volume: 1.0,
        replay_gain_mode: ctx.settings.replay_gain_mode,
    };

    // Keeps the Player's background task alive for the whole run; dropped
    // (and shut down) only when `run_program` returns, i.e. at process
    // exit.
    let bg_rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let handle = bg_rt.block_on(async {
        let deps = PlayerDeps::for_context(&ctx).await.map_err(|e| {
            anyhow::anyhow!("wiring play reporting, scrobbling and streaming privileges: {e}")
        })?;
        Ok::<_, anyhow::Error>(Player::spawn(ctx.api.clone(), engine, erx, cfg, deps))
    })?;

    if effective != requested {
        handle.publish(Event::Warning {
            message: format!(
                "quality ceiling capped from {requested} to {effective}: this build's decoders \
                 cannot reach {requested} (D-003 decoder probe)"
            ),
        });
    }

    // OS media controls (D-030): MPRIS on Linux, SMTC on Windows,
    // NowPlaying on macOS — the same call `streamboatd` makes; registration
    // failures log and never block startup.
    streamboat_player::media_controls::spawn(handle.clone());

    let link: SharedLink = Arc::new(InProcessLink::new(handle));
    let show_request_path = Some(ctx.dirs.show_request_path());

    run_program(
        ctx,
        link,
        open_url,
        decoder_support,
        show_request_path,
        bg_rt,
        Some(lock),
    )
}

/// Another process already holds the lock and hosts the control API
/// (always `streamboatd` today, D-031): become a remote client over
/// [`RemoteLink`] instead of spawning an engine here.
fn run_as_remote_client(
    ctx: Context,
    addr: std::net::SocketAddr,
    open_url: Option<String>,
) -> anyhow::Result<()> {
    let bg_rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let token = instance::read_control_token(&ctx.dirs.control_token_path())?;
    let link: SharedLink = Arc::new(RemoteLink::new(addr, token, bg_rt.handle().clone()));
    // Nothing runs locally to probe or cap: the daemon already resolved
    // its own ceiling at its own startup, and this process never touches
    // an `Engine` (D-010) — "everything reachable" is the honest default
    // for a value this wave does not otherwise use in the remote case.
    let decoder_support = DecoderSupport::all();
    run_program(ctx, link, open_url, decoder_support, None, bg_rt, None)
}

/// Shared by both startup paths above: builds and runs the iced daemon.
/// `_bg_rt` and `_lock` are held for the whole call — dropped only when
/// `.run()` returns, i.e. at process exit — and never touched again after
/// being handed in, which the leading underscores mark as deliberate.
fn run_program(
    ctx: Context,
    link: SharedLink,
    open_url: Option<String>,
    decoder_support: DecoderSupport,
    show_request_path: Option<PathBuf>,
    _bg_rt: tokio::runtime::Runtime,
    _lock: Option<InstanceLock>,
) -> anyhow::Result<()> {
    iced::daemon(
        move || {
            App::boot(
                ctx.clone(),
                link.clone(),
                open_url.clone(),
                decoder_support,
                show_request_path.clone(),
            )
        },
        App::update,
        App::view,
    )
    .title(App::title)
    .theme(App::theme)
    .subscription(App::subscription)
    .run()
    .map_err(Into::into)
}
