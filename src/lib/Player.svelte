<script lang="ts">
  import Hls from "hls.js";
  import { onDestroy, onMount } from "svelte";
  import { playlistUrl, log, type ChatPayload, type VideoInfo } from "./api";
  import { clock, store, stored } from "./format";

  interface Props {
    video: VideoInfo;
    chat: ChatPayload | null;
    currentTime: number;
    theater: boolean;
    chatOpen: boolean;
    /** Plein écran de toute la fenêtre (géré par App). */
    fullscreen: boolean;
    /** Titre et chaîne affichés en surimpression en mode cinéma / plein écran. */
    title?: string;
    channel?: string;
    /** Position de départ (reprise de lecture), en secondes. */
    startAt: number;
    /** Faut-il capturer une miniature pour la bibliothèque ? */
    needThumbnail: boolean;
    onProgress: (t: number) => void;
    onThumbnail: (jpegDataUrl: string) => void;
    onToggleFullscreen: () => void;
  }
  let {
    video, chat, currentTime = $bindable(0), theater = $bindable(false), chatOpen = $bindable(true),
    fullscreen, title, channel, startAt, needThumbnail, onProgress, onThumbnail, onToggleFullscreen,
  }: Props = $props();
  let immersive = $derived(theater || fullscreen);

  let el: HTMLVideoElement;
  let stage: HTMLDivElement;
  let hls: Hls | null = null;

  let paused = $state(true);
  let duration = $state(0);
  let buffered = $state(0);
  let volume = $state(stored("volume", 1));
  let muted = $state(stored("muted", false));
  let rate = $state(1);
  let controlsVisible = $state(true);
  let menuOpen = $state(false);
  let error = $state<string | null>(null);
  let hover = $state<{ x: number; t: number } | null>(null);
  let seeking = $state(false);
  let flash = $state<string | null>(null);

  const SPEEDS = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2];

  onMount(() => {
    if (import.meta.env.DEV) log(`lecteur : montage, MSE=${"MediaSource" in window} hls=${Hls.isSupported()} ` +
      `hlsNatif=${el.canPlayType("application/vnd.apple.mpegurl") || "non"} ` +
      `h264=${(window as any).MediaSource?.isTypeSupported?.('video/mp4; codecs="avc1.64002a,mp4a.40.2"')}`);
    duration = video.duration;
    el.volume = volume;
    el.muted = muted;
    if (Hls.isSupported()) {
      hls = new Hls({ enableWorker: true, maxBufferLength: 30, backBufferLength: 60, startPosition: startAt });
      hls.on(Hls.Events.ERROR, (_e, data) => {
        log(`hls ${data.fatal ? "FATAL" : "avertissement"} : ${data.type} / ${data.details}`);
        if (!data.fatal) return;
        if (data.type === Hls.ErrorTypes.MEDIA_ERROR) {
          hls?.recoverMediaError();
        } else {
          error = `Lecture impossible (${data.details}).`;
        }
      });
      if (import.meta.env.DEV) hls.on(Hls.Events.MANIFEST_PARSED, (_e, d) => log(`hls : playlist lue (${d.levels.length} niveau)`));
      if (import.meta.env.DEV) hls.on(Hls.Events.FRAG_BUFFERED, (_e, d) => d.frag.sn !== "initSegment" && +d.frag.sn < 2 && log(`hls : morceau ${d.frag.sn} prêt`));
      hls.loadSource(playlistUrl(video));
      hls.attachMedia(el);
    } else if (el.canPlayType("application/vnd.apple.mpegurl")) {
      el.src = playlistUrl(video);
      el.currentTime = startAt;
    } else {
      error = "Ce système ne sait pas lire les vidéos MPEG-TS (Media Source Extensions indisponible).";
    }
    el.play().catch((e) => log(`lecture auto refusée : ${e}`));
    if (startAt > 0) show(`Reprise à ${clock(startAt)}`, 1800);

    // Position enregistrée toutes les 5 s pendant la lecture (reprise au prochain lancement).
    const saver = setInterval(() => !el.paused && onProgress(el.currentTime), 5000);

    if (import.meta.env.DEV) {
      const t = setInterval(() => {
        log(`diag t=${el.currentTime.toFixed(2)} paused=${el.paused} ready=${el.readyState} ` +
          `size=${el.videoWidth}x${el.videoHeight} buffered=${buffered.toFixed(1)}`);
      }, 2000);
      return () => { clearInterval(t); clearInterval(saver); };
    }
    return () => clearInterval(saver);
  });

  onDestroy(() => {
    if (el) onProgress(el.currentTime);
    hls?.destroy();
  });

  /** Capture d'une image de la vidéo pour la miniature de la bibliothèque. */
  let thumbDone = false;
  function maybeThumbnail() {
    if (!needThumbnail || thumbDone || !el.videoWidth) return;
    if (el.currentTime < Math.min(10, duration * 0.4)) return;
    thumbDone = true;
    try {
      const c = document.createElement("canvas");
      c.width = 480;
      c.height = Math.round((480 * el.videoHeight) / el.videoWidth);
      c.getContext("2d")!.drawImage(el, 0, 0, c.width, c.height);
      onThumbnail(c.toDataURL("image/jpeg", 0.82));
    } catch (e) {
      log(`miniature impossible : ${e}`);
    }
  }

  // --- Commandes ---------------------------------------------------------
  function toggle() {
    if (el.paused) el.play().catch(() => {});
    else el.pause();
  }
  function seek(t: number) {
    el.currentTime = Math.min(Math.max(0, t), Math.max(0, duration - 0.1));
  }
  function skip(d: number) {
    seek(el.currentTime + d);
    show(`${d > 0 ? "+" : "−"}${Math.abs(d)} s`);
  }
  function setVolume(v: number) {
    volume = Math.min(1, Math.max(0, v));
    el.volume = volume;
    if (volume > 0 && muted) setMuted(false);
    store("volume", volume);
  }
  function setMuted(m: boolean) {
    muted = m;
    el.muted = m;
    store("muted", m);
  }
  function setRate(r: number) {
    rate = r;
    el.playbackRate = r;
    menuOpen = false;
  }
  // Plein écran de la fenêtre entière (géré par App via Tauri) : le chat reste visible.
  const toggleFullscreen = () => onToggleFullscreen();
  function show(text: string, ms = 600) {
    flash = text;
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => (flash = null), ms);
  }
  let flashTimer: ReturnType<typeof setTimeout>;

  // Masquage automatique des commandes pendant la lecture.
  let idleTimer: ReturnType<typeof setTimeout>;
  function wake() {
    controlsVisible = true;
    clearTimeout(idleTimer);
    idleTimer = setTimeout(() => {
      if (!el.paused && !menuOpen && !seeking) controlsVisible = false;
    }, 2500);
  }

  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (t.closest("input, textarea, [contenteditable]")) return;
    const k = e.key.toLowerCase();
    const actions: Record<string, () => void> = {
      " ": toggle, k: toggle,
      arrowleft: () => skip(-5), arrowright: () => skip(5),
      j: () => skip(-10), l: () => skip(10),
      arrowup: () => setVolume(volume + 0.05), arrowdown: () => setVolume(volume - 0.05),
      f: toggleFullscreen, m: () => setMuted(!muted),
      t: () => (theater = !theater), c: () => (chatOpen = !chatOpen),
    };
    const a = actions[k];
    if (!a || e.ctrlKey || e.altKey || e.metaKey) return;
    e.preventDefault();
    a();
    wake();
  }

  // --- Barre de progression ---------------------------------------------
  let track: HTMLDivElement;
  function timeAt(clientX: number) {
    const r = track.getBoundingClientRect();
    return (Math.min(Math.max(0, clientX - r.left), r.width) / r.width) * duration;
  }
  function onTrackDown(e: PointerEvent) {
    seeking = true;
    track.setPointerCapture(e.pointerId);
    seek(timeAt(e.clientX));
  }
  function onTrackMove(e: PointerEvent) {
    const r = track.getBoundingClientRect();
    hover = { x: Math.min(Math.max(0, e.clientX - r.left), r.width), t: timeAt(e.clientX) };
    if (seeking) seek(hover.t);
  }
  function onTrackUp() {
    seeking = false;
  }

  /** Activité du chat pendant la vidéo (messages par tranche), pour la barre. */
  const BUCKETS = 160;
  let activity = $derived.by(() => {
    if (!chat || duration <= 0) return null;
    const counts = new Array(BUCKETS).fill(0);
    const start = chat.startTime;
    for (const c of chat.comments) {
      const rel = c.t - start;
      if (rel < 0 || rel >= duration) continue;
      counts[Math.floor((rel / duration) * BUCKETS)]++;
    }
    const max = Math.max(...counts);
    if (max === 0) return null;
    let d = `M0 1`;
    counts.forEach((n, i) => (d += ` L${(i + 0.5) / BUCKETS} ${1 - n / max}`));
    return d + " L1 1 Z";
  });

  let vodStart = $derived(chat?.startTime ?? 0);
  let progress = $derived(duration > 0 ? (currentTime / duration) * 100 : 0);
  let bufferedPct = $derived(duration > 0 ? (buffered / duration) * 100 : 0);
