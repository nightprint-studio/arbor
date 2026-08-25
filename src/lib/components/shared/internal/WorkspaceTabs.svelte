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
  import { X, Plus, LayoutGrid } from 'lucide-svelte';
  import ProductMark from './ProductMark.svelte';
  import { surfaceStore, surfaceDef } from '$lib/stores/surfaces.svelte';
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';

  /*
   * A tab wears its product's INITIAL, not a picture of what the product does.
   *
   * It used to be a lucide glyph each — a branch, a coffee cup, a musical note, a database —
   * and the problem is the size: this strip gives its icons 13px. At that size a picture has
   * lost the detail that made it specific, and what is left is a generic shape you have to
   * already know the answer to read. A letter loses nothing, and the six of them cannot
   * collide.
   *
   * `home` keeps a picture, because it is the one tab that is not a product and has no
   * initial to wear — a letter there would imply a seventh product.
   */
</script>

{#if surfaceStore.inContainer}
  <div class="wt-strip" role="tablist" aria-label="Open products">
    {#each surfaceStore.tabs as id (id)}
      {@const def = surfaceDef(id)}
      {@const isActive = surfaceStore.isActive(id)}
      <div class="wt-tab" class:active={isActive}>
        <button
          type="button"
          class="wt-main"
          role="tab"
          aria-selected={isActive}
          onclick={() => surfaceStore.show(id)}
          onauxclick={(e) => { if (e.button === 1) void surfaceStore.close(id); }}
        >
          {#if id === 'home'}
            <LayoutGrid size={13} />
          {:else}
            <ProductMark {id} size={15} />
          {/if}
          <span class="wt-label">{def.label}</span>
        </button>
        {#if surfaceStore.tabs.length > 1}
          <button
            type="button"
            class="wt-close"
            aria-label="Close {def.label} tab"
            use:tooltip={'Close tab'}
            onclick={() => void surfaceStore.close(id)}
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
  /* Translucent hover, so a tab passing over the product tint lightens it instead of
     cutting a grey rectangle out of it. The ACTIVE tab keeps its opaque `--bg-base`: that
     one is meant to be a solid card continuing into the body below. */
  .wt-tab:hover { background: color-mix(in srgb, var(--text-primary) 8%, transparent); color: var(--text-secondary); }
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
  .wt-close:hover { opacity: 1; background: color-mix(in srgb, var(--text-primary) 12%, transparent); }

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
  .wt-new:hover { background: color-mix(in srgb, var(--text-primary) 9%, transparent); color: var(--text-primary); }
</style>
