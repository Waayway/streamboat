//! The Playlist entity page: header (image/title/description/
//! creator/track count), paginated items with load-more, play-all,
//! favourite/unfavourite, and — for the user's own playlists — rename/
//! describe, remove item, move item up/down (through the ETag-precondition
//! helpers in `api/playlists.rs`), and delete with confirmation
//! (`tidal-client-features` library-playlists-collections.md §1).

use std::collections::HashSet;

use iced::widget::{button, column, container, image, row, scrollable, text, text_input};
use iced::{Alignment, Border, Element, Length, Task, Theme};

use streamboat_core::api::pagination::DEFAULT_PAGE_SIZE as PAGE_SIZE;
use streamboat_core::models::{Playlist, PlaylistItem};
use streamboat_core::proto::QueuePosition;
use streamboat_core::{ApiClient, Error};

use crate::ui::actions::{self, TrackAction};
use crate::ui::design::Tokens;
use crate::ui::format::{mmss, playlist_cover_url};
use crate::ui::images::ImageCache;
use crate::ui::nav::EntityRef;
use crate::ui::widgets::{ChipTone, banner};

pub struct State {
    uuid: String,
    playlist: Option<Playlist>,
    items: Vec<PlaylistItem>,
    has_more: bool,
    is_owner: bool,
    is_favorite: bool,
    favorite_tracks: HashSet<u64>,
    loading: bool,
    error: Option<String>,
    edit: Option<EditState>,
    confirm_delete: bool,
    deleting: bool,
}

struct EditState {
    title: String,
    description: String,
    saving: bool,
}

impl State {
    pub fn new(uuid: String) -> Self {
        Self {
            uuid,
            playlist: None,
            items: Vec::new(),
            has_more: false,
            is_owner: false,
            is_favorite: false,
            favorite_tracks: HashSet::new(),
            loading: true,
            error: None,
            edit: None,
            confirm_delete: false,
            deleting: false,
        }
    }

    pub fn uuid(&self) -> &str {
        &self.uuid
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Boxed for the same reason `screens::album::Message::AlbumLoaded` is —
    /// `Playlist` is by far this enum's largest payload.
    PlaylistLoaded(Result<Box<Playlist>, String>),
    OwnershipChecked(Result<bool, String>),
    ItemsLoaded(Result<(Vec<PlaylistItem>, bool), String>),
    FavoritesLoaded(Result<(bool, HashSet<u64>), String>),
    LoadMore,
    PlayAll,
    TogglePlaylistFavorite,
    PlaylistFavoriteToggled(Result<bool, String>),
    TrackFavoriteToggled(Result<(u64, bool), String>),
    Track(TrackAction),
    Navigate(EntityRef),
    StartEdit,
    CancelEdit,
    EditTitleChanged(String),
    EditDescriptionChanged(String),
    SaveEdit,
    EditSaved(Result<(String, String), String>),
    RemoveItem(usize),
    ItemRemoved(usize, Result<(), String>),
    MoveUp(usize),
    MoveDown(usize),
    ItemMoved(usize, usize, Result<(), String>),
    RequestDelete,
    CancelDelete,
    ConfirmDelete,
    Deleted(Result<(), String>),
}

pub enum Effect {
    PlayTracks(Vec<u64>),
    Enqueue(u64, QueuePosition),
    Navigate(EntityRef),
    ImagesNeeded(Vec<String>),
    OpenAddToPlaylist(u64),
    /// The user deleted their own playlist; `ui::app` should navigate away
    /// (to My Collection) since this screen no longer has anything to show.
    Deleted,
}

impl State {
    pub fn load(api: &ApiClient, uuid: String) -> Task<Message> {
        Task::batch([
            Task::perform(
                load_playlist(api.clone(), uuid.clone()),
                Message::PlaylistLoaded,
            ),
            Task::perform(
                check_ownership(api.clone(), uuid.clone()),
                Message::OwnershipChecked,
            ),
            Task::perform(
                load_items(api.clone(), uuid.clone(), 0),
                Message::ItemsLoaded,
            ),
            Task::perform(load_favorites(api.clone(), uuid), Message::FavoritesLoaded),
        ])
    }

