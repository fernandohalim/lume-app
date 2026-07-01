//! Runtime configuration. Nothing is compiled into the binary — the app reads a
//! **`lume.env`** file at startup, so every build (yours or a friend's) is
//! configured the same way and `lume.env` is *required* to run.
//!
//! Files are loaded (first value for a key wins; an already-set process env var
//! beats all, which is how `dev.cmd`/`dev.sh` inject values):
//!   1. `lume.env` next to the executable   ← how a distributed build is configured
//!   2. `lume.env` / `.env` / `../.env` in the CWD ← conveniences for local dev
//!
//! `LUME_SPOTIFY_CLIENT_ID` is mandatory; `LUME_HTTP_PROXY` is optional (blank = none).

use std::sync::OnceLock;

struct Config {
    client_id: Option<String>,
    proxy: Option<String>,
}

fn load() -> &'static Config {
    static CFG: OnceLock<Config> = OnceLock::new();
    CFG.get_or_init(|| {
        // Pull candidate files into the process env. dotenvy never overwrites an
        // already-set var, so the priority order below is preserved.
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let _ = dotenvy::from_path(dir.join("lume.env"));
            }
        }
        let _ = dotenvy::from_filename("lume.env");
        let _ = dotenvy::from_filename(".env");
        let _ = dotenvy::from_filename("../.env");

        let nonblank = |k: &str| std::env::var(k).ok().filter(|s| !s.is_empty());
        Config {
            client_id: nonblank("LUME_SPOTIFY_CLIENT_ID"),
            proxy: nonblank("LUME_HTTP_PROXY"),
        }
    })
}

/// True when a Client ID is configured — the UI uses this to gate the app.
#[tauri::command]
pub fn is_configured() -> bool {
    load().client_id.is_some()
}

pub fn client_id() -> Result<String, String> {
    load().client_id.clone().ok_or_else(|| {
        "No Spotify Client ID. Create a `lume.env` file next to the app (copy \
         lume.env.example) and set LUME_SPOTIFY_CLIENT_ID, then restart."
            .to_string()
    })
}

pub fn proxy() -> Option<String> {
    load().proxy.clone()
}
