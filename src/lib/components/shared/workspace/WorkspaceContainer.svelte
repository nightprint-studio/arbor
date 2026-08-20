<script lang="ts">
  /**
   * The tabbed container window — several products in one window.
   *
   * Mounted for the `workspace` window label. It owns no chrome of its own: the
   * active product's title bar renders the tab strip in its `leading` slot, so
   * the container costs zero extra vertical space and every product keeps its
   * own menus. All this component does is decide WHICH shells are mounted and
   * which one is on screen.
   *
   * Products are not interchangeable with the other surfaces: the File Explorer
   * is freely multi-instance and Tyto belongs in the tray, so neither is
   * tabbable (see `SurfaceKind` in the shell). And each product gets at most one
   * tab — its state lives in module-level stores, one set per window.
   */
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import {
    takeWorkspaceIntent, WORKSPACE_OPEN_PRODUCT_EVENT,
  } from '$lib/ipc/window';
  import { surfaceStore, SURFACES, type SurfaceId } from '$lib/stores/surfaces.svelte';
  import SurfaceBoot from './SurfaceBoot.svelte';
  import { keybindingsStore } from '$lib/stores/keybindings.svelte';
  import { matchesBinding } from '$lib/utils/keybindings';
  import SurfaceHost from './SurfaceHost.svelte';

  function isSurfaceId(v: string): v is SurfaceId {
    return SURFACES.some((s) => s.id === v);
  }

  /** Honour a "show this product" request from the launcher / another window. */
  function showRequested(product: string | null) {
    if (product && isSurfaceId(product)) surfaceStore.show(product);
  }

  surfaceStore.enterContainer();

  onMount(() => {
    // The launcher parks its intent before this window exists (a fresh container
    // can't receive an event yet), so pull it once on mount…
    void takeWorkspaceIntent().then(showRequested).catch(() => {});
    // …and listen from here on, for launches into an already-open container.
    const un = listen<string>(WORKSPACE_OPEN_PRODUCT_EVENT, (e) => showRequested(e.payload));
    return () => { void un.then((off) => off()); };
  });

  // Tab navigation. Capture phase: a product shell's own global key handler must
  // not swallow the chord that leaves it — the same reasoning as the window
  // switcher's listener.
  function onKeydown(e: KeyboardEvent) {
    const kb = keybindingsStore.getBinding.bind(keybindingsStore);
    if (matchesBinding(e, kb('new_surface_tab')))  { e.preventDefault(); e.stopPropagation(); surfaceStore.openHome(); return; }
    if (matchesBinding(e, kb('next_surface_tab'))) { e.preventDefault(); e.stopPropagation(); surfaceStore.step(1);   return; }
    if (matchesBinding(e, kb('prev_surface_tab'))) { e.preventDefault(); e.stopPropagation(); surfaceStore.step(-1);  return; }
    // Ctrl+1…9 jumps straight to a tab — authored here rather than as nine
    // keybindings, mirroring how browsers and IDEs treat positional tab keys.
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && /^[1-9]$/.test(e.key)) {
      const idx = Number(e.key) - 1;
      if (idx < surfaceStore.tabs.length) {
        e.preventDefault();
        e.stopPropagation();
        surfaceStore.showIndex(idx);
      }
    }
  }
</script>

<svelte:window onkeydowncapture={onKeydown} />

<div class="workspace">
  {#each surfaceStore.mounted as id (id)}
    <SurfaceHost {id} active={surfaceStore.isActive(id)} />
  {/each}

  <!-- The active tab's shell waits for its backend before mounting (a product
       shell fires its first backend call on mount and doesn't retry), so the
       first open of a product shows this instead of an empty window. -->
  {#if surfaceStore.active && !surfaceStore.mounted.includes(surfaceStore.active)}
    <SurfaceBoot id={surfaceStore.active} phase="backend" />
  {/if}
</div>

<style>
  /* The positioning context for the stacked surfaces (each is absolutely
     inset-0, so only the active one is laid out visibly). */
  .workspace {
    position: relative;
    height: 100vh;
    overflow: hidden;
    background: var(--bg-elevated);
  }
</style>
