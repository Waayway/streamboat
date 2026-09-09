//! Wire models for the parts of the unofficial v1 API the spike touches.
//! Every optional-on-the-wire field is `Option` and every struct tolerates
//! unknown fields: TIDAL's JSON drifts, and a missing `duration` must never
//! drop an item (`tidal-api` transport §8).

use serde::{Deserialize, Serialize};

/// TIDAL's audio quality tiers, in the names the API uses.
///
/// `HiResLegacy` (`HI_RES`) is the retired MQA tier; it is modelled so a
/// response carrying it deserializes, but streamboat never requests it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AudioQuality {
    #[serde(rename = "LOW")]
    Low,
    #[serde(rename = "HIGH")]
    High,
    #[serde(rename = "LOSSLESS")]
    Lossless,
    #[serde(rename = "HI_RES_LOSSLESS")]
    #[default]
    HiResLossless,
    #[serde(rename = "HI_RES")]
    HiResLegacy,
}

impl AudioQuality {
    /// The tiers streamboat requests, highest first, with the retired
    /// `HI_RES` tier excluded (dead content since July 2024).
    pub const LADDER: [AudioQuality; 4] = [
        AudioQuality::HiResLossless,
        AudioQuality::Lossless,
        AudioQuality::High,
        AudioQuality::Low,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            AudioQuality::Low => "LOW",
            AudioQuality::High => "HIGH",
            AudioQuality::Lossless => "LOSSLESS",
            AudioQuality::HiResLossless => "HI_RES_LOSSLESS",
            AudioQuality::HiResLegacy => "HI_RES",
        }
    }

    /// Higher is better. `HI_RES` sits between LOSSLESS and HI_RES_LOSSLESS.
    pub fn rank(self) -> u8 {
        match self {
            AudioQuality::Low => 0,
            AudioQuality::High => 1,
            AudioQuality::Lossless => 2,
            AudioQuality::HiResLegacy => 3,
            AudioQuality::HiResLossless => 4,
        }
    }

    pub fn is_hi_res(self) -> bool {
        matches!(
            self,
            AudioQuality::HiResLossless | AudioQuality::HiResLegacy
        )
    }

    pub fn is_lossless(self) -> bool {
        self.rank() >= 2
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().replace('-', "_").as_str() {
            "LOW" => Some(Self::Low),
            "HIGH" => Some(Self::High),
            "LOSSLESS" => Some(Self::Lossless),
            "HI_RES_LOSSLESS" | "HIRES_LOSSLESS" | "HIRES" | "HI_RES_FLAC" | "MAX" => {
                Some(Self::HiResLossless)
            }
            "HI_RES" => Some(Self::HiResLegacy),
            _ => None,
        }
    }
}

