//! The Artist entity page (task item 1): picture header, follow toggle
//! (D-039 — "follow" is a documented alias over favouriting the artist),
//! top tracks, Albums / EPs & Singles / Compilations via the documented
//! `artists/{id}/albums?filter=` values, similar artists, bio, and "play
//! artist mix" when `artists/{id}/mix` resolves an id
//! (`tidal-client-features` library-playlists-collections.md §3).

use std::collections::HashSet;

use iced::widget::{button, column, container, image, row, scrollable, text};
use iced::{Alignment, Border, Element, Length, Task, Theme};

use streamboat_core::models::{Album, ArtistAlbumFilter, ArtistProfile, Track};
use streamboat_core::proto::QueuePosition;
use streamboat_core::{ApiClient, Error};

use crate::ui::actions::{self, TrackAction};
use crate::ui::design::Tokens;
use crate::ui::format::{cover_url, mmss};
use crate::ui::images::ImageCache;
use crate::ui::nav::EntityRef;
use crate::ui::widgets::{ChipTone, banner, card};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumTab {
    Albums,
    EpsAndSingles,
    Compilations,
}

impl AlbumTab {
    const ALL: [AlbumTab; 3] = [
        AlbumTab::Albums,
        AlbumTab::EpsAndSingles,
        AlbumTab::Compilations,
    ];

    fn label(self) -> &'static str {
        match self {
            AlbumTab::Albums => "Albums",
            AlbumTab::EpsAndSingles => "EPs & Singles",
            AlbumTab::Compilations => "Compilations",
        }
    }
}

pub struct State {
    artist_id: u64,
    artist: Option<ArtistProfile>,
    top_tracks: Vec<Track>,
    albums: Vec<Album>,
    eps_singles: Vec<Album>,
    compilations: Vec<Album>,
    similar: Vec<ArtistProfile>,
    bio: Option<streamboat_core::models::TextWithSource>,
    mix_id: Option<String>,
    is_following: bool,
    favorite_tracks: HashSet<u64>,
    tab: AlbumTab,
    loading: bool,
    error: Option<String>,
}

impl State {
    pub fn new(artist_id: u64) -> Self {
        Self {
            artist_id,
            artist: None,
            top_tracks: Vec::new(),
            albums: Vec::new(),
            eps_singles: Vec::new(),
            compilations: Vec::new(),
            similar: Vec::new(),
            bio: None,
            mix_id: None,
            is_following: false,
            favorite_tracks: HashSet::new(),
            tab: AlbumTab::Albums,
            loading: true,
            error: None,
        }
    }

