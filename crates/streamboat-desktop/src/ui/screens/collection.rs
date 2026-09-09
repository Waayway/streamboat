//! My Collection: tabs for tracks / albums / artists /
//! playlists / mixes, the documented per-list sort orders, paging with
//! load-more through `api/pagination` helpers, favourite/unfavourite from
//! every card and row, a "create playlist" dialog, and a playlists-and-
//! folders list (`tidal-client-features` library-playlists-collections.md
//! §2). Every tab loads lazily, on first visit, rather than all five lists
//! at once — this screen alone would otherwise fire more requests on login
//! than Home and Explore combined.

use iced::widget::{button, column, container, pick_list, row, scrollable, text, text_input};
use iced::{Alignment, Border, Element, Length, Task, Theme};

use streamboat_core::api::library::{AlbumOrder, ArtistOrder, ItemOrder, MixOrder};
use streamboat_core::api::pagination::DEFAULT_PAGE_SIZE as PAGE_SIZE;
use streamboat_core::models::{Album, ArtistProfile, Favorite, MixSummary, Playlist, Track};
use streamboat_core::proto::QueuePosition;
use streamboat_core::{ApiClient, Error};

use crate::ui::actions::{self, TrackAction};
use crate::ui::design::Tokens;
use crate::ui::format::mmss;
use crate::ui::nav::EntityRef;
use crate::ui::widgets::{ChipTone, banner};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Tracks,
    Albums,
    Artists,
    Playlists,
    Mixes,
}

impl Tab {
    const ALL: [Tab; 5] = [
        Tab::Tracks,
        Tab::Albums,
        Tab::Artists,
        Tab::Playlists,
        Tab::Mixes,
    ];

    fn label(self) -> &'static str {
        match self {
            Tab::Tracks => "Tracks",
            Tab::Albums => "Albums",
            Tab::Artists => "Artists",
            Tab::Playlists => "Playlists",
            Tab::Mixes => "Mixes & Radio",
        }
    }
}

pub struct State {
    tab: Tab,
    loaded: bool,

    tracks: Vec<Favorite<Track>>,
    tracks_offset: u32,
    tracks_has_more: bool,
    tracks_loading: bool,
    tracks_order: ItemOrder,

    albums: Vec<Favorite<Album>>,
    albums_offset: u32,
    albums_has_more: bool,
    albums_loading: bool,
    albums_order: AlbumOrder,

    artists: Vec<Favorite<ArtistProfile>>,
    artists_offset: u32,
    artists_has_more: bool,
    artists_loading: bool,
    artists_order: ArtistOrder,

    mixes: Vec<Favorite<MixSummary>>,
    mixes_offset: u32,
    mixes_has_more: bool,
    mixes_loading: bool,
    mixes_order: MixOrder,

    playlists: Vec<Playlist>,
    playlists_loading: bool,
    playlists_sort_by_name: bool,
    folders: Vec<serde_json::Value>,

    create_playlist: Option<CreatePlaylistState>,
    error: Option<String>,
}

struct CreatePlaylistState {
    name: String,
    description: String,
    saving: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            tab: Tab::Tracks,
            loaded: false,
            tracks: Vec::new(),
            tracks_offset: 0,
            tracks_has_more: false,
            tracks_loading: true,
            tracks_order: ItemOrder::Date,
            albums: Vec::new(),
            albums_offset: 0,
            albums_has_more: false,
            albums_loading: false,
            albums_order: AlbumOrder::Date,
            artists: Vec::new(),
            artists_offset: 0,
            artists_has_more: false,
            artists_loading: false,
            artists_order: ArtistOrder::Date,
            mixes: Vec::new(),
            mixes_offset: 0,
            mixes_has_more: false,
            mixes_loading: false,
            mixes_order: MixOrder::Date,
            playlists: Vec::new(),
            playlists_loading: false,
            playlists_sort_by_name: false,
            folders: Vec::new(),
            create_playlist: None,
            error: None,
        }
    }
}

impl State {
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    TabSelected(Tab),

    TracksLoaded(Result<(Vec<Favorite<Track>>, bool), String>),
    LoadMoreTracks,
    TrackOrderChanged(ItemOrder),
    Track(TrackAction),
    TrackFavoriteToggled(Result<(u64, bool), String>),

    AlbumsLoaded(Result<(Vec<Favorite<Album>>, bool), String>),
    LoadMoreAlbums,
    AlbumOrderChanged(AlbumOrder),
    UnfavoriteAlbum(u64),
    AlbumUnfavorited(u64, Result<(), String>),

    ArtistsLoaded(Result<(Vec<Favorite<ArtistProfile>>, bool), String>),
    LoadMoreArtists,
    ArtistOrderChanged(ArtistOrder),
    UnfollowArtist(u64),
    ArtistUnfollowed(u64, Result<(), String>),

