//! Search (task item 3): all types with a type filter, debounced input.

use std::time::Duration;

use iced::widget::{button, column, row, scrollable, text, text_input};
use iced::{Element, Length, Task, Theme};

use streamboat_core::models::SearchResults;
use streamboat_core::{ApiClient, Error};

use crate::ui::design::Tokens;
use crate::ui::nav::EntityRef;
use crate::ui::widgets::{ChipTone, banner, card};

const DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeFilter {
    All,
    Tracks,
    Albums,
    Artists,
    Playlists,
}

impl TypeFilter {
    const ALL: [TypeFilter; 5] = [
        TypeFilter::All,
        TypeFilter::Tracks,
        TypeFilter::Albums,
        TypeFilter::Artists,
        TypeFilter::Playlists,
    ];

    fn label(self) -> &'static str {
        match self {
            TypeFilter::All => "All",
            TypeFilter::Tracks => "Tracks",
            TypeFilter::Albums => "Albums",
            TypeFilter::Artists => "Artists",
            TypeFilter::Playlists => "Playlists",
        }
    }
}

pub struct State {
    query: String,
    filter: TypeFilter,
    results: Option<SearchResults>,
    loading: bool,
    error: Option<String>,
    generation: u64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            query: String::new(),
            filter: TypeFilter::All,
            results: None,
            loading: false,
            error: None,
            generation: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    QueryChanged(String),
    FilterChanged(TypeFilter),
    Debounced(u64),
    ResultsLoaded(u64, Result<SearchResults, String>),
    TrackClicked(u64),
    PlayNext(u64),
    AddLast(u64),
    EntityClicked(EntityRef),
}

pub enum Effect {
    PlayTrack(u64),
    /// Queue task item 3's "play-next/add-last" without interrupting
    /// playback (`Command::Enqueue`).
    Enqueue(u64, streamboat_core::proto::QueuePosition),
    Navigate(EntityRef),
    ImagesNeeded(Vec<String>),
}

impl State {
    pub fn update(&mut self, message: Message, api: &ApiClient) -> (Task<Message>, Vec<Effect>) {
        match message {
            Message::QueryChanged(query) => {
                self.query = query;
                self.generation += 1;
                let generation = self.generation;
                if self.query.trim().is_empty() {
                    self.results = None;
                    self.loading = false;
                    return (Task::none(), Vec::new());
                }
                (
                    Task::perform(debounce(generation), Message::Debounced),
                    Vec::new(),
                )
            }
            Message::FilterChanged(filter) => {
                self.filter = filter;
                (Task::none(), Vec::new())
            }
            Message::Debounced(generation) => {
                if generation != self.generation || self.query.trim().is_empty() {
                    return (Task::none(), Vec::new());
                }
                self.loading = true;
                self.error = None;
                let query = self.query.clone();
                (
                    Task::perform(run_search(api.clone(), query, generation), |(g, r)| {
                        Message::ResultsLoaded(g, r)
                    }),
                    Vec::new(),
                )
            }
            Message::ResultsLoaded(generation, result) => {
                if generation != self.generation {
                    return (Task::none(), Vec::new());
                }
                self.loading = false;
                match result {
                    Ok(results) => {
                        let images = image_ids(&results);
                        self.results = Some(results);
                        self.error = None;
                        (Task::none(), vec![Effect::ImagesNeeded(images)])
                    }
                    Err(e) => {
                        self.error = Some(e);
                        (Task::none(), Vec::new())
                    }
                }
            }
            Message::TrackClicked(id) => (Task::none(), vec![Effect::PlayTrack(id)]),
            Message::PlayNext(id) => (
                Task::none(),
                vec![Effect::Enqueue(
                    id,
                    streamboat_core::proto::QueuePosition::Next,
                )],
            ),
            Message::AddLast(id) => (
                Task::none(),
                vec![Effect::Enqueue(
                    id,
                    streamboat_core::proto::QueuePosition::Last,
                )],
            ),
            Message::EntityClicked(entity) => (Task::none(), vec![Effect::Navigate(entity)]),
        }
    }

