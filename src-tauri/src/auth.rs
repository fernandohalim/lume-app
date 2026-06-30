//! Phase 1 — Spotify OAuth (Authorization Code + PKCE) and token storage.
//!
//! Everything secret lives here in Rust. The webview only ever sees command
//! results (a display name, a bool) — never the access or refresh token.
//!
//! Flow: generate a PKCE verifier/challenge, bind the loopback listener on
//! 127.0.0.1:8888 *before* opening the browser, send the user to Spotify's
//! consent page, catch the `?code=` on the redirect, exchange it for tokens,
//! and stash them in the OS keychain. `valid_access_token` transparently
//! refreshes when the access token is near expiry.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const CLIENT_ID: &str = "c833608e7cbe4caaa05d60a788a453be";
const REDIRECT_URI: &str = "http://127.0.0.1:8888/callback";
const REDIRECT_PORT: u16 = 8888;
const SCOPES: &str =
    "user-read-playback-state user-modify-playback-state user-read-currently-playing";
const AUTH_URL: &str = "https://accounts.spotify.com/authorize";
const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";

// OS keychain coordinates. One JSON blob holds all three token fields.
const KEYRING_SERVICE: &str = "lume";
const KEYRING_ACCOUNT: &str = "spotify-tokens";

// Refresh a bit early so a token never expires mid-request.
const EXPIRY_SKEW_SECS: u64 = 60;
// Give the user up to 5 minutes to complete the consent screen.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Serialize, Deserialize, Clone)]
struct StoredTokens {
    access_token: String,
    refresh_token: String,
    /// Absolute unix time (secs) after which the access token should be refreshed.
    expires_at: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    // Spotify omits this on refresh responses sometimes — keep the old one then.
    refresh_token: Option<String>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn random_bytes(n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut b);
    b
}

/// (code_verifier, code_challenge) per RFC 7636, S256 method.
fn pkce_pair() -> (String, String) {
    let verifier = URL_SAFE_NO_PAD.encode(random_bytes(32)); // 43 chars, within 43–128
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

// ---- keychain --------------------------------------------------------------

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT).map_err(|e| e.to_string())
}

fn save_tokens(t: &StoredTokens) -> Result<(), String> {
    let json = serde_json::to_string(t).map_err(|e| e.to_string())?;
    entry()?.set_password(&json).map_err(|e| e.to_string())
}

fn load_tokens() -> Result<Option<StoredTokens>, String> {
    match entry()?.get_password() {
        Ok(s) => serde_json::from_str(&s).map(Some).map_err(|e| e.to_string()),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

fn clear_tokens() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

// ---- OAuth flow ------------------------------------------------------------

fn authorize_url(challenge: &str, state: &str) -> String {
    format!(
        "{AUTH_URL}?response_type=code&client_id={}&scope={}&code_challenge_method=S256\
         &code_challenge={}&redirect_uri={}&state={}",
        urlencoding::encode(CLIENT_ID),
        urlencoding::encode(SCOPES),
        urlencoding::encode(challenge),
        urlencoding::encode(REDIRECT_URI),
        urlencoding::encode(state),
    )
}

fn http_page(message: &str) -> String {
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>lūme</title></head>\
         <body style=\"font-family:system-ui;background:#0b0b0f;color:#f2f3f5;display:grid;\
         place-items:center;height:100vh;margin:0\"><p>{message}</p></body></html>"
    );
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    )
}

/// Accept connections until we see `/callback`, validate `state`, return the code.
async fn accept_code(listener: TcpListener, expected_state: &str) -> Result<String, String> {
    loop {
        let (mut stream, _) = listener.accept().await.map_err(|e| e.to_string())?;

        let mut buf = vec![0u8; 8192];
        let n = stream.read(&mut buf).await.map_err(|e| e.to_string())?;
        let request = String::from_utf8_lossy(&buf[..n]);

        // First request line: "GET /callback?code=...&state=... HTTP/1.1"
        let path = request
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or("");

        if !path.starts_with("/callback") {
            // Browsers also ask for /favicon.ico — answer and keep waiting.
            let _ = stream.write_all(http_page("…").as_bytes()).await;
            continue;
        }

        let query = path.splitn(2, '?').nth(1).unwrap_or("");
        let (mut code, mut state, mut error) = (None, None, None);
        for pair in query.split('&') {
            let mut it = pair.splitn(2, '=');
            let key = it.next().unwrap_or("");
            let val = it.next().unwrap_or("");
            let decoded = urlencoding::decode(val).map(|c| c.into_owned()).unwrap_or_default();
            match key {
                "code" => code = Some(decoded),
                "state" => state = Some(decoded),
                "error" => error = Some(decoded),
                _ => {}
            }
        }

        let (message, result) = if let Some(err) = error {
            ("Authorization failed. You can close this window.".to_string(),
             Err(format!("spotify denied authorization: {err}")))
        } else if state.as_deref() != Some(expected_state) {
            ("State mismatch. You can close this window.".to_string(),
             Err("state mismatch — possible CSRF, aborting".to_string()))
        } else if let Some(c) = code {
            ("lūme is connected. You can close this window and return to the app.".to_string(),
             Ok(c))
        } else {
            ("Missing authorization code. You can close this window.".to_string(),
             Err("callback had no code".to_string()))
        };

        let _ = stream.write_all(http_page(&message).as_bytes()).await;
        let _ = stream.flush().await;
        return result;
    }
}