    pub fn artist_id(&self) -> u64 {
        self.artist_id
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    ArtistLoaded(Result<ArtistProfile, String>),
    TopTracksLoaded(Result<Vec<Track>, String>),
    AlbumsLoaded(AlbumTab, Result<Vec<Album>, String>),
    SimilarLoaded(Result<Vec<ArtistProfile>, String>),
    BioLoaded(Result<Option<streamboat_core::models::TextWithSource>, String>),
    MixIdLoaded(Result<Option<String>, String>),
    FavoritesLoaded(Result<(bool, HashSet<u64>), String>),
    ToggleFollow,
    FollowToggled(Result<bool, String>),
    TrackFavoriteToggled(Result<(u64, bool), String>),
    TabSelected(AlbumTab),
    PlayArtistMix,
    ArtistMixLoaded(Result<Vec<u64>, String>),
    Track(TrackAction),
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
    pub fn load(api: &ApiClient, artist_id: u64) -> Task<Message> {
        Task::batch([
            Task::perform(load_artist(api.clone(), artist_id), Message::ArtistLoaded),
            Task::perform(
                load_top_tracks(api.clone(), artist_id),
                Message::TopTracksLoaded,
            ),
            Task::perform(load_albums(api.clone(), artist_id, None), |r| {
                Message::AlbumsLoaded(AlbumTab::Albums, r)
            }),
            Task::perform(
                load_albums(
                    api.clone(),
                    artist_id,
                    Some(ArtistAlbumFilter::EpsAndSingles),
                ),
                |r| Message::AlbumsLoaded(AlbumTab::EpsAndSingles, r),
            ),
            Task::perform(
                load_albums(
                    api.clone(),
                    artist_id,
                    Some(ArtistAlbumFilter::Compilations),
                ),
                |r| Message::AlbumsLoaded(AlbumTab::Compilations, r),
            ),
            Task::perform(load_similar(api.clone(), artist_id), Message::SimilarLoaded),
            Task::perform(load_bio(api.clone(), artist_id), Message::BioLoaded),
            Task::perform(load_mix_id(api.clone(), artist_id), Message::MixIdLoaded),
            Task::perform(
                load_favorites(api.clone(), artist_id),
                Message::FavoritesLoaded,
            ),
        ])
    }

    pub fn update(&mut self, message: Message, api: &ApiClient) -> (Task<Message>, Vec<Effect>) {
        match message {
            Message::ArtistLoaded(Ok(artist)) => {
                let images = artist.picture.clone().into_iter().collect();
                self.artist = Some(artist);
                self.loading = false;
                self.error = None;
                (Task::none(), vec![Effect::ImagesNeeded(images)])
            }
            Message::ArtistLoaded(Err(e)) => {
                self.loading = false;
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::TopTracksLoaded(Ok(tracks)) => {
                self.top_tracks = tracks;
                (Task::none(), Vec::new())
            }
            Message::TopTracksLoaded(Err(_)) => (Task::none(), Vec::new()),
            Message::AlbumsLoaded(tab, Ok(albums)) => {
                let images = albums.iter().filter_map(|a| a.cover.clone()).collect();
                match tab {
                    AlbumTab::Albums => self.albums = albums,
                    AlbumTab::EpsAndSingles => self.eps_singles = albums,
                    AlbumTab::Compilations => self.compilations = albums,
                }
                (Task::none(), vec![Effect::ImagesNeeded(images)])
            }
            Message::AlbumsLoaded(_, Err(_)) => (Task::none(), Vec::new()),
            Message::SimilarLoaded(Ok(similar)) => {
                let images = similar.iter().filter_map(|a| a.picture.clone()).collect();
                self.similar = similar;
                (Task::none(), vec![Effect::ImagesNeeded(images)])
            }
            Message::SimilarLoaded(Err(_)) => (Task::none(), Vec::new()),
            Message::BioLoaded(Ok(bio)) => {
                self.bio = bio;
                (Task::none(), Vec::new())
            }
            Message::BioLoaded(Err(_)) => (Task::none(), Vec::new()),
            Message::MixIdLoaded(Ok(id)) => {
                self.mix_id = id;
                (Task::none(), Vec::new())
            }
            Message::MixIdLoaded(Err(_)) => (Task::none(), Vec::new()),
            Message::FavoritesLoaded(Ok((following, tracks))) => {
                self.is_following = following;
                self.favorite_tracks = tracks;
                (Task::none(), Vec::new())
            }
            Message::FavoritesLoaded(Err(_)) => (Task::none(), Vec::new()),
            Message::ToggleFollow => {
                let api = api.clone();
                let id = self.artist_id;
                let was = self.is_following;
                (
                    Task::perform(toggle_follow(api, id, was), Message::FollowToggled),
                    Vec::new(),
                )
            }
            Message::FollowToggled(Ok(now)) => {
                self.is_following = now;
                (Task::none(), Vec::new())
            }
            Message::FollowToggled(Err(e)) => {
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::TrackFavoriteToggled(Ok((id, now))) => {
                if now {
                    self.favorite_tracks.insert(id);
                } else {
                    self.favorite_tracks.remove(&id);
                }
                (Task::none(), Vec::new())
            }
            Message::TrackFavoriteToggled(Err(e)) => {
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::TabSelected(tab) => {
                self.tab = tab;
                (Task::none(), Vec::new())
            }
            Message::PlayArtistMix => {
                let Some(mix_id) = self.mix_id.clone() else {
                    return (Task::none(), Vec::new());
                };
                (
                    Task::perform(
                        load_mix_track_ids(api.clone(), mix_id),
                        Message::ArtistMixLoaded,
                    ),
                    Vec::new(),
                )
            }
            Message::ArtistMixLoaded(Ok(ids)) => (Task::none(), vec![Effect::PlayTracks(ids)]),
            Message::ArtistMixLoaded(Err(e)) => {
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::Track(action) => self.handle_track_action(action, api),
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
            TrackAction::ToggleFavorite(id) => {
                let was = self.favorite_tracks.contains(&id);
                let api = api.clone();
                (
                    Task::perform(
                        actions::toggle_track_favorite(api, id, was),
                        Message::TrackFavoriteToggled,
                    ),
                    Vec::new(),
                )
            }
            TrackAction::AddToPlaylist(id) => (Task::none(), vec![Effect::OpenAddToPlaylist(id)]),
        }
    }

    pub fn view<'a>(&'a self, tokens: Tokens, images: &ImageCache) -> Element<'a, Message> {
        if self.loading {
            return crate::ui::widgets::page(
                tokens,
                text("Loading…").size(tokens.text_md).color(tokens.muted),
            );
        }
        let Some(artist) = &self.artist else {
            return crate::ui::widgets::page(
                tokens,
                column![
                    text("Artist not found")
                        .size(tokens.text_lg)
                        .color(tokens.text),
                    error_banner(tokens, self),
                ]
                .spacing(tokens.space_sm),
            );
        };

        let art: Element<'a, Message> = match artist
            .picture
            .as_deref()
            .and_then(cover_url)
            .and_then(|u| images.peek(&u))
        {
            Some(handle) => image(handle).width(160.0).height(160.0).into(),
            None => container(text("")).width(160.0).height(160.0).into(),
        };

        let mut header_actions = row![follow_toggle(
            tokens,
            self.is_following,
            Message::ToggleFollow
        )]
        .spacing(tokens.space_sm);
        if self.mix_id.is_some() {
            header_actions = header_actions.push(secondary_button(
                tokens,
                "Play artist mix",
                Message::PlayArtistMix,
            ));
        }

        let header = row![
            art,
            column![
                text(artist.name.clone())
                    .size(tokens.text_xl)
                    .color(tokens.text),
                header_actions,
            ]
            .spacing(tokens.space_sm),
        ]
        .spacing(tokens.space_lg)
        .align_y(Alignment::Center);

        let mut body = column![header, error_banner(tokens, self)].spacing(tokens.space_lg);

        if !self.top_tracks.is_empty() {
            body = body.push(top_tracks_section(tokens, self));
        }

        body = body.push(albums_section(tokens, self, images));

        if !self.similar.is_empty() {
            body = body.push(similar_section(tokens, self, images));
        }

        if let Some(bio) = &self.bio {
            if let Some(text_body) = &bio.text {
                let source = bio
                    .source
                    .as_deref()
                    .map(|s| format!(" — {s}"))
                    .unwrap_or_default();
                body = body.push(
                    column![
                        text(format!("Biography{source}"))
                            .size(tokens.text_md)
                            .color(tokens.text),
                        text(text_body.clone())
                            .size(tokens.text_sm)
                            .color(tokens.muted),
                    ]
                    .spacing(tokens.space_xs),
                );
            }
        }

        crate::ui::widgets::page(tokens, scrollable(body).height(Length::Fill))
    }
}

fn error_banner<'a>(tokens: Tokens, state: &State) -> Element<'a, Message> {
    match &state.error {
        Some(e) => banner(tokens, e.clone(), ChipTone::Danger),
        None => column![].into(),
    }
}

fn top_tracks_section<'a>(tokens: Tokens, state: &'a State) -> Element<'a, Message> {
    let mut col = column![text("Top tracks").size(tokens.text_md).color(tokens.text)]
        .spacing(tokens.space_xs);
    for t in state.top_tracks.iter().take(10) {
        let is_favorite = state.favorite_tracks.contains(&t.id);
        let album = t.album.as_ref().and_then(|a| a.id).map(EntityRef::Album);
        let row_content = row![
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
                is_favorite,
                album,
                None,
                Message::Track,
                Message::Navigate,
            ),
        ]
        .spacing(tokens.space_sm)
        .align_y(Alignment::Center)
        .width(Length::Fill);
        col = col.push(
            container(row_content)
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
    col.into()
}

fn albums_section<'a>(
    tokens: Tokens,
    state: &'a State,
    images: &ImageCache,
) -> Element<'a, Message> {
    let mut tabs = row![].spacing(tokens.space_xs);
    for tab in AlbumTab::ALL {
        let active = tab == state.tab;
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

    let albums = match state.tab {
        AlbumTab::Albums => &state.albums,
        AlbumTab::EpsAndSingles => &state.eps_singles,
        AlbumTab::Compilations => &state.compilations,
    };
    let grid = album_grid(tokens, albums, images);
    column![tabs, grid].spacing(tokens.space_sm).into()
}

fn album_grid<'a>(
    tokens: Tokens,
    albums: &'a [Album],
    images: &ImageCache,
) -> Element<'a, Message> {
    if albums.is_empty() {
        return text("Nothing here yet.")
            .size(tokens.text_sm)
            .color(tokens.muted)
            .into();
    }
    let mut r = row![].spacing(tokens.space_md);
    for a in albums.iter().take(20) {
        let art: Element<'a, Message> = match a
            .cover
            .as_deref()
            .and_then(cover_url)
            .and_then(|u| images.peek(&u))
        {
            Some(handle) => image(handle).width(140.0).height(140.0).into(),
            None => container(text("")).width(140.0).height(140.0).into(),
        };
        let body = column![
            art,
            text(a.title.clone())
                .size(tokens.text_sm)
                .color(tokens.text)
        ]
        .spacing(tokens.space_xs);
        r = r.push(card(
            tokens,
            140.0,
            body,
            Some(Message::Navigate(EntityRef::Album(a.id))),
        ));
    }
    scrollable(r)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default(),
        ))
        .into()
}