impl std::fmt::Display for AudioQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for AudioQuality {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        AudioQuality::parse(s).ok_or_else(|| {
            format!("unknown quality {s:?}; expected LOW, HIGH, LOSSLESS or HI_RES_LOSSLESS")
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AudioMode {
    #[serde(rename = "STEREO")]
    #[default]
    Stereo,
    #[serde(rename = "DOLBY_ATMOS")]
    DolbyAtmos,
    #[serde(rename = "SONY_360RA")]
    Sony360Ra,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    pub id: Option<u64>,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AlbumRef {
    pub id: Option<u64>,
    #[serde(default)]
    pub title: String,
    /// Cover image id (`uuid` with dashes); see `tidal-api` catalog §Images.
    pub cover: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaMetadata {
    #[serde(default)]
    pub tags: Vec<String>,
}

/// A track as returned by `GET /v1/tracks/{id}` and inside search results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: u64,
    #[serde(default)]
    pub title: String,
    /// Seconds. The authoritative duration for prefetch and reporting maths.
    pub duration: Option<u32>,
    pub track_number: Option<u32>,
    pub volume_number: Option<u32>,
    pub replay_gain: Option<f64>,
    pub peak: Option<f64>,
    pub allow_streaming: Option<bool>,
    pub stream_ready: Option<bool>,
    pub explicit: Option<bool>,
    pub isrc: Option<String>,
    pub audio_quality: Option<AudioQuality>,
    #[serde(default)]
    pub audio_modes: Vec<AudioMode>,
    pub media_metadata: Option<MediaMetadata>,
    pub artist: Option<Artist>,
    #[serde(default)]
    pub artists: Vec<Artist>,
    pub album: Option<AlbumRef>,
    pub url: Option<String>,
}

impl Track {
    pub fn artist_names(&self) -> String {
        if !self.artists.is_empty() {
            self.artists
                .iter()
                .map(|a| a.name.as_str())
                .filter(|n| !n.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            self.artist
                .as_ref()
                .map(|a| a.name.clone())
                .unwrap_or_default()
        }
    }

    /// Whether the catalogue says this track has a HI_RES_LOSSLESS master
    /// (`mediaMetadata.tags` contains `HIRES_LOSSLESS`). Cheap pre-flight
    /// used to avoid a wasted hi-res request.
    pub fn has_hires_master(&self) -> bool {
        self.media_metadata
            .as_ref()
            .map(|m| m.tags.iter().any(|t| t == "HIRES_LOSSLESS"))
            .unwrap_or(false)
    }
}

/// Compact track description carried in events and queue snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TrackSummary {
    pub id: u64,
    pub title: String,
    pub artists: String,
    pub album: String,
    pub duration_ms: Option<u64>,
    pub cover: Option<String>,
}

impl From<&Track> for TrackSummary {
    fn from(t: &Track) -> Self {
        TrackSummary {
            id: t.id,
            title: t.title.clone(),
            artists: t.artist_names(),
            album: t
                .album
                .as_ref()
                .map(|a| a.title.clone())
                .unwrap_or_default(),
            duration_ms: t.duration.map(|d| u64::from(d) * 1000),
            cover: t.album.as_ref().and_then(|a| a.cover.clone()),
        }
    }
}

/// `GET /v1/sessions`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub session_id: Option<String>,
    pub user_id: Option<u64>,
    #[serde(default)]
    pub country_code: String,
    pub channel_id: Option<u64>,
    pub partner_id: Option<u64>,
    pub client: Option<SessionClient>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionClient {
    pub id: Option<u64>,
    #[serde(default)]
    pub name: String,
    pub authorized_for_offline: Option<bool>,
}

/// The `user` object inside a token response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TokenUser {
    pub user_id: Option<u64>,
    pub email: Option<String>,
    pub country_code: Option<String>,
    pub full_name: Option<String>,
    pub username: Option<String>,
}

/// A page of search results (`GET /v1/search/tracks`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    #[serde(default = "Vec::new")]
    pub items: Vec<T>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub total_number_of_items: Option<u64>,
}

// ---------------------------------------------------------------------------
// Catalogue entity pages (`tidal-api` catalog-and-library.md §2; D-015).

/// A fuller album entity (`GET /v1/albums/{id}`), distinct from the compact
/// [`AlbumRef`] embedded in a [`Track`]. Field list from the desktop
/// client's own model (`tidal-client-features`
/// library-playlists-collections.md §4); unknown fields are ignored as
/// everywhere else in this module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: u64,
    #[serde(default)]
    pub title: String,
    pub duration: Option<u32>,
    pub number_of_tracks: Option<u32>,
    pub number_of_videos: Option<u32>,
    pub number_of_volumes: Option<u32>,
    pub release_date: Option<String>,
    pub copyright: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub version: Option<String>,
    pub url: Option<String>,
    pub explicit: Option<bool>,
    pub upc: Option<String>,
    pub popularity: Option<u32>,
    pub audio_quality: Option<AudioQuality>,
    #[serde(default)]
    pub audio_modes: Vec<AudioMode>,
    pub media_metadata: Option<MediaMetadata>,
    pub cover: Option<String>,
    pub video_cover: Option<String>,
    pub artist: Option<Artist>,
    #[serde(default)]
    pub artists: Vec<Artist>,
}

/// `albums/{id}/review` and `artists/{id}/bio` share this shape: `{text}`,
/// plus a `source` attribution the desktop client shows (e.g. "TiVo") that
/// python-tidal's own `get_bio()` discards by only reading `text`
/// (`tidal-client-features` library-playlists-collections.md §3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TextWithSource {
    pub text: Option<String>,
    pub source: Option<String>,
}

