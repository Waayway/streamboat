//! Pure view-model helpers with no `iced` dependency, so they are plain
//! `#[test]`-able functions: time formatting, feed section to card mapping,
//! message routing.

use streamboat_core::models::{AudioMode, AudioQuality, FeedItem, FeedSection, PageModuleV1};
use streamboat_core::proto::StreamInfo;

/// `mm:ss` (or `h:mm:ss` past an hour), rounding down to the whole second.
/// `None` renders as `--:--`.
pub fn mmss(ms: Option<u64>) -> String {
    let Some(ms) = ms else {
        return "--:--".to_string();
    };
    let total_secs = ms / 1000;
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// The quality badge text for the playback bar / Now Playing / Settings,
/// from what the manifest actually delivered — never from the requested
/// ceiling, which can differ (`ResolvedStream::warnings`).
pub fn quality_badge(quality: Option<AudioQuality>) -> String {
    match quality {
        Some(AudioQuality::HiResLossless) => "MAX".to_string(),
        Some(AudioQuality::Lossless) => "LOSSLESS".to_string(),
        Some(AudioQuality::High) => "HIGH".to_string(),
        Some(AudioQuality::Low) => "LOW".to_string(),
        Some(AudioQuality::HiResLegacy) => "MQA".to_string(),
        None => "—".to_string(),
    }
}

/// A one-line codec/format caption for the signal-path panel and Now
/// Playing, e.g. `FLAC · 24-bit / 96 kHz`.
pub fn format_caption(stream: &StreamInfo) -> String {
    let codec = stream.codec.clone().unwrap_or_else(|| "?".to_string());
    match (stream.bit_depth, stream.sample_rate) {
        (Some(bits), Some(rate)) => {
            format!("{codec} · {bits}-bit / {} kHz", rate as f64 / 1000.0)
        }
        (None, Some(rate)) => format!("{codec} · {} kHz", rate as f64 / 1000.0),
        _ => codec,
    }
}

/// Whether a track's audio mode makes "bit-perfect" a meaningful claim.
/// AAC/lossy sources have no canonical bit-width to preserve — D-036's
/// "lossy source, bit-perfect not applicable" line.
pub fn bit_perfect_applicable(codec: &Option<String>, quality: Option<AudioQuality>) -> bool {
    let lossy_codec = codec
        .as_deref()
        .map(|c| {
            let c = c.to_ascii_uppercase();
            c.contains("AAC") || c.contains("HE-AAC") || c.contains("EAC3") || c.contains("AC3")
        })
        .unwrap_or(false);
    let lossy_quality = matches!(quality, Some(AudioQuality::Low) | Some(AudioQuality::High));
    !lossy_codec && !lossy_quality
}

pub fn audio_mode_label(mode: Option<AudioMode>) -> &'static str {
    match mode {
        Some(AudioMode::DolbyAtmos) => "Dolby Atmos",
        Some(AudioMode::Sony360Ra) => "360 Reality Audio",
        Some(AudioMode::Stereo) | None => "Stereo",
        Some(AudioMode::Unknown) => "Unknown",
    }
}

/// A card's view-model, built from one [`FeedItem`] inside a recognised
/// section. Kept deliberately small: a title, subtitle, artwork id (when
/// this item type carries one) and the [`crate::ui::nav::EntityRef`] a click
/// should navigate to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedCard {
    pub title: String,
    pub subtitle: Option<String>,
    pub image_id: Option<String>,
    pub entity: Option<crate::ui::nav::EntityRef>,
}

fn extra_string(item: &FeedItem, key: &str) -> Option<String> {
    item.extra.get(key)?.as_str().map(str::to_string)
}

fn extra_u64(item: &FeedItem) -> Option<u64> {
    item.id.as_ref().and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
}

fn extra_string_id(item: &FeedItem) -> Option<String> {
    item.id.as_ref().and_then(|v| {
        v.as_str()
            .map(str::to_string)
            .or_else(|| v.as_u64().map(|n| n.to_string()))
    })
}

