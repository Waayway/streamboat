//! Entity pages: album, artist, playlist, mix and video metadata. Track
//! itself is already covered by [`crate::api::ApiClient::track`] in
//! `api/mod.rs`; track credits live here alongside the other entity calls.
//!
//! Endpoints, parameters and response shapes are exactly as documented in
//! `tidal-api` catalog-and-library.md §2-§4 unless a doc comment on the
//! specific method says otherwise.

use serde::Deserialize;

use super::pagination::{self, clamp_limit};
use crate::error::{Error, Result};
use crate::http::ApiClient;
use crate::models::{
    Album, ArtistAlbumFilter, ArtistProfile, Credit, Page, Playlist, PlaylistItem, TextWithSource,
    Track, Video,
};

impl ApiClient {
    // -- Album ---------------------------------------------------------

    /// `GET /v1/albums/{id}`.
    pub async fn album(&self, id: u64) -> Result<Album> {
        let cc = self.country_code().await?;
        self.get_json(&format!("v1/albums/{id}"), &[("countryCode", cc)], &[])
            .await
    }

    /// `GET /v1/albums/{id}/tracks`.
    pub async fn album_tracks(&self, id: u64, limit: u32, offset: u32) -> Result<Page<Track>> {
        let cc = self.country_code().await?;
        self.get_json(
            &format!("v1/albums/{id}/tracks"),
            &[
                ("limit", clamp_limit(limit).to_string()),
                ("offset", offset.to_string()),
                ("countryCode", cc),
            ],
            &[],
        )
        .await
    }

    /// `GET /v1/albums/{id}/items` — tracks *and* videos, each wrapped
    /// `{item, type}` like a playlist's items (`tidal-api`
    /// catalog-and-library.md §2).
    pub async fn album_items(
        &self,
        id: u64,
        limit: u32,
        offset: u32,
    ) -> Result<Page<PlaylistItem>> {
        let cc = self.country_code().await?;
        self.get_json(
            &format!("v1/albums/{id}/items"),
            &[
                ("limit", clamp_limit(limit).to_string()),
                ("offset", offset.to_string()),
                ("countryCode", cc),
            ],
            &[],
        )
        .await
    }

