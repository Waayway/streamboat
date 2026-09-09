//! The floating mini-player window (D-036): a second iced
//! window over the same [`crate::ui::app::App`] state — art, title/artists,
//! previous/play-pause/next, a thin seek bar, the quality badge, and a
//! button to restore the main window. Toggled from the playback bar
//! (`ui::playback_bar::Message::ToggleMiniPlayer`) and a keyboard shortcut
//! (`ui::app::Shortcut::ToggleMiniPlayer`); both windows read the same
//! `App::player_state`/`App::current_art()` — this module owns only the
//! view and its own small `Message`, never a copy of the state.

use iced::widget::{button, column, container, image, row, slider, text};
use iced::{Alignment, Border, Element, Length, Theme};

use streamboat_core::proto::{PlaybackStatus, PlayerState};

use crate::ui::design::Tokens;
use crate::ui::format::quality_badge;

/// The window's fixed, compact logical size. Not
/// resizable — see `ui::app`'s window settings for this window.
pub const SIZE: iced::Size = iced::Size::new(300.0, 132.0);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Message {
    PlayPause,
    Previous,
    Next,
    /// Milliseconds, capped to `u32` — same reasoning as
    /// `ui::playback_bar::Message::SeekChanged`.
    SeekChanged(u32),
    SeekReleased,
    /// Restore (unhide + focus) the main window.
    Restore,
}

pub fn view<'a>(
    tokens: Tokens,
    state: &'a PlayerState,
    art: Option<image::Handle>,
) -> Element<'a, Message> {
    let art_widget: Element<'a, Message> = match art {
        Some(handle) => container(image(handle).width(48.0).height(48.0))
            .style(move |_theme: &Theme| art_frame_style(tokens))
            .into(),
        None => container(text(""))
            .width(48.0)
            .height(48.0)
            .style(move |_theme: &Theme| art_frame_style(tokens))
            .into(),
    };

    let title = state
        .current
        .as_ref()
        .map(|t| t.title.as_str())
        .unwrap_or("Nothing playing");
    let artist = state
        .current
        .as_ref()
        .map(|t| t.artists.as_str())
        .unwrap_or("streamboat");
    let info = column![
        text(title).size(tokens.text_sm).color(tokens.text),
        text(artist).size(tokens.text_xs).color(tokens.muted),
    ]
    .spacing(2)
    .width(Length::Fill);

    let restore = button(text("⤢").size(tokens.text_sm))
        .padding(tokens.space_xs)
        .on_press(Message::Restore)
        .style(move |_theme: &Theme, status| icon_button_style(tokens, status));

    let header = row![art_widget, info, restore]
        .spacing(tokens.space_sm)
        .align_y(Alignment::Center);

    let play_pause_label = if state.status == PlaybackStatus::Playing {
        "⏸"
    } else {
        "▶"
    };
    let transport = row![
        transport_button("⏮", Message::Previous, tokens),
        transport_button(play_pause_label, Message::PlayPause, tokens),
        transport_button("⏭", Message::Next, tokens),
    ]
    .spacing(tokens.space_xs)
    .align_y(Alignment::Center);

    let duration_ms = state.duration_ms.unwrap_or(0).max(1);
    let position_ms = state.position_ms.min(duration_ms);
    let duration_u32 = duration_ms.min(u32::MAX as u64) as u32;
    let position_u32 = position_ms.min(u32::MAX as u64) as u32;
    let seek = slider(0..=duration_u32, position_u32, Message::SeekChanged)
        .on_release(Message::SeekReleased)
        .width(Length::Fill)
        .style(move |theme: &Theme, status| {
            // A thin rail — the default
            // handle/rail proportions read as too heavy at this window's
            // scale, so this shrinks the rail width only.
            let mut style = slider::default(theme, status);
            style.rail.width = 3.0;
            style
        });

    let quality = quality_badge(state.stream.as_ref().and_then(|s| s.quality));
    let footer = row![
        transport,
        iced::widget::space::horizontal(),
        text(quality).size(tokens.text_xs).color(tokens.muted),
    ]
    .align_y(Alignment::Center);

    container(
        column![header, seek, footer]
            .spacing(tokens.space_xs)
            .padding(tokens.space_sm),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_theme: &Theme| container::Style {
        background: Some(tokens.surface.into()),
        text_color: Some(tokens.text),
        ..container::Style::default()
    })
    .into()
}

fn transport_button<'a>(label: &'a str, on_press: Message, tokens: Tokens) -> Element<'a, Message> {
    button(text(label).size(tokens.text_md))
        .padding(tokens.space_xs)
        .on_press(on_press)
        .style(move |_theme: &Theme, status| icon_button_style(tokens, status))
        .into()
}

fn icon_button_style(
    tokens: Tokens,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let hovered = matches!(
        status,
        iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
    );
    iced::widget::button::Style {
        background: hovered.then_some(tokens.elevated.into()),
        text_color: tokens.text,
        border: Border {
            radius: tokens.radius_pill.into(),
            ..Border::default()
        },
        ..iced::widget::button::Style::default()
    }
}

fn art_frame_style(tokens: Tokens) -> container::Style {
    container::Style {
        background: Some(tokens.elevated.into()),
        border: Border {
            radius: tokens.radius_sm.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_test::simulator;
    use streamboat_core::models::TrackSummary;

    #[test]
    fn shows_nothing_playing_when_idle() {
        let state = PlayerState::default();
        let tokens = Tokens::dark();
        let mut ui = simulator(view(tokens, &state, None));
        assert!(ui.find("Nothing playing").is_ok());
    }

    #[test]
    fn shows_the_current_track_and_quality_badge() {
        let state = PlayerState {
            current: Some(TrackSummary {
                id: 1,
                title: "A Title".into(),
                artists: "An Artist".into(),
                album: "An Album".into(),
                duration_ms: Some(200_000),
                cover: None,
            }),
            stream: Some(streamboat_core::proto::StreamInfo {
                quality: Some(streamboat_core::AudioQuality::Lossless),
                ..Default::default()
            }),
            ..PlayerState::default()
        };
        let tokens = Tokens::dark();
        let mut ui = simulator(view(tokens, &state, None));
        assert!(ui.find("A Title").is_ok());
        assert!(ui.find("An Artist").is_ok());
        assert!(ui.find("LOSSLESS").is_ok());
    }

    #[test]
    fn the_restore_button_is_clickable() {
        let state = PlayerState::default();
        let tokens = Tokens::dark();
        let mut ui = simulator(view(tokens, &state, None));
        assert!(ui.click("⤢").is_ok());
        let messages: Vec<Message> = ui.into_messages().collect();
        assert_eq!(messages, vec![Message::Restore]);
    }
}
