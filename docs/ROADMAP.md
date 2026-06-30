# Custom Spotify Miniplayer — Build Roadmap (Tauri, Windows + macOS)

> Paste this whole file into a new Claude Code session as the project brief.
> It is self-contained: it includes the current Spotify API constraints so you
> don't waste time on deprecated endpoints.

## 1. What we're building

A small, sleek, always-on-top desktop **miniplayer** that **remote-controls the
user's already-running Spotify desktop app**. The goal is a modern, custom UI/UX
to replace Spotify's native miniplayer — for **personal use**.

We do **not** stream audio ourselves. The official Spotify desktop client stays
open (can be minimized); it appears as a Spotify Connect device, and we send it
playback commands via the Spotify Web API. This is simpler and more reliable than
the Web Playback SDK and is the correct architecture for this use case.

**Scope note:** This is a personal/hobby app. Spotify "Development Mode" allows
1 Client ID per developer and up to 5 authorized users. Public distribution would
require Extended Quota Mode, which is organizations-only — out of scope. Do not
architect for multi-tenant scale.

## 2. Hard API constraints (READ THIS — it will save you hours)

The Spotify Web API was heavily cut in Nov 2024 and Feb 2026. Assume your training
data is stale. Specifically:

**Requires Spotify Premium.** Every playback-control endpoint returns
`403 PREMIUM_REQUIRED` for free accounts. The end user must be Premium.

**Deprecated / unavailable to new apps (do NOT call these — they 403/404):**
- `GET /audio-features`, `GET /audio-analysis` (no BPM, key, energy, danceability)
- `GET /recommendations`
- `GET /artists/{id}/related-artists`
- Featured/editorial playlists, `GET /browse/new-releases`
- `GET /artists/{id}/top-tracks`, `GET /markets`
- 30-second preview URLs in multi-get responses

**February 2026 behavior changes:**
- `GET /search`: max `limit` is now **10** (was 50), default 5. Paginate with `offset`.
- Playlist items only returned for playlists the user **owns or collaborates on**.
- Dev Mode requires Premium, 1 Client ID per dev, max 5 authorized users.
- Some library/save/follow calls moved to generic `PUT/DELETE /me/library`.

**Queue is read + append only.** You can `GET /me/player/queue` and
`POST /me/player/queue` (append to end). There is **no** endpoint to reorder,
remove, or clear the queue. Build the queue UI around this limit — no drag-to-reorder
against the live queue.

**No lyrics in the Spotify API, ever.** Use a third-party source (see Phase 5).

### Endpoints we WILL use (all confirmed available)
- `GET /me/player` — full playback state (track, device, `progress_ms`, `is_playing`, shuffle, repeat)
- `GET /me/player/currently-playing`
- `PUT /me/player/play`, `PUT /me/player/pause`
- `POST /me/player/next`, `POST /me/player/previous`
- `PUT /me/player/seek?position_ms=`
- `PUT /me/player/volume?volume_percent=`
- `PUT /me/player/repeat?state=`, `PUT /me/player/shuffle?state=`
- `GET /me/player/queue`, `POST /me/player/queue?uri=`
- `GET /me/player/devices`, `PUT /me/player` (transfer playback)
- `GET /search?type=track,artist,album&limit=10`
- `GET /me/playlists`, `GET /playlists/{id}` (owned/collab for items)

