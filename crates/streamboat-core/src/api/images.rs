//! Pure helpers: no network call, no `ApiClient`. Two things live here:
//!
//! 1. Turning a cover/artist/playlist image id into a `resources.tidal.com`
//!    URL at one of the documented allowed sizes (`tidal-api`
//!    catalog-and-library.md §10). Images are unauthenticated — "route
//!    image fetches outside your authenticated API client" — which is
//!    exactly why these are free functions, not `ApiClient` methods.
//! 2. Parsing (never registering a handler for — `tidal-api` auth.md §13)
//!    `tidal://` content links and the `listen.tidal.com`/`tidal.com`
//!    web-link forms users paste. streamboat's own OAuth redirect and
//!    content-link scheme is `streamboat://`, never `tidal://` — this
//!    module only *reads* the links other apps produce.

use crate::error::{Error, Result};

/// Valid album-cover sizes (square) — a wrong size 403s (`tidal-api`
/// catalog-and-library.md §10).
pub const ALBUM_COVER_SIZES: [u32; 5] = [80, 160, 320, 640, 1280];
/// Valid artist-picture sizes — note this tops out at 750, *not* 1280;
/// reusing the album-cover size list here 403s.
pub const ARTIST_PICTURE_SIZES: [u32; 4] = [160, 320, 480, 750];
/// Valid playlist square-cover sizes.
pub const PLAYLIST_SQUARE_SIZES: [u32; 6] = [160, 320, 480, 640, 750, 1080];
/// Valid playlist wide-cover and video-thumbnail (w, h) pairs — both entity
/// types share this exact size list.
pub const WIDE_SIZES: [(u32, u32); 4] = [(160, 107), (480, 320), (750, 500), (1080, 720)];
/// Valid user-picture sizes.
pub const USER_PICTURE_SIZES: [u32; 3] = [100, 210, 600];

/// Placeholder album-cover id, for an entity with no artwork.
pub const PLACEHOLDER_ALBUM_COVER: &str = "0dfd3368-3aa1-49a3-935f-10ffb39803c0";
/// Placeholder artist-picture id.
pub const PLACEHOLDER_ARTIST_PICTURE: &str = "1e01cdb6-f15d-4d8b-8440-a047976c1cac";

/// A TIDAL image id is a UUID; the URL path groups its hex digits as
/// `8/4/4/4/12` with the dashes replaced by slashes (`tidal-api`
/// catalog-and-library.md §10's worked example).
fn id_path(id: &str) -> Result<String> {
    let cleaned: String = id.chars().filter(|c| *c != '-').collect();
    let groups = [8usize, 4, 4, 4, 12];
    if cleaned.len() != groups.iter().sum::<usize>()
        || !cleaned.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err(Error::Config(format!("not a TIDAL image id: {id:?}")));
    }
    let mut out = String::new();
    let mut rest = cleaned.as_str();
    for (i, g) in groups.iter().enumerate() {
        if i > 0 {
            out.push('/');
        }
        out.push_str(&rest[..*g]);
        rest = &rest[*g..];
    }
    Ok(out)
}

fn invalid_size<T: std::fmt::Debug>(got: T, valid: &str) -> Error {
    Error::Config(format!(
        "invalid image size {got:?}; valid sizes are {valid}"
    ))
}

/// `https://resources.tidal.com/images/<id>/<size>x<size>.jpg`.
pub fn album_cover_url(id: &str, size: u32) -> Result<String> {
    if !ALBUM_COVER_SIZES.contains(&size) {
        return Err(invalid_size(size, "80, 160, 320, 640, 1280 (or origin)"));
    }
    Ok(format!(
        "https://resources.tidal.com/images/{}/{size}x{size}.jpg",
        id_path(id)?
    ))
}

/// `https://resources.tidal.com/images/<id>/origin.jpg` — full resolution.
pub fn album_cover_origin_url(id: &str) -> Result<String> {
    Ok(format!(
        "https://resources.tidal.com/images/{}/origin.jpg",
        id_path(id)?
    ))
}

/// `https://resources.tidal.com/videos/<id>/<size>x<size>.mp4` — animated
/// cover, same size list as the still cover.
pub fn album_animated_cover_url(id: &str, size: u32) -> Result<String> {
    if !ALBUM_COVER_SIZES.contains(&size) {
        return Err(invalid_size(size, "80, 160, 320, 640, 1280 (or origin)"));
    }
    Ok(format!(
        "https://resources.tidal.com/videos/{}/{size}x{size}.mp4",
        id_path(id)?
    ))
}

