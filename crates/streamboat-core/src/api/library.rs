//! My Collection: favourite tracks/albums/artists/playlists/mixes lists with
//! pagination and ordering, favourites add/remove, and the user's playlists
//! (including folders where documented). Base path `users/{userId}/favorites`
//! (`tidal-api` catalog-and-library.md §6-§7).
//!
//! Every add/remove method here is a **library write** (D-028: "Favourites,
//! playlist create/edit/reorder ... and the queue; nothing else").

use super::pagination::clamp_limit;
use crate::error::Result;
use crate::http::ApiClient;
use crate::models::{
    Album, ArtistProfile, Favorite, FavoriteIds, MixSummary, Page, Playlist, Track,
};

/// `orderDirection` on every favourites/folder list endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDirection {
    Asc,
    Desc,
}

impl OrderDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            OrderDirection::Asc => "ASC",
            OrderDirection::Desc => "DESC",
        }
    }
}

/// Sort orders for `favorites/albums` (`tidal-api` catalog-and-library.md §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumOrder {
    Artist,
    Date,
    Name,
    ReleaseDate,
}
impl AlbumOrder {
    fn as_str(self) -> &'static str {
        match self {
            AlbumOrder::Artist => "ARTIST",
            AlbumOrder::Date => "DATE",
            AlbumOrder::Name => "NAME",
            AlbumOrder::ReleaseDate => "RELEASE_DATE",
        }
    }
}

/// See [`ArtistOrder`]'s `Display` impl for why this exists (a `pick_list`
/// label, not the wire value).
impl std::fmt::Display for AlbumOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlbumOrder::Artist => write!(f, "Artist"),
            AlbumOrder::Date => write!(f, "Date added"),
            AlbumOrder::Name => write!(f, "Name"),
            AlbumOrder::ReleaseDate => write!(f, "Release date"),
        }
    }
}

/// Sort orders for `favorites/artists`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtistOrder {
    Date,
    Name,
}
impl ArtistOrder {
    fn as_str(self) -> &'static str {
        match self {
            ArtistOrder::Date => "DATE",
            ArtistOrder::Name => "NAME",
        }
    }
}

/// A human label for a `pick_list` — the wire value is `as_str()`
/// (`"DATE"`/`"NAME"`/...); this is title-cased for display, not sent over
/// the wire (`ui::screens::collection`'s sort-order pickers use this).
impl std::fmt::Display for ArtistOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArtistOrder::Date => write!(f, "Date added"),
            ArtistOrder::Name => write!(f, "Name"),
        }
    }
}

/// Sort orders for `favorites/tracks` and `favorites/videos` ("items").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemOrder {
    Album,
    Artist,
    Date,
    Index,
    Length,
    Name,
}
impl ItemOrder {
    fn as_str(self) -> &'static str {
        match self {
            ItemOrder::Album => "ALBUM",
            ItemOrder::Artist => "ARTIST",
            ItemOrder::Date => "DATE",
            ItemOrder::Index => "INDEX",
            ItemOrder::Length => "LENGTH",
            ItemOrder::Name => "NAME",
        }
    }
}

/// See [`ArtistOrder`]'s `Display` impl for why this exists (a `pick_list`
/// label, not the wire value).
impl std::fmt::Display for ItemOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ItemOrder::Album => write!(f, "Album"),
            ItemOrder::Artist => write!(f, "Artist"),
            ItemOrder::Date => write!(f, "Date added"),
            ItemOrder::Index => write!(f, "Playlist order"),
            ItemOrder::Length => write!(f, "Length"),
            ItemOrder::Name => write!(f, "Name"),
        }
    }
}

/// Sort orders for `favorites/playlists`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistOrder {
    Date,
    Name,
}
impl PlaylistOrder {
    fn as_str(self) -> &'static str {
        match self {
            PlaylistOrder::Date => "DATE",
            PlaylistOrder::Name => "NAME",
        }
    }
}

