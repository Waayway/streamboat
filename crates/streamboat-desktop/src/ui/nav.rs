//! The navigation stack: a persistent left sidebar plus back/forward, per
//! `tidal-client-features` browse-pages-screens.md §7. Entity pages,
//! Collection and Lyrics are real screens now (task item 1-2 of the next
//! wave); this module still owns every [`Screen`] variant and the id each
//! one carries, plus [`Screen::from_content_link`] (task item 5), which
//! turns a parsed deep link into the screen it opens.

use streamboat_core::api::images::ContentLink;

/// One entity an entity page renders.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EntityRef {
    Album(u64),
    Artist(u64),
    Track(u64),
    Playlist(String),
    Mix(String),
    /// Metadata only — video playback is not supported yet (D-038).
    Video(u64),
}

impl EntityRef {
    pub fn kind_label(&self) -> &'static str {
        match self {
            EntityRef::Album(_) => "Album",
            EntityRef::Artist(_) => "Artist",
            EntityRef::Track(_) => "Track",
            EntityRef::Playlist(_) => "Playlist",
            EntityRef::Mix(_) => "Mix",
            EntityRef::Video(_) => "Video",
        }
    }

    pub fn id_label(&self) -> String {
        match self {
            EntityRef::Album(id)
            | EntityRef::Track(id)
            | EntityRef::Artist(id)
            | EntityRef::Video(id) => id.to_string(),
            EntityRef::Playlist(id) | EntityRef::Mix(id) => id.clone(),
        }
    }
}

/// Every reachable screen. Login is reachable only before the user is
/// signed in (`App` refuses to navigate anywhere else until then); every
/// other variant is reachable once logged in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Login,
    Home,
    Explore,
    Search,
    NowPlaying,
    Settings,
    /// Album/artist/playlist/mix/track/video pages, routed with the real
    /// entity id.
    Entity(EntityRef),
    /// My Collection: favourites, playlists and folders.
    Collection,
    /// Full-screen synced lyrics for a track.
    Lyrics(u64),
}

impl Screen {
    pub fn title(&self) -> String {
        match self {
            Screen::Login => "Log in".to_string(),
            Screen::Home => "Home".to_string(),
            Screen::Explore => "Explore".to_string(),
            Screen::Search => "Search".to_string(),
            Screen::NowPlaying => "Now Playing".to_string(),
            Screen::Settings => "Settings".to_string(),
            Screen::Entity(e) => format!("{} {}", e.kind_label(), e.id_label()),
            Screen::Collection => "My Collection".to_string(),
            Screen::Lyrics(id) => format!("Lyrics — track {id}"),
        }
    }

    /// A parsed deep link (task item 5), turned into the screen it opens.
    /// Always resolves to *some* screen — a folder link, with no dedicated
    /// per-folder screen yet, opens My Collection rather than failing.
    /// Callers additionally special-case [`ContentLink::Playlist`] to also
    /// start playback (D-039: "shared playlist links open and play") — that
    /// needs an async fetch this pure mapping deliberately doesn't do.
    pub fn from_content_link(link: ContentLink) -> Screen {
        match link {
            ContentLink::Track(id) => Screen::Entity(EntityRef::Track(id)),
            ContentLink::Album(id) => Screen::Entity(EntityRef::Album(id)),
            ContentLink::Artist(id) => Screen::Entity(EntityRef::Artist(id)),
            ContentLink::Video(id) => Screen::Entity(EntityRef::Video(id)),
            ContentLink::Playlist(uuid) => Screen::Entity(EntityRef::Playlist(uuid)),
            ContentLink::Mix(id) => Screen::Entity(EntityRef::Mix(id)),
            ContentLink::Folder(_) => Screen::Collection,
            ContentLink::AlbumTrack { track_id, .. } => Screen::Entity(EntityRef::Track(track_id)),
        }
    }
}

/// Back/forward navigation over [`Screen`]s (task item 1: "a `Screen` enum
/// with a navigation stack (back/forward)").
#[derive(Debug, Clone)]
pub struct Nav {
    current: Screen,
    back: Vec<Screen>,
    forward: Vec<Screen>,
}

impl Nav {
    pub fn new(start: Screen) -> Self {
        Self {
            current: start,
            back: Vec::new(),
            forward: Vec::new(),
        }
    }

