//! System tray (D-014): Show/Hide, Play/Pause, Next, Previous,
//! Quit, and a "now playing" tooltip. Linux uses `ksni` (StatusNotifierItem
//! over D-Bus, no GTK dependency, version verified against its real
//! 0.3.6 crate source on 2026-09-09 — see below); Windows/macOS use
//! `tray-icon` 0.24.1, cfg-gated and compile-checked only.
//!
//! Both backends only ever produce [`TrayEvent`]s onto one channel; nothing
//! about window lifecycle or `Command` construction lives in the
//! platform-specific code, so [`tray_event_to_command`] — the one place
//! that maps a click to a [`Command`] — is plain, D-Bus-free logic,
//! testable (and tested) without a session bus or a display.
//!
//! The tray fails soft everywhere: no SNI host, no
//! session bus, or (on the other platforms) no icon/menu backend available
//! all end in a logged warning and an event stream that simply never
//! yields anything — never a startup failure, and `ui::app::run` never
//! awaits tray startup before opening the main window.
//!
//! ## What is and isn't verified here
//!
//! - **Linux (`ksni = "0.3"`, resolved to 0.3.6 in this workspace)**:
//!   exercised by this crate's own tests only as far as the pure
//!   `TrayEvent -> Command` mapping below; the actual D-Bus registration
//!   needs a session bus this sandbox does not have (no
//!   `dbus-run-session`), so it compiles and the crate's full test suite
//!   passes with it linked, but the tray itself is not run here — the same
//!   posture `mpris.rs` already documents for its own D-Bus registration.
//! - **Windows/macOS (`tray-icon = "0.24"`, resolved to 0.24.1)**:
//!   compile-checked only, and even that only by explicitly cross-checking
//!   with `cargo check --target x86_64-pc-windows-gnu` / `--target
//!   aarch64-apple-darwin` — this platform module is behind
//!   `#[cfg(not(target_os = "linux"))]`, so an ordinary `cargo check` on
//!   this Linux sandbox never parses it at all. `tray-icon`'s own
//!   documentation requires the tray icon to be created on the same thread
//!   as a running native event loop (a win32 message loop on Windows, the
//!   main thread's loop on macOS); this implementation creates it on its
//!   own dedicated thread with no such loop pumped on Windows, and off the
//!   main thread entirely on macOS — both contradict that requirement.
//!   This is flagged loudly rather than shipped as if it were equivalent to
//!   the Linux path: it needs a real Windows/macOS machine, or integration
//!   with `iced_winit`'s own event loop (not exposed to application code
//!   today), before it can be trusted. Nothing here should be read as
//!   "tray works on Windows/macOS" — only "it is wired up and the crate
//!   graph resolves and type-checks for those targets."

use streamboat_core::proto::Command;

use crate::ui::player_link::SharedLink;
use crate::ui::stream_ext::BoxStream;

/// What a tray interaction means, decoupled from both the platform backend
/// below and from `ui::app::Message` — `ui::app` maps this into its own
/// `Message::Tray` variant and handles it in `update`, the same seam every
/// other cross-cutting input (keyboard shortcuts, `PlayerLink` events) goes
/// through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    ShowMain,
    HideMain,
    PlayPause,
    Next,
    Previous,
    Quit,
}

/// The menu entries every backend below builds, in order — one list so
/// Linux and Windows/macOS can never drift into offering a different set
/// (the fixed six: "Show/Hide, Play/Pause, Next, Previous, Quit").
const MENU_ENTRIES: &[(&str, TrayEvent)] = &[
    ("Show", TrayEvent::ShowMain),
    ("Hide", TrayEvent::HideMain),
    ("Play/Pause", TrayEvent::PlayPause),
    ("Next", TrayEvent::Next),
    ("Previous", TrayEvent::Previous),
    ("Quit", TrayEvent::Quit),
];

/// The one pure mapping this module exists to keep pure:
/// which [`Command`] a tray event sends, or `None` for the three window/
/// process-lifecycle events (`ShowMain`/`HideMain`/`Quit`) that are not
/// `Command`s at all — `ui::app::update` handles those directly against
/// iced's own window/`exit` `Task`s instead.
pub fn tray_event_to_command(event: TrayEvent) -> Option<Command> {
    match event {
        TrayEvent::PlayPause => Some(Command::TogglePlayPause),
        TrayEvent::Next => Some(Command::Next),
        TrayEvent::Previous => Some(Command::Previous),
        TrayEvent::ShowMain | TrayEvent::HideMain | TrayEvent::Quit => None,
    }
}