/// Map one feed item to a card, or `None` for a shape this function does
/// not recognise (the item is simply skipped, never a hard failure — same
/// "graceful unknown fallback" spirit as the section-level one below).
pub fn feed_item_to_card(item: &FeedItem) -> Option<FeedCard> {
    use crate::ui::nav::EntityRef;

    let title = item
        .title
        .clone()
        .or_else(|| extra_string(item, "name"))
        .unwrap_or_default();
    let image_id = extra_string(item, "cover")
        .or_else(|| extra_string(item, "picture"))
        .or_else(|| extra_string(item, "image"))
        .or_else(|| extra_string(item, "squareImage"));
    let subtitle = extra_string(item, "artist")
        .or_else(|| extra_string(item, "subtitle"))
        .or_else(|| extra_string(item, "description"));

    let entity = match item.kind.as_str() {
        "ALBUM" => extra_u64(item).map(EntityRef::Album),
        "ARTIST" => extra_u64(item).map(EntityRef::Artist),
        "TRACK" => extra_u64(item).map(EntityRef::Track),
        "PLAYLIST" => extra_string_id(item).map(EntityRef::Playlist),
        "MIX" => extra_string_id(item).map(EntityRef::Mix),
        _ => None,
    };

    if title.is_empty() && image_id.is_none() && entity.is_none() {
        return None;
    }

    Some(FeedCard {
        title,
        subtitle,
        image_id,
        entity,
    })
}

/// A rendered section: either a recognised list of cards, or the graceful
/// "not supported yet" fallback D-015 requires for a section `type` this
/// crate does not (yet) special-case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectionView {
    Cards {
        title: String,
        cards: Vec<FeedCard>,
        api_path: Option<String>,
    },
    Unsupported {
        title: String,
        kind: String,
    },
}

/// The v2 section `type`s this renderer knows how to turn into cards
/// (`tidal-client-features` browse-pages-screens.md §2).
const KNOWN_SECTION_KINDS: &[&str] = &[
    "SHORTCUT_LIST",
    "HORIZONTAL_LIST",
    "HORIZONTAL_LIST_WITH_CONTEXT",
    "TRACK_LIST",
    "MIXED_LIST",
    "GRID",
];

/// Map one [`FeedSection`] to a [`SectionView`] — never fails, per D-015:
/// an unrecognised `type` becomes [`SectionView::Unsupported`], not a
/// dropped section or a page-render error.
pub fn feed_section_to_view(section: &FeedSection) -> SectionView {
    let title = section
        .title
        .clone()
        .unwrap_or_else(|| section.kind.clone());
    if !KNOWN_SECTION_KINDS.contains(&section.kind.as_str()) {
        return SectionView::Unsupported {
            title,
            kind: section.kind.clone(),
        };
    }
    let cards = section.items.iter().filter_map(feed_item_to_card).collect();
    SectionView::Cards {
        title,
        cards,
        api_path: section.api_path.clone(),
    }
}

/// The `resources.tidal.com` URL for a cover/picture id, trying the album
/// size table then falling back to the artist one — shared by
/// `ui::widgets`' card rendering and `ui::app`'s image-prefetch pass so
/// both agree on exactly one URL per id.
pub fn cover_url(image_id: &str) -> Option<String> {
    streamboat_core::api::images::album_cover_url(image_id, 320)
        .or_else(|_| streamboat_core::api::images::artist_picture_url(image_id, 320))
        .ok()
}

/// A playlist's cover: its square image at the id `cover_url` also uses
/// (320) when present, else its wide image — playlists carry either or both
/// (`tidal-client-features` library-playlists-collections.md §1's `image`
/// vs `squareImage`), and neither shares the album-cover size table
/// (`api/images.rs`'s `PLAYLIST_SQUARE_SIZES`/`WIDE_SIZES`).
pub fn playlist_cover_url(playlist: &streamboat_core::models::Playlist) -> Option<String> {
    if let Some(id) = playlist.square_image.as_deref() {
        if let Ok(url) = streamboat_core::api::images::playlist_square_url(id, 320) {
            return Some(url);
        }
    }
    let id = playlist.image.as_deref()?;
    streamboat_core::api::images::playlist_wide_url(id, 480, 320).ok()
}

