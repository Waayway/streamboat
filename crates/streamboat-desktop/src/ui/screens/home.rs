//! Home (task item 3): the server-driven `home/feed` sections, tab bar from
//! the header, paging on the top-level cursor, per-section "view all" via
//! `expand_section`, and the graceful unknown-section card (D-015).

use iced::widget::{button, row, text};
use iced::{Element, Task, Theme};

use streamboat_core::api::pagination::clamp_limit;
use streamboat_core::models::{FeedItem, FeedSection, HomeFeed, HomeTab, ItemsWrap};
use streamboat_core::{ApiClient, Error};

use crate::ui::design::Tokens;
use crate::ui::format::feed_section_to_view;
use crate::ui::nav::EntityRef;

pub struct State {
    tabs: Vec<HomeTab>,
    active_tab: usize,
    sections: Vec<FeedSection>,
    cursor: Option<String>,
    loading: bool,
    error: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active_tab: 0,
            sections: Vec::new(),
            cursor: None,
            loading: true,
            error: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    TabsLoaded(Result<Vec<HomeTab>, String>),
    FeedLoaded(Result<HomeFeed, String>),
    SelectTab(usize),
    LoadMore,
    ViewAll(usize, String),
    ViewAllLoaded(usize, Result<Vec<FeedItem>, String>),
    CardClicked(EntityRef),
}

/// What Home needs `ui::app` to do outside its own state — navigate, or
/// fetch artwork for a newly-seen cover id through the shared image cache.
pub enum Effect {
    Navigate(EntityRef),
    ImagesNeeded(Vec<String>),
}

impl State {
    pub fn load(api: &ApiClient) -> Task<Message> {
        Task::batch([
            Task::perform(load_tabs(api.clone()), Message::TabsLoaded),
            Task::perform(
                load_feed(api.clone(), "static".to_string(), None),
                Message::FeedLoaded,
            ),
        ])
    }

    pub fn update(&mut self, message: Message, api: &ApiClient) -> (Task<Message>, Vec<Effect>) {
        match message {
            Message::TabsLoaded(Ok(tabs)) => {
                self.tabs = tabs;
                (Task::none(), Vec::new())
            }
            Message::TabsLoaded(Err(_)) => (Task::none(), Vec::new()),
            Message::FeedLoaded(Ok(feed)) => {
                self.loading = false;
                self.error = None;
                self.cursor = feed.cursor;
                self.sections = feed.items;
                (Task::none(), vec![Effect::ImagesNeeded(self.image_ids())])
            }
            Message::FeedLoaded(Err(e)) => {
                self.loading = false;
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::SelectTab(index) => {
                let Some(slug) = self.tabs.get(index).and_then(HomeTab::slug) else {
                    return (Task::none(), Vec::new());
                };
                self.active_tab = index;
                self.loading = true;
                self.cursor = None;
                (
                    Task::perform(load_feed(api.clone(), slug, None), Message::FeedLoaded),
                    Vec::new(),
                )
            }
            Message::LoadMore => {
                let Some(cursor) = self.cursor.clone() else {
                    return (Task::none(), Vec::new());
                };
                let slug = self
                    .tabs
                    .get(self.active_tab)
                    .and_then(HomeTab::slug)
                    .unwrap_or_else(|| "static".to_string());
                (
                    Task::perform(
                        load_feed(api.clone(), slug, Some(cursor)),
                        Message::FeedLoaded,
                    ),
                    Vec::new(),
                )
            }
            Message::ViewAll(index, api_path) => {
                let limit = clamp_limit(20);
                let offset = self
                    .sections
                    .get(index)
                    .map(|s| s.items.len() as u32)
                    .unwrap_or(0);
                (
                    Task::perform(expand(api.clone(), api_path, limit, offset), move |r| {
                        Message::ViewAllLoaded(index, r)
                    }),
                    Vec::new(),
                )
            }
            Message::ViewAllLoaded(index, Ok(items)) => {
                let images = items
                    .iter()
                    .filter_map(crate::ui::format::feed_item_to_card)
                    .filter_map(|c| c.image_id)
                    .collect::<Vec<_>>();
                if let Some(section) = self.sections.get_mut(index) {
                    section.items.extend(items);
                    section.has_more = false;
                }
                (Task::none(), vec![Effect::ImagesNeeded(images)])
            }
            Message::ViewAllLoaded(index, Err(e)) => {
                if let Some(section) = self.sections.get_mut(index) {
                    section.has_more = false;
                }
                self.error = Some(format!("couldn't load more: {e}"));
                (Task::none(), Vec::new())
            }
            Message::CardClicked(entity) => (Task::none(), vec![Effect::Navigate(entity)]),
        }
    }

    fn image_ids(&self) -> Vec<String> {
        self.sections
            .iter()
            .flat_map(|s| s.items.iter())
            .filter_map(crate::ui::format::feed_item_to_card)
            .filter_map(|c| c.image_id)
            .collect()
    }

    pub fn view<'a>(
        &'a self,
        tokens: Tokens,
        images: &crate::ui::images::ImageCache,
    ) -> Element<'a, Message> {
        let sections = self
            .sections
            .iter()
            .enumerate()
            .map(|(i, s)| (i, feed_section_to_view(s)))
            .collect();
        crate::ui::screens::feed_body(
            tokens,
            self.loading,
            self.error.as_deref(),
            tab_bar(tokens, &self.tabs, self.active_tab),
            sections,
            images,
            self.cursor.is_some().then_some(Message::LoadMore),
            Message::ViewAll,
            Message::CardClicked,
        )
    }
}