    pub fn current(&self) -> &Screen {
        &self.current
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// Navigate to `screen`, pushing the current screen onto the back
    /// stack and clearing the forward stack — a no-op if already there.
    pub fn go_to(&mut self, screen: Screen) {
        if screen == self.current {
            return;
        }
        self.forward.clear();
        self.back.push(std::mem::replace(&mut self.current, screen));
    }

    /// Replace the current screen without touching either stack — used
    /// when a screen navigates within itself (e.g. Home ↔ Explore tabs
    /// that should not pile up back-stack entries). Unused by the wave-1
    /// screens today but kept for the next wave's tab bars.
    #[allow(dead_code)]
    pub fn replace(&mut self, screen: Screen) {
        self.current = screen;
    }

    pub fn back(&mut self) -> bool {
        let Some(previous) = self.back.pop() else {
            return false;
        };
        self.forward
            .push(std::mem::replace(&mut self.current, previous));
        true
    }

    pub fn forward(&mut self) -> bool {
        let Some(next) = self.forward.pop() else {
            return false;
        };
        self.back.push(std::mem::replace(&mut self.current, next));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_to_pushes_history_and_clears_forward() {
        let mut nav = Nav::new(Screen::Home);
        nav.go_to(Screen::Explore);
        nav.go_to(Screen::Search);
        assert_eq!(nav.current(), &Screen::Search);
        assert!(nav.can_go_back());
        assert!(!nav.can_go_forward());
    }

    #[test]
    fn back_then_forward_round_trips() {
        let mut nav = Nav::new(Screen::Home);
        nav.go_to(Screen::Explore);
        nav.go_to(Screen::Search);

        assert!(nav.back());
        assert_eq!(nav.current(), &Screen::Explore);
        assert!(nav.can_go_forward());

        assert!(nav.back());
        assert_eq!(nav.current(), &Screen::Home);
        assert!(!nav.can_go_back());

        assert!(nav.forward());
        assert_eq!(nav.current(), &Screen::Explore);
        assert!(nav.forward());
        assert_eq!(nav.current(), &Screen::Search);
        assert!(!nav.can_go_forward());
    }

    #[test]
    fn back_on_empty_history_is_a_no_op() {
        let mut nav = Nav::new(Screen::Home);
        assert!(!nav.back());
        assert_eq!(nav.current(), &Screen::Home);
    }

    #[test]
    fn navigating_after_back_drops_the_old_forward_branch() {
        let mut nav = Nav::new(Screen::Home);
        nav.go_to(Screen::Explore);
        nav.go_to(Screen::Search);
        assert!(nav.back());
        nav.go_to(Screen::Settings);
        assert!(!nav.can_go_forward());
        assert!(nav.back());
        assert_eq!(nav.current(), &Screen::Explore);
    }

    #[test]
    fn go_to_same_screen_is_a_no_op() {
        let mut nav = Nav::new(Screen::Home);
        nav.go_to(Screen::Home);
        assert!(!nav.can_go_back());
    }

    #[test]
    fn entity_labels_render_id_by_kind() {
        assert_eq!(EntityRef::Album(42).id_label(), "42");
        assert_eq!(EntityRef::Playlist("abc-123".into()).id_label(), "abc-123");
        assert_eq!(EntityRef::Artist(7).kind_label(), "Artist");
        assert_eq!(EntityRef::Video(9).kind_label(), "Video");
    }

    #[test]
    fn content_link_maps_onto_the_matching_entity_screen() {
        assert_eq!(
            Screen::from_content_link(ContentLink::Track(1)),
            Screen::Entity(EntityRef::Track(1))
        );
        assert_eq!(
            Screen::from_content_link(ContentLink::Playlist("abc".into())),
            Screen::Entity(EntityRef::Playlist("abc".into()))
        );
        assert_eq!(
            Screen::from_content_link(ContentLink::Video(3)),
            Screen::Entity(EntityRef::Video(3))
        );
    }

    #[test]
    fn folder_link_falls_back_to_collection() {
        assert_eq!(
            Screen::from_content_link(ContentLink::Folder("f-1".into())),
            Screen::Collection
        );
    }

    #[test]
    fn album_track_link_opens_the_track() {
        assert_eq!(
            Screen::from_content_link(ContentLink::AlbumTrack {
                album_id: 10,
                track_id: 20
            }),
            Screen::Entity(EntityRef::Track(20))
        );
    }
}