/// Sort orders for `v2/favorites/mixes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixOrder {
    Date,
    MixType,
    Name,
}
impl MixOrder {
    fn as_str(self) -> &'static str {
        match self {
            MixOrder::Date => "DATE",
            MixOrder::MixType => "MIX_TYPE",
            MixOrder::Name => "NAME",
        }
    }
}

/// See [`ArtistOrder`]'s `Display` impl for why this exists (a `pick_list`
/// label, not the wire value).
impl std::fmt::Display for MixOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MixOrder::Date => write!(f, "Date added"),
            MixOrder::MixType => write!(f, "Mix type"),
            MixOrder::Name => write!(f, "Name"),
        }
    }
}

impl ApiClient {
    // -- Favourites: list -------------------------------------------------

    /// `GET users/{id}/favorites/tracks?limit&offset&order&orderDirection`.
    pub async fn favorite_tracks(
        &self,
        limit: u32,
        offset: u32,
        order: Option<ItemOrder>,
        direction: Option<OrderDirection>,
    ) -> Result<Page<Favorite<Track>>> {
        self.favorites_page(
            "tracks",
            limit,
            offset,
            order.map(ItemOrder::as_str),
            direction,
        )
        .await
    }

    /// `GET users/{id}/favorites/albums?limit&offset&order&orderDirection`.
    pub async fn favorite_albums(
        &self,
        limit: u32,
        offset: u32,
        order: Option<AlbumOrder>,
        direction: Option<OrderDirection>,
    ) -> Result<Page<Favorite<Album>>> {
        self.favorites_page(
            "albums",
            limit,
            offset,
            order.map(AlbumOrder::as_str),
            direction,
        )
        .await
    }

    /// `GET users/{id}/favorites/artists?limit&offset&order&orderDirection`.
    pub async fn favorite_artists(
        &self,
        limit: u32,
        offset: u32,
        order: Option<ArtistOrder>,
        direction: Option<OrderDirection>,
    ) -> Result<Page<Favorite<ArtistProfile>>> {
        self.favorites_page(
            "artists",
            limit,
            offset,
            order.map(ArtistOrder::as_str),
            direction,
        )
        .await
    }

    /// `GET users/{id}/favorites/playlists?countryCode&limit&offset` — a
    /// fifth favourites collection, separate from the four entity types
    /// above (`tidal-api` catalog-and-library.md §6).
    pub async fn favorite_playlists(
        &self,
        limit: u32,
        offset: u32,
        order: Option<PlaylistOrder>,
        direction: Option<OrderDirection>,
    ) -> Result<Page<Favorite<Playlist>>> {
        self.favorites_page(
            "playlists",
            limit,
            offset,
            order.map(PlaylistOrder::as_str),
            direction,
        )
        .await
    }

    /// `GET v2/favorites/mixes?limit&offset&order&orderDirection&countryCode&locale&deviceType` —
    /// mixes use their own v2 endpoint family, distinct from the v1
    /// favourites above (`tidal-api` catalog-and-library.md §6).
    pub async fn favorite_mixes(
        &self,
        limit: u32,
        offset: u32,
        order: Option<MixOrder>,
        direction: Option<OrderDirection>,
    ) -> Result<Page<Favorite<MixSummary>>> {
        let cc = self.country_code().await?;
        let mut query = vec![
            ("limit", clamp_limit(limit).to_string()),
            ("offset", offset.to_string()),
            ("countryCode", cc),
            ("locale", self.locale()),
            ("deviceType", "BROWSER".to_string()),
        ];
        if let Some(o) = order {
            query.push(("order", o.as_str().to_string()));
        }
        if let Some(d) = direction {
            query.push(("orderDirection", d.as_str().to_string()));
        }
        self.get_json("v2/favorites/mixes", &query, &[]).await
    }

