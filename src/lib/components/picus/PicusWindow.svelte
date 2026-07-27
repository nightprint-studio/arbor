<script lang="ts">
  /**
   * PicusWindow — standalone boot shell for the dedicated Picus window (SQL
   * studio). Mirrors TytoWindow / MerulaWindow: it is NOT the full Arbor app, it
   * only boots the theme / appearance / animation config and mounts `PicusShell`.
   *
   * The Picus backend (`picus-be`) does not exist yet: every store falls back to
   * the fixtures in `ipc/picus/mock`, the same staging Tyto's control panel went
   * through before its capture engine landed. Each window is its own JS context,
   * so these stores are independent of the main window's.
   */
  import { onMount } from 'svelte';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import PicusShell from './PicusShell.svelte';

  onMount(() => {
    themeStore.init();
    void appearanceStore.loadConfig();
    void animStore.loadConfig();
  });
</script>

<div class="picus-window">
  <PicusShell />
</div>

<style>
  .picus-window {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg-elevated);
    overflow: hidden;
  }
</style>
