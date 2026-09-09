//! Settings: quality ceiling, output device + exclusive
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

/// `streamboat_player::snapcast` holds the fixed *audio* format
/// (48000/16/stereo) both engine backends target, but not a default
/// host/port — a snapserver address is always this screen's own field, so
/// the fallback lives here instead, matching README.md's own example
/// (`docs/architecture.md` "Multiroom: Snapcast output").
const DEFAULT_SNAPCAST_HOST: &str = "127.0.0.1";
const DEFAULT_SNAPCAST_PORT: u16 = 4953;

/// The three output modes this screen offers (D-034): `OutputConfig` itself
/// has no `Display`/pick-list-friendly shape, so this mirrors its variants
/// one-for-one purely for the picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Shared,
    Exclusive,
    Snapcast,
}

impl OutputMode {
    pub const ALL: [OutputMode; 3] = [
        OutputMode::Shared,
        OutputMode::Exclusive,
        OutputMode::Snapcast,
    ];
}

impl std::fmt::Display for OutputMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            OutputMode::Shared => "Shared",
            OutputMode::Exclusive => "Exclusive (bit-perfect)",
            OutputMode::Snapcast => "Snapcast (multiroom)",
        })
    }
}

pub struct State {
    quality_ceiling: AudioQuality,
    output_mode: OutputMode,
    output_device: String,
    snapcast_host: String,
    snapcast_port: String,
    replay_gain_mode: ReplayGainMode,
    play_reporting_enabled: bool,
    theme: ThemePreference,
    client_id: String,
    client_secret: String,
    pkce_client_id: String,
    pkce_client_secret: String,
    key_storage: KeyStorage,
    key_location: String,
    /// Quality tiers this build cannot decode (D-003 decoder probe), left
    /// out of the ceiling picker and named under it — iced's `pick_list` has
    /// no per-item disabled state, so "greyed out" is "not offered, with the
    /// reason shown."
    unreachable: Vec<AudioQuality>,
    saved: bool,
    error: Option<String>,
}

impl State {
    pub fn from_settings(settings: &Settings, key_location: String) -> Self {
        let (output_mode, output_device, snapcast_host, snapcast_port) =
            match settings.output.clone().unwrap_or_default() {
                OutputConfig::Exclusive { device } => (
                    OutputMode::Exclusive,
                    device,
                    DEFAULT_SNAPCAST_HOST.to_string(),
                    DEFAULT_SNAPCAST_PORT.to_string(),
                ),
                OutputConfig::Shared { device } => (
                    OutputMode::Shared,
                    device.unwrap_or_default(),
                    DEFAULT_SNAPCAST_HOST.to_string(),
                    DEFAULT_SNAPCAST_PORT.to_string(),
                ),
                OutputConfig::Snapcast { host, port } => {
                    (OutputMode::Snapcast, String::new(), host, port.to_string())
                }
            };
        Self {
            quality_ceiling: settings.quality_ceiling(),
            output_mode,
            output_device,
            snapcast_host,
            snapcast_port,
            replay_gain_mode: settings.replay_gain_mode,
            play_reporting_enabled: settings.play_reporting,
            theme: settings.theme,
            client_id: settings.client_id.clone().unwrap_or_default(),
            client_secret: settings.client_secret.clone().unwrap_or_default(),
            pkce_client_id: settings.pkce_client_id.clone().unwrap_or_default(),
            pkce_client_secret: settings.pkce_client_secret.clone().unwrap_or_default(),
            key_storage: settings.key_storage,
            key_location,
            unreachable: Vec::new(),
            saved: true,
            error: None,
        }
    }

    /// Record which tiers the startup decoder probe found unreachable
    /// (D-003), so the picker only offers what this build can play.
    pub fn with_decoder_support(mut self, support: streamboat_player::DecoderSupport) -> Self {
        self.unreachable = AudioQuality::LADDER
            .into_iter()
            .filter(|q| !support.is_reachable(*q))
            .collect();
        self
    }

