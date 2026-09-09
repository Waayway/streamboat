//! Catalogue and library surface tests: pure-parsing unit tests against
//! synthetic (structure-preserving, invented-content, D-047) JSON fixtures,
//! plus transport-level tests against an in-process HTTP server, in the
//! style of `tests/transport.rs`.

use std::sync::Arc;

use serde_json::json;
use streamboat_core::models::{
    Album, Favorite, FeedItem, HomeFeed, HomeTab, Page, PageV1, Playlist, PlaylistItem, Track,
};
use streamboat_core::token_store::{MemoryTokenStore, TokenSet, now_secs};
use streamboat_core::{ApiClient, AuthFlow, ClientCredentials};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn creds() -> ClientCredentials {
    ClientCredentials::new("cid", Some("sec".to_string()))
}

fn tokens() -> TokenSet {
    TokenSet {
        access_token: "at".into(),
        refresh_token: Some("rt".into()),
        token_type: "Bearer".into(),
        expires_at: now_secs() + 3600,
        scope: "r_usr w_usr w_sub".into(),
        client_id: "cid".into(),
        flow: AuthFlow::DeviceCode,
        client_unique_key: None,
        user_id: Some(7),
        country_code: Some("NL".into()),
    }
}

fn client(server: &MockServer) -> ApiClient {
    ApiClient::builder(creds(), Arc::new(MemoryTokenStore::with(tokens())))
        .api_base(&format!("{}/", server.uri()))
        .auth_base(&format!("{}/", server.uri()))
        .build()
        .unwrap()
}

// ---------------------------------------------------------------------------
// Pure parsing — synthetic fixtures, no network.

#[test]
fn home_feed_tolerates_unknown_section_types_and_null_fields() {
    // Structure-preserving, invented content (D-047): a mystery future
    // section type sitting next to a documented one, a null tab name/type,
    // and a null section title.
    let raw = json!({
        "header": {
            "vibes": {
                "items": [
                    {"name": "For You", "type": "STATIC"},
                    {"name": null, "type": null}
                ]
            }
        },
        "items": [
            {"type": "SOME_FUTURE_SECTION_TYPE", "title": "Mystery", "weirdField": 123},
            {"type": "TRACK_LIST", "title": null, "items": []}
        ],
        "cursor": "abc"
    });
    let feed: HomeFeed = serde_json::from_value(raw).unwrap();
    assert_eq!(feed.cursor.as_deref(), Some("abc"));
    let tabs: Vec<HomeTab> = feed.header.unwrap().vibes.unwrap().items;
    assert_eq!(tabs[0].slug(), Some("static".to_string()));
    assert_eq!(tabs[1].slug(), None);
    assert_eq!(feed.items.len(), 2);
    let unknown = &feed.items[0];
    assert_eq!(unknown.kind, "SOME_FUTURE_SECTION_TYPE");
    assert_eq!(unknown.title.as_deref(), Some("Mystery"));
    // The unrecognised field is still there in `raw`, never dropped.
    assert_eq!(unknown.raw["weirdField"], 123);
    let known = &feed.items[1];
    assert_eq!(known.kind, "TRACK_LIST");
    assert_eq!(known.title, None);
    assert!(known.items.is_empty());
}

#[test]
fn home_feed_handles_a_missing_header_and_empty_items() {
    // A response with no `header` at all and no `items` array — the
    // "missing section"/"empty" case.
    let feed: HomeFeed = serde_json::from_value(json!({})).unwrap();
    assert!(feed.header.is_none());
    assert!(feed.items.is_empty());
    assert!(feed.cursor.is_none());
}

#[test]
fn feed_section_that_is_not_even_an_object_still_parses() {
    // A section entry that's just a bare string, not an object at all —
    // the deserializer must not panic or bubble a hard error; it degrades
    // to default typed fields with `raw` preserving whatever was there.
    let raw = json!({"items": ["not an object"]});
    let feed: HomeFeed = serde_json::from_value(raw).unwrap();
    assert_eq!(feed.items.len(), 1);
    assert_eq!(feed.items[0].kind, "");
    assert_eq!(feed.items[0].raw, json!("not an object"));
}

#[test]
fn feed_item_keeps_unmodeled_fields_in_extra() {
    let item: FeedItem = serde_json::from_value(json!({
        "type": "ALBUM",
        "id": 42,
        "title": "An Album",
        "artistName": "Someone",
        "releaseDate": "2024-01-01"
    }))
    .unwrap();
    assert_eq!(item.kind, "ALBUM");
    assert_eq!(item.title.as_deref(), Some("An Album"));
    assert_eq!(item.extra["artistName"], "Someone");
    assert_eq!(item.extra["releaseDate"], "2024-01-01");
}