/// "Now playing" text for the tray's tooltip/title. Kept here
/// (not duplicated per backend) since both platform paths want the exact
/// same string.
pub fn now_playing_text(title: Option<&str>, artists: Option<&str>) -> String {
    match (title, artists) {
        (Some(t), Some(a)) if !t.is_empty() && !a.is_empty() => format!("{t} — {a}"),
        (Some(t), _) if !t.is_empty() => t.to_string(),
        _ => "streamboat".to_string(),
    }
}

/// Spawns the tray on its own dedicated OS thread and returns the stream of
/// clicks. Fails soft: any error along the way is logged and the returned
/// stream simply never yields, matching `mpris::spawn`'s "optional OS
/// integration, never something the daemon (or here, the shell) depends on
/// to run" posture.
pub fn spawn(link: SharedLink) -> BoxStream<TrayEvent> {
    #[cfg(target_os = "linux")]
    {
        linux::spawn(link)
    }
    #[cfg(not(target_os = "linux"))]
    {
        other::spawn(link)
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use futures::StreamExt as _;
    use tokio::sync::mpsc;

    use super::{MENU_ENTRIES, TrayEvent, now_playing_text};
    use crate::ui::player_link::SharedLink;
    use crate::ui::stream_ext::BoxStream;

    /// The `ksni::Tray` implementor. `activate` callbacks
    /// (`Box<dyn Fn(&mut T) + Send>` per ksni 0.3.6's `menu::StandardItem`)
    /// get `&mut Self` and send into `tx` — a non-blocking, synchronous
    /// `UnboundedSender::send`, safe to call from inside ksni's own D-Bus
    /// callback context.
    struct StreamboatTray {
        tx: mpsc::UnboundedSender<TrayEvent>,
        title: String,
        subtitle: String,
    }

    impl ksni::Tray for StreamboatTray {
        fn id(&self) -> String {
            "io.github.waayway.streamboat".into()
        }
        fn icon_name(&self) -> String {
            "media-playback-start".into()
        }
        fn title(&self) -> String {
            now_playing_text(Some(&self.title), Some(&self.subtitle))
        }
        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: now_playing_text(Some(&self.title), Some(&self.subtitle)),
                ..Default::default()
            }
        }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::StandardItem;
            MENU_ENTRIES
                .iter()
                .map(|(label, event)| {
                    let event = *event;
                    StandardItem {
                        label: label.to_string(),
                        activate: Box::new(move |this: &mut Self| {
                            let _ = this.tx.send(event);
                        }),
                        ..Default::default()
                    }
                    .into()
                })
                .collect()
        }
    }

    pub fn spawn(link: SharedLink) -> BoxStream<TrayEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        std::thread::Builder::new()
            .name("streamboat-tray".into())
            .spawn(move || run(tx, link))
            .expect("spawn streamboat-tray thread");
        Box::pin(futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|ev| (ev, rx))
        }))
    }

    fn run(tx: mpsc::UnboundedSender<TrayEvent>, link: SharedLink) {
        use ksni::TrayMethods as _;
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                tracing::warn!(error = %e, "tray: could not start a runtime; tray disabled");
                return;
            }
        };
        rt.block_on(async move {
            let tray = StreamboatTray {
                tx,
                title: String::new(),
                subtitle: String::new(),
            };
            let handle = match tray.spawn().await {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "tray: no StatusNotifierItem host or session bus reachable; tray disabled"
                    );
                    return;
                }
            };
            tracing::info!("tray: registered a StatusNotifierItem");
            let mut events = link.events();
            while let Some(ev) = events.next().await {
                if let streamboat_core::proto::Event::TrackStarted { track, .. } = ev {
                    let _ = handle
                        .update(|t: &mut StreamboatTray| {
                            t.title = track.title;
                            t.subtitle = track.artists;
                        })
                        .await;
                }
            }
        });
    }
}

/// Windows/macOS via `tray-icon` — see this module's top-level doc comment
/// for exactly what "compile-checked only" means here.
#[cfg(not(target_os = "linux"))]
mod other {
    use std::collections::HashMap;
    use std::sync::mpsc as std_mpsc;

    use super::{MENU_ENTRIES, TrayEvent};
    use crate::ui::player_link::SharedLink;
    use crate::ui::stream_ext::BoxStream;

    pub fn spawn(_link: SharedLink) -> BoxStream<TrayEvent> {
        let (tx, rx) = std_mpsc::channel::<TrayEvent>();
        std::thread::Builder::new()
            .name("streamboat-tray".into())
            .spawn(move || run(tx))
            .expect("spawn streamboat-tray thread");
        Box::pin(futures::stream::unfold(rx, |rx| async move {
            // `std::sync::mpsc::Receiver::recv` blocks the *calling*
            // thread; the closure below runs on whatever executor polls
            // this subscription's stream (iced's own tokio runtime), so
            // blocking it directly would stall every other task on that
            // runtime. Move the blocking receive onto tokio's blocking
            // pool instead.
            tokio::task::spawn_blocking(move || rx.recv().ok().map(|ev| (ev, rx)))
                .await
                .ok()
                .flatten()
        }))
    }

