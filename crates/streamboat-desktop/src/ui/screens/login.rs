//! Login (task item 3): device-code flow with the code, verification URL,
//! an "open browser" button and polling; PKCE flow with either the paste or
//! the loopback capture, reusing the exact auth helpers the CLI already
//! uses (`streamboat_core::auth::{device_code, pkce}`). Errors show inline.

use std::sync::Arc;
use std::time::Duration;

use iced::widget::{button, column, container, row, text, text_input};
use iced::{Element, Length, Task, Theme};

use streamboat_core::auth::device_code::{self, DeviceAuthorization};
use streamboat_core::auth::pkce::{self, PkceSession};
use streamboat_core::token_store::TokenSet;
use streamboat_core::{ApiClient, Error};

use crate::ui::design::Tokens;
use crate::ui::widgets::{ChipTone, banner};

const LOOPBACK_PORT: u16 = 17893;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    DeviceCode,
    PkcePaste,
    PkceLoopback,
}

pub struct State {
    mode: Mode,
    device_auth: Option<DeviceAuthorization>,
    pkce: Option<Arc<PkceSession>>,
    paste_input: String,
    error: Option<String>,
    busy: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            mode: Mode::DeviceCode,
            device_auth: None,
            pkce: None,
            paste_input: String::new(),
            error: None,
            busy: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    SelectMode(Mode),
    StartFlow,
    DeviceAuthReady(Result<DeviceAuthorization, String>),
    PkceReady(Result<Arc<PkceSession>, String>),
    OpenBrowser,
    PasteInputChanged(String),
    SubmitPaste,
    LoopbackCodeReady(Result<String, String>),
    LoginFinished(Result<TokenSet, String>),
}

/// Reported back to `ui::app` so it can switch screens and start loading
/// Home/Explore once a login actually succeeds.
pub enum Effect {
    LoggedIn,
}

impl State {
    pub fn update(
        &mut self,
        message: Message,
        api: &ApiClient,
        client_unique_key: &str,
    ) -> (Task<Message>, Option<Effect>) {
        match message {
            Message::SelectMode(mode) => {
                self.mode = mode;
                self.error = None;
                (Task::none(), None)
            }
            Message::StartFlow => {
                self.error = None;
                self.busy = true;
                match self.mode {
                    Mode::DeviceCode => {
                        let api = api.clone();
                        (
                            Task::perform(start_device_flow(api), Message::DeviceAuthReady),
                            None,
                        )
                    }
                    Mode::PkcePaste | Mode::PkceLoopback => {
                        let redirect = (self.mode == Mode::PkceLoopback)
                            .then(|| format!("http://127.0.0.1:{LOOPBACK_PORT}/callback"));
                        let api = api.clone();
                        let key = client_unique_key.to_string();
                        (
                            Task::perform(start_pkce(api, key, redirect), Message::PkceReady),
                            None,
                        )
                    }
                }
            }
            Message::DeviceAuthReady(Ok(auth)) => {
                self.busy = false;
                self.device_auth = Some(auth.clone());
                let api = api.clone();
                (
                    Task::perform(
                        async move { device_code::wait_for_device_token(&api, &auth, || true).await },
                        |r| Message::LoginFinished(r.map_err(|e| e.to_string())),
                    ),
                    None,
                )
            }
            Message::DeviceAuthReady(Err(e)) => {
                self.busy = false;
                self.error = Some(e);
                (Task::none(), None)
            }
            Message::PkceReady(Ok(session)) => {
                self.busy = false;
                let should_capture_loopback = self.mode == Mode::PkceLoopback;
                self.pkce = Some(session.clone());
                if should_capture_loopback {
                    self.busy = true;
                    (
                        Task::perform(capture_loopback(), Message::LoopbackCodeReady),
                        None,
                    )
                } else {
                    (Task::none(), None)
                }
            }
            Message::PkceReady(Err(e)) => {
                self.busy = false;
                self.error = Some(e);
                (Task::none(), None)
            }
            Message::OpenBrowser => {
                let url = self
                    .device_auth
                    .as_ref()
                    .map(|a| a.verification_url())
                    .or_else(|| self.pkce.as_ref().map(|p| p.authorize_url.clone()));
                if let Some(url) = url {
                    crate::ui::open_browser(&url);
                }
                (Task::none(), None)
            }
            Message::PasteInputChanged(value) => {
                self.paste_input = value;
                (Task::none(), None)
            }
            Message::SubmitPaste => {
                let Some(session) = self.pkce.clone() else {
                    return (Task::none(), None);
                };
                match pkce::code_from_redirect(&self.paste_input) {
                    Ok(code) => {
                        self.busy = true;
                        self.error = None;
                        let api = api.clone();
                        (
                            Task::perform(
                                async move { session.exchange(&api, &code).await },
                                |r| Message::LoginFinished(r.map_err(|e| e.to_string())),
                            ),
                            None,
                        )
                    }
                    Err(e) => {
                        self.error = Some(e.to_string());
                        (Task::none(), None)
                    }
                }
            }
            Message::LoopbackCodeReady(Ok(code)) => {
                let Some(session) = self.pkce.clone() else {
                    return (Task::none(), None);
                };
                let api = api.clone();
                (
                    Task::perform(async move { session.exchange(&api, &code).await }, |r| {
                        Message::LoginFinished(r.map_err(|e| e.to_string()))
                    }),
                    None,
                )
            }
            Message::LoopbackCodeReady(Err(e)) => {
                self.busy = false;
                self.error = Some(e);
                (Task::none(), None)
            }
            Message::LoginFinished(Ok(_tokens)) => {
                self.busy = false;
                self.error = None;
                (Task::none(), Some(Effect::LoggedIn))
            }
            Message::LoginFinished(Err(e)) => {
                self.busy = false;
                self.error = Some(e);
                (Task::none(), None)
            }
        }
    }