fn similar_section<'a>(
    tokens: Tokens,
    state: &'a State,
    images: &ImageCache,
) -> Element<'a, Message> {
    let mut r = row![].spacing(tokens.space_md);
    for a in state.similar.iter().take(10) {
        let art: Element<'a, Message> = match a
            .picture
            .as_deref()
            .and_then(cover_url)
            .and_then(|u| images.peek(&u))
        {
            Some(handle) => image(handle).width(120.0).height(120.0).into(),
            None => container(text("")).width(120.0).height(120.0).into(),
        };
        let body = column![
            art,
            text(a.name.clone()).size(tokens.text_sm).color(tokens.text)
        ]
        .spacing(tokens.space_xs);
        r = r.push(card(
            tokens,
            120.0,
            body,
            Some(Message::Navigate(EntityRef::Artist(a.id))),
        ));
    }
    column![
        text("Similar artists")
            .size(tokens.text_md)
            .color(tokens.text),
        scrollable(r).direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default()
        )),
    ]
    .spacing(tokens.space_sm)
    .into()
}

fn follow_toggle<'a>(
    tokens: Tokens,
    is_following: bool,
    on_press: Message,
) -> Element<'a, Message> {
    button(text(if is_following { "Following" } else { "Follow" }).size(tokens.text_sm))
        .padding([tokens.space_xs, tokens.space_md])
        .on_press(on_press)
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, button::Status::Hovered);
            let background = if is_following {
                if hovered {
                    tokens.elevated
                } else {
                    tokens.surface
                }
            } else if hovered {
                tokens.accent_hover()
            } else {
                tokens.accent
            };
            button::Style {
                background: Some(background.into()),
                text_color: if is_following {
                    tokens.text
                } else {
                    tokens.background
                },
                border: Border {
                    color: tokens.border,
                    width: if is_following { 1.0 } else { 0.0 },
                    radius: tokens.radius_pill.into(),
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

async fn load_artist(api: ApiClient, id: u64) -> Result<ArtistProfile, String> {
    api.artist(id).await.map_err(fmt_err)
}

async fn load_top_tracks(api: ApiClient, id: u64) -> Result<Vec<Track>, String> {
    api.artist_top_tracks(id, 20, 0)
        .await
        .map(|p| p.items)
        .map_err(fmt_err)
}

async fn load_albums(
    api: ApiClient,
    id: u64,
    filter: Option<ArtistAlbumFilter>,
) -> Result<Vec<Album>, String> {
    api.artist_albums(id, filter, 50, 0)
        .await
        .map(|p| p.items)
        .map_err(fmt_err)
}

async fn load_similar(api: ApiClient, id: u64) -> Result<Vec<ArtistProfile>, String> {
    api.artist_similar(id).await.map_err(fmt_err)
}

async fn load_bio(
    api: ApiClient,
    id: u64,
) -> Result<Option<streamboat_core::models::TextWithSource>, String> {
    api.artist_bio(id).await.map_err(fmt_err)
}

async fn load_mix_id(api: ApiClient, id: u64) -> Result<Option<String>, String> {
    api.artist_mix_id(id).await.map_err(fmt_err)
}

async fn load_favorites(api: ApiClient, artist_id: u64) -> Result<(bool, HashSet<u64>), String> {
    let ids = api.favorite_ids().await.map_err(fmt_err)?;
    let is_following = ids.artist.iter().any(|a| a == &artist_id.to_string());
    let tracks = ids
        .track
        .into_iter()
        .filter_map(|s| s.parse().ok())
        .collect();
    Ok((is_following, tracks))
}

async fn toggle_follow(api: ApiClient, id: u64, was_following: bool) -> Result<bool, String> {
    let result = if was_following {
        api.unfollow_artist(id).await
    } else {
        api.follow_artist(id).await
    };
    result.map(|()| !was_following).map_err(fmt_err)
}

async fn load_mix_track_ids(api: ApiClient, mix_id: String) -> Result<Vec<u64>, String> {
    let items = api.mix_items(&mix_id, 100, 0).await.map_err(fmt_err)?;
    Ok(items
        .items
        .iter()
        .filter_map(|i| i.as_track())
        .map(|t| t.id)
        .collect())
}

fn fmt_err(e: Error) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_test::simulator;

    fn synthetic_artist() -> ArtistProfile {
        ArtistProfile {
            id: 1,
            name: "Synthetic Artist".into(),
            ..ArtistProfile::default()
        }
    }

    #[test]
    fn renders_header_top_tracks_and_album_tabs() {
        let mut state = State::new(1);
        state.loading = false;
        state.artist = Some(synthetic_artist());
        state.top_tracks = vec![Track {
            id: 5,
            title: "Top Track".into(),
            ..Track::default()
        }];
        state.albums = vec![Album {
            id: 2,
            title: "Main Album".into(),
            ..Album::default()
        }];
        let tokens = Tokens::dark();
        let images = ImageCache::new(4);
        let mut ui = simulator(state.view(tokens, &images));
        assert!(ui.find("Synthetic Artist").is_ok());
        assert!(ui.find("Top Track").is_ok());
        assert!(ui.find("Main Album").is_ok());
        assert!(ui.find("EPs & Singles").is_ok());
        assert!(ui.find("Follow").is_ok());
    }

    #[test]
    fn following_state_shows_the_following_label() {
        let mut state = State::new(1);
        state.loading = false;
        state.artist = Some(synthetic_artist());
        state.is_following = true;
        let tokens = Tokens::dark();
        let images = ImageCache::new(4);
        let mut ui = simulator(state.view(tokens, &images));
        assert!(ui.find("Following").is_ok());
    }

    #[test]
    fn tab_selection_switches_the_shown_album_list() {
        let mut state = State::new(1);
        let (_, effects) = state.update(Message::TabSelected(AlbumTab::Compilations), &test_api());
        assert!(effects.is_empty());
        assert_eq!(state.tab, AlbumTab::Compilations);
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