/// A video's thumbnail at the smallest documented wide size — videos have
/// no square-cover size table (`api/images.rs`'s `WIDE_SIZES`).
pub fn video_thumbnail_url(image_id: &str) -> Option<String> {
    streamboat_core::api::images::video_thumbnail_url(image_id, 480, 320).ok()
}

/// The v1 module `type`s this renderer knows how to turn into cards
/// (`tidal-client-features` browse-pages-screens.md §2). Explore's items
/// are raw JSON (`PageModuleV1::items`), so unlike [`feed_item_to_card`]
/// this never infers a clickable entity — it fills the graceful "not
/// supported yet" gap for Explore's still-live v1 shape (D-015) without
/// guessing at a per-item entity type this crate has not confirmed.
const KNOWN_MODULE_KINDS: &[&str] = &[
    "ALBUM_LIST",
    "ARTIST_LIST",
    "PLAYLIST_LIST",
    "TRACK_LIST",
    "MIX_LIST",
    "MIXED_TYPES_LIST",
];

/// The cover/picture/image id of a raw v1 item, if any — shared by
/// [`raw_item_to_card`] and by `screens::explore`'s image-prefetch pass so
/// both agree on where an id can come from.
pub fn raw_item_image_id(value: &serde_json::Value) -> Option<String> {
    let obj = value.as_object()?;
    ["cover", "picture", "image", "squareImage"]
        .iter()
        .find_map(|key| obj.get(*key).and_then(|v| v.as_str()))
        .map(str::to_string)
}

