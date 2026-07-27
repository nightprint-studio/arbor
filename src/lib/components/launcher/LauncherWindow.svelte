<script lang="ts">
  /**
   * LauncherWindow — boot shell for the Arbor Canopy launcher (the entry-point
   * home, JetBrains-Toolbox-like). Mounts on the `main` (and `launcher`) window
   * instead of the Git AppShell, which now opens in its own `corvus` window.
   *
   * Like MerulaWindow/ExplorerWindow it is NOT the full Arbor app: it boots only
   * the theme / appearance / animation config and the global Tooltip + feedback
   * host, then mounts LauncherShell. Because the launcher replaces BootSplash on
   * the main window, it also fires the `frontend_ready` handshake so the plugin
   * boot thread doesn't sit out its 5s safety timeout before loading plugins.
   */
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import LauncherShell from '$lib/components/launcher/LauncherShell.svelte';
  import Tooltip from '$lib/components/shared/Tooltip.svelte';
  import FeedbackHost from '$lib/feedback/FeedbackHost.svelte';
  import { createNativeMenuPublisher } from '$lib/utils/native-menu';

  // macOS: the menu bar is app-wide and the launcher has no menu of its own, so
  // it claims the baseline bar (App · Edit · Window · Help). Without this, the
  // last product window's File/Tools menus would linger while the launcher is
  // focused. No-op elsewhere.
  const publishNativeMenu = createNativeMenuPublisher('Arbor');

  onMount(() => {
    themeStore.init();
    void appearanceStore.loadConfig();
    void animStore.loadConfig();
    publishNativeMenu([]);
    // Release the plugin boot thread (no BootSplash on the launcher window).
    invoke('frontend_ready').catch(() => { /* legacy backend without handshake */ });
  });
</script>

<LauncherShell />

<Tooltip />

<!-- Feedback addressed to the launcher (toasts from product opens, etc.). Items
     without a target still go to the main window — which is this launcher. -->
<FeedbackHost id="main" />
