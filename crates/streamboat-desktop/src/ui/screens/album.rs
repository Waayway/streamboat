//! The Album entity page: art, title, artists, year, track
//! count and duration, explicit/quality badges, a track list with per-row
//! actions, play-all/shuffle, a favourite toggle, a similar-albums row, and
//! the editorial review text when TIDAL returns one
//! (`tidal-client-features` library-playlists-collections.md §4).
//!
//! [`view_with_highlight`] is what [`crate::ui::screens::track`] reuses to
//! show a standalone track's album with that one row highlighted — the
//! mechanism behind "Track opens its album with the track highlighted."

use std::collections::HashSet;

use iced::widget::{button, column, container, image, row, scrollable, text};
use iced::{Alignment, Border, Element, Length, Task, Theme};

use streamboat_core::api::pagination;
use streamboat_core::models::{Album, Track};
use streamboat_core::proto::QueuePosition;
use streamboat_core::{ApiClient, Error};

use crate::ui::actions::{self, TrackAction};
use crate::ui::design::Tokens;
use crate::ui::format::{cover_url, mmss, quality_badge};
use crate::ui::images::ImageCache;
use crate::ui::nav::EntityRef;
use crate::ui::widgets::{ChipTone, banner, card, chip};

/// A pathologically large box set still needs a bound; ordinary albums
/// (even multi-volume ones) finish in the first page or two.
const MAX_TRACKS: usize = 500;

pub struct State {
    album_id: u64,
    album: Option<Album>,
    tracks: Vec<Track>,
    review: Option<String>,
    similar: Vec<Album>,
    favorite_tracks: HashSet<u64>,
    is_favorite: bool,
    loading: bool,
    error: Option<String>,
}

impl State {
    pub fn new(album_id: u64) -> Self {
        Self {
            album_id,
            album: None,
            tracks: Vec::new(),
            review: None,
            similar: Vec::new(),
            favorite_tracks: HashSet::new(),
            is_favorite: false,
            loading: true,
            error: None,
        }
    }

    pub fn album_id(&self) -> u64 {
        self.album_id
    }

    /// Every track id, in order — [`crate::ui::screens::track`] uses this to
    /// build the album's play-all list when it wants to play from a
    /// specific highlighted track onward.
    pub fn track_ids(&self) -> Vec<u64> {
        self.tracks.iter().map(|t| t.id).collect()
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Boxed: `Album` is by far this enum's largest payload, and boxing it
    /// keeps every other `Message` variant from paying that size on the
    /// stack (same reasoning as `ui::app::Message::PlayerEvent`).
    AlbumLoaded(Result<Box<Album>, String>),
    TracksLoaded(Result<Vec<Track>, String>),
    ReviewLoaded(Result<Option<String>, String>),
    SimilarLoaded(Result<Vec<Album>, String>),
    FavoritesLoaded(Result<(bool, HashSet<u64>), String>),
    ToggleAlbumFavorite,
    AlbumFavoriteToggled(Result<bool, String>),
    TrackFavoriteToggled(Result<(u64, bool), String>),
    PlayAll,
    Shuffle,
    Track(TrackAction),
    Navigate(EntityRef),
}

/// What `ui::app` must do beyond this screen's own state: send a play/queue
/// command through the player link, navigate, prefetch artwork, or open the
/// shared "add to playlist" picker.
pub enum Effect {
    PlayTracks(Vec<u64>),
    Enqueue(u64, QueuePosition),
    Navigate(EntityRef),
    ImagesNeeded(Vec<String>),
    OpenAddToPlaylist(u64),
}

impl State {
    pub fn load(api: &ApiClient, album_id: u64) -> Task<Message> {
        Task::batch([
            Task::perform(load_album(api.clone(), album_id), Message::AlbumLoaded),
            Task::perform(load_tracks(api.clone(), album_id), Message::TracksLoaded),
            Task::perform(load_review(api.clone(), album_id), Message::ReviewLoaded),
            Task::perform(load_similar(api.clone(), album_id), Message::SimilarLoaded),
            Task::perform(
                load_favorites(api.clone(), album_id),
                Message::FavoritesLoaded,
            ),
        ])
    }

