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
  import { onMount, setContext } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { explorerStore } from '$lib/stores/sitta/explorer.svelte';
  import { explorerProjects, EXPLORER_PROJECTS_KEY } from '$lib/stores/sitta/explorerProjects.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import Tooltip from '$lib/components/shared/Tooltip.svelte';
  import ToastItem from '$lib/components/shared/Toast.svelte';

  // Provide the git Projects source to the explorer below (and any picker it
  // opens) — the standalone window surfaces projects just like the Corvus window.
  setContext(EXPLORER_PROJECTS_KEY, explorerProjects);

  onMount(() => {
    // Repaint with the active theme + apply persisted user config locally.
    themeStore.init();
    void appearanceStore.loadConfig();
    void animStore.loadConfig();
    // This is the only window that owns the sitta-be config — opt in before loading
    // so the launcher window never reads/writes the (legitimately down) sitta backend.
    explorerStore.enableSitta();
    void explorerStore.loadConfig();
    // This window surfaces the git projects sidebar — load the registry source
    // through sitta-be (no corvus-be here), kept live on registry-changed.
    void explorerProjects.load({ local: true });

    // `sitta-be` spawns off-thread, racing this window's first reads: if it
    // attaches after we already read, the 14 sitta-owned settings stayed on
    // defaults AND the Projects sidebar came back empty. Re-read both once the
    // backend signals it's routable so they take.
    const unlisten = listen('arbor://sitta-be-up', () => {
      void explorerStore.loadConfig();
      void explorerProjects.refresh();
    });
    return () => { void unlisten.then((off) => off()); };
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
  /* Viewport-anchored flex root. `100vh` (not `height: 100%`): the page mounts
     through app.html's `display: contents` wrapper, and a percentage height
     resolving across a `display:contents` ancestor is unreliable on WebView2 —
     it left the descendants collapsed. `100vh` measures the viewport directly,
     and everything below fills with `flex: 1` (no percentage-height chain, no
     `position: fixed`). */
  .explorer-window {
    height: 100vh;
    display: flex;
    flex-direction: column;
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
