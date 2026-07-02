<script lang="ts">
  /**
   * TytoWindow — standalone boot shell for the dedicated Tyto window (screen
   * recorder). Mirrors ExplorerWindow / MerulaWindow: it is NOT the full Arbor
   * app, it just boots the theme / appearance / animation config, the Tyto
   * launcher-config (the opt-in global shortcut), and the global Tooltip + Toast
   * hosts, then mounts TytoShell.
   *
   * NB: the capture/encode backend does not exist yet — TytoShell is a mocked UI.
   * Each window is its own JS context, so these stores are independent of the
   * main window's (AppShell's onMount never runs here).
   */
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { tytoConfigStore } from '$lib/stores/tyto/config.svelte';
  import { recorderStore } from '$lib/stores/tyto/recorder.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import TytoShell from './TytoShell.svelte';
  import Tooltip from '$lib/components/shared/Tooltip.svelte';
  import ToastItem from '$lib/components/shared/Toast.svelte';

  onMount(() => {
    themeStore.init();
    void appearanceStore.loadConfig();
    void animStore.loadConfig();
    // This window owns the Tyto launcher-side config (the global-shortcut toggle).
    void tytoConfigStore.loadConfig();
    // Bring the recorder store onto tyto-be: subscribe to attach/detach + fetch
    // config / sources / captures (best-effort — mock stays until the engine lands).
    recorderStore.initBackend();

    // Re-sync persisted settings when the window regains focus, so a control can't
    // show a stale value (the Sitta "force refresh" pattern).
    let unfocus: (() => void) | undefined;
    void getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => { if (focused) void recorderStore.reloadConfig(); })
      .then((f) => (unfocus = f));
    return () => unfocus?.();
  });
</script>

<div class="tyto-window">
  <TytoShell />
</div>

<Tooltip />

<!-- Minimal toast host (settings save errors, mock actions). -->
<div class="tyto-toasts" aria-live="polite" aria-atomic="false">
  {#each uiStore.toasts as toast (toast.id)}
    <ToastItem {toast} />
  {/each}
</div>

<style>
  .tyto-window {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg-elevated);
    overflow: hidden;
  }
  .tyto-toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: var(--z-toast, 9999);
    pointer-events: none;
  }
  .tyto-toasts > :global(*) { pointer-events: auto; }
</style>