    MixesLoaded(Result<(Vec<Favorite<MixSummary>>, bool), String>),
    LoadMoreMixes,
    MixOrderChanged(MixOrder),
    UnfavoriteMix(String),
    MixUnfavorited(String, Result<(), String>),

    PlaylistsLoaded(Result<Vec<Playlist>, String>),
    FoldersLoaded(Result<Vec<serde_json::Value>, String>),
    TogglePlaylistSort,

    OpenCreatePlaylist,
    CancelCreatePlaylist,
    CreatePlaylistNameChanged(String),
    CreatePlaylistDescriptionChanged(String),
    SubmitCreatePlaylist,
    PlaylistCreated(Result<(), String>),

    Navigate(EntityRef),
}

pub enum Effect {
    PlayTracks(Vec<u64>),
    Enqueue(u64, QueuePosition),
    Navigate(EntityRef),
    ImagesNeeded(Vec<String>),
    OpenAddToPlaylist(u64),
}

impl State {
    /// The first tab's load, fired once when the user first opens My
    /// Collection — every other tab loads lazily on [`Message::TabSelected`].
    pub fn load(api: &ApiClient) -> Task<Message> {
        Task::perform(
            load_tracks(api.clone(), 0, ItemOrder::Date),
            Message::TracksLoaded,
        )
    }

