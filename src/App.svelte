<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import Player from "./lib/Player.svelte";
  import Chat from "./lib/Chat.svelte";
  import { loadChat, log, openVideo, startupFile, type ChatPayload, type ChatRefusal, type VideoInfo } from "./lib/api";
  import { clock, longDate } from "./lib/format";

  let video = $state<VideoInfo | null>(null);
  let chat = $state<ChatPayload | null>(null);
  let refusal = $state<ChatRefusal | null>(null);
  let opening = $state(false);
  let chatLoading = $state(false);
  let openError = $state<string | null>(null);
  let currentTime = $state(0);
  let theater = $state(false);
  let chatOpen = $state(true);
  let offset = $state(0);
  let dragging = $state(false);
  let playerKey = $state(0);
  let player: Player | undefined = $state();

  /** Moment courant dans la VOD, vu par le chat (décalage appliqué). */
  let vodTime = $derived((chat?.startTime ?? 0) + currentTime - offset);

  async function openPath(path: string) {
    opening = true;
    openError = null;
    try {
      const v = await openVideo(path);
      video = v;
      chat = null;
      refusal = null;
      currentTime = 0;
      offset = 0;
      playerKey++;
      if (v.chat?.status === "loaded") chat = v.chat;
      else if (v.chat?.status === "refused") refusal = v.chat;
      log(`vidéo ouverte : ${v.fileName} (${v.duration.toFixed(1)} s), chat : ${v.chat?.status ?? "aucun"}`);
    } catch (e) {
      openError = String(e);
      log(`ouverture impossible : ${e}`);
    } finally {
      opening = false;
    }
  }

  async function pickVideo() {
    const path = await open({ multiple: false, filters: [{ name: "Vidéo Xtra (.ts)", extensions: ["ts"] }] });
    if (typeof path === "string") openPath(path);
  }

  async function chatFromPath(path: string) {
    if (!video) return;
    chatLoading = true;
    try {
      const r = await loadChat(video.id, path);
      // Refusé : on garde le chat actuel s'il y en a un, et on affiche l'erreur.
      if (r.status === "loaded") {
        chat = r;
        refusal = null;
      } else {
        refusal = r;
        chat = null;
      }
    } catch (e) {
      log(`chargement du chat : ${e}`);
    } finally {
      chatLoading = false;
    }
  }

  async function pickChat() {
    const path = await open({ multiple: false, filters: [{ name: "Chat Xtra (.json)", extensions: ["json"] }] });
    if (typeof path === "string") chatFromPath(path);
  }

  function seekVod(t: number) {
    if (!chat) return;
    const target = t - chat.startTime + offset;
    const el = document.querySelector("video");
    if (el) el.currentTime = Math.max(0, target);
  }

  onMount(() => {
    startupFile().then((p) => { if (p) openPath(p); });
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((e) => {
        const p = e.payload;
        if (p.type === "enter" || p.type === "over") dragging = true;
        else if (p.type === "leave") dragging = false;
        else if (p.type === "drop") {
          dragging = false;
          const ts = p.paths.find((x) => x.toLowerCase().endsWith(".ts"));
          const json = p.paths.find((x) => x.toLowerCase().endsWith(".json"));
          if (ts) openPath(ts);
          else if (json && video) chatFromPath(json);
        }
      })
      .then((u) => (unlisten = u));
    return () => unlisten?.();
  });

  let meta = $derived(chat?.video ?? null);
  let channel = $derived(meta?.channelName || meta?.channelLogin || "");
</script>

