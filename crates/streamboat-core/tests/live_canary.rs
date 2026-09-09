//! The opt-in live canary (D-046): exercises the real `api.tidal.com`
//! against the developer's own already-logged-in account, to catch API
//! drift the synthetic-fixture test suite cannot. **Never run in CI** —
//! every test here is `#[ignore]` and additionally self-skips unless
//! `STREAMBOAT_LIVE_CANARY=1` is set, so a bare `cargo test --workspace`
//! (what CI runs) neither builds a network dependency into the result nor
//! needs `--ignored` passed to skip it by accident. Run by hand with:
//!
//! ```text
//! STREAMBOAT_LIVE_CANARY=1 cargo test -p streamboat-core --test live_canary -- --ignored
//! ```
//!
//! against a box that has already run `streamboat login` (device-code or
//! PKCE) — this suite reads whatever token store `Context::load()` finds
//! (respecting `STREAMBOAT_HOME` the same way every other command does),
//! it does not accept or mint credentials of its own. Every assertion here
//! checks *shape* — a field exists and has the right type/enum member —
//! never printing a token, a full stream URL, or anything else that would
//! turn a CI log into a credential leak if this ever ran there by mistake.

use streamboat_core::bootstrap::Context;

/// Gate every test in this file: the explicit opt-in flag, and a
/// `Context` that is actually logged in already. Panics with a clear
/// message rather than silently reporting a false pass when the
/// environment is missing something, so a maintainer running this by hand
/// gets a useful error instead of a confusing skip.
async fn require_live_account() -> Context {
    assert_eq!(
        std::env::var("STREAMBOAT_LIVE_CANARY").ok().as_deref(),
        Some("1"),
        "the live canary only runs with STREAMBOAT_LIVE_CANARY=1 set explicitly"
    );
    let ctx = Context::load()
        .expect("Context::load() failed — is STREAMBOAT_HOME (if set) pointing at a real config?");
    assert!(
        ctx.api.is_logged_in().await,
        "no stored session; run `streamboat login` against this STREAMBOAT_HOME first"
    );
    ctx
}

/// Login-state check: a logged-in `Context` reports a real session with a
/// non-empty country code and *some* user id — never printed, just shaped.
#[tokio::test]
#[ignore = "live TIDAL account required; see the module doc comment"]
async fn login_state_reports_a_real_session() {
    let ctx = require_live_account().await;
    let session = ctx
        .api
        .session()
        .await
        .expect("GET /v1/sessions should succeed for a logged-in account");
    assert!(
        !session.country_code.is_empty(),
        "a real session always carries a country code"
    );
    assert!(
        session.user_id.is_some(),
        "a real session always carries a user id"
    );
}

/// One search: asserts the response shape (at least one result, with the
/// documented fields present), never asserts on specific catalogue content
/// (TIDAL's catalogue changes; the query below exists only to return
/// *something*).
#[tokio::test]
#[ignore = "live TIDAL account required; see the module doc comment"]
async fn search_returns_shaped_results() {
    let ctx = require_live_account().await;
    let page = ctx
        .api
        .search_tracks("a", 5)
        .await
        .expect("GET /v1/search/tracks should succeed for a logged-in account");
    assert!(
        !page.items.is_empty(),
        "searching for a single common letter should return at least one track"
    );
    let first = &page.items[0];
    assert!(first.id > 0, "a real track always has a positive id");
    assert!(!first.title.is_empty(), "a real track always has a title");
}

/// One `playbackinfopostpaywall` resolve, through the same
/// `resolve_stream` cascade playback uses: asserts the manifest parsed and
/// something playable came back, never plays the audio or logs the
/// resolved (signed, expiring) URL.
#[tokio::test]
#[ignore = "live TIDAL account required; see the module doc comment"]
async fn playback_info_resolves_to_a_playable_manifest() {
    let ctx = require_live_account().await;
    let page = ctx
        .api
        .search_tracks("a", 1)
        .await
        .expect("search should succeed to get a real track id to resolve");
    let track_id = page
        .items
        .first()
        .expect("search for a single common letter should return a track")
        .id;
    let ceiling = ctx.settings.quality_ceiling();
    let session_id = uuid::Uuid::new_v4().to_string();
    let resolved = ctx
        .api
        .resolve_stream(track_id, ceiling, &session_id)
        .await
        .expect("playbackinfopostpaywall should resolve to a playable stream");
    assert!(
        resolved.requested.rank() <= ceiling.rank(),
        "the cascade must never resolve above the requested ceiling"
    );
    assert!(
        !resolved.info.manifest_kind.is_empty(),
        "a resolved stream always names its manifest kind (bts/dash/emu)"
    );
    // Deliberately not printed: the resolved `source` carries a signed,
    // time-limited CDN URL (or the DASH document containing one) — logging
    // it, even to a local terminal, is exactly the kind of thing this
    // module's own doc comment promises never to do.
}
