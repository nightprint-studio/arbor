<script lang="ts">
  /**
   * GarrulusWindow — standalone boot shell for the dedicated Garrulus window
   * (notes). Mirrors PicusWindow / TytoWindow: it is NOT the full Arbor app, it
   * only boots the theme / appearance / animation config and mounts
   * `GarrulusShell`.
   *
   * `garrulus-be` serves everything the product does — the vault, its notes and
   * types, the link/search index, sync and the filesystem watcher. Each window is
   * its own JS context, so any store this window grows is independent of the main
   * window's.
   */
  import { onMount } from 'svelte';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import GarrulusShell from './GarrulusShell.svelte';

  onMount(() => {
    themeStore.init();
    void appearanceStore.loadConfig();
    void animStore.loadConfig();

    // `garrulus-be` spawns off-thread and races this window's first reads, so the
    // vault/config loads that land here will need re-running on
    // `arbor://garrulus-be-up` (see `onGarrulusBeUp` in `$lib/ipc/garrulus`).
    // Nothing is read yet — the listener goes in with the store that reads.
  });
</script>

<div class="garrulus-window">
  <GarrulusShell />
</div>

<style>
  .garrulus-window {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg-elevated);
    overflow: hidden;
  }
</style>
