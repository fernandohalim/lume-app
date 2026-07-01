//! Phase 5 — synced lyrics via LRCLIB (https://lrclib.net).
//!
//! Spotify's Web API never exposes lyrics, so we fetch them from LRCLIB — free,
//! no auth, returns synced `.lrc`. We look a track up by name/artist/album/
//! duration, parse the `[mm:ss.xx]` timestamps into `{timeMs,text}` lines, and
//! fall back to plain (unsynced) lyrics when no synced version exists.
//!
//! Results are cached per-track so the panel doesn't refetch on every poll.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

const API: &str = "https://lrclib.net/api/get";
// LRCLIB asks clients to identify themselves (see their API docs).
const UA: &str = concat!("lume/", env!("CARGO_PKG_VERSION"), " (https://github.com/lume-app)");

/// Shape sent to the webview (camelCase to match the Svelte side).
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    /// True when `lines` carry real timestamps (karaoke scroll); false = plain text.
    synced: bool,
    /// LRCLIB flagged the track as instrumental — show a ♪ state, not "not found".
    instrumental: bool,
    lines: Vec<LyricLine>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LyricLine {
    /// Milliseconds into the track. `-1` for unsynced lines (no timestamp).
    time_ms: i64,
    text: String,
}

impl Lyrics {
    fn empty() -> Self {
        Lyrics { synced: false, instrumental: false, lines: Vec::new() }
    }
    fn instrumental() -> Self {
        Lyrics { synced: false, instrumental: true, lines: Vec::new() }
    }
}

/// The subset of LRCLIB's response we read (their JSON is camelCase).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrcResponse {
    #[serde(default)]
    instrumental: bool,
    #[serde(default)]
    plain_lyrics: Option<String>,
    #[serde(default)]
    synced_lyrics: Option<String>,
}

fn cache() -> &'static Mutex<HashMap<String, Lyrics>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Lyrics>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Fetch (and cache) lyrics for a track. Never errors on "not found" — an empty
/// `Lyrics` means the panel shows a tidy empty state rather than an error.
#[tauri::command]
pub async fn get_lyrics(
    title: String,
    artist: String,
    album: String,
    duration_ms: u64,
) -> Result<Lyrics, String> {
    let title = title.trim();
    let artist = artist.trim();
    if title.is_empty() {
        return Ok(Lyrics::empty());
    }

    // Duration rounded to whole seconds — LRCLIB matches with a small tolerance.
    let duration = ((duration_ms as f64) / 1000.0).round() as u64;
    let duration_str = duration.to_string();
    let key = format!("{title}\u{1}{artist}\u{1}{duration}");

    if let Ok(map) = cache().lock() {
        if let Some(hit) = map.get(&key) {
            return Ok(hit.clone());
        }
    }

    let resp = crate::http::client()
        .get(API)
        .header(reqwest::header::USER_AGENT, UA)
        .query(&[
            ("track_name", title),
            ("artist_name", artist),
            ("album_name", album.trim()),
            ("duration", duration_str.as_str()),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    // 404 = LRCLIB has no match for this track. Treat as "no lyrics", cache it so
    // we don't hammer them on every poll for a track they don't have.
    let lyrics = if resp.status() == reqwest::StatusCode::NOT_FOUND {
        Lyrics::empty()
    } else if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("lrclib returned {status}: {body}"));
    } else {
        let r: LrcResponse = resp.json().await.map_err(|e| e.to_string())?;
        build(r)
    };

    if let Ok(mut map) = cache().lock() {
        // Bound the cache so a long session doesn't grow it without limit.
        if map.len() > 128 {
            map.clear();
        }
        map.insert(key, lyrics.clone());
    }
    Ok(lyrics)
}

/// Turn an LRCLIB response into our line list: prefer synced, then plain.
fn build(r: LrcResponse) -> Lyrics {
    if let Some(synced) = r.synced_lyrics.as_deref() {
        let lines = parse_lrc(synced);
        if !lines.is_empty() {
            return Lyrics { synced: true, instrumental: false, lines };
        }
    }
    if let Some(plain) = r.plain_lyrics.as_deref() {
        let lines: Vec<LyricLine> = plain
            .lines()
            .map(|l| LyricLine { time_ms: -1, text: l.trim_end().to_string() })
            .collect();
        if lines.iter().any(|l| !l.text.is_empty()) {
            return Lyrics { synced: false, instrumental: false, lines };
        }
    }
    if r.instrumental {
        return Lyrics::instrumental();
    }
    Lyrics::empty()
}

/// Parse an `.lrc` body into timestamped lines, sorted by time.
///
/// Each line may carry one or more `[mm:ss.xx]` (or `[mm:ss]`) prefixes; metadata
/// tags like `[ar:...]`/`[length:...]` have non-numeric bodies and are skipped.
/// A timestamp with empty text is kept — those are the instrumental gaps that let
/// the karaoke scroll breathe between verses.
fn parse_lrc(body: &str) -> Vec<LyricLine> {
    let mut out: Vec<LyricLine> = Vec::new();
    for raw in body.lines() {
        let mut rest = raw;
        let mut stamps: Vec<i64> = Vec::new();
        // Consume every leading `[...]` group on the line.
        while rest.starts_with('[') {
            let Some(close) = rest.find(']') else { break };
            let inner = &rest[1..close];
            if let Some(ms) = parse_stamp(inner) {
                stamps.push(ms);
            } else {
                // Not a timestamp (e.g. a metadata tag) — stop scanning prefixes.
                break;
            }
            rest = &rest[close + 1..];
        }
        if stamps.is_empty() {
            continue;
        }
        let text = rest.trim().to_string();
        for ms in stamps {
            out.push(LyricLine { time_ms: ms, text: text.clone() });
        }
    }
    out.sort_by_key(|l| l.time_ms);
    out
}

/// Parse `mm:ss`, `mm:ss.xx`, or `mm:ss.xxx` into milliseconds. Returns `None`
/// for anything non-numeric (so metadata tags are rejected).
fn parse_stamp(s: &str) -> Option<i64> {
    let (min_s, rest) = s.split_once(':')?;
    let minutes: i64 = min_s.trim().parse().ok()?;
    let (sec_s, frac_s) = match rest.split_once('.') {
        Some((a, b)) => (a, b),
        None => (rest, ""),
    };
    let seconds: i64 = sec_s.parse().ok()?;
    let frac_ms = if frac_s.is_empty() {
        0
    } else {
        // Normalise hundredths/thousandths to milliseconds.
        let digits: String = frac_s.chars().take(3).collect();
        let scale = 10i64.pow(3 - digits.len() as u32);
        digits.parse::<i64>().ok()? * scale
    };
    Some((minutes * 60 + seconds) * 1000 + frac_ms)
}
