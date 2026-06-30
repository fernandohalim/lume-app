//! Phase 2 — playback state + transport commands against the Spotify Web API.
//!
//! Every call authorizes through `auth::valid_access_token` (silent refresh).
//! Transport commands return `Ok(())` on success; the frontend re-polls after
//! each one rather than trusting optimistic state (the API doesn't guarantee
//! command ordering — see ROADMAP gotchas).

use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};

use crate::auth::valid_access_token;

const API: &str = "https://api.spotify.com/v1";

/// Shape sent to the webview (camelCase to match the Svelte side).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playback {
    /// False when there's no active device or nothing is loaded.
    is_active: bool,
    is_playing: bool,
    progress_ms: u64,
    duration_ms: u64,
    title: String,
    artist: String,
    album: String,
    /// Largest cover image URL, or "" — the frontend falls back to a gradient.
    art: String,
    device_name: String,
    volume_percent: u8,
    shuffle: bool,
    /// "off" | "context" | "track"
    repeat: String,
    track_uri: String,
}

impl Default for Playback {
    fn default() -> Self {
        Playback {
            is_active: false,
            is_playing: false,
            progress_ms: 0,
            duration_ms: 0,
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            art: String::new(),
            device_name: String::new(),
            volume_percent: 0,
            shuffle: false,
            repeat: "off".into(),
            track_uri: String::new(),
        }
    }
}

// ---- Spotify response shapes (only the fields we read) --------------------

#[derive(Deserialize)]
struct PlayerResponse {
    #[serde(default)]
    is_playing: bool,
    #[serde(default)]
    progress_ms: u64,
    #[serde(default)]
    shuffle_state: bool,
    #[serde(default)]
    repeat_state: String,
    device: Option<Device>,
    item: Option<Item>,
}

#[derive(Deserialize)]
struct Device {
    #[serde(default)]
    name: String,
    volume_percent: Option<u8>,
}

#[derive(Deserialize)]
struct Item {
    #[serde(default)]
    name: String,
    #[serde(default)]
    duration_ms: u64,
    #[serde(default)]
    uri: String,
    #[serde(default)]
    artists: Vec<Named>,
    album: Option<Album>,
}

#[derive(Deserialize)]
struct Named {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct Album {
    #[serde(default)]
    name: String,
    #[serde(default)]
    images: Vec<Image>,
}

#[derive(Deserialize)]
struct Image {
    #[serde(default)]
    url: String,
}

// ---- commands -------------------------------------------------------------

#[tauri::command]
pub async fn get_playback_state() -> Result<Playback, String> {
    let token = valid_access_token().await?;
    let resp = reqwest::Client::new()
        .get(format!("{API}/me/player"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    // 204 No Content = no active device / nothing playing.
    if resp.status() == StatusCode::NO_CONTENT {
        return Ok(Playback::default());
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("/me/player returned {status}: {body}"));
    }

    let p: PlayerResponse = resp.json().await.map_err(|e| e.to_string())?;
    let Some(item) = p.item else {
        return Ok(Playback::default());
    };
    let album = item.album.unwrap_or(Album {
        name: String::new(),
        images: Vec::new(),
    });
    let (device_name, volume_percent) = match p.device {
        Some(d) => (d.name, d.volume_percent.unwrap_or(0)),
        None => (String::new(), 0),
    };

    Ok(Playback {
        is_active: true,
        is_playing: p.is_playing,
        progress_ms: p.progress_ms,
        duration_ms: item.duration_ms,
        title: item.name,
        artist: item
            .artists
            .iter()
            .map(|a| a.name.as_str())
            .filter(|n| !n.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        album: album.name,
        // Spotify orders images largest-first.
        art: album.images.into_iter().next().map(|i| i.url).unwrap_or_default(),
        device_name,
        volume_percent,
        shuffle: p.shuffle_state,
        repeat: if p.repeat_state.is_empty() {
            "off".into()
        } else {
            p.repeat_state
        },
        track_uri: item.uri,
    })
}

/// Send a transport request with an empty body and check for success.
async fn send(method: Method, path: &str) -> Result<(), String> {
    let token = valid_access_token().await?;
    let resp = reqwest::Client::new()
        .request(method, format!("{API}{path}"))
        .bearer_auth(token)
        .header(reqwest::header::CONTENT_LENGTH, 0)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    // 404 with NO_ACTIVE_DEVICE is the common "Spotify isn't running" case.
    let body = resp.text().await.unwrap_or_default();
    Err(format!("Spotify returned {status}: {body}"))
}

#[tauri::command]
pub async fn play() -> Result<(), String> {
    send(Method::PUT, "/me/player/play").await
}

#[tauri::command]
pub async fn pause() -> Result<(), String> {
    send(Method::PUT, "/me/player/pause").await
}

#[tauri::command]
pub async fn next() -> Result<(), String> {
    send(Method::POST, "/me/player/next").await
}

#[tauri::command]
pub async fn previous() -> Result<(), String> {
    send(Method::POST, "/me/player/previous").await
}

#[tauri::command]
pub async fn seek(position_ms: u64) -> Result<(), String> {
    send(Method::PUT, &format!("/me/player/seek?position_ms={position_ms}")).await
}

#[tauri::command]
pub async fn set_volume(percent: u8) -> Result<(), String> {
    let percent = percent.min(100);
    send(Method::PUT, &format!("/me/player/volume?volume_percent={percent}")).await
}

#[tauri::command]
pub async fn set_shuffle(state: bool) -> Result<(), String> {
    send(Method::PUT, &format!("/me/player/shuffle?state={state}")).await
}

#[tauri::command]
pub async fn set_repeat(state: String) -> Result<(), String> {
    if !matches!(state.as_str(), "off" | "context" | "track") {
        return Err(format!("invalid repeat state: {state}"));
    }
    send(Method::PUT, &format!("/me/player/repeat?state={state}")).await
}