/// `https://resources.tidal.com/images/<id>/<size>x<size>.jpg` — artist
/// picture. **Do not reuse [`ALBUM_COVER_SIZES`] here**: 640/1280 403 for
/// this entity type.
pub fn artist_picture_url(id: &str, size: u32) -> Result<String> {
    if !ARTIST_PICTURE_SIZES.contains(&size) {
        return Err(invalid_size(size, "160, 320, 480, 750"));
    }
    Ok(format!(
        "https://resources.tidal.com/images/{}/{size}x{size}.jpg",
        id_path(id)?
    ))
}

/// Playlist square cover.
pub fn playlist_square_url(id: &str, size: u32) -> Result<String> {
    if !PLAYLIST_SQUARE_SIZES.contains(&size) {
        return Err(invalid_size(size, "160, 320, 480, 640, 750, 1080"));
    }
    Ok(format!(
        "https://resources.tidal.com/images/{}/{size}x{size}.jpg",
        id_path(id)?
    ))
}

/// Playlist wide cover: `(w, h)` must be one of [`WIDE_SIZES`].
pub fn playlist_wide_url(id: &str, w: u32, h: u32) -> Result<String> {
    if !WIDE_SIZES.contains(&(w, h)) {
        return Err(invalid_size((w, h), "160x107, 480x320, 750x500, 1080x720"));
    }
    Ok(format!(
        "https://resources.tidal.com/images/{}/{w}x{h}.jpg",
        id_path(id)?
    ))
}

/// Video thumbnail: same size list as [`playlist_wide_url`].
pub fn video_thumbnail_url(id: &str, w: u32, h: u32) -> Result<String> {
    if !WIDE_SIZES.contains(&(w, h)) {
        return Err(invalid_size((w, h), "160x107, 480x320, 750x500, 1080x720"));
    }
    Ok(format!(
        "https://resources.tidal.com/images/{}/{w}x{h}.jpg",
        id_path(id)?
    ))
}

/// User picture.
pub fn user_picture_url(id: &str, size: u32) -> Result<String> {
    if !USER_PICTURE_SIZES.contains(&size) {
        return Err(invalid_size(size, "100, 210, 600"));
    }
    Ok(format!(
        "https://resources.tidal.com/images/{}/{size}x{size}.jpg",
        id_path(id)?
    ))
}

/// `MULTIPLE_TOP_PROMOTIONS` promo banner — **550x400 only**, the square
/// sizes 403.
pub fn promo_banner_url(id: &str) -> Result<String> {
    Ok(format!(
        "https://resources.tidal.com/images/{}/550x400.jpg",
        id_path(id)?
    ))
}

// ---------------------------------------------------------------------------
// Deep links / share URLs (`tidal-api` auth.md §13). Parse only.

/// A parsed TIDAL content link. Track/album/artist/video ids are integers;
/// playlist and mix ids are strings (playlist ids are UUIDs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentLink {
    Track(u64),
    Album(u64),
    Artist(u64),
    Video(u64),
    Playlist(String),
    Mix(String),
    Folder(String),
    /// `listen.tidal.com/album/{albumId}/track/{trackId}` — a track shown in
    /// the context of its album.
    AlbumTrack {
        album_id: u64,
        track_id: u64,
    },
}

/// Parse a TIDAL content link — the `tidal://<type>/<id>` scheme Sone and
/// High Tide both register (`type` in `track`, `album`, `artist`,
/// `playlist`, `mix`), or the web forms users paste:
/// `https://listen.tidal.com/{track,album,artist,playlist,video}/{id}`,
/// `https://listen.tidal.com/album/{albumId}/track/{trackId}`,
/// `https://tidal.com/browse/{type}/{id}`,
/// `https://listen.tidal.com/folder/{folderId}`. Returns `None` for
/// anything else, including a `streamboat://` link (this app's own scheme,
/// handled elsewhere) or an OAuth redirect URL.
///
/// **This function only reads a link — it never registers `tidal://` (or
/// any web form) as an OS handler.** Claiming the OS handler for either is
/// a separate, explicit opt-in the desktop shell owns; `auth.tidal.com`'s
/// own scheme collision (Strawberry's OAuth redirect and the official
/// desktop app both also claim `tidal://`) is exactly why streamboat's own
/// redirect and content-link scheme is `streamboat://` instead.
pub fn parse_content_link(input: &str) -> Option<ContentLink> {
    let s = input.trim();
    if let Some(rest) = s.strip_prefix("tidal://") {
        return parse_type_id_path(rest.trim_start_matches('/'));
    }
    let url = url::Url::parse(s).ok()?;
    match url.host_str()? {
        "listen.tidal.com" => parse_type_id_path(url.path().trim_start_matches('/')),
        "tidal.com" => {
            parse_type_id_path(url.path().trim_start_matches('/').strip_prefix("browse/")?)
        }
        _ => None,
    }
}

