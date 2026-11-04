<script lang="ts">
  /**
   * HirundoWindow — standalone boot shell for the dedicated Hirundo window (HTTP client). Not the full Arbor
   * app: it boots the theme / appearance / animation config and mounts `HirundoShell`. `hirundo-be` spawns
   * off-thread and can race the first read, so the project restore runs again on `arbor://hirundo-be-up`.
   */
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import HirundoShell from './HirundoShell.svelte';

  onMount(() => {
    themeStore.init();
    void appearanceStore.loadConfig();
    void animStore.loadConfig();
    void hirundoStore.init();
    let unlisten: (() => void) | undefined;
    void listen('arbor://hirundo-be-up', () => { if (!hirundoStore.project) void hirundoStore.init(); }).then((u) => (unlisten = u));
    return () => unlisten?.();
  });
</script>

<div class="hirundo-window">
  <HirundoShell />
</div>

<style>
  .hirundo-window { height: 100vh; display: flex; flex-direction: column; background: var(--bg-elevated); overflow: hidden; }
</style>
