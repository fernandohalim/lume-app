# Lume — project brief for Claude Code

A sleek, always-on-top desktop **miniplayer** that remote-controls the user's
running Spotify desktop app via the Spotify Web API. Personal use, Windows + macOS.
We do **not** stream audio — the official Spotify client stays open (can be
minimized) and acts as the Spotify Connect device we send commands to.

**Full phased plan is in `ROADMAP.md` — read it before building.** It contains the
current Spotify API constraints (many endpoints were deprecated in Nov 2024 and
Feb 2026); do not call dead endpoints.

## Stack (already scaffolded)
- Tauri 2 + SvelteKit (TypeScript), Vite. SPA mode (`ssr = false`).
- Window is configured frameless, transparent, always-on-top (`src-tauri/tauri.conf.json`).
- Secrets live in **Rust**: do OAuth (Authorization Code + PKCE) and token storage
  in `src-tauri`, store tokens in the OS keychain (`keyring` crate), expose
  `#[tauri::command]`s to the frontend. Never put tokens or client secret in the webview.

## Design direction (the whole point of the project)
The native Spotify miniplayer is being replaced because its UI is dated. Keep the
bar high. The signature idea: **the album art is the light source.** Sample a
dominant color from the current cover into `--accent`, and a blurred copy of the
art blooms behind the card — so the player glows the color of whatever is playing.
Everything else is quiet dark glass. Tokens live in `src/lib/theme.css`; the
starter now-playing screen is `src/routes/+page.svelte` (placeholder data + TODOs).

- Palette: near-black `#0b0b0f` glass, dynamic art-derived accent (fallback lilac).
- Type: Bricolage Grotesque (title/labels) + Geist Mono (timestamps, tabular). Two faces only.
- Motion: minimal — smooth progress interpolation, 380ms accent cross-fade on track change, reduced-motion respected.

## Required OAuth scopes
`user-read-playback-state user-modify-playback-state user-read-currently-playing`
(add `playlist-read-private` only if listing the user's playlists).

## Start here
1. Register a Spotify app (Dashboard → Web API), redirect URI `http://127.0.0.1:8888/callback`
   (use the `127.0.0.1` IP form, not `localhost`), and add yourself as an allowlisted user.
2. Build **Phase 1** (OAuth PKCE + keychain token storage + silent refresh) and verify
   it survives a restart before any UI work.
3. Then **Phase 2**: wire `get_playback_state` + transport commands into the existing UI.

## Dev
```
npm install
npm run tauri dev
```
(Requires the Rust toolchain + Tauri OS prerequisites installed locally — see https://tauri.app/start/prerequisites/)
