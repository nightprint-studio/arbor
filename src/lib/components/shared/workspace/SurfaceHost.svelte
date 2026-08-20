<script lang="ts">
  /**
   * One tab's product shell inside the container.
   *
   * Loads the shell with the SAME dynamic import the standalone windows use
   * (`+page.svelte`), so a product pays for its chunk only if the user actually
   * opens its tab, and its module-level side effects (config reads, backend
   * handshakes) fire on first visit rather than at container boot.
   *
   * Once mounted a shell is kept alive and merely hidden when its tab is not on
   * screen: remounting would re-run the whole product boot on every tab switch,
   * and the user expects a tab to be exactly where they left it.
   */
  import type { Component } from 'svelte';
  import type { SurfaceId } from '$lib/stores/surfaces.svelte';
  import SurfaceBoot from './SurfaceBoot.svelte';

  interface Props {
    id: SurfaceId;
    /** Whether this surface is the one on screen. */
    active: boolean;
  }

  let { id, active }: Props = $props();

  const LOADERS: Record<SurfaceId, () => Promise<{ default: Component }>> = {
    home:   () => import('$lib/components/launcher/LauncherWindow.svelte'),
    corvus: () => import('$lib/components/corvus/AppShell.svelte'),
    bennu:  () => import('$lib/components/bennu/BennuWindow.svelte'),
    merula: () => import('$lib/components/merula/MerulaWindow.svelte'),
    picus:  () => import('$lib/components/picus/PicusWindow.svelte'),
    garrulus: () => import('$lib/components/garrulus/GarrulusWindow.svelte'),
  };

  let Shell = $state<Component | null>(null);
  LOADERS[id]().then((m) => { Shell = m.default; });
</script>

<!-- `display: none` rather than an `{#if}`: the shell stays mounted (and its
     state alive) while its tab is in the background. `inert` keeps the hidden
     subtree out of the tab order and away from the accessibility tree. -->
<div class="surface" class:hidden={!active} inert={!active} aria-hidden={!active}>
  {#if Shell}
    <Shell />
  {:else}
    <!-- The SECOND half of one wait, not a screen of its own: the container showed the same
         thing while the backend came up, and only the line at the bottom changes here. -->
    <SurfaceBoot {id} phase="interface" />
  {/if}
</div>

<style>
  .surface {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
  }
  .surface.hidden { display: none; }

</style>
