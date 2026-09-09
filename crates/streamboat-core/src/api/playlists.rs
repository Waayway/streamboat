//! Playlists CRUD: create, rename/describe, add tracks, remove item,
//! move/reorder, delete — every mutation guarded by the ETag /
//! `If-None-Match` write-precondition flow (`tidal-api`
//! catalog-and-library.md §7, `tidal-oss-landscape` sone-deep-dive.md §4b).
//!
//! **Every method in this file is a library write (D-028).**
//!
//! The ETag is a *write precondition*, not a cache validator — nobody in
//! the reference clients sends it on a read to get a cheap 304
//! (sone-deep-dive.md §4b). Every mutation below either takes an `etag`
//! you already hold from a recent fetch, or fetches one itself via
//! [`ApiClient::playlist_etag`] when you pass `None` — "fetch etag ->
//! mutate -> refetch" as one helper, per the reference's explicit
//! recommendation. **Re-fetch the etag after every mutation** if you plan
//! to mutate the same playlist again; this crate does not cache it for you
//! across calls, since a stale cached etag is exactly the bug ETags exist
//! to prevent.

use reqwest::Method;
use reqwest::header::ETAG;
use serde::Deserialize;

use crate::error::Result;
use crate::http::ApiClient;
use crate::models::Playlist;

/// `onDupes` on `POST playlists/{uuid}/items`. `Fail` is attested only in
/// Sone's shipped code, not observed against a live response — python-tidal
/// only ever sends `Add`/`Skip` at its own call sites (`tidal-api`
/// catalog-and-library.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnDupes {
    #[default]
    Add,
    Skip,
    Fail,
}
impl OnDupes {
    fn as_str(self) -> &'static str {
        match self {
            OnDupes::Add => "ADD",
            OnDupes::Skip => "SKIP",
            OnDupes::Fail => "FAIL",
        }
    }
}

/// `onArtifactNotFound` on the same endpoint. `Fail` *is* independently
/// verified: python-tidal's `merge()` sends it when `allow_missing` is
/// false (`tidal-api` catalog-and-library.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnArtifactNotFound {
    #[default]
    Skip,
    Fail,
}
impl OnArtifactNotFound {
    fn as_str(self) -> &'static str {
        match self {
            OnArtifactNotFound::Skip => "SKIP",
            OnArtifactNotFound::Fail => "FAIL",
        }
    }
}

impl ApiClient {
    /// Fetch the playlist and return its `etag` response header — the write
    /// precondition every mutation below needs. Falls back to `"*"` when
    /// TIDAL omits the header, matching every reference client's default
    /// (`tidal-oss-landscape` sone-deep-dive.md §4b).
    pub async fn playlist_etag(&self, uuid: &str) -> Result<String> {
        let cc = self.country_code().await?;
        let (_playlist, headers) = self
            .request_json::<Playlist>(
                Method::GET,
                &format!("v1/playlists/{uuid}"),
                &[("countryCode", cc)],
                None,
                &[],
            )
            .await?;
        Ok(headers
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("*")
            .to_string())
    }

    async fn resolve_etag(&self, uuid: &str, etag: Option<&str>) -> Result<String> {
        match etag {
            Some(e) => Ok(e.to_string()),
            None => self.playlist_etag(uuid).await,
        }
    }

    /// `PUT my-collection/playlists/folders/create-playlist?name=&description=&folderId=root`
    /// (`tidal-api` catalog-and-library.md §7). **The response shape isn't
    /// documented by any reference this crate cites** — this returns raw
    /// JSON rather than a guessed struct; fetch the playlist by whatever id
    /// the response names, with [`ApiClient::playlist`], once you know its
    /// field name. Library write.
    pub async fn create_playlist(
        &self,
        name: &str,
        description: Option<&str>,
        folder_id: Option<&str>,
    ) -> Result<serde_json::Value> {
        let cc = self.country_code().await?;
        let mut query = vec![
            ("name", name.to_string()),
            ("folderId", folder_id.unwrap_or("root").to_string()),
            ("countryCode", cc),
        ];
        if let Some(d) = description {
            query.push(("description", d.to_string()));
        }
        let (v, _) = self
            .request_json::<serde_json::Value>(
                Method::PUT,
                "v2/my-collection/playlists/folders/create-playlist",
                &query,
                None,
                &[],
            )
            .await?;
        Ok(v)
    }

