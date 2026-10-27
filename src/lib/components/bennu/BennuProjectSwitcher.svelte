<script lang="ts">
  /**
   * BennuProjectSwitcher — the Alt+Tab-style overlay over your open projects.
   *
   * A strip of tiles rather than a list: a task switcher is something you glance at and release,
   * and a project is recognised by its mark and its name, not by reading a path. The selected tile
   * lifts and takes the accent; the path of whatever is selected appears underneath, since that is
   * the one thing a tile has no room for and the only way to tell two checkouts of one project
   * apart.
   *
   * Purely presentational: the keys (Ctrl+Tab to open and walk, release Ctrl to commit, Esc to
   * cancel, arrows too) are owned by BennuWindow, because they have to run before everything else
   * on the window. Tiles are also clickable, so the overlay works from the palette with a mouse.
   *
   * It lists only what is loaded — on screen, visited this session, or with an index / language
   * server in memory (asked of the backend). Everything else, including a workspace member that
   * was only restored in the background, is "Go to project…", which searches.
   *
   * Motion is opacity and transform only — no blur, which WebView2 pays for on every frame.
   */
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { projectSwitcherStore, type SwitcherEntry } from '$lib/stores/bennu/project-switcher.svelte';
  import { workspacesStore, wsColorVar } from '$lib/stores/bennu/workspaces.svelte';

  let { onPick }: { onPick: (entry: SwitcherEntry) => void } = $props();

  const activeRoot = $derived(projectStore.project?.root ?? null);
  const activeWorkspace = $derived(workspacesStore.activeId);
  const showWorkspace = $derived(workspacesStore.hasMany);
  const selected = $derived(projectSwitcherStore.entries[projectSwitcherStore.index]);

  function isCurrent(e: SwitcherEntry): boolean {
    return e.root === activeRoot && e.workspaceId === activeWorkspace;
  }
</script>

{#if projectSwitcherStore.open}
  <div class="ps-scrim">
    <div class="ps-card" role="dialog" aria-label="Switch project" tabindex="-1">
      <div class="ps-strip" role="listbox" aria-label="Open projects">
        {#each projectSwitcherStore.entries as p, i (p.workspaceId + '|' + p.root)}
          <button
            type="button"
            class="ps-tile"
            class:active={i === projectSwitcherStore.index}
            role="option"
            aria-selected={i === projectSwitcherStore.index}
            style:--i={Math.min(i, 8)}
            style:--ws={showWorkspace ? wsColorVar(p.colorIdx) : 'var(--accent)'}
            onmousemove={() => projectSwitcherStore.select(i)}
            onclick={() => onPick(p)}
          >
            <span class="ps-mark">
              <Monogram
                name={p.name}
                size={44}
                color={showWorkspace ? wsColorVar(p.colorIdx) : undefined}
              />
              {#if isCurrent(p)}<span class="ps-dot" title="Current project"></span>{/if}
            </span>
            <span class="ps-name">{p.name}</span>
            {#if showWorkspace}<span class="ps-ws">{p.workspaceName}</span>{/if}
          </button>
        {/each}
      </div>

      <!-- Keyed by the selection so the line re-fades each time it changes. -->
      {#key projectSwitcherStore.index}
        <div class="ps-path" title={selected?.root}>{selected?.root ?? ''}</div>
      {/key}

      <div class="ps-foot">
        <span><Kbd keys={['Tab']} size="sm" /> next</span>
        <span><Kbd keys={['Shift', 'Tab']} size="sm" /> previous</span>
        <span>release <Kbd keys={['Ctrl']} size="sm" /> to switch</span>
        <span><Kbd keys={['Esc']} size="sm" /> cancel</span>
      </div>
    </div>
  </div>
{/if}

<style>
  .ps-scrim {
    position: fixed; inset: 0; z-index: 1000;
    display: flex; align-items: center; justify-content: center;
    background: color-mix(in srgb, var(--bg-base) 62%, transparent);
    animation: ps-fade 120ms ease both;
  }
  .ps-card {
    max-width: min(760px, calc(100vw - 48px)); max-height: 80vh;
    display: flex; flex-direction: column; align-items: stretch;
    padding: 14px 14px 0;
    background: var(--bg-elevated); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    box-shadow: 0 18px 60px rgba(0, 0, 0, 0.5), 0 0 0 1px color-mix(in srgb, var(--accent) 8%, transparent);
    overflow: hidden;
    animation: ps-pop 200ms cubic-bezier(0.18, 0.9, 0.25, 1.1) both;
  }

  .ps-strip {
    display: flex; flex-wrap: wrap; justify-content: center; gap: 8px;
    padding: 6px 4px 4px; overflow-y: auto;
  }
  .ps-tile {
    position: relative; width: 112px; padding: 12px 8px 10px;
    display: flex; flex-direction: column; align-items: center; gap: 6px;
    background: transparent; border: 1px solid transparent; border-radius: var(--radius-md, 10px);
    color: var(--text-secondary); font-family: var(--font-ui-sans); cursor: pointer;
    transition: transform 160ms cubic-bezier(0.2, 0.9, 0.3, 1.25), background var(--transition-fast),
      border-color var(--transition-fast), box-shadow 160ms ease, color var(--transition-fast);
    /* `backwards`, not `both`: a fill that holds `transform: none` after the entrance would beat
       the `.active` lift below for as long as the overlay is open. */
    animation: ps-tile-in 240ms cubic-bezier(0.2, 0.9, 0.3, 1.1) backwards;
    animation-delay: calc(var(--i) * 22ms);
  }
  .ps-tile.active {
    transform: translateY(-3px) scale(1.06);
    color: var(--text-primary);
    background: color-mix(in srgb, var(--ws) 13%, transparent);
    border-color: color-mix(in srgb, var(--ws) 55%, transparent);
    box-shadow: 0 8px 22px color-mix(in srgb, var(--ws) 28%, transparent);
  }
  .ps-mark { position: relative; display: inline-flex; }
  .ps-dot {
    position: absolute; right: -3px; bottom: -3px; width: 10px; height: 10px; border-radius: 50%;
    background: var(--success); border: 2px solid var(--bg-elevated);
  }
  .ps-name {
    max-width: 100%; text-align: center; font-size: var(--font-size-xs); font-weight: 600; line-height: 1.25;
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
    word-break: break-word;
  }
  .ps-ws {
    max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-size: var(--font-size-3xs); color: var(--text-muted);
  }

  .ps-path {
    margin: 6px 4px 0; padding: 0 4px; min-height: 18px; text-align: center;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl;
    animation: ps-fade 160ms ease both;
  }
  .ps-foot {
    display: flex; flex-wrap: wrap; justify-content: center; gap: 6px 16px; margin: 8px -14px 0;
    padding: 9px 14px; border-top: 1px solid var(--border-subtle);
    font-size: var(--font-size-2xs); color: var(--text-muted);
  }

  @keyframes ps-fade { from { opacity: 0; } to { opacity: 1; } }
  @keyframes ps-pop {
    from { opacity: 0; transform: translateY(10px) scale(0.96); }
    to   { opacity: 1; transform: none; }
  }
  @keyframes ps-tile-in {
    from { opacity: 0; transform: translateY(10px) scale(0.92); }
    to   { opacity: 1; transform: none; }
  }
  @media (prefers-reduced-motion: reduce) {
    .ps-scrim, .ps-card, .ps-tile, .ps-path { animation: none; }
    .ps-tile { transition: none; }
  }
</style>
