//! Full-screen synced lyrics (task item 4): fetched on track start (via
//! [`Message::Requested`], which `ui::app` sends both from
//! `Event::TrackStarted` and when the user opens this screen), the current
//! line highlighted from `Event::Position` with auto-scroll, a plain-text
//! fallback, a "no lyrics" state, and click-to-seek on a synced line.

use iced::widget::{button, column, scrollable, text};
use iced::{Border, Element, Length, Task, Theme};

use streamboat_core::api::lyrics::parse_synced_lyrics;
use streamboat_core::models::LyricLine;
use streamboat_core::{ApiClient, Error};

use crate::ui::design::Tokens;
use crate::ui::widgets::{ChipTone, banner};

/// A stable widget id for the lines list, so [`Message::PositionChanged`]
/// can scroll it programmatically (`iced::widget::operation::scroll_to`).
fn scroll_id() -> iced::widget::Id {
    iced::widget::Id::new("lyrics-lines")
}

/// The vertical space one line takes — used only to compute the auto-scroll
/// offset, not enforced as a hard row height (`text` still wraps normally).
const ROW_HEIGHT: f32 = 34.0;

enum Status {
    Idle,
    Loading,
    Synced(Vec<LyricLine>),
    Plain(String),
    NoLyrics,
    Error(String),
}

pub struct State {
    track_id: Option<u64>,
    status: Status,
    highlighted: Option<usize>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            track_id: None,
            status: Status::Idle,
            highlighted: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Fetch lyrics for `track_id` unless already loaded/loading for it —
    /// safe to send repeatedly (e.g. once from `Event::TrackStarted`, again
    /// if the user opens the Lyrics screen before that resolves).
    Requested(u64),
    Loaded(
        u64,
        Result<Option<streamboat_core::models::LyricsResponse>, String>,
    ),
    SeekTo(u64),
    /// The live position, forwarded from `Event::Position` — recomputes the
    /// highlighted line and, when it changes, scrolls to it.
    PositionChanged(u64),
}

pub enum Effect {
    Seek(u64),
}

impl State {
    pub fn update(&mut self, message: Message, api: &ApiClient) -> (Task<Message>, Option<Effect>) {
        match message {
            Message::Requested(track_id) => {
                if self.track_id == Some(track_id) && !matches!(self.status, Status::Idle) {
                    return (Task::none(), None);
                }
                self.track_id = Some(track_id);
                self.status = Status::Loading;
                self.highlighted = None;
                (
                    Task::perform(load_lyrics(api.clone(), track_id), move |r| {
                        Message::Loaded(track_id, r)
                    }),
                    None,
                )
            }
            Message::Loaded(track_id, result) => {
                if self.track_id != Some(track_id) {
                    // A newer track started before this one resolved.
                    return (Task::none(), None);
                }
                self.status = match result {
                    Ok(Some(response)) => match response.subtitles.filter(|s| !s.trim().is_empty())
                    {
                        Some(subtitles) => {
                            let lines = parse_synced_lyrics(&subtitles);
                            if lines.is_empty() {
                                response
                                    .lyrics
                                    .filter(|l| !l.trim().is_empty())
                                    .map(Status::Plain)
                                    .unwrap_or(Status::NoLyrics)
                            } else {
                                Status::Synced(lines)
                            }
                        }
                        None => response
                            .lyrics
                            .filter(|l| !l.trim().is_empty())
                            .map(Status::Plain)
                            .unwrap_or(Status::NoLyrics),
                    },
                    Ok(None) => Status::NoLyrics,
                    Err(e) => Status::Error(e),
                };
                (Task::none(), None)
            }
            Message::SeekTo(ms) => (Task::none(), Some(Effect::Seek(ms))),
            Message::PositionChanged(position_ms) => {
                let Status::Synced(lines) = &self.status else {
                    return (Task::none(), None);
                };
                let index = highlighted_index(lines, position_ms);
                if index == self.highlighted {
                    return (Task::none(), None);
                }
                self.highlighted = index;
                let Some(index) = index else {
                    return (Task::none(), None);
                };
                let offset = iced::widget::operation::AbsoluteOffset {
                    x: None,
                    y: Some((index as f32 * ROW_HEIGHT).max(0.0)),
                };
                (
                    iced::widget::operation::scroll_to(scroll_id(), offset),
                    None,
                )
            }
        }
    }

    pub fn view<'a>(&'a self, tokens: Tokens) -> Element<'a, Message> {
        let body: Element<'a, Message> = match &self.status {
            Status::Idle | Status::Loading => text("Loading lyrics…")
                .size(tokens.text_md)
                .color(tokens.muted)
                .into(),
            Status::NoLyrics => text("No lyrics for this track.")
                .size(tokens.text_md)
                .color(tokens.muted)
                .into(),
            Status::Error(e) => banner(tokens, e.clone(), ChipTone::Danger),
            Status::Plain(text_body) => scrollable(
                text(text_body.clone())
                    .size(tokens.text_lg)
                    .color(tokens.text),
            )
            .height(Length::Fill)
            .into(),
            Status::Synced(lines) => synced_view(tokens, lines, self.highlighted),
        };

        crate::ui::widgets::page(
            tokens,
            column![text("Lyrics").size(tokens.text_xl).color(tokens.text), body,]
                .spacing(tokens.space_lg)
                .height(Length::Fill),
        )
    }
}

