//! Settings (task item 3): quality ceiling, output device + exclusive
//! toggle, ReplayGain mode, play reporting toggle with the D-027 disclosure
//! text, credentials, key storage, theme choice, logout.

use iced::widget::{button, checkbox, column, pick_list, row, text, text_input};
use iced::{Element, Length, Task, Theme};

use streamboat_core::config::{ReplayGainMode, Settings, ThemePreference};
use streamboat_core::proto::OutputConfig;
use streamboat_core::token_store::KeyStorage;
use streamboat_core::{ApiClient, AudioQuality};

use crate::ui::design::Tokens;
use crate::ui::widgets::{ChipTone, banner};

pub struct State {
    quality_ceiling: AudioQuality,
    output_device: String,
    exclusive: bool,
    replay_gain_mode: ReplayGainMode,
    play_reporting_enabled: bool,
    theme: ThemePreference,
    client_id: String,
    client_secret: String,
    pkce_client_id: String,
    pkce_client_secret: String,
    key_storage: KeyStorage,
    key_location: String,
    saved: bool,
    error: Option<String>,
}

impl State {
    pub fn from_settings(settings: &Settings, key_location: String) -> Self {
        let (exclusive, device) = match settings.output.clone().unwrap_or_default() {
            OutputConfig::Exclusive { device } => (true, device),
            OutputConfig::Shared { device } => (false, device.unwrap_or_default()),
        };
        Self {
            quality_ceiling: settings.quality_ceiling(),
            output_device: device,
            exclusive,
            replay_gain_mode: settings.replay_gain_mode,
            play_reporting_enabled: settings.play_reporting,
            theme: settings.theme,
            client_id: settings.client_id.clone().unwrap_or_default(),
            client_secret: settings.client_secret.clone().unwrap_or_default(),
            pkce_client_id: settings.pkce_client_id.clone().unwrap_or_default(),
            pkce_client_secret: settings.pkce_client_secret.clone().unwrap_or_default(),
            key_storage: settings.key_storage,
            key_location,
            saved: true,
            error: None,
        }
    }

    /// Writes the edited fields back into `settings`, returning the
    /// [`OutputConfig`] to apply live through the player link.
    pub fn apply_to(&self, settings: &mut Settings) -> OutputConfig {
        settings.quality_ceiling = Some(self.quality_ceiling);
        let output = if self.exclusive {
            OutputConfig::Exclusive {
                device: self.output_device.clone(),
            }
        } else {
            OutputConfig::Shared {
                device: (!self.output_device.is_empty()).then(|| self.output_device.clone()),
            }
        };
        settings.output = Some(output.clone());
        settings.replay_gain_mode = self.replay_gain_mode;
        settings.play_reporting = self.play_reporting_enabled;
        settings.theme = self.theme;
        settings.client_id = non_empty(&self.client_id);
        settings.client_secret = non_empty(&self.client_secret);
        settings.pkce_client_id = non_empty(&self.pkce_client_id);
        settings.pkce_client_secret = non_empty(&self.pkce_client_secret);
        settings.key_storage = self.key_storage;
        output
    }
}

fn non_empty(s: &str) -> Option<String> {
    (!s.trim().is_empty()).then(|| s.trim().to_string())
}

#[derive(Debug, Clone)]
pub enum Message {
    QualityChanged(AudioQuality),
    OutputDeviceChanged(String),
    ExclusiveToggled(bool),
    ReplayGainChanged(ReplayGainMode),
    PlayReportingToggled(bool),
    ThemeChanged(ThemePreference),
    ClientIdChanged(String),
    ClientSecretChanged(String),
    PkceClientIdChanged(String),
    PkceClientSecretChanged(String),
    KeyStorageChanged(KeyStorage),
    Save,
    Saved(Result<(), String>),
    Logout,
    LoggedOut,
}

/// What `ui::app` must do beyond this screen's own edited fields: persist
/// them, apply the output live, react to a theme change, or drop back to
/// Login after a successful logout.
pub enum Effect {
    /// Persist the edited fields: `ui::app` calls [`State::apply_to`] on the
    /// canonical `Settings`, writes it to disk, and applies the resulting
    /// [`OutputConfig`]/quality ceiling live through the player link.
    Save,
    ThemeChanged(ThemePreference),
    LoggedOut,
}