    pub fn view<'a>(&'a self, tokens: Tokens) -> Element<'a, Message> {
        let mode_row = row![
            mode_button(tokens, "Device code", Mode::DeviceCode, self.mode),
            mode_button(tokens, "Browser (paste)", Mode::PkcePaste, self.mode),
            mode_button(tokens, "Browser (loopback)", Mode::PkceLoopback, self.mode),
        ]
        .spacing(tokens.space_sm);

        let mut content = column![
            text("Log in to TIDAL")
                .size(tokens.text_xl)
                .color(tokens.text),
            text(
                "streamboat plays streams your own paid TIDAL subscription is entitled to; it \
                 never bypasses TIDAL's access controls."
            )
            .size(tokens.text_sm)
            .color(tokens.muted),
            mode_row,
        ]
        .spacing(tokens.space_md)
        .width(Length::Fixed(480.0));

        if let Some(error) = &self.error {
            content = content.push(banner(tokens, error.clone(), ChipTone::Danger));
        }

        content = content.push(match self.mode {
            Mode::DeviceCode => self.device_code_body(tokens),
            Mode::PkcePaste => self.pkce_paste_body(tokens),
            Mode::PkceLoopback => self.pkce_loopback_body(tokens),
        });

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(move |_theme: &Theme| iced::widget::container::Style {
                background: Some(tokens.background.into()),
                text_color: Some(tokens.text),
                ..iced::widget::container::Style::default()
            })
            .into()
    }

    fn device_code_body<'a>(&'a self, tokens: Tokens) -> Element<'a, Message> {
        if let Some(auth) = &self.device_auth {
            column![
                text("Open this link and enter the code:")
                    .size(tokens.text_sm)
                    .color(tokens.muted),
                text(auth.verification_url())
                    .size(tokens.text_sm)
                    .color(tokens.accent),
                text(&auth.user_code)
                    .size(tokens.text_display)
                    .color(tokens.text),
                row![button(text("Open browser")).on_press(Message::OpenBrowser),]
                    .spacing(tokens.space_sm),
                text(if self.busy {
                    "Waiting for you to finish in the browser…"
                } else {
                    ""
                })
                .size(tokens.text_xs)
                .color(tokens.muted),
            ]
            .spacing(tokens.space_sm)
            .into()
        } else {
            primary_button(
                tokens,
                "Log in with a device code",
                Message::StartFlow,
                self.busy,
            )
        }
    }

    fn pkce_paste_body<'a>(&'a self, tokens: Tokens) -> Element<'a, Message> {
        if let Some(session) = &self.pkce {
            column![
                text("Open this link and log in to TIDAL:")
                    .size(tokens.text_sm)
                    .color(tokens.muted),
                text(session.authorize_url.clone())
                    .size(tokens.text_xs)
                    .color(tokens.accent),
                button(text("Open browser")).on_press(Message::OpenBrowser),
                text(
                    "After logging in you land on a TIDAL page that says \"Oops\" — paste that \
                     page's full URL below."
                )
                .size(tokens.text_xs)
                .color(tokens.muted),
                text_input("Paste the redirected URL", &self.paste_input)
                    .on_input(Message::PasteInputChanged)
                    .on_submit(Message::SubmitPaste),
                primary_button(tokens, "Continue", Message::SubmitPaste, self.busy),
            ]
            .spacing(tokens.space_sm)
            .into()
        } else {
            primary_button(
                tokens,
                "Log in with a browser",
                Message::StartFlow,
                self.busy,
            )
        }
    }

    fn pkce_loopback_body<'a>(&'a self, tokens: Tokens) -> Element<'a, Message> {
        if let Some(session) = &self.pkce {
            column![
                text("Open this link and log in to TIDAL:")
                    .size(tokens.text_sm)
                    .color(tokens.muted),
                text(session.authorize_url.clone())
                    .size(tokens.text_xs)
                    .color(tokens.accent),
                button(text("Open browser")).on_press(Message::OpenBrowser),
                text(format!(
                    "Waiting for the browser to reach 127.0.0.1:{LOOPBACK_PORT}/callback…"
                ))
                .size(tokens.text_xs)
                .color(tokens.muted),
            ]
            .spacing(tokens.space_sm)
            .into()
        } else {
            primary_button(
                tokens,
                "Log in with a browser (loopback)",
                Message::StartFlow,
                self.busy,
            )
        }
    }
}

