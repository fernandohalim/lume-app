<script lang="ts">
  import "$lib/theme.css";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, onDestroy } from "svelte";
  import { fade } from "svelte/transition";
  import {
    getCurrentWindow,
    currentMonitor,
    LogicalSize,
    LogicalPosition,
  } from "@tauri-apps/api/window";

  // Scroll (marquee) text back-and-forth only when it overflows its container.
  function marquee(node: HTMLElement, _text: string) {
    const parent = node.parentElement as HTMLElement;
    let raf = 0;
    function measure() {
      const overflow = node.scrollWidth - parent.clientWidth;
      if (overflow > 4) {
        node.style.setProperty("--mq-x", `-${overflow + 10}px`);
        node.style.setProperty("--mq-dur", `${Math.max(5, (overflow + 10) / 24)}s`);
        node.classList.add("mq-on");
      } else {
        node.classList.remove("mq-on");
      }
    }
    const ro = new ResizeObserver(() => measure());
    ro.observe(parent);
    raf = requestAnimationFrame(measure);
    return {
      update() {
        node.classList.remove("mq-on"); // restart for the new text
        cancelAnimationFrame(raf);
        raf = requestAnimationFrame(measure);
      },
      destroy() {
        ro.disconnect();
        cancelAnimationFrame(raf);
      },
    };
  }

  /* ---------------------------------------------------------------
     PHASE 1 — auth gate. The webview never touches tokens; it only
     calls the Rust commands and reacts to their results.
     --------------------------------------------------------------- */
  type AuthState =
    | "checking"
    | "unconfigured"
    | "signed-out"
    | "connecting"
    | "signed-in"
    | "error";
  let auth = $state<AuthState>("checking");
  let who = $state("");
  let errorMsg = $state("");

  onMount(async () => {
    tick = setInterval(() => (now = performance.now()), 250);
    // Capture DPI scale and force the compact height (window-state may have
    // restored a taller size from a previous session with the queue open).
    try {
      const w = getCurrentWindow();
      winScale = await w.scaleFactor();
      await resizeWindow(COMPACT_H);
    } catch {
      /* not in Tauri */
    }
    try {
      // Requires a lume.env with a Client ID next to the app.
      if (!(await invoke<boolean>("is_configured"))) {
        auth = "unconfigured";
        return;
      }
      if (await invoke<boolean>("is_authenticated")) await loadProfile();
      else auth = "signed-out";
    } catch {
      auth = "signed-out";
    }
  });

  onDestroy(() => {
    clearInterval(tick);
    clearTimeout(pollTimer);
  });

  async function loadProfile() {
    try {
      who = await invoke<string>("whoami");
      auth = "signed-in";
      poll(); // kick off the playback loop
    } catch (e) {
      errorMsg = String(e);
      auth = "error";
    }
  }

  async function connect() {
    auth = "connecting";
    errorMsg = "";
    try {
      await invoke("login");
      await loadProfile();
    } catch (e) {
      errorMsg = String(e);
      auth = "error";
    }
  }

  async function disconnect() {
    clearTimeout(pollTimer);
    await invoke("logout");
    who = "";
    auth = "signed-out";
  }

  /* ---------------------------------------------------------------
     PHASE 2 — live playback. Poll get_playback_state (~1s playing,
     slower when paused), interpolate progress locally between polls,
     and re-poll after every transport command.
     --------------------------------------------------------------- */
  type Playback = {
    isActive: boolean;
    isPlaying: boolean;
    progressMs: number;
    durationMs: number;
    title: string;
    artist: string;
    album: string;
    art: string;
    deviceName: string;
    volumePercent: number;
    shuffle: boolean;
    repeat: "off" | "context" | "track";
    trackUri: string;
  };

  const EMPTY: Playback = {
    isActive: false, isPlaying: false, progressMs: 0, durationMs: 0,
    title: "", artist: "", album: "", art: "", deviceName: "",
    volumePercent: 0, shuffle: false, repeat: "off", trackUri: "",
  };

  let pb = $state<Playback>({ ...EMPTY });
  let syncedAt = 0;            // performance.now() at last poll, for interpolation
  let now = $state(0);        // ticking clock that drives the progress bar
  let tick: ReturnType<typeof setInterval>;
  let pollTimer: ReturnType<typeof setTimeout>;
  let prevUri = "";
  let fastUntil = 0; // poll quickly until this time (post-command reconcile)
  let changing = $state(false); // a track change we requested is still settling
  let changingTarget = ""; // when set, stay "settling" until this URI actually lands
  let changeTimer: ReturnType<typeof setTimeout>;

  async function poll() {
    try {
      const next = await invoke<Playback>("get_playback_state");

      // Mid multi-skip: ignore the tracks we're passing through so the UI holds on
      // the destination (shown optimistically) instead of flashing #1, #2, #3…
      const passingThrough =
        changingTarget && next.trackUri && next.trackUri !== changingTarget && pb.isActive;

      if (!passingThrough) {
        syncedAt = performance.now();
        if (next.trackUri && next.trackUri !== prevUri) {
          prevUri = next.trackUri;
          applyAccent(next.art);
          // Keep the queue live whenever the track changes on screen.
          // (Lyrics refresh reactively — see the $effect below — so a skip or a
          //  natural song change updates them without reopening the panel.)
          if (panel === "queue") refreshQueue();
        }
        if (!changingTarget || next.trackUri === changingTarget) {
          changing = false;
          changingTarget = "";
        }
        pb = next;
      }
      errorMsg = "";
    } catch (e) {
      errorMsg = String(e);
    }
    clearTimeout(pollTimer);
    // Burst-poll while a command settles (or we're still skipping toward a target).
    const fast = performance.now() < fastUntil || changing;
    pollTimer = setTimeout(poll, fast ? 300 : pb.isPlaying ? 1000 : 4000);
  }

  // Local progress between polls so the bar moves smoothly at 1s poll cadence.
  const liveMs = $derived.by(() => {
    if (scrubbing) return scrubMs;
    if (!pb.isActive || !pb.isPlaying || pb.durationMs === 0) return pb.progressMs;
    return Math.min(pb.progressMs + (now - syncedAt), pb.durationMs);
  });
  const pct = $derived(pb.durationMs ? (liveMs / pb.durationMs) * 100 : 0);

  const fmt = (ms: number) =>
    `${Math.floor(ms / 60000)}:${String(Math.floor((ms % 60000) / 1000)).padStart(2, "0")}`;

  // ---- transport ----
  // After any command, poll fast for a short window so the real state appears
  // quickly instead of waiting for the 1s cadence.
  function reconcile() {
    fastUntil = performance.now() + 2500;
    clearTimeout(pollTimer);
    pollTimer = setTimeout(poll, 180);
  }

  async function cmd(fn: () => Promise<unknown>) {
    try {
      await fn();
    } catch (e) {
      errorMsg = String(e);
    }
    reconcile();
  }

  // Flag a requested track change as in-flight → the now-playing shows a
  // "settling" shimmer until the new track lands (or a safety timeout fires).
  function markChanging() {
    changing = true;
    clearTimeout(changeTimer);
    changeTimer = setTimeout(() => {
      changing = false;
      changingTarget = "";
    }, 5000);
  }

  function togglePlay() {
    pb.isPlaying = !pb.isPlaying; // optimistic
    syncedAt = performance.now();
    cmd(() => invoke(pb.isPlaying ? "play" : "pause"));
  }
  const next = () => {
    markChanging();
    cmd(() => invoke("next"));
  };

  // Standard player behavior: past the first ~3s, "previous" restarts the current
  // song; only near the very start does it go to the actual previous track.
  const PREV_RESTART_MS = 3000;
  function prev() {
    if (pb.isActive && liveMs > PREV_RESTART_MS) {
      pb.progressMs = 0; // optimistic restart
      syncedAt = performance.now();
      cmd(() => invoke("seek", { positionMs: 0 }));
    } else {
      markChanging();
      cmd(() => invoke("previous"));
    }
  }
  function toggleShuffle() {
    pb.shuffle = !pb.shuffle; // optimistic
    cmd(() => invoke("set_shuffle", { state: pb.shuffle }));
  }

  const REPEAT_CYCLE = ["off", "context", "track"] as const;
  function cycleRepeat() {
    const state = REPEAT_CYCLE[(REPEAT_CYCLE.indexOf(pb.repeat) + 1) % 3];
    pb.repeat = state; // optimistic
    cmd(() => invoke("set_repeat", { state }));
  }

  // ---- seek (click + drag) ----
  let scrubbing = $state(false);
  let scrubMs = $state(0);
  function railMs(e: PointerEvent, el: HTMLElement) {
    const r = el.getBoundingClientRect();
    const ratio = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
    return ratio * pb.durationMs;
  }
  function scrubDown(e: PointerEvent) {
    if (!pb.isActive) return;
    scrubbing = true;
    scrubMs = railMs(e, e.currentTarget as HTMLElement);
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function scrubMove(e: PointerEvent) {
    if (scrubbing) scrubMs = railMs(e, e.currentTarget as HTMLElement);
  }
  function scrubUp() {
    if (!scrubbing) return;
    const ms = Math.round(scrubMs);
    scrubbing = false;
    // Optimistic: hold the bar at the drop point so it doesn't snap back while
    // we wait for Spotify + the re-poll to catch up.
    pb.progressMs = ms;
    syncedAt = performance.now();
    cmd(() => invoke("seek", { positionMs: ms }));
  }

  // ---- volume ----
  let volDragging = false;
  let vol = $state(50);
  $effect(() => {
    if (!volDragging) vol = pb.volumePercent;
  });
  function volInput(e: Event) {
    volDragging = true;
    vol = +(e.target as HTMLInputElement).value;
  }
  function volCommit() {
    const percent = vol;
    volDragging = false;
    cmd(() => invoke("set_volume", { percent }));
  }

  /* ---------------------------------------------------------------
     PHASE 4 — queue + search. Expands the window into a panel with a
     read-only "up next" list and a track search that appends to the
     queue (the only queue mutation the Web API allows).
     --------------------------------------------------------------- */
  type TrackLite = {
    title: string;
    artist: string;
    art: string;
    uri: string;
    durationMs: number;
  };

  const COMPACT_H = 132;
  const DEFAULT_EXPANDED_H = 440;
  const MIN_EXPANDED_H = 300;
  const MAX_EXPANDED_H = 680;

  type Panel = "none" | "queue" | "settings" | "lyrics";
  let panel = $state<Panel>("none");
  const SETTINGS_H = 232;
  const LYRICS_H = 420;
  let expandedH = DEFAULT_EXPANDED_H; // session memory for how tall the queue is
  let winScale = 1;
  let queue = $state<{ now: TrackLite | null; upNext: TrackLite[] }>({ now: null, upNext: [] });
  let searchQuery = $state("");
  let searchResults = $state<TrackLite[]>([]);
  let searching = $state(false);
  let searchTimer: ReturnType<typeof setTimeout>;
  let added = $state<Record<string, boolean>>({}); // per-uri "just added" checkmark

  // We change height programmatically only (the window itself is non-resizable),
  // anchored top-left so growth extends downward — never leaving empty space.
  async function resizeWindow(height: number) {
    try {
      const win = getCurrentWindow();
      const logical = (await win.innerSize()).toLogical(winScale);
      await win.setSize(new LogicalSize(logical.width, height));
    } catch {
      /* not in Tauri (e.g. plain browser preview) */
    }
  }

  // Explicit width+height (the pill needs a narrower window than the player).
  async function resizeWindowWH(w: number, h: number) {
    try {
      await getCurrentWindow().setSize(new LogicalSize(w, h));
    } catch {
      /* not in Tauri */
    }
  }

  /* ---------------------------------------------------------------
     PHASE 6 — window controls + mini-pill. Frameless, so we provide
     our own minimize / close-to-tray / minify affordances. Minify
     shrinks the whole window into a small draggable pill that snaps
     to the nearest screen corner; clicking it restores the player.
     --------------------------------------------------------------- */
  const NORMAL_W = 380;
  const PILL_W = 216;
  const PILL_H = 52;
  const PILL_MARGIN = 16; // gap from the screen edge when snapped

  let minified = $state(false);
  let preMinify: { x: number; y: number } | null = null; // player spot to return to

  async function minimizeWin() {
    try {
      await getCurrentWindow().minimize();
    } catch {
      /* not in Tauri */
    }
  }

  // "Close" doesn't quit — it hides to the tray (the Rust side keeps running).
  async function hideToTray() {
    try {
      await getCurrentWindow().hide();
    } catch {
      /* not in Tauri */
    }
  }

  async function minify() {
    try {
      const win = getCurrentWindow();
      const pos = (await win.outerPosition()).toLogical(winScale);
      preMinify = { x: pos.x, y: pos.y };
    } catch {
      preMinify = null;
    }
    panel = "none"; // pill has no room for panels
    minified = true;
    await resizeWindowWH(PILL_W, PILL_H);
    await snapToNearestCorner();
  }

  async function restorePlayer() {
    minified = false;
    await resizeWindowWH(NORMAL_W, COMPACT_H);
    if (preMinify) {
      try {
        await getCurrentWindow().setPosition(new LogicalPosition(preMinify.x, preMinify.y));
      } catch {
        /* not in Tauri */
      }
    }
  }

  // Pull the pill to whichever screen corner its center is closest to.
  async function snapToNearestCorner() {
    try {
      const win = getCurrentWindow();
      const mon = await currentMonitor();
      if (!mon) return;
      const s = mon.scaleFactor;
      const monX = mon.position.x / s;
      const monY = mon.position.y / s;
      const monW = mon.size.width / s;
      const monH = mon.size.height / s;
      const M = PILL_MARGIN;
      const right = monX + monW - PILL_W - M;
      const bottom = monY + monH - PILL_H - M;
      const corners = [
        { x: monX + M, y: monY + M }, // top-left
        { x: right, y: monY + M }, // top-right
        { x: monX + M, y: bottom }, // bottom-left
        { x: right, y: bottom }, // bottom-right
      ];
      const cur = (await win.outerPosition()).toLogical(winScale);
      const cx = cur.x + PILL_W / 2;
      const cy = cur.y + PILL_H / 2;
      let best = corners[2]; // default bottom-left
      let bestD = Infinity;
      for (const c of corners) {
        const d = (c.x + PILL_W / 2 - cx) ** 2 + (c.y + PILL_H / 2 - cy) ** 2;
        if (d < bestD) {
          bestD = d;
          best = c;
        }
      }
      await win.setPosition(new LogicalPosition(best.x, best.y));
    } catch {
      /* not in Tauri */
    }
  }

  // Pill drag: move the OS window while dragging; a click (no movement) restores.
  let pillDrag = false;
  let pillMoved = false;
  let pStartX = 0;
  let pStartY = 0;
  let pWinX = 0;
  let pWinY = 0;
  async function pillDown(e: PointerEvent) {
    if (e.button !== 0) return;
    pillDrag = true;
    pillMoved = false;
    pStartX = e.screenX;
    pStartY = e.screenY;
    try {
      const pos = (await getCurrentWindow().outerPosition()).toLogical(winScale);
      pWinX = pos.x;
      pWinY = pos.y;
    } catch {
      /* not in Tauri */
    }
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function pillMove(e: PointerEvent) {
    if (!pillDrag) return;
    const dx = e.screenX - pStartX;
    const dy = e.screenY - pStartY;
    if (!pillMoved && Math.hypot(dx, dy) > 4) pillMoved = true;
    if (pillMoved) {
      getCurrentWindow().setPosition(new LogicalPosition(pWinX + dx, pWinY + dy));
    }
  }
  async function pillUp() {
    if (!pillDrag) return;
    pillDrag = false;
    if (pillMoved) await snapToNearestCorner();
    else await restorePlayer(); // a click (not a drag) opens the full player
  }

  async function setPanel(mode: Panel) {
    panel = panel === mode ? "none" : mode; // clicking the active one closes it
    const h =
      panel === "queue" ? expandedH
      : panel === "settings" ? SETTINGS_H
      : panel === "lyrics" ? LYRICS_H
      : COMPACT_H;
    await resizeWindow(h);
    if (panel === "queue") refreshQueue();
    if (panel === "none") clearSearch();
  }
  const toggleQueue = () => setPanel("queue");
  const toggleSettings = () => setPanel("settings");
  const toggleLyrics = () => setPanel("lyrics");

  function clearSearch() {
    searchQuery = "";
    searchResults = [];
  }

  // Bottom grip (only shown while the queue is open): drag to set queue height.
  let gripping = false;
  let gripStartY = 0;
  let gripStartH = 0;
  function gripDown(e: PointerEvent) {
    gripping = true;
    gripStartY = e.screenY;
    gripStartH = expandedH;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function gripMove(e: PointerEvent) {
    if (!gripping) return;
    const delta = (e.screenY - gripStartY) / winScale;
    expandedH = Math.max(MIN_EXPANDED_H, Math.min(MAX_EXPANDED_H, gripStartH + delta));
    resizeWindow(expandedH);
  }
  function gripUp() {
    gripping = false;
  }

  async function refreshQueue() {
    try {
      queue = await invoke("get_queue");
    } catch (e) {
      errorMsg = String(e);
    }
  }

  function onSearchInput() {
    clearTimeout(searchTimer);
    if (!searchQuery.trim()) {
      searchResults = [];
      return;
    }
    searchTimer = setTimeout(async () => {
      searching = true;
      try {
        searchResults = await invoke<TrackLite[]>("search", { query: searchQuery });
      } catch (e) {
        errorMsg = String(e);
      }
      searching = false;
    }, 350);
  }

  async function addToQueue(uri: string) {
    added[uri] = true; // instant ✓ feedback so it's clear it worked
    try {
      await invoke("add_to_queue", { uri });
    } catch (e) {
      errorMsg = String(e);
      delete added[uri];
      return;
    }
    setTimeout(refreshQueue, 400);
    // Revert to "+" after a moment so an intentional second add is still possible.
    setTimeout(() => delete added[uri], 1600);
  }

  // Jump to a queued track by skipping forward to it (index 0 = next song = 1 skip).
  let skippingIndex = $state<number | null>(null);
  async function skipTo(index: number) {
    const target = queue.upNext[index];
    skippingIndex = index;
    changingTarget = target?.uri ?? "";
    markChanging();
    if (target) {
      // Show the destination immediately (we already have its metadata) and hold
      // on it while we skip forward — no flashing through the in-between tracks.
      pb = {
        ...pb,
        title: target.title,
        artist: target.artist,
        album: "",
        art: target.art,
        durationMs: target.durationMs,
        progressMs: 0,
      };
      syncedAt = performance.now();
      applyAccent(target.art);
    }
    try {
      await invoke("skip_forward", { steps: index + 1 });
    } catch (e) {
      errorMsg = String(e);
    }
    skippingIndex = null;
    reconcile();
  }

  /* ---------------------------------------------------------------
     PHASE 5 — synced lyrics (LRCLIB via Rust). Fetch once per track,
     highlight the current line from liveMs (interpolated progress) and
     auto-scroll it to center. Synced = karaoke; unsynced = plain text.
     --------------------------------------------------------------- */
  type LyricLine = { timeMs: number; text: string };
  type Lyrics = { synced: boolean; instrumental: boolean; lines: LyricLine[] };

  let lyrics = $state<Lyrics>({ synced: false, instrumental: false, lines: [] });
  let lyricsLoading = $state(false);
  let lyricsUri = ""; // the track these lyrics belong to (avoids refetch each poll)
  let lyricsEl = $state<HTMLElement | null>(null);
  const prefersReduced =
    typeof window !== "undefined" &&
    !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

  async function loadLyrics() {
    const uri = pb.trackUri;
    if (!uri || uri === lyricsUri) return; // already have (or are getting) these
    lyricsUri = uri;
    lyricsLoading = true;
    lyrics = { synced: false, instrumental: false, lines: [] };
    try {
      const result = await invoke<Lyrics>("get_lyrics", {
        title: pb.title,
        artist: pb.artist,
        album: pb.album,
        durationMs: pb.durationMs,
      });
      if (pb.trackUri !== uri) return; // a newer track change superseded this fetch
      lyrics = result;
    } catch (e) {
      if (pb.trackUri !== uri) return; // stale failure — ignore
      errorMsg = String(e);
      lyricsUri = ""; // let a retry happen on the next track-change / reopen
    }
    lyricsLoading = false;
  }

  // Auto-load lyrics whenever the panel is open and the track changes — opening
  // the panel, skipping, or a song ending into the next all trigger this. The
  // per-track guard in loadLyrics() makes repeat polls of the same track a no-op.
  $effect(() => {
    if (panel === "lyrics" && pb.trackUri) loadLyrics();
  });

  // Index of the line that should be lit right now (synced only): the last line
  // whose timestamp we've passed. lyrics.lines arrive sorted ascending from Rust.
  const activeLine = $derived.by(() => {
    if (!lyrics.synced) return -1;
    const t = liveMs;
    let idx = -1;
    for (let i = 0; i < lyrics.lines.length; i++) {
      if (lyrics.lines[i].timeMs <= t) idx = i;
      else break;
    }
    return idx;
  });

  // Keep the lit line centered as the song plays.
  $effect(() => {
    const idx = activeLine;
    if (idx < 0 || panel !== "lyrics" || !lyricsEl) return;
    const el = lyricsEl.querySelector<HTMLElement>(`[data-i="${idx}"]`);
    el?.scrollIntoView({ block: "center", behavior: prefersReduced ? "auto" : "smooth" });
  });

  // Tap a synced line to jump to its moment (nice-to-have from the roadmap).
  function seekToLine(line: LyricLine) {
    if (!lyrics.synced || line.timeMs < 0 || !pb.isActive) return;
    pb.progressMs = line.timeMs; // optimistic
    syncedAt = performance.now();
    cmd(() => invoke("seek", { positionMs: line.timeMs }));
  }

  // ---- accent from art: the player glows the color of what's playing ----
  async function applyAccent(art: string) {
    try {
      const hex = await invoke<string>("get_accent", { artUrl: art });
      // Setting --accent on <html> triggers the 380ms cross-fade in theme.css.
      document.documentElement.style.setProperty("--accent", hex);
    } catch {
      /* keep the previous accent on failure */
    }
  }
</script>

{#if auth === "signed-in"}
{#if minified}
<!-- Mini-pill: art + title + play/pause. Drag to move (snaps to the nearest
     corner); a plain click on the body restores the full player. -->
<div
  class="pill"
  onpointerdown={pillDown}
  onpointermove={pillMove}
  onpointerup={pillUp}
  role="button"
  tabindex="0"
  title="Click to expand · drag to move"
>
  <div class="pill-bloom" style="--art: url({pb.art})"></div>
  <!-- Spinning vinyl: the cover becomes a record that turns while playing. -->
  <div class="pill-art" class:empty={!pb.art} class:paused={!pb.isPlaying}>
    {#if pb.art}<img src={pb.art} alt="" />{/if}
  </div>
  {#if pb.isActive}
    <div class="pill-info">
      <span class="pill-title"><span class="scroll" use:marquee={pb.title}>{pb.title}</span></span>
      <span class="pill-artist"><span class="scroll" use:marquee={pb.artist}>{pb.artist}</span></span>
    </div>
    <button
      class="pill-play"
      onpointerdown={(e) => e.stopPropagation()}
      onclick={togglePlay}
      aria-label={pb.isPlaying ? "Pause" : "Play"}
    >{pb.isPlaying ? "❙❙" : "▶"}</button>
  {:else}
    <span class="pill-idle">lūme</span>
  {/if}
</div>
{:else}
<div class="app" class:expanded={panel !== "none"}>
<button class="gear" class:on={panel === "settings"} onclick={toggleSettings} aria-label="Settings">⚙</button>
<!-- Window controls (frameless → our own): minimize, minify to pill, close to tray.
     Hover-revealed so the resting bar stays art-first. -->
<div class="winctl">
  <button class="wc" onclick={minimizeWin} aria-label="Minimize" title="Minimize">
    <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor"
      stroke-width="1.6" stroke-linecap="round"><line x1="4" y1="8.5" x2="12" y2="8.5" /></svg>
  </button>
  <button class="wc" onclick={minify} aria-label="Minify to pill" title="Minify to a corner pill">
    <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor"
      stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <rect x="8.5" y="8.5" width="5.5" height="4" rx="1.3" />
      <path d="M3.5 4.5 L7 8" /><path d="M3.5 7 V4 H6.5" />
    </svg>
  </button>
  <button class="wc close" onclick={hideToTray} aria-label="Close to tray" title="Close to tray">
    <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor"
      stroke-width="1.6" stroke-linecap="round"><path d="M4 4 L12 12 M12 4 L4 12" /></svg>
  </button>
</div>
<!-- Drag hint: a slim top strip that grows on hover to invite moving the window. -->
<div class="drag-handle" data-tauri-drag-region><span class="grip-pill"></span></div>
<main class="card" class:loading={changing} data-tauri-drag-region>
  <div class="bloom" style="--art: url({pb.art})"></div>
  <!-- Keeps the glow over the art but darkens under the text so bright covers
       stay readable. -->
  <div class="scrim"></div>

  <div class="art" class:empty={!pb.art}>
    {#if pb.art}<img src={pb.art} alt="" />{/if}
    <!-- Lyrics affordance lives on the cover: a mic pill bottom-left that grows
         to reveal "Lyrics" on hover, and stays lit/expanded while the panel is open. -->
    {#if pb.isActive}
    <button
      class="lyr-toggle"
      class:on={panel === "lyrics"}
      onclick={toggleLyrics}
      aria-label="Lyrics"
      title="Lyrics"
    >
      <span class="lyr-ico" aria-hidden="true">
        <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor"
          stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <rect x="9" y="2.5" width="6" height="11" rx="3" />
          <path d="M6 11a6 6 0 0 0 12 0" />
          <line x1="12" y1="17" x2="12" y2="21" />
          <line x1="9" y1="21" x2="15" y2="21" />
        </svg>
      </span>
      <span class="lyr-label">Lyrics</span>
    </button>
    {/if}
  </div>

  {#if pb.isActive}
  <section class="meta">
    <div class="lines">
      <h1 class="title"><span class="scroll" use:marquee={pb.title}>{pb.title}</span></h1>
      <p class="sub">
        <span class="scroll" use:marquee={pb.artist + pb.album}
          >{pb.artist}{pb.album ? ` · ${pb.album}` : ""}</span
        >
      </p>
    </div>

    <div class="scrub">
      <div
        class="rail"
        onpointerdown={scrubDown}
        onpointermove={scrubMove}
        onpointerup={scrubUp}
        role="slider"
        aria-label="Seek"
        aria-valuemin={0}
        aria-valuemax={pb.durationMs}
        aria-valuenow={Math.round(liveMs)}
        tabindex="0"
      >
        <div class="fill" class:scrubbing style="width:{pct}%"></div>
      </div>
      <div class="times">
        <span>{fmt(liveMs)}</span>
        <span class="device" title={pb.deviceName}>{pb.deviceName}</span>
        <span class="dim">{fmt(pb.durationMs)}</span>
      </div>
    </div>

    <div class="controls">
      <button onclick={prev} aria-label="Previous">&lsaquo;&lsaquo;</button>
      <button class="play" onclick={togglePlay} aria-label={pb.isPlaying ? "Pause" : "Play"}>
        {pb.isPlaying ? "❙❙" : "▶"}
      </button>
      <button onclick={next} aria-label="Next">&rsaquo;&rsaquo;</button>

      <input
        class="vol"
        type="range"
        min="0"
        max="100"
        value={vol}
        oninput={volInput}
        onchange={volCommit}
        aria-label="Volume"
      />

      <button class="ghost" class:on={pb.shuffle} onclick={toggleShuffle} aria-label="Shuffle">⤮</button>
      <button class="ghost" class:on={pb.repeat !== "off"} onclick={cycleRepeat} aria-label="Repeat">
        {pb.repeat === "track" ? "↻¹" : "↻"}
      </button>
      <button class="ghost" class:on={panel === "queue"} onclick={toggleQueue} aria-label="Queue &amp; search">☰</button>
    </div>
  </section>
  {:else}
  <section class="meta nodevice">
    <h1 class="title">Nothing playing</h1>
    <p class="sub">Open Spotify and hit play — lūme will pick it up.</p>
    <button class="retry" onclick={poll}>Refresh</button>
  </section>
  {/if}
</main>

{#if panel !== "none"}
<section class="panel">
  {#if panel === "queue"}
  <div class="searchbar">
    <input
      class="sinput"
      type="text"
      placeholder="Search tracks to add to queue…"
      bind:value={searchQuery}
      oninput={onSearchInput}
    />
    {#if searchQuery}
      <button class="clear" onclick={clearSearch} aria-label="Clear search">&times;</button>
    {/if}
  </div>

  {#if searchQuery.trim()}
    <ul class="list">
      {#if searching && !searchResults.length}
        <li class="empty">Searching…</li>
      {:else if !searchResults.length}
        <li class="empty">No results.</li>
      {/if}
      {#each searchResults as t (t.uri)}
        <li class="row">
          <div class="thumb" class:empty={!t.art}>{#if t.art}<img src={t.art} alt="" />{/if}</div>
          <div class="tinfo">
            <span class="tt">{t.title}</span>
            <span class="ta">{t.artist}</span>
          </div>
          <button
            class="add"
            class:added={added[t.uri]}
            onclick={() => addToQueue(t.uri)}
            aria-label={added[t.uri] ? "Added to queue" : "Add to queue"}
          >{added[t.uri] ? "✓" : "+"}</button>
        </li>
      {/each}
    </ul>
  {:else}
    <div class="qhead">Up next <span class="note">· tap to play · reorder/remove aren't supported by Spotify</span></div>
    <ul class="list">
      {#if !queue.upNext.length}
        <li class="empty">Queue is empty.</li>
      {/if}
      {#each queue.upNext as t, i (t.uri + i)}
        <li transition:fade={{ duration: 180 }}>
          <button
            class="row playable rowbtn"
            class:busy={skippingIndex === i}
            onclick={() => skipTo(i)}
            title="Skip to this song"
          >
            <div class="thumb" class:empty={!t.art}>
              {#if t.art}<img src={t.art} alt="" />{/if}
              <span class="playmark">
                {#if skippingIndex === i}<span class="spin"></span>{:else}⏭{/if}
              </span>
            </div>
            <div class="tinfo">
              <span class="tt">{t.title}</span>
              <span class="ta">{t.artist}</span>
            </div>
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  <!-- Bottom grip: the only resize affordance, and only while the queue is open. -->
  <div
    class="grip"
    onpointerdown={gripDown}
    onpointermove={gripMove}
    onpointerup={gripUp}
    role="separator"
    aria-label="Resize queue"
    aria-orientation="horizontal"
    title="Drag to resize the queue"
  ></div>
  {:else if panel === "settings"}
  <div class="settings">
    <!-- Account card: a soft accent bloom keeps the "art is the light source"
         motif without a literal avatar. -->
    <div class="idcard">
      <div class="idglow"></div>
      <div class="idinfo">
        <span class="idlabel">Signed in via Spotify</span>
        <span class="idname" title={who}>{who || "—"}</span>
      </div>
      <button class="logout" onclick={disconnect} title="Log out of Spotify">
        <span>Log out</span>
      </button>
    </div>

    <!-- One-line wordmark footer: ● Connected · lūme · v1.0 -->
    <div class="brand">
      <span class="dot" aria-hidden="true"></span>
      <span>Connected</span>
      <span class="sep" aria-hidden="true">·</span>
      <span class="brand-mark">lūme</span>
      <span class="sep" aria-hidden="true">·</span>
      <span class="brand-ver">v1.0</span>
    </div>
  </div>
  {:else if panel === "lyrics"}
  <div class="lyrics" class:synced={lyrics.synced} bind:this={lyricsEl}>
    {#if !pb.isActive}
      <div class="lyr-state">Nothing playing.</div>
    {:else if lyricsLoading && !lyrics.lines.length}
      <div class="lyr-state"><span class="lyr-dots">Finding lyrics…</span></div>
    {:else if lyrics.instrumental}
      <div class="lyr-state">♪ Instrumental</div>
    {:else if !lyrics.lines.length}
      <div class="lyr-state">No lyrics found for this track.</div>
    {:else if lyrics.synced}
      {#each lyrics.lines as line, i (i)}
        <button
          class="lyr-line"
          class:active={i === activeLine}
          class:past={i < activeLine}
          data-i={i}
          onclick={() => seekToLine(line)}
          title="Jump to this line"
        >{line.text || "♪"}</button>
      {/each}
    {:else}
      {#each lyrics.lines as line, i (i)}
        <p class="lyr-line plain" data-i={i}>{line.text || " "}</p>
      {/each}
    {/if}
  </div>
  {/if}
</section>
{/if}
</div>
{/if}

{:else}
<main class="gate" data-tauri-drag-region>
  <div class="bloom gate-bloom"></div>
  <div class="gate-inner">
    <span class="wordmark">lūme</span>

    {#if auth === "checking"}
      <span class="hint">…</span>
    {:else if auth === "unconfigured"}
      <span class="hint setup">
        Add your Spotify Client ID to <code>lume.env</code> next to the app
        (copy <code>lume.env.example</code>), then restart.
      </span>
    {:else if auth === "connecting"}
      <span class="hint">Waiting for Spotify… finish in your browser.</span>
    {:else}
      <button class="connect" onclick={connect}>Connect Spotify</button>
      {#if auth === "error"}<span class="err">{errorMsg}</span>{/if}
    {/if}
  </div>
</main>
{/if}

<style>
  /* Glass shell holds the now-playing card and, when expanded, the queue panel. */
  .app {
    position: relative;
    height: 100vh;
    display: flex;
    flex-direction: column;
    border-radius: var(--radius);
    background: var(--bg);
    box-shadow: var(--card-shadow);
    overflow: hidden;
    /* clip-path enforces the rounded corners at the compositor level. The
       album-art bloom (filter: blur) renders into a separate composited texture
       that ignores border-radius/overflow on macOS WKWebView — bleeding an
       opaque rectangle into the corners (visible as black corner triangles,
       worst with dark/monochrome covers). clip-path DOES clip composited
       descendants, so it contains the bloom. */
    clip-path: inset(0 round var(--radius));
  }

  /* Drag hint: a slim strip pinned to the top that grows + reveals a grip pill
     on hover, so it's obvious you can grab here to move the window. */
  .drag-handle {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 7px;
    z-index: 6;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: grab;
    transition: height 160ms var(--ease), background 160ms var(--ease);
  }
  .drag-handle:active { cursor: grabbing; }
  .app:hover .drag-handle {
    height: 15px;
    background: linear-gradient(var(--surface), transparent);
  }
  .grip-pill {
    width: 30px;
    height: 3px;
    border-radius: 99px;
    background: var(--text-faint);
    opacity: 0;
    transform: translateY(-2px);
    transition: opacity 160ms var(--ease), transform 160ms var(--ease);
  }
  .app:hover .grip-pill { opacity: 1; transform: translateY(0); }

  .card {
    position: relative;
    display: flex;
    gap: 14px;
    align-items: center;
    flex: none;
    height: 132px;
    padding: 14px;
    overflow: hidden; /* clips the bloom to the now-playing row */
  }

  /* SIGNATURE: blurred album art bleeds behind the card as ambient light.
     The --bloom color tints the fallback so the glow reads even before art
     loads, and cross-fades with --accent on track change. */
  .bloom {
    position: absolute;
    inset: -40%;
    background: var(--art) center / cover no-repeat, var(--bloom);
    /* brightness cap keeps very bright covers from blowing out the backdrop */
    filter: blur(52px) saturate(1.5) brightness(0.82);
    opacity: 0.64;
    transition: opacity var(--accent-fade);
    pointer-events: none;
    z-index: 0;
  }
  /* Contrast scrim: transparent over the art (left), darkening toward the text
     (right) so the title/artist stay legible regardless of cover brightness. */
  .scrim {
    position: absolute;
    inset: 0;
    z-index: 0;
    pointer-events: none;
    background: linear-gradient(
      90deg,
      transparent 0%,
      transparent 28%,
      color-mix(in srgb, var(--bg) 72%, transparent) 68%,
      color-mix(in srgb, var(--bg) 82%, transparent) 100%
    );
  }
  .card > :not(.bloom):not(.scrim) { position: relative; z-index: 1; }

  .art {
    position: relative; /* anchors the lyrics toggle in the bottom-left corner */
    width: 96px;
    height: 96px;
    flex: none;
    border-radius: var(--radius-sm);
    overflow: hidden;
    box-shadow: 0 8px 24px -8px var(--glow), 0 0 0 1px var(--hairline);
    transition: opacity 240ms var(--ease), transform 240ms var(--ease);
  }
  .art img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .art.empty {
    background: linear-gradient(135deg, var(--accent) 0%, transparent 70%), var(--surface-2);
  }

  .meta { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 9px; }
  /* keep the title clear of the top-right gear */
  .lines { padding-right: 22px; position: relative; overflow: hidden; }

  /* "Settling" state while a requested track change lands: the art eases back a
     touch, the text softens, and a light band sweeps across — smooth in and out,
     much calmer than a blink. */
  .card.loading .art { opacity: 0.6; transform: scale(0.96); }
  .card.loading .title,
  .card.loading .sub { opacity: 0.5; }
  .card.loading .lines::after {
    content: "";
    position: absolute;
    inset: 0;
    background: linear-gradient(
      100deg,
      transparent 20%,
      color-mix(in srgb, var(--text) 12%, transparent) 50%,
      transparent 80%
    );
    background-size: 220% 100%;
    animation: sweep 1.05s linear infinite;
    pointer-events: none;
  }
  @keyframes sweep {
    from { background-position: 220% 0; }
    to { background-position: -120% 0; }
  }
  .nodevice { justify-content: center; gap: 4px; }
  .nodevice .retry {
    align-self: flex-start;
    width: auto; height: auto;
    margin-top: 8px;
    padding: 5px 12px;
    font-size: 11px; font-weight: 600;
    background: var(--surface-2); color: var(--text);
    border-radius: 99px;
  }
  .nodevice .retry:hover { background: var(--surface); }

  .title {
    margin: 0;
    font-size: 17px;
    font-weight: 700;
    letter-spacing: -0.01em;
    white-space: nowrap; overflow: hidden;
    transition: opacity 220ms var(--ease);
  }
  .sub {
    margin: 0;
    font-size: 12px;
    color: var(--text-dim);
    white-space: nowrap; overflow: hidden;
    transition: opacity 220ms var(--ease);
  }
  /* marquee: only animates when the text overflows (see the marquee action) */
  .title > .scroll, .sub > .scroll {
    display: inline-block;
    white-space: nowrap;
    max-width: 100%;
  }
  /* .mq-on is toggled at runtime by the marquee action → mark it :global so
     Svelte doesn't prune the rule as "unused". */
  .scroll:global(.mq-on) {
    max-width: none;
    will-change: transform;
    animation: marquee var(--mq-dur, 8s) ease-in-out infinite alternate;
  }
  @keyframes marquee {
    0%, 12% { transform: translateX(0); }
    88%, 100% { transform: translateX(var(--mq-x, 0)); }
  }

  .rail {
    height: 4px; border-radius: 99px; background: var(--surface-2);
    overflow: hidden; cursor: pointer; touch-action: none;
  }
  .rail:hover { height: 6px; margin: -1px 0; }
  .fill { height: 100%; background: var(--accent); transition: width 240ms linear; }
  .fill.scrubbing { transition: none; }
  .times {
    display: flex; justify-content: space-between; align-items: center;
    gap: 8px;
    margin-top: 5px;
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--text-dim);
  }
  .times .device {
    flex: 1; min-width: 0; text-align: center;
    color: var(--text-faint);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .times .dim { color: var(--text-faint); }

  /* Art-first: transport recedes at rest and lifts in on hover. The play
     button keeps its accent fill so it stays discoverable. */
  .controls {
    display: flex; align-items: center; gap: 6px;
    opacity: 0.55;
    transition: opacity 200ms var(--ease);
  }
  .card:hover .controls,
  .controls:focus-within { opacity: 1; }

  button {
    appearance: none; border: none; cursor: pointer;
    background: transparent; color: var(--text);
    font-size: 13px; line-height: 1;
    width: 30px; height: 30px; border-radius: 8px;
    transition: background 140ms var(--ease), color 140ms var(--ease),
      transform 90ms var(--ease);
  }
  button:hover { background: var(--surface); }
  button:active { transform: scale(0.92); } /* instant tactile acknowledgement */
  button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .ghost { color: var(--text-dim); width: 26px; height: 26px; font-size: 12px; }
  .ghost.on { color: var(--accent); }
  .play {
    background: var(--accent);
    color: #0b0b0f;
    box-shadow: 0 4px 16px -4px var(--glow);
  }
  .play:hover { filter: brightness(1.08); background: var(--accent); }

  /* compact volume slider */
  .vol {
    flex: 1; min-width: 40px; max-width: 80px;
    height: 4px; margin: 0 4px;
    appearance: none; -webkit-appearance: none;
    background: var(--surface-2); border-radius: 99px; cursor: pointer;
  }
  .vol::-webkit-slider-thumb {
    -webkit-appearance: none; appearance: none;
    width: 11px; height: 11px; border-radius: 50%;
    background: var(--accent); box-shadow: 0 0 8px -1px var(--glow);
  }
  .vol::-moz-range-thumb {
    width: 11px; height: 11px; border: none; border-radius: 50%;
    background: var(--accent);
  }

  /* ---- Phase 4 queue + search panel ---- */
  .panel {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--hairline);
    padding: 10px 12px 12px;
    overflow: hidden;
  }
  .searchbar { flex: none; margin-bottom: 8px; position: relative; }
  .sinput {
    width: 100%;
    padding: 7px 30px 7px 11px;
    font-family: var(--font-display);
    font-size: 12px;
    color: var(--text);
    background: var(--surface-2);
    border: 1px solid transparent;
    border-radius: 99px;
    outline: none;
  }
  .sinput::placeholder { color: var(--text-faint); }
  .sinput:focus { border-color: var(--accent); background: var(--surface); }
  .clear {
    position: absolute;
    top: 50%;
    right: 6px;
    transform: translateY(-50%);
    width: 20px;
    height: 20px;
    font-size: 14px;
    line-height: 1;
    color: var(--text-dim);
    background: var(--surface-2);
    border-radius: 50%;
  }
  .clear:hover { background: var(--surface); color: var(--text); }
  .clear:active { transform: translateY(-50%) scale(0.9); }

  .qhead {
    flex: none;
    font-size: 10.5px;
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    margin: 2px 2px 6px;
  }
  .qhead .note {
    text-transform: none;
    letter-spacing: 0;
    color: var(--text-faint);
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0 4px 0 0;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
    scrollbar-width: thin; /* Firefox */
    scrollbar-color: var(--surface-2) transparent;
  }
  /* Slim, glassy scrollbar (WebKit/Chromium — the Tauri webview) */
  .list::-webkit-scrollbar { width: 8px; }
  .list::-webkit-scrollbar-track { background: transparent; }
  .list::-webkit-scrollbar-thumb {
    background: var(--surface-2);
    border-radius: 99px;
    border: 2px solid transparent;
    background-clip: padding-box;
  }
  .list::-webkit-scrollbar-thumb:hover {
    background: var(--text-faint);
    background-clip: padding-box;
  }
  .list .empty {
    color: var(--text-faint);
    font-size: 11.5px;
    padding: 10px 2px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 5px 2px;
    border-radius: 8px;
  }
  .row:hover { background: var(--surface); }
  .row.playable { cursor: pointer; }
  /* queue rows are <button>s — reset the compact global button sizing */
  .rowbtn {
    width: 100%;
    height: auto;
    text-align: left;
    font: inherit;
    color: inherit;
  }
  .thumb {
    position: relative;
    width: 34px;
    height: 34px;
    flex: none;
    border-radius: 5px;
    overflow: hidden;
    box-shadow: 0 0 0 1px var(--hairline);
  }
  .thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .thumb.empty { background: var(--surface-2); }
  /* play overlay revealed when hovering a queue row */
  .playmark {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    font-size: 11px;
    color: #fff;
    background: rgba(0, 0, 0, 0.5);
    opacity: 0;
    transition: opacity 140ms var(--ease);
  }
  .row.playable:hover .playmark { opacity: 1; }
  .row.busy .playmark { opacity: 1; }
  .spin {
    width: 12px;
    height: 12px;
    border: 2px solid rgba(255, 255, 255, 0.35);
    border-top-color: #fff;
    border-radius: 50%;
    animation: rot 0.6s linear infinite;
  }
  @keyframes rot { to { transform: rotate(360deg); } }
  .tinfo { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .tt {
    font-size: 12px;
    color: var(--text);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .ta {
    font-size: 10.5px;
    color: var(--text-dim);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .add {
    flex: none;
    width: 26px; height: 26px;
    font-size: 16px; font-weight: 600;
    color: var(--accent);
    border-radius: 50%;
  }
  .add:hover { background: var(--accent); color: #0b0b0f; }
  .add.added {
    color: #0b0b0f;
    background: #57d97f; /* green confirmation, distinct from the accent */
  }

  /* Bottom resize grip — the sole resize affordance, queue-open only. */
  .grip {
    flex: none;
    height: 12px;
    margin: 4px -12px -12px; /* span full width, sit at the very bottom */
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: ns-resize;
    touch-action: none;
  }
  .grip::before {
    content: "";
    width: 34px;
    height: 3px;
    border-radius: 99px;
    background: var(--text-faint);
    opacity: 0.5;
    transition: opacity 140ms var(--ease);
  }
  .grip:hover::before { opacity: 1; }

  /* ---- Phase 1 auth gate ---- */
  .gate {
    position: relative;
    height: 100vh;
    border-radius: var(--radius);
    background: var(--bg);
    box-shadow: var(--card-shadow);
    overflow: hidden;
    display: grid;
    place-items: center;
    /* Same macOS fix as .app: clip-path contains the blurred gate-bloom. */
    clip-path: inset(0 round var(--radius));
  }
  .gate-bloom {
    inset: -40%;
    background: var(--accent-soft);
    filter: blur(60px) saturate(1.4);
    opacity: 0.5;
  }
  .gate-inner {
    position: relative;
    z-index: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    text-align: center;
    padding: 0 16px;
  }
  .wordmark {
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.02em;
    line-height: 1;
  }
  .hint { font-size: 11px; color: var(--text-dim); }
  .hint.setup { max-width: 300px; line-height: 1.5; }
  .hint.setup code {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text);
    background: var(--surface-2);
    padding: 1px 4px;
    border-radius: 4px;
  }
  .err {
    font-family: var(--font-mono);
    font-size: 9.5px;
    color: #ff8585;
    max-width: 320px;
    max-height: 28px;
    overflow: hidden;
  }
  .connect {
    width: auto;
    height: auto;
    padding: 7px 16px;
    font-size: 12.5px;
    font-weight: 600;
    background: var(--accent);
    color: #0b0b0f;
    border-radius: 99px;
    box-shadow: 0 6px 20px -6px var(--glow);
  }
  .connect:hover { filter: brightness(1.08); background: var(--accent); }

  /* Settings entry: a small gear pinned top-right (replaces the old name chip
     that collided with the title). */
  .gear {
    position: absolute;
    top: 7px;
    right: 8px;
    z-index: 7;
    width: 22px;
    height: 22px;
    font-size: 12px;
    color: var(--text-faint);
    border-radius: 50%;
  }
  .gear:hover { background: var(--surface); color: var(--text-dim); }
  .gear.on { color: var(--accent); }

  /* Window controls (minimize / minify / close-to-tray): a hover-revealed
     cluster left of the gear so the resting bar stays clean and art-first. */
  /* Hover-revealed control group. It sits over the title, so it rides on a
     blurred glass pill (matched by the gear) to stay legible over any text. */
  .winctl {
    position: absolute;
    top: 5px;
    right: 33px;
    z-index: 7;
    display: flex;
    gap: 1px;
    padding: 1px;
    border-radius: 99px;
    background: color-mix(in srgb, var(--bg) 78%, transparent);
    -webkit-backdrop-filter: blur(10px);
    backdrop-filter: blur(10px);
    box-shadow: 0 2px 10px -4px rgba(0, 0, 0, 0.6), 0 0 0 1px var(--hairline);
    opacity: 0;
    transform: translateY(-2px);
    pointer-events: none;
    transition: opacity 160ms var(--ease), transform 160ms var(--ease);
  }
  .app:hover .winctl,
  .winctl:focus-within {
    opacity: 1;
    transform: translateY(0);
    pointer-events: auto;
  }
  .winctl .wc {
    width: 22px;
    height: 22px;
    color: var(--text-dim);
    border-radius: 99px;
  }
  .winctl .wc svg { display: block; }
  .winctl .wc:hover { background: var(--surface); color: var(--text); }
  .winctl .close:hover {
    color: #ff8585;
    background: color-mix(in srgb, #ff8585 16%, transparent);
  }
  /* Match the gear to the control group's glass while the bar is hovered. */
  .app:hover .gear {
    background: color-mix(in srgb, var(--bg) 78%, transparent);
    -webkit-backdrop-filter: blur(10px);
    backdrop-filter: blur(10px);
    box-shadow: 0 2px 10px -4px rgba(0, 0, 0, 0.6), 0 0 0 1px var(--hairline);
    color: var(--text-dim);
  }
  .app:hover .gear:hover { color: var(--text); }
  .app:hover .gear.on { color: var(--accent); }

  /* ---- Phase 6 mini-pill ---- */
  .pill {
    position: relative;
    height: 100vh;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 0 10px 0 7px;
    border-radius: 26px;
    background: var(--bg);
    box-shadow: var(--card-shadow);
    overflow: hidden;
    cursor: grab;
    user-select: none;
    /* Same macOS fix as .app: clip-path contains the blurred pill-bloom. */
    clip-path: inset(0 round 26px);
  }
  .pill:active { cursor: grabbing; }
  /* Same light-source motif, scaled down: art blooms behind the pill. */
  .pill-bloom {
    position: absolute;
    inset: -60%;
    background: var(--art) center / cover no-repeat, var(--bloom);
    filter: blur(34px) saturate(1.5) brightness(0.8);
    opacity: 0.55;
    transition: opacity var(--accent-fade);
    pointer-events: none;
    z-index: 0;
  }
  .pill > :not(.pill-bloom) { position: relative; z-index: 1; }
  .pill-art {
    position: relative;
    width: 38px;
    height: 38px;
    flex: none;
    border-radius: 50%; /* record disc */
    overflow: hidden;
    box-shadow: 0 4px 12px -4px var(--glow), 0 0 0 1px var(--hairline);
    animation: vinyl 7s linear infinite;
  }
  .pill-art.paused { animation-play-state: paused; } /* holds angle when not playing */
  .pill-art img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .pill-art.empty {
    background: linear-gradient(135deg, var(--accent) 0%, transparent 70%), var(--surface-2);
  }
  /* Spindle hole at the center to sell the record look. */
  .pill-art::after {
    content: "";
    position: absolute;
    top: 50%;
    left: 50%;
    width: 8px;
    height: 8px;
    transform: translate(-50%, -50%);
    border-radius: 50%;
    background: var(--bg);
    box-shadow: 0 0 0 1.5px rgba(0, 0, 0, 0.45), inset 0 0 0 1px rgba(255, 255, 255, 0.08);
  }
  @keyframes vinyl {
    to { transform: rotate(360deg); }
  }
  @media (prefers-reduced-motion: reduce) {
    .pill-art { animation: none; }
  }
  .pill-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; overflow: hidden; }
  .pill-title {
    font-size: 12.5px;
    font-weight: 700;
    letter-spacing: -0.01em;
    white-space: nowrap;
    overflow: hidden;
  }
  .pill-artist {
    font-size: 10.5px;
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
  }
  .pill-title > .scroll, .pill-artist > .scroll {
    display: inline-block;
    white-space: nowrap;
    max-width: 100%;
  }
  .pill-idle {
    flex: 1;
    font-size: 13px;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: var(--text-dim);
  }
  .pill-play {
    flex: none;
    width: 28px;
    height: 28px;
    font-size: 11px;
    background: var(--accent);
    color: #0b0b0f;
    border-radius: 50%;
    box-shadow: 0 3px 12px -3px var(--glow);
  }
  .pill-play:hover { filter: brightness(1.08); background: var(--accent); }

  /* Settings panel — fills the panel so branding can pin to the bottom. */
  .settings { flex: 1; min-height: 0; display: flex; flex-direction: column; padding: 2px; }

  /* Account card: a single glass tile holding the account line + logout,
     collapsing the old stacked rows into one compact unit. */
  .idcard {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    border-radius: var(--radius-sm);
    background: var(--surface);
    box-shadow: 0 0 0 1px var(--hairline) inset;
    overflow: hidden;
  }
  /* faint accent bloom bleeding from the left edge — the light source motif */
  .idglow {
    position: absolute;
    top: -40%;
    left: -10%;
    width: 55%;
    height: 180%;
    background: radial-gradient(closest-side, var(--accent-soft), transparent 70%);
    filter: blur(14px);
    opacity: 0.9;
    pointer-events: none;
    transition: background var(--accent-fade);
  }
  .idcard > :not(.idglow) { position: relative; z-index: 1; }

  .idinfo { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .idlabel {
    font-size: 9.5px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-dim);
  }
  .idname {
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  /* Compact, balanced logout pill (icon + label, even padding both sides). */
  .logout {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    width: auto;
    height: auto;
    padding: 8px 14px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-dim);
    background: var(--surface-2);
    border-radius: 99px;
    transition: color 140ms var(--ease), background 140ms var(--ease),
      transform 90ms var(--ease);
  }
  .logout .logout-ico { font-size: 12.5px; line-height: 1; }
  .logout:hover {
    color: #ff8585;
    background: color-mix(in srgb, #ff8585 14%, transparent);
  }
  .logout:active { transform: scale(0.96); }

  /* One-line footer pinned to the bottom: ● Connected · lūme · v0.1.0 */
  .brand {
    margin-top: auto;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    padding-top: 12px;
    font-size: 11px;
    color: var(--text-faint);
  }
  .brand .dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: #57d97f;
    box-shadow: 0 0 6px -1px #57d97f;
  }
  .brand .sep { color: var(--text-faint); opacity: 0.55; }
  .brand-mark {
    font-size: 12.5px;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: var(--text-dim);
  }
  .brand-ver {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
  }

  /* ---- Phase 5 lyrics panel ---- */
  /* Lyrics toggle on the cover: collapsed to a mic disc, expands to a labeled
     pill on cover hover, and stays lit + open while the panel is active. */
  .lyr-toggle {
    position: absolute;
    left: 7px;
    bottom: 7px;
    z-index: 2;
    display: inline-flex;
    align-items: center;
    width: auto;
    height: 24px;
    max-width: 24px;
    padding: 0;
    border-radius: 99px;
    color: #fff;
    background: rgba(8, 8, 12, 0.55);
    -webkit-backdrop-filter: blur(8px);
    backdrop-filter: blur(8px);
    box-shadow: 0 2px 8px -2px rgba(0, 0, 0, 0.55);
    overflow: hidden;
    white-space: nowrap;
    transition: max-width 240ms var(--ease), background 160ms var(--ease),
      color 160ms var(--ease);
  }
  .lyr-ico {
    flex: none;
    width: 24px;
    height: 24px;
    display: grid;
    place-items: center;
  }
  .lyr-ico svg { display: block; }
  .lyr-label {
    font-size: 11px;
    font-weight: 600;
    padding-right: 10px;
    opacity: 0;
    transition: opacity 200ms var(--ease);
  }
  /* reveal on cover hover, keyboard focus, or while the panel is open */
  .art:hover .lyr-toggle,
  .lyr-toggle.on,
  .lyr-toggle:focus-visible { max-width: 110px; }
  .art:hover .lyr-label,
  .lyr-toggle.on .lyr-label,
  .lyr-toggle:focus-visible .lyr-label { opacity: 1; }
  .lyr-toggle:hover { background: rgba(8, 8, 12, 0.78); }
  .lyr-toggle.on { background: var(--accent); color: #0b0b0f; }
  .lyr-toggle.on:hover { background: var(--accent); filter: brightness(1.08); }

  .lyrics {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 6px 6px 40px; /* bottom room so the last line can scroll to center */
    scrollbar-width: thin; /* Firefox */
    scrollbar-color: var(--surface-2) transparent;
  }
  .lyrics::-webkit-scrollbar { width: 8px; }
  .lyrics::-webkit-scrollbar-track { background: transparent; }
  .lyrics::-webkit-scrollbar-thumb {
    background: var(--surface-2);
    border-radius: 99px;
    border: 2px solid transparent;
    background-clip: padding-box;
  }
  .lyrics::-webkit-scrollbar-thumb:hover {
    background: var(--text-faint);
    background-clip: padding-box;
  }
  /* Synced (karaoke): centered, big, with a lit current line. */
  .lyrics.synced { text-align: center; padding-top: 28px; }

  .lyr-line {
    /* reset the compact global button sizing for these full-width text rows */
    width: 100%;
    height: auto;
    display: block;
    text-align: inherit;
    font: inherit;
    padding: 5px 8px;
    border-radius: 8px;
    font-size: 15px;
    font-weight: 600;
    line-height: 1.32;
    color: var(--text-faint);
    background: transparent;
    transition: color 260ms var(--ease), opacity 260ms var(--ease),
      transform 260ms var(--ease);
  }
  /* synced-line specifics: past lines recede, the active line glows the accent */
  .lyrics.synced .lyr-line { cursor: pointer; }
  .lyrics.synced .lyr-line:hover { color: var(--text-dim); background: var(--surface); }
  .lyr-line.past { color: var(--text-faint); opacity: 0.5; }
  .lyr-line.active {
    color: var(--accent);
    opacity: 1;
    transform: scale(1.04);
    text-shadow: 0 0 18px var(--glow);
  }
  .lyrics.synced .lyr-line.active:hover { background: transparent; }

  /* Plain (unsynced): quiet left-aligned reading column, no interaction. */
  .lyr-line.plain {
    margin: 0;
    font-weight: 500;
    font-size: 13.5px;
    color: var(--text-dim);
    cursor: default;
    min-height: 0.6em; /* keep blank separator lines from collapsing */
  }

  .lyr-state {
    height: 100%;
    display: grid;
    place-items: center;
    text-align: center;
    padding: 0 16px;
    font-size: 12px;
    color: var(--text-faint);
  }
  .lyr-dots { animation: pulse 1.4s var(--ease) infinite; }
  @keyframes pulse { 50% { opacity: 0.4; } }
</style>