    /// `POST playlists/{uuid}` form `title=&description=`,
    /// `If-None-Match: <etag>` (`tidal-api` catalog-and-library.md §7). The
    /// response body isn't documented; this only asserts success. Library
    /// write.
    pub async fn update_playlist(
        &self,
        uuid: &str,
        title: Option<&str>,
        description: Option<&str>,
        etag: Option<&str>,
    ) -> Result<()> {
        let etag = self.resolve_etag(uuid, etag).await?;
        let mut form = Vec::new();
        if let Some(t) = title {
            form.push(("title", t.to_string()));
        }
        if let Some(d) = description {
            form.push(("description", d.to_string()));
        }
        self.request_empty(
            Method::POST,
            &format!("v1/playlists/{uuid}"),
            &[],
            Some(&form),
            &[("If-None-Match", etag)],
        )
        .await?;
        Ok(())
    }

    /// `POST playlists/{uuid}/items` form `trackIds=1,2,3&toIndex=<n>&onDupes=&onArtifactNotFound=`,
    /// `If-None-Match: <etag>` -> `{addedItemIds: [...]}` (`tidal-api`
    /// catalog-and-library.md §7). Library write.
    pub async fn playlist_add_tracks(
        &self,
        uuid: &str,
        track_ids: &[u64],
        to_index: Option<u32>,
        on_dupes: OnDupes,
        on_artifact_not_found: OnArtifactNotFound,
        etag: Option<&str>,
    ) -> Result<Vec<u64>> {
        let etag = self.resolve_etag(uuid, etag).await?;
        let ids = track_ids
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let mut form = vec![
            ("trackIds", ids),
            ("onDupes", on_dupes.as_str().to_string()),
            (
                "onArtifactNotFound",
                on_artifact_not_found.as_str().to_string(),
            ),
        ];
        if let Some(i) = to_index {
            form.push(("toIndex", i.to_string()));
        }
        #[derive(Deserialize, Default)]
        #[serde(rename_all = "camelCase")]
        struct Added {
            #[serde(default)]
            added_item_ids: Vec<u64>,
        }
        let (added, _headers) = self
            .request_json::<Added>(
                Method::POST,
                &format!("v1/playlists/{uuid}/items"),
                &[],
                Some(&form),
                &[("If-None-Match", etag)],
            )
            .await?;
        Ok(added.added_item_ids)
    }

    /// `POST playlists/{uuid}/items` form `fromPlaylistUuid=<uuid>&onDupes=&onArtifactNotFound=`,
    /// `If-None-Match: <etag>` — merge another playlist's tracks in
    /// (`tidal-api` catalog-and-library.md §7). Library write.
    pub async fn playlist_merge_from(
        &self,
        uuid: &str,
        from_playlist_uuid: &str,
        on_dupes: OnDupes,
        on_artifact_not_found: OnArtifactNotFound,
        etag: Option<&str>,
    ) -> Result<Vec<u64>> {
        let etag = self.resolve_etag(uuid, etag).await?;
        let form = [
            ("fromPlaylistUuid", from_playlist_uuid.to_string()),
            ("onDupes", on_dupes.as_str().to_string()),
            (
                "onArtifactNotFound",
                on_artifact_not_found.as_str().to_string(),
            ),
        ];
        #[derive(Deserialize, Default)]
        #[serde(rename_all = "camelCase")]
        struct Added {
            #[serde(default)]
            added_item_ids: Vec<u64>,
        }
        let (added, _headers) = self
            .request_json::<Added>(
                Method::POST,
                &format!("v1/playlists/{uuid}/items"),
                &[],
                Some(&form),
                &[("If-None-Match", etag)],
            )
            .await?;
        Ok(added.added_item_ids)
    }

    /// `POST playlists/{uuid}/items/{i,j,k}` form `toIndex=<n>`,
    /// `If-None-Match: <etag>` — **reorder addresses items by index, not by
    /// id** (`tidal-api` catalog-and-library.md §7: "reference clients
    /// implement move/remove by id by fetching all tracks and computing the
    /// index — racy and expensive on a large playlist"). Library write.
    pub async fn playlist_reorder(
        &self,
        uuid: &str,
        item_indices: &[u32],
        to_index: u32,
        etag: Option<&str>,
    ) -> Result<()> {
        let etag = self.resolve_etag(uuid, etag).await?;
        let idx = item_indices
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let form = [("toIndex", to_index.to_string())];
        self.request_empty(
            Method::POST,
            &format!("v1/playlists/{uuid}/items/{idx}"),
            &[],
            Some(&form),
            &[("If-None-Match", etag)],
        )
        .await?;
        Ok(())
    }

