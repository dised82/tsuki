<script lang="ts">
  interface Props {
    onClose: () => void;
  }
  let { onClose }: Props = $props();

  const groups: { title: string; keys: [string[], string][] }[] = [
    {
      title: "Lecture",
      keys: [
        [["Espace"], "Lecture / pause"],
        [["K"], "Lecture / pause"],
        [["←", "→"], "Reculer / avancer de 5 s"],
        [["J", "L"], "Reculer / avancer de 10 s"],
        [["0", "…", "9"], "Aller à 0 %, 10 % … 90 %"],
        [["Début", "Fin"], "Début / fin de la vidéo"],
        [["<", ">"], "Ralentir / accélérer"],
        [["↑", "↓"], "Volume"],
        [["M"], "Couper le son"],
      ],
    },
    {
      title: "Affichage",
      keys: [
        [["F"], "Plein écran"],
        [["T"], "Mode cinéma"],
        [["C"], "Afficher / masquer le chat"],
        [["Échap"], "Quitter le plein écran"],
        [["Double-clic"], "Plein écran"],
      ],
    },
    {
      title: "Chat",
      keys: [
        [["/"], "Rechercher dans le chat"],
        [["Ctrl", "F"], "Rechercher dans le chat"],
        [["@pseudo"], "Messages d'un utilisateur (dans la recherche)"],
        [["Clic sur un pseudo"], "Fiche de l'utilisateur"],
        [["Échap"], "Fermer la recherche ou la fiche"],
      ],
    },
    {
      title: "Général",
      keys: [[["?"], "Afficher cette aide"]],
    },
  ];
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && (e.preventDefault(), e.stopPropagation(), onClose())} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="sc-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <header>
      <h2 id="sc-title">Raccourcis clavier</h2>
      <button class="icon-btn" onclick={onClose} aria-label="Fermer">✕</button>
    </header>
    <div class="cols">
      {#each groups as g}
        <section>
          <h3>{g.title}</h3>
          {#each g.keys as [keys, label]}
            <div class="row">
              <span class="keys">
                {#each keys as k}{#if k === "…"}<span class="dots">…</span>{:else}<kbd>{k}</kbd>{/if}{/each}
              </span>
              <span class="label">{label}</span>
            </div>
          {/each}
        </section>
      {/each}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed; inset: 0; z-index: 100; display: grid; place-items: center; padding: 20px;
    background: rgba(0, 0, 0, 0.6);
  }
  .dialog {
    width: min(760px, 100%); max-height: 100%; overflow-y: auto; border-radius: 10px; padding: 18px 22px 22px;
    background: var(--bg-alt); border: 1px solid var(--border); box-shadow: 0 16px 48px rgba(0, 0, 0, 0.6);
  }
  header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 10px; }
  h2 { margin: 0; font-size: 18px; }
  .cols { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 10px 30px; }
  h3 { margin: 12px 0 6px; font-size: 12px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--accent); }
  .row { display: flex; align-items: center; gap: 12px; padding: 4px 0; font-size: 13px; }
  .keys { display: inline-flex; gap: 4px; min-width: 118px; flex-shrink: 0; }
  .label { color: var(--text-dim); }
  kbd {
    min-width: 24px; padding: 2px 6px; border-radius: 4px; text-align: center;
    font-family: inherit; font-size: 12px; font-weight: 600; line-height: 1.4; white-space: nowrap;
    background: var(--bg-elev); border: 1px solid var(--border); border-bottom-width: 2px;
  }
  .dots { color: var(--text-faint); align-self: center; }
</style>
