//! The notification banner area: `Event::Warning`,
//! `Event::Error` and `Event::PlaybackTakenOver { by }` each push one
//! dismissible, auto-expiring banner instead of being matched with a
//! silent no-op in `ui::app`. A takeover banner additionally carries a resume
//! button that sends a user-intent `Command::Resume` (D-033: never resend a
//! claim automatically — only a real button press counts).

use std::time::Duration;

use iced::widget::{button, column, container, row, text};
use iced::{Border, Element, Length, Task, Theme};

use crate::ui::design::Tokens;

/// How long a banner stays up before auto-expiring, absent a dismiss.
const AUTO_EXPIRE: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Warning,
    Danger,
    Info,
}

struct Entry {
    id: u64,
    message: String,
    tone: Tone,
    /// `true` for a `PlaybackTakenOver` banner — shows a "Resume" button.
    resumable: bool,
}

#[derive(Default)]
pub struct State {
    items: Vec<Entry>,
    next_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    Dismiss(u64),
    Expire(u64),
    Resume(u64),
}

pub enum Effect {
    /// The user pressed "Resume" on a takeover banner — genuine user intent
    /// (D-033); `ui::app` sends `Command::Resume` in response.
    Resume,
}

impl State {
    /// Push one banner and return the `Task` that auto-expires it —
    /// `ui::app` must `.map(Message::Banner)` and batch this task in.
    pub fn push(
        &mut self,
        message: impl Into<String>,
        tone: Tone,
        resumable: bool,
    ) -> Task<Message> {
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(Entry {
            id,
            message: message.into(),
            tone,
            resumable,
        });
        // `tokio::time::sleep(...)` must be called lazily, inside the async
        // block, not eagerly here — calling it directly needs a live tokio
        // reactor at the call site, which a plain `#[test]` (and, for that
        // matter, `push`'s own caller in `update`) does not have; deferring
        // it into the future's body (only actually polled once iced's own
        // tokio executor runs this `Task`) is the same shape `screens::search`'s
        // `debounce` already uses.
        Task::perform(
            async move { tokio::time::sleep(AUTO_EXPIRE).await },
            move |()| Message::Expire(id),
        )
    }

    pub fn update(&mut self, message: Message) -> Option<Effect> {
        match message {
            Message::Dismiss(id) | Message::Expire(id) => {
                self.items.retain(|e| e.id != id);
                None
            }
            Message::Resume(id) => {
                self.items.retain(|e| e.id != id);
                Some(Effect::Resume)
            }
        }
    }

    pub fn view(&self, tokens: Tokens) -> Option<Element<'_, Message>> {
        if self.items.is_empty() {
            return None;
        }
        let mut col = column![].spacing(tokens.space_xs).width(Length::Fill);
        for entry in &self.items {
            col = col.push(banner_row(tokens, entry));
        }
        Some(
            container(col)
                .padding(tokens.space_sm)
                .width(Length::Fill)
                .into(),
        )
    }
}

fn banner_row<'a>(tokens: Tokens, entry: &'a Entry) -> Element<'a, Message> {
    let color = match entry.tone {
        Tone::Danger => tokens.danger,
        Tone::Warning => tokens.warning,
        Tone::Info => tokens.accent,
    };
    let mut content = row![
        text(entry.message.clone())
            .size(tokens.text_sm)
            .color(color)
    ]
    .spacing(tokens.space_sm)
    .align_y(iced::Alignment::Center)
    .width(Length::Fill);
    content = content.push(iced::widget::space::horizontal());
    if entry.resumable {
        content = content.push(small_button(
            tokens,
            "Resume",
            Message::Resume(entry.id),
            color,
        ));
    }
    content = content.push(small_button(
        tokens,
        "Dismiss",
        Message::Dismiss(entry.id),
        color,
    ));

    container(content)
        .padding(tokens.space_sm)
        .width(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced::Color { a: 0.14, ..color }.into()),
            border: Border {
                color,
                width: 1.0,
                radius: tokens.radius_sm.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn small_button<'a>(
    tokens: Tokens,
    label: &'a str,
    on_press: Message,
    color: iced::Color,
) -> Element<'a, Message> {
    button(text(label).size(tokens.text_xs).color(color))
        .padding([tokens.space_xs, tokens.space_sm])
        .on_press(on_press)
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: hovered.then_some(iced::Color { a: 0.2, ..color }.into()),
                text_color: color,
                border: Border {
                    color,
                    width: 1.0,
                    radius: tokens.radius_pill.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pushed_banner_renders_its_message() {
        let mut state = State::default();
        let _ = state.push("playback started on Kitchen Speaker", Tone::Info, true);
        let tokens = Tokens::dark();
        let element = state.view(tokens).expect("a banner is present");
        let mut ui = iced_test::simulator(element);
        assert!(ui.find("playback started on Kitchen Speaker").is_ok());
        assert!(ui.find("Resume").is_ok());
    }

    #[test]
    fn dismiss_removes_the_banner() {
        let mut state = State::default();
        let _ = state.push("warning", Tone::Warning, false);
        let id = state.items[0].id;
        let effect = state.update(Message::Dismiss(id));
        assert!(effect.is_none());
        assert!(state.view(Tokens::dark()).is_none());
    }

    #[test]
    fn resume_bubbles_an_effect_and_clears_the_banner() {
        let mut state = State::default();
        let _ = state.push("playback started elsewhere", Tone::Info, true);
        let id = state.items[0].id;
        let effect = state.update(Message::Resume(id));
        assert!(matches!(effect, Some(Effect::Resume)));
        assert!(state.view(Tokens::dark()).is_none());
    }

    #[test]
    fn no_banners_renders_nothing() {
        let state = State::default();
        assert!(state.view(Tokens::dark()).is_none());
    }
}
