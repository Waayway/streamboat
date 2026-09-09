//! The Mix entity page: items list, play-all. Mixes have no
//! dedicated metadata endpoint beyond the v1 per-mix page
//! (`ApiClient::mix_page`) for the title, and `ApiClient::mix_items`
//! (best-effort shape, see that method's doc comment) for the track list
//! (`tidal-client-features` browse-pages-screens.md §4).

use std::collections::HashSet;

use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Border, Element, Length, Task, Theme};

use streamboat_core::models::PlaylistItem;
use streamboat_core::proto::QueuePosition;
use streamboat_core::{ApiClient, Error};

use crate::ui::actions::{self, TrackAction};
use crate::ui::design::Tokens;
use crate::ui::format::mmss;
use crate::ui::nav::EntityRef;
use crate::ui::widgets::{ChipTone, banner};

const MAX_ITEMS: usize = 300;

pub struct State {
    mix_id: String,
    title: Option<String>,
    items: Vec<PlaylistItem>,
    favorite_tracks: HashSet<u64>,
    loading: bool,
    error: Option<String>,
}

impl State {
    pub fn new(mix_id: String) -> Self {
        Self {
            mix_id,
            title: None,
            items: Vec::new(),
            favorite_tracks: HashSet::new(),
            loading: true,
            error: None,
        }
    }

    pub fn mix_id(&self) -> &str {
        &self.mix_id
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    TitleLoaded(Result<Option<String>, String>),
    ItemsLoaded(Result<Vec<PlaylistItem>, String>),
    FavoritesLoaded(Result<HashSet<u64>, String>),
    TrackFavoriteToggled(Result<(u64, bool), String>),
    PlayAll,
    Track(TrackAction),
    Navigate(EntityRef),
}

pub enum Effect {
    PlayTracks(Vec<u64>),
    Enqueue(u64, QueuePosition),
    Navigate(EntityRef),
    OpenAddToPlaylist(u64),
}

impl State {
    pub fn load(api: &ApiClient, mix_id: String) -> Task<Message> {
        Task::batch([
            Task::perform(
                load_title(api.clone(), mix_id.clone()),
                Message::TitleLoaded,
            ),
            Task::perform(load_items(api.clone(), mix_id), Message::ItemsLoaded),
            Task::perform(
                actions::load_favorite_track_ids(api.clone()),
                Message::FavoritesLoaded,
            ),
        ])
    }

    pub fn update(&mut self, message: Message, api: &ApiClient) -> (Task<Message>, Vec<Effect>) {
        match message {
            Message::TitleLoaded(Ok(title)) => {
                self.title = title;
                self.loading = false;
                (Task::none(), Vec::new())
            }
            Message::TitleLoaded(Err(e)) => {
                self.loading = false;
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::ItemsLoaded(Ok(items)) => {
                // No artwork is rendered on this list — it's an items-list-and
                // -play-all view only — so unlike the other entity screens
                // this does not emit `Effect::ImagesNeeded`; nothing would
                // ever read the cache entries it would produce.
                self.items = items;
                (Task::none(), Vec::new())
            }
            Message::ItemsLoaded(Err(e)) => {
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::FavoritesLoaded(Ok(tracks)) => {
                self.favorite_tracks = tracks;
                (Task::none(), Vec::new())
            }
            Message::FavoritesLoaded(Err(_)) => (Task::none(), Vec::new()),
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

    fn track_ids(&self) -> Vec<u64> {
        self.items
            .iter()
            .filter_map(|i| i.as_track())
            .map(|t| t.id)
            .collect()
    }

    pub fn view<'a>(&'a self, tokens: Tokens) -> Element<'a, Message> {
        if self.loading {
            return crate::ui::widgets::page(
                tokens,
                text("Loading…").size(tokens.text_md).color(tokens.muted),
            );
        }
        let title = self.title.clone().unwrap_or_else(|| "Mix".to_string());
        let mut body = column![
            row![
                text(title).size(tokens.text_xl).color(tokens.text),
                iced::widget::space::horizontal(),
                play_all_button(tokens),
            ]
            .align_y(Alignment::Center)
            .width(Length::Fill),
        ]
        .spacing(tokens.space_lg);

        if let Some(e) = &self.error {
            body = body.push(banner(tokens, e.clone(), ChipTone::Danger));
        }

        body = body.push(items_list(tokens, self));

        crate::ui::widgets::page(tokens, scrollable(body).height(Length::Fill))
    }
}

fn play_all_button<'a>(tokens: Tokens) -> Element<'a, Message> {
    use iced::widget::button;
    button(text("Play all").size(tokens.text_sm))
        .padding([tokens.space_xs, tokens.space_md])
        .on_press(Message::PlayAll)
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

fn items_list<'a>(tokens: Tokens, state: &'a State) -> Element<'a, Message> {
    if state.items.is_empty() {
        return text("No items.")
            .size(tokens.text_sm)
            .color(tokens.muted)
            .into();
    }
    let mut col = column![].spacing(tokens.space_xs);
    for item in &state.items {
        let Some(track) = item.as_track() else {
            continue;
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
        let content = row![
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
        col = col.push(
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
                }),
        );
    }
    col.into()
}

async fn load_title(api: ApiClient, mix_id: String) -> Result<Option<String>, String> {
    let page = api.mix_page(&mix_id).await.map_err(fmt_err)?;
    Ok(page.title)
}

async fn load_items(api: ApiClient, mix_id: String) -> Result<Vec<PlaylistItem>, String> {
    streamboat_core::api::pagination::collect_all(
        streamboat_core::api::pagination::DEFAULT_PAGE_SIZE,
        MAX_ITEMS,
        |offset, limit| api.mix_items(&mix_id, limit, offset),
    )
    .await
    .map_err(fmt_err)
}

fn fmt_err(e: Error) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_test::simulator;

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
    fn renders_title_and_items() {
        let mut state = State::new("mix-1".into());
        state.loading = false;
        state.title = Some("Synthetic Daily Mix".into());
        state.items = vec![track_item(1, "Mix Track")];
        let tokens = Tokens::dark();
        let mut ui = simulator(state.view(tokens));
        assert!(ui.find("Synthetic Daily Mix").is_ok());
        assert!(ui.find("Mix Track").is_ok());
        assert!(ui.find("Play all").is_ok());
    }

    #[test]
    fn play_all_carries_every_track_id() {
        let mut state = State::new("mix-1".into());
        state.items = vec![track_item(1, "A"), track_item(2, "B")];
        let (_, effects) = state.update(Message::PlayAll, &test_api());
        match &effects[0] {
            Effect::PlayTracks(ids) => assert_eq!(ids, &vec![1, 2]),
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
