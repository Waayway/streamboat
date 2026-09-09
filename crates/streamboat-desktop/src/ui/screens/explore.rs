//! Explore (task item 3): the still-live v1 `pages/explore` shape (D-015 —
//! Explore has no v2 feed counterpart), rendered with the same graceful
//! unknown-module fallback as Home. No tab bar and no cursor: v1 pages are
//! one shot, per `ApiClient::explore`'s doc comment.

use iced::Task;

use streamboat_core::models::{FeedItem, ItemsWrap, PageModuleV1, PageV1};
use streamboat_core::{ApiClient, Error};

use crate::ui::design::Tokens;
use crate::ui::format::page_module_to_view;
use crate::ui::nav::EntityRef;

pub struct State {
    modules: Vec<PageModuleV1>,
    loading: bool,
    error: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            modules: Vec::new(),
            loading: true,
            error: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Result<PageV1, String>),
    ViewAll(usize, String),
    ViewAllLoaded(usize, Result<Vec<FeedItem>, String>),
    CardClicked(EntityRef),
}

pub enum Effect {
    Navigate(EntityRef),
    ImagesNeeded(Vec<String>),
}

impl State {
    pub fn load(api: &ApiClient) -> Task<Message> {
        Task::perform(load_explore(api.clone()), Message::Loaded)
    }

    pub fn update(&mut self, message: Message, api: &ApiClient) -> (Task<Message>, Vec<Effect>) {
        match message {
            Message::Loaded(Ok(page)) => {
                self.loading = false;
                self.error = None;
                self.modules = page.rows.into_iter().flat_map(|r| r.modules).collect();
                (Task::none(), vec![Effect::ImagesNeeded(self.image_ids())])
            }
            Message::Loaded(Err(e)) => {
                self.loading = false;
                self.error = Some(e);
                (Task::none(), Vec::new())
            }
            Message::ViewAll(index, api_path) => {
                let offset = self
                    .modules
                    .get(index)
                    .map(|m| m.items.len() as u32)
                    .unwrap_or(0);
                (
                    Task::perform(expand(api.clone(), api_path, 20, offset), move |r| {
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
                if let Some(module) = self.modules.get_mut(index) {
                    module.items.extend(
                        items
                            .iter()
                            .map(|i| serde_json::to_value(i).unwrap_or_default()),
                    );
                }
                (Task::none(), vec![Effect::ImagesNeeded(images)])
            }
            Message::ViewAllLoaded(_, Err(e)) => {
                self.error = Some(format!("couldn't load more: {e}"));
                (Task::none(), Vec::new())
            }
            Message::CardClicked(entity) => (Task::none(), vec![Effect::Navigate(entity)]),
        }
    }

    fn image_ids(&self) -> Vec<String> {
        self.modules
            .iter()
            .flat_map(|m| m.items.iter())
            .filter_map(crate::ui::format::raw_item_image_id)
            .collect()
    }

    pub fn view<'a>(
        &'a self,
        tokens: Tokens,
        images: &crate::ui::images::ImageCache,
    ) -> iced::Element<'a, Message> {
        let sections = self
            .modules
            .iter()
            .enumerate()
            .map(|(i, m)| (i, page_module_to_view(m)))
            .collect();
        crate::ui::screens::feed_body(
            tokens,
            self.loading,
            self.error.as_deref(),
            None,
            sections,
            images,
            None,
            Message::ViewAll,
            Message::CardClicked,
        )
    }
}

async fn load_explore(api: ApiClient) -> Result<PageV1, String> {
    api.explore().await.map_err(fmt_err)
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

    fn module(json: serde_json::Value) -> PageModuleV1 {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn renders_known_and_unknown_v1_modules() {
        let state = State {
            loading: false,
            modules: vec![
                module(json!({
                    "type": "ALBUM_LIST",
                    "title": "New Releases",
                    "pagedList": {"items": [{"title": "Synthetic Album"}]}
                })),
                module(json!({"type": "TEXT_BLOCK", "title": "About TIDAL Rising"})),
            ],
            ..State::default()
        };
        let tokens = Tokens::dark();
        let images = crate::ui::images::ImageCache::new(8);
        let mut ui = simulator(state.view(tokens, &images));
        assert!(ui.find("New Releases").is_ok());
        assert!(ui.find("Synthetic Album").is_ok());
        assert!(
            ui.find("\"TEXT_BLOCK\" sections aren't supported yet")
                .is_ok()
        );
    }
}
