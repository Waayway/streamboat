//! The image cache: a memory LRU keyed by URL, fetched with a
//! plain `reqwest` client and decoded off the UI thread. `Task::perform`
//! runs the fetch-and-decode future on iced's own executor, not inside the
//! synchronous `update`/`view` call, so a slow network or a large cover
//! never blocks a frame.

use std::collections::{HashMap, VecDeque};

use iced::widget::image::Handle;

/// A small in-memory LRU. Bounded by item count, not bytes — TIDAL cover
/// art is capped at 1280×1280 (`api/images.rs`'s `ALBUM_COVER_SIZES`), so a
/// few hundred entries is a small, predictable amount of memory.
pub struct ImageCache {
    entries: HashMap<String, Handle>,
    order: VecDeque<String>,
    capacity: usize,
}

impl ImageCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }

    pub fn contains(&self, url: &str) -> bool {
        self.entries.contains_key(url)
    }

    /// Read-only lookup that does not touch LRU order — for view code,
    /// which only ever holds `&ImageCache` (mutating during `view()` would
    /// make an unrelated render reorder eviction priority).
    pub fn peek(&self, url: &str) -> Option<Handle> {
        self.entries.get(url).cloned()
    }

    pub fn insert(&mut self, url: String, handle: Handle) {
        if self.entries.insert(url.clone(), handle).is_none() {
            self.order.push_back(url.clone());
        }
        self.touch(&url);
        while self.order.len() > self.capacity {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&oldest);
        }
    }

    fn touch(&mut self, url: &str) {
        if let Some(pos) = self.order.iter().position(|u| u == url) {
            let entry = self.order.remove(pos).expect("position just found");
            self.order.push_back(entry);
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Fetch `url` and decode it off the UI thread, returning the URL back
/// alongside the result so the caller can key the cache and know which
/// in-flight request just finished. `None` on any network or decode error;
/// callers show a placeholder rather than surfacing a hard error for one
/// missing image.
pub fn fetch(client: reqwest::Client, url: String) -> iced::Task<(String, Option<Handle>)> {
    let for_message = url.clone();
    iced::Task::perform(load(client, url), move |handle| {
        (for_message.clone(), handle)
    })
}

async fn load(client: reqwest::Client, url: String) -> Option<Handle> {
    let response = client.get(&url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let bytes = response.bytes().await.ok()?;
    // Decode on a blocking thread: `image::load_from_memory` is CPU-bound
    // and would otherwise stall the async executor's worker.
    tokio::task::spawn_blocking(move || decode(&bytes))
        .await
        .ok()?
}

fn decode(bytes: &[u8]) -> Option<Handle> {
    let image = ::image::load_from_memory(bytes).ok()?.to_rgba8();
    let (width, height) = (image.width(), image.height());
    Some(Handle::from_rgba(width, height, image.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_handle() -> Handle {
        Handle::from_rgba(1, 1, vec![0, 0, 0, 255])
    }

    #[test]
    fn insert_then_get_round_trips() {
        let mut cache = ImageCache::new(4);
        cache.insert("https://example.com/a.jpg".into(), dummy_handle());
        assert!(cache.peek("https://example.com/a.jpg").is_some());
        assert!(cache.peek("https://example.com/missing.jpg").is_none());
    }

    #[test]
    fn evicts_the_oldest_entry_past_capacity() {
        // `peek` (the only read the app ever calls from immutable `view`
        // code) deliberately never touches LRU order, so eviction is
        // insertion-order: re-inserting "a" is what bumps it, not reading it.
        let mut cache = ImageCache::new(2);
        cache.insert("a".into(), dummy_handle());
        cache.insert("b".into(), dummy_handle());
        cache.insert("a".into(), dummy_handle());
        cache.insert("c".into(), dummy_handle());

        assert_eq!(cache.len(), 2);
        assert!(cache.contains("a"));
        assert!(cache.contains("c"));
        assert!(!cache.contains("b"));
    }

    #[test]
    fn re_inserting_an_existing_url_does_not_grow_the_cache() {
        let mut cache = ImageCache::new(2);
        cache.insert("a".into(), dummy_handle());
        cache.insert("a".into(), dummy_handle());
        assert_eq!(cache.len(), 1);
    }
}
