<script lang="ts">
  import { thumbUrl, type LibraryItem } from "./api";
  import { clock, longDate } from "./format";

  interface Props {
    items: LibraryItem[];
    busy: string | null;
    errors: { path: string; message: string }[];
    onOpen: (item: LibraryItem) => void;
    onAdd: () => void;
    onRemove: (item: LibraryItem) => void;
    onRelocate: (item: LibraryItem) => void;
    onDismissErrors: () => void;
  }
  let { items, busy, errors, onOpen, onAdd, onRemove, onRelocate, onDismissErrors }: Props = $props();

  let query = $state("");
  let sort = $state<"recent" | "stream" | "channel">("recent");
  let menuFor = $state<string | null>(null);

  let shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const list = q
      ? items.filter((i) =>
          [i.title, i.channel, i.game, i.fileName].some((s) => s?.toLowerCase().includes(q)),
        )
      : [...items];
    if (sort === "stream") list.sort((a, b) => (b.uploadDate ?? 0) - (a.uploadDate ?? 0));
    else if (sort === "channel") list.sort((a, b) => (a.channel ?? "~").localeCompare(b.channel ?? "~", "fr"));
    return list;
  });

  const progress = (i: LibraryItem) => (i.duration > 0 ? Math.min(100, (i.position / i.duration) * 100) : 0);
  const name = (i: LibraryItem) => i.title || i.fileName;
</script>

<svelte:window onclick={() => (menuFor = null)} />