    pub fn update(&mut self, message: Message, api: &ApiClient) -> (Task<Message>, Vec<Effect>) {
        self.loaded = true;
        match message {
            Message::TabSelected(tab) => {
                self.tab = tab;
                let task = match tab {
                    Tab::Tracks if self.tracks.is_empty() && self.tracks_loading => Task::none(),
                    Tab::Albums if self.albums.is_empty() && !self.albums_loading => {
                        self.albums_loading = true;
                        Task::perform(
                            load_albums(api.clone(), 0, self.albums_order),
                            Message::AlbumsLoaded,
                        )
                    }
                    Tab::Artists if self.artists.is_empty() && !self.artists_loading => {
                        self.artists_loading = true;
                        Task::perform(
                            load_artists(api.clone(), 0, self.artists_order),
                            Message::ArtistsLoaded,
                        )
                    }
                    Tab::Mixes if self.mixes.is_empty() && !self.mixes_loading => {
                        self.mixes_loading = true;
                        Task::perform(
                            load_mixes(api.clone(), 0, self.mixes_order),
                            Message::MixesLoaded,
                        )
                    }
                    Tab::Playlists if self.playlists.is_empty() && !self.playlists_loading => {
                        self.playlists_loading = true;
                        Task::batch([
                            Task::perform(load_playlists(api.clone()), Message::PlaylistsLoaded),
                            Task::perform(load_folders(api.clone()), Message::FoldersLoaded),
                        ])
                    }
                    _ => Task::none(),
                };
                (task, Vec::new())
            }

            Message::TracksLoaded(Ok((items, has_more))) => {
                self.tracks_loading = false;
                self.tracks_has_more = has_more;
                self.tracks_offset += items.len() as u32;
                self.tracks.extend(items);
                (Task::none(), Vec::new())
            }
            Message::TracksLoaded(Err(e)) => {
                self.tracks_loading = false;
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::LoadMoreTracks => {
                self.tracks_loading = true;
                (
                    Task::perform(
                        load_tracks(api.clone(), self.tracks_offset, self.tracks_order),
                        Message::TracksLoaded,
                    ),
                    Vec::new(),
                )
            }
            Message::TrackOrderChanged(order) => {
                self.tracks_order = order;
                self.tracks = Vec::new();
                self.tracks_offset = 0;
                self.tracks_loading = true;
                (
                    Task::perform(load_tracks(api.clone(), 0, order), Message::TracksLoaded),
                    Vec::new(),
                )
            }
            Message::Track(action) => self.handle_track_action(action, api),
            Message::TrackFavoriteToggled(Ok((id, now))) => {
                if !now {
                    self.tracks.retain(|f| f.item.id != id);
                }
                (Task::none(), Vec::new())
            }
            Message::TrackFavoriteToggled(Err(e)) => {
                self.error = Some(e);
                (Task::none(), Vec::new())
            }

            Message::AlbumsLoaded(Ok((items, has_more))) => {
                self.albums_loading = false;
                self.albums_has_more = has_more;
                self.albums_offset += items.len() as u32;
                let images = items.iter().filter_map(|f| f.item.cover.clone()).collect();
                self.albums.extend(items);
                (Task::none(), vec![Effect::ImagesNeeded(images)])
            }
            Message::AlbumsLoaded(Err(e)) => {
                self.albums_loading = false;
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::LoadMoreAlbums => {
                self.albums_loading = true;
                (
                    Task::perform(
                        load_albums(api.clone(), self.albums_offset, self.albums_order),
                        Message::AlbumsLoaded,
                    ),
                    Vec::new(),
                )
            }
            Message::AlbumOrderChanged(order) => {
                self.albums_order = order;
                self.albums = Vec::new();
                self.albums_offset = 0;
                self.albums_loading = true;
                (
                    Task::perform(load_albums(api.clone(), 0, order), Message::AlbumsLoaded),
                    Vec::new(),
                )
            }
            Message::UnfavoriteAlbum(id) => (
                Task::perform(unfavorite_album(api.clone(), id), move |r| {
                    Message::AlbumUnfavorited(id, r)
                }),
                Vec::new(),
            ),
            Message::AlbumUnfavorited(id, Ok(())) => {
                self.albums.retain(|f| f.item.id != id);
                (Task::none(), Vec::new())
            }
            Message::AlbumUnfavorited(_, Err(e)) => {
                self.error = Some(e);
                (Task::none(), Vec::new())
            }

            Message::ArtistsLoaded(Ok((items, has_more))) => {
                self.artists_loading = false;
                self.artists_has_more = has_more;
                self.artists_offset += items.len() as u32;
                let images = items
                    .iter()
                    .filter_map(|f| f.item.picture.clone())
                    .collect();
                self.artists.extend(items);
                (Task::none(), vec![Effect::ImagesNeeded(images)])
            }
            Message::ArtistsLoaded(Err(e)) => {
                self.artists_loading = false;
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::LoadMoreArtists => {
                self.artists_loading = true;
                (
                    Task::perform(
                        load_artists(api.clone(), self.artists_offset, self.artists_order),
                        Message::ArtistsLoaded,
                    ),
                    Vec::new(),
                )
            }
            Message::ArtistOrderChanged(order) => {
                self.artists_order = order;
                self.artists = Vec::new();
                self.artists_offset = 0;
                self.artists_loading = true;
                (
                    Task::perform(load_artists(api.clone(), 0, order), Message::ArtistsLoaded),
                    Vec::new(),
                )
            }
            Message::UnfollowArtist(id) => (
                Task::perform(unfollow_artist(api.clone(), id), move |r| {
                    Message::ArtistUnfollowed(id, r)
                }),
                Vec::new(),
            ),
            Message::ArtistUnfollowed(id, Ok(())) => {
                self.artists.retain(|f| f.item.id != id);
                (Task::none(), Vec::new())
            }
            Message::ArtistUnfollowed(_, Err(e)) => {
                self.error = Some(e);
                (Task::none(), Vec::new())
            }

            Message::MixesLoaded(Ok((items, has_more))) => {
                self.mixes_loading = false;
                self.mixes_has_more = has_more;
                self.mixes_offset += items.len() as u32;
                self.mixes.extend(items);
                (Task::none(), Vec::new())
            }
            Message::MixesLoaded(Err(e)) => {
                self.mixes_loading = false;
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::LoadMoreMixes => {
                self.mixes_loading = true;
                (
                    Task::perform(
                        load_mixes(api.clone(), self.mixes_offset, self.mixes_order),
                        Message::MixesLoaded,
                    ),
                    Vec::new(),
                )
            }
            Message::MixOrderChanged(order) => {
                self.mixes_order = order;
                self.mixes = Vec::new();
                self.mixes_offset = 0;
                self.mixes_loading = true;
                (
                    Task::perform(load_mixes(api.clone(), 0, order), Message::MixesLoaded),
                    Vec::new(),
                )
            }
            Message::UnfavoriteMix(id) => (
                Task::perform(unfavorite_mix(api.clone(), id.clone()), move |r| {
                    Message::MixUnfavorited(id.clone(), r)
                }),
                Vec::new(),
            ),
            Message::MixUnfavorited(id, Ok(())) => {
                self.mixes
                    .retain(|f| f.item.id.as_deref() != Some(id.as_str()));
                (Task::none(), Vec::new())
            }
            Message::MixUnfavorited(_, Err(e)) => {
                self.error = Some(e);
                (Task::none(), Vec::new())
            }

            Message::PlaylistsLoaded(Ok(playlists)) => {
                self.playlists_loading = false;
                let images = playlists
                    .iter()
                    .filter_map(crate::ui::format::playlist_cover_url)
                    .collect();
                self.playlists = playlists;
                (Task::none(), vec![Effect::ImagesNeeded(images)])
            }
            Message::PlaylistsLoaded(Err(e)) => {
                self.playlists_loading = false;
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::FoldersLoaded(Ok(folders)) => {
                self.folders = folders;
                (Task::none(), Vec::new())
            }
            Message::FoldersLoaded(Err(_)) => (Task::none(), Vec::new()),
            Message::TogglePlaylistSort => {
                self.playlists_sort_by_name = !self.playlists_sort_by_name;
                (Task::none(), Vec::new())
            }

            Message::OpenCreatePlaylist => {
                self.create_playlist = Some(CreatePlaylistState {
                    name: String::new(),
                    description: String::new(),
                    saving: false,
                });
                (Task::none(), Vec::new())
            }
            Message::CancelCreatePlaylist => {
                self.create_playlist = None;
                (Task::none(), Vec::new())
            }
            Message::CreatePlaylistNameChanged(v) => {
                if let Some(c) = &mut self.create_playlist {
                    c.name = v;
                }
                (Task::none(), Vec::new())
            }
            Message::CreatePlaylistDescriptionChanged(v) => {
                if let Some(c) = &mut self.create_playlist {
                    c.description = v;
                }
                (Task::none(), Vec::new())
            }
            Message::SubmitCreatePlaylist => {
                let Some(c) = &mut self.create_playlist else {
                    return (Task::none(), Vec::new());
                };
                if c.name.trim().is_empty() {
                    return (Task::none(), Vec::new());
                }
                c.saving = true;
                (
                    Task::perform(
                        create_playlist(api.clone(), c.name.clone(), c.description.clone()),
                        Message::PlaylistCreated,
                    ),
                    Vec::new(),
                )
            }
            Message::PlaylistCreated(Ok(())) => {
                self.create_playlist = None;
                self.playlists_loading = true;
                (
                    Task::perform(load_playlists(api.clone()), Message::PlaylistsLoaded),
                    Vec::new(),
                )
            }
            Message::PlaylistCreated(Err(e)) => {
                if let Some(c) = &mut self.create_playlist {
                    c.saving = false;
                }
                self.error = Some(e);
                (Task::none(), Vec::new())
            }

            Message::Navigate(entity) => (Task::none(), vec![Effect::Navigate(entity)]),
        }
    }

    fn handle_track_action(
        &mut self,
        action: TrackAction,
        api: &ApiClient,
    ) -> (Task<Message>, Vec<Effect>) {
        match action {
            TrackAction::PlayNow(id) => (Task::none(), vec![Effect::PlayTracks(vec![id])]),
            TrackAction::PlayNext(id) => {
                (Task::none(), vec![Effect::Enqueue(id, QueuePosition::Next)])
            }
            TrackAction::AddLast(id) => {
                (Task::none(), vec![Effect::Enqueue(id, QueuePosition::Last)])
            }
            TrackAction::ToggleFavorite(id) => (
                Task::perform(
                    actions::toggle_track_favorite(api.clone(), id, true),
                    Message::TrackFavoriteToggled,
                ),
                Vec::new(),
            ),
            TrackAction::AddToPlaylist(id) => (Task::none(), vec![Effect::OpenAddToPlaylist(id)]),
        }
    }

    pub fn view<'a>(
        &'a self,
        tokens: Tokens,
        images: &crate::ui::images::ImageCache,
    ) -> Element<'a, Message> {
        let mut tabs = row![].spacing(tokens.space_xs);
        for tab in Tab::ALL {
            let active = tab == self.tab;
            tabs = tabs.push(
                button(text(tab.label()).size(tokens.text_sm))
                    .padding([tokens.space_xs, tokens.space_md])
                    .on_press(Message::TabSelected(tab))
                    .style(move |_theme: &Theme, status| {
                        let hovered = matches!(status, button::Status::Hovered);
                        let background = if active {
                            tokens.accent_muted
                        } else if hovered {
                            tokens.elevated
                        } else {
                            tokens.surface
                        };
                        button::Style {
                            background: Some(background.into()),
                            text_color: if active { tokens.accent } else { tokens.text },
                            border: Border {
                                radius: tokens.radius_pill.into(),
                                ..Border::default()
                            },
                            ..button::Style::default()
                        }
                    }),
            );
        }

        let mut body = column![
            text("My Collection")
                .size(tokens.text_xl)
                .color(tokens.text),
            tabs,
        ]
        .spacing(tokens.space_md)
        .width(Length::Fill);

        if let Some(e) = &self.error {
            body = body.push(banner(tokens, e.clone(), ChipTone::Danger));
        }

        let tab_content = match self.tab {
            Tab::Tracks => self.tracks_view(tokens),
            Tab::Albums => self.albums_view(tokens, images),
            Tab::Artists => self.artists_view(tokens, images),
            Tab::Playlists => self.playlists_view(tokens, images),
            Tab::Mixes => self.mixes_view(tokens),
        };
        body = body.push(scrollable(tab_content).height(Length::Fill));

        crate::ui::widgets::page(tokens, body)
    }

    fn tracks_view<'a>(&'a self, tokens: Tokens) -> Element<'a, Message> {
        let order = pick_list(
            [
                ItemOrder::Date,
                ItemOrder::Name,
                ItemOrder::Artist,
                ItemOrder::Album,
                ItemOrder::Length,
                ItemOrder::Index,
            ]
            .to_vec(),
            Some(self.tracks_order),
            Message::TrackOrderChanged,
        );
        let mut col = column![sort_row(tokens, "Sort", order.into())].spacing(tokens.space_sm);
        if self.tracks.is_empty() && !self.tracks_loading {
            col = col.push(
                text("No favourite tracks yet.")
                    .size(tokens.text_sm)
                    .color(tokens.muted),
            );
        }
        for f in &self.tracks {
            let t = &f.item;
            let album = t.album.as_ref().and_then(|a| a.id).map(EntityRef::Album);
            let artist = t
                .artists
                .first()
                .or(t.artist.as_ref())
                .and_then(|a| a.id)
                .map(EntityRef::Artist);
            let content = row![
                column![
                    text(t.title.clone())
                        .size(tokens.text_sm)
                        .color(tokens.text),
                    text(t.artist_names())
                        .size(tokens.text_xs)
                        .color(tokens.muted),
                ]
                .spacing(2)
                .width(Length::Fill),
                text(mmss(t.duration.map(|d| u64::from(d) * 1000)))
                    .size(tokens.text_sm)
                    .color(tokens.muted),
                actions::track_action_row(
                    tokens,
                    t.id,
                    true,
                    album,
                    artist,
                    Message::Track,
                    Message::Navigate
                ),
            ]
            .spacing(tokens.space_sm)
            .align_y(Alignment::Center)
            .width(Length::Fill);
            col = col.push(row_container(tokens, content));
        }
        if self.tracks_loading {
            col = col.push(text("Loading…").size(tokens.text_sm).color(tokens.muted));
        } else if self.tracks_has_more {
            col = col.push(load_more_button(tokens, Message::LoadMoreTracks));
        }
        col.into()
    }

    fn albums_view<'a>(
        &'a self,
        tokens: Tokens,
        images: &crate::ui::images::ImageCache,
    ) -> Element<'a, Message> {
        let order = pick_list(
            [
                AlbumOrder::Date,
                AlbumOrder::Name,
                AlbumOrder::Artist,
                AlbumOrder::ReleaseDate,
            ]
            .to_vec(),
            Some(self.albums_order),
            Message::AlbumOrderChanged,
        );
        let mut col = column![sort_row(tokens, "Sort", order.into())].spacing(tokens.space_sm);
        if self.albums.is_empty() && !self.albums_loading {
            col = col.push(
                text("No favourite albums yet.")
                    .size(tokens.text_sm)
                    .color(tokens.muted),
            );
        }
        for chunk in self.albums.chunks(5) {
            let mut r = row![].spacing(tokens.space_md);
            for f in chunk {
                r = r.push(album_card(tokens, &f.item, images));
            }
            col = col.push(r);
        }
        if self.albums_loading {
            col = col.push(text("Loading…").size(tokens.text_sm).color(tokens.muted));
        } else if self.albums_has_more {
            col = col.push(load_more_button(tokens, Message::LoadMoreAlbums));
        }
        col.into()
    }

    fn artists_view<'a>(
        &'a self,
        tokens: Tokens,
        images: &crate::ui::images::ImageCache,
    ) -> Element<'a, Message> {
        let order = pick_list(
            [ArtistOrder::Date, ArtistOrder::Name].to_vec(),
            Some(self.artists_order),
            Message::ArtistOrderChanged,
        );
        let mut col = column![sort_row(tokens, "Sort", order.into())].spacing(tokens.space_sm);
        if self.artists.is_empty() && !self.artists_loading {
            col = col.push(
                text("No followed artists yet.")
                    .size(tokens.text_sm)
                    .color(tokens.muted),
            );
        }
        for chunk in self.artists.chunks(5) {
            let mut r = row![].spacing(tokens.space_md);
            for f in chunk {
                r = r.push(artist_card(tokens, &f.item, images));
            }
            col = col.push(r);
        }
        if self.artists_loading {
            col = col.push(text("Loading…").size(tokens.text_sm).color(tokens.muted));
        } else if self.artists_has_more {
            col = col.push(load_more_button(tokens, Message::LoadMoreArtists));
        }
        col.into()
    }

    fn mixes_view<'a>(&'a self, tokens: Tokens) -> Element<'a, Message> {
        let order = pick_list(
            [MixOrder::Date, MixOrder::Name, MixOrder::MixType].to_vec(),
            Some(self.mixes_order),
            Message::MixOrderChanged,
        );
        let mut col = column![sort_row(tokens, "Sort", order.into())].spacing(tokens.space_sm);
        if self.mixes.is_empty() && !self.mixes_loading {
            col = col.push(
                text("No mixes yet.")
                    .size(tokens.text_sm)
                    .color(tokens.muted),
            );
        }
        for f in &self.mixes {
            let m = &f.item;
            let title = m.title.clone().unwrap_or_else(|| "Mix".to_string());
            let Some(id) = m.id.clone() else { continue };
            let content = row![
                column![
                    text(title).size(tokens.text_sm).color(tokens.text),
                    text(m.sub_title.clone().unwrap_or_default())
                        .size(tokens.text_xs)
                        .color(tokens.muted),
                ]
                .spacing(2)
                .width(Length::Fill),
                open_button(
                    tokens,
                    "Open",
                    Message::Navigate(EntityRef::Mix(id.clone()))
                ),
                heart_button(tokens, Message::UnfavoriteMix(id)),
            ]
            .spacing(tokens.space_sm)
            .align_y(Alignment::Center)
            .width(Length::Fill);
            col = col.push(row_container(tokens, content));
        }
        if self.mixes_loading {
            col = col.push(text("Loading…").size(tokens.text_sm).color(tokens.muted));
        } else if self.mixes_has_more {
            col = col.push(load_more_button(tokens, Message::LoadMoreMixes));
        }
        col.into()
    }

    fn playlists_view<'a>(
        &'a self,
        tokens: Tokens,
        images: &crate::ui::images::ImageCache,
    ) -> Element<'a, Message> {
        let mut col = column![
            row![
                secondary_button(tokens, "+ New playlist", Message::OpenCreatePlaylist),
                secondary_button(
                    tokens,
                    if self.playlists_sort_by_name {
                        "Sorted: name"
                    } else {
                        "Sorted: recent"
                    },
                    Message::TogglePlaylistSort,
                ),
            ]
            .spacing(tokens.space_sm),
        ]
        .spacing(tokens.space_sm);

        if let Some(create) = &self.create_playlist {
            col = col.push(create_playlist_form(tokens, create));
        }

        let mut sorted: Vec<&Playlist> = self.playlists.iter().collect();
        if self.playlists_sort_by_name {
            sorted.sort_by(|a, b| a.title.cmp(&b.title));
        }
        if sorted.is_empty() && !self.playlists_loading {
            col = col.push(
                text("No playlists yet.")
                    .size(tokens.text_sm)
                    .color(tokens.muted),
            );
        }
        for p in sorted {
            col = col.push(playlist_row(tokens, p, images));
        }
        if self.playlists_loading {
            col = col.push(text("Loading…").size(tokens.text_sm).color(tokens.muted));
        }

        if !self.folders.is_empty() {
            col = col.push(text("Folders").size(tokens.text_md).color(tokens.text));
            for f in &self.folders {
                if let Some(name) = folder_display_name(f) {
                    col = col.push(
                        container(text(name).size(tokens.text_sm).color(tokens.muted))
                            .padding(tokens.space_sm)
                            .width(Length::Fill)
                            .style(move |_theme: &Theme| container::Style {
                                background: Some(tokens.surface.into()),
                                border: Border {
                                    radius: tokens.radius_sm.into(),
                                    ..Border::default()
                                },
                                ..container::Style::default()
                            }),
                    );
                }
            }
        }

        col.into()
    }
}