<div class="app" class:theater>
  {#if !theater}
    <header class="topbar">
      <div class="brand">
        <img src="/icon.png" alt="" />
        <span>Tsuki</span>
      </div>
      <span class="spacer"></span>
      <button class="btn" onclick={pickVideo} disabled={opening}>
        <svg viewBox="0 0 20 20" width="16" height="16"><path fill="currentColor" d="M2 4h6l2 2h8v10H2zm2 4v6h12V8z" /></svg>
        Ouvrir une vidéo
      </button>
    </header>
  {/if}

  <main>
    {#if video}
      <section class="player-col">
        {#key playerKey}
          <Player bind:this={player} {video} {chat} bind:currentTime bind:theater bind:chatOpen />
        {/key}
        {#if !theater}
          <div class="info">
            <div class="avatar" aria-hidden="true">{(channel || video.fileName)[0]?.toUpperCase()}</div>
            <div class="details">
              {#if meta}
                <div class="channel">{channel}</div>
                <h1>{meta.title}</h1>
                <div class="tags">
                  {#if meta.gameName}<span class="game">{meta.gameName}</span>{/if}
                  {#if meta.uploadDate}<span>Diffusé le {longDate(meta.uploadDate)}</span>{/if}
                  {#if chat && chat.startTime > 0}
                    <span>Passage {clock(chat.startTime)} → {clock(chat.startTime + video.duration)} de la VOD</span>
                  {/if}
                  {#if chat}<span>{chat.comments.length.toLocaleString("fr-FR")} messages</span>{/if}
                </div>
              {:else}
                <h1>{video.fileName}</h1>
                <div class="tags"><span>{clock(video.duration)}</span><span>Chat non chargé</span></div>
              {/if}
            </div>
          </div>
        {/if}
      </section>

      {#if chatOpen}
        <Chat
          videoId={video.id}
          {chat}
          {refusal}
          {vodTime}
          {offset}
          loading={chatLoading}
          onPick={pickChat}
          onSeekVod={seekVod}
          onOffset={(o) => (offset = Math.round(o * 10) / 10)}
          onClose={() => (chatOpen = false)}
        />
      {/if}
    {:else}
      <div class="home">
        <img class="moon" src="/icon.png" alt="" />
        <h1>Aucune rediffusion pour l'instant</h1>
        <p>Ouvre une vidéo <code>.ts</code> téléchargée avec Xtra, ou glisse-la dans cette fenêtre.<br />
          Son chat <code>_chat.json</code> sera chargé automatiquement s'il est dans le même dossier.</p>
        <button class="btn primary" onclick={pickVideo} disabled={opening}>
          {opening ? "Analyse de la vidéo…" : "Ouvrir une vidéo"}
        </button>
        {#if openError}<p class="error" role="alert">{openError}</p>{/if}
      </div>
    {/if}
  </main>

  {#if dragging}
    <div class="drop">Dépose une vidéo <code>.ts</code>{video ? " ou un chat .json" : ""}</div>
  {/if}
</div>

<style>
  .app { display: flex; flex-direction: column; height: 100%; }
  .topbar {
    display: flex; align-items: center; gap: 12px; height: var(--topbar); padding: 0 12px; flex-shrink: 0;
    background: var(--bg-alt); border-bottom: 1px solid var(--border); box-shadow: 0 1px 2px rgba(0, 0, 0, 0.4); z-index: 2;
  }
  .brand { display: flex; align-items: center; gap: 8px; font-weight: 800; font-size: 17px; letter-spacing: 0.01em; }
  .brand img { width: 28px; height: 28px; }
  .spacer { flex: 1; }

  main { flex: 1; display: flex; min-height: 0; }
  .player-col { flex: 1; min-width: 0; overflow-y: auto; container-type: inline-size; }
  .theater .player-col { overflow: hidden; }

  .info { display: flex; gap: 14px; padding: 16px 20px 24px; }
  .avatar {
    width: 64px; height: 64px; flex-shrink: 0; border-radius: 50%; display: grid; place-items: center;
    font-size: 26px; font-weight: 800; color: #0e0e10; background: linear-gradient(135deg, #d9d4ff, var(--accent));
  }
  .details { min-width: 0; }
  .channel { font-size: 18px; font-weight: 700; }
  h1 { margin: 4px 0 8px; font-size: 15px; font-weight: 600; }
  .tags { display: flex; flex-wrap: wrap; gap: 6px 14px; color: var(--text-dim); font-size: 13px; }
  .game { color: var(--accent); font-weight: 600; }

  .home { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; padding: 24px; text-align: center; }
  .home .moon { width: 96px; height: 96px; opacity: 0.9; }
  .home h1 { font-size: 22px; margin: 8px 0 0; }
  .home p { color: var(--text-dim); max-width: 460px; line-height: 1.6; margin: 0 0 8px; }
  code { background: var(--bg-hover); padding: 1px 5px; border-radius: 4px; font-size: 0.92em; }
  .error { color: var(--danger); }

  .drop {
    position: fixed; inset: 12px; display: grid; place-items: center; border-radius: 12px; z-index: 50;
    border: 2px dashed var(--accent); background: rgba(14, 14, 16, 0.85); font-size: 18px; font-weight: 700; pointer-events: none;
  }
</style>