    /// `GET users/{id}/favorites/ids?countryCode&locale&deviceType` — every
    /// id at once, as strings even for integer-id types. The cheap way to
    /// render "is favourited" across a whole view; fetch once per session.
    pub async fn favorite_ids(&self) -> Result<FavoriteIds> {
        let uid = self.user_id().await?;
        let cc = self.country_code().await?;
        self.get_json(
            &format!("v1/users/{uid}/favorites/ids"),
            &[
                ("countryCode", cc),
                ("locale", self.locale()),
                ("deviceType", "BROWSER".to_string()),
            ],
            &[],
        )
        .await
    }

    async fn favorites_page<T: serde::de::DeserializeOwned>(
        &self,
        kind: &str,
        limit: u32,
        offset: u32,
        order: Option<&str>,
        direction: Option<OrderDirection>,
    ) -> Result<Page<Favorite<T>>> {
        let uid = self.user_id().await?;
        let cc = self.country_code().await?;
        let mut query = vec![
            ("limit", clamp_limit(limit).to_string()),
            ("offset", offset.to_string()),
            ("countryCode", cc),
        ];
        if let Some(o) = order {
            query.push(("order", o.to_string()));
        }
        if let Some(d) = direction {
            query.push(("orderDirection", d.as_str().to_string()));
        }
        self.get_json(&format!("v1/users/{uid}/favorites/{kind}"), &query, &[])
            .await
    }

    // -- Favourites: add/remove (library writes, D-028) -------------------

    /// `POST users/{id}/favorites/tracks` form `trackId=<id>`. Library write.
    pub async fn favorite_track(&self, track_id: u64) -> Result<()> {
        self.favorite_add("tracks", "trackId", &track_id.to_string())
            .await
    }

    /// `DELETE users/{id}/favorites/tracks/{id}`. Library write.
    pub async fn unfavorite_track(&self, track_id: u64) -> Result<()> {
        self.favorite_remove("tracks", &track_id.to_string()).await
    }

    /// `POST users/{id}/favorites/albums` form `albumId=<id>`. Library write.
    pub async fn favorite_album(&self, album_id: u64) -> Result<()> {
        self.favorite_add("albums", "albumId", &album_id.to_string())
            .await
    }

    /// `DELETE users/{id}/favorites/albums/{id}`. Library write.
    pub async fn unfavorite_album(&self, album_id: u64) -> Result<()> {
        self.favorite_remove("albums", &album_id.to_string()).await
    }

    /// `POST users/{id}/favorites/artists` form `artistId=<id>`. Library
    /// write. See [`ApiClient::follow_artist`] for why this is also what
    /// "follow" means in streamboat.
    pub async fn favorite_artist(&self, artist_id: u64) -> Result<()> {
        self.favorite_add("artists", "artistId", &artist_id.to_string())
            .await
    }

    /// `DELETE users/{id}/favorites/artists/{id}`. Library write.
    pub async fn unfavorite_artist(&self, artist_id: u64) -> Result<()> {
        self.favorite_remove("artists", &artist_id.to_string())
            .await
    }

    /// `POST users/{id}/favorites/videos?limit=100` form `videoIds=<id>` —
    /// **plural**, unlike the other three entity types above. Library
    /// write.
    pub async fn favorite_video(&self, video_id: u64) -> Result<()> {
        let uid = self.user_id().await?;
        let form = [("videoIds", video_id.to_string())];
        self.request_empty(
            reqwest::Method::POST,
            &format!("v1/users/{uid}/favorites/videos"),
            &[("limit", "100".to_string())],
            Some(&form),
            &[],
        )
        .await?;
        Ok(())
    }

    /// `DELETE users/{id}/favorites/videos/{id}`. Library write.
    pub async fn unfavorite_video(&self, video_id: u64) -> Result<()> {
        self.favorite_remove("videos", &video_id.to_string()).await
    }