fn row_container<'a>(
    tokens: Tokens,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(content)
        .padding(tokens.space_sm)
        .width(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(tokens.surface.into()),
            border: Border {
                radius: tokens.radius_sm.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn sort_row<'a>(
    tokens: Tokens,
    label: &'a str,
    control: Element<'a, Message>,
) -> Element<'a, Message> {
    row![
        text(label).size(tokens.text_xs).color(tokens.muted),
        control
    ]
    .spacing(tokens.space_sm)
    .align_y(Alignment::Center)
    .into()
}

fn album_card<'a>(
    tokens: Tokens,
    album: &'a Album,
    images: &crate::ui::images::ImageCache,
) -> Element<'a, Message> {
    use iced::widget::image;
    let art: Element<'a, Message> = match album
        .cover
        .as_deref()
        .and_then(crate::ui::format::cover_url)
        .and_then(|u| images.peek(&u))
    {
        Some(handle) => image(handle).width(140.0).height(140.0).into(),
        None => container(text("")).width(140.0).height(140.0).into(),
    };
    let body = column![
        art,
        text(album.title.clone())
            .size(tokens.text_sm)
            .color(tokens.text),
        row![
            open_button(
                tokens,
                "Open",
                Message::Navigate(EntityRef::Album(album.id))
            ),
            heart_button(tokens, Message::UnfavoriteAlbum(album.id)),
        ]
        .spacing(tokens.space_xs),
    ]
    .spacing(tokens.space_xs);
    crate::ui::widgets::card(tokens, 140.0, body, None)
}