    pub fn update(&mut self, message: Message, api: &ApiClient) -> (Task<Message>, Vec<Effect>) {
        match message {
            Message::AlbumLoaded(Ok(album)) => {
                let images = album.cover.clone().into_iter().collect();
                self.album = Some(*album);
                self.loading = false;
                self.error = None;
                (Task::none(), vec![Effect::ImagesNeeded(images)])
            }
            Message::AlbumLoaded(Err(e)) => {
                self.loading = false;
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::TracksLoaded(Ok(tracks)) => {
                self.tracks = tracks;
                (Task::none(), Vec::new())
            }
            Message::TracksLoaded(Err(e)) => {
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::ReviewLoaded(Ok(review)) => {
                self.review = review;
                (Task::none(), Vec::new())
            }
            Message::ReviewLoaded(Err(_)) => (Task::none(), Vec::new()),
            Message::SimilarLoaded(Ok(similar)) => {
                let images = similar.iter().filter_map(|a| a.cover.clone()).collect();
                self.similar = similar;
                (Task::none(), vec![Effect::ImagesNeeded(images)])
            }
            Message::SimilarLoaded(Err(_)) => (Task::none(), Vec::new()),
            Message::FavoritesLoaded(Ok((is_favorite, tracks))) => {
                self.is_favorite = is_favorite;
                self.favorite_tracks = tracks;
                (Task::none(), Vec::new())
            }
            Message::FavoritesLoaded(Err(_)) => (Task::none(), Vec::new()),
            Message::ToggleAlbumFavorite => {
                let api = api.clone();
                let id = self.album_id;
                let was = self.is_favorite;
                (
                    Task::perform(
                        toggle_album_favorite(api, id, was),
                        Message::AlbumFavoriteToggled,
                    ),
                    Vec::new(),
                )
            }
            Message::AlbumFavoriteToggled(Ok(now)) => {
                self.is_favorite = now;
                (Task::none(), Vec::new())
            }
            Message::AlbumFavoriteToggled(Err(e)) => {
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
            Message::PlayAll => (Task::none(), vec![Effect::PlayTracks(self.track_ids())]),
            Message::Shuffle => (
                Task::none(),
                vec![Effect::PlayTracks(actions::shuffled(self.track_ids()))],
            ),
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
        view_with_highlight(tokens, self, images, None)
    }
}

/// Shared with [`crate::ui::screens::track`]: the same album page, with
/// `highlight` (when `Some`) picking out one row.
pub fn view_with_highlight<'a>(
    tokens: Tokens,
    state: &'a State,
    images: &ImageCache,
    highlight: Option<u64>,
) -> Element<'a, Message> {
    crate::ui::widgets::page(
        tokens,
        scrollable(content_with_highlight(tokens, state, images, highlight)).height(Length::Fill),
    )
}

/// The album page's content, without the outer page/scrollable wrap —
/// [`crate::ui::screens::track`] uses this directly so it can append a
/// credits panel inside the same scrollable rather than nesting one page
/// inside another.
pub fn content_with_highlight<'a>(
    tokens: Tokens,
    state: &'a State,
    images: &ImageCache,
    highlight: Option<u64>,
) -> Element<'a, Message> {
    if state.loading {
        return text("Loading…")
            .size(tokens.text_md)
            .color(tokens.muted)
            .into();
    }
    let Some(album) = &state.album else {
        return column![
            text("Album not found")
                .size(tokens.text_lg)
                .color(tokens.text),
            error_banner(tokens, state),
        ]
        .spacing(tokens.space_sm)
        .into();
    };