    pub fn view<'a>(
        &'a self,
        tokens: Tokens,
        images: &crate::ui::images::ImageCache,
    ) -> Element<'a, Message> {
        let input = text_input("Search TIDAL", &self.query).on_input(Message::QueryChanged);

        let mut filters = row![].spacing(tokens.space_xs);
        for f in TypeFilter::ALL {
            let active = f == self.filter;
            filters = filters.push(
                button(text(f.label()).size(tokens.text_sm))
                    .padding([tokens.space_xs, tokens.space_sm])
                    .on_press(Message::FilterChanged(f))
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
                                radius: tokens.radius_pill.into(),
                                ..iced::Border::default()
                            },
                            ..iced::widget::button::Style::default()
                        }
                    }),
            );
        }

        let mut col = column![
            text("Search").size(tokens.text_xl).color(tokens.text),
            input,
            filters,
        ]
        .spacing(tokens.space_md)
        .width(Length::Fill);

        if let Some(e) = &self.error {
            col = col.push(banner(tokens, e.clone(), ChipTone::Danger));
        }
        if self.loading {
            col = col.push(text("Searching…").size(tokens.text_sm).color(tokens.muted));
        } else if let Some(results) = &self.results {
            col = col.push(scrollable(self.results_view(tokens, results, images)));
        }

        crate::ui::widgets::page(tokens, col)
    }

    fn results_view<'a>(
        &'a self,
        tokens: Tokens,
        results: &'a SearchResults,
        _images: &crate::ui::images::ImageCache,
    ) -> Element<'a, Message> {
        let mut col = column![].spacing(tokens.space_md).width(Length::Fill);
        let mut any = false;

        if matches!(self.filter, TypeFilter::All | TypeFilter::Tracks)
            && !results.tracks.items.is_empty()
        {
            any = true;
            let mut rows = column![].spacing(tokens.space_xs);
            for t in &results.tracks.items {
                rows = rows.push(track_row(tokens, t));
            }
            col = col.push(group(tokens, "Tracks", rows));
        }
        if matches!(self.filter, TypeFilter::All | TypeFilter::Albums)
            && !results.albums.items.is_empty()
        {
            any = true;
            let mut rows = row![].spacing(tokens.space_sm);
            for a in &results.albums.items {
                rows = rows.push(entity_card(tokens, a.title.clone(), EntityRef::Album(a.id)));
            }
            col = col.push(group(tokens, "Albums", rows));
        }
        if matches!(self.filter, TypeFilter::All | TypeFilter::Artists)
            && !results.artists.items.is_empty()
        {
            any = true;
            let mut rows = row![].spacing(tokens.space_sm);
            for a in &results.artists.items {
                rows = rows.push(entity_card(tokens, a.name.clone(), EntityRef::Artist(a.id)));
            }
            col = col.push(group(tokens, "Artists", rows));
        }
        if matches!(self.filter, TypeFilter::All | TypeFilter::Playlists)
            && !results.playlists.items.is_empty()
        {
            any = true;
            let mut rows = row![].spacing(tokens.space_sm);
            for p in &results.playlists.items {
                if let Some(uuid) = p.uuid.clone() {
                    rows = rows.push(entity_card(
                        tokens,
                        p.title.clone(),
                        EntityRef::Playlist(uuid),
                    ));
                }
            }
            col = col.push(group(tokens, "Playlists", rows));
        }

        if !any {
            return text("No results.")
                .size(tokens.text_sm)
                .color(tokens.muted)
                .into();
        }
        col.into()
    }
}

fn group<'a>(
    tokens: Tokens,
    title: &'a str,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    column![
        text(title).size(tokens.text_lg).color(tokens.text),
        content.into(),
    ]
    .spacing(tokens.space_sm)
    .into()
}