fn artist_card<'a>(
    tokens: Tokens,
    artist: &'a ArtistProfile,
    images: &crate::ui::images::ImageCache,
) -> Element<'a, Message> {
    use iced::widget::image;
    let art: Element<'a, Message> = match artist
        .picture
        .as_deref()
        .and_then(crate::ui::format::cover_url)
        .and_then(|u| images.peek(&u))
    {
        Some(handle) => image(handle).width(120.0).height(120.0).into(),
        None => container(text("")).width(120.0).height(120.0).into(),
    };
    let body = column![
        art,
        text(artist.name.clone())
            .size(tokens.text_sm)
            .color(tokens.text),
        row![
            open_button(
                tokens,
                "Open",
                Message::Navigate(EntityRef::Artist(artist.id))
            ),
            heart_button(tokens, Message::UnfollowArtist(artist.id)),
        ]
        .spacing(tokens.space_xs),
    ]
    .spacing(tokens.space_xs);
    crate::ui::widgets::card(tokens, 120.0, body, None)
}

fn playlist_row<'a>(
    tokens: Tokens,
    playlist: &'a Playlist,
    images: &crate::ui::images::ImageCache,
) -> Element<'a, Message> {
    use iced::widget::image;
    let art: Element<'a, Message> =
        match crate::ui::format::playlist_cover_url(playlist).and_then(|u| images.peek(&u)) {
            Some(handle) => image(handle).width(56.0).height(56.0).into(),
            None => container(text("")).width(56.0).height(56.0).into(),
        };
    let content = row![
        art,
        column![
            text(playlist.title.clone())
                .size(tokens.text_sm)
                .color(tokens.text),
            text(format!("{} tracks", playlist.number_of_tracks.unwrap_or(0)))
                .size(tokens.text_xs)
                .color(tokens.muted),
        ]
        .spacing(2)
        .width(Length::Fill),
    ]
    .spacing(tokens.space_sm)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let Some(uuid) = playlist.uuid.clone() else {
        return row_container(tokens, content);
    };
    button(content)
        .width(Length::Fill)
        .padding(tokens.space_sm)
        .on_press(Message::Navigate(EntityRef::Playlist(uuid)))
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(
                    (if hovered {
                        tokens.elevated
                    } else {
                        tokens.surface
                    })
                    .into(),
                ),
                text_color: tokens.text,
                border: Border {
                    radius: tokens.radius_sm.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }
        })
        .into()
}