**Required OAuth scopes:**
`user-read-playback-state user-modify-playback-state user-read-currently-playing`
(add `playlist-read-private` if you list the user's playlists).

## 3. Tech stack

- **Tauri 2.x** (Rust core + web frontend). Small binary, native always-on-top.
- **Frontend:** your choice of a lightweight framework — Svelte or React + Vite.
  Keep it minimal; this is a small window.
- **Rust side owns secrets:** handle OAuth token exchange + storage in Rust, expose
  `#[tauri::command]`s to the frontend via `invoke()`. Never put the client secret
  or tokens in the webview/JS. (Note: with PKCE there is no client secret, but the
  access/refresh tokens still live in Rust.)
- **Token storage:** OS keychain via the `keyring` crate (Keychain on macOS,
  Credential Manager on Windows).
- **HTTP:** `reqwest` in Rust.

## 4. Architecture

```
Spotify Web API  <--HTTPS--  [Rust core: OAuth, token refresh, API proxy]
                                      ^  invoke() / events
                                      v
                             [Webview frontend: UI, polling trigger, rendering]
                                      |
Spotify Desktop App (Connect device) <-- commands routed by Web API
```

- Rust exposes commands like `get_playback_state`, `play`, `pause`, `next`,
  `seek(ms)`, `set_volume(pct)`, `add_to_queue(uri)`, `search(query)`,
  `get_lyrics(track, artist, duration_ms)`.
- Frontend polls `get_playback_state` ~every 1s while playing; back off to ~3–5s
  when paused or window unfocused to respect rate limits.
- Rust auto-refreshes the access token when it's near expiry, transparently.

## 5. Phased roadmap

### Phase 0 — Setup

**Naming & identity (locked).** The app is **`lūme`** — always lowercase, macron
ū, set in Bricolage Grotesque. The name *is* the thesis: luminescence — the album
art is the light source, blooming its color through dark glass. Identity direction:
**bold bloom · deep night glass · art-first.** Three layers — *night* (constant
`#0b0b0f` translucent glass), *light* (dynamic `--accent`/`--bloom` sampled from the
cover, used only on the backdrop bloom, progress fill, and active icons), and *quiet*
(everything else recessive so the art stays the brightest thing on screen). Signature
moment: 380ms accent + art cross-fade on track change. Tokens live in `src/lib/theme.css`.

- [ ] Register an app at developer.spotify.com → Dashboard. Select "Web API".
      Set Redirect URI to a loopback: `http://127.0.0.1:8888/callback`
      (use `127.0.0.1`, not `localhost` — Spotify requires the IP form).
- [ ] Add yourself as a user in the app's dashboard (dev mode allowlist).
- [x] Scaffolded: Tauri 2 + SvelteKit (TS), SPA. Window is frameless / transparent /
      always-on-top (`src-tauri/tauri.conf.json`). Builds and runs.
- [ ] Add Rust deps: `reqwest`, `serde`, `tokio`, `keyring`, `oauth2` (or hand-roll PKCE).

### Phase 1 — Auth (Authorization Code + PKCE)
- [ ] Implement PKCE: generate code_verifier/challenge.
- [ ] Open the system browser to `/authorize` with scopes + challenge.
- [ ] Spin up a tiny local listener on `127.0.0.1:8888` to catch the redirect code.
- [ ] Exchange code → access + refresh tokens at `/api/token`.
- [ ] Store tokens in the OS keychain. Implement silent refresh.
- [ ] Acceptance: app authorizes once, survives restart without re-login.

### Phase 2 — Now-playing core
- [ ] `get_playback_state` command + 1s polling loop.
- [ ] Render: album art, track title, artist, album, progress bar, device name.
- [ ] Transport: play/pause, next, previous, seek (click/drag progress bar),
      volume, shuffle toggle, repeat toggle.
- [ ] Handle "no active device" gracefully (prompt user to start Spotify / pick a device via `/devices` + transfer).
- [ ] Acceptance: full control of the running desktop client from the miniplayer.

### Phase 3 — UI/UX polish (the whole point)
- [ ] Frameless, compact, **always-on-top** window. Custom drag region.
- [ ] Custom theme system (consider pulling a dominant color from album art for accents).
- [ ] Smooth progress animation between polls (interpolate locally using `progress_ms` + elapsed time; reconcile on each poll).
- [ ] Remember window position/size between launches.
- [ ] Optional: compact/expanded modes, hover controls.

### Phase 4 — Queue + search
- [ ] Queue view: `GET /me/player/queue` (currently playing + up next). Read-only display.
- [ ] "Add to queue": search (`limit=10`) → `POST /me/player/queue`.
- [ ] Be explicit in UI that reorder/remove isn't supported (API limitation).

### Phase 5 — Synced lyrics
- [ ] Fetch from **LRCLIB** (`https://lrclib.net/api/get` by track name, artist,
      album, duration) — free, returns synced `.lrc`. No auth.
- [ ] Parse `.lrc` timestamps; highlight the current line using `progress_ms`
      from the player state. Interpolate between polls for smooth scroll.
- [ ] Fallback to plain (unsynced) lyrics if no synced version exists.
- [ ] Note: BPM/key/energy are NOT available (deprecated). Skip or use a paid
      third-party API later if truly needed.

### Phase 6 — Nice-to-haves (optional)
- [ ] Global media hotkeys.
- [ ] System tray with mini-controls.
- [ ] Settings panel (poll rate, theme, hotkeys).

## 6. Gotchas
- Rate limits: don't poll faster than ~1/s; back off when idle. Handle `429` with `Retry-After`.
- Token refresh must be transparent; a 401 means refresh and retry once.
- Don't crop or alter album artwork (Spotify design guideline) — frame it, don't edit it.
- Playback commands can briefly race; the API doesn't guarantee ordering, so
  re-fetch state after a command rather than assuming success.
- This stays personal-use (≤5 allowlisted users). Don't build signup/multi-user infra.

## 7. Start here
Begin with **Phase 0 and Phase 1**. Get OAuth + secure token storage working and
verified before any UI work. Once `get_playback_state` returns live data from my
running Spotify desktop app, move to Phase 2.