//! The top-level `App`: startup (task item 2), the split top-level
//! `Message` enum wrapping each screen's own (D-013), the navigation stack,
//! the persistent sidebar and bottom playback bar, and the keyboard
//! shortcuts (task item 4).

use std::sync::Arc;
use std::sync::mpsc as std_mpsc;

use iced::widget::{column, container, row};
use iced::{Element, Length, Subscription, Task, Theme};

use streamboat_core::bootstrap::Context;
use streamboat_core::config::ThemePreference;
use streamboat_core::models::TrackSummary;
use streamboat_core::proto::{Command, Event, PlayItem, PlayerState};
use streamboat_player::{Player, PlayerConfig, PlayerDeps};

use crate::ui::design::Tokens;
use crate::ui::engine_select;
use crate::ui::format::cover_url;
use crate::ui::images::ImageCache;
use crate::ui::nav::{Nav, Screen};
use crate::ui::player_link::{InProcessLink, LinkKey, SharedLink};
use crate::ui::{playback_bar, screens, signal_path};

const IMAGE_CACHE_CAPACITY: usize = 512;

pub struct App {
    ctx: Context,
    link: SharedLink,
    http: reqwest::Client,

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
    /// Boxed: `Event::State` carries a full `PlayerState` snapshot, which
    /// makes this by far the largest variant — boxing it keeps every other
    /// `Message` variant (and every `Task`/`Subscription` built from them)
    /// from paying that size on the stack.
    PlayerEvent(Box<Event>),
    LoginCheck(bool),
    ImageFetched(String, Option<iced::widget::image::Handle>),
    KeyShortcut(Shortcut),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavAction {
    Go(NavTarget),
    Back,
    Forward,
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
pub enum Shortcut {
    TogglePlayPause,
    FocusSearch,
    Back,
}

impl App {
    fn boot(ctx: Context, link: SharedLink) -> (Self, Task<Message>) {
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
        let settings = screens::settings::State::from_settings(&ctx.settings, key_location);
        let api = ctx.api.clone();
        let app = Self {
            ctx,
            link,
            http,
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
        };
        let check = Task::perform(async move { api.is_logged_in().await }, Message::LoginCheck);
        (app, check)
    }

    fn title(&self) -> String {
        format!("streamboat — {}", self.nav.current().title())
    }

    fn theme(&self) -> Theme {
        self.tokens.iced_theme(match self.ctx.settings.theme {
            ThemePreference::Dark => "streamboat dark",
            ThemePreference::Light => "streamboat light",
        })
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            Subscription::run_with(LinkKey(self.link.clone()), player_events)
                .map(|event| Message::PlayerEvent(Box::new(event))),
            iced::event::listen_with(keyboard_shortcut).map(Message::KeyShortcut),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::LoginCheck(true) => {
                self.logged_in = true;
                self.nav = Nav::new(Screen::Home);
                self.load_home_and_explore()
            }
            Message::LoginCheck(false) => Task::none(),
            Message::Nav(action) => {
                self.apply_nav(action);
                Task::none()
            }
            Message::Login(inner) => self.update_login(inner),
            Message::Home(inner) => self.update_home(inner),
            Message::Explore(inner) => self.update_explore(inner),
            Message::Search(inner) => self.update_search(inner),
            Message::NowPlaying(inner) => self.update_now_playing(inner),
            Message::Settings(inner) => self.update_settings(inner),
            Message::PlaybackBar(inner) => self.update_playback_bar(inner),
            Message::PlayerEvent(event) => self.handle_player_event(*event),
            Message::ImageFetched(url, handle) => {
                if let Some(handle) = handle {
                    self.images.insert(url, handle);
                }
                Task::none()
            }
            Message::KeyShortcut(shortcut) => self.handle_shortcut(shortcut),
        }
    }

    fn load_home_and_explore(&mut self) -> Task<Message> {
        Task::batch([
            screens::home::State::load(&self.ctx.api).map(Message::Home),
            screens::explore::State::load(&self.ctx.api).map(Message::Explore),
        ])
    }

    fn apply_nav(&mut self, action: NavAction) {
        if !self.logged_in {
            return;
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
    }

    fn update_login(&mut self, inner: screens::login::Message) -> Task<Message> {
        let (task, effect) =
            self.login
                .update(inner, &self.ctx.api, &self.ctx.device.client_unique_key);
        if matches!(effect, Some(screens::login::Effect::LoggedIn)) {
            self.logged_in = true;
            self.nav = Nav::new(Screen::Home);
            return Task::batch([task.map(Message::Login), self.load_home_and_explore()]);
        }
        task.map(Message::Login)
    }

    fn update_home(&mut self, inner: screens::home::Message) -> Task<Message> {
        let (task, effects) = self.home.update(inner, &self.ctx.api);
        let mut tasks = vec![task.map(Message::Home)];
        for effect in effects {
            match effect {
                screens::home::Effect::Navigate(entity) => self.nav.go_to(Screen::Entity(entity)),
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
                    self.nav.go_to(Screen::Entity(entity))
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
                    self.nav.go_to(Screen::NowPlaying);
                }
                screens::search::Effect::Enqueue(id, position) => {
                    self.link.send(Command::Enqueue {
                        items: vec![PlayItem { track_id: id }],
                        position,
                    });
                }
                screens::search::Effect::Navigate(entity) => self.nav.go_to(Screen::Entity(entity)),
                screens::search::Effect::ImagesNeeded(ids) => tasks.push(self.prefetch_images(ids)),
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
                self.nav.go_to(Screen::Lyrics(track_id));
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
        }
        Task::none()
    }

    fn handle_player_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::State { state } => self.player_state = state,
            Event::QueueChanged { queue, .. } => self.queue = queue,
            Event::Position {
                position_ms,
                duration_ms,
            } => {
                self.player_state.position_ms = position_ms;
                self.player_state.duration_ms = duration_ms;
            }
            Event::TrackStarted { .. }
            | Event::TrackFinished { .. }
            | Event::Buffering { .. }
            | Event::Warning { .. }
            | Event::Error { .. }
            | Event::PlaybackTakenOver { .. }
            | Event::AuthRequired { .. }
            | Event::AuthOk { .. }
            | Event::PinProgress { .. }
            | Event::PinReady { .. }
            | Event::PinFailed { .. }
            | Event::PinsChanged
            | Event::EndOfQueue
            | Event::Stopped => {}
        }
        Task::none()
    }

    fn handle_shortcut(&mut self, shortcut: Shortcut) -> Task<Message> {
        if !self.logged_in {
            return Task::none();
        }
        match shortcut {
            Shortcut::TogglePlayPause => {
                self.link.send(Command::TogglePlayPause);
            }
            Shortcut::FocusSearch => self.nav.go_to(Screen::Search),
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

    fn view(&self) -> Element<'_, Message> {
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
            Screen::Entity(entity) => {
                crate::ui::screens::placeholder::entity_view(self.tokens, entity)
                    .map(|m| match m {})
            }
            Screen::Collection => {
                crate::ui::screens::placeholder::collection_view(self.tokens).map(|m| match m {})
            }
            Screen::Lyrics(id) => {
                crate::ui::screens::placeholder::lyrics_view(self.tokens, *id).map(|m| match m {})
            }
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
        )
        .map(Message::PlaybackBar);

        container(column![main_row, bar].height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme: &Theme| iced::widget::container::Style {
                background: Some(self.tokens.background.into()),
                text_color: Some(self.tokens.text),
                ..iced::widget::container::Style::default()
            })
            .into()
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

/// Keyboard shortcuts (task item 4): space play/pause, ctrl+f focuses
/// search, escape goes back. Media keys arrive via MPRIS later, not here.
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
        _ => None,
    }
}

/// Runs the desktop shell: loads [`Context`], builds the platform engine
/// (D-016 via `engine_select`), spawns [`Player`], and opens the window
/// (task item 2). If the stored tokens are not valid, the boot task flips
/// to the Login screen instead of failing.
pub fn run() -> anyhow::Result<()> {
    let ctx = Context::load()?;
    let (etx, erx) = std_mpsc::channel();
    let output = ctx.settings.output.clone().unwrap_or_default();
    let engine = engine_select::build(etx, output.clone(), &ctx.dirs.runtime)
        .map_err(|e| anyhow::anyhow!("starting the audio engine: {e}"))?;
    let cfg = PlayerConfig {
        quality_ceiling: ctx.settings.quality_ceiling(),
        output,
        volume: 1.0,
        replay_gain_mode: ctx.settings.replay_gain_mode,
    };
    // Keeps the Player's background task alive for the whole run; dropped
    // (and shut down) only when `run()` returns, i.e. at process exit.
    let player_rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let handle = player_rt.block_on(async {
        let deps = PlayerDeps::for_context(&ctx).await.map_err(|e| {
            anyhow::anyhow!("wiring play reporting, scrobbling and streaming privileges: {e}")
        })?;
        Ok::<_, anyhow::Error>(Player::spawn(ctx.api.clone(), engine, erx, cfg, deps))
    })?;
    let link: SharedLink = Arc::new(InProcessLink::new(handle));

    iced::application(
        move || App::boot(ctx.clone(), link.clone()),
        App::update,
        App::view,
    )
    .title(App::title)
    .theme(App::theme)
    .subscription(App::subscription)
    .window(iced::window::Settings {
        size: iced::Size::new(1180.0, 760.0),
        min_size: Some(iced::Size::new(860.0, 560.0)),
        ..iced::window::Settings::default()
    })
    .run()
    .map_err(Into::into)
}
