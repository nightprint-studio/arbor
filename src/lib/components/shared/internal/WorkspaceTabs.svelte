<script lang="ts">
  /**
   * Product tab strip for the tabbed container.
   *
   * Rendered by each product's title bar (in its `leading` slot) rather than by
   * a bar of its own: a second chrome row just to hold four tabs would eat ~40px
   * for nothing, and the products already paint a title bar. Outside the
   * container `surfaceStore.inContainer` is false and this renders nothing, so
   * the title bars can mount it unconditionally.
   */
  import { X, Plus, GitBranch, Coffee, Music, Database, LayoutGrid } from 'lucide-svelte';
  import type { IconComponent } from '$lib/types/icon';
  import { surfaceStore, surfaceDef, type SurfaceId } from '$lib/stores/surfaces.svelte';
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';

  const ICONS: Record<SurfaceId, IconComponent> = {
    home:   LayoutGrid,
    corvus: GitBranch,
    bennu:  Coffee,
    merula: Music,
    picus:  Database,
  };
</script>

{#if surfaceStore.inContainer}
  <div class="wt-strip" role="tablist" aria-label="Open products">
    {#each surfaceStore.tabs as id (id)}
      {@const def = surfaceDef(id)}
      {@const Icon = ICONS[id]}
      {@const isActive = surfaceStore.isActive(id)}
      <div class="wt-tab" class:active={isActive}>
        <button
          type="button"
          class="wt-main"
          role="tab"
          aria-selected={isActive}
          onclick={() => surfaceStore.show(id)}
          onauxclick={(e) => { if (e.button === 1) surfaceStore.close(id); }}
        >
          <Icon size={13} />
          <span class="wt-label">{def.label}</span>
        </button>
        {#if surfaceStore.tabs.length > 1}
          <button
            type="button"
            class="wt-close"
            aria-label="Close {def.label} tab"
            use:tooltip={'Close tab'}
            onclick={() => surfaceStore.close(id)}
          >
            <X size={11} />
          </button>
        {/if}
      </div>
    {/each}

    <!-- Brings the welcome page back: opening a product replaces it, so this is
         the way to it (along with Ctrl+T). -->
    <button
      type="button"
      class="wt-new"
      aria-label="Open the welcome page in a new tab"
      use:tooltip={{ content: 'Welcome page', shortcut: 'Ctrl+T' }}
      onclick={() => surfaceStore.openHome()}
    >
      <Plus size={13} />
    </button>
  </div>
{/if}

<style>
  .wt-strip {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 100%;
    padding: 0 6px;
    min-width: 0;
  }

  /* A tab reads as a raised card against the title bar's elevated background —
     the active one takes the panel colour, so it visually continues into the
     body below it. */
  .wt-tab {
    display: flex;
    align-items: center;
    height: calc(100% - 8px);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    background: transparent;
    transition: background var(--anim-dur-fast), color var(--anim-dur-fast);
    min-width: 0;
  }
  .wt-tab:hover { background: var(--bg-hover); color: var(--text-secondary); }
  .wt-tab.active {
    background: var(--bg-base);
    color: var(--text-primary);
  }

  .wt-main {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 100%;
    padding: 0 4px 0 9px;
    background: none;
    border: none;
    color: inherit;
    font: 12px var(--font-ui-sans);
    cursor: pointer;
    min-width: 0;
  }
  .wt-label {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .wt-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 17px;
    height: 17px;
    margin-right: 5px;
    padding: 0;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: inherit;
    opacity: 0.55;
    cursor: pointer;
  }
  .wt-close:hover { opacity: 1; background: var(--bg-overlay); }

  .wt-new {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    margin-left: 2px;
    padding: 0;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    cursor: pointer;
  }
  .wt-new:hover { background: var(--bg-hover); color: var(--text-primary); }
</style>