fn create_playlist_form<'a>(
    tokens: Tokens,
    state: &'a CreatePlaylistState,
) -> Element<'a, Message> {
    container(
        column![
            text_input("Playlist name", &state.name).on_input(Message::CreatePlaylistNameChanged),
            text_input("Description (optional)", &state.description)
                .on_input(Message::CreatePlaylistDescriptionChanged),
            row![
                secondary_button(
                    tokens,
                    if state.saving {
                        "Creating…"
                    } else {
                        "Create"
                    },
                    Message::SubmitCreatePlaylist
                ),
                secondary_button(tokens, "Cancel", Message::CancelCreatePlaylist),
            ]
            .spacing(tokens.space_sm),
        ]
        .spacing(tokens.space_sm),
    )
    .padding(tokens.space_sm)
    .width(Length::Fill)
    .style(move |_theme: &Theme| container::Style {
        background: Some(tokens.elevated.into()),
        border: Border {
            color: tokens.border,
            width: 1.0,
            radius: tokens.radius_sm.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn folder_display_name(value: &serde_json::Value) -> Option<String> {
    let obj = value.as_object()?;
    obj.get("name")
        .or_else(|| obj.get("title"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn open_button<'a>(tokens: Tokens, label: &'a str, on_press: Message) -> Element<'a, Message> {
    button(text(label).size(tokens.text_xs))
        .padding([tokens.space_xs, tokens.space_sm])
        .on_press(on_press)
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: hovered.then_some(tokens.elevated.into()),
                text_color: tokens.muted,
                border: Border {
                    radius: tokens.radius_sm.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }
        })
        .into()
}

fn heart_button<'a>(tokens: Tokens, on_press: Message) -> Element<'a, Message> {
    button(text("♥").size(tokens.text_sm))
        .padding([tokens.space_xs, tokens.space_sm])
        .on_press(on_press)
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: hovered.then_some(tokens.elevated.into()),
                text_color: tokens.accent,
                border: Border {
                    radius: tokens.radius_sm.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }
        })
        .into()
}