impl State {
    pub fn update(&mut self, message: Message, api: &ApiClient) -> (Task<Message>, Option<Effect>) {
        match message {
            Message::QualityChanged(q) => {
                self.quality_ceiling = q;
                self.saved = false;
                (Task::none(), None)
            }
            Message::OutputDeviceChanged(v) => {
                self.output_device = v;
                self.saved = false;
                (Task::none(), None)
            }
            Message::ExclusiveToggled(v) => {
                self.exclusive = v;
                self.saved = false;
                (Task::none(), None)
            }
            Message::ReplayGainChanged(mode) => {
                self.replay_gain_mode = mode;
                self.saved = false;
                (Task::none(), None)
            }
            Message::PlayReportingToggled(v) => {
                self.play_reporting_enabled = v;
                self.saved = false;
                (Task::none(), None)
            }
            Message::ThemeChanged(theme) => {
                self.theme = theme;
                self.saved = false;
                (Task::none(), Some(Effect::ThemeChanged(theme)))
            }
            Message::ClientIdChanged(v) => {
                self.client_id = v;
                self.saved = false;
                (Task::none(), None)
            }
            Message::ClientSecretChanged(v) => {
                self.client_secret = v;
                self.saved = false;
                (Task::none(), None)
            }
            Message::PkceClientIdChanged(v) => {
                self.pkce_client_id = v;
                self.saved = false;
                (Task::none(), None)
            }
            Message::PkceClientSecretChanged(v) => {
                self.pkce_client_secret = v;
                self.saved = false;
                (Task::none(), None)
            }
            Message::KeyStorageChanged(v) => {
                self.key_storage = v;
                self.saved = false;
                (Task::none(), None)
            }
            Message::Save => {
                self.saved = true;
                self.error = None;
                (Task::none(), Some(Effect::Save))
            }
            Message::Saved(Ok(())) => (Task::none(), None),
            Message::Saved(Err(e)) => {
                self.error = Some(e);
                (Task::none(), None)
            }
            Message::Logout => {
                let api = api.clone();
                (
                    Task::perform(logout(api), |r| match r {
                        Ok(()) => Message::LoggedOut,
                        Err(e) => Message::Saved(Err(e)),
                    }),
                    None,
                )
            }
            Message::LoggedOut => (Task::none(), Some(Effect::LoggedOut)),
        }
    }

    pub fn view<'a>(&'a self, tokens: Tokens) -> Element<'a, Message> {
        let mut col = column![text("Settings").size(tokens.text_xl).color(tokens.text),]
            .spacing(tokens.space_lg)
            .width(Length::Fixed(560.0));

        if let Some(e) = &self.error {
            col = col.push(banner(tokens, e.clone(), ChipTone::Danger));
        }

        col = col.push(field_group(
            tokens,
            "Playback",
            column![
                labelled(
                    tokens,
                    "Quality ceiling",
                    pick_list(
                        AudioQuality::LADDER.to_vec(),
                        Some(self.quality_ceiling),
                        Message::QualityChanged,
                    )
                ),
                labelled(
                    tokens,
                    "Output device",
                    text_input(
                        "e.g. hw:1,0 (leave blank for the default device)",
                        &self.output_device
                    )
                    .on_input(Message::OutputDeviceChanged),
                ),
                checkbox(self.exclusive)
                    .label("Exclusive (bit-perfect) output")
                    .on_toggle(Message::ExclusiveToggled),
                labelled(
                    tokens,
                    "ReplayGain",
                    pick_list(
                        ReplayGainMode::ALL.to_vec(),
                        Some(self.replay_gain_mode),
                        Message::ReplayGainChanged,
                    )
                ),
            ]
            .spacing(tokens.space_sm),
        ));

        col = col.push(field_group(
            tokens,
            "Privacy",
            column![
                checkbox(self.play_reporting_enabled)
                    .label("Report what I play to TIDAL")
                    .on_toggle(Message::PlayReportingToggled),
                text(
                    "Play reporting sends TIDAL the track and timestamp after 30 seconds of \
                     playback so your history and radio/mix recommendations stay accurate \
                     (D-027). streamboat never reports a preview asset. Turn this off if you \
                     don't want that."
                )
                .size(tokens.text_xs)
                .color(tokens.muted),
            ]
            .spacing(tokens.space_xs),
        ));

        col = col.push(field_group(
            tokens,
            "Appearance",
            labelled(
                tokens,
                "Theme",
                pick_list(
                    ThemePreference::ALL.to_vec(),
                    Some(self.theme),
                    Message::ThemeChanged,
                ),
            ),
        ));

        col = col.push(field_group(
            tokens,
            "Credentials",
            column![
                labelled(
                    tokens,
                    "Device-code client id",
                    text_input("", &self.client_id).on_input(Message::ClientIdChanged),
                ),
                labelled(
                    tokens,
                    "Device-code client secret",
                    text_input("", &self.client_secret)
                        .secure(true)
                        .on_input(Message::ClientSecretChanged),
                ),
                labelled(
                    tokens,
                    "PKCE client id",
                    text_input("", &self.pkce_client_id).on_input(Message::PkceClientIdChanged),
                ),
                labelled(
                    tokens,
                    "PKCE client secret",
                    text_input("", &self.pkce_client_secret)
                        .secure(true)
                        .on_input(Message::PkceClientSecretChanged),
                ),
                text(
                    "Any client id embedded in a binary is extractable, and TIDAL can revoke \
                     one at any time (D-023)."
                )
                .size(tokens.text_xs)
                .color(tokens.muted),
            ]
            .spacing(tokens.space_sm),
        ));

        col = col.push(field_group(
            tokens,
            "Key storage",
            column![
                labelled(
                    tokens,
                    "Token-file master key",
                    pick_list(
                        vec![KeyStorage::Auto, KeyStorage::Keyring, KeyStorage::File],
                        Some(self.key_storage),
                        Message::KeyStorageChanged,
                    ),
                ),
                text(format!("Currently: {}", self.key_location))
                    .size(tokens.text_xs)
                    .color(tokens.muted),
            ]
            .spacing(tokens.space_xs),
        ));

        col = col.push(
            row![
                save_button(tokens, self.saved),
                button(text("Log out")).on_press(Message::Logout).style(
                    move |_theme: &Theme, status| {
                        let hovered = matches!(status, iced::widget::button::Status::Hovered);
                        iced::widget::button::Style {
                            background: Some(
                                (if hovered {
                                    tokens.danger
                                } else {
                                    tokens.surface
                                })
                                .into(),
                            ),
                            text_color: if hovered {
                                tokens.background
                            } else {
                                tokens.danger
                            },
                            border: iced::Border {
                                color: tokens.danger,
                                width: 1.0,
                                radius: tokens.radius_sm.into(),
                            },
                            ..iced::widget::button::Style::default()
                        }
                    }
                ),
            ]
            .spacing(tokens.space_sm),
        );

        crate::ui::widgets::page(tokens, iced::widget::scrollable(col))
    }
}