/// The full artist entity (`GET /v1/artists/{id}`). **The field list beyond
/// `id`/`name`/`picture` is not enumerated by any reference this crate
/// cites for this specific unofficial endpoint** — the desktop client's
/// Redux `Artist.ts` model describes the app's internal shape, not a
/// confirmed 1:1 mapping to this JSON response. Extend this struct once a
/// response is captured against a live account; unknown fields already
/// deserialize safely (they are ignored), so nothing breaks in the
/// meantime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ArtistProfile {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    /// Picture image id (`uuid` with dashes); see `tidal-api` catalog §10.
    pub picture: Option<String>,
    pub popularity: Option<u32>,
    pub url: Option<String>,
    #[serde(default)]
    pub artist_types: Vec<String>,
}

/// `artists/{id}/albums?filter=` — omit `filter` for the main discography.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtistAlbumFilter {
    EpsAndSingles,
    Compilations,
}

impl ArtistAlbumFilter {
    pub fn as_str(self) -> &'static str {
        match self {
            ArtistAlbumFilter::EpsAndSingles => "EPSANDSINGLES",
            ArtistAlbumFilter::Compilations => "COMPILATIONS",
        }
    }
}

/// `tracks/{id}/credits` item shape: `{type, contributors: [{name, id}]}`
/// (`tidal-api` catalog-and-library.md §2). Album credits have no dedicated
/// unofficial-API endpoint (see `ApiClient::album_review` doc comment) so
/// this type is track-only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Credit {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub contributors: Vec<Contributor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Contributor {
    pub name: Option<String>,
    pub id: Option<u64>,
}

/// A playlist entity (`GET /v1/playlists/{uuid}` and inside every playlist
/// list endpoint). `public_playlist` is the legacy v1 boolean, which cannot
/// represent the v2 `UNLISTED` state (`tidal-client-features`
/// library-playlists-collections.md §1) — treat `Some(false)` as "private or
/// unlisted," not as a confirmed private playlist, when only this field is
/// available.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub uuid: Option<String>,
    #[serde(default)]
    pub title: String,
    pub description: Option<String>,
    pub duration: Option<u32>,
    pub number_of_tracks: Option<u32>,
    pub number_of_videos: Option<u32>,
    /// Creation date; also where `playlistsAndFavoritePlaylists`'s
    /// `{playlist, created}` wrapper's `created` is folded back in
    /// (`tidal-api` catalog-and-library.md §7 — python-tidal reproduces this
    /// same unwrap so the date isn't silently lost).
    pub created: Option<String>,
    pub last_updated: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub public_playlist: Option<bool>,
    pub url: Option<String>,
    pub image: Option<String>,
    pub square_image: Option<String>,
    pub creator: Option<PlaylistCreator>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PlaylistCreator {
    pub id: Option<u64>,
}

/// One entry of `GET playlists/{uuid}/items` (and, best-effort, `GET
/// mixes/{id}/items` — see `ApiClient::mix_items`'s doc comment for why that
/// second use is a guess): `{item, type}`, `type` being `"track"` or
/// `"video"` (`tidal-api` catalog-and-library.md §7). `item` is left as raw
/// JSON because its shape depends on `kind`; use [`PlaylistItem::as_track`]
/// once you've checked `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistItem {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub item: Option<serde_json::Value>,
}