fn secondary_button<'a>(tokens: Tokens, label: &'a str, on_press: Message) -> Element<'a, Message> {
    button(text(label).size(tokens.text_sm))
        .padding([tokens.space_xs, tokens.space_md])
        .on_press(on_press)
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(
                    (if hovered {
                        tokens.elevated
                    } else {
                        tokens.surface
                    })
                    .into(),
                ),
                text_color: tokens.text,
                border: Border {
                    color: tokens.border,
                    width: 1.0,
                    radius: tokens.radius_pill.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}

fn load_more_button<'a>(tokens: Tokens, on_press: Message) -> Element<'a, Message> {
    secondary_button(tokens, "Load more", on_press)
}

async fn load_tracks(
    api: ApiClient,
    offset: u32,
    order: ItemOrder,
) -> Result<(Vec<Favorite<Track>>, bool), String> {
    let page = api
        .favorite_tracks(PAGE_SIZE, offset, Some(order), None)
        .await
        .map_err(fmt_err)?;
    let has_more = page.items.len() as u32 == PAGE_SIZE;
    Ok((page.items, has_more))
}

async fn load_albums(
    api: ApiClient,
    offset: u32,
    order: AlbumOrder,
) -> Result<(Vec<Favorite<Album>>, bool), String> {
    let page = api
        .favorite_albums(PAGE_SIZE, offset, Some(order), None)
        .await
        .map_err(fmt_err)?;
    let has_more = page.items.len() as u32 == PAGE_SIZE;
    Ok((page.items, has_more))
}

