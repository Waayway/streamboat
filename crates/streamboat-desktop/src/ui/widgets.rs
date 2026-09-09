//! Small shared widgets built from the design tokens (`ui::design`): rounded
//! cards with hover states, the sidebar's nav items, section headers and
//! quality/status chips. Every screen composes these instead of hand-rolling
//! its own container styling, which is what keeps "one streamboat look"
//! (D-012) actually one look instead of five slightly different ones.

use iced::widget::{button, column, container, image, row, scrollable, text};
use iced::{Border, Color, Element, Length, Theme};

use crate::ui::design::Tokens;
use crate::ui::format::{FeedCard, SectionView};
use crate::ui::images::ImageCache;
use crate::ui::nav::EntityRef;

/// A rounded, elevated card that reacts to hover — the building block for
/// every feed section's items. `on_press` is optional so a card can render
/// without being clickable (e.g. a card whose entity id could not be
/// resolved).
pub fn card<'a, Message: Clone + 'a>(
    tokens: Tokens,
    width: f32,
    content: impl Into<Element<'a, Message>>,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let mut b = button(content)
        .width(Length::Fixed(width))
        .padding(tokens.space_sm)
        .style(move |_theme: &Theme, status| card_style(tokens, status));
    if let Some(msg) = on_press {
        b = b.on_press(msg);
    }
    b.into()
}

fn card_style(tokens: Tokens, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => tokens.elevated,
        button::Status::Active | button::Status::Disabled => tokens.surface,
    };
    button::Style {
        background: Some(background.into()),
        text_color: tokens.text,
        border: Border {
            color: tokens.border,
            width: 1.0,
            radius: tokens.radius_md.into(),
        },
        ..button::Style::default()
    }
}

/// One sidebar entry. `active` renders the accent highlight for the current
/// screen.
pub fn nav_item<'a, Message: Clone + 'a>(
    tokens: Tokens,
    label: &'a str,
    active: bool,
    on_press: Message,
) -> Element<'a, Message> {
    button(text(label).size(tokens.text_md))
        .width(Length::Fill)
        .padding([tokens.space_sm, tokens.space_md])
        .on_press(on_press)
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let background = if active {
                Some(tokens.accent_muted.into())
            } else if hovered {
                Some(tokens.elevated.into())
            } else {
                None
            };
            button::Style {
                background,
                text_color: if active { tokens.accent } else { tokens.text },
                border: Border {
                    radius: tokens.radius_sm.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }
        })
        .into()
}

/// A section title with an optional "View all" action.
pub fn section_header<'a, Message: Clone + 'a>(
    tokens: Tokens,
    title: String,
    view_all: Option<Message>,
) -> Element<'a, Message> {
    let heading = text(title).size(tokens.text_lg).color(tokens.text);
    let mut r = row![heading]
        .width(Length::Fill)
        .align_y(iced::Alignment::Center);
    if let Some(msg) = view_all {
        r = r.push(iced::widget::space::horizontal());
        r = r.push(
            button(text("View all").size(tokens.text_sm).color(tokens.accent))
                .style(button::text)
                .on_press(msg),
        );
    }
    r.into()
}