impl PlaylistItem {
    /// `Some(track)` when `kind == "track"` and `item` parses as one;
    /// `None` for a video item or a shape this crate doesn't model.
    pub fn as_track(&self) -> Option<Track> {
        if !self.kind.as_deref()?.eq_ignore_ascii_case("track") {
            return None;
        }
        serde_json::from_value(self.item.clone()?).ok()
    }
}

/// Video metadata only (`GET /v1/videos/{id}`) — streamboat does not play
/// video (later scope, `tidal-client-features` browse-pages-screens.md §7),
/// so no manifest/stream method exists for this type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Video {
    pub id: u64,
    #[serde(default)]
    pub title: String,
    pub duration: Option<u32>,
    pub quality: Option<String>,
    pub image_id: Option<String>,
    pub release_date: Option<String>,
    pub explicit: Option<bool>,
    pub artist: Option<Artist>,
    #[serde(default)]
    pub artists: Vec<Artist>,
}

// ---------------------------------------------------------------------------
// Search (`tidal-api` catalog-and-library.md §1)

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ItemsWrap<T> {
    #[serde(default)]
    pub items: Vec<T>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TopHit {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub value: Option<serde_json::Value>,
}

/// `GET v1/search` (v1; see `ApiClient::search`'s doc comment for why v1
/// rather than v2). Response shape: each type is `{items: [...]}` plus one
/// `topHit` (`tidal-api` catalog-and-library.md §1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    #[serde(default)]
    pub artists: ItemsWrap<ArtistProfile>,
    #[serde(default)]
    pub albums: ItemsWrap<Album>,
    #[serde(default)]
    pub tracks: ItemsWrap<Track>,
    #[serde(default)]
    pub videos: ItemsWrap<Video>,
    #[serde(default)]
    pub playlists: ItemsWrap<Playlist>,
    pub top_hit: Option<TopHit>,
}

// ---------------------------------------------------------------------------
// Home / Explore (v2 `home/feed`, D-015) and the v1 `pages/*` fallback.

/// `GET v2/home/feed/{slug}` (`tidal-api` transport §1, catalog-and-library.md
/// §5; `tidal-client-features` browse-pages-screens.md §1). Response-only:
/// no `Serialize` impl, since [`FeedSection`] (via its manual `Deserialize`)
/// doesn't have one either.
#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HomeFeed {
    pub header: Option<HomeFeedHeader>,
    #[serde(default)]
    pub items: Vec<FeedSection>,
    /// Top-level cursor; pass back as `?cursor=` for the next page
    /// (`tidal-api` transport §4).
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HomeFeedHeader {
    pub vibes: Option<HomeVibes>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HomeVibes {
    #[serde(default)]
    pub items: Vec<HomeTab>,
}

/// One entry of `header.vibes.items[]` — Home's tab bar. Observed `type`
/// values `STATIC`, `EDITORIAL`, `UPLOADS`; the real feed slug is the
/// lowercased `type` (`tidal-client-features` browse-pages-screens.md §1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HomeTab {
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

impl HomeTab {
    /// The `home/feed/<slug>` slug for this tab, or `None` when TIDAL
    /// omitted `type`.
    pub fn slug(&self) -> Option<String> {
        self.kind.as_ref().map(|s| s.to_ascii_lowercase())
    }
}

/// One item inside a [`FeedSection`] — `PLAYLIST`, `VIDEO`, `TRACK`,
/// `ARTIST`, `ALBUM`, `MIX` and others TIDAL adds over time
/// (`tidal-client-features` browse-pages-screens.md §2). Only the fields
/// every item type is expected to carry are named; everything else lands in
/// `extra` so a new item shape never loses data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FeedItem {
    #[serde(rename = "type", default)]
    pub kind: String,
    pub id: Option<serde_json::Value>,
    pub title: Option<String>,
    #[serde(flatten)]
    pub extra: std::collections::BTreeMap<String, serde_json::Value>,
}

