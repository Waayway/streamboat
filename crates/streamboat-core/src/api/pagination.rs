//! Small pagination helpers shared by every collection/favourites/search
//! call: the offset/limit caps TIDAL's servers actually enforce (`tidal-api`
//! transport §4), and two "fetch everything, but stop at a cap" helpers —
//! one offset-paginated, one cursor-paginated — built on [`crate::models::Page`].
//! The cap exists because a bug that turns either loop unbounded must fail
//! closed, never hang or hammer TIDAL with requests.

use std::future::Future;

use crate::error::Result;
use crate::models::Page;

/// Default page size this crate requests unless the caller asks for
/// something else.
pub const DEFAULT_PAGE_SIZE: u32 = 50;

/// The highest `limit` this crate will ever send on a v1 collection
/// endpoint. Real, reproduced server caps exist below this in places
/// (search tops out at 300 *total*, `playlistsAndFavoritePlaylists` is
/// capped at 50, `favorites/videos` rejects `limit=10000` outright) — this
/// is a client-side ceiling, not a claim that every endpoint accepts 100
/// (`tidal-api` transport §4: "never send `limit` above 100 on v1 collection
/// endpoints").
pub const MAX_PAGE_SIZE: u32 = 100;

/// Clamp a caller-supplied page size into `1..=MAX_PAGE_SIZE`.
pub fn clamp_limit(limit: u32) -> u32 {
    limit.clamp(1, MAX_PAGE_SIZE)
}

/// Fetch every item of an offset/limit-paginated collection by calling
/// `fetch(offset, limit)` repeatedly, stopping at `cap` items or as soon as
/// the server signals there's nothing more left (an empty page, a
/// short page, or `total_number_of_items` reached). Pass a large `cap`
/// (e.g. a few thousand) for "effectively unbounded" — never omit it.
pub async fn collect_all<T, F, Fut>(page_size: u32, cap: usize, mut fetch: F) -> Result<Vec<T>>
where
    F: FnMut(u32, u32) -> Fut,
    Fut: Future<Output = Result<Page<T>>>,
{
    let page_size = clamp_limit(page_size);
    let mut out = Vec::new();
    let mut offset: u32 = 0;
    loop {
        if out.len() >= cap {
            break;
        }
        let page = fetch(offset, page_size).await?;
        let got = page.items.len();
        let total = page.total_number_of_items;
        out.extend(page.items);
        if out.len() > cap {
            out.truncate(cap);
        }
        offset = offset.saturating_add(page_size);
        let exhausted =
            got == 0 || (got as u32) < page_size || total.is_some_and(|t| u64::from(offset) >= t);
        if exhausted {
            break;
        }
    }
    Ok(out)
}

/// The cursor-paginated equivalent of [`collect_all`], for the endpoints
/// that page on an opaque `cursor` instead of `offset` — the v2 home feed
/// and `my-collection/playlists/folders`, which the reference notes
/// "ignores `offset`" outright (`tidal-api` transport §4). `fetch` takes the
/// previous page's cursor (`None` for the first call) and returns the page's
/// items plus the next cursor (`None`, or an empty string, means "no more").
pub async fn collect_all_cursor<T, F, Fut>(cap: usize, mut fetch: F) -> Result<Vec<T>>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: Future<Output = Result<(Vec<T>, Option<String>)>>,
{
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        if out.len() >= cap {
            break;
        }
        let (items, next) = fetch(cursor.clone()).await?;
        if items.is_empty() {
            break;
        }
        out.extend(items);
        if out.len() > cap {
            out.truncate(cap);
        }
        match next {
            Some(c) if !c.is_empty() => cursor = Some(c),
            _ => break,
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn collect_all_stops_on_short_page() {
        let calls = AtomicU32::new(0);
        let items: Vec<u32> = collect_all(10, 1000, |offset, limit| {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            async move {
                assert_eq!(limit, 10);
                let page: Vec<u32> = match n {
                    0 => (offset..offset + 10).collect(),
                    1 => (offset..offset + 3).collect(),
                    _ => panic!("must not be called a third time"),
                };
                Ok(Page {
                    items: page,
                    limit: Some(10),
                    offset: Some(offset),
                    total_number_of_items: None,
                })
            }
        })
        .await
        .unwrap();
        assert_eq!(items.len(), 13);
    }

    #[tokio::test]
    async fn collect_all_respects_the_cap() {
        let items: Vec<u32> = collect_all(10, 5, |offset, limit| async move {
            Ok(Page {
                items: (offset..offset + limit).collect(),
                limit: Some(limit),
                offset: Some(offset),
                total_number_of_items: Some(1_000_000),
            })
        })
        .await
        .unwrap();
        assert_eq!(items.len(), 5);
    }

    #[tokio::test]
    async fn collect_all_cursor_stops_on_empty_cursor() {
        let calls = AtomicU32::new(0);
        let items: Vec<u32> = collect_all_cursor(1000, |cursor| {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            async move {
                match (n, cursor) {
                    (0, None) => Ok((vec![1, 2, 3], Some("next".to_string()))),
                    (1, Some(c)) if c == "next" => Ok((vec![4, 5], None)),
                    other => panic!("unexpected call {other:?}"),
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(items, vec![1, 2, 3, 4, 5]);
    }
}
