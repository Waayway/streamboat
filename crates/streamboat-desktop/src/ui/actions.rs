//! The shared per-track action row: play now, play next, add to end of
//! queue, favourite toggle, add to playlist, go to album/artist. Every
//! entity list (album tracks, artist top tracks, playlist items, mix
//! items, My Collection tracks) renders one row per track through
//! [`track_action_row`] so the actions — and their wiring — stay in
//! exactly one place. `wrap` is a plain function pointer (the same
//! "generic over each screen's own split `Message` type" idiom
//! `ui::widgets::feed_sections` already uses), so no boxed closures are
//! needed to embed this in five different `Message` enums.
//!
//! [`TrackAction::PlayNext`]/[`TrackAction::AddLast`] both map onto the
//! existing [`streamboat_core::proto::Command::Enqueue`] with
//! [`streamboat_core::proto::QueuePosition::Next`]/`Last` rather than a
//! new, additive `Command::PlayNext`, since `Enqueue{position: Next}`
//! already *is* that command; adding a second one would just be two names
//! for the same `Player` behaviour (`crates/streamboat-player/src/player.rs`'s
//! `Command::Enqueue` arm inserts at `index + 1` for `Next`).
//!
//! This module also owns the "add to playlist" picker
//! ([`PickerState`]/[`PickerMessage`]), a small modal `ui::app` renders as a
//! stacked overlay over whichever screen opened it — centralised here
//! (rather than duplicated per screen) because every screen that lists
//! tracks needs the exact same "fetch my playlists once, let the user pick
//! one, `POST` the track in" flow.

use std::collections::HashSet;

use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Border, Element, Length, Task, Theme};

use streamboat_core::api::playlists::{OnArtifactNotFound, OnDupes};
use streamboat_core::models::{FavoriteIds, Playlist};
use streamboat_core::{ApiClient, Error};

use crate::ui::design::Tokens;
use crate::ui::nav::EntityRef;

/// One action a track row can trigger. Screens translate the album/artist
/// variants straight into an `Effect::Navigate`, `PlayNow`/`PlayNext`/
/// `AddLast` into the matching `Command` through their own `Effect`
/// bubbling (the same pattern `screens::search::Effect` already uses), and
/// handle `ToggleFavorite` themselves via [`toggle_track_favorite`] since
/// that needs no cross-screen state. `AddToPlaylist` is the one variant
/// every screen forwards up to `ui::app`, which owns [`PickerState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackAction {
    PlayNow(u64),
    PlayNext(u64),
    AddLast(u64),
    ToggleFavorite(u64),
    AddToPlaylist(u64),
}

/// A track row's action buttons. `album`/`artist` are omitted (no button
/// rendered) when the surrounding screen already *is* that entity's page —
/// e.g. the Album screen's own track list has no "Go to album" button.
pub fn track_action_row<'a, Message: Clone + 'a>(
    tokens: Tokens,
    track_id: u64,
    is_favorite: bool,
    album: Option<EntityRef>,
    artist: Option<EntityRef>,
    wrap: fn(TrackAction) -> Message,
    navigate: fn(EntityRef) -> Message,
) -> Element<'a, Message> {
    let mut r = row![
        small_button(tokens, "Play", wrap(TrackAction::PlayNow(track_id))),
        small_button(tokens, "Next", wrap(TrackAction::PlayNext(track_id))),
        small_button(tokens, "Queue", wrap(TrackAction::AddLast(track_id))),
        favorite_button(
            tokens,
            is_favorite,
            wrap(TrackAction::ToggleFavorite(track_id))
        ),
        small_button(tokens, "+ List", wrap(TrackAction::AddToPlaylist(track_id)),),
    ]
    .spacing(tokens.space_xs);
    if let Some(a) = album {
        r = r.push(small_button(tokens, "Album", navigate(a)));
    }
    if let Some(a) = artist {
        r = r.push(small_button(tokens, "Artist", navigate(a)));
    }
    r.into()
}

fn small_button<'a, Message: Clone + 'a>(
    tokens: Tokens,
    label: &'static str,
    on_press: Message,
) -> Element<'a, Message> {
    button(text(label).size(tokens.text_xs))
        .padding([tokens.space_xs, tokens.space_sm])
        .on_press(on_press)
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
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