/// One section ("module") of a v2 Home/Explore feed:
/// `SHORTCUT_LIST`/`HORIZONTAL_LIST`/`HORIZONTAL_LIST_WITH_CONTEXT`/
/// `TRACK_LIST` and more (`tidal-client-features` browse-pages-screens.md
/// §2). `raw` always holds the section's complete original JSON — read from
/// it for a `kind` this struct doesn't special-case, or for a field not
/// named below; this is the "graceful unknown-section fallback" D-015
/// requires. A section whose JSON isn't even an object (or that fails to
/// deserialize into the shape below at all) still keeps its `raw` value and
/// gets empty/default typed fields, rather than failing the whole page.
#[derive(Debug, Clone, PartialEq)]
pub struct FeedSection {
    pub kind: String,
    pub title: Option<String>,
    pub module_id: Option<String>,
    pub subtitle: Option<String>,
    pub has_more: bool,
    /// Relative API path used to expand this section ("View All"); see
    /// `ApiClient::expand_section`'s doc comment for how confirmed that
    /// contract is.
    pub api_path: Option<String>,
    pub items: Vec<FeedItem>,
    pub raw: serde_json::Value,
}

impl<'de> Deserialize<'de> for FeedSection {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = serde_json::Value::deserialize(deserializer)?;
        #[derive(Deserialize, Default)]
        #[serde(rename_all = "camelCase")]
        struct Shape {
            #[serde(rename = "type", default)]
            kind: String,
            #[serde(default)]
            title: Option<String>,
            #[serde(default)]
            module_id: Option<String>,
            #[serde(default)]
            subtitle: Option<String>,
            #[serde(default)]
            has_more: bool,
            #[serde(default)]
            api_path: Option<String>,
            #[serde(default)]
            items: Vec<FeedItem>,
        }
        let shape: Shape = serde_json::from_value(raw.clone()).unwrap_or_default();
        Ok(FeedSection {
            kind: shape.kind,
            title: shape.title,
            module_id: shape.module_id,
            subtitle: shape.subtitle,
            has_more: shape.has_more,
            api_path: shape.api_path,
            items: shape.items,
            raw,
        })
    }
}

/// The still-live v1 Pages shape (`GET pages/{slug}`;
/// `tidal-client-features` browse-pages-screens.md §1, `tidal-api`
/// catalog-and-library.md §5). Kept as an optional fallback/alternate path
/// (D-015 leaves "whether to also parse the v1 shape as a fallback" as a
/// later implementation call) and as the only supported shape for slugs
/// with no v2 equivalent (`pages/explore`, mood/genre pages, `pages/mix`).
#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PageV1 {
    pub title: Option<String>,
    #[serde(default)]
    pub rows: Vec<PageRowV1>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PageRowV1 {
    #[serde(default)]
    pub modules: Vec<PageModuleV1>,
}

/// One v1 module. Like [`FeedSection`], `raw` always carries the complete
/// original JSON so an unrecognised `type` (new ones ship without warning,
/// per the reference) never fails the page — "log unknown shapes, never
/// hard-fail a page because one module changed"
/// (`tidal-client-features` browse-pages-screens.md §2).
#[derive(Debug, Clone, PartialEq)]
pub struct PageModuleV1 {
    pub kind: String,
    pub title: Option<String>,
    /// `showMore.apiPath`, when present.
    pub show_more_api_path: Option<String>,
    /// `pagedList.items`, tolerant of any item shape.
    pub items: Vec<serde_json::Value>,
    pub raw: serde_json::Value,
}