#[test]
fn page_v1_falls_back_gracefully_on_an_unknown_module_type() {
    let raw = json!({
        "title": "Explore",
        "rows": [
            {"modules": [
                {"type": "SOME_NEW_MODULE_TYPE", "title": "New"},
                {
                    "type": "ALBUM_LIST",
                    "title": "Popular Albums",
                    "showMore": {"apiPath": "pages/rising"},
                    "pagedList": {"items": [{"id": 1}, {"id": 2}]}
                }
            ]}
        ]
    });
    let page: PageV1 = serde_json::from_value(raw).unwrap();
    let modules = &page.rows[0].modules;
    assert_eq!(modules[0].kind, "SOME_NEW_MODULE_TYPE");
    assert!(modules[0].items.is_empty());
    assert_eq!(modules[1].kind, "ALBUM_LIST");
    assert_eq!(
        modules[1].show_more_api_path.as_deref(),
        Some("pages/rising")
    );
    assert_eq!(modules[1].items.len(), 2);
}

#[test]
fn favorite_accepts_the_wrapped_created_item_shape() {
    let fav: Favorite<Track> = serde_json::from_value(json!({
        "created": "2024-02-02T00:00:00.000+0000",
        "item": {"id": 10, "title": "Wrapped"}
    }))
    .unwrap();
    assert_eq!(fav.created.as_deref(), Some("2024-02-02T00:00:00.000+0000"));
    assert_eq!(fav.item.title, "Wrapped");
}

#[test]
fn favorite_accepts_a_bare_item_with_no_wrapper() {
    let fav: Favorite<Track> = serde_json::from_value(json!({"id": 11, "title": "Bare"})).unwrap();
    assert_eq!(fav.created, None);
    assert_eq!(fav.item.title, "Bare");
}

#[test]
fn playlist_item_as_track_only_matches_the_track_kind() {
    let track_item: PlaylistItem = serde_json::from_value(json!({
        "type": "track",
        "item": {"id": 1, "title": "T"}
    }))
    .unwrap();
    assert_eq!(track_item.as_track().unwrap().title, "T");

    let video_item: PlaylistItem = serde_json::from_value(json!({
        "type": "video",
        "item": {"id": 2, "title": "V"}
    }))
    .unwrap();
    assert!(video_item.as_track().is_none());
}

// ---------------------------------------------------------------------------
// Transport-level tests against an in-process HTTP server.

#[tokio::test]
async fn album_and_album_tracks_round_trip() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/albums/500"))
        .and(query_param("countryCode", "NL"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 500, "title": "Test Album", "numberOfTracks": 2, "explicit": false
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/albums/500/tracks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{"id": 1, "title": "A"}, {"id": 2, "title": "B"}],
            "totalNumberOfItems": 2
        })))
        .mount(&server)
        .await;
    let c = client(&server);
    let album: Album = c.album(500).await.unwrap();
    assert_eq!(album.title, "Test Album");
    let tracks: Page<Track> = c.album_tracks(500, 50, 0).await.unwrap();
    assert_eq!(tracks.items.len(), 2);
}

#[tokio::test]
async fn album_similar_maps_a_404_to_an_empty_list() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/albums/500/similar"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "status": 404, "subStatus": 0, "userMessage": "not found"
        })))
        .mount(&server)
        .await;
    let c = client(&server);
    assert_eq!(c.album_similar(500).await.unwrap(), Vec::new());
}

#[tokio::test]
async fn artist_entity_and_top_tracks() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/artists/9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 9, "name": "An Artist", "picture": "1e01cdb6-f15d-4d8b-8440-a047976c1cac"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/artists/9/toptracks"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"items": [{"id": 1, "title": "Hit"}]})),
        )
        .mount(&server)
        .await;
    let c = client(&server);
    let artist = c.artist(9).await.unwrap();
    assert_eq!(artist.name, "An Artist");
    let top = c.artist_top_tracks(9, 10, 0).await.unwrap();
    assert_eq!(top.items[0].title, "Hit");
}

#[tokio::test]
async fn playlist_and_playlist_items() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/playlists/pl-1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"uuid": "pl-1", "title": "My Playlist"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/playlists/pl-1/items"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [
                {"type": "track", "item": {"id": 1, "title": "T1"}},
                {"type": "video", "item": {"id": 2, "title": "V1"}}
            ],
            "totalNumberOfItems": 2
        })))
        .mount(&server)
        .await;
    let c = client(&server);
    let pl: Playlist = c.playlist("pl-1").await.unwrap();
    assert_eq!(pl.title, "My Playlist");
    let items = c.playlist_items("pl-1", 50, 0).await.unwrap();
    assert_eq!(items.items.len(), 2);
    assert!(items.items[0].as_track().is_some());
    assert!(items.items[1].as_track().is_none());
}