    pub fn update(&mut self, message: Message, api: &ApiClient) -> (Task<Message>, Vec<Effect>) {
        match message {
            Message::PlaylistLoaded(Ok(playlist)) => {
                let images = playlist_cover_url(&playlist).into_iter().collect();
                self.playlist = Some(*playlist);
                self.loading = false;
                self.error = None;
                (Task::none(), vec![Effect::ImagesNeeded(images)])
            }
            Message::PlaylistLoaded(Err(e)) => {
                self.loading = false;
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::OwnershipChecked(Ok(is_owner)) => {
                self.is_owner = is_owner;
                (Task::none(), Vec::new())
            }
            Message::OwnershipChecked(Err(_)) => (Task::none(), Vec::new()),
            Message::ItemsLoaded(Ok((items, has_more))) => {
                let images = items
                    .iter()
                    .filter_map(|i| i.as_track())
                    .filter_map(|t| t.album.and_then(|a| a.cover))
                    .collect();
                self.items.extend(items);
                self.has_more = has_more;
                (Task::none(), vec![Effect::ImagesNeeded(images)])
            }
            Message::ItemsLoaded(Err(e)) => {
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::FavoritesLoaded(Ok((favorite, tracks))) => {
                self.is_favorite = favorite;
                self.favorite_tracks = tracks;
                (Task::none(), Vec::new())
            }
            Message::FavoritesLoaded(Err(_)) => (Task::none(), Vec::new()),
            Message::LoadMore => {
                let offset = self.items.len() as u32;
                (
                    Task::perform(
                        load_items(api.clone(), self.uuid.clone(), offset),
                        Message::ItemsLoaded,
                    ),
                    Vec::new(),
                )
            }
            Message::PlayAll => (Task::none(), vec![Effect::PlayTracks(self.track_ids())]),
            Message::TogglePlaylistFavorite => {
                let api = api.clone();
                let uuid = self.uuid.clone();
                let was = self.is_favorite;
                (
                    Task::perform(
                        toggle_playlist_favorite(api, uuid, was),
                        Message::PlaylistFavoriteToggled,
                    ),
                    Vec::new(),
                )
            }
            Message::PlaylistFavoriteToggled(Ok(now)) => {
                self.is_favorite = now;
                (Task::none(), Vec::new())
            }
            Message::PlaylistFavoriteToggled(Err(e)) => {
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
            Message::Track(action) => self.handle_track_action(action, api),
            Message::Navigate(entity) => (Task::none(), vec![Effect::Navigate(entity)]),
            Message::StartEdit => {
                let playlist = self.playlist.as_ref();
                self.edit = Some(EditState {
                    title: playlist.map(|p| p.title.clone()).unwrap_or_default(),
                    description: playlist
                        .and_then(|p| p.description.clone())
                        .unwrap_or_default(),
                    saving: false,
                });
                (Task::none(), Vec::new())
            }
            Message::CancelEdit => {
                self.edit = None;
                (Task::none(), Vec::new())
            }
            Message::EditTitleChanged(v) => {
                if let Some(edit) = &mut self.edit {
                    edit.title = v;
                }
                (Task::none(), Vec::new())
            }
            Message::EditDescriptionChanged(v) => {
                if let Some(edit) = &mut self.edit {
                    edit.description = v;
                }
                (Task::none(), Vec::new())
            }
            Message::SaveEdit => {
                let Some(edit) = &mut self.edit else {
                    return (Task::none(), Vec::new());
                };
                edit.saving = true;
                let api = api.clone();
                let uuid = self.uuid.clone();
                let title = edit.title.clone();
                let description = edit.description.clone();
                (
                    Task::perform(save_edit(api, uuid, title, description), Message::EditSaved),
                    Vec::new(),
                )
            }
            Message::EditSaved(Ok((title, description))) => {
                if let Some(p) = &mut self.playlist {
                    p.title = title;
                    p.description = Some(description).filter(|d| !d.is_empty());
                }
                self.edit = None;
                (Task::none(), Vec::new())
            }
            Message::EditSaved(Err(e)) => {
                if let Some(edit) = &mut self.edit {
                    edit.saving = false;
                }
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::RemoveItem(index) => {
                let api = api.clone();
                let uuid = self.uuid.clone();
                (
                    Task::perform(remove_item(api, uuid, index as u32), move |r| {
                        Message::ItemRemoved(index, r)
                    }),
                    Vec::new(),
                )
            }
            Message::ItemRemoved(index, Ok(())) => {
                if index < self.items.len() {
                    self.items.remove(index);
                }
                (Task::none(), Vec::new())
            }
            Message::ItemRemoved(_, Err(e)) => {
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::MoveUp(index) => self.move_item(index, index.saturating_sub(1), api),
            Message::MoveDown(index) => self.move_item(index, index + 1, api),
            Message::ItemMoved(from, to, Ok(())) => {
                if from < self.items.len() && to < self.items.len() {
                    let entry = self.items.remove(from);
                    self.items.insert(to, entry);
                }
                (Task::none(), Vec::new())
            }
            Message::ItemMoved(_, _, Err(e)) => {
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::RequestDelete => {
                self.confirm_delete = true;
                (Task::none(), Vec::new())
            }
            Message::CancelDelete => {
                self.confirm_delete = false;
                (Task::none(), Vec::new())
            }
            Message::ConfirmDelete => {
                self.deleting = true;
                let api = api.clone();
                let uuid = self.uuid.clone();
                (
                    Task::perform(delete_playlist(api, uuid), Message::Deleted),
                    Vec::new(),
                )
            }
            Message::Deleted(Ok(())) => (Task::none(), vec![Effect::Deleted]),
            Message::Deleted(Err(e)) => {
                self.deleting = false;
                self.confirm_delete = false;
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
        }
    }

    fn move_item(
        &mut self,
        from: usize,
        to: usize,
        api: &ApiClient,
    ) -> (Task<Message>, Vec<Effect>) {
        if from == to || to >= self.items.len() {
            return (Task::none(), Vec::new());
        }
        let api = api.clone();
        let uuid = self.uuid.clone();
        (
            Task::perform(reorder_item(api, uuid, from as u32, to as u32), move |r| {
                Message::ItemMoved(from, to, r)
            }),
            Vec::new(),
        )
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

    fn track_ids(&self) -> Vec<u64> {
        self.items
            .iter()
            .filter_map(|i| i.as_track())
            .map(|t| t.id)
            .collect()
    }

    pub fn view<'a>(&'a self, tokens: Tokens, images: &ImageCache) -> Element<'a, Message> {
        if self.loading {
            return crate::ui::widgets::page(
                tokens,
                text("Loading…").size(tokens.text_md).color(tokens.muted),
            );
        }
        let Some(playlist) = &self.playlist else {
            return crate::ui::widgets::page(
                tokens,
                column![
                    text("Playlist not found")
                        .size(tokens.text_lg)
                        .color(tokens.text),
                    error_banner(tokens, self),
                ]
                .spacing(tokens.space_sm),
            );
        };

        let art: Element<'a, Message> =
            match playlist_cover_url(playlist).and_then(|u| images.peek(&u)) {
                Some(handle) => image(handle).width(200.0).height(200.0).into(),
                None => container(text("")).width(200.0).height(200.0).into(),
            };

        let track_count = playlist.number_of_tracks.unwrap_or(self.items.len() as u32);

        let header_info: Element<'a, Message> = if let Some(edit) = &self.edit {
            column![
                text_input("Title", &edit.title).on_input(Message::EditTitleChanged),
                text_input("Description", &edit.description)
                    .on_input(Message::EditDescriptionChanged),
                row![
                    primary_button(
                        tokens,
                        if edit.saving { "Saving…" } else { "Save" },
                        Message::SaveEdit
                    ),
                    secondary_button(tokens, "Cancel", Message::CancelEdit),
                ]
                .spacing(tokens.space_sm),
            ]
            .spacing(tokens.space_sm)
            .into()
        } else {
            let mut col = column![
                text(playlist.title.clone())
                    .size(tokens.text_xl)
                    .color(tokens.text),
            ]
            .spacing(tokens.space_sm);
            if let Some(desc) = &playlist.description {
                if !desc.is_empty() {
                    col = col.push(text(desc.clone()).size(tokens.text_sm).color(tokens.muted));
                }
            }
            col = col.push(
                text(format!(
                    "{track_count} tracks · {}",
                    mmss(playlist.duration.map(|d| u64::from(d) * 1000))
                ))
                .size(tokens.text_sm)
                .color(tokens.muted),
            );
            let mut actions_row = row![
                primary_button(tokens, "Play all", Message::PlayAll),
                favorite_toggle(tokens, self.is_favorite, Message::TogglePlaylistFavorite),
            ]
            .spacing(tokens.space_sm);
            if self.is_owner {
                actions_row = actions_row
                    .push(secondary_button(tokens, "Edit", Message::StartEdit))
                    .push(danger_button(tokens, "Delete", Message::RequestDelete));
            }
            col.push(actions_row).into()
        };

        let header = row![art, header_info].spacing(tokens.space_lg);

        let mut body = column![header, error_banner(tokens, self)].spacing(tokens.space_lg);

        if self.confirm_delete {
            body = body.push(delete_confirm(tokens, self.deleting));
        }

        body = body.push(items_list(tokens, self));

        if self.has_more {
            body = body.push(secondary_button(tokens, "Load more", Message::LoadMore));
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

fn delete_confirm<'a>(tokens: Tokens, deleting: bool) -> Element<'a, Message> {
    container(
        row![
            text("Delete this playlist? This cannot be undone.")
                .size(tokens.text_sm)
                .color(tokens.danger),
            iced::widget::space::horizontal(),
            danger_button(
                tokens,
                if deleting {
                    "Deleting…"
                } else {
                    "Confirm delete"
                },
                Message::ConfirmDelete
            ),
            secondary_button(tokens, "Cancel", Message::CancelDelete),
        ]
        .spacing(tokens.space_sm)
        .align_y(Alignment::Center),
    )
    .padding(tokens.space_sm)
    .width(Length::Fill)
    .style(move |_theme: &Theme| container::Style {
        background: Some(
            iced::Color {
                a: 0.12,
                ..tokens.danger
            }
            .into(),
        ),
        border: Border {
            color: tokens.danger,
            width: 1.0,
            radius: tokens.radius_sm.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn items_list<'a>(tokens: Tokens, state: &'a State) -> Element<'a, Message> {
    if state.items.is_empty() {
        return text("This playlist is empty.")
            .size(tokens.text_sm)
            .color(tokens.muted)
            .into();
    }
    let mut col = column![].spacing(tokens.space_xs);
    let len = state.items.len();
    for (index, item) in state.items.iter().enumerate() {
        col = col.push(item_row(tokens, state, item, index, len));
    }
    col.into()
}

fn item_row<'a>(
    tokens: Tokens,
    state: &'a State,
    item: &'a PlaylistItem,
    index: usize,
    len: usize,
) -> Element<'a, Message> {
    let Some(track) = item.as_track() else {
        // A video item — streamboat does not play video yet (D-038); show
        // it, but with no playback actions.
        let title = video_title(item).unwrap_or_else(|| "Video".to_string());
        return container(
            row![text(title).size(tokens.text_sm).color(tokens.muted)].width(Length::Fill),
        )
        .padding(tokens.space_sm)
        .width(Length::Fill)
        .into();
    };

    let is_favorite = state.favorite_tracks.contains(&track.id);
    let album = track
        .album
        .as_ref()
        .and_then(|a| a.id)
        .map(EntityRef::Album);
    let artist = track
        .artists
        .first()
        .or(track.artist.as_ref())
        .and_then(|a| a.id)
        .map(EntityRef::Artist);

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

    let mut content = row![
        label,
        text(mmss(track.duration.map(|d| u64::from(d) * 1000)))
            .size(tokens.text_sm)
            .color(tokens.muted),
        actions::track_action_row(
            tokens,
            track.id,
            is_favorite,
            album,
            artist,
            Message::Track,
            Message::Navigate,
        ),
    ]
    .spacing(tokens.space_sm)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    if state.is_owner {
        content = content.push(
            row![
                small_button(tokens, "▲", (index > 0).then_some(Message::MoveUp(index))),
                small_button(
                    tokens,
                    "▼",
                    (index + 1 < len).then_some(Message::MoveDown(index))
                ),
                small_button(tokens, "✕", Some(Message::RemoveItem(index))),
            ]
            .spacing(tokens.space_xs),
        );
    }

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

fn video_title(item: &PlaylistItem) -> Option<String> {
    item.item
        .as_ref()?
        .as_object()?
        .get("title")?
        .as_str()
        .map(str::to_string)
}

fn small_button<'a>(
    tokens: Tokens,
    label: &'a str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let mut b = button(text(label).size(tokens.text_xs))
        .padding(tokens.space_xs)
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
        });
    if let Some(msg) = on_press {
        b = b.on_press(msg);
    }
    b.into()
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

fn danger_button<'a>(tokens: Tokens, label: &'a str, on_press: Message) -> Element<'a, Message> {
    button(text(label).size(tokens.text_sm))
        .padding([tokens.space_xs, tokens.space_md])
        .on_press(on_press)
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: hovered.then_some(tokens.danger.into()),
                text_color: if hovered {
                    tokens.background
                } else {
                    tokens.danger
                },
                border: Border {
                    color: tokens.danger,
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

async fn load_playlist(api: ApiClient, uuid: String) -> Result<Box<Playlist>, String> {
    api.playlist(&uuid).await.map(Box::new).map_err(fmt_err)
}

async fn check_ownership(api: ApiClient, uuid: String) -> Result<bool, String> {
    let playlist = api.playlist(&uuid).await.map_err(fmt_err)?;
    let Some(creator_id) = playlist.creator.and_then(|c| c.id) else {
        return Ok(false);
    };
    let user_id = api.user_id().await.map_err(fmt_err)?;
    Ok(creator_id == user_id)
}

async fn load_items(
    api: ApiClient,
    uuid: String,
    offset: u32,
) -> Result<(Vec<PlaylistItem>, bool), String> {
    let page = api
        .playlist_items(&uuid, PAGE_SIZE, offset)
        .await
        .map_err(fmt_err)?;
    let has_more = page.items.len() as u32 == PAGE_SIZE
        && page
            .total_number_of_items
            .is_none_or(|t| u64::from(offset + PAGE_SIZE) < t);
    Ok((page.items, has_more))
}

async fn load_favorites(api: ApiClient, uuid: String) -> Result<(bool, HashSet<u64>), String> {
    let ids = api.favorite_ids().await.map_err(fmt_err)?;
    let is_favorite = ids.playlist.iter().any(|p| p == &uuid);
    let tracks = ids
        .track
        .into_iter()
        .filter_map(|s| s.parse().ok())
        .collect();
    Ok((is_favorite, tracks))
}

async fn toggle_playlist_favorite(
    api: ApiClient,
    uuid: String,
    was_favorite: bool,
) -> Result<bool, String> {
    let result = if was_favorite {
        api.unfavorite_playlist(&uuid).await
    } else {
        api.favorite_playlist(&uuid).await
    };
    result.map(|()| !was_favorite).map_err(fmt_err)
}

async fn save_edit(
    api: ApiClient,
    uuid: String,
    title: String,
    description: String,
) -> Result<(String, String), String> {
    api.update_playlist(&uuid, Some(&title), Some(&description), None)
        .await
        .map(|()| (title, description))
        .map_err(fmt_err)
}

async fn remove_item(api: ApiClient, uuid: String, index: u32) -> Result<(), String> {
    api.playlist_remove_items(&uuid, &[index], None)
        .await
        .map_err(fmt_err)
}

async fn reorder_item(api: ApiClient, uuid: String, from: u32, to: u32) -> Result<(), String> {
    api.playlist_reorder(&uuid, &[from], to, None)
        .await
        .map_err(fmt_err)
}

async fn delete_playlist(api: ApiClient, uuid: String) -> Result<(), String> {
    api.delete_playlist(&uuid, None).await.map_err(fmt_err)
}

fn fmt_err(e: Error) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_test::simulator;

    fn synthetic_playlist() -> Playlist {
        Playlist {
            uuid: Some("uuid-1".into()),
            title: "Synthetic Playlist".into(),
            description: Some("A synthetic description".into()),
            number_of_tracks: Some(1),
            ..Playlist::default()
        }
    }

    fn track_item(id: u64, title: &str) -> PlaylistItem {
        PlaylistItem {
            kind: Some("track".into()),
            item: Some(
                serde_json::to_value(streamboat_core::models::Track {
                    id,
                    title: title.into(),
                    ..streamboat_core::models::Track::default()
                })
                .unwrap(),
            ),
        }
    }

    #[test]
    fn own_playlist_shows_editing_controls() {
        let mut state = State::new("uuid-1".into());
        state.loading = false;
        state.playlist = Some(synthetic_playlist());
        state.is_owner = true;
        state.items = vec![track_item(1, "Track One")];
        let tokens = Tokens::dark();
        let images = ImageCache::new(4);
        let mut ui = simulator(state.view(tokens, &images));
        assert!(ui.find("Edit").is_ok());
        assert!(ui.find("Delete").is_ok());
        assert!(ui.find("Track One").is_ok());
    }

    #[test]
    fn foreign_playlist_hides_editing_controls() {
        let mut state = State::new("uuid-1".into());
        state.loading = false;
        state.playlist = Some(synthetic_playlist());
        state.is_owner = false;
        state.items = vec![track_item(1, "Track One")];
        let tokens = Tokens::dark();
        let images = ImageCache::new(4);
        let mut ui = simulator(state.view(tokens, &images));
        assert!(ui.find("Edit").is_err());
        assert!(ui.find("Delete").is_err());
        assert!(ui.find("♡ Favourite").is_ok());
    }

    #[test]
    fn removing_an_item_drops_it_from_the_local_list() {
        let mut state = State::new("uuid-1".into());
        state.items = vec![track_item(1, "A"), track_item(2, "B")];
        let (_, effects) = state.update(Message::ItemRemoved(0, Ok(())), &test_api());
        assert!(effects.is_empty());
        assert_eq!(state.items.len(), 1);
        assert_eq!(state.items[0].as_track().unwrap().id, 2);
    }

    #[test]
    fn moving_an_item_up_swaps_local_order() {
        let mut state = State::new("uuid-1".into());
        state.items = vec![track_item(1, "A"), track_item(2, "B")];
        let (_, effects) = state.update(Message::ItemMoved(1, 0, Ok(())), &test_api());
        assert!(effects.is_empty());
        assert_eq!(state.items[0].as_track().unwrap().id, 2);
        assert_eq!(state.items[1].as_track().unwrap().id, 1);
    }

    #[test]
    fn confirmed_delete_bubbles_a_deleted_effect() {
        let mut state = State::new("uuid-1".into());
        let (_, effects) = state.update(Message::Deleted(Ok(())), &test_api());
        assert_eq!(effects.len(), 1);
        assert!(matches!(effects[0], Effect::Deleted));
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
