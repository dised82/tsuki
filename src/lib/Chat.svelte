<script lang="ts">
  import { tick } from "svelte";
  import { badgeUrl, emoteUrl, type ChatPayload, type ChatRefusal, type Comment } from "./api";
  import { clock, nameColor, store, stored } from "./format";

  interface Props {
    videoId: number;
    chat: ChatPayload | null;
    refusal: ChatRefusal | null;
    /** Moment courant dans la VOD (secondes), décalage du chat déjà appliqué. */
    vodTime: number;
    offset: number;
    loading: boolean;
    onPick: () => void;
    onSeekVod: (vodTime: number) => void;
    onOffset: (offset: number) => void;
    onClose: () => void;
  }
  let { videoId, chat, refusal, vodTime, offset, loading, onPick, onSeekVod, onOffset, onClose }: Props = $props();

  const LIVE_WINDOW = 150;
  const PAUSED_WINDOW = 500;

  let list: HTMLDivElement | undefined = $state();
  let paused = $state(false);
  let frozenEnd = $state(0);
  let menuOpen = $state(false);
  let showTime = $state(stored("chat.showTime", false));
  let fontSize = $state(stored("chat.fontSize", 13));

  let emoteSet = $derived(new Set(chat?.emotes ?? []));
  let badgeSet = $derived(new Set((chat?.badges ?? []).map(([s, v]) => `${s}/${v}`)));

  /** Nombre de messages déjà postés à cet instant (recherche dichotomique). */
  let upto = $derived.by(() => {
    const cs = chat?.comments;
    if (!cs) return 0;
    let lo = 0, hi = cs.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (cs[mid].t <= vodTime) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  });

  // Retour en arrière dans la vidéo : on relance le défilement automatique.
  $effect(() => {
    if (paused && upto < frozenEnd) paused = false;
  });

  let end = $derived(paused ? frozenEnd : upto);
  let start = $derived(Math.max(0, end - (paused ? PAUSED_WINDOW : LIVE_WINDOW)));
  let visible = $derived(chat ? chat.comments.slice(start, end) : []);
  let pending = $derived(paused ? upto - frozenEnd : 0);

  // Défilement automatique vers le bas quand de nouveaux messages arrivent.
  $effect(() => {
    void visible;
    if (!paused && list) tick().then(() => list && (list.scrollTop = list.scrollHeight));
  });

  function onScroll() {
    if (!list) return;
    const fromBottom = list.scrollHeight - list.scrollTop - list.clientHeight;
    if (!paused && fromBottom > 60) {
      paused = true;
      frozenEnd = upto;
    } else if (paused && fromBottom < 4) {
      paused = false;
    }
  }

  function resume() {
    paused = false;
  }

  function setShowTime(v: boolean) {
    showTime = v;
    store("chat.showTime", v);
  }
  function setFont(v: number) {
    fontSize = Math.min(20, Math.max(11, v));
    store("chat.fontSize", fontSize);
  }

  const key = (c: Comment, i: number) => start + i;
</script>

