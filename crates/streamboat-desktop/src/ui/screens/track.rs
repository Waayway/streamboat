//! The standalone Track page (task item 1): resolves the track's album and
//! opens it with this one row highlighted
//! ([`crate::ui::screens::album::content_with_highlight`]), plus a credits
//! panel from `tracks/{id}/credits` appended below
//! (`tidal-client-features` library-playlists-collections.md §5).

use iced::widget::{column, row, scrollable, text};
use iced::{Element, Length, Task};

use streamboat_core::models::{Credit, Track};
use streamboat_core::proto::QueuePosition;
use streamboat_core::{ApiClient, Error};

use crate::ui::design::Tokens;
use crate::ui::images::ImageCache;
use crate::ui::nav::EntityRef;
use crate::ui::screens::album;
use crate::ui::widgets::{ChipTone, banner};

pub struct State {
    track_id: u64,
    album: album::State,
    credits: Vec<Credit>,
    error: Option<String>,
}

impl State {
    pub fn new(track_id: u64) -> Self {
        Self {
            track_id,
            // A placeholder album id (0) until `TrackLoaded` resolves the
            // real one; `album.loading` stays `true` until then, so this
            // never renders as a wrong album.
            album: album::State::new(0),
            credits: Vec::new(),
            error: None,
        }
    }

    pub fn track_id(&self) -> u64 {
        self.track_id
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Boxed for the same reason `screens::album::Message::AlbumLoaded` is —
    /// `Track` is by far this enum's largest payload.
    TrackLoaded(Result<Box<Track>, String>),
    CreditsLoaded(Result<Vec<Credit>, String>),
    Album(album::Message),
}

pub enum Effect {
    PlayTracks(Vec<u64>),
    Enqueue(u64, QueuePosition),
    Navigate(EntityRef),
    ImagesNeeded(Vec<String>),
    OpenAddToPlaylist(u64),
}

impl From<album::Effect> for Effect {
    fn from(e: album::Effect) -> Self {
        match e {
            album::Effect::PlayTracks(ids) => Effect::PlayTracks(ids),
            album::Effect::Enqueue(id, pos) => Effect::Enqueue(id, pos),
            album::Effect::Navigate(entity) => Effect::Navigate(entity),
            album::Effect::ImagesNeeded(ids) => Effect::ImagesNeeded(ids),
            album::Effect::OpenAddToPlaylist(id) => Effect::OpenAddToPlaylist(id),
        }
    }
}

impl State {
    pub fn load(api: &ApiClient, track_id: u64) -> Task<Message> {
        Task::batch([
            Task::perform(load_track(api.clone(), track_id), Message::TrackLoaded),
            Task::perform(load_credits(api.clone(), track_id), Message::CreditsLoaded),
        ])
    }

    pub fn update(&mut self, message: Message, api: &ApiClient) -> (Task<Message>, Vec<Effect>) {
        match message {
            Message::TrackLoaded(Ok(track)) => {
                let Some(album_id) = track.album.and_then(|a| a.id) else {
                    self.error = Some("this track has no album on file".to_string());
                    return (Task::none(), Vec::new());
                };
                self.album = album::State::new(album_id);
                (
                    album::State::load(api, album_id).map(Message::Album),
                    Vec::new(),
                )
            }
            Message::TrackLoaded(Err(e)) => {
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::CreditsLoaded(Ok(credits)) => {
                self.credits = credits;
                (Task::none(), Vec::new())
            }
            Message::CreditsLoaded(Err(_)) => (Task::none(), Vec::new()),
            Message::Album(inner) => {
                let (task, effects) = self.album.update(inner, api);
                (
                    task.map(Message::Album),
                    effects.into_iter().map(Effect::from).collect(),
                )
            }
        }
    }

    pub fn view<'a>(&'a self, tokens: Tokens, images: &ImageCache) -> Element<'a, Message> {
        let mut body = column![].spacing(tokens.space_lg);
        if let Some(e) = &self.error {
            body = body.push(banner(tokens, e.clone(), ChipTone::Danger));
        }
        body = body.push(
            album::content_with_highlight(tokens, &self.album, images, Some(self.track_id))
                .map(Message::Album),
        );
        if !self.credits.is_empty() {
            body = body.push(credits_panel(tokens, &self.credits));
        }
        crate::ui::widgets::page(tokens, scrollable(body).height(Length::Fill))
    }
}

fn credits_panel<'a>(tokens: Tokens, credits: &'a [Credit]) -> Element<'a, Message> {
    let mut col =
        column![text("Credits").size(tokens.text_md).color(tokens.text)].spacing(tokens.space_sm);
    for c in credits {
        let role = c.kind.clone().unwrap_or_else(|| "Credit".to_string());
        let names = c
            .contributors
            .iter()
            .filter_map(|contributor| contributor.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        col = col.push(
            row![
                text(role)
                    .size(tokens.text_sm)
                    .color(tokens.muted)
                    .width(Length::Fixed(180.0)),
                text(names).size(tokens.text_sm).color(tokens.text),
            ]
            .spacing(tokens.space_sm),
        );
    }
    col.into()
}

async fn load_track(api: ApiClient, id: u64) -> Result<Box<Track>, String> {
    api.track(id).await.map(Box::new).map_err(fmt_err)
}

async fn load_credits(api: ApiClient, id: u64) -> Result<Vec<Credit>, String> {
    api.track_credits(id).await.map_err(fmt_err)
}

fn fmt_err(e: Error) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_test::simulator;
    use streamboat_core::models::{Album, Artist, Contributor};

    #[test]
    fn track_started_load_kicks_off_the_album_load() {
        let mut state = State::new(42);
        let track = Track {
            id: 42,
            title: "Standalone Track".into(),
            album: Some(streamboat_core::models::AlbumRef {
                id: Some(7),
                title: "Its Album".into(),
                cover: None,
            }),
            ..Track::default()
        };
        let (_, effects) = state.update(Message::TrackLoaded(Ok(Box::new(track))), &test_api());
        assert!(effects.is_empty());
        assert_eq!(state.album.album_id(), 7);
    }

    #[test]
    fn renders_credits_panel_when_present() {
        let mut state = State::new(1);
        let _ = state.album.update(
            album::Message::AlbumLoaded(Ok(Box::new(Album {
                id: 7,
                title: "Its Album".into(),
                artists: vec![Artist {
                    id: Some(9),
                    name: "Artist".into(),
                }],
                ..Album::default()
            }))),
            &test_api(),
        );
        state.credits = vec![Credit {
            kind: Some("Producer".into()),
            contributors: vec![Contributor {
                name: Some("Synthetic Producer".into()),
                id: None,
            }],
        }];
        let tokens = Tokens::dark();
        let images = ImageCache::new(4);
        let mut ui = simulator(state.view(tokens, &images));
        assert!(ui.find("Credits").is_ok());
        assert!(ui.find("Synthetic Producer").is_ok());
        assert!(ui.find("Its Album").is_ok());
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