    let art: Element<'a, Message> = match album
        .cover
        .as_deref()
        .and_then(cover_url)
        .and_then(|u| images.peek(&u))
    {
        Some(handle) => container(image(handle).width(220.0).height(220.0))
            .style(move |_theme: &Theme| art_style(tokens))
            .into(),
        None => container(text(""))
            .width(220.0)
            .height(220.0)
            .style(move |_theme: &Theme| art_style(tokens))
            .into(),
    };

    let year = album
        .release_date
        .as_deref()
        .and_then(|d| d.split('-').next())
        .unwrap_or("");
    let total_secs: u32 = state.tracks.iter().filter_map(|t| t.duration).sum();
    let track_count = album.number_of_tracks.unwrap_or(state.tracks.len() as u32);

    let mut artist_row = row![].spacing(tokens.space_xs);
    for a in &album.artists {
        let label = text(a.name.clone())
            .size(tokens.text_lg)
            .color(tokens.muted);
        match a.id {
            Some(id) => {
                artist_row = artist_row.push(
                    button(label)
                        .style(button::text)
                        .on_press(Message::Navigate(EntityRef::Artist(id))),
                )
            }
            None => artist_row = artist_row.push(label),
        }
    }

    let mut badges = row![].spacing(tokens.space_xs);
    if let Some(q) = album.audio_quality {
        badges = badges.push(chip(tokens, quality_badge(Some(q)), ChipTone::Accent));
    }
    if album.explicit == Some(true) {
        badges = badges.push(chip(tokens, "EXPLICIT", ChipTone::Warning));
    }

    let meta = text(format!(
        "{year}{sep}{track_count} tracks{sep2}{duration}",
        sep = if year.is_empty() { "" } else { " · " },
        track_count = track_count,
        sep2 = " · ",
        duration = mmss(Some(u64::from(total_secs) * 1000)),
    ))
    .size(tokens.text_sm)
    .color(tokens.muted);

    let actions_row = row![
        primary_button(tokens, "Play all", Message::PlayAll),
        secondary_button(tokens, "Shuffle", Message::Shuffle),
        favorite_toggle(tokens, state.is_favorite, Message::ToggleAlbumFavorite),
    ]
    .spacing(tokens.space_sm);

    let header = row![
        art,
        column![
            text(album.title.clone())
                .size(tokens.text_xl)
                .color(tokens.text),
            artist_row,
            meta,
            badges,
            actions_row,
        ]
        .spacing(tokens.space_sm)
        .width(Length::Fill),
    ]
    .spacing(tokens.space_lg);

    let mut body = column![header].spacing(tokens.space_lg);
    body = body.push(error_banner(tokens, state));
    body = body.push(track_list(tokens, state, highlight));

    if let Some(review) = &state.review {
        body = body.push(
            column![
                text("Review").size(tokens.text_md).color(tokens.text),
                text(review.clone())
                    .size(tokens.text_sm)
                    .color(tokens.muted),
            ]
            .spacing(tokens.space_xs),
        );
    }

    if !state.similar.is_empty() {
        body = body.push(similar_row(tokens, &state.similar, images));
    }

    body.into()
}

fn error_banner<'a>(tokens: Tokens, state: &State) -> Element<'a, Message> {
    match &state.error {
        Some(e) => banner(tokens, e.clone(), ChipTone::Danger),
        None => column![].into(),
    }
}

fn track_list<'a>(
    tokens: Tokens,
    state: &'a State,
    highlight: Option<u64>,
) -> Element<'a, Message> {
    if state.tracks.is_empty() {
        return text("No tracks.")
            .size(tokens.text_sm)
            .color(tokens.muted)
            .into();
    }
    let mut col = column![].spacing(tokens.space_xs);
    for t in &state.tracks {
        col = col.push(track_row(tokens, state, t, highlight == Some(t.id)));
    }
    col.into()
}