    /// `GET /v1/albums/{id}/similar` — a 404 means "no similar albums," per
    /// the reference, not an error; returns `Ok(vec![])` for that case.
    pub async fn album_similar(&self, id: u64) -> Result<Vec<Album>> {
        let cc = self.country_code().await?;
        match self
            .get_json::<Page<Album>>(
                &format!("v1/albums/{id}/similar"),
                &[("countryCode", cc)],
                &[],
            )
            .await
        {
            Ok(p) => Ok(p.items),
            Err(Error::Api(e)) if e.status == 404 => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    /// `GET /v1/albums/{id}/review` -> `{text}`. There is **no dedicated
    /// album-credits endpoint on the unofficial API** (`tidal-api`
    /// catalog-and-library.md §4: credits arrive only as a `credits` module
    /// inside the `pages/album` module, or via the official API's
    /// `/credits/{id}`) — this crate deliberately does not implement an
    /// `album_credits` method for that reason, not by oversight. Track
    /// credits, which *do* have a dedicated endpoint, are
    /// [`ApiClient::track_credits`].
    pub async fn album_review(&self, id: u64) -> Result<Option<String>> {
        let cc = self.country_code().await?;
        match self
            .get_json::<TextWithSource>(
                &format!("v1/albums/{id}/review"),
                &[("countryCode", cc)],
                &[],
            )
            .await
        {
            Ok(r) => Ok(r.text),
            Err(Error::Api(e)) if e.status == 404 => Ok(None),
            Err(e) => Err(e),
        }
    }

    // -- Artist ---------------------------------------------------------

    /// `GET /v1/artists/{id}`.
    pub async fn artist(&self, id: u64) -> Result<ArtistProfile> {
        let cc = self.country_code().await?;
        self.get_json(&format!("v1/artists/{id}"), &[("countryCode", cc)], &[])
            .await
    }

    /// `GET /v1/artists/{id}/toptracks`.
    pub async fn artist_top_tracks(&self, id: u64, limit: u32, offset: u32) -> Result<Page<Track>> {
        let cc = self.country_code().await?;
        self.get_json(
            &format!("v1/artists/{id}/toptracks"),
            &[
                ("limit", clamp_limit(limit).to_string()),
                ("offset", offset.to_string()),
                ("countryCode", cc),
            ],
            &[],
        )
        .await
    }

    /// `GET /v1/artists/{id}/albums[?filter=EPSANDSINGLES|COMPILATIONS]` —
    /// omit `filter` for the main discography.
    pub async fn artist_albums(
        &self,
        id: u64,
        filter: Option<ArtistAlbumFilter>,
        limit: u32,
        offset: u32,
    ) -> Result<Page<Album>> {
        let cc = self.country_code().await?;
        let mut query = vec![
            ("limit", clamp_limit(limit).to_string()),
            ("offset", offset.to_string()),
            ("countryCode", cc),
        ];
        if let Some(f) = filter {
            query.push(("filter", f.as_str().to_string()));
        }
        self.get_json(&format!("v1/artists/{id}/albums"), &query, &[])
            .await
    }

    /// `GET /v1/artists/{id}/similar`. Whether a 404 here means "none," as
    /// it does for `albums/{id}/similar`, is not independently confirmed by
    /// any reference this crate cites for the artist endpoint specifically
    /// — applied here by analogy, not as a confirmed fact.
    pub async fn artist_similar(&self, id: u64) -> Result<Vec<ArtistProfile>> {
        let cc = self.country_code().await?;
        match self
            .get_json::<Page<ArtistProfile>>(
                &format!("v1/artists/{id}/similar"),
                &[("countryCode", cc)],
                &[],
            )
            .await
        {
            Ok(p) => Ok(p.items),
            Err(Error::Api(e)) if e.status == 404 => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    /// `GET /v1/artists/{id}/bio` -> `{text}`, plus a `source` attribution
    /// the desktop client shows (e.g. "TiVo") that python-tidal's own
    /// `get_bio()` discards (`tidal-client-features`
    /// library-playlists-collections.md §3) — kept here since nothing
    /// forces streamboat to drop it too. A 404 is treated as "no bio," by
    /// analogy with the confirmed `albums/{id}/similar` 404 behaviour, not
    /// as an independently confirmed fact for this endpoint.
    pub async fn artist_bio(&self, id: u64) -> Result<Option<TextWithSource>> {
        let cc = self.country_code().await?;
        match self
            .get_json::<TextWithSource>(
                &format!("v1/artists/{id}/bio"),
                &[("countryCode", cc)],
                &[],
            )
            .await
        {
            Ok(b) => Ok(Some(b)),
            Err(Error::Api(e)) if e.status == 404 => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// `GET /v1/artists/{id}/mix` -> `{id: "<mixId>"}` (artist radio as a
    /// Mix).
    pub async fn artist_mix_id(&self, id: u64) -> Result<Option<String>> {
        #[derive(Deserialize, Default)]
        struct MixRef {
            id: Option<String>,
        }
        let cc = self.country_code().await?;
        let m: MixRef = self
            .get_json(&format!("v1/artists/{id}/mix"), &[("countryCode", cc)], &[])
            .await?;
        Ok(m.id)
    }

    // -- Track credits ---------------------------------------------------

    /// `GET /v1/tracks/{id}/credits` -> array of `{type, contributors:
    /// [{name, id}]}`.
    pub async fn track_credits(&self, id: u64) -> Result<Vec<Credit>> {
        let cc = self.country_code().await?;
        self.get_json(
            &format!("v1/tracks/{id}/credits"),
            &[("countryCode", cc)],
            &[],
        )
        .await
    }

    /// `GET /v1/tracks/{id}/mix` -> `{id: "<mixId>"}` (track radio as a
    /// Mix).
    pub async fn track_mix_id(&self, id: u64) -> Result<Option<String>> {
        #[derive(Deserialize, Default)]
        struct MixRef {
            id: Option<String>,
        }
        let cc = self.country_code().await?;
        let m: MixRef = self
            .get_json(&format!("v1/tracks/{id}/mix"), &[("countryCode", cc)], &[])
            .await?;
        Ok(m.id)
    }

    // -- Playlist ---------------------------------------------------------

    /// `GET /v1/playlists/{uuid}`. Use [`ApiClient::playlist_etag`]
    /// separately when you need the write-precondition ETag for a mutation
    /// — this method does not return response headers.
    pub async fn playlist(&self, uuid: &str) -> Result<Playlist> {
        let cc = self.country_code().await?;
        self.get_json(&format!("v1/playlists/{uuid}"), &[("countryCode", cc)], &[])
            .await
    }

    /// `GET /v1/playlists/{uuid}/tracks?limit&offset&order&orderDirection`.
    pub async fn playlist_tracks(
        &self,
        uuid: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Page<Track>> {
        let cc = self.country_code().await?;
        self.get_json(
            &format!("v1/playlists/{uuid}/tracks"),
            &[
                ("limit", clamp_limit(limit).to_string()),
                ("offset", offset.to_string()),
                ("countryCode", cc),
            ],
            &[],
        )
        .await
    }

    /// `GET /v1/playlists/{uuid}/items?limit&offset` — tracks *and* videos,
    /// each wrapped `{item, type}` (`tidal-api` catalog-and-library.md §7).
    pub async fn playlist_items(
        &self,
        uuid: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Page<PlaylistItem>> {
        let cc = self.country_code().await?;
        self.get_json(
            &format!("v1/playlists/{uuid}/items"),
            &[
                ("limit", clamp_limit(limit).to_string()),
                ("offset", offset.to_string()),
                ("countryCode", cc),
            ],
            &[],
        )
        .await
    }

    /// Fetch every item of a playlist, paging with [`pagination::collect_all`].
    /// `cap` bounds a pathologically large or misbehaving playlist; pass
    /// something like `10_000` for "effectively the whole playlist."
    pub async fn playlist_items_all(&self, uuid: &str, cap: usize) -> Result<Vec<PlaylistItem>> {
        pagination::collect_all(pagination::DEFAULT_PAGE_SIZE, cap, |offset, limit| {
            self.playlist_items(uuid, limit, offset)
        })
        .await
    }

    /// `GET /v1/playlists/{uuid}/recommendations/items?limit&offset` —
    /// suggested additions.
    pub async fn playlist_recommendations(
        &self,
        uuid: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Page<Track>> {
        let cc = self.country_code().await?;
        self.get_json(
            &format!("v1/playlists/{uuid}/recommendations/items"),
            &[
                ("limit", clamp_limit(limit).to_string()),
                ("offset", offset.to_string()),
                ("countryCode", cc),
            ],
            &[],
        )
        .await
    }

    // -- Mix ---------------------------------------------------------

    /// `GET /v1/mixes/{id}/items` — documented only as "legacy fallback for
    /// mix contents" (`tidal-api` catalog-and-library.md §2), with no
    /// response shape spelled out anywhere this crate cites. Modeled here
    /// as the same `{items: [{item, type}], limit, offset,
    /// totalNumberOfItems}` page shape `playlists/{uuid}/items` uses, since
    /// both are v1 collection endpoints and no counter-example exists — this
    /// is a best-effort guess, not a confirmed fact; verify against a
    /// captured fixture before depending on it in a release build.
    pub async fn mix_items(
        &self,
        mix_id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Page<PlaylistItem>> {
        let cc = self.country_code().await?;
        self.get_json(
            &format!("v1/mixes/{mix_id}/items"),
            &[
                ("limit", clamp_limit(limit).to_string()),
                ("offset", offset.to_string()),
                ("countryCode", cc),
            ],
            &[],
        )
        .await
    }

    // -- Video (metadata only; no playback — later scope) ------------------

    /// `GET /v1/videos/{id}` — metadata only. streamboat does not play video
    /// yet (`tidal-client-features` browse-pages-screens.md §7: "later,
    /// desktop only, as a separate HLS path"), so there is deliberately no
    /// `video_playback_info`/`video_url` method here.
    pub async fn video(&self, id: u64) -> Result<Video> {
        let cc = self.country_code().await?;
        self.get_json(&format!("v1/videos/{id}"), &[("countryCode", cc)], &[])
            .await
    }
}
