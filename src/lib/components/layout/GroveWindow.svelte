<script lang="ts">
  /**
   * GroveWindow — standalone boot shell for the dedicated grove window (music
   * live-coding DAW). Mirrors ExplorerWindow: it is NOT the full Arbor app, it
   * just boots the theme / appearance / animation config and the global Tooltip
   * host the shared widgets need, then mounts GroveShell.
   *
   * This boot wrapper is the Arbor↔grove bridge, so it may touch Arbor stores
   * (theme, etc.). The grove UI itself (GroveShell + everything under
   * components/grove/) imports ONLY from shared/ui/, keeping grove extractable
   * as a standalone app (see design/grove/ui.md).
   *
   * Each window is its own JS context, so these stores are independent of the
   * main window's — AppShell's onMount never runs here, hence the local boot.
   */
  import { onMount } from 'svelte';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import GroveShell from '$lib/components/grove/GroveShell.svelte';
  import Tooltip from '$lib/components/shared/Tooltip.svelte';
  import ToastItem from '$lib/components/shared/Toast.svelte';

  onMount(() => {
    // Repaint with the active theme + apply persisted user config locally.
    themeStore.init();
    void appearanceStore.loadConfig();
    void animStore.loadConfig();
  });
</script>

<GroveShell />

<Tooltip />

<!-- Minimal toast host: the file/folder + save pickers (FileExplorerModal)
     surface errors via uiStore.showToast; AppShell's full feed stack doesn't
     run in this window. Mirrors ExplorerWindow. -->
<div class="grove-toasts" aria-live="polite" aria-atomic="false">
  {#each uiStore.toasts as toast (toast.id)}
    <ToastItem {toast} />
  {/each}
</div>

<style>
  .grove-toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: var(--z-toast, 9999);
    pointer-events: none;
  }
  .grove-toasts > :global(*) { pointer-events: auto; }
</style>
