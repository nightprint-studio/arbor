<script lang="ts">
  /**
   * ExplorerWindow — standalone shell for the dedicated File Explorer window
   * (opened via the OS-global Ctrl+Shift+E shortcut). It is NOT the full Arbor
   * app: it boots only the theme / appearance / animation config and the
   * global Tooltip + Toast hosts the explorer needs, then mounts
   * FileExplorerModal in `standalone` mode (frameless, full-window — no modal
   * backdrop, its own titlebar + WindowControls).
   *
   * Each window is its own JS context, so the stores here are independent of
   * the main window's — AppShell's onMount never runs in this window, hence the
   * minimal local bootstrap below.
   */
  import { onMount } from 'svelte';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import FileExplorerModal from '$lib/components/shared/FileExplorerModal.svelte';
  import Tooltip from '$lib/components/shared/Tooltip.svelte';
  import ToastItem from '$lib/components/shared/Toast.svelte';

  onMount(() => {
    // Repaint with the active theme + apply persisted user config locally.
    themeStore.init();
    void appearanceStore.loadConfig();
    void animStore.loadConfig();
  });
</script>

<div class="explorer-window">
  <FileExplorerModal standalone />
</div>

<Tooltip />

<!-- Minimal toast host: the explorer surfaces errors (move/copy failed, …)
     via uiStore.showToast; AppShell's full feed stack doesn't run here. -->
<div class="explorer-toasts" aria-live="polite" aria-atomic="false">
  {#each uiStore.toasts as toast (toast.id)}
    <ToastItem {toast} />
  {/each}
</div>

<style>
  .explorer-window {
    position: fixed;
    inset: 0;
    background: var(--bg-elevated);
    overflow: hidden;
  }
  .explorer-toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: var(--z-toast, 9999);
    pointer-events: none;
  }
  .explorer-toasts > :global(*) { pointer-events: auto; }
</style>