#[tokio::test]
async fn mix_items_best_effort_shape() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/mixes/mix-1/items"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{"type": "track", "item": {"id": 5, "title": "Mixed"}}]
        })))
        .mount(&server)
        .await;
    let c = client(&server);
    let page = c.mix_items("mix-1", 50, 0).await.unwrap();
    assert_eq!(page.items[0].as_track().unwrap().title, "Mixed");
}

#[tokio::test]
async fn video_metadata_only() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/videos/77"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"id": 77, "title": "A Video", "quality": "MP4_1080P"})),
        )
        .mount(&server)
        .await;
    let c = client(&server);
    let v = c.video(77).await.unwrap();
    assert_eq!(v.title, "A Video");
    assert_eq!(v.quality.as_deref(), Some("MP4_1080P"));
}

#[tokio::test]
async fn search_returns_every_type_plus_top_hit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/search"))
        .and(query_param("query", "test"))
        .and(query_param(
            "types",
            "artists,albums,tracks,videos,playlists,mixs",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "artists": {"items": [{"id": 1, "name": "Art"}]},
            "albums": {"items": [{"id": 2, "title": "Alb"}]},
            "tracks": {"items": [{"id": 3, "title": "Trk"}]},
            "videos": {"items": []},
            "playlists": {"items": []},
            "topHit": {"type": "TRACKS", "value": {"id": 3, "title": "Trk"}}
        })))
        .mount(&server)
        .await;
    let c = client(&server);
    let results = c.search("test", 10, 0).await.unwrap();
    assert_eq!(results.artists.items[0].name, "Art");
    assert_eq!(results.albums.items[0].title, "Alb");
    assert_eq!(results.tracks.items[0].title, "Trk");
    assert_eq!(results.top_hit.unwrap().kind.as_deref(), Some("TRACKS"));
}

#[tokio::test]
async fn favorite_track_add_list_and_remove() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/users/7/favorites/tracks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{"created": "2024-01-01", "item": {"id": 1, "title": "Fav"}}],
            "totalNumberOfItems": 1
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/users/7/favorites/tracks"))
        .respond_with(ResponseTemplate::new(201))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/users/7/favorites/tracks/1"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    let page = c.favorite_tracks(50, 0, None, None).await.unwrap();
    assert_eq!(page.items[0].item.title, "Fav");
    c.favorite_track(1).await.unwrap();
    c.unfavorite_track(1).await.unwrap();
}

#[tokio::test]
async fn follow_artist_is_favorite_artist_under_the_hood() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/users/7/favorites/artists"))
        .respond_with(ResponseTemplate::new(201))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    c.follow_artist(42).await.unwrap();
}

#[tokio::test]
async fn lyrics_404_is_not_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/tracks/1/lyrics"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "status": 404, "subStatus": 0, "userMessage": "no lyrics"
        })))
        .mount(&server)
        .await;
    let c = client(&server);
    assert_eq!(c.lyrics(1).await.unwrap(), None);
}

#[tokio::test]
async fn lyrics_200_carries_synced_subtitles() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/tracks/2/lyrics"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "trackId": 2,
            "lyrics": "plain text",
            "subtitles": "[00:01.00]line one\n[00:02.00]line two"
        })))
        .mount(&server)
        .await;
    let c = client(&server);
    let l = c.lyrics(2).await.unwrap().unwrap();
    assert_eq!(l.lyrics.as_deref(), Some("plain text"));
    let lines = streamboat_core::api::lyrics::parse_synced_lyrics(&l.subtitles.unwrap());
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].text, "line one");
}