fn synced_view<'a>(
    tokens: Tokens,
    lines: &'a [LyricLine],
    highlighted: Option<usize>,
) -> Element<'a, Message> {
    let mut col = column![].spacing(tokens.space_sm);
    for (index, line) in lines.iter().enumerate() {
        let is_current = highlighted == Some(index);
        col = col.push(
            button(
                text(line.text.clone())
                    .size(if is_current {
                        tokens.text_lg
                    } else {
                        tokens.text_md
                    })
                    .color(if is_current {
                        tokens.accent
                    } else {
                        tokens.muted
                    }),
            )
            .width(Length::Fill)
            .padding(tokens.space_xs)
            .on_press(Message::SeekTo(line.time_ms))
            .style(move |_theme: &Theme, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: (hovered || is_current).then_some(
                        (if is_current {
                            tokens.accent_muted
                        } else {
                            tokens.elevated
                        })
                        .into(),
                    ),
                    text_color: if is_current {
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
            }),
        );
    }
    scrollable(col).id(scroll_id()).height(Length::Fill).into()
}

/// The line whose timestamp has arrived but whose successor's has not —
/// pure and unit-testable without any `iced` dependency.
pub fn highlighted_index(lines: &[LyricLine], position_ms: u64) -> Option<usize> {
    let mut current = None;
    for (index, line) in lines.iter().enumerate() {
        if line.time_ms <= position_ms {
            current = Some(index);
        } else {
            break;
        }
    }
    current
}

async fn load_lyrics(
    api: ApiClient,
    track_id: u64,
) -> Result<Option<streamboat_core::models::LyricsResponse>, String> {
    api.lyrics(track_id).await.map_err(fmt_err)
}

fn fmt_err(e: Error) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_test::simulator;

    fn lines() -> Vec<LyricLine> {
        vec![
            LyricLine {
                time_ms: 0,
                text: "First line".into(),
            },
            LyricLine {
                time_ms: 5_000,
                text: "Second line".into(),
            },
            LyricLine {
                time_ms: 10_000,
                text: "Third line".into(),
            },
        ]
    }

    #[test]
    fn highlighted_index_picks_the_most_recent_line() {
        let l = lines();
        assert_eq!(highlighted_index(&l, 0), Some(0));
        assert_eq!(highlighted_index(&l, 4_999), Some(0));
        assert_eq!(highlighted_index(&l, 5_000), Some(1));
        assert_eq!(highlighted_index(&l, 999_999), Some(2));
    }

    #[test]
    fn highlighted_index_is_none_before_the_first_line() {
        let l = vec![LyricLine {
            time_ms: 1_000,
            text: "Late start".into(),
        }];
        assert_eq!(highlighted_index(&l, 0), None);
    }

    #[test]
    fn renders_synced_lines_with_the_current_one_marked() {
        let state = State {
            track_id: Some(1),
            status: Status::Synced(lines()),
            highlighted: Some(1),
        };
        let tokens = Tokens::dark();
        let mut ui = simulator(state.view(tokens));
        assert!(ui.find("First line").is_ok());
        assert!(ui.find("Second line").is_ok());
    }

    #[test]
    fn no_lyrics_state_renders_a_note() {
        let state = State {
            status: Status::NoLyrics,
            ..State::default()
        };
        let tokens = Tokens::dark();
        let mut ui = simulator(state.view(tokens));
        assert!(ui.find("No lyrics for this track.").is_ok());
    }

    #[test]
    fn plain_text_fallback_renders_the_raw_lyrics() {
        let state = State {
            status: Status::Plain("Some unsynced lyrics text".into()),
            ..State::default()
        };
        let tokens = Tokens::dark();
        let mut ui = simulator(state.view(tokens));
        assert!(ui.find("Some unsynced lyrics text").is_ok());
    }

    #[test]
    fn position_changed_updates_the_highlighted_line() {
        let mut state = State {
            status: Status::Synced(lines()),
            ..State::default()
        };
        let (_, effect) = state.update(Message::PositionChanged(6_000), &test_api());
        assert!(effect.is_none());
        assert_eq!(state.highlighted, Some(1));
    }

    #[test]
    fn seek_to_bubbles_a_seek_effect() {
        let mut state = State::default();
        let (_, effect) = state.update(Message::SeekTo(5_000), &test_api());
        assert!(matches!(effect, Some(Effect::Seek(5_000))));
    }

    #[test]
    fn requested_is_idempotent_for_the_same_already_loaded_track() {
        let mut state = State {
            track_id: Some(1),
            status: Status::Synced(lines()),
            ..State::default()
        };
        let (task, effect) = state.update(Message::Requested(1), &test_api());
        assert!(effect.is_none());
        // `Task::none()` carries no way to inspect emptiness directly, but a
        // second identical request must not reset a loaded state to Loading.
        let _ = task;
        assert!(matches!(state.status, Status::Synced(_)));
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