    /// `POST users/{id}/favorites/playlists` form `uuid=<uuid>` —
    /// **singular `uuid`, a UUID, not `playlistId`**. Library write.
    pub async fn favorite_playlist(&self, uuid: &str) -> Result<()> {
        let uid = self.user_id().await?;
        let form = [("uuid", uuid.to_string())];
        self.request_empty(
            reqwest::Method::POST,
            &format!("v1/users/{uid}/favorites/playlists"),
            &[],
            Some(&form),
            &[],
        )
        .await?;
        Ok(())
    }

    /// `DELETE users/{id}/favorites/playlists/{uuid}`. Library write.
    pub async fn unfavorite_playlist(&self, uuid: &str) -> Result<()> {
        self.favorite_remove("playlists", uuid).await
    }

    /// `PUT v2/favorites/mixes/add?mixIds=a,b&onArtifactNotFound=FAIL`.
    /// Library write.
    pub async fn favorite_mixes_add(&self, mix_ids: &[&str]) -> Result<()> {
        self.request_empty(
            reqwest::Method::PUT,
            "v2/favorites/mixes/add",
            &[
                ("mixIds", mix_ids.join(",")),
                ("onArtifactNotFound", "FAIL".to_string()),
            ],
            None,
            &[],
        )
        .await?;
        Ok(())
    }

    /// `PUT v2/favorites/mixes/remove?mixIds=a,b`. Library write.
    pub async fn favorite_mixes_remove(&self, mix_ids: &[&str]) -> Result<()> {
        self.request_empty(
            reqwest::Method::PUT,
            "v2/favorites/mixes/remove",
            &[("mixIds", mix_ids.join(","))],
            None,
            &[],
        )
        .await?;
        Ok(())
    }

    async fn favorite_add(&self, kind: &str, field: &str, id: &str) -> Result<()> {
        let uid = self.user_id().await?;
        let form = [(field, id.to_string())];
        self.request_empty(
            reqwest::Method::POST,
            &format!("v1/users/{uid}/favorites/{kind}"),
            &[],
            Some(&form),
            &[],
        )
        .await?;
        Ok(())
    }

    async fn favorite_remove(&self, kind: &str, id: &str) -> Result<()> {
        let uid = self.user_id().await?;
        self.request_empty(
            reqwest::Method::DELETE,
            &format!("v1/users/{uid}/favorites/{kind}/{id}"),
            &[],
            None,
            &[],
        )
        .await?;
        Ok(())
    }

    // -- Follow artists (D-039) --------------------------------------------

    /// TIDAL's own client presents this as "Follow" on the artist page
    /// (`tidal-client-features` library-playlists-collections.md §3), but
    /// **the unofficial API has no endpoint distinct from favouriting an
    /// artist** — no `follow`/`unfollow` call exists on `api.tidal.com` in
    /// any reference this crate cites (`tidal-api` catalog-and-library.md
    /// §7: "Follow/unfollow does not exist on the unofficial API in any
    /// checkout"; the only follower surface anywhere is the *official*
    /// Developer API's `/artists/{id}/relationships/followers`, out of
    /// reach under D-028's library-writes-only rule). D-039 ("Following
    /// artists is a library feature") resolves this by scoping "follow" to
    /// exactly that: this method is a thin, explicitly-named alias over
    /// [`ApiClient::favorite_artist`] so a call site can say what the
    /// product means without re-deriving this note each time. Library
    /// write.
    pub async fn follow_artist(&self, artist_id: u64) -> Result<()> {
        self.favorite_artist(artist_id).await
    }

    /// See [`ApiClient::follow_artist`] — "unfollow" is unfavouriting.
    /// Library write.
    pub async fn unfollow_artist(&self, artist_id: u64) -> Result<()> {
        self.unfavorite_artist(artist_id).await
    }

    // -- User playlists (including folders where documented) --------------