<div class="library">
  <div class="head">
    <h1>Vidéos récentes</h1>
    {#if items.length > 0}
      <input class="search" type="search" placeholder="Rechercher un titre, une chaîne…" bind:value={query} />
      <select bind:value={sort} aria-label="Trier">
        <option value="recent">Regardées récemment</option>
        <option value="stream">Date du stream</option>
        <option value="channel">Chaîne</option>
      </select>
    {/if}
    <span class="spacer"></span>
    <button class="btn primary" onclick={onAdd} disabled={!!busy}>
      <svg viewBox="0 0 20 20" width="16" height="16"><path fill="currentColor" d="M9 4h2v5h5v2h-5v5H9v-5H4V9h5z" /></svg>
      Ajouter des vidéos
    </button>
  </div>

  {#if busy}
    <div class="banner"><span class="spinner"></span>{busy}</div>
  {/if}
  {#if errors.length > 0}
    <div class="banner error" role="alert">
      <div>
        <strong>{errors.length} fichier{errors.length > 1 ? "s" : ""} non ajouté{errors.length > 1 ? "s" : ""} :</strong>
        <ul>
          {#each errors as e}<li><code>{e.path.split(/[\\/]/).pop()}</code> — {e.message}</li>{/each}
        </ul>
      </div>
      <button class="icon-btn" onclick={onDismissErrors} aria-label="Fermer">✕</button>
    </div>
  {/if}

  {#if items.length === 0}
    <div class="empty">
      <img class="logo" src="/icon.png" alt="" />
      <h2>Aucune rediffusion pour l'instant</h2>
      <p>Ajoute des vidéos <code>.ts</code> téléchargées avec Xtra — tu peux en choisir plusieurs à la fois,
        ou les glisser dans cette fenêtre. Leur chat <code>_chat.json</code> est chargé automatiquement
        s'il est dans le même dossier.</p>
      <button class="btn primary" onclick={onAdd} disabled={!!busy}>Ajouter des vidéos</button>
    </div>
  {:else if shown.length === 0}
    <p class="none">Aucune vidéo ne correspond à « {query} ».</p>
  {:else}
    <div class="grid">
      {#each shown as item (item.id)}
        <article class="card" class:missing={!item.exists}>
          <button class="thumb" onclick={() => (item.exists ? onOpen(item) : onRelocate(item))}
            title={item.exists ? `Regarder « ${name(item)} »` : "Fichier introuvable — cliquer pour le relocaliser"}>
            {#if item.hasThumbnail}
              <img src={thumbUrl(item.id, item.lastOpenedAt ?? 0)} alt="" loading="lazy" />
            {:else}
              <div class="placeholder">{(item.channel || item.fileName)[0]?.toUpperCase()}</div>
            {/if}
            <span class="badge duration">{clock(item.duration)}</span>
            {#if item.startTime > 0}<span class="badge from">Dès {clock(item.startTime)}</span>{/if}
            {#if item.position > 5}
              <span class="progress"><span style:width="{progress(item)}%"></span></span>
            {/if}
            {#if !item.exists}
              <span class="lost">
                <strong>Fichier introuvable</strong>
                <span>Cliquer pour relocaliser</span>
              </span>
            {/if}
          </button>

          <div class="meta">
            <div class="avatar" aria-hidden="true">{(item.channel || item.fileName)[0]?.toUpperCase()}</div>
            <div class="text">
              <h3 title={name(item)}>{name(item)}</h3>
              <div class="sub">{item.channel ?? "Chat non chargé"}</div>
              <div class="sub">
                {#if item.game}<span class="game">{item.game}</span> · {/if}
                {item.uploadDate ? longDate(item.uploadDate) : item.fileName}
              </div>
            </div>
            <div class="more">
              <button class="icon-btn" aria-label="Options"
                onclick={(e) => { e.stopPropagation(); menuFor = menuFor === item.id ? null : item.id; }}>
                <svg viewBox="0 0 20 20"><circle fill="currentColor" cx="10" cy="4" r="1.6" /><circle fill="currentColor" cx="10" cy="10" r="1.6" /><circle fill="currentColor" cx="10" cy="16" r="1.6" /></svg>
              </button>
              {#if menuFor === item.id}
                <div class="menu" role="menu">
                  {#if item.exists}<button onclick={() => onOpen(item)}>Regarder</button>{/if}
                  <button onclick={() => onRelocate(item)}>Relocaliser le fichier…</button>
                  <button class="danger" onclick={() => onRemove(item)}>Retirer de la liste</button>
                </div>
              {/if}
            </div>
          </div>
        </article>
      {/each}
    </div>
  {/if}
</div>

<style>
  .library { height: 100%; overflow-y: auto; padding: 24px 30px 40px; }
  .head { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; margin-bottom: 20px; }
  h1 { margin: 0 8px 0 0; font-size: 24px; font-weight: 800; }
  .spacer { flex: 1; }
  .search, select {
    height: 34px; padding: 0 12px; border-radius: var(--radius); border: 1px solid var(--border);
    background: var(--bg-elev); color: var(--text); font: inherit;
  }
  .search { width: 280px; }
  .search:focus, select:focus { outline: 2px solid var(--accent); border-color: transparent; }

  .banner {
    display: flex; align-items: flex-start; gap: 10px; padding: 10px 14px; margin-bottom: 16px;
    border-radius: var(--radius); background: var(--bg-elev); border: 1px solid var(--border);
  }
  .banner.error { background: var(--danger-soft); border-color: rgba(255, 92, 92, 0.35); justify-content: space-between; }
  .banner ul { margin: 6px 0 0; padding-left: 18px; color: var(--text-dim); }
  .spinner {
    width: 16px; height: 16px; border-radius: 50%; border: 2px solid var(--border); border-top-color: var(--accent);
    animation: spin 0.8s linear infinite; flex-shrink: 0; margin-top: 2px;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 26px 18px; }
  .card { min-width: 0; }
  .thumb {
    position: relative; display: block; width: 100%; aspect-ratio: 16 / 9; padding: 0; overflow: hidden;
    background: var(--bg-elev); transition: transform 0.12s, box-shadow 0.12s;
  }
  .thumb:hover { transform: translate(6px, -6px); box-shadow: -6px 6px 0 var(--accent); }
  .thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .placeholder {
    width: 100%; height: 100%; display: grid; place-items: center; font-size: 46px; font-weight: 800;
    color: rgba(255, 255, 255, 0.25); background: linear-gradient(135deg, #2a2540, #141420);
  }
  .badge {
    position: absolute; padding: 1px 5px; border-radius: 3px; font-size: 12px; font-weight: 600;
    background: rgba(0, 0, 0, 0.75); font-variant-numeric: tabular-nums;
  }
  .duration { right: 8px; bottom: 10px; }
  .from { left: 8px; top: 8px; }
  .progress { position: absolute; left: 0; right: 0; bottom: 0; height: 4px; background: rgba(255, 255, 255, 0.25); }
  .progress span { display: block; height: 100%; background: var(--accent); }
  .card.missing .thumb img, .card.missing .placeholder { filter: grayscale(1) brightness(0.45); }
  .lost {
    position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 4px; color: var(--text); font-size: 13px;
  }
  .lost strong { color: var(--danger); font-size: 14px; }

  .meta { display: flex; gap: 10px; margin-top: 10px; }
  .avatar {
    width: 40px; height: 40px; flex-shrink: 0; border-radius: 50%; display: grid; place-items: center;
    font-weight: 800; color: #0e0e10; background: linear-gradient(135deg, #d9d4ff, var(--accent));
  }
  .text { flex: 1; min-width: 0; }
  h3 { margin: 0 0 3px; font-size: 14px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sub { color: var(--text-dim); font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .game { color: var(--accent); }
  .more { position: relative; }
  .menu {
    position: absolute; right: 0; top: 34px; z-index: 5; width: 210px; padding: 6px; border-radius: var(--radius);
    background: var(--bg-elev); border: 1px solid var(--border); box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
    display: flex; flex-direction: column;
  }
  .menu button { text-align: left; padding: 8px 10px; border-radius: 4px; }
  .menu button:hover { background: var(--bg-hover); }
  .menu .danger { color: var(--danger); }

  .empty { display: flex; flex-direction: column; align-items: center; text-align: center; gap: 12px; padding: 60px 20px; }
  .logo { width: 110px; height: 110px; border-radius: 24px; }
  .empty h2 { margin: 8px 0 0; font-size: 22px; }
  .empty p { color: var(--text-dim); max-width: 520px; line-height: 1.6; margin: 0 0 8px; }
  .none { color: var(--text-dim); }
  code { background: var(--bg-hover); padding: 1px 5px; border-radius: 4px; font-size: 0.92em; }
</style>
