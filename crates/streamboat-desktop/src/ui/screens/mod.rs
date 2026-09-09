//! Every screen: Login, Home, Explore, Search, Now Playing, Settings, the
//! entity pages (Album, Artist, Playlist, Mix, Track, Video), My
//! Collection and Lyrics. The mini-player lives in its own top-level
//! `ui::mini_player` module, not among these screens.

pub mod album;
pub mod artist;
pub mod collection;
pub mod explore;
pub mod home;
pub mod login;
pub mod lyrics;
pub mod mix;
pub mod now_playing;
pub mod playlist;
pub mod search;
pub mod settings;
pub mod track;
pub mod video;

use iced::widget::{button, column, text};
use iced::{Element, Length, Theme};

use crate::ui::design::Tokens;
use crate::ui::format::SectionView;
use crate::ui::images::ImageCache;
use crate::ui::nav::EntityRef;
use crate::ui::widgets::{ChipTone, banner, feed_sections};

/// The scaffold Home and Explore share: a loading note, an inline error
/// banner, an optional tab bar, the section list (with the graceful
/// unknown-section fallback, D-015), and a "load more" button gated on the
/// top-level cursor. `view_all`/`clicked`/`load_more` are plain function
/// pointers so this stays generic over each screen's own split `Message`
/// type (D-013) with no boxed closures.
#[allow(clippy::too_many_arguments)]
pub fn feed_body<'a, Message: Clone + 'a>(
    tokens: Tokens,
    loading: bool,
    error: Option<&'a str>,
    tab_bar: Option<Element<'a, Message>>,
    sections: Vec<(usize, SectionView)>,
    images: &ImageCache,
    load_more: Option<Message>,
    view_all: fn(usize, String) -> Message,
    clicked: fn(EntityRef) -> Message,
) -> Element<'a, Message> {
    let mut col = column![].spacing(tokens.space_md).width(Length::Fill);

    if let Some(tabs) = tab_bar {
        col = col.push(tabs);
    }
    if let Some(message) = error {
        col = col.push(banner(tokens, message, ChipTone::Danger));
    }

    if loading {
        col = col.push(text("Loading…").size(tokens.text_md).color(tokens.muted));
    } else {
        col = col.push(feed_sections(tokens, sections, images, view_all, clicked));
        if let Some(load_more) = load_more {
            col = col.push(
                button(text("Load more").size(tokens.text_sm))
                    .on_press(load_more)
                    .style(move |_theme: &Theme, status| {
                        let hovered = matches!(status, iced::widget::button::Status::Hovered);
                        iced::widget::button::Style {
                            background: Some(
                                (if hovered {
                                    tokens.elevated
                                } else {
                                    tokens.surface
                                })
                                .into(),
                            ),
                            text_color: tokens.accent,
                            border: iced::Border {
                                color: tokens.border,
                                width: 1.0,
                                radius: tokens.radius_sm.into(),
                            },
                            ..iced::widget::button::Style::default()
                        }
                    }),
            );
        }
    }

    crate::ui::widgets::page(tokens, col)
}