    /// `DELETE playlists/{uuid}/items/{i,j,k}`, `If-None-Match: <etag>` —
    /// remove by index, same caveat as reorder. Library write.
    pub async fn playlist_remove_items(
        &self,
        uuid: &str,
        item_indices: &[u32],
        etag: Option<&str>,
    ) -> Result<()> {
        let etag = self.resolve_etag(uuid, etag).await?;
        let idx = item_indices
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        self.request_empty(
            Method::DELETE,
            &format!("v1/playlists/{uuid}/items/{idx}"),
            &[],
            None,
            &[("If-None-Match", etag)],
        )
        .await?;
        Ok(())
    }

    /// `DELETE playlists/{uuid}`. The reference lists this without an
    /// explicit `If-None-Match` note, but Sone's own shipped code applies
    /// the same fetch-etag-then-mutate pattern here too
    /// (sone-deep-dive.md §4b, `:2072-2084`) — sent here for consistency;
    /// pass `etag` explicitly to skip the extra fetch. Library write.
    pub async fn delete_playlist(&self, uuid: &str, etag: Option<&str>) -> Result<()> {
        let etag = self.resolve_etag(uuid, etag).await?;
        self.request_empty(
            Method::DELETE,
            &format!("v1/playlists/{uuid}"),
            &[],
            None,
            &[("If-None-Match", etag)],
        )
        .await?;
        Ok(())
    }

    /// `PUT v2/playlists/{uuid}/set-public` / `.../set-private` — the
    /// legacy binary visibility toggle; it cannot express the v2
    /// `UNLISTED` state (`tidal-client-features`
    /// library-playlists-collections.md §1). Library write.
    pub async fn set_playlist_public(&self, uuid: &str, public: bool) -> Result<()> {
        let path = if public {
            format!("v2/playlists/{uuid}/set-public")
        } else {
            format!("v2/playlists/{uuid}/set-private")
        };
        self.request_empty(Method::PUT, &path, &[], None, &[])
            .await?;
        Ok(())
    }

    // -- Folders ------------------------------------------------------------

    /// `PUT my-collection/playlists/folders/create-folder?name=&folderId=root`.
    /// Response shape undocumented; returns raw JSON. Library write.
    pub async fn create_folder(
        &self,
        name: &str,
        folder_id: Option<&str>,
    ) -> Result<serde_json::Value> {
        let cc = self.country_code().await?;
        let query = [
            ("name", name.to_string()),
            ("folderId", folder_id.unwrap_or("root").to_string()),
            ("countryCode", cc),
        ];
        let (v, _) = self
            .request_json::<serde_json::Value>(
                Method::PUT,
                "v2/my-collection/playlists/folders/create-folder",
                &query,
                None,
                &[],
            )
            .await?;
        Ok(v)
    }

    /// `PUT my-collection/playlists/folders/rename?trn=trn:folder:<id>&name=`.
    /// Library write.
    pub async fn rename_folder(&self, trn: &str, name: &str) -> Result<()> {
        self.request_empty(
            Method::PUT,
            "v2/my-collection/playlists/folders/rename",
            &[("trn", trn.to_string()), ("name", name.to_string())],
            None,
            &[],
        )
        .await?;
        Ok(())
    }

    /// `PUT my-collection/playlists/folders/remove?trns=trn:folder:<id>[,trn:playlist:<uuid>]`.
    /// Library write.
    pub async fn remove_from_folders(&self, trns: &[&str]) -> Result<()> {
        self.request_empty(
            Method::PUT,
            "v2/my-collection/playlists/folders/remove",
            &[("trns", trns.join(","))],
            None,
            &[],
        )
        .await?;
        Ok(())
    }

    /// `PUT my-collection/playlists/folders/move?folderId=<id>&trns=trn:playlist:<uuid>,…`.
    /// Library write.
    pub async fn move_to_folder(&self, folder_id: &str, trns: &[&str]) -> Result<()> {
        self.request_empty(
            Method::PUT,
            "v2/my-collection/playlists/folders/move",
            &[
                ("folderId", folder_id.to_string()),
                ("trns", trns.join(",")),
            ],
            None,
            &[],
        )
        .await?;
        Ok(())
    }

    /// `PUT my-collection/playlists/folders/add-favorites?folderId=root&uuids=<uuid,…>`.
    /// Library write.
    pub async fn add_favorites_to_folder(
        &self,
        uuids: &[&str],
        folder_id: Option<&str>,
    ) -> Result<()> {
        self.request_empty(
            Method::PUT,
            "v2/my-collection/playlists/folders/add-favorites",
            &[
                ("folderId", folder_id.unwrap_or("root").to_string()),
                ("uuids", uuids.join(",")),
            ],
            None,
            &[],
        )
        .await?;
        Ok(())
    }
}
