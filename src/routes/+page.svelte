<script lang="ts">
  import "$lib/theme.css";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, onDestroy } from "svelte";

  /* ---------------------------------------------------------------
     PHASE 1 — auth gate. The webview never touches tokens; it only
     calls the Rust commands and reacts to their results.
     --------------------------------------------------------------- */
  type AuthState = "checking" | "signed-out" | "connecting" | "signed-in" | "error";
  let auth = $state<AuthState>("checking");
  let who = $state("");
  let errorMsg = $state("");

  onMount(async () => {
    tick = setInterval(() => (now = performance.now()), 250);
    try {
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

  async function poll() {
    try {
      const next = await invoke<Playback>("get_playback_state");
      syncedAt = performance.now();
      // On track change, re-derive the accent from the new art (Phase 3 hook).
      if (next.trackUri && next.trackUri !== prevUri) {
        prevUri = next.trackUri;
        applyAccent(next.art);
      }
      pb = next;
      errorMsg = "";
    } catch (e) {
      errorMsg = String(e);
    }
    clearTimeout(pollTimer);
    pollTimer = setTimeout(poll, pb.isPlaying ? 1000 : 4000);
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
  // Run a command, then re-poll shortly after (the API doesn't guarantee the
  // new state is visible immediately).
  async function cmd(fn: () => Promise<unknown>) {
    try {
      await fn();
    } catch (e) {
      errorMsg = String(e);
    }
    clearTimeout(pollTimer);
    pollTimer = setTimeout(poll, 250);
  }

  function togglePlay() {
    pb.isPlaying = !pb.isPlaying; // optimistic
    syncedAt = performance.now();
    cmd(() => invoke(pb.isPlaying ? "play" : "pause"));
  }
  const next = () => cmd(() => invoke("next"));
  const prev = () => cmd(() => invoke("previous"));
  const toggleShuffle = () => cmd(() => invoke("set_shuffle", { state: !pb.shuffle }));

  const REPEAT_CYCLE = ["off", "context", "track"] as const;
  function cycleRepeat() {
    const state = REPEAT_CYCLE[(REPEAT_CYCLE.indexOf(pb.repeat) + 1) % 3];
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
<main class="card" data-tauri-drag-region>
  <div class="bloom" style="--art: url({pb.art})"></div>

  <div class="art" class:empty={!pb.art}>
    {#if pb.art}<img src={pb.art} alt="" />{/if}
  </div>

  {#if pb.isActive}
  <section class="meta">
    <div class="lines">
      <h1 class="title">{pb.title}</h1>
      <p class="sub">{pb.artist}{pb.album ? ` · ${pb.album}` : ""}</p>
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

<button class="account" onclick={disconnect} title="Disconnect {who}">{who} ·&times;</button>

{:else}
<main class="gate" data-tauri-drag-region>
  <div class="bloom gate-bloom"></div>
  <div class="gate-inner">
    <span class="wordmark">lūme</span>

    {#if auth === "checking"}
      <span class="hint">…</span>
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
  .card {
    position: relative;
    display: flex;
    gap: 14px;
    align-items: center;
    height: 100vh;
    padding: 14px;
    border-radius: var(--radius);
    background: var(--bg);
    box-shadow: var(--card-shadow);
    overflow: hidden;
  }

  /* SIGNATURE: blurred album art bleeds behind the card as ambient light.
     The --bloom color tints the fallback so the glow reads even before art
     loads, and cross-fades with --accent on track change. */
  .bloom {
    position: absolute;
    inset: -40%;
    background: var(--art) center / cover no-repeat, var(--bloom);
    filter: blur(52px) saturate(1.5);
    opacity: 0.7;
    transition: opacity var(--accent-fade);
    pointer-events: none;
    z-index: 0;
  }
  .card > :not(.bloom) { position: relative; z-index: 1; }

  .art {
    width: 96px;
    height: 96px;
    flex: none;
    border-radius: var(--radius-sm);
    overflow: hidden;
    box-shadow: 0 8px 24px -8px var(--glow), 0 0 0 1px var(--hairline);
  }
  .art img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .art.empty {
    background: linear-gradient(135deg, var(--accent) 0%, transparent 70%), var(--surface-2);
  }

  .meta { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 9px; }
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
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .sub {
    margin: 0;
    font-size: 12px;
    color: var(--text-dim);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
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
    transition: background 140ms var(--ease), color 140ms var(--ease);
  }
  button:hover { background: var(--surface); }
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

  .account {
    position: fixed;
    top: 8px;
    right: 10px;
    z-index: 2;
    width: auto;
    height: auto;
    padding: 2px 7px;
    font-family: var(--font-mono);
    font-size: 9.5px;
    color: var(--text-faint);
    border-radius: 99px;
    max-width: 140px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .account:hover { background: var(--surface); color: var(--text-dim); }
</style>
