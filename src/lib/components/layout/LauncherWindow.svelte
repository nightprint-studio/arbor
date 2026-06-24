<script lang="ts">
  /**
   * LauncherWindow — boot shell for the Arbor Canopy launcher (the entry-point
   * home, JetBrains-Toolbox-like). Mounts on the `main` (and `launcher`) window
   * instead of the Git AppShell, which now opens in its own `corvus` window.
   *
   * Like NemusWindow/ExplorerWindow it is NOT the full Arbor app: it boots only
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

  onMount(() => {
    themeStore.init();
    void appearanceStore.loadConfig();
    void animStore.loadConfig();
    // Release the plugin boot thread (no BootSplash on the launcher window).
    invoke('frontend_ready').catch(() => { /* legacy backend without handshake */ });
  });
</script>

<LauncherShell />

<Tooltip />

<!-- Feedback addressed to the launcher (toasts from product opens, etc.). Items
     without a target still go to the main window — which is this launcher. -->
<FeedbackHost id="main" />
