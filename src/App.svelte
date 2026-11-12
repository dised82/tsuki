<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Player from "./lib/Player.svelte";
  import Chat from "./lib/Chat.svelte";
  import Library from "./lib/Library.svelte";
  import {
    libraryAdd, libraryList, libraryRelocate, libraryRemove, loadChat, log, openEntry, openVideo,
    saveThumbnail, scanFolder, setProgress, startupFiles,
    type ChatPayload, type ChatRefusal, type LibraryItem, type VideoInfo,
  } from "./lib/api";
  import { clock, longDate } from "./lib/format";
  import { onThumbnailSaved, queueThumbnails } from "./lib/thumbnails";

  // --- Bibliothèque -------------------------------------------------------
  let items = $state<LibraryItem[]>([]);
  let busy = $state<string | null>(null);
  let addErrors = $state<{ path: string; message: string }[]>([]);

  async function refresh() {
    try {
      items = await libraryList();
      // Miniatures manquantes (vidéos jamais lues), capturées en arrière-plan.
      queueThumbnails(items.filter((i) => i.exists && !i.hasThumbnail).map((i) => i.id));
    } catch (e) {
      log(`bibliothèque : ${e}`);
    }
  }
  let thumbRefresh: ReturnType<typeof setTimeout> | undefined;
  onThumbnailSaved(() => {
    clearTimeout(thumbRefresh);
    thumbRefresh = setTimeout(async () => (items = await libraryList().catch(() => items)), 400);
  });

  /** Ajoute plusieurs vidéos à la liste, sans les lire. */
  async function addPaths(paths: string[]) {
    const ts = paths.filter((p) => p.toLowerCase().endsWith(".ts"));
    if (ts.length === 0) return;
    busy = ts.length > 1 ? `Analyse de ${ts.length} vidéos…` : "Analyse de la vidéo…";
    try {
      const r = await libraryAdd(ts);
      addErrors = r.errors;
    } catch (e) {
      addErrors = [{ path: ts.join(", "), message: String(e) }];
    } finally {
      busy = null;
      refresh();
    }
  }

  async function pickFolder() {
    const dir = await open({ directory: true, multiple: false, title: "Dossier de téléchargements Xtra" });
    if (typeof dir !== "string") return;
    busy = "Recherche des vidéos…";
    try {
      const found = await scanFolder(dir);
      if (found.length === 0) {
        busy = null;
        addErrors = [{ path: dir, message: "Aucune vidéo .ts dans ce dossier." }];
        return;
      }
      await addPaths(found);
    } catch (e) {
      busy = null;
      addErrors = [{ path: dir, message: String(e) }];
    }
  }

  async function pickVideos() {
    const r = await open({ multiple: true, filters: [{ name: "Vidéos Xtra (.ts)", extensions: ["ts"] }] });
    if (r) addPaths(Array.isArray(r) ? r : [r]);
  }

  async function relocate(item: LibraryItem) {
    const path = await open({
      multiple: false,
      title: `Où se trouve « ${item.title || item.fileName} » ?`,
      filters: [{ name: "Vidéo Xtra (.ts)", extensions: ["ts"] }],
    });
    if (typeof path !== "string") return;
    busy = "Vérification du fichier…";
    try {
      await libraryRelocate(item.id, path);
      addErrors = [];
    } catch (e) {
      addErrors = [{ path, message: String(e) }];
    } finally {
      busy = null;
      refresh();
    }
  }

  async function remove(item: LibraryItem) {
    await libraryRemove(item.id).catch((e) => log(`retrait : ${e}`));
    refresh();
  }

  // --- Lecture ------------------------------------------------------------
  let video = $state<VideoInfo | null>(null);
  let chat = $state<ChatPayload | null>(null);
  let refusal = $state<ChatRefusal | null>(null);
  let chatLoading = $state(false);
  let openError = $state<string | null>(null);
  let currentTime = $state(0);
  let startAt = $state(0);
  let theater = $state(false);
  let fullscreen = $state(false);
  /** Mode cinéma ou plein écran : pas de barre du haut ni d'informations sous la vidéo. */
  let immersive = $derived(!!video && (theater || fullscreen));
  let chatOpen = $state(true);
  let offset = $state(0);
  let dragging = $state(false);
  let playerKey = $state(0);

  /** Moment courant dans la VOD, vu par le chat (décalage appliqué). */
  let vodTime = $derived((chat?.startTime ?? 0) + currentTime - offset);

  function show(v: VideoInfo) {
    const e = v.entry;
    // Reprise, sauf si la vidéo était presque finie.
    startAt = e.position > 5 && e.position < v.duration - 15 ? e.position : 0;
    video = v;
    chat = v.chat?.status === "loaded" ? v.chat : null;
    refusal = v.chat?.status === "refused" ? v.chat : null;
    currentTime = startAt;
    offset = 0;
    playerKey++;
    log(`vidéo ouverte : ${v.fileName} (${v.duration.toFixed(1)} s), chat : ${v.chat?.status ?? "aucun"}`);
  }

  async function play(open: () => Promise<VideoInfo>) {
    openError = null;
    busy = "Ouverture de la vidéo…";
    try {
      show(await open());
    } catch (e) {
      openError = String(e);
      addErrors = [{ path: "", message: String(e) }];
      log(`ouverture impossible : ${e}`);
    } finally {
      busy = null;
    }
  }
  const playPath = (path: string) => play(() => openVideo(path));
  const playItem = (item: LibraryItem) => play(() => openEntry(item.id));

  async function pickAndPlay() {
    const path = await open({ multiple: false, filters: [{ name: "Vidéo Xtra (.ts)", extensions: ["ts"] }] });
    if (typeof path === "string") playPath(path);
  }

  async function backToLibrary() {
    if (fullscreen) await toggleFullscreen();
    theater = false;
    video = null; // le lecteur enregistre sa position en se fermant
    chat = null;
    refusal = null;
    setTimeout(refresh, 250);
  }

  async function chatFromPath(path: string) {
    if (!video) return;
    chatLoading = true;
    try {
      const r = await loadChat(video.id, path);
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
    const el = document.querySelector("video");
    if (el) el.currentTime = Math.max(0, t - chat.startTime + offset);
  }

  /** Plein écran de la fenêtre entière, via Tauri (le chat et les commandes restent en place). */
  async function toggleFullscreen() {
    const next = !fullscreen;
    try {
      await getCurrentWindow().setFullscreen(next);
      fullscreen = next;
    } catch (e) {
      log(`plein écran : ${e}`);
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && fullscreen) {
      e.preventDefault();
      toggleFullscreen();
    }
  }

  function onProgress(t: number) {
    if (!video) return;
    video.entry.position = t;
    setProgress(video.entry.id, t);
  }

  function onThumbnail(data: string) {
    if (!video) return;
    video.entry.hasThumbnail = true;
    saveThumbnail(video.entry.id, data);
  }

  onMount(() => {
    refresh();
    startupFiles().then((files) => {
      if (files.length === 1) playPath(files[0]);
      else if (files.length > 1) addPaths(files);
    });
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((e) => {
        const p = e.payload;
        if (p.type === "enter" || p.type === "over") dragging = true;
        else if (p.type === "leave") dragging = false;
        else if (p.type === "drop") {
          dragging = false;
          const ts = p.paths.filter((x) => x.toLowerCase().endsWith(".ts"));
          const json = p.paths.find((x) => x.toLowerCase().endsWith(".json"));
          // Une vidéo : on la lit. Plusieurs : on les ajoute à la liste.
          if (ts.length === 1) playPath(ts[0]);
          else if (ts.length > 1) addPaths(ts);
          else if (json && video) chatFromPath(json);
        }
      })
      .then((u) => (unlisten = u));
    return () => unlisten?.();
  });

  let meta = $derived(chat?.video ?? null);
  let channel = $derived(meta?.channelName || meta?.channelLogin || "");
</script>

<svelte:window onkeydown={onKey} />

<div class="app" class:immersive>
  {#if !immersive}
    <header class="topbar">
      <button class="brand" onclick={backToLibrary} title="Bibliothèque">
        <img src="/icon.png" alt="" />
        <span>Tsuki</span>
      </button>
      {#if video}
        <button class="btn ghost" onclick={backToLibrary}>
          <svg viewBox="0 0 20 20" width="16" height="16"><path fill="currentColor" d="M12.5 4 14 5.5 9.5 10l4.5 4.5-1.5 1.5-6-6z" /></svg>
          Bibliothèque
        </button>
      {/if}
      <span class="spacer"></span>
      <button class="btn" onclick={pickVideos} disabled={!!busy} title="Choisir une ou plusieurs vidéos à ajouter à la liste">
        <svg viewBox="0 0 20 20" width="16" height="16"><path fill="currentColor" d="M9 4h2v5h5v2h-5v5H9v-5H4V9h5z" /></svg>
        Ajouter
      </button>
      <button class="btn primary" onclick={pickAndPlay} disabled={!!busy}>
        <svg viewBox="0 0 20 20" width="16" height="16"><path fill="currentColor" d="M2 4h6l2 2h8v10H2zm2 4v6h12V8z" /></svg>
        Ouvrir une vidéo
      </button>
    </header>
  {/if}

  <main>
    {#if video}
      <section class="player-col">
        {#key playerKey}
          <Player
            {video} {chat} {fullscreen} {startAt} bind:currentTime bind:theater bind:chatOpen
            title={meta?.title ?? video.fileName} {channel}
            needThumbnail={!video.entry.hasThumbnail}
            {onProgress} {onThumbnail} onToggleFullscreen={toggleFullscreen}
          />
        {/key}
        {#if !immersive}
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
      <Library
        {items}
        {busy}
        errors={addErrors}
        onOpen={playItem}
        onAdd={pickVideos}
        onAddFolder={pickFolder}
        onRemove={remove}
        onRelocate={relocate}
        onDismissErrors={() => (addErrors = [])}
      />
    {/if}
  </main>

  {#if dragging}
    <div class="drop">
      Dépose une ou plusieurs vidéos <code>.ts</code>{video ? ", ou un chat .json" : ""}
    </div>
  {/if}
  {#if openError && video}
    <div class="toast" role="alert">{openError}</div>
  {/if}
</div>

<style>
  .app { display: flex; flex-direction: column; height: 100%; }
  .topbar {
    display: flex; align-items: center; gap: 10px; height: var(--topbar); padding: 0 12px; flex-shrink: 0;
    background: var(--bg-alt); border-bottom: 1px solid var(--border); box-shadow: 0 1px 2px rgba(0, 0, 0, 0.4); z-index: 2;
  }
  .brand { display: flex; align-items: center; gap: 8px; font-weight: 800; font-size: 17px; letter-spacing: 0.01em; padding: 4px 6px; border-radius: var(--radius); }
  .brand:hover { background: var(--bg-hover); }
  .brand img { width: 30px; height: 30px; border-radius: 7px; }
  .btn.ghost { background: none; color: var(--text-dim); }
  .btn.ghost:hover { background: var(--bg-hover); color: var(--text); }
  .spacer { flex: 1; }

  main { flex: 1; display: flex; min-height: 0; }
  main > :global(.library) { flex: 1; }
  .player-col { flex: 1; min-width: 0; overflow-y: auto; container-type: inline-size; }
  .immersive .player-col { overflow: hidden; display: flex; flex-direction: column; }

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
  code { background: var(--bg-hover); padding: 1px 5px; border-radius: 4px; font-size: 0.92em; }

  .drop {
    position: fixed; inset: 12px; display: grid; place-items: center; border-radius: 12px; z-index: 50;
    border: 2px dashed var(--accent); background: rgba(14, 14, 16, 0.85); font-size: 18px; font-weight: 700; pointer-events: none;
  }
  .toast {
    position: fixed; left: 50%; bottom: 20px; transform: translateX(-50%); z-index: 40; padding: 10px 16px;
    border-radius: var(--radius); background: var(--danger-soft); border: 1px solid rgba(255, 92, 92, 0.4);
  }
</style>