/// A small rounded chip, used for the quality badge and status labels
/// ("PREVIEW", "EXCLUSIVE", a warning tag, ...).
pub fn chip<'a, Message: 'a>(
    tokens: Tokens,
    label: impl ToString,
    tone: ChipTone,
) -> Element<'a, Message> {
    let (bg, fg) = match tone {
        ChipTone::Accent => (tokens.accent_muted, tokens.accent),
        ChipTone::Muted => (tokens.elevated, tokens.muted),
        ChipTone::Warning => (with_alpha(tokens.warning, 0.18), tokens.warning),
        ChipTone::Danger => (with_alpha(tokens.danger, 0.18), tokens.danger),
    };
    container(text(label.to_string()).size(tokens.text_xs).color(fg))
        .padding([tokens.space_xs / 2.0, tokens.space_sm])
        .style(move |_theme: &Theme| container::Style {
            background: Some(bg.into()),
            border: Border {
                radius: tokens.radius_pill.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipTone {
    Accent,
    Muted,
    Warning,
    Danger,
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

/// The surface a whole screen sits on, with the standard page padding.
pub fn page<'a, Message: 'a>(
    tokens: Tokens,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(tokens.space_lg)
        .style(move |_theme: &Theme| container::Style {
            background: Some(tokens.background.into()),
            text_color: Some(tokens.text),
            ..container::Style::default()
        })
        .into()
}

/// A short banner used for inline errors/warnings (Login screen errors,
/// feed-load failures, ...).
pub fn banner<'a, Message: 'a>(
    tokens: Tokens,
    message: impl ToString,
    tone: ChipTone,
) -> Element<'a, Message> {
    let color = match tone {
        ChipTone::Danger => tokens.danger,
        ChipTone::Warning => tokens.warning,
        _ => tokens.muted,
    };
    container(
        column![text(message.to_string()).size(tokens.text_sm).color(color)]
            .spacing(tokens.space_xs),
    )
    .padding(tokens.space_sm)
    .width(Length::Fill)
    .style(move |_theme: &Theme| container::Style {
        background: Some(with_alpha(color, 0.12).into()),
        border: Border {
            color,
            width: 1.0,
            radius: tokens.radius_sm.into(),
        },
        ..container::Style::default()
    })
    .into()
}

const CARD_WIDTH: f32 = 140.0;
const CARDS_PER_ROW: usize = 5;

/// A scrollable stack of every section (task item 3: Home/Explore render
/// server-driven sections with a graceful unknown-type fallback, D-015).
/// Shared by Home and Explore so the two screens' cards, hover states and
/// unknown-section note render identically. `view_all`/`clicked` are plain
/// function pointers so this stays generic over each screen's own `Message`
/// type without an extra trait or boxed closure.
pub fn feed_sections<'a, Message: Clone + 'a>(
    tokens: Tokens,
    sections: Vec<(usize, SectionView)>,
    images: &ImageCache,
    view_all: fn(usize, String) -> Message,
    clicked: fn(EntityRef) -> Message,
) -> Element<'a, Message> {
    let mut col = column![].spacing(tokens.space_lg).width(Length::Fill);
    for (index, view) in sections {
        col = col.push(one_section(tokens, index, view, images, view_all, clicked));
    }
    scrollable(col)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn one_section<'a, Message: Clone + 'a>(
    tokens: Tokens,
    index: usize,
    view: SectionView,
    images: &ImageCache,
    view_all: fn(usize, String) -> Message,
    clicked: fn(EntityRef) -> Message,
) -> Element<'a, Message> {
    match view {
        SectionView::Cards {
            title,
            cards,
            api_path,
        } => {
            let header = section_header(tokens, title, api_path.map(|p| view_all(index, p)));
            column![header, card_grid(tokens, cards, images, clicked)]
                .spacing(tokens.space_sm)
                .into()
        }
        SectionView::Unsupported { title, kind } => column![
            section_header(tokens, title, None),
            banner(
                tokens,
                format!("\"{kind}\" sections aren't supported yet"),
                ChipTone::Muted,
            ),
        ]
        .spacing(tokens.space_sm)
        .into(),
    }
}

/// A simple wrapping grid: iced 0.14 has no built-in flow/wrap widget, so
/// this chunks cards into fixed-size rows inside a column — enough for the
/// fixed-width cover cards every section uses.
fn card_grid<'a, Message: Clone + 'a>(
    tokens: Tokens,
    cards: Vec<FeedCard>,
    images: &ImageCache,
    clicked: fn(EntityRef) -> Message,
) -> Element<'a, Message> {
    let mut rows_col = column![].spacing(tokens.space_md);
    for chunk in cards.chunks(CARDS_PER_ROW) {
        let mut r = row![].spacing(tokens.space_md);
        for c in chunk {
            r = r.push(feed_card(tokens, c.clone(), images, clicked));
        }
        rows_col = rows_col.push(r);
    }
    rows_col.into()
}

fn feed_card<'a, Message: Clone + 'a>(
    tokens: Tokens,
    c: FeedCard,
    images: &ImageCache,
    clicked: fn(EntityRef) -> Message,
) -> Element<'a, Message> {
    let cover_url = c.image_id.as_deref().and_then(crate::ui::format::cover_url);
    let art: Element<'a, Message> = match cover_url.and_then(|url| images.peek(&url)) {
        Some(handle) => image(handle).width(CARD_WIDTH).height(CARD_WIDTH).into(),
        None => art_placeholder(tokens),
    };
    let mut body = column![
        art,
        text(c.title.clone())
            .size(tokens.text_sm)
            .color(tokens.text),
    ]
    .spacing(tokens.space_xs);
    if let Some(subtitle) = c.subtitle.clone() {
        body = body.push(text(subtitle).size(tokens.text_xs).color(tokens.muted));
    }
    card(tokens, CARD_WIDTH, body, c.entity.clone().map(clicked))
}

fn art_placeholder<'a, Message: 'a>(tokens: Tokens) -> Element<'a, Message> {
    container(text(""))
        .width(CARD_WIDTH)
        .height(CARD_WIDTH)
        .style(move |_theme: &Theme| container::Style {
            background: Some(tokens.elevated.into()),
            border: Border {
                radius: tokens.radius_sm.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}