    /// The tiers offered by the ceiling picker: everything the probe found
    /// decodable, plus the current setting itself so a persisted choice this
    /// build cannot reach still shows (capped at startup with a warning).
    fn offered_tiers(&self) -> Vec<AudioQuality> {
        AudioQuality::LADDER
            .into_iter()
            .filter(|q| !self.unreachable.contains(q) || *q == self.quality_ceiling)
            .collect()
    }

    /// Writes the edited fields back into `settings`, returning the
    /// [`OutputConfig`] to apply live through the player link.
    pub fn apply_to(&self, settings: &mut Settings) -> OutputConfig {
        settings.quality_ceiling = Some(self.quality_ceiling);
        let output = match self.output_mode {
            OutputMode::Exclusive => OutputConfig::Exclusive {
                device: self.output_device.clone(),
            },
            OutputMode::Shared => OutputConfig::Shared {
                device: (!self.output_device.is_empty()).then(|| self.output_device.clone()),
            },
            OutputMode::Snapcast => OutputConfig::Snapcast {
                host: {
                    let host = self.snapcast_host.trim();
                    if host.is_empty() {
                        DEFAULT_SNAPCAST_HOST.to_string()
                    } else {
                        host.to_string()
                    }
                },
                port: self
                    .snapcast_port
                    .trim()
                    .parse()
                    .unwrap_or(DEFAULT_SNAPCAST_PORT),
            },
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
    OutputModeChanged(OutputMode),
    OutputDeviceChanged(String),
    SnapcastHostChanged(String),
    SnapcastPortChanged(String),
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
            Message::OutputModeChanged(mode) => {
                self.output_mode = mode;
                self.saved = false;
                (Task::none(), None)
            }
            Message::OutputDeviceChanged(v) => {
                self.output_device = v;
                self.saved = false;
                (Task::none(), None)
            }
            Message::SnapcastHostChanged(v) => {
                self.snapcast_host = v;
                self.saved = false;
                (Task::none(), None)
            }
            Message::SnapcastPortChanged(v) => {
                self.snapcast_port = v;
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

    /// The fields specific to the chosen [`OutputMode`]: a device string for
    /// Shared/Exclusive, or a snapserver host/port plus the D-034
    /// mutual-exclusivity note for Snapcast — never both at once, since the
    /// two are different `OutputConfig` variants a user picks between, not
    /// independent toggles.
    fn output_fields<'a>(&'a self, tokens: Tokens) -> Element<'a, Message> {
        match self.output_mode {
            OutputMode::Shared | OutputMode::Exclusive => labelled(
                tokens,
                "Output device",
                text_input(
                    "e.g. hw:1,0 (leave blank for the default device)",
                    &self.output_device,
                )
                .on_input(Message::OutputDeviceChanged),
            ),
            OutputMode::Snapcast => column![
                labelled(
                    tokens,
                    "Snapserver host",
                    text_input(DEFAULT_SNAPCAST_HOST, &self.snapcast_host)
                        .on_input(Message::SnapcastHostChanged),
                ),
                labelled(
                    tokens,
                    "Snapserver port",
                    text_input(&DEFAULT_SNAPCAST_PORT.to_string(), &self.snapcast_port)
                        .on_input(Message::SnapcastPortChanged),
                ),
                text(
                    "Snapcast resamples every track to a fixed 48000 Hz / 16-bit / stereo PCM \
                     stream for snapserver to distribute; it is mutually exclusive with \
                     bit-perfect output (D-034)."
                )
                .size(tokens.text_xs)
                .color(tokens.muted),
            ]
            .spacing(tokens.space_sm)
            .into(),
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
                        self.offered_tiers(),
                        Some(self.quality_ceiling),
                        Message::QualityChanged,
                    )
                ),
                text(if self.unreachable.is_empty() {
                    "Every tier is decodable by this build.".to_string()
                } else {
                    format!(
                        "Not decodable by this build, so not offered: {} (no matching decoder; \
                         the ceiling is capped at startup).",
                        self.unreachable
                            .iter()
                            .map(|q| q.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
                .size(tokens.text_sm)
                .color(tokens.muted),
                labelled(
                    tokens,
                    "Output mode",
                    pick_list(
                        OutputMode::ALL.to_vec(),
                        Some(self.output_mode),
                        Message::OutputModeChanged,
                    ),
                ),
                self.output_fields(tokens),
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
    fn decoder_probe_drops_unreachable_tiers_from_the_picker() {
        let settings = Settings {
            quality_ceiling: Some(AudioQuality::High),
            ..Settings::default()
        };
        let no_flac = streamboat_player::DecoderSupport {
            low: true,
            high: true,
            lossless: false,
            hi_res_lossless: false,
        };
        let state = State::from_settings(&settings, "file".into()).with_decoder_support(no_flac);
        assert_eq!(
            state.unreachable,
            vec![AudioQuality::HiResLossless, AudioQuality::Lossless]
        );
        assert_eq!(
            state.offered_tiers(),
            vec![AudioQuality::High, AudioQuality::Low]
        );

        // A persisted ceiling this build cannot reach still shows, so the
        // user sees what was asked for rather than a silently changed value.
        let persisted = Settings {
            quality_ceiling: Some(AudioQuality::Lossless),
            ..Settings::default()
        };
        let state = State::from_settings(&persisted, "file".into()).with_decoder_support(no_flac);
        assert!(state.offered_tiers().contains(&AudioQuality::Lossless));
        assert!(!state.offered_tiers().contains(&AudioQuality::HiResLossless));
    }

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
        assert_eq!(state.output_mode, OutputMode::Exclusive);
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
    fn snapcast_output_round_trips_through_apply_to() {
        let settings = Settings {
            output: Some(OutputConfig::Snapcast {
                host: "192.168.1.50".into(),
                port: 4953,
            }),
            ..Settings::default()
        };
        let state = State::from_settings(&settings, "file".into());
        assert_eq!(state.output_mode, OutputMode::Snapcast);
        assert_eq!(state.snapcast_host, "192.168.1.50");
        assert_eq!(state.snapcast_port, "4953");
        // Snapcast carries no device string of its own.
        assert_eq!(state.output_device, "");

        let mut round_tripped = Settings::default();
        let output = state.apply_to(&mut round_tripped);
        assert_eq!(
            output,
            OutputConfig::Snapcast {
                host: "192.168.1.50".into(),
                port: 4953,
            }
        );
        assert_eq!(round_tripped.output, Some(output));
    }

    #[test]
    fn snapcast_output_falls_back_to_documented_defaults_when_blank() {
        // A user who switches into Snapcast mode without editing the
        // pre-filled fields, or clears them by hand, must still get
        // README's documented default rather than an invalid host/port.
        let mut state = State::from_settings(&Settings::default(), "file".into());
        state.output_mode = OutputMode::Snapcast;
        state.snapcast_host = "  ".into();
        state.snapcast_port = "not a port".into();

        let mut settings = Settings::default();
        let output = state.apply_to(&mut settings);
        assert_eq!(
            output,
            OutputConfig::Snapcast {
                host: DEFAULT_SNAPCAST_HOST.into(),
                port: DEFAULT_SNAPCAST_PORT,
            }
        );
    }

    #[test]
    fn play_reporting_defaults_to_enabled_per_d027() {
        let settings = Settings::default();
        assert!(settings.play_reporting);
    }

    mod view_tests {
        use super::*;
        use iced_test::simulator;

        #[test]
        fn snapcast_fields_render_only_in_snapcast_mode() {
            let tokens = Tokens::dark();

            let shared = State::from_settings(&Settings::default(), "file".into());
            let mut ui = simulator(shared.view(tokens));
            assert!(ui.find("Output device").is_ok());
            assert!(ui.find("Snapserver host").is_err());
            assert!(ui.find("Snapserver port").is_err());

            let snapcast_settings = Settings {
                output: Some(OutputConfig::Snapcast {
                    host: "192.168.1.50".into(),
                    port: 4953,
                }),
                ..Settings::default()
            };
            let snapcast = State::from_settings(&snapcast_settings, "file".into());
            let mut ui = simulator(snapcast.view(tokens));
            assert!(ui.find("Snapserver host").is_ok());
            assert!(ui.find("Snapserver port").is_ok());
            assert!(ui.find("Output device").is_err());
        }
    }
}