fn mode_button<'a>(
    tokens: Tokens,
    label: &'a str,
    mode: Mode,
    current: Mode,
) -> Element<'a, Message> {
    let active = mode == current;
    button(text(label).size(tokens.text_sm))
        .padding([tokens.space_xs, tokens.space_sm])
        .on_press(Message::SelectMode(mode))
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, iced::widget::button::Status::Hovered);
            let background = if active {
                tokens.accent_muted
            } else if hovered {
                tokens.elevated
            } else {
                tokens.surface
            };
            iced::widget::button::Style {
                background: Some(background.into()),
                text_color: if active { tokens.accent } else { tokens.text },
                border: iced::Border {
                    color: tokens.border,
                    width: 1.0,
                    radius: tokens.radius_sm.into(),
                },
                ..iced::widget::button::Style::default()
            }
        })
        .into()
}

fn primary_button<'a>(
    tokens: Tokens,
    label: &'a str,
    on_press: Message,
    busy: bool,
) -> Element<'a, Message> {
    let mut b = button(text(if busy { "Working…" } else { label }).size(tokens.text_sm))
        .padding([tokens.space_sm, tokens.space_md])
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, iced::widget::button::Status::Hovered);
            iced::widget::button::Style {
                background: Some(
                    if hovered {
                        tokens.accent_hover()
                    } else {
                        tokens.accent
                    }
                    .into(),
                ),
                text_color: tokens.background,
                border: iced::Border {
                    radius: tokens.radius_sm.into(),
                    ..iced::Border::default()
                },
                ..iced::widget::button::Style::default()
            }
        });
    if !busy {
        b = b.on_press(on_press);
    }
    b.into()
}

async fn start_device_flow(api: ApiClient) -> Result<DeviceAuthorization, String> {
    device_code::start_device_flow(&api).await.map_err(fmt_err)
}

async fn start_pkce(
    api: ApiClient,
    client_unique_key: String,
    redirect: Option<String>,
) -> Result<Arc<PkceSession>, String> {
    PkceSession::start(&api, &client_unique_key, redirect.as_deref())
        .map(Arc::new)
        .map_err(fmt_err)
}

async fn capture_loopback() -> Result<String, String> {
    pkce::capture_code_loopback(
        ([127, 0, 0, 1], LOOPBACK_PORT).into(),
        "/callback",
        Duration::from_secs(600),
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

    #[test]
    fn renders_the_device_code_and_verification_url() {
        let state = State {
            device_auth: Some(DeviceAuthorization {
                device_code: "d".into(),
                user_code: "ABCD-EFGH".into(),
                verification_uri: "link.tidal.com".into(),
                verification_uri_complete: Some("link.tidal.com/ABCDEFGH".into()),
                expires_in: 300,
                interval: 5,
            }),
            ..State::default()
        };
        let tokens = Tokens::dark();
        let mut ui = simulator(state.view(tokens));
        assert!(ui.find("ABCD-EFGH").is_ok());
        assert!(ui.find("https://link.tidal.com/ABCDEFGH").is_ok());
    }

    #[test]
    fn renders_an_inline_error() {
        let state = State {
            error: Some("the login code expired before it was used".into()),
            ..State::default()
        };
        let tokens = Tokens::dark();
        let mut ui = simulator(state.view(tokens));
        assert!(ui.find("the login code expired before it was used").is_ok());
    }
}
