//! Home and Explore (D-015): the server-driven v2 `home/feed` sections
//! renderer input, plus the still-live v1 `pages/*` shape as an optional
//! fallback parser.
//!
//! Both generations are documented in `tidal-api` transport §1,
//! catalog-and-library.md §5 and `tidal-client-features`
//! browse-pages-screens.md §1-§3. D-015 requires: enumerate the tab bar from
//! the header, page on the top-level cursor, expand sections via their
//! `apiPath`, and fall back gracefully on an unknown section type — the
//! typed-but-open [`crate::models::FeedSection`]/[`crate::models::PageModuleV1`]
//! models carry a `raw` field precisely so an unrecognised `type` never
//! fails the whole page.

use crate::error::Result;
use crate::http::ApiClient;
use crate::models::{HomeFeed, HomeTab, PageV1};

impl ApiClient {
    /// `GET /v2/home/feed/{slug}?countryCode&locale&deviceType=BROWSER&platform=WEB[&cursor=]`
    /// (`tidal-api` transport §1, catalog-and-library.md §5). `slug` is the
    /// lowercased `type` of a `header.vibes.items[]` entry — `static`,
    /// `editorial` and `uploads` are the ones observed in the reference —
    /// or any other v2 feed slug you already know. Page on the response's
    /// top-level `cursor` by passing it back in as `cursor`.
    pub async fn home_feed(&self, slug: &str, cursor: Option<&str>) -> Result<HomeFeed> {
        let cc = self.country_code().await?;
        let mut query = vec![
            ("countryCode", cc),
            ("locale", self.locale()),
            ("deviceType", "BROWSER".to_string()),
            ("platform", "WEB".to_string()),
        ];
        if let Some(c) = cursor {
            query.push(("cursor", c.to_string()));
        }
        self.get_json(&format!("v2/home/feed/{slug}"), &query, &[])
            .await
    }

    /// Home's tab bar: `header.vibes.items[]` on the `static` slug's
    /// response (`tidal-client-features` browse-pages-screens.md §1).
    /// Fetches `home/feed/static` once, since that's the tab both TIDAL's
    /// own client and python-tidal's default request; call
    /// [`ApiClient::home_feed`] again with each tab's own
    /// [`HomeTab::slug`] to get that tab's sections.
    pub async fn home_tabs(&self) -> Result<Vec<HomeTab>> {
        let feed = self.home_feed("static", None).await?;
        Ok(feed
            .header
            .and_then(|h| h.vibes)
            .map(|v| v.items)
            .unwrap_or_default())
    }

    /// `GET pages/explore` (v1). Explore has no v2 feed counterpart
    /// documented anywhere this crate cites — only Home does
    /// (`tidal-client-features` browse-pages-screens.md §1 vs §3) — so this
    /// is the only supported call for Explore, not a fallback of a v2
    /// primary.
    pub async fn explore(&self) -> Result<PageV1> {
        self.page_v1("explore").await
    }

    /// The still-live v1 Pages shape: `GET pages/{slug}?deviceType=BROWSER&locale=&countryCode=`
    /// (`tidal-api` catalog-and-library.md §5). Kept as the optional
    /// fallback D-015 leaves open ("whether to also parse the v1 shape as a
    /// fallback... a later implementation call" — `docs/DECISIONS.md`
    /// D-015) and as the only supported shape for slugs with no v2
    /// equivalent: `pages/explore`, `pages/moods`, `pages/genre_page`,
    /// `pages/mix?mixId=`, `pages/my_collection_recently_played`, and more
    /// (`tidal-client-features` browse-pages-screens.md §3).
    pub async fn page_v1(&self, slug: &str) -> Result<PageV1> {
        let cc = self.country_code().await?;
        self.get_json(
            &format!("pages/{slug}"),
            &[
                ("deviceType", "BROWSER".to_string()),
                ("locale", self.locale()),
                ("countryCode", cc),
            ],
            &[],
        )
        .await
    }

    /// `GET pages/mix?mixId=<id>&deviceType=BROWSER&locale=&countryCode=` —
    /// the per-mix v1 page (`tidal-client-features`
    /// browse-pages-screens.md §4: "note the required query param — don't
    /// drop it"). Mixes have no v2 feed counterpart and no dedicated
    /// metadata endpoint beyond this page and [`ApiClient::mix_items`]
    /// (`api/catalog.rs`); this is the only documented way to get a mix's
    /// own title/subtitle short of walking a user's recommendations
    /// relationships, which no reference this crate cites has captured a
    /// response shape for. Used for the header only — track listing comes
    /// from [`ApiClient::mix_items`], whose typed shape is more reliable
    /// than parsing this page's raw modules.
    pub async fn mix_page(&self, mix_id: &str) -> Result<PageV1> {
        let cc = self.country_code().await?;
        self.get_json(
            "pages/mix",
            &[
                ("mixId", mix_id.to_string()),
                ("deviceType", "BROWSER".to_string()),
                ("locale", self.locale()),
                ("countryCode", cc),
            ],
            &[],
        )
        .await
    }

    /// Expand a "View All" target — a v2 [`crate::models::FeedSection::api_path`]
    /// or a v1 module's `showMore.apiPath`.
    ///
    /// **The response shape of a `View All` target is confirmed only for
    /// one worked example in any reference this crate cites: the
    /// artist-page pattern
    /// `artist/{id}/ARTIST_TOP_TRACKS/view-all?artistId=&limit=&offset=`**
    /// (`tidal-api` catalog-and-library.md §5's pitfall paragraph, which
    /// also warns that python-tidal's own `PageCategoryV2.view_all()` calls
    /// a method that does not exist anywhere in that package — "following
    /// it produces an `AttributeError`. ... do not copy it"). For any other
    /// `apiPath` this is a best-effort GET, not a confirmed contract — it
    /// returns raw JSON rather than a guessed struct for that reason; parse
    /// the fields you need at the call site once you know what the target
    /// actually returns.
    pub async fn expand_section(
        &self,
        api_path: &str,
        limit: u32,
        offset: u32,
    ) -> Result<serde_json::Value> {
        let cc = self.country_code().await?;
        let trimmed = api_path.trim_start_matches('/');
        let path = if trimmed.starts_with("v1/") || trimmed.starts_with("v2/") {
            trimmed.to_string()
        } else {
            // Every confirmed example (the artist view-all pattern above)
            // is a v2 path; assume v2 for a bare relative apiPath.
            format!("v2/{trimmed}")
        };
        self.get_json(
            &path,
            &[
                ("limit", limit.to_string()),
                ("offset", offset.to_string()),
                ("countryCode", cc),
                ("locale", self.locale()),
            ],
            &[],
        )
        .await
    }
}
