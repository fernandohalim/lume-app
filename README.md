<div align="center">
  <img src="assets/icon.png" alt="lūme logo" width="120" />

  # lūme
  **A Spotify Custom Miniplayer**

  [![Tauri](https://img.shields.io/badge/Tauri_2-24C8DB?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app/)
  [![SvelteKit](https://img.shields.io/badge/SvelteKit-FF3E00?style=flat-square&logo=svelte&logoColor=white)](https://kit.svelte.dev/)
  [![TypeScript](https://img.shields.io/badge/TypeScript-3178C6?style=flat-square&logo=typescript&logoColor=white)](https://www.typescriptlang.org/)
  [![Rust](https://img.shields.io/badge/Rust-000000?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org/)
  [![Spotify Web API](https://img.shields.io/badge/Spotify_Web_API-1DB954?style=flat-square&logo=spotify&logoColor=white)](https://developer.spotify.com/documentation/web-api)

  [Report a Bug](https://github.com/fernandohalim/lume-app/issues)

</div>

## What is lūme?

**lūme** is a sleek, always-on-top desktop miniplayer that remote-controls your
already-running Spotify desktop app — a modern replacement for Spotify's dated
native miniplayer. It doesn't stream audio itself: the official client stays open
(minimized is fine) as a Spotify Connect device, and lūme sends it commands over
the Spotify Web API.

The whole idea lives in the name — *luminescence*. **the album art is the light
source:** lūme samples a dominant colour from the current cover and a blurred copy
of the art blooms behind the card, so the player literally glows the colour of
whatever's playing. Everything else is quiet dark glass.

> Personal-use / hobby app. runs on Windows + macOS, needs a **Spotify Premium**
> account (every playback-control endpoint requires it).

## Features

* **Album Art as The Light Source:** A dominant colour is sampled from each cover in Rust and drives the accent, the backdrop bloom, the progress fill, and the active icons — with a smooth 380ms cross-fade on every track change.
* **Full Remote Transport:** Play/pause, next/previous, click-or-drag seek, volume, shuffle, and repeat — all optimistic, with a fast reconcile burst so a remote command *feels* instant.
* **Synced Lyrics:** Karaoke-style highlight and auto-scroll pulled from **LRCLIB**, with tap-a-line-to-seek, a plain-text fallback, and tidy instrumental / not-found states.
* **Queue + Search:** Browse what's up next, search tracks and append to the queue, and tap any queued row to skip straight to it (the queue is preserved).
* **Mini-Pill Mode:** Shrink the whole window into a small draggable pill that snaps to the nearest screen corner, with the cover spinning like a vinyl record — click to expand back to the full player.
* **System Tray:** Left-click to hide/show, close to the tray (the app keeps running), quit from the tray or ⌘Q. minimize, minify, and close controls live in a hover-revealed cluster.
* **Secrets Stay in Rust:** OAuth (Authorization Code + PKCE, no client secret) and token storage happen in the Rust core — tokens live in the **OS keychain** and never touch the webview.
* **Quiet, Considerate UI:** Frameless transparent glass, always-on-top, remembers its window position, marquee titles only when they overflow, and full `prefers-reduced-motion` support.

## Tech stack

this project pairs a small native Rust core with a lightweight web frontend:

* **Framework:** [Tauri 2](https://tauri.app/) (Rust core + webview, tiny native binary)
* **Frontend:** [SvelteKit](https://kit.svelte.dev/) (SPA mode) + [TypeScript](https://www.typescriptlang.org/) + [Vite](https://vite.dev/)
* **Core & Secrets:** [Rust](https://www.rust-lang.org/) — [reqwest](https://docs.rs/reqwest) (HTTP), [keyring](https://docs.rs/keyring) (OS keychain), hand-rolled PKCE
* **Music Control:** [Spotify Web API](https://developer.spotify.com/documentation/web-api)
* **Lyrics:** [LRCLIB](https://lrclib.net/) (free, synced `.lrc`, no auth)

## Getting started

You'll need [Node.js](https://nodejs.org/), the [Rust toolchain](https://rustup.rs/),
the [Tauri OS prerequisites](https://tauri.app/start/prerequisites/), and a **Spotify
Premium** account.

**1. Register a Spotify app** at the [Developer Dashboard](https://developer.spotify.com/dashboard)
(select *Web API*), set the redirect URI to `http://127.0.0.1:8888/callback` (use the
`127.0.0.1` IP form, not `localhost`), and add your account under the dev-mode user
allowlist. Copy the **Client ID**.

**2. Clone, configure, and run:**

```bash
# clone the repository
git clone https://github.com/fernandohalim/lume-app.git

# jump into the directory
cd lume-app

# install the frontend dependencies
npm install

# configure: copy the example env and add your Spotify Client ID
cp lume.env.example lume.env
# then edit lume.env  →  LUME_SPOTIFY_CLIENT_ID=your_client_id_here
# (LUME_HTTP_PROXY is optional — leave blank for a direct connection)

# start the app in dev mode
.\dev.cmd        # Windows
bash dev.sh      # macOS / Linux
```

> The launchers wire up the toolchain (and an optional corporate proxy from `.env`)
> that a bare `npm run tauri dev` doesn't. to build a distributable, use
> `.\build.cmd` (Windows) or `bash build.sh` (macOS/Linux).

## License

This project is licensed under the MIT License — see the **LICENSE** file for details.