fn save_button<'a>(tokens: Tokens, saved: bool) -> Element<'a, Message> {
    button(text(if saved { "Saved" } else { "Save" }))
        .on_press(Message::Save)
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, iced::widget::button::Status::Hovered);
            iced::widget::button::Style {
                background: Some(
                    (if hovered {
                        tokens.accent_hover()
                    } else {
                        tokens.accent
                    })
                    .into(),
                ),
                text_color: tokens.background,
                border: iced::Border {
                    radius: tokens.radius_sm.into(),
                    ..iced::Border::default()
                },
                ..iced::widget::button::Style::default()
            }
        })
        .into()
}

fn field_group<'a>(
    tokens: Tokens,
    title: &'a str,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    column![
        text(title).size(tokens.text_md).color(tokens.text),
        content.into(),
    ]
    .spacing(tokens.space_sm)
    .into()
}

fn labelled<'a>(
    tokens: Tokens,
    label: &'a str,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    column![
        text(label).size(tokens.text_xs).color(tokens.muted),
        content.into(),
    ]
    .spacing(2)
    .into()
}

async fn logout(api: ApiClient) -> Result<(), String> {
    api.logout().await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_settings_round_trips_through_apply_to() {
        let settings = Settings {
            quality_ceiling: Some(AudioQuality::Lossless),
            output: Some(OutputConfig::Exclusive {
                device: "hw:1,0".into(),
            }),
            ..Settings::default()
        };
        let state = State::from_settings(&settings, "keyring".into());
        assert_eq!(state.quality_ceiling, AudioQuality::Lossless);
        assert!(state.exclusive);
        assert_eq!(state.output_device, "hw:1,0");

        let mut round_tripped = Settings::default();
        let output = state.apply_to(&mut round_tripped);
        assert_eq!(
            output,
            OutputConfig::Exclusive {
                device: "hw:1,0".into()
            }
        );
        assert_eq!(round_tripped.quality_ceiling, Some(AudioQuality::Lossless));
    }

    #[test]
    fn play_reporting_defaults_to_enabled_per_d027() {
        let settings = Settings::default();
        assert!(settings.play_reporting);
    }
}