fn track_row<'a>(
    tokens: Tokens,
    state: &'a State,
    track: &'a Track,
    highlighted: bool,
) -> Element<'a, Message> {
    let number = track
        .track_number
        .map(|n| n.to_string())
        .unwrap_or_default();
    let label = column![
        text(track.title.clone())
            .size(tokens.text_sm)
            .color(tokens.text),
        text(track.artist_names())
            .size(tokens.text_xs)
            .color(tokens.muted),
    ]
    .spacing(2)
    .width(Length::Fill);

    let is_favorite = state.favorite_tracks.contains(&track.id);
    let artist = track
        .artists
        .first()
        .or(track.artist.as_ref())
        .and_then(|a| a.id)
        .map(EntityRef::Artist);

    let content = row![
        text(number)
            .size(tokens.text_sm)
            .color(tokens.muted)
            .width(Length::Fixed(28.0)),
        label,
        text(mmss(track.duration.map(|d| u64::from(d) * 1000)))
            .size(tokens.text_sm)
            .color(tokens.muted),
        actions::track_action_row(
            tokens,
            track.id,
            is_favorite,
            None,
            artist,
            Message::Track,
            Message::Navigate,
        ),
    ]
    .spacing(tokens.space_sm)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    container(content)
        .padding(tokens.space_sm)
        .width(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: highlighted.then_some(tokens.accent_muted.into()),
            border: Border {
                radius: tokens.radius_sm.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn similar_row<'a>(
    tokens: Tokens,
    similar: &'a [Album],
    images: &ImageCache,
) -> Element<'a, Message> {
    let mut r = row![].spacing(tokens.space_md);
    for a in similar.iter().take(10) {
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
                .color(tokens.text),
        ]
        .spacing(tokens.space_xs);
        r = r.push(card(
            tokens,
            140.0,
            body,
            Some(Message::Navigate(EntityRef::Album(a.id))),
        ));
    }
    column![
        text("Similar albums")
            .size(tokens.text_md)
            .color(tokens.text),
        scrollable(r).direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default()
        )),
    ]
    .spacing(tokens.space_sm)
    .into()
}