fn favorite_button<'a, Message: Clone + 'a>(
    tokens: Tokens,
    is_favorite: bool,
    on_press: Message,
) -> Element<'a, Message> {
    button(text(if is_favorite { "♥" } else { "♡" }).size(tokens.text_sm))
        .padding([tokens.space_xs, tokens.space_sm])
        .on_press(on_press)
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: hovered.then_some(tokens.elevated.into()),
                text_color: if is_favorite {
                    tokens.accent
                } else {
                    tokens.muted
                },
                border: Border {
                    radius: tokens.radius_sm.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }
        })
        .into()
}

// ---------------------------------------------------------------------------
// Favourite-track helpers shared by every screen that lists tracks.

/// `favorite_ids().track`, parsed into a `HashSet<u64>` — the cheap "is this
/// track favourited" lookup every entity/collection screen needs once per
/// load (`tidal-api` catalog-and-library.md §6: ids come back as strings
/// even for this integer-id type).
pub async fn load_favorite_track_ids(api: ApiClient) -> Result<HashSet<u64>, String> {
    let ids: FavoriteIds = api.favorite_ids().await.map_err(fmt_err)?;
    Ok(ids
        .track
        .into_iter()
        .filter_map(|s| s.parse().ok())
        .collect())
}

/// Flip one track's favourite state and report the new state back, so the
/// caller can update its local set optimistically-after-the-fact rather than
/// refetching the whole id list.
pub async fn toggle_track_favorite(
    api: ApiClient,
    track_id: u64,
    currently_favorite: bool,
) -> Result<(u64, bool), String> {
    let result = if currently_favorite {
        api.unfavorite_track(track_id).await
    } else {
        api.favorite_track(track_id).await
    };
    result
        .map(|()| (track_id, !currently_favorite))
        .map_err(fmt_err)
}

/// A stable-ish shuffle with no extra dependency: hash-sort by a fresh
/// per-call random seed (`std::collections::hash_map::RandomState` draws
/// from the OS, same as any `HashMap`'s own randomisation) rather than
/// pulling in the `rand` crate for one button.
pub fn shuffled(mut ids: Vec<u64>) -> Vec<u64> {
    use std::collections::hash_map::RandomState;
    use std::hash::BuildHasher;
    let seed = RandomState::new();
    ids.sort_by_key(|id| seed.hash_one(id));
    ids
}

fn fmt_err(e: Error) -> String {
    e.to_string()
}

// ---------------------------------------------------------------------------
// "Add to playlist" picker, a modal `ui::app` overlays.

pub struct PickerState {
    pub track_id: u64,
    playlists: Vec<Playlist>,
    loading: bool,
    error: Option<String>,
    adding_uuid: Option<String>,
    added_to: Option<String>,
}

#[derive(Debug, Clone)]
pub enum PickerMessage {
    Loaded(Result<Vec<Playlist>, String>),
    Choose(String),
    Added(String, Result<(), String>),
    Close,
}

pub enum PickerEffect {
    Close,
}

impl PickerState {
    /// Opens the picker for `track_id` and kicks off the one fetch it needs:
    /// every playlist the user owns or has favourited
    /// (`playlistsAndFavoritePlaylists`, which already unwraps the
    /// `{playlist, created}` unwrap for us).
    pub fn open(track_id: u64, api: &ApiClient) -> (Self, Task<PickerMessage>) {
        let state = Self {
            track_id,
            playlists: Vec::new(),
            loading: true,
            error: None,
            adding_uuid: None,
            added_to: None,
        };
        let task = Task::perform(load_playlists(api.clone()), PickerMessage::Loaded);
        (state, task)
    }

    pub fn update(
        &mut self,
        message: PickerMessage,
        api: &ApiClient,
    ) -> (Task<PickerMessage>, Option<PickerEffect>) {
        match message {
            PickerMessage::Loaded(Ok(playlists)) => {
                self.loading = false;
                self.playlists = playlists;
                (Task::none(), None)
            }
            PickerMessage::Loaded(Err(e)) => {
                self.loading = false;
                self.error = Some(e);
                (Task::none(), None)
            }
            PickerMessage::Choose(uuid) => {
                self.error = None;
                self.adding_uuid = Some(uuid.clone());
                let title = self
                    .playlists
                    .iter()
                    .find(|p| p.uuid.as_deref() == Some(uuid.as_str()))
                    .map(|p| p.title.clone())
                    .unwrap_or_default();
                let api = api.clone();
                let track_id = self.track_id;
                (
                    Task::perform(add_track(api, uuid, track_id), move |r| {
                        PickerMessage::Added(title.clone(), r)
                    }),
                    None,
                )
            }
            PickerMessage::Added(title, Ok(())) => {
                self.adding_uuid = None;
                self.added_to = Some(title);
                (Task::none(), None)
            }
            PickerMessage::Added(_, Err(e)) => {
                self.adding_uuid = None;
                self.error = Some(e);
                (Task::none(), None)
            }
            PickerMessage::Close => (Task::none(), Some(PickerEffect::Close)),
        }
    }

