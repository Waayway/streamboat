//! Lyrics: `GET tracks/{id}/lyrics?countryCode=` (sync and plain text), plus
//! a parsed representation of the synced form (`tidal-api`
//! catalog-and-library.md §9).

use crate::error::{Error, Result};
use crate::http::ApiClient;
use crate::models::{LyricLine, LyricsResponse};

impl ApiClient {
    /// `GET tracks/{id}/lyrics?countryCode=XX` ->
    /// `{trackId, lyricsProvider, providerCommontrackId, providerLyricsId,
    /// lyrics, subtitles, isRightToLeft}`. `lyrics` is plain text;
    /// `subtitles`, when present, is LRC-formatted synced lyrics — parse it
    /// with [`parse_synced_lyrics`]. A 404 means "no lyrics for this
    /// track," per the reference — not an error; returns `Ok(None)`.
    pub async fn lyrics(&self, track_id: u64) -> Result<Option<LyricsResponse>> {
        let cc = self.country_code().await?;
        match self
            .get_json::<LyricsResponse>(
                &format!("v1/tracks/{track_id}/lyrics"),
                &[("countryCode", cc)],
                &[],
            )
            .await
        {
            Ok(l) => Ok(Some(l)),
            Err(Error::Api(e)) if e.status == 404 => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// Parse an LRC `subtitles` string into timed lines: `[mm:ss.xx]text`,
/// accepting either `.` or `:` before 1-3 fractional digits — confirmed by
/// two independent reference parsers, Sone's `parseLrc` and High Tide's
/// `[m:ss.xx]text` regex (`tidal-api` catalog-and-library.md §9). A line can
/// carry several timestamp tags (a backing-vocal/duet convention); each tag
/// produces its own [`LyricLine`] with the same text. Lines with no
/// timestamp tag at all are dropped rather than kept with a nonsensical
/// time. Whether any track carries **word-level** (rather than line-level)
/// timing is not established by either reference parser — this function
/// only looks for line-level tags.
pub fn parse_synced_lyrics(subtitles: &str) -> Vec<LyricLine> {
    let mut out = Vec::new();
    for line in subtitles.lines() {
        let mut rest = line;
        let mut times = Vec::new();
        while let Some(start) = rest.find('[') {
            let Some(end_rel) = rest[start..].find(']') else {
                break;
            };
            let end = start + end_rel;
            let Some(ms) = parse_lrc_timestamp(&rest[start + 1..end]) else {
                break;
            };
            times.push(ms);
            rest = &rest[end + 1..];
        }
        if times.is_empty() {
            continue;
        }
        let text = rest.trim().to_string();
        for t in times {
            out.push(LyricLine {
                time_ms: t,
                text: text.clone(),
            });
        }
    }
    out.sort_by_key(|l| l.time_ms);
    out
}

fn parse_lrc_timestamp(tag: &str) -> Option<u64> {
    let (mm_str, rest) = tag.split_once(':')?;
    let mm: u64 = mm_str.trim().parse().ok()?;
    let (ss_str, frac_str) = match rest.find(['.', ':']) {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, ""),
    };
    let ss: u64 = ss_str.trim().parse().ok()?;
    let frac_ms: u64 = if frac_str.is_empty() {
        0
    } else {
        let mut f = frac_str.trim().to_string();
        if !f.chars().all(|c| c.is_ascii_digit()) || f.is_empty() {
            return None;
        }
        f.truncate(3);
        while f.len() < 3 {
            f.push('0');
        }
        f.parse().ok()?
    };
    Some(mm * 60_000 + ss * 1000 + frac_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_lrc_with_dot_fraction() {
        let lines = parse_synced_lyrics("[00:12.34]Hello\n[00:15.5]World");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].time_ms, 12_340);
        assert_eq!(lines[0].text, "Hello");
        assert_eq!(lines[1].time_ms, 15_500);
        assert_eq!(lines[1].text, "World");
    }

    #[test]
    fn accepts_colon_fraction_separator() {
        let lines = parse_synced_lyrics("[01:02:03]Colon variant");
        assert_eq!(
            lines,
            vec![LyricLine {
                time_ms: 62_030,
                text: "Colon variant".into()
            }]
        );
    }

    #[test]
    fn multiple_tags_on_one_line_repeat_the_text() {
        let lines = parse_synced_lyrics("[00:01.00][00:05.00]Duet line");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].time_ms, 1_000);
        assert_eq!(lines[1].time_ms, 5_000);
        assert_eq!(lines[0].text, "Duet line");
        assert_eq!(lines[1].text, "Duet line");
    }

    #[test]
    fn lines_without_a_timestamp_are_dropped() {
        let lines = parse_synced_lyrics("no tag here\n[00:01.00]tagged");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "tagged");
    }

    #[test]
    fn empty_input_yields_no_lines() {
        assert!(parse_synced_lyrics("").is_empty());
    }

    #[test]
    fn sorts_by_time_regardless_of_input_order() {
        let lines = parse_synced_lyrics("[00:05.00]second\n[00:01.00]first");
        assert_eq!(lines[0].text, "first");
        assert_eq!(lines[1].text, "second");
    }
}