fn primary_button<'a>(tokens: Tokens, label: &'a str, on_press: Message) -> Element<'a, Message> {
    button(text(label).size(tokens.text_sm))
        .padding([tokens.space_xs, tokens.space_md])
        .on_press(on_press)
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(
                    (if hovered {
                        tokens.accent_hover()
                    } else {
                        tokens.accent
                    })
                    .into(),
                ),
                text_color: tokens.background,
                border: Border {
                    radius: tokens.radius_pill.into(),
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

fn favorite_toggle<'a>(
    tokens: Tokens,
    is_favorite: bool,
    on_press: Message,
) -> Element<'a, Message> {
    button(
        text(if is_favorite {
            "♥ Favourited"
        } else {
            "♡ Favourite"
        })
        .size(tokens.text_sm),
    )
    .padding([tokens.space_xs, tokens.space_md])
    .on_press(on_press)
    .style(move |_theme: &Theme, status| {
        let hovered = matches!(status, button::Status::Hovered);
        button::Style {
            background: hovered.then_some(tokens.elevated.into()),
            text_color: if is_favorite {
                tokens.accent
            } else {
                tokens.muted
            },
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

fn art_style(tokens: Tokens) -> container::Style {
    container::Style {
        background: Some(tokens.elevated.into()),
        border: Border {
            radius: tokens.radius_lg.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

async fn load_album(api: ApiClient, id: u64) -> Result<Box<Album>, String> {
    api.album(id).await.map(Box::new).map_err(fmt_err)
}

async fn load_tracks(api: ApiClient, id: u64) -> Result<Vec<Track>, String> {
    pagination::collect_all(
        pagination::DEFAULT_PAGE_SIZE,
        MAX_TRACKS,
        |offset, limit| api.album_tracks(id, limit, offset),
    )
    .await
    .map_err(fmt_err)
}

async fn load_review(api: ApiClient, id: u64) -> Result<Option<String>, String> {
    api.album_review(id).await.map_err(fmt_err)
}

async fn load_similar(api: ApiClient, id: u64) -> Result<Vec<Album>, String> {
    api.album_similar(id).await.map_err(fmt_err)
}

async fn load_favorites(api: ApiClient, album_id: u64) -> Result<(bool, HashSet<u64>), String> {
    let ids = api.favorite_ids().await.map_err(fmt_err)?;
    let is_favorite = ids.album.iter().any(|a| a == &album_id.to_string());
    let tracks = ids
        .track
        .into_iter()
        .filter_map(|s| s.parse().ok())
        .collect();
    Ok((is_favorite, tracks))
}

async fn toggle_album_favorite(
    api: ApiClient,
    id: u64,
    was_favorite: bool,
) -> Result<bool, String> {
    let result = if was_favorite {
        api.unfavorite_album(id).await
    } else {
        api.favorite_album(id).await
    };
    result.map(|()| !was_favorite).map_err(fmt_err)
}

fn fmt_err(e: Error) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_test::simulator;

    fn synthetic_album() -> Album {
        Album {
            id: 1,
            title: "Synthetic Album".into(),
            number_of_tracks: Some(2),
            release_date: Some("2024-05-01".into()),
            explicit: Some(true),
            audio_quality: Some(streamboat_core::AudioQuality::Lossless),
            artists: vec![streamboat_core::models::Artist {
                id: Some(9),
                name: "Synthetic Artist".into(),
            }],
            ..Album::default()
        }
    }

    fn synthetic_track(id: u64, title: &str) -> Track {
        Track {
            id,
            title: title.into(),
            duration: Some(200),
            track_number: Some(1),
            artists: vec![streamboat_core::models::Artist {
                id: Some(9),
                name: "Synthetic Artist".into(),
            }],
            ..Track::default()
        }
    }

    #[test]
    fn renders_album_header_and_tracks() {
        let mut state = State::new(1);
        state.loading = false;
        state.album = Some(synthetic_album());
        state.tracks = vec![
            synthetic_track(10, "Track One"),
            synthetic_track(11, "Track Two"),
        ];
        let tokens = Tokens::dark();
        let images = ImageCache::new(4);
        let mut ui = simulator(state.view(tokens, &images));
        assert!(ui.find("Synthetic Album").is_ok());
        assert!(ui.find("Synthetic Artist").is_ok());
        assert!(ui.find("Track One").is_ok());
        assert!(ui.find("EXPLICIT").is_ok());
        assert!(ui.find("Play all").is_ok());
    }

    #[test]
    fn highlighted_track_is_still_rendered_by_view_with_highlight() {
        let mut state = State::new(1);
        state.loading = false;
        state.album = Some(synthetic_album());
        state.tracks = vec![synthetic_track(10, "Track One")];
        let tokens = Tokens::dark();
        let images = ImageCache::new(4);
        let mut ui = simulator(view_with_highlight(tokens, &state, &images, Some(10)));
        assert!(ui.find("Track One").is_ok());
    }

    #[test]
    fn loading_state_shows_a_loading_note() {
        let state = State::new(1);
        let tokens = Tokens::dark();
        let images = ImageCache::new(4);
        let mut ui = simulator(state.view(tokens, &images));
        assert!(ui.find("Loading…").is_ok());
    }

    #[test]
    fn track_action_updates_favorite_set_on_success() {
        let mut state = State::new(1);
        state.tracks = vec![synthetic_track(10, "Track One")];
        let (_, effects) = state.update(Message::TrackFavoriteToggled(Ok((10, true))), &test_api());
        assert!(effects.is_empty());
        assert!(state.favorite_tracks.contains(&10));
    }

    #[test]
    fn play_all_effect_carries_every_track_in_order() {
        let mut state = State::new(1);
        state.tracks = vec![synthetic_track(10, "A"), synthetic_track(11, "B")];
        let (_, effects) = state.update(Message::PlayAll, &test_api());
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            Effect::PlayTracks(ids) => assert_eq!(ids, &vec![10, 11]),
            _ => panic!("expected PlayTracks"),
        }
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
