//! The Video entity page (task item 1): metadata only, marked "video
//! playback is not supported yet" per D-038 — streamboat shows video
//! entities as unplayable rather than hiding them or pretending a play
//! button would work.

use iced::widget::{button, column, container, image, row, text};
use iced::{Element, Length, Task};

use streamboat_core::models::Video;
use streamboat_core::{ApiClient, Error};

use crate::ui::design::Tokens;
use crate::ui::format::{mmss, video_thumbnail_url};
use crate::ui::images::ImageCache;
use crate::ui::nav::EntityRef;
use crate::ui::widgets::{ChipTone, banner};

pub struct State {
    video_id: u64,
    video: Option<Video>,
    loading: bool,
    error: Option<String>,
}

impl State {
    pub fn new(video_id: u64) -> Self {
        Self {
            video_id,
            video: None,
            loading: true,
            error: None,
        }
    }

    pub fn video_id(&self) -> u64 {
        self.video_id
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Result<Video, String>),
    Navigate(EntityRef),
}

pub enum Effect {
    Navigate(EntityRef),
    ImagesNeeded(Vec<String>),
}

impl State {
    pub fn load(api: &ApiClient, video_id: u64) -> Task<Message> {
        Task::perform(load_video(api.clone(), video_id), Message::Loaded)
    }

    pub fn update(&mut self, message: Message, _api: &ApiClient) -> (Task<Message>, Vec<Effect>) {
        match message {
            Message::Loaded(Ok(video)) => {
                let images = video.image_id.clone().into_iter().collect();
                self.video = Some(video);
                self.loading = false;
                self.error = None;
                (Task::none(), vec![Effect::ImagesNeeded(images)])
            }
            Message::Loaded(Err(e)) => {
                self.loading = false;
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::Navigate(entity) => (Task::none(), vec![Effect::Navigate(entity)]),
        }
    }

    pub fn view<'a>(&'a self, tokens: Tokens, images: &ImageCache) -> Element<'a, Message> {
        if self.loading {
            return crate::ui::widgets::page(
                tokens,
                text("Loading…").size(tokens.text_md).color(tokens.muted),
            );
        }
        let Some(video) = &self.video else {
            return crate::ui::widgets::page(
                tokens,
                column![
                    text("Video not found")
                        .size(tokens.text_lg)
                        .color(tokens.text),
                    error_banner(tokens, self),
                ]
                .spacing(tokens.space_sm),
            );
        };

        let art: Element<'a, Message> = match video
            .image_id
            .as_deref()
            .and_then(video_thumbnail_url)
            .and_then(|u| images.peek(&u))
        {
            Some(handle) => image(handle).width(480.0).height(320.0).into(),
            None => container(text("")).width(480.0).height(320.0).into(),
        };

        let mut artist_row = row![].spacing(tokens.space_xs);
        for a in &video.artists {
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
        if video.explicit == Some(true) {
            badges = badges.push(crate::ui::widgets::chip(
                tokens,
                "EXPLICIT",
                ChipTone::Warning,
            ));
        }
        if let Some(q) = &video.quality {
            badges = badges.push(crate::ui::widgets::chip(tokens, q.clone(), ChipTone::Muted));
        }

        let body = column![
            error_banner(tokens, self),
            art,
            text(video.title.clone())
                .size(tokens.text_xl)
                .color(tokens.text),
            artist_row,
            text(mmss(video.duration.map(|d| u64::from(d) * 1000)))
                .size(tokens.text_sm)
                .color(tokens.muted),
            badges,
            crate::ui::widgets::banner(
                tokens,
                "Video playback is not supported yet.",
                ChipTone::Muted,
            ),
        ]
        .spacing(tokens.space_md)
        .width(Length::Fill);

        crate::ui::widgets::page(tokens, body)
    }
}

fn error_banner<'a>(tokens: Tokens, state: &State) -> Element<'a, Message> {
    match &state.error {
        Some(e) => banner(tokens, e.clone(), ChipTone::Danger),
        None => column![].into(),
    }
}

async fn load_video(api: ApiClient, id: u64) -> Result<Video, String> {
    api.video(id).await.map_err(fmt_err)
}

fn fmt_err(e: Error) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_test::simulator;

    #[test]
    fn renders_metadata_and_the_unsupported_notice() {
        let mut state = State::new(1);
        state.loading = false;
        state.video = Some(Video {
            id: 1,
            title: "Synthetic Video".into(),
            duration: Some(180),
            explicit: Some(true),
            ..Video::default()
        });
        let tokens = Tokens::dark();
        let images = ImageCache::new(4);
        let mut ui = simulator(state.view(tokens, &images));
        assert!(ui.find("Synthetic Video").is_ok());
        assert!(ui.find("Video playback is not supported yet.").is_ok());
        assert!(ui.find("EXPLICIT").is_ok());
    }

    #[test]
    fn loading_state_shows_a_loading_note() {
        let state = State::new(1);
        let tokens = Tokens::dark();
        let images = ImageCache::new(4);
        let mut ui = simulator(state.view(tokens, &images));
        assert!(ui.find("Loading…").is_ok());
    }
}