impl<'de> Deserialize<'de> for PageModuleV1 {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = serde_json::Value::deserialize(deserializer)?;
        #[derive(Deserialize, Default)]
        #[serde(rename_all = "camelCase")]
        struct PagedList {
            #[serde(default)]
            items: Vec<serde_json::Value>,
        }
        #[derive(Deserialize, Default)]
        #[serde(rename_all = "camelCase")]
        struct ShowMore {
            api_path: Option<String>,
        }
        #[derive(Deserialize, Default)]
        #[serde(rename_all = "camelCase")]
        struct Shape {
            #[serde(rename = "type", default)]
            kind: String,
            #[serde(default)]
            title: Option<String>,
            #[serde(default)]
            show_more: Option<ShowMore>,
            #[serde(default)]
            paged_list: Option<PagedList>,
        }
        let shape: Shape = serde_json::from_value(raw.clone()).unwrap_or_default();
        Ok(PageModuleV1 {
            kind: shape.kind,
            title: shape.title,
            show_more_api_path: shape.show_more.and_then(|s| s.api_path),
            items: shape.paged_list.map(|p| p.items).unwrap_or_default(),
            raw,
        })
    }
}

// ---------------------------------------------------------------------------
// My Collection / favourites (`tidal-api` catalog-and-library.md §6)

/// Some favourites list endpoints wrap each entry as `{created, item}` —
/// **confirmed only for `users/{id}/playlistsAndFavoritePlaylists`**
/// (folded directly into [`Playlist::created`] there); whether
/// `favorites/{tracks,albums,artists,videos,playlists}` do the same is not
/// confirmed by any reference this crate cites. This type accepts either
/// shape — the wrapped form's `item`, or the bare entity — so a response
/// either way still deserializes; treat `created` as best-effort until a
/// captured fixture confirms which shape each endpoint actually uses.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Favorite<T> {
    pub created: Option<String>,
    pub item: T,
}

impl<'de, T: serde::de::DeserializeOwned> Deserialize<'de> for Favorite<T> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = serde_json::Value::deserialize(deserializer)?;
        if let Some(item_val) = v.as_object().and_then(|o| o.get("item")) {
            let created = v
                .as_object()
                .and_then(|o| o.get("created"))
                .and_then(|c| c.as_str())
                .map(str::to_string);
            let item =
                serde_json::from_value(item_val.clone()).map_err(serde::de::Error::custom)?;
            return Ok(Favorite { created, item });
        }
        let item = serde_json::from_value(v).map_err(serde::de::Error::custom)?;
        Ok(Favorite {
            created: None,
            item,
        })
    }
}

/// `GET users/{id}/favorites/ids` — every id comes back as a **string**,
/// even for the integer-id entity types (`tidal-api` catalog-and-library.md
/// §6). The cheap way to render "is favourited" state across a whole view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct FavoriteIds {
    #[serde(default)]
    pub track: Vec<String>,
    #[serde(default)]
    pub album: Vec<String>,
    #[serde(default)]
    pub artist: Vec<String>,
    #[serde(default)]
    pub playlist: Vec<String>,
    #[serde(default)]
    pub video: Vec<String>,
    #[serde(default)]
    pub mix: Vec<String>,
}

/// A favourited mix summary (`GET v2/favorites/mixes`). Field list is a
/// best-effort superset of what mix listings elsewhere in the API carry
/// (`mixType`, images as full URLs per `tidal-api` catalog-and-library.md
/// §10's "Mix" row) — not independently confirmed for this specific v2
/// endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MixSummary {
    pub id: Option<String>,
    pub title: Option<String>,
    pub sub_title: Option<String>,
    pub mix_type: Option<String>,
}

// ---------------------------------------------------------------------------
// Lyrics (`tidal-api` catalog-and-library.md §9)

/// `GET tracks/{id}/lyrics?countryCode=`. `lyrics` is plain text; `subtitles`
/// is LRC-formatted synced lyrics — see [`crate::api::lyrics::parse_synced_lyrics`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LyricsResponse {
    pub track_id: Option<u64>,
    pub lyrics_provider: Option<String>,
    pub provider_commontrack_id: Option<String>,
    pub provider_lyrics_id: Option<String>,
    pub lyrics: Option<String>,
    pub subtitles: Option<String>,
    pub is_right_to_left: Option<bool>,
}

/// One timed line parsed from [`LyricsResponse::subtitles`].
#[derive(Debug, Clone, PartialEq)]
pub struct LyricLine {
    pub time_ms: u64,
    pub text: String,
}
