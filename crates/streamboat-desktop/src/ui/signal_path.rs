//! The signal-path panel (D-036): renders [`SignalPath`] from
//! [`PlayerState`] exactly as the engine reports it — source format, device
//! format, exclusive/shared, ReplayGain state, the converted flag, and the
//! "lossy source, bit-perfect not applicable" line for AAC. Every field is
//! `None` when the active engine cannot observe it; this panel says so
//! rather than guessing, per D-036's "must report only what the active
//! engine can actually observe."

use iced::widget::{column, container, row, text};
use iced::{Border, Element, Length, Theme};

use streamboat_core::proto::{PlayerState, SignalPath};

use crate::ui::design::Tokens;
use crate::ui::format::{audio_mode_label, bit_perfect_applicable};

/// This panel has no interactive elements yet (a close button lives on the
/// playback bar's toggle instead), but keeps its own empty `Message` type
/// per D-013's "split messages per screen/module" so a future control
/// (e.g. "copy debug info") slots in without touching the top-level enum's
/// shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {}

pub fn view<'a>(tokens: Tokens, state: &'a PlayerState) -> Element<'a, Message> {
    let Some(sp) = state.signal_path.as_ref() else {
        return panel(
            tokens,
            column![
                text("Nothing playing.")
                    .size(tokens.text_sm)
                    .color(tokens.muted)
            ],
        );
    };

    let mut rows: Vec<Element<'a, Message>> = vec![
        field(tokens, "Engine", sp.engine.clone()),
        field(tokens, "Source format", opt(&sp.source_format)),
        field(tokens, "Decoder", opt(&sp.decoder)),
        field(tokens, "Sink", opt(&sp.sink)),
        field(tokens, "Device", opt(&sp.device)),
        field(tokens, "Device format", opt(&sp.device_format)),
        field(
            tokens,
            "Audio mode",
            audio_mode_label(state.stream.as_ref().and_then(|s| s.audio_mode)).to_string(),
        ),
        field(
            tokens,
            "Mode",
            if sp.exclusive {
                "Exclusive".into()
            } else {
                "Shared".into()
            },
        ),
        field(
            tokens,
            "Converted",
            sp.converted.clone().unwrap_or_else(|| "No".into()),
        ),
        field(tokens, "Volume applied", yes_no(sp.volume_applied)),
        field(tokens, "ReplayGain applied", yes_no(sp.replaygain_applied)),
    ];

    rows.push(bit_perfect_row(tokens, sp, state));

    panel(tokens, column(rows).spacing(tokens.space_xs))
}

fn bit_perfect_row<'a>(
    tokens: Tokens,
    sp: &SignalPath,
    state: &PlayerState,
) -> Element<'a, Message> {
    let codec = state.stream.as_ref().and_then(|s| s.codec.clone());
    let quality = state.stream.as_ref().and_then(|s| s.quality);
    if !bit_perfect_applicable(&codec, quality) {
        return field(
            tokens,
            "Bit-perfect",
            "lossy source, bit-perfect not applicable".to_string(),
        );
    }
    field(
        tokens,
        "Bit-perfect",
        match sp.bit_perfect {
            Some(true) => "Yes".to_string(),
            Some(false) => "No".to_string(),
            None => "Unknown (engine does not report this)".to_string(),
        },
    )
}

fn field<'a>(tokens: Tokens, label: &'a str, value: String) -> Element<'a, Message> {
    row![
        text(label)
            .size(tokens.text_sm)
            .color(tokens.muted)
            .width(Length::Fixed(140.0)),
        text(value).size(tokens.text_sm).color(tokens.text),
    ]
    .spacing(tokens.space_sm)
    .into()
}

fn opt(value: &Option<String>) -> String {
    value.clone().unwrap_or_else(|| "Unknown".to_string())
}

fn yes_no(value: bool) -> String {
    if value {
        "Yes".to_string()
    } else {
        "No".to_string()
    }
}

fn panel<'a>(tokens: Tokens, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(
        column![
            text("Signal path").size(tokens.text_md).color(tokens.text),
            content.into(),
        ]
        .spacing(tokens.space_sm),
    )
    .padding(tokens.space_md)
    .width(Length::Fixed(320.0))
    .style(move |_theme: &Theme| container::Style {
        background: Some(tokens.surface.into()),
        border: Border {
            color: tokens.border,
            width: 1.0,
            radius: tokens.radius_md.into(),
        },
        ..container::Style::default()
    })
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_test::simulator;

    fn aac_state() -> PlayerState {
        PlayerState {
            signal_path: Some(SignalPath {
                engine: "gstreamer".into(),
                source_format: Some("AAC 44100 Hz".into()),
                device_format: Some("44100 Hz / 16-bit".into()),
                exclusive: false,
                bit_perfect: Some(false),
                ..SignalPath::default()
            }),
            stream: Some(streamboat_core::proto::StreamInfo {
                codec: Some("AAC".into()),
                quality: Some(streamboat_core::AudioQuality::High),
                ..streamboat_core::proto::StreamInfo::default()
            }),
            ..PlayerState::default()
        }
    }

    #[test]
    fn aac_shows_bit_perfect_not_applicable() {
        let tokens = Tokens::dark();
        let state = aac_state();
        let mut ui = simulator(view(tokens, &state));
        assert!(ui.find("lossy source, bit-perfect not applicable").is_ok());
    }

    #[test]
    fn nothing_playing_renders_a_placeholder() {
        let tokens = Tokens::dark();
        let state = PlayerState::default();
        let mut ui = simulator(view(tokens, &state));
        assert!(ui.find("Nothing playing.").is_ok());
    }
}