fn parse_type_id_path(path: &str) -> Option<ContentLink> {
    let segs: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    match segs.as_slice() {
        ["album", album_id, "track", track_id] => Some(ContentLink::AlbumTrack {
            album_id: album_id.parse().ok()?,
            track_id: track_id.parse().ok()?,
        }),
        [kind, id] => match *kind {
            "track" => Some(ContentLink::Track(id.parse().ok()?)),
            "album" => Some(ContentLink::Album(id.parse().ok()?)),
            "artist" => Some(ContentLink::Artist(id.parse().ok()?)),
            "video" => Some(ContentLink::Video(id.parse().ok()?)),
            "playlist" => Some(ContentLink::Playlist(id.to_string())),
            "mix" => Some(ContentLink::Mix(id.to_string())),
            "folder" => Some(ContentLink::Folder(id.to_string())),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn album_cover_builds_grouped_path() {
        let url = album_cover_url("1e01cdb6-f15d-4d8b-8440-a047976c1cac", 320).unwrap();
        assert_eq!(
            url,
            "https://resources.tidal.com/images/1e01cdb6/f15d/4d8b/8440/a047976c1cac/320x320.jpg"
        );
    }

    #[test]
    fn album_cover_rejects_artist_only_size() {
        // 1280 is valid for an album cover...
        assert!(album_cover_url(PLACEHOLDER_ALBUM_COVER, 1280).is_ok());
        // ...but not for an artist picture, which tops out at 750.
        assert!(artist_picture_url(PLACEHOLDER_ARTIST_PICTURE, 1280).is_err());
    }

    #[test]
    fn playlist_wide_accepts_a_documented_pair() {
        assert!(playlist_wide_url(PLACEHOLDER_ALBUM_COVER, 160, 107).is_ok());
    }

    #[test]
    fn playlist_wide_rejects_unpaired_size() {
        let err = playlist_wide_url(PLACEHOLDER_ALBUM_COVER, 160, 999).unwrap_err();
        assert!(matches!(err, Error::Config(_)));
    }

    #[test]
    fn rejects_non_hex_id() {
        assert!(album_cover_url("not-a-uuid", 320).is_err());
    }

    #[test]
    fn parses_tidal_scheme_track() {
        assert_eq!(
            parse_content_link("tidal://track/12345"),
            Some(ContentLink::Track(12345))
        );
    }

    #[test]
    fn parses_tidal_scheme_playlist_uuid() {
        assert_eq!(
            parse_content_link("tidal://playlist/aaaa-bbbb"),
            Some(ContentLink::Playlist("aaaa-bbbb".into()))
        );
    }

    #[test]
    fn parses_listen_tidal_com_album_track() {
        assert_eq!(
            parse_content_link("https://listen.tidal.com/album/1/track/2"),
            Some(ContentLink::AlbumTrack {
                album_id: 1,
                track_id: 2
            })
        );
    }

    #[test]
    fn parses_listen_tidal_com_folder() {
        assert_eq!(
            parse_content_link("https://listen.tidal.com/folder/f-1"),
            Some(ContentLink::Folder("f-1".into()))
        );
    }

    #[test]
    fn parses_tidal_com_browse() {
        assert_eq!(
            parse_content_link("https://tidal.com/browse/artist/99"),
            Some(ContentLink::Artist(99))
        );
    }

    #[test]
    fn rejects_streamboat_scheme_and_garbage() {
        assert_eq!(parse_content_link("streamboat://login?code=abc"), None);
        assert_eq!(parse_content_link("not a url at all"), None);
    }
}
