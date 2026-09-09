//! Search across types (tracks, albums, artists, playlists, videos, top
//! hits), with `limit`/`offset` (`tidal-api` catalog-and-library.md §1).

use super::pagination::clamp_limit;
use crate::error::Result;
use crate::http::ApiClient;
use crate::models::SearchResults;

impl ApiClient {
    /// `GET search?query=&limit=&offset=&types=` (v1). Response:
    /// `artists`/`albums`/`tracks`/`videos`/`playlists` each `{items:
    /// [...]}` plus one `topHit: {type, value}` — the shape
    /// [`SearchResults`] models. `types` is sent as the default string
    /// python-tidal uses, including its ungrammatical `mixs` (a mechanical
    /// pluralization of the identifier `mix`, not a typo to "fix").
    ///
    /// TIDAL's own client and Sone both prefer the v2 endpoint
    /// (`GET /v2/search`, uppercase `types=ARTISTS,ALBUMS,...`) — but no
    /// reference this crate cites documents v2's **response** shape, only
    /// its request parameters (`tidal-api` catalog-and-library.md §1). This
    /// method uses v1, whose response shape *is* documented, rather than
    /// guess at v2's; revisit once a v2 response has been captured.
    ///
    /// Regardless of `limit`/`offset`, TIDAL caps search at 300 items
    /// total (`tidal-api` transport §4) — this method does not try to page
    /// past that.
    pub async fn search(&self, query: &str, limit: u32, offset: u32) -> Result<SearchResults> {
        let cc = self.country_code().await?;
        self.get_json(
            "v1/search",
            &[
                ("query", query.to_string()),
                ("limit", clamp_limit(limit).to_string()),
                ("offset", offset.to_string()),
                (
                    "types",
                    "artists,albums,tracks,videos,playlists,mixs".to_string(),
                ),
                ("countryCode", cc),
            ],
            &[],
        )
        .await
    }
}
