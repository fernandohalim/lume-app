//! Shared reqwest client. Applies the proxy baked in at build time from
//! `LUME_HTTP_PROXY` (see build.rs / .env) so the installed binary reaches
//! Spotify on a corporate network without any runtime env vars. Blank => direct
//! connection (home / macOS). localhost is always bypassed so the OAuth loopback
//! and any local calls stay direct.

use std::sync::OnceLock;

/// A process-wide client (cheap to clone — it's `Arc`-backed internally).
pub fn client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            let mut builder = reqwest::Client::builder();
            if let Some(url) = crate::config::proxy() {
                if let Ok(proxy) = reqwest::Proxy::all(&url) {
                    builder = builder
                        .proxy(proxy.no_proxy(reqwest::NoProxy::from_string("localhost,127.0.0.1")));
                }
            }
            builder.build().unwrap_or_else(|_| reqwest::Client::new())
        })
        .clone()
}