fn raw_item_to_card(value: &serde_json::Value) -> Option<FeedCard> {
    let obj = value.as_object()?;
    let title = obj
        .get("title")
        .or_else(|| obj.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let image_id = raw_item_image_id(value);
    let subtitle = obj
        .get("artist")
        .and_then(|a| a.get("name"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if title.is_empty() && image_id.is_none() {
        return None;
    }
    Some(FeedCard {
        title,
        subtitle,
        image_id,
        // v1 items are raw JSON with no confirmed per-item type field
        // (`api/pages.rs`'s doc comment); routing to an entity page is not
        // built yet — it needs that shape captured against a live account
        // first, so these cards render without a click target for now.
        entity: None,
    })
}

/// Map one v1 [`PageModuleV1`] to a [`SectionView`] — same graceful
/// unknown-`type` fallback as [`feed_section_to_view`], for the still-live
/// v1 Pages shape Explore uses (D-015).
pub fn page_module_to_view(module: &PageModuleV1) -> SectionView {
    let title = module.title.clone().unwrap_or_else(|| module.kind.clone());
    if !KNOWN_MODULE_KINDS.contains(&module.kind.as_str()) {
        return SectionView::Unsupported {
            title,
            kind: module.kind.clone(),
        };
    }
    let cards = module.items.iter().filter_map(raw_item_to_card).collect();
    SectionView::Cards {
        title,
        cards,
        api_path: module.show_more_api_path.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use streamboat_core::models::FeedSection;

    #[test]
    fn mmss_formats_sub_hour_durations() {
        assert_eq!(mmss(Some(0)), "00:00");
        assert_eq!(mmss(Some(65_000)), "01:05");
        assert_eq!(mmss(None), "--:--");
    }

    #[test]
    fn mmss_formats_past_an_hour() {
        assert_eq!(mmss(Some(3_661_000)), "1:01:01");
    }

    #[test]
    fn quality_badge_uses_the_ui_label_not_the_wire_name() {
        // LOSSLESS is "High" in TIDAL's own UI but streamboat still shows
        // the wire word here, distinct from HI_RES_LOSSLESS's "MAX".
        assert_eq!(quality_badge(Some(AudioQuality::Lossless)), "LOSSLESS");
        assert_eq!(quality_badge(Some(AudioQuality::HiResLossless)), "MAX");
        assert_eq!(quality_badge(None), "—");
    }

    #[test]
    fn bit_perfect_not_applicable_for_aac() {
        assert!(!bit_perfect_applicable(
            &Some("AAC".to_string()),
            Some(AudioQuality::High)
        ));
        assert!(bit_perfect_applicable(
            &Some("FLAC".to_string()),
            Some(AudioQuality::Lossless)
        ));
    }

    fn section_from(json: serde_json::Value) -> FeedSection {
        serde_json::from_value(json).expect("valid section fixture")
    }

    #[test]
    fn known_section_maps_items_to_cards() {
        let section = section_from(json!({
            "type": "HORIZONTAL_LIST",
            "title": "Suggested New Albums",
            "items": [
                {"type": "ALBUM", "id": 42, "title": "Synthetic Album", "cover": "abc"},
                {"type": "ARTIST", "id": "99", "title": "Synthetic Artist"}
            ]
        }));
        let view = feed_section_to_view(&section);
        match view {
            SectionView::Cards { title, cards, .. } => {
                assert_eq!(title, "Suggested New Albums");
                assert_eq!(cards.len(), 2);
                assert_eq!(cards[0].entity, Some(crate::ui::nav::EntityRef::Album(42)));
                assert_eq!(cards[1].entity, Some(crate::ui::nav::EntityRef::Artist(99)));
            }
            SectionView::Unsupported { .. } => panic!("expected Cards"),
        }
    }

    #[test]
    fn unknown_section_type_falls_back_gracefully() {
        let section = section_from(json!({
            "type": "SOME_FUTURE_MODULE_TYPE",
            "title": "New From TIDAL",
            "items": [{"type": "TRACK", "id": 1, "title": "Whatever"}]
        }));
        let view = feed_section_to_view(&section);
        match view {
            SectionView::Unsupported { title, kind } => {
                assert_eq!(title, "New From TIDAL");
                assert_eq!(kind, "SOME_FUTURE_MODULE_TYPE");
            }
            SectionView::Cards { .. } => panic!("expected Unsupported fallback"),
        }
    }

    #[test]
    fn section_with_no_title_falls_back_to_the_type_name() {
        let section = section_from(json!({"type": "WEIRD_TYPE", "items": []}));
        match feed_section_to_view(&section) {
            SectionView::Unsupported { title, .. } => assert_eq!(title, "WEIRD_TYPE"),
            SectionView::Cards { .. } => panic!("expected Unsupported fallback"),
        }
    }

    fn module_from(json: serde_json::Value) -> PageModuleV1 {
        serde_json::from_value(json).expect("valid v1 module fixture")
    }

    #[test]
    fn known_v1_module_maps_raw_items_to_cards() {
        let module = module_from(json!({
            "type": "ALBUM_LIST",
            "title": "New Releases",
            "pagedList": {"items": [{"title": "Synthetic Album", "cover": "abc"}]}
        }));
        match page_module_to_view(&module) {
            SectionView::Cards { title, cards, .. } => {
                assert_eq!(title, "New Releases");
                assert_eq!(cards.len(), 1);
                assert_eq!(cards[0].title, "Synthetic Album");
            }
            SectionView::Unsupported { .. } => panic!("expected Cards"),
        }
    }

    #[test]
    fn unknown_v1_module_type_falls_back_gracefully() {
        let module = module_from(json!({"type": "TEXT_BLOCK", "title": "About"}));
        match page_module_to_view(&module) {
            SectionView::Unsupported { title, kind } => {
                assert_eq!(title, "About");
                assert_eq!(kind, "TEXT_BLOCK");
            }
            SectionView::Cards { .. } => panic!("expected Unsupported fallback"),
        }
    }
}
