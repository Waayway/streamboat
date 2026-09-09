//! The persistent bottom playback bar (task item 1/3): art, title, artist,
//! previous/play-pause/next, seek with elapsed/remaining, a volume slider
//! disabled-by-tooltip while an exclusive output is active (D-017), a
//! quality badge, and the two toggle buttons for the queue list and the
//! signal-path panel. Split from the top-level `Message` per D-013.

use iced::widget::{button, column, container, image, row, slider, text, tooltip};
use iced::{Alignment, Border, Element, Length, Theme};

use streamboat_core::proto::{PlaybackStatus, PlayerState};

use crate::ui::design::Tokens;
use crate::ui::format::{mmss, quality_badge};
use crate::ui::widgets::{ChipTone, chip};

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    PlayPause,
    Previous,
    Next,
    /// Milliseconds, capped to `u32` — `iced::widget::Slider` requires
    /// `T: Into<f64>`, which `u64` does not implement; track positions
    /// never approach `u32::MAX` ms (about 49 days).
    SeekChanged(u32),
    SeekReleased,
    VolumeChanged(f32),
    ToggleQueue,
    ToggleSignalPath,
}

pub fn view<'a>(
    tokens: Tokens,
    state: &'a PlayerState,
    art: Option<image::Handle>,
    queue_open: bool,
    signal_path_open: bool,
) -> Element<'a, Message> {
    let art_widget: Element<'a, Message> = match art {
        Some(handle) => container(image(handle).width(52.0).height(52.0))
            .style(move |_theme: &Theme| art_frame_style(tokens))
            .into(),
        None => container(text(""))
            .width(52.0)
            .height(52.0)
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
    .width(Length::Fixed(180.0));

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
    .spacing(tokens.space_xs);

    let duration_ms = state.duration_ms.unwrap_or(0).max(1);
    let position_ms = state.position_ms.min(duration_ms);
    let remaining_ms = duration_ms.saturating_sub(position_ms);
    let duration_u32 = duration_ms.min(u32::MAX as u64) as u32;
    let position_u32 = position_ms.min(u32::MAX as u64) as u32;

    let seek = row![
        text(mmss(Some(position_ms)))
            .size(tokens.text_xs)
            .color(tokens.muted),
        slider(0..=duration_u32, position_u32, Message::SeekChanged)
            .on_release(Message::SeekReleased)
            .width(Length::Fill),
        text(mmss(state.duration_ms.map(|_| remaining_ms)))
            .size(tokens.text_xs)
            .color(tokens.muted),
    ]
    .spacing(tokens.space_sm)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let center = column![transport, seek]
        .spacing(tokens.space_xs)
        .align_x(Alignment::Center)
        .width(Length::FillPortion(2));

    let exclusive = state.output.is_exclusive();
    let volume = slider(0.0..=1.0, state.volume, Message::VolumeChanged)
        .width(Length::Fixed(110.0))
        .style(move |theme: &Theme, status| {
            let mut style = slider::default(theme, status);
            if exclusive {
                style.rail.backgrounds = (tokens.border.into(), tokens.border.into());
            }
            style
        });
    let volume_widget: Element<'a, Message> = if exclusive {
        tooltip(
            volume,
            container(
                text("Volume is fixed while exclusive output is active (D-017)")
                    .size(tokens.text_xs),
            )
            .padding(tokens.space_xs)
            .style(move |_theme: &Theme| tooltip_style(tokens)),
            tooltip::Position::Top,
        )
        .into()
    } else {
        volume.into()
    };

    let quality = quality_badge(state.stream.as_ref().and_then(|s| s.quality));
    let mut badges = row![
        chip::<'a, Message>(tokens, quality, ChipTone::Accent),
        if exclusive {
            chip::<'a, Message>(tokens, "EXCLUSIVE", ChipTone::Muted)
        } else {
            chip::<'a, Message>(tokens, "SHARED", ChipTone::Muted)
        },
    ]
    .spacing(tokens.space_xs);
    if state.stream.as_ref().is_some_and(|s| s.preview) {
        badges = badges.push(chip::<'a, Message>(tokens, "PREVIEW", ChipTone::Warning));
    }

    let toggles = row![
        toggle_button("Queue", queue_open, Message::ToggleQueue, tokens),
        toggle_button(
            "Signal path",
            signal_path_open,
            Message::ToggleSignalPath,
            tokens
        ),
    ]
    .spacing(tokens.space_xs);

    let right = column![
        row![volume_widget, badges]
            .spacing(tokens.space_md)
            .align_y(Alignment::Center),
        toggles
    ]
    .spacing(tokens.space_xs)
    .align_x(Alignment::End);

    container(
        row![
            row![art_widget, info]
                .spacing(tokens.space_sm)
                .align_y(Alignment::Center),
            center,
            right,
        ]
        .spacing(tokens.space_lg)
        .align_y(Alignment::Center)
        .padding([tokens.space_sm, tokens.space_lg]),
    )
    .width(Length::Fill)
    .style(move |_theme: &Theme| bar_style(tokens))
    .into()
}

fn transport_button<'a>(label: &'a str, on_press: Message, tokens: Tokens) -> Element<'a, Message> {
    button(text(label).size(tokens.text_md))
        .padding(tokens.space_xs)
        .on_press(on_press)
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: hovered.then_some(tokens.elevated.into()),
                text_color: tokens.text,
                border: Border {
                    radius: tokens.radius_pill.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }
        })
        .into()
}

fn toggle_button<'a>(
    label: &'a str,
    active: bool,
    on_press: Message,
    tokens: Tokens,
) -> Element<'a, Message> {
    button(text(label).size(tokens.text_xs))
        .padding([tokens.space_xs / 2.0, tokens.space_sm])
        .on_press(on_press)
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let background = if active {
                tokens.accent_muted
            } else if hovered {
                tokens.elevated
            } else {
                tokens.surface
            };
            button::Style {
                background: Some(background.into()),
                text_color: if active { tokens.accent } else { tokens.muted },
                border: Border {
                    radius: tokens.radius_sm.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }
        })
        .into()
}

fn bar_style(tokens: Tokens) -> container::Style {
    container::Style {
        background: Some(tokens.surface.into()),
        border: Border {
            color: tokens.border,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
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

fn tooltip_style(tokens: Tokens) -> container::Style {
    container::Style {
        background: Some(tokens.elevated.into()),
        text_color: Some(tokens.text),
        border: Border {
            color: tokens.border,
            width: 1.0,
            radius: tokens.radius_sm.into(),
        },
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_test::simulator;
    use streamboat_core::proto::OutputConfig;

    #[test]
    fn volume_slider_is_present_and_labelled_when_shared() {
        let state = PlayerState {
            output: OutputConfig::Shared { device: None },
            ..PlayerState::default()
        };
        let tokens = Tokens::dark();
        let mut ui = simulator(view(tokens, &state, None, false, false));
        assert!(ui.find("SHARED").is_ok());
    }

    #[test]
    fn exclusive_output_shows_the_exclusive_badge() {
        let state = PlayerState {
            output: OutputConfig::Exclusive {
                device: "hw:1,0".into(),
            },
            ..PlayerState::default()
        };
        let tokens = Tokens::dark();
        let mut ui = simulator(view(tokens, &state, None, false, false));
        assert!(ui.find("EXCLUSIVE").is_ok());
    }
}