#[tokio::test]
async fn playlist_create_update_and_delete() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/v2/my-collection/playlists/folders/create-playlist"))
        .and(query_param("name", "New Playlist"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"uuid": "new-uuid"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/playlists/pl-2"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"e-update\"")
                .set_body_json(json!({"uuid": "pl-2", "title": "Old"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/playlists/pl-2"))
        .and(header("if-none-match", "\"e-update\""))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/playlists/pl-2"))
        .and(header("if-none-match", "\"e-update\""))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    let created = c.create_playlist("New Playlist", None, None).await.unwrap();
    assert_eq!(created["uuid"], "new-uuid");
    c.update_playlist("pl-2", Some("New Title"), None, None)
        .await
        .unwrap();
    c.delete_playlist("pl-2", None).await.unwrap();
}

/// The ETag precondition flow, specifically for reorder: fetch the
/// playlist, read its `etag` response header, and send that value back as
/// `If-None-Match` on the mutation (`tidal-api` catalog-and-library.md §7,
/// `tidal-oss-landscape` sone-deep-dive.md §4b).
#[tokio::test]
async fn playlist_reorder_sends_the_fetched_etag_as_if_none_match() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/playlists/pl-3"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"reorder-etag-123\"")
                .set_body_json(json!({"uuid": "pl-3", "title": "Reorder Me"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/playlists/pl-3/items/0,2"))
        .and(header("if-none-match", "\"reorder-etag-123\""))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    c.playlist_reorder("pl-3", &[0, 2], 5, None).await.unwrap();
}

/// Same flow, but the caller already holds an etag from a previous fetch:
/// no extra `GET` should happen.
#[tokio::test]
async fn playlist_reorder_skips_the_fetch_when_an_etag_is_supplied() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/playlists/pl-4"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/playlists/pl-4/items/1"))
        .and(header("if-none-match", "\"held-etag\""))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    c.playlist_reorder("pl-4", &[1], 0, Some("\"held-etag\""))
        .await
        .unwrap();
}

#[tokio::test]
async fn playlist_add_tracks_and_remove_items_use_the_etag_flow_too() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/playlists/pl-5"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"e5\"")
                .set_body_json(json!({"uuid": "pl-5"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/playlists/pl-5/items"))
        .and(header("if-none-match", "\"e5\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"addedItemIds": [10, 11]})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/playlists/pl-5/items/0"))
        .and(header("if-none-match", "\"e5\""))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    let added = c
        .playlist_add_tracks(
            "pl-5",
            &[10, 11],
            None,
            streamboat_core::api::playlists::OnDupes::Add,
            streamboat_core::api::playlists::OnArtifactNotFound::Skip,
            None,
        )
        .await
        .unwrap();
    assert_eq!(added, vec![10, 11]);
    c.playlist_remove_items("pl-5", &[0], None).await.unwrap();
}

#[tokio::test]
async fn favorite_playlist_and_video_use_their_own_field_names() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/users/7/favorites/playlists"))
        .respond_with(ResponseTemplate::new(201))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/users/7/favorites/videos"))
        .and(query_param("limit", "100"))
        .respond_with(ResponseTemplate::new(201))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    c.favorite_playlist("uuid-1").await.unwrap();
    c.favorite_video(9).await.unwrap();
}

#[tokio::test]
async fn favorite_ids_are_all_strings() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/users/7/favorites/ids"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "TRACK": ["1", "2"], "ALBUM": ["3"], "ARTIST": [], "PLAYLIST": ["uuid-a"]
        })))
        .mount(&server)
        .await;
    let c = client(&server);
    let ids = c.favorite_ids().await.unwrap();
    assert_eq!(ids.track, vec!["1", "2"]);
    assert_eq!(ids.playlist, vec!["uuid-a"]);
}

#[tokio::test]
async fn playlists_and_favorite_playlists_unwraps_the_created_wrapper() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/users/7/playlistsAndFavoritePlaylists"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{
                "playlist": {"uuid": "pl-9", "title": "Wrapped Playlist"},
                "created": "2023-05-05T00:00:00.000+0000"
            }]
        })))
        .mount(&server)
        .await;
    let c = client(&server);
    let pls = c.playlists_and_favorite_playlists().await.unwrap();
    assert_eq!(pls[0].title, "Wrapped Playlist");
    assert_eq!(
        pls[0].created.as_deref(),
        Some("2023-05-05T00:00:00.000+0000")
    );
}

#[tokio::test]
async fn home_feed_home_tabs_and_expand_section_over_the_wire() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/home/feed/static"))
        .and(query_param("countryCode", "NL"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "header": {"vibes": {"items": [{"name": "For You", "type": "STATIC"}]}},
            "items": [{
                "type": "TRACK_LIST",
                "title": "Some Tracks",
                "apiPath": "artist/9/ARTIST_TOP_TRACKS/view-all",
                "items": []
            }],
            "cursor": "next-page"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/artist/9/ARTIST_TOP_TRACKS/view-all"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": [{"id": 1}]})))
        .mount(&server)
        .await;
    let c = client(&server);
    let tabs = c.home_tabs().await.unwrap();
    assert_eq!(tabs[0].slug(), Some("static".to_string()));
    let feed = c.home_feed("static", None).await.unwrap();
    assert_eq!(feed.cursor.as_deref(), Some("next-page"));
    let expanded = c
        .expand_section(&feed.items[0].api_path.clone().unwrap(), 50, 0)
        .await
        .unwrap();
    assert_eq!(expanded["items"][0]["id"], 1);
}

#[tokio::test]
async fn explore_and_page_v1_fallback() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/pages/explore"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "title": "Explore",
            "rows": [{"modules": [{"type": "ALBUM_LIST", "title": "New", "pagedList": {"items": []}}]}]
        })))
        .mount(&server)
        .await;
    let c = client(&server);
    let page = c.explore().await.unwrap();
    assert_eq!(page.title.as_deref(), Some("Explore"));
    assert_eq!(page.rows[0].modules[0].kind, "ALBUM_LIST");
}