</script>

<svelte:window onkeydown={onKey} />

<div
  class="stage"
  class:immersive
  class:idle={!controlsVisible}
  bind:this={stage}
  onpointermove={wake}
  onpointerleave={() => !el.paused && (controlsVisible = false)}
  role="region"
  aria-label="Lecteur vidéo"
>
  <!-- svelte-ignore a11y_media_has_caption -->
  <video
    bind:this={el}
    onclick={toggle}
    ondblclick={toggleFullscreen}
    ontimeupdate={() => {
      currentTime = el.currentTime;
      buffered = el.buffered.length ? el.buffered.end(el.buffered.length - 1) : 0;
      maybeThumbnail();
    }}
    onseeking={() => (currentTime = el.currentTime)}
    onplay={() => { paused = false; wake(); }}
    onpause={() => { paused = true; controlsVisible = true; onProgress(el.currentTime); }}
    ondurationchange={() => isFinite(el.duration) && el.duration > 0 && (duration = el.duration)}
    onprogress={() => (buffered = el.buffered.length ? el.buffered.end(el.buffered.length - 1) : 0)}
    onratechange={() => (rate = el.playbackRate)}
  ></video>

  {#if immersive && (title || channel)}
    <div class="titlebar" class:visible={controlsVisible || paused}>
      {#if channel}<span class="tb-channel">{channel}</span>{/if}
      {#if title}<span class="tb-title">{title}</span>{/if}
    </div>
  {/if}

  {#if error}
    <div class="overlay-msg"><strong>Oups.</strong> {error}</div>
  {/if}
  {#if flash}
    <div class="flash">{flash}</div>
  {/if}

  <div class="controls" class:visible={controlsVisible || paused}>
    <div
      class="track"
      bind:this={track}
      onpointerdown={onTrackDown}
      onpointermove={onTrackMove}
      onpointerup={onTrackUp}
      onpointerleave={() => (hover = null)}
      role="slider"
      aria-label="Position"
      aria-valuemin={0}
      aria-valuemax={duration}
      aria-valuenow={currentTime}
      tabindex="-1"
    >
      {#if activity}
        <svg class="activity" viewBox="0 0 1 1" preserveAspectRatio="none" aria-hidden="true">
          <path d={activity} />
        </svg>
      {/if}
      <div class="rail">
        <div class="buffered" style:width="{bufferedPct}%"></div>
        <div class="played" style:width="{progress}%"></div>
      </div>
      <div class="knob" style:left="{progress}%"></div>
      {#if hover}
        <div class="tip" style:left="{hover.x}px">
          {clock(hover.t)}
          {#if vodStart > 0}<span>VOD {clock(vodStart + hover.t)}</span>{/if}
        </div>
      {/if}
    </div>

    <div class="bar">
      <button class="icon-btn" onclick={toggle} title={paused ? "Lecture (k)" : "Pause (k)"}>
        {#if paused}
          <svg viewBox="0 0 20 20"><path fill="currentColor" d="M5 3.5v13l11-6.5z" /></svg>
        {:else}
          <svg viewBox="0 0 20 20"><path fill="currentColor" d="M5 3h3.5v14H5zm6.5 0H15v14h-3.5z" /></svg>
        {/if}
      </button>

      <div class="volume">
        <button class="icon-btn" onclick={() => setMuted(!muted)} title="Son (m)">
          {#if muted || volume === 0}
            <svg viewBox="0 0 20 20"><path fill="currentColor" d="M3 7.5h3l4-3.5v12l-4-3.5H3zm10.3.2 1.2-1.2 1.8 1.8 1.8-1.8 1.2 1.2-1.8 1.8 1.8 1.8-1.2 1.2-1.8-1.8-1.8 1.8-1.2-1.2 1.8-1.8z" /></svg>
          {:else}
            <svg viewBox="0 0 20 20"><path fill="currentColor" d="M3 7.5h3l4-3.5v12l-4-3.5H3zm9.5-1.3a5 5 0 0 1 0 7.6l-1-1.1a3.5 3.5 0 0 0 0-5.4zm2-2.1a8 8 0 0 1 0 11.8l-1-1.1a6.5 6.5 0 0 0 0-9.6z" /></svg>
          {/if}
        </button>
        <input
          type="range" min="0" max="1" step="0.01"
          value={muted ? 0 : volume}
          oninput={(e) => setVolume(+e.currentTarget.value)}
          style:--fill="{(muted ? 0 : volume) * 100}%"
          aria-label="Volume"
        />
      </div>

      <span class="time">{clock(currentTime)} <span class="dim">/ {clock(duration)}</span></span>

      <span class="spacer"></span>

      <div class="menu-wrap">
        <button class="icon-btn" onclick={() => (menuOpen = !menuOpen)} title="Vitesse de lecture">
          {#if rate !== 1}<span class="rate">{rate}×</span>{:else}
            <svg viewBox="0 0 20 20"><path fill="currentColor" d="M10 2a8 8 0 1 0 8 8h-2a6 6 0 1 1-6-6zm4.6 1.6-5.3 5.7a1 1 0 0 0 1.4 1.4l5.7-5.3z" /></svg>
          {/if}
        </button>
        {#if menuOpen}
          <div class="menu" role="menu">
            <div class="menu-title">Vitesse de lecture</div>
            {#each SPEEDS as s}
              <button class:active={s === rate} onclick={() => setRate(s)} role="menuitemradio" aria-checked={s === rate}>
                {s === 1 ? "Normale" : `${s}×`}
              </button>
            {/each}
          </div>
        {/if}
      </div>

      <button class="icon-btn" onclick={() => (chatOpen = !chatOpen)} title={chatOpen ? "Masquer le chat (c)" : "Afficher le chat (c)"}>
        <svg viewBox="0 0 20 20"><path fill="currentColor" d="M3 3h14v10h-7l-4 4v-4H3zm2 2v6h3v1.2L9.2 11H15V5z" /></svg>
      </button>
      <button class="icon-btn" onclick={() => (theater = !theater)} title="Mode cinéma (t)">
        <svg viewBox="0 0 20 20"><path fill="currentColor" d="M2 4h16v12H2zm2 2v8h9V6zm11 0v8h1V6z" /></svg>
      </button>
      <button class="icon-btn" onclick={toggleFullscreen} title="Plein écran (f)">
        {#if fullscreen}
          <svg viewBox="0 0 20 20"><path fill="currentColor" d="M7 3v4H3v2h6V3zm6 0v6h6V7h-4V3zM3 11v2h4v4h2v-6zm8 0v6h2v-4h4v-2z" /></svg>
        {:else}
          <svg viewBox="0 0 20 20"><path fill="currentColor" d="M3 3h6v2H5v4H3zm8 0h6v6h-2V5h-4zM3 11h2v4h4v2H3zm12 0h2v6h-6v-2h4z" /></svg>
        {/if}
      </button>
    </div>
  </div>
</div>

<style>
  .stage {
    position: relative;
    width: 100%;
    height: min(56.25cqw, calc(100vh - var(--topbar) - 130px));
    min-height: 240px;
    background: #000;
    user-select: none;
  }
  .stage.immersive { height: 100%; }

  .titlebar {
    position: absolute; top: 0; left: 0; right: 0; padding: 14px 18px 40px; display: flex; flex-direction: column; gap: 2px;
    background: linear-gradient(rgba(0, 0, 0, 0.75), transparent); opacity: 0; transition: opacity 0.2s; pointer-events: none;
  }
  .titlebar.visible { opacity: 1; }
  .tb-channel { font-weight: 700; font-size: 16px; }
  .tb-title { color: #dcdce2; font-size: 14px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .stage.idle { cursor: none; }
  video { width: 100%; height: 100%; display: block; object-fit: contain; background: #000; }

  .overlay-msg {
    position: absolute; inset: 0; display: grid; place-content: center; text-align: center;
    padding: 24px; background: rgba(0, 0, 0, 0.75); color: var(--text-dim); gap: 6px;
  }
  .overlay-msg strong { color: var(--text); font-size: 18px; }

  .flash {
    position: absolute; top: 50%; left: 50%; transform: translate(-50%, -50%);
    padding: 10px 16px; border-radius: 999px; background: rgba(0, 0, 0, 0.65);
    font-weight: 700; font-size: 16px; pointer-events: none;
  }

  .controls {
    position: absolute; left: 0; right: 0; bottom: 0; padding: 40px 10px 6px;
    background: linear-gradient(transparent, rgba(0, 0, 0, 0.8));
    opacity: 0; transition: opacity 0.2s; pointer-events: none;
  }
  .controls.visible { opacity: 1; pointer-events: auto; }

  .track { position: relative; height: 18px; display: flex; align-items: flex-end; cursor: pointer; touch-action: none; }
  .activity {
    position: absolute; left: 0; right: 0; bottom: 6px; width: 100%; height: 22px;
    fill: var(--accent); opacity: 0.28; pointer-events: none; transition: opacity 0.15s;
  }
  .track:hover .activity { opacity: 0.5; }
  .rail { position: relative; width: 100%; height: 4px; margin-bottom: 6px; background: rgba(255, 255, 255, 0.25); border-radius: 2px; overflow: hidden; transition: height 0.1s; }
  .track:hover .rail { height: 6px; margin-bottom: 5px; }
  .buffered { position: absolute; inset: 0 auto 0 0; background: rgba(255, 255, 255, 0.35); }
  .played { position: absolute; inset: 0 auto 0 0; background: var(--accent); }
  .knob {
    position: absolute; bottom: 3px; width: 12px; height: 12px; margin-left: -6px; border-radius: 50%;
    background: var(--accent); transform: scale(0); transition: transform 0.1s; pointer-events: none;
  }
  .track:hover .knob { transform: scale(1); }
  .tip {
    position: absolute; bottom: 24px; transform: translateX(-50%); padding: 4px 8px; border-radius: 4px;
    background: rgba(14, 14, 16, 0.92); font-size: 12px; font-weight: 600; white-space: nowrap; pointer-events: none;
    display: flex; flex-direction: column; align-items: center;
  }
  .tip span { color: var(--text-dim); font-weight: 400; font-size: 11px; }

  .bar { display: flex; align-items: center; gap: 2px; height: 36px; }
  .spacer { flex: 1; }
  .time { margin-left: 8px; font-variant-numeric: tabular-nums; font-size: 13px; }
  .dim { color: var(--text-dim); }
  .rate { font-weight: 700; font-size: 12px; }

  .volume { display: flex; align-items: center; }
  .volume input {
    width: 0; opacity: 0; transition: width 0.15s, opacity 0.15s; height: 4px; appearance: none;
    background: linear-gradient(to right, #fff var(--fill), rgba(255, 255, 255, 0.3) var(--fill)); border-radius: 2px;
  }
  .volume:hover input, .volume input:focus { width: 80px; opacity: 1; margin: 0 6px; }
  .volume input::-webkit-slider-thumb { appearance: none; width: 12px; height: 12px; border-radius: 50%; background: #fff; }

  .menu-wrap { position: relative; }
  .menu {
    position: absolute; right: 0; bottom: 40px; min-width: 170px; padding: 6px; border-radius: var(--radius);
    background: var(--bg-elev); border: 1px solid var(--border); box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
    display: flex; flex-direction: column;
  }
  .menu-title { padding: 6px 10px; font-size: 12px; font-weight: 600; color: var(--text-dim); text-transform: uppercase; }
  .menu button { text-align: left; padding: 7px 10px; border-radius: 4px; }
  .menu button:hover { background: var(--bg-hover); }
  .menu button.active { color: var(--accent); font-weight: 700; }
</style>
