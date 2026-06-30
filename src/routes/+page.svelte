<script lang="ts">
  import "$lib/theme.css";
  // import { invoke } from "@tauri-apps/api/core";
  // import { getCurrentWindow } from "@tauri-apps/api/window";

  /* ---------------------------------------------------------------
     PLACEHOLDER STATE — replace with live data from the Rust side.
     The Rust core should expose `get_playback_state` and transport
     commands; poll ~1s while playing (see ROADMAP.md, Phase 2).
     --------------------------------------------------------------- */
  let track = $state({
    title: "Nightshift",
    artist: "Lucy Dacus",
    album: "Home Video",
    art: "", // cover URL from Spotify; empty = fallback gradient
    durationMs: 379000,
    progressMs: 84000,
    isPlaying: true,
  });

  const fmt = (ms: number) =>
    `${Math.floor(ms / 60000)}:${String(Math.floor((ms % 60000) / 1000)).padStart(2, "0")}`;
  const pct = $derived((track.progressMs / track.durationMs) * 100);

  // TODO: extract a dominant color from `track.art` on load and set
  // document.documentElement.style.setProperty('--accent', color).
  // That single line is the whole identity — the player glows the music.

  // TODO wire to Rust commands:
  // const playPause = () => invoke(track.isPlaying ? "pause" : "play");
  // const next = () => invoke("next");
  // const prev = () => invoke("previous");
  // const seek = (ms: number) => invoke("seek", { positionMs: ms });
</script>

<main class="card" data-tauri-drag-region>
  <div class="bloom" style="--art: url({track.art})"></div>

  <div class="art" class:empty={!track.art}>
    {#if track.art}<img src={track.art} alt="" />{/if}
  </div>

  <section class="meta">
    <div class="lines">
      <h1 class="title">{track.title}</h1>
      <p class="sub">{track.artist} · {track.album}</p>
    </div>

    <div class="scrub">
      <div class="rail"><div class="fill" style="width:{pct}%"></div></div>
      <div class="times">
        <span>{fmt(track.progressMs)}</span>
        <span class="dim">{fmt(track.durationMs)}</span>
      </div>
    </div>

    <div class="controls">
      <button aria-label="Previous">&lsaquo;&lsaquo;</button>
      <button class="play" aria-label={track.isPlaying ? "Pause" : "Play"}>
        {track.isPlaying ? "❙❙" : "▶"}
      </button>
      <button aria-label="Next">&rsaquo;&rsaquo;</button>
      <span class="spacer"></span>
      <button class="ghost" aria-label="Shuffle">⤮</button>
      <button class="ghost" aria-label="Repeat">↻</button>
    </div>
  </section>
</main>

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

  /* SIGNATURE: blurred album art bleeds behind the card as ambient light */
  .bloom {
    position: absolute;
    inset: -40%;
    background: var(--art) center / cover no-repeat, var(--accent-soft);
    filter: blur(48px) saturate(1.4);
    opacity: 0.55;
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

  .rail { height: 4px; border-radius: 99px; background: var(--surface-2); overflow: hidden; }
  .fill { height: 100%; background: var(--accent); transition: width 240ms linear; }
  .times {
    display: flex; justify-content: space-between;
    margin-top: 5px;
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--text-dim);
  }
  .times .dim { color: var(--text-faint); }

  .controls { display: flex; align-items: center; gap: 6px; }
  .spacer { flex: 1; }

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
  .play {
    background: var(--accent);
    color: #0b0b0f;
    box-shadow: 0 4px 16px -4px var(--glow);
  }
  .play:hover { filter: brightness(1.08); background: var(--accent); }
</style>