async fn post_token(params: &[(&str, &str)]) -> Result<StoredTokens, String> {
    let resp = reqwest::Client::new()
        .post(TOKEN_URL)
        .form(params)
        .send()
        .await
        .map_err(|e| format!("token request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("token endpoint returned {status}: {body}"));
    }

    let tr: TokenResponse = resp.json().await.map_err(|e| e.to_string())?;
    let refresh_token = tr.refresh_token.unwrap_or_default();
    Ok(StoredTokens {
        access_token: tr.access_token,
        refresh_token,
        expires_at: now_secs() + tr.expires_in.saturating_sub(EXPIRY_SKEW_SECS),
    })
}

// ---- public commands -------------------------------------------------------

/// Run the full PKCE login. Opens the browser, captures the redirect, stores tokens.
#[tauri::command]
pub async fn login() -> Result<(), String> {
    let (verifier, challenge) = pkce_pair();
    let state = URL_SAFE_NO_PAD.encode(random_bytes(16));

    // Bind BEFORE opening the browser so the redirect can never arrive first.
    let listener = TcpListener::bind(("127.0.0.1", REDIRECT_PORT))
        .await
        .map_err(|e| format!("couldn't bind {REDIRECT_URI}: {e} (is another login in progress?)"))?;

    open::that(authorize_url(&challenge, &state))
        .map_err(|e| format!("couldn't open the browser: {e}"))?;

    let code = tokio::time::timeout(LOGIN_TIMEOUT, accept_code(listener, &state))
        .await
        .map_err(|_| "login timed out waiting for Spotify".to_string())??;

    let tokens = post_token(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", REDIRECT_URI),
        ("client_id", CLIENT_ID),
        ("code_verifier", &verifier),
    ])
    .await?;

    if tokens.refresh_token.is_empty() {
        return Err("Spotify returned no refresh token".to_string());
    }
    save_tokens(&tokens)
}

/// True if we hold stored tokens (i.e. the user has logged in before).
#[tauri::command]
pub fn is_authenticated() -> Result<bool, String> {
    Ok(load_tokens()?.is_some())
}

/// Forget the stored tokens.
#[tauri::command]
pub fn logout() -> Result<(), String> {
    clear_tokens()
}

/// A valid access token, refreshing silently when the current one is near expiry.
/// Crate-internal: Phase 2 calls this to authorize Web API requests. Never returned
/// to the webview directly.
pub async fn valid_access_token() -> Result<String, String> {
    let tokens = load_tokens()?.ok_or("not authenticated")?;
    if now_secs() < tokens.expires_at {
        return Ok(tokens.access_token);
    }

    let mut refreshed = post_token(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &tokens.refresh_token),
        ("client_id", CLIENT_ID),
    ])
    .await?;

    // Refresh responses often omit a new refresh token — reuse the existing one.
    if refreshed.refresh_token.is_empty() {
        refreshed.refresh_token = tokens.refresh_token;
    }
    save_tokens(&refreshed)?;
    Ok(refreshed.access_token)
}

/// Phase 1 acceptance probe: prove the stored token works end-to-end by hitting
/// `GET /me`, without ever exposing the token to the frontend. Returns the
/// account's display name.
#[tauri::command]
pub async fn whoami() -> Result<String, String> {
    let token = valid_access_token().await?;
    let resp = reqwest::Client::new()
        .get("https://api.spotify.com/v1/me")
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("/me returned {status}: {body}"));
    }

    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(v.get("display_name")
        .and_then(|d| d.as_str())
        .unwrap_or("(no display name)")
        .to_string())
}
