<script lang="ts">
  import type { Snippet } from 'svelte';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { ChevronDown, Loader } from 'lucide-svelte';
  import { animStore } from '$lib/stores/animations.svelte';

  interface Props {
    label: string;
    /** Lucide icon component rendered at size 10 inside the chip */
    icon?: any;
    /** Count badge — also drives chip-active when > 0 */
    count?: number;
    /** Explicit active override (takes precedence over count) */
    active?: boolean;
    /** Wider panel (min-width: 220px vs 180px) */
    wide?: boolean;
    searchable?: boolean;
    searchPlaceholder?: string;
    /** Show a loading spinner instead of children content */
    loading?: boolean;
    /** Fired when the panel opens (use for lazy-loading filter options) */
    onopen?: () => void;
    children?: Snippet<[{ filter: string; close: () => void }]>;
    class?: string;
  }

  let {
    label,
    icon: Icon,
    count = 0,
    active,
    wide = false,
    searchable = false,
    searchPlaceholder = 'Filter…',
    loading = false,
    onopen,
    children,
    class: rootClass = '',
  }: Props = $props();

  let open      = $state(false);
  let anchor    = $state<{ x: number; y: number } | null>(null);
  let filter    = $state('');
  let triggerEl = $state<HTMLElement | undefined>();
  let panelEl   = $state<HTMLElement | undefined>();

  const isActive = $derived(active !== undefined ? active : count > 0);

  function toggle(e: MouseEvent) {
    if (open) { close(); return; }
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    anchor = { x: r.left, y: r.bottom + 4 };
    filter = '';
    open   = true;
    onopen?.();
  }

  function close() {
    open   = false;
    anchor = null;
  }

  $effect(() => {
    if (!open) return;
    function onPointer(e: PointerEvent) {
      const t = e.target as Node;
      if (panelEl?.contains(t) || triggerEl?.contains(t)) return;
      close();
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') { e.stopPropagation(); close(); }
    }
    document.addEventListener('pointerdown', onPointer, { capture: true });
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('pointerdown', onPointer, { capture: true } as EventListenerOptions);
      document.removeEventListener('keydown', onKey);
    };
  });
</script>

<div class="fb-root {rootClass}">
  <button
    class="chip"
    class:chip-active={isActive}
    onclick={toggle}
    bind:this={triggerEl}
  >
    {#if Icon}<Icon size={10} />{/if}
    {label}
    {#if count > 0}<span class="chip-badge">{count}</span>{/if}
    <ChevronDown size={9} />
  </button>

  {#if open && anchor}
    <div
      class="chip-drop"
      class:chip-drop-wide={wide}
      style="left:{anchor.x}px; top:{anchor.y}px"
      bind:this={panelEl}
      transition:fly={{ y: -6, duration: animStore.dFast, easing: cubicOut }}
    >
      {#if searchable}
        <div class="chip-drop-search-wrap">
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="chip-drop-search"
            type="text"
            placeholder={searchPlaceholder}
            bind:value={filter}
            autofocus
          />
        </div>
      {/if}

      {#if loading}
        <div class="chip-drop-empty"><Loader size={12} class="spin" /> Loading…</div>
      {:else}
        {@render children?.({ filter, close })}
      {/if}
    </div>
  {/if}
</div>

<style>
  .fb-root { display: inline-block; }

  .chip {
    display: inline-flex; align-items: center; gap: 3px;
    padding: 3px 7px;
    font-size: 10px; font-weight: 500;
    font-family: var(--font-ui-sans);
    color: var(--text-muted);
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: 99px; cursor: pointer;
    transition: all var(--transition-fast);
    white-space: nowrap;
  }
  .chip:hover { border-color: var(--border); color: var(--text-secondary); }
  .chip-active {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--accent);
  }
  .chip-badge {
    background: var(--accent);
    color: var(--bg-base);
    border-radius: 99px;
    padding: 0 4px;
    font-size: 9px;
  }

  .chip-drop {
    position: fixed; z-index: var(--z-menu, 300);
    min-width: 180px; max-height: 280px; overflow-y: auto;
    background: var(--bg-overlay); border: 1px solid var(--border);
    border-radius: var(--radius-md); padding: 4px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.4);
  }
  .chip-drop-wide { min-width: 220px; }

  .chip-drop-search-wrap {
    padding: 4px 4px 2px;
    position: sticky; top: 0;
    background: var(--bg-overlay);
    z-index: 1;
  }
  .chip-drop-search {
    width: 100%; box-sizing: border-box;
    padding: 4px 8px; font-size: 11px;
    font-family: var(--font-ui-sans);
    background: var(--bg-base); color: var(--text-primary);
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    outline: none;
    transition: border-color var(--transition-fast);
  }
  .chip-drop-search:focus { border-color: var(--accent); }

  .chip-drop-empty {
    display: flex; align-items: center; gap: 6px; justify-content: center;
    padding: 10px 8px; font-size: 11px; color: var(--text-muted); font-style: italic;
  }
</style>