fn track_row<'a>(tokens: Tokens, t: &'a streamboat_core::models::Track) -> Element<'a, Message> {
    let label = button(
        row![
            text(t.title.clone())
                .size(tokens.text_sm)
                .color(tokens.text),
            text(format!("— {}", t.artist_names()))
                .size(tokens.text_sm)
                .color(tokens.muted),
        ]
        .spacing(tokens.space_xs),
    )
    .width(Length::Fill)
    .padding(tokens.space_sm)
    .on_press(Message::TrackClicked(t.id))
    .style(move |_theme: &Theme, status| {
        let hovered = matches!(status, iced::widget::button::Status::Hovered);
        iced::widget::button::Style {
            background: hovered.then_some(tokens.elevated.into()),
            text_color: tokens.text,
            border: iced::Border {
                radius: tokens.radius_sm.into(),
                ..iced::Border::default()
            },
            ..iced::widget::button::Style::default()
        }
    });

    row![
        label,
        queue_button(tokens, "Play next", Message::PlayNext(t.id)),
        queue_button(tokens, "Add last", Message::AddLast(t.id)),
    ]
    .spacing(tokens.space_xs)
    .align_y(iced::Alignment::Center)
    .into()
}

fn queue_button<'a>(tokens: Tokens, label: &'a str, on_press: Message) -> Element<'a, Message> {
    button(text(label).size(tokens.text_xs))
        .padding([tokens.space_xs, tokens.space_sm])
        .on_press(on_press)
        .style(move |_theme: &Theme, status| {
            let hovered = matches!(status, iced::widget::button::Status::Hovered);
            iced::widget::button::Style {
                background: hovered.then_some(tokens.elevated.into()),
                text_color: tokens.muted,
                border: iced::Border {
                    radius: tokens.radius_sm.into(),
                    ..iced::Border::default()
                },
                ..iced::widget::button::Style::default()
            }
        })
        .into()
}

fn entity_card<'a>(tokens: Tokens, title: String, entity: EntityRef) -> Element<'a, Message> {
    card(
        tokens,
        140.0,
        text(title).size(tokens.text_sm).color(tokens.text),
        Some(Message::EntityClicked(entity)),
    )
}

fn image_ids(results: &SearchResults) -> Vec<String> {
    results
        .albums
        .items
        .iter()
        .filter_map(|a| a.cover.clone())
        .chain(
            results
                .artists
                .items
                .iter()
                .filter_map(|a| a.picture.clone()),
        )
        .collect()
}

async fn debounce(generation: u64) -> u64 {
    tokio::time::sleep(DEBOUNCE).await;
    generation
}

async fn run_search(
    api: ApiClient,
    query: String,
    generation: u64,
) -> (u64, Result<SearchResults, String>) {
    let result = api.search(&query, 25, 0).await.map_err(fmt_err);
    (generation, result)
}

fn fmt_err(e: Error) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_test::simulator;
    use serde_json::json;

    #[test]
    fn renders_the_query_and_filter_row() {
        let state = State::default();
        let tokens = Tokens::dark();
        let images = crate::ui::images::ImageCache::new(4);
        let mut ui = simulator(state.view(tokens, &images));
        assert!(ui.find("Tracks").is_ok());
        assert!(ui.find("Albums").is_ok());
    }

    #[test]
    fn renders_track_results() {
        let results: SearchResults = serde_json::from_value(json!({
            "tracks": {"items": [{"id": 1, "title": "Synthetic Track", "artist": {"name": "Synthetic Artist"}}]}
        }))
        .unwrap();
        let state = State {
            results: Some(results),
            ..State::default()
        };
        let tokens = Tokens::dark();
        let images = crate::ui::images::ImageCache::new(4);
        let mut ui = simulator(state.view(tokens, &images));
        assert!(ui.find("Synthetic Track").is_ok());
    }
}