fn tab_bar<'a>(tokens: Tokens, tabs: &'a [HomeTab], active: usize) -> Option<Element<'a, Message>> {
    if tabs.len() <= 1 {
        return None;
    }
    let mut r = row![].spacing(tokens.space_sm);
    for (index, tab) in tabs.iter().enumerate() {
        let label = tab.name.clone().unwrap_or_else(|| format!("Tab {index}"));
        let is_active = index == active;
        r = r.push(
            button(text(label).size(tokens.text_sm))
                .padding([tokens.space_xs, tokens.space_md])
                .on_press(Message::SelectTab(index))
                .style(move |_theme: &Theme, status| {
                    let hovered = matches!(status, iced::widget::button::Status::Hovered);
                    let background = if is_active {
                        tokens.accent_muted
                    } else if hovered {
                        tokens.elevated
                    } else {
                        tokens.surface
                    };
                    iced::widget::button::Style {
                        background: Some(background.into()),
                        text_color: if is_active {
                            tokens.accent
                        } else {
                            tokens.text
                        },
                        border: iced::Border {
                            radius: tokens.radius_pill.into(),
                            ..iced::Border::default()
                        },
                        ..iced::widget::button::Style::default()
                    }
                }),
        );
    }
    Some(r.into())
}

async fn load_tabs(api: ApiClient) -> Result<Vec<HomeTab>, String> {
    api.home_tabs().await.map_err(fmt_err)
}

async fn load_feed(
    api: ApiClient,
    slug: String,
    cursor: Option<String>,
) -> Result<HomeFeed, String> {
    api.home_feed(&slug, cursor.as_deref())
        .await
        .map_err(fmt_err)
}

async fn expand(
    api: ApiClient,
    api_path: String,
    limit: u32,
    offset: u32,
) -> Result<Vec<FeedItem>, String> {
    let value = api
        .expand_section(&api_path, limit, offset)
        .await
        .map_err(fmt_err)?;
    serde_json::from_value::<ItemsWrap<FeedItem>>(value)
        .map(|w| w.items)
        .map_err(|e| format!("unexpected \"view all\" response shape: {e}"))
}

fn fmt_err(e: Error) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_test::simulator;
    use serde_json::json;

    fn section(json: serde_json::Value) -> FeedSection {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn renders_a_known_section_and_an_unknown_one_gracefully() {
        let state = State {
            loading: false,
            sections: vec![
                section(json!({
                    "type": "HORIZONTAL_LIST",
                    "title": "Suggested New Albums",
                    "items": [{"type": "ALBUM", "id": 1, "title": "Synthetic Album"}]
                })),
                section(json!({
                    "type": "SOME_FUTURE_TYPE",
                    "title": "New From TIDAL",
                    "items": []
                })),
            ],
            ..State::default()
        };
        let tokens = Tokens::dark();
        let images = crate::ui::images::ImageCache::new(8);
        let mut ui = simulator(state.view(tokens, &images));
        assert!(ui.find("Suggested New Albums").is_ok());
        assert!(ui.find("Synthetic Album").is_ok());
        assert!(ui.find("New From TIDAL").is_ok());
        assert!(
            ui.find("\"SOME_FUTURE_TYPE\" sections aren't supported yet")
                .is_ok()
        );
    }

    #[test]
    fn loading_state_shows_a_loading_note_instead_of_stale_sections() {
        let state = State::default();
        let tokens = Tokens::dark();
        let images = crate::ui::images::ImageCache::new(8);
        let mut ui = simulator(state.view(tokens, &images));
        assert!(ui.find("Loading…").is_ok());
    }
}