    /// `GET users/{id}/playlists` — playlists the user created (v1).
    pub async fn user_playlists(&self) -> Result<Vec<Playlist>> {
        let uid = self.user_id().await?;
        let cc = self.country_code().await?;
        let page: Page<Playlist> = self
            .get_json(
                &format!("v1/users/{uid}/playlists"),
                &[("countryCode", cc)],
                &[],
            )
            .await?;
        Ok(page.items)
    }

    /// `GET users/{id}/playlistsAndFavoritePlaylists?limit=50` —
    /// **server-capped at 50**, and each item is a `{playlist, created}`
    /// wrapper, not a bare playlist (`tidal-api` catalog-and-library.md
    /// §7). This unwraps that envelope and folds `created` into
    /// [`Playlist::created`] when the playlist itself didn't already carry
    /// one — python-tidal does the same rewrite so the date isn't silently
    /// lost.
    pub async fn playlists_and_favorite_playlists(&self) -> Result<Vec<Playlist>> {
        #[derive(serde::Deserialize, Default)]
        #[serde(rename_all = "camelCase")]
        struct Wrapper {
            playlist: Playlist,
            created: Option<String>,
        }
        let uid = self.user_id().await?;
        let cc = self.country_code().await?;
        let page: Page<Wrapper> = self
            .get_json(
                &format!("v1/users/{uid}/playlistsAndFavoritePlaylists"),
                &[("limit", "50".to_string()), ("countryCode", cc)],
                &[],
            )
            .await?;
        Ok(page
            .items
            .into_iter()
            .map(|w| {
                let mut p = w.playlist;
                if p.created.is_none() {
                    p.created = w.created;
                }
                p
            })
            .collect())
    }

    /// `GET v2/my-collection/playlists/folders?folderId=&limit=&includeOnly=&order=&orderDirection=[&cursor=]`
    /// (`tidal-api` catalog-and-library.md §7). **The exact field names
    /// inside each folder-listing entry are not enumerated by any reference
    /// this crate cites** — only the endpoint, its parameters, and the TRN
    /// scheme (`trn:playlist:<uuid>`, `trn:folder:<id>`) are documented.
    /// Items are therefore returned as raw JSON; read `trn`/`name`/etc. at
    /// the call site until a captured fixture is available. **`offset` is
    /// ignored on this endpoint — page with `cursor` instead**, which the
    /// response also carries for the next page.
    pub async fn collection_folders(
        &self,
        folder_id: &str,
        include_only: Option<&str>,
        cursor: Option<String>,
    ) -> Result<(Vec<serde_json::Value>, Option<String>)> {
        #[derive(serde::Deserialize, Default)]
        struct FolderPage {
            #[serde(default)]
            items: Vec<serde_json::Value>,
            cursor: Option<String>,
        }
        let cc = self.country_code().await?;
        let mut query = vec![
            ("folderId", folder_id.to_string()),
            ("limit", clamp_limit(50).to_string()),
            ("order", "DATE".to_string()),
            ("orderDirection", "DESC".to_string()),
            ("countryCode", cc),
            ("locale", self.locale()),
        ];
        if let Some(io) = include_only {
            query.push(("includeOnly", io.to_string()));
        }
        if let Some(c) = cursor {
            query.push(("cursor", c));
        }
        let page: FolderPage = self
            .get_json("v2/my-collection/playlists/folders", &query, &[])
            .await?;
        Ok((page.items, page.cursor))
    }

    /// Every playlist/folder entry at `folder_id`, across every page, via
    /// [`super::pagination::collect_all_cursor`]. `cap` bounds a
    /// pathologically deep or misbehaving folder tree.
    pub async fn collection_folders_all(
        &self,
        folder_id: &str,
        include_only: Option<&str>,
        cap: usize,
    ) -> Result<Vec<serde_json::Value>> {
        super::pagination::collect_all_cursor(cap, |cursor| {
            self.collection_folders(folder_id, include_only, cursor)
        })
        .await
    }
}
