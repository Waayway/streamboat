//! Now Playing (task item 3): large art, title/artists/album, seek bar,
//! quality badge, and a queue list with reorder-by-buttons plus play-next/
//! add-last for anything already queued. Sends
//! [`streamboat_core::proto::Command::Seek`]/`MoveQueueItem`/
//! `RemoveQueueItem` straight through `ui::app`'s `PlayerLink` — this
//! screen has no state of its own beyond what `PlayerState` already reports
//! (task item 1's "the only engine seam").

use iced::widget::{button, column, container, image, row, scrollable, slider, text};
use iced::{Alignment, Border, Element, Length, Theme};

use streamboat_core::models::TrackSummary;
use streamboat_core::proto::PlayerState;

use crate::ui::design::Tokens;
use crate::ui::format::{format_caption, mmss, quality_badge};
use crate::ui::widgets::{ChipTone, chip};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    /// Milliseconds, capped to `u32` — see `playback_bar::Message::SeekChanged`.
    SeekChanged(u32),
    SeekReleased,
    MoveUp(usize),
    MoveDown(usize),
    Remove(usize),
    /// Opens the full-screen synced-lyrics view (`ui::screens::lyrics`) for
    /// the currently playing track.
    OpenLyrics(u64),
}

pub fn view<'a>(
    tokens: Tokens,
    state: &'a PlayerState,
    queue: &'a [TrackSummary],
    art: Option<image::Handle>,
) -> Element<'a, Message> {
    let Some(current) = state.current.as_ref() else {
        return crate::ui::widgets::page(
            tokens,
            column![
                text("Nothing playing.")
                    .size(tokens.text_lg)
                    .color(tokens.muted)
            ],
        );
    };

    let art_widget: Element<'a, Message> = match art {
        Some(handle) => container(image(handle).width(280.0).height(280.0))
            .style(move |_theme: &Theme| art_style(tokens))
            .into(),
        None => container(text(""))
            .width(280.0)
            .height(280.0)
            .style(move |_theme: &Theme| art_style(tokens))
            .into(),
    };

    let duration_ms = state.duration_ms.unwrap_or(0).max(1);
    let position_ms = state.position_ms.min(duration_ms);
    let duration_u32 = duration_ms.min(u32::MAX as u64) as u32;
    let position_u32 = position_ms.min(u32::MAX as u64) as u32;
    let seek = row![
        text(mmss(Some(position_ms)))
            .size(tokens.text_xs)
            .color(tokens.muted),
        slider(0..=duration_u32, position_u32, Message::SeekChanged)
            .on_release(Message::SeekReleased)
            .width(Length::Fill),
        text(mmss(state.duration_ms))
            .size(tokens.text_xs)
            .color(tokens.muted),
    ]
    .spacing(tokens.space_sm)
    .align_y(Alignment::Center);

    let mut badges = row![chip::<'a, Message>(
        tokens,
        quality_badge(state.stream.as_ref().and_then(|s| s.quality)),
        ChipTone::Accent,
    )]
    .spacing(tokens.space_sm)
    .align_y(Alignment::Center);
    if let Some(caption) = state.stream.as_ref().map(format_caption) {
        badges = badges.push(text(caption).size(tokens.text_sm).color(tokens.muted));
    }
    badges = badges.push(lyrics_button(tokens, current.id));

    let info = column![
        text(current.title.clone())
            .size(tokens.text_display)
            .color(tokens.text),
        text(current.artists.clone())
            .size(tokens.text_lg)
            .color(tokens.muted),
        text(current.album.clone())
            .size(tokens.text_md)
            .color(tokens.muted),
        badges,
        seek,
    ]
    .spacing(tokens.space_sm)
    .width(Length::Fill);

    let header = row![art_widget, info]
        .spacing(tokens.space_lg)
        .align_y(Alignment::Start);

    let queue_view = column![
        text("Queue").size(tokens.text_lg).color(tokens.text),
        scrollable(queue_list(tokens, queue, state.current_index)).height(Length::Fill),
    ]
    .spacing(tokens.space_sm)
    .width(Length::Fill);

    crate::ui::widgets::page(
        tokens,
        column![header, queue_view]
            .spacing(tokens.space_lg)
            .height(Length::Fill),
    )
}