<aside class="chat" style:--chat-font="{fontSize}px">
  <header>
    <button class="icon-btn" onclick={onClose} title="Masquer le chat (c)">
      <svg viewBox="0 0 20 20"><path fill="currentColor" d="M4 4h2v12H4zm4 6 5-5 1.4 1.4L11.8 9H17v2h-5.2l2.6 2.6L13 15z" /></svg>
    </button>
    <h2>Chat de la rediffusion</h2>
    <div class="menu-wrap">
      <button class="icon-btn" onclick={() => (menuOpen = !menuOpen)} title="Options du chat" aria-haspopup="menu">
        <svg viewBox="0 0 20 20"><circle fill="currentColor" cx="10" cy="4" r="1.6" /><circle fill="currentColor" cx="10" cy="10" r="1.6" /><circle fill="currentColor" cx="10" cy="16" r="1.6" /></svg>
      </button>
      {#if menuOpen}
        <div class="menu" role="menu">
          <button onclick={() => { menuOpen = false; onPick(); }} role="menuitem">
            Choisir un fichier JSON…
          </button>
          <div class="sep"></div>
          <label class="row">
            <input type="checkbox" checked={showTime} onchange={(e) => setShowTime(e.currentTarget.checked)} />
            Afficher l'heure des messages
          </label>
          <div class="row">
            <span>Taille du texte</span>
            <span class="stepper">
              <button onclick={() => setFont(fontSize - 1)} aria-label="Réduire">−</button>
              <b>{fontSize}</b>
              <button onclick={() => setFont(fontSize + 1)} aria-label="Agrandir">+</button>
            </span>
          </div>
          <div class="row">
            <span>Décalage du chat</span>
            <span class="stepper">
              <button onclick={() => onOffset(offset - 0.5)} aria-label="Avancer le chat">−</button>
              <b>{offset > 0 ? "+" : ""}{offset.toFixed(1)} s</b>
              <button onclick={() => onOffset(offset + 0.5)} aria-label="Retarder le chat">+</button>
            </span>
          </div>
          {#if offset !== 0}
            <button class="small" onclick={() => onOffset(0)}>Réinitialiser le décalage</button>
          {/if}
        </div>
      {/if}
    </div>
  </header>

  {#if loading}
    <div class="empty"><div class="spinner"></div>Chargement du chat…</div>
  {:else if refusal}
    <div class="empty">
      <div class="refusal" role="alert">
        <strong>{refusal.message}</strong>
        {#if refusal.detail}<p>{refusal.detail}</p>{/if}
        <code title={refusal.path}>{refusal.path.split(/[\\/]/).pop()}</code>
      </div>
      <button class="btn primary" onclick={onPick}>Choisir un autre fichier</button>
    </div>
  {:else if !chat}
    <div class="empty">
      <svg viewBox="0 0 20 20" class="big"><path fill="currentColor" d="M3 3h14v10h-7l-4 4v-4H3zm2 2v6h3v1.2L9.2 11H15V5z" /></svg>
      <p>Aucun chat pour cette vidéo.</p>
      <button class="btn primary" onclick={onPick}>Choisir un fichier JSON</button>
    </div>
  {:else}
    <div class="list" bind:this={list} onscroll={onScroll}>
      {#if visible.length === 0}
        <p class="welcome">Bienvenue dans le chat ! Les messages apparaîtront au fil de la vidéo.</p>
      {/if}
      {#each visible as c, i (key(c, i))}
        <div class="msg">
          {#if showTime}
            <button class="ts" onclick={() => onSeekVod(c.t)} title="Aller à ce moment">{clock(c.t)}</button>
          {/if}
          {#each c.b as [set, ver]}
            {#if badgeSet.has(`${set}/${ver}`)}
              <img class="badge" src={badgeUrl(videoId, chat.generation, set, ver)} alt={set} title={set} />
            {/if}
          {/each}
          <span class="name" style:color={nameColor(c.c, c.l || c.n)}>{c.n}</span><span class="colon">:</span>
          {#each c.f as [text, emote]}
            {#if emote && emoteSet.has(emote)}
              <img class="emote" src={emoteUrl(videoId, chat.generation, emote)} alt={text} title={text} />
            {:else}
              <span>{text}</span>
            {/if}
          {/each}
        </div>
      {/each}
    </div>
    {#if paused}
      <button class="paused" onclick={resume}>
        {#if pending > 0}
          {pending > 99 ? "99+" : pending} nouveau{pending > 1 ? "x" : ""} message{pending > 1 ? "s" : ""}
        {:else}
          Chat en pause en raison du défilement
        {/if}
      </button>
    {/if}
  {/if}
</aside>

<style>
  .chat {
    position: relative; display: flex; flex-direction: column; height: 100%; min-height: 0;
    width: var(--chat-width); flex-shrink: 0; background: var(--bg-alt); border-left: 1px solid var(--border);
  }
  header {
    display: flex; align-items: center; gap: 4px; height: 50px; padding: 0 8px; flex-shrink: 0;
    border-bottom: 1px solid var(--border);
  }
  h2 { flex: 1; margin: 0; text-align: center; font-size: 12px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.04em; }

  /* Comme sur Twitch : les messages arrivent par le bas et poussent les anciens vers le haut. */
  .list {
    flex: 1; overflow-y: auto; padding: 8px 0 16px; font-size: var(--chat-font); line-height: 1.6; overflow-wrap: anywhere;
    display: flex; flex-direction: column;
  }
  .list > :first-child { margin-top: auto; }
  .welcome { color: var(--text-faint); padding: 0 16px; font-size: 13px; }
  .msg { padding: 3px 16px; }
  .msg:hover { background: rgba(255, 255, 255, 0.04); }
  .ts { color: var(--text-faint); font-size: 0.85em; margin-right: 4px; font-variant-numeric: tabular-nums; padding: 0; }
  .ts:hover { color: var(--accent); text-decoration: underline; }
  .badge { width: 18px; height: 18px; vertical-align: middle; margin: -2px 3px 0 0; border-radius: 3px; }
  .name { font-weight: 700; }
  .colon { margin-right: 4px; }
  .emote { height: 28px; vertical-align: middle; margin: -6px 0 -4px; }

  .paused {
    position: absolute; left: 50%; bottom: 14px; transform: translateX(-50%); white-space: nowrap;
    padding: 8px 14px; border-radius: var(--radius); background: rgba(14, 14, 16, 0.92);
    border: 1px solid var(--border); font-weight: 600; font-size: 13px; box-shadow: 0 4px 14px rgba(0, 0, 0, 0.5);
  }
  .paused:hover { background: var(--bg-hover); }

  .empty {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 12px; padding: 24px; text-align: center; color: var(--text-dim);
  }
  .empty p { margin: 0; }
  .big { width: 40px; height: 40px; color: var(--text-faint); }
  .refusal {
    display: flex; flex-direction: column; gap: 6px; padding: 14px; border-radius: var(--radius);
    background: var(--danger-soft); border: 1px solid rgba(255, 92, 92, 0.35); color: var(--text); text-align: left;
  }
  .refusal strong { color: var(--danger); }
  .refusal p { color: var(--text-dim); font-size: 13px; }
  .refusal code { font-size: 11px; color: var(--text-faint); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .spinner {
    width: 28px; height: 28px; border-radius: 50%; border: 3px solid var(--border); border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .menu-wrap { position: relative; }
  .menu {
    position: absolute; right: 0; top: 38px; z-index: 10; width: 270px; padding: 6px; border-radius: var(--radius);
    background: var(--bg-elev); border: 1px solid var(--border); box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
    display: flex; flex-direction: column; gap: 2px;
  }
  .menu > button, .row { text-align: left; padding: 8px 10px; border-radius: 4px; }
  .menu > button:hover { background: var(--bg-hover); }
  .row { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  label.row { justify-content: flex-start; cursor: pointer; }
  label.row input { accent-color: var(--accent); }
  .stepper { display: inline-flex; align-items: center; gap: 6px; }
  .stepper button { width: 24px; height: 24px; border-radius: 4px; background: var(--bg-hover); font-weight: 700; }
  .stepper b { min-width: 48px; text-align: center; font-variant-numeric: tabular-nums; }
  .small { font-size: 12px; color: var(--accent); }
  .sep { height: 1px; background: var(--border); margin: 4px 0; }
</style>