    fn run(tx: std_mpsc::Sender<TrayEvent>) {
        let icon = match placeholder_icon() {
            Ok(icon) => icon,
            Err(e) => {
                tracing::warn!(error = %e, "tray: could not build the tray icon; tray disabled");
                return;
            }
        };

        let menu = tray_icon::menu::Menu::new();
        let mut by_id: HashMap<tray_icon::menu::MenuId, TrayEvent> = HashMap::new();
        for (label, event) in MENU_ENTRIES {
            let item = tray_icon::menu::MenuItem::new(*label, true, None);
            by_id.insert(item.id().clone(), *event);
            if let Err(e) = menu.append(&item) {
                tracing::warn!(error = %e, "tray: could not append a menu item");
            }
        }

        // Kept alive for as long as this function's loop runs below;
        // dropping it would remove the tray icon.
        let _tray_icon = match tray_icon::TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("streamboat")
            .with_icon(icon)
            .build()
        {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "tray: could not create the tray icon; tray disabled");
                return;
            }
        };

        let menu_events = tray_icon::menu::MenuEvent::receiver();
        loop {
            match menu_events.recv() {
                Ok(ev) => {
                    if let Some(mapped) = by_id.get(ev.id()) {
                        if tx.send(*mapped).is_err() {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
    }

    /// A flat, solid-accent square (`Tokens::dark().accent`, hand-copied
    /// rather than imported to keep this platform module free of a
    /// dependency on `ui::design`): no bundled icon asset exists yet, and
    /// `tray_icon::Icon::from_rgba` needs raw RGBA bytes, not a path or a
    /// named freedesktop icon the way `ksni::Tray::icon_name` gets to use
    /// on Linux. A real glyph is follow-up design work, not something this
    /// tray's plumbing needs to block on.
    fn placeholder_icon() -> Result<tray_icon::Icon, tray_icon::BadIcon> {
        const SIZE: u32 = 16;
        let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
        for _ in 0..(SIZE * SIZE) {
            rgba.extend_from_slice(&[0x6c, 0x8c, 0xff, 0xff]);
        }
        tray_icon::Icon::from_rgba(rgba, SIZE, SIZE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_pause_next_previous_map_to_the_matching_command() {
        assert_eq!(
            tray_event_to_command(TrayEvent::PlayPause),
            Some(Command::TogglePlayPause)
        );
        assert_eq!(tray_event_to_command(TrayEvent::Next), Some(Command::Next));
        assert_eq!(
            tray_event_to_command(TrayEvent::Previous),
            Some(Command::Previous)
        );
    }

    #[test]
    fn window_and_process_lifecycle_events_are_not_commands() {
        assert_eq!(tray_event_to_command(TrayEvent::ShowMain), None);
        assert_eq!(tray_event_to_command(TrayEvent::HideMain), None);
        assert_eq!(tray_event_to_command(TrayEvent::Quit), None);
    }

    #[test]
    fn every_menu_entry_maps_to_something_ui_app_can_act_on() {
        // Not every entry maps to a `Command` (Show/Hide/Quit legitimately
        // don't), but every one of the fixed six in `MENU_ENTRIES` must at
        // least be present and distinct — this catches an accidental
        // duplicate or a dropped entry there.
        let events: Vec<TrayEvent> = MENU_ENTRIES.iter().map(|(_, e)| *e).collect();
        let mut unique = events.clone();
        unique.sort_by_key(|e| format!("{e:?}"));
        unique.dedup();
        assert_eq!(events.len(), 6);
        assert_eq!(unique.len(), 6, "MENU_ENTRIES must not repeat an event");
    }

    #[test]
    fn now_playing_text_falls_back_to_the_app_name_when_idle() {
        assert_eq!(now_playing_text(None, None), "streamboat");
        assert_eq!(now_playing_text(Some(""), Some("")), "streamboat");
    }

    #[test]
    fn now_playing_text_joins_title_and_artist_when_both_are_known() {
        assert_eq!(
            now_playing_text(Some("A Title"), Some("An Artist")),
            "A Title — An Artist"
        );
    }

    #[test]
    fn now_playing_text_falls_back_to_the_title_alone() {
        assert_eq!(now_playing_text(Some("A Title"), None), "A Title");
        assert_eq!(now_playing_text(Some("A Title"), Some("")), "A Title");
    }
}