fn lyrics_button<'a>(tokens: Tokens, track_id: u64) -> Element<'a, Message> {
    button(text("Lyrics").size(tokens.text_xs))
        .padding([tokens.space_xs, tokens.space_sm])
        .on_press(Message::OpenLyrics(track_id))
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, iced::widget::button::Status::Hovered);
            iced::widget::button::Style {
                background: hovered.then_some(tokens.elevated.into()),
                text_color: tokens.muted,
                border: Border {
                    radius: tokens.radius_pill.into(),
                    ..Border::default()
                },
                ..iced::widget::button::Style::default()
            }
        })
        .into()
}

fn queue_list<'a>(
    tokens: Tokens,
    queue: &'a [TrackSummary],
    current_index: Option<usize>,
) -> Element<'a, Message> {
    if queue.is_empty() {
        return text("The queue is empty.")
            .size(tokens.text_sm)
            .color(tokens.muted)
            .into();
    }
    let mut col = column![].spacing(tokens.space_xs);
    for (index, track) in queue.iter().enumerate() {
        let is_current = current_index == Some(index);
        col = col.push(queue_row(tokens, index, track, is_current, queue.len()));
    }
    col.into()
}

fn queue_row<'a>(
    tokens: Tokens,
    index: usize,
    track: &'a TrackSummary,
    is_current: bool,
    len: usize,
) -> Element<'a, Message> {
    let label = column![
        text(track.title.clone())
            .size(tokens.text_sm)
            .color(tokens.text),
        text(track.artists.clone())
            .size(tokens.text_xs)
            .color(tokens.muted),
    ]
    .spacing(2)
    .width(Length::Fill);

    let mut controls = row![].spacing(tokens.space_xs);
    controls = controls.push(small_button(
        tokens,
        "▲",
        (index > 0).then_some(Message::MoveUp(index)),
    ));
    controls = controls.push(small_button(
        tokens,
        "▼",
        (index + 1 < len).then_some(Message::MoveDown(index)),
    ));
    controls = controls.push(small_button(
        tokens,
        "✕",
        (!is_current).then_some(Message::Remove(index)),
    ));

    let row_content = row![label, controls]
        .spacing(tokens.space_sm)
        .align_y(Alignment::Center)
        .width(Length::Fill);

    container(row_content)
        .padding(tokens.space_sm)
        .width(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(
                (if is_current {
                    tokens.accent_muted
                } else {
                    tokens.surface
                })
                .into(),
            ),
            border: Border {
                radius: tokens.radius_sm.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn small_button<'a>(
    tokens: Tokens,
    label: &'a str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let mut b = button(text(label).size(tokens.text_xs))
        .padding(tokens.space_xs)
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, iced::widget::button::Status::Hovered);
            iced::widget::button::Style {
                background: hovered.then_some(tokens.elevated.into()),
                text_color: tokens.muted,
                border: Border {
                    radius: tokens.radius_sm.into(),
                    ..Border::default()
                },
                ..iced::widget::button::Style::default()
            }
        });
    if let Some(msg) = on_press {
        b = b.on_press(msg);
    }
    b.into()
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

#[cfg(test)]
mod tests {
    use super::*;
    use iced_test::simulator;
    use streamboat_core::AudioQuality;
    use streamboat_core::proto::StreamInfo;

    fn queue() -> Vec<TrackSummary> {
        vec![
            TrackSummary {
                id: 1,
                title: "First".into(),
                artists: "Artist A".into(),
                ..TrackSummary::default()
            },
            TrackSummary {
                id: 2,
                title: "Second".into(),
                artists: "Artist B".into(),
                ..TrackSummary::default()
            },
        ]
    }

    #[test]
    fn renders_current_track_and_queue() {
        let state = PlayerState {
            current: Some(TrackSummary {
                id: 1,
                title: "First".into(),
                artists: "Artist A".into(),
                album: "Album".into(),
                ..TrackSummary::default()
            }),
            current_index: Some(0),
            stream: Some(StreamInfo {
                quality: Some(AudioQuality::Lossless),
                ..StreamInfo::default()
            }),
            ..PlayerState::default()
        };
        let queue = queue();
        let tokens = Tokens::dark();
        let mut ui = simulator(view(tokens, &state, &queue, None));
        assert!(ui.find("First").is_ok());
        assert!(ui.find("Second").is_ok());
        assert!(ui.find("LOSSLESS").is_ok());
    }

    #[test]
    fn nothing_playing_shows_a_placeholder() {
        let state = PlayerState::default();
        let tokens = Tokens::dark();
        let mut ui = simulator(view(tokens, &state, &[], None));
        assert!(ui.find("Nothing playing.").is_ok());
    }
}