async fn load_artists(
    api: ApiClient,
    offset: u32,
    order: ArtistOrder,
) -> Result<(Vec<Favorite<ArtistProfile>>, bool), String> {
    let page = api
        .favorite_artists(PAGE_SIZE, offset, Some(order), None)
        .await
        .map_err(fmt_err)?;
    let has_more = page.items.len() as u32 == PAGE_SIZE;
    Ok((page.items, has_more))
}

async fn load_mixes(
    api: ApiClient,
    offset: u32,
    order: MixOrder,
) -> Result<(Vec<Favorite<MixSummary>>, bool), String> {
    let page = api
        .favorite_mixes(PAGE_SIZE, offset, Some(order), None)
        .await
        .map_err(fmt_err)?;
    let has_more = page.items.len() as u32 == PAGE_SIZE;
    Ok((page.items, has_more))
}

async fn load_playlists(api: ApiClient) -> Result<Vec<Playlist>, String> {
    api.playlists_and_favorite_playlists()
        .await
        .map_err(fmt_err)
}

async fn load_folders(api: ApiClient) -> Result<Vec<serde_json::Value>, String> {
    api.collection_folders_all("root", None, 200)
        .await
        .map_err(fmt_err)
}

async fn unfavorite_album(api: ApiClient, id: u64) -> Result<(), String> {
    api.unfavorite_album(id).await.map_err(fmt_err)
}

async fn unfollow_artist(api: ApiClient, id: u64) -> Result<(), String> {
    api.unfollow_artist(id).await.map_err(fmt_err)
}

async fn unfavorite_mix(api: ApiClient, id: String) -> Result<(), String> {
    api.favorite_mixes_remove(&[&id]).await.map_err(fmt_err)
}

async fn create_playlist(api: ApiClient, name: String, description: String) -> Result<(), String> {
    let description = (!description.trim().is_empty()).then(|| description.clone());
    api.create_playlist(&name, description.as_deref(), None)
        .await
        .map(|_| ())
        .map_err(fmt_err)
}

fn fmt_err(e: Error) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_test::simulator;

    #[test]
    fn default_tab_is_tracks_and_renders_the_tab_bar() {
        let state = State::default();
        let tokens = Tokens::dark();
        let images = crate::ui::images::ImageCache::new(4);
        let mut ui = simulator(state.view(tokens, &images));
        assert!(ui.find("Albums").is_ok());
        assert!(ui.find("Playlists").is_ok());
        assert!(ui.find("Mixes & Radio").is_ok());
    }

    #[test]
    fn tab_selection_switches_the_active_tab() {
        let mut state = State::default();
        let (_, effects) = state.update(Message::TabSelected(Tab::Albums), &test_api());
        assert!(effects.is_empty());
        assert_eq!(state.tab, Tab::Albums);
    }

    #[test]
    fn unfavoriting_a_track_removes_it_from_the_list() {
        let mut state = State {
            tracks: vec![Favorite {
                created: None,
                item: Track {
                    id: 1,
                    title: "A".into(),
                    ..Track::default()
                },
            }],
            ..State::default()
        };
        let (_, effects) = state.update(Message::TrackFavoriteToggled(Ok((1, false))), &test_api());
        assert!(effects.is_empty());
        assert!(state.tracks.is_empty());
    }

    #[test]
    fn create_playlist_dialog_opens_and_closes() {
        let mut state = State::default();
        let _ = state.update(Message::OpenCreatePlaylist, &test_api());
        assert!(state.create_playlist.is_some());
        let _ = state.update(Message::CancelCreatePlaylist, &test_api());
        assert!(state.create_playlist.is_none());
    }

    fn test_api() -> ApiClient {
        use std::sync::Arc;
        use streamboat_core::ClientCredentials;
        use streamboat_core::token_store::MemoryTokenStore;
        ApiClient::builder(
            ClientCredentials::new("cid", None),
            Arc::new(MemoryTokenStore::default()),
        )
        .build()
        .unwrap()
    }
}
