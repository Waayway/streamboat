//! Placeholder screens for the NEXT wave (task item 3): entity pages
//! (album/artist/playlist/mix/track), Collection, and lyrics. Routing is
//! complete now — clicking a card already navigates here with the right
//! entity id — the screens themselves are just a "coming soon" note until
//! the next wave builds them for real.

use iced::Element;
use iced::widget::{column, text};

use crate::ui::design::Tokens;
use crate::ui::nav::EntityRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {}

pub fn entity_view<'a>(tokens: Tokens, entity: &'a EntityRef) -> Element<'a, Message> {
    body(
        tokens,
        format!("{} page", entity.kind_label()),
        format!(
            "{} {} isn't built yet — this screen is the NEXT wave's placeholder. Routing \
             already carries the right id here.",
            entity.kind_label(),
            entity.id_label()
        ),
    )
}

pub fn collection_view<'a>(tokens: Tokens) -> Element<'a, Message> {
    body(
        tokens,
        "My Collection",
        "Favourites, playlists and library folders are the NEXT wave.".to_string(),
    )
}

pub fn lyrics_view<'a>(tokens: Tokens, track_id: u64) -> Element<'a, Message> {
    body(
        tokens,
        "Lyrics",
        format!("Synced lyrics for track {track_id} are the NEXT wave."),
    )
}

fn body<'a>(
    tokens: Tokens,
    title: impl Into<String>,
    note: impl Into<String>,
) -> Element<'a, Message> {
    crate::ui::widgets::page(
        tokens,
        column![
            text(title.into()).size(tokens.text_xl).color(tokens.text),
            text(note.into()).size(tokens.text_sm).color(tokens.muted),
        ]
        .spacing(tokens.space_md),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_test::simulator;

    #[test]
    fn entity_placeholder_shows_the_kind_and_id() {
        let tokens = Tokens::dark();
        let entity = EntityRef::Album(42);
        let mut ui = simulator(entity_view(tokens, &entity));
        assert!(ui.find("Album page").is_ok());
    }
}