    pub fn view(&self, tokens: Tokens) -> Element<'_, PickerMessage> {
        let mut col = column![
            row![
                text("Add to playlist")
                    .size(tokens.text_lg)
                    .color(tokens.text),
                iced::widget::space::horizontal(),
                small_button(tokens, "✕", PickerMessage::Close),
            ]
            .align_y(iced::Alignment::Center),
        ]
        .spacing(tokens.space_md)
        .width(Length::Fixed(360.0));

        if let Some(title) = &self.added_to {
            col = col.push(
                text(format!("Added to \"{title}\"."))
                    .size(tokens.text_sm)
                    .color(tokens.success),
            );
        }
        if let Some(e) = &self.error {
            col = col.push(text(e.clone()).size(tokens.text_sm).color(tokens.danger));
        }
        if self.loading {
            col = col.push(
                text("Loading your playlists…")
                    .size(tokens.text_sm)
                    .color(tokens.muted),
            );
        } else if self.playlists.is_empty() {
            col = col.push(
                text("You have no playlists yet.")
                    .size(tokens.text_sm)
                    .color(tokens.muted),
            );
        } else {
            let mut list = column![].spacing(tokens.space_xs);
            for p in &self.playlists {
                let Some(uuid) = p.uuid.clone() else { continue };
                let busy = self.adding_uuid.as_deref() == Some(uuid.as_str());
                let label = if busy {
                    format!("{}…", p.title)
                } else {
                    p.title.clone()
                };
                list = list.push(
                    button(text(label).size(tokens.text_sm).color(tokens.text))
                        .width(Length::Fill)
                        .padding(tokens.space_sm)
                        .on_press_maybe((!busy).then(|| PickerMessage::Choose(uuid.clone())))
                        .style(move |_theme: &Theme, status| {
                            let hovered = matches!(status, button::Status::Hovered);
                            button::Style {
                                background: hovered.then_some(tokens.elevated.into()),
                                text_color: tokens.text,
                                border: Border {
                                    radius: tokens.radius_sm.into(),
                                    ..Border::default()
                                },
                                ..button::Style::default()
                            }
                        }),
                );
            }
            col = col.push(scrollable(list).height(Length::Fixed(280.0)));
        }

        container(col)
            .padding(tokens.space_lg)
            .style(move |_theme: &Theme| container::Style {
                background: Some(tokens.surface.into()),
                border: Border {
                    color: tokens.border,
                    width: 1.0,
                    radius: tokens.radius_lg.into(),
                },
                ..container::Style::default()
            })
            .into()
    }
}

async fn load_playlists(api: ApiClient) -> Result<Vec<Playlist>, String> {
    api.playlists_and_favorite_playlists()
        .await
        .map_err(fmt_err)
}

async fn add_track(api: ApiClient, uuid: String, track_id: u64) -> Result<(), String> {
    api.playlist_add_tracks(
        &uuid,
        &[track_id],
        None,
        OnDupes::default(),
        OnArtifactNotFound::default(),
        None,
    )
    .await
    .map(|_| ())
    .map_err(fmt_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shuffled_keeps_every_id_exactly_once() {
        let ids = vec![1, 2, 3, 4, 5];
        let out = shuffled(ids.clone());
        let mut sorted = out.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, ids);
    }

    #[test]
    fn track_action_row_renders_the_provided_buttons() {
        #[derive(Debug, Clone, PartialEq, Eq)]
        enum M {
            Action(TrackAction),
            Nav(EntityRef),
        }
        let tokens = Tokens::dark();
        let element = track_action_row(
            tokens,
            42,
            true,
            Some(EntityRef::Album(1)),
            Some(EntityRef::Artist(2)),
            M::Action,
            M::Nav,
        );
        let mut ui = iced_test::simulator(element);
        assert!(ui.find("Play").is_ok());
        assert!(ui.find("Album").is_ok());
        assert!(ui.find("Artist").is_ok());
        assert!(ui.find("♥").is_ok());
    }
}
