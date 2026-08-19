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
  import WelcomeHome from '$lib/components/launcher/WelcomeHome.svelte';
  import Tooltip from '$lib/components/shared/Tooltip.svelte';
  import { mcpStore } from '$lib/stores/mcp.svelte';
  import type { McpAuditEntry } from '$lib/types/mcp';
  import { listen } from '@tauri-apps/api/event';
  import FeedbackHost from '$lib/feedback/FeedbackHost.svelte';
  import { createNativeMenuPublisher } from '$lib/utils/native-menu';
  import { syncWindowTitle } from '$lib/utils/window-title.svelte';
  import { surfaceStore } from '$lib/stores/surfaces.svelte';

  // macOS: the menu bar is app-wide and the launcher has no menu of its own, so
  // it claims the baseline bar (App · Edit · Window · Help). Without this, the
  // last product window's File/Tools menus would linger while the launcher is
  // focused. No-op elsewhere.
  const publishNativeMenu = createNativeMenuPublisher('Arbor');

  // Names the window while the home surface is on screen — in the container
  // that means "back to the home tab" reads as Welcome in the taskbar and the
  // switcher, instead of whichever product was there before.
  syncWindowTitle('Welcome', () => null, { active: () => surfaceStore.hasFocus('home') });

  // Claim the baseline macOS menu bar only while Canopy is the surface on
  // screen — in the container the product shells are mounted too, and whoever
  // publishes last owns the app-wide bar.
  $effect(() => {
    if (surfaceStore.hasFocus('home')) publishNativeMenu([]);
  });

  onMount(() => {
    themeStore.init();
    void appearanceStore.loadConfig();
    void animStore.loadConfig();
    // Release the plugin boot thread (no BootSplash on the launcher window).
    invoke('frontend_ready').catch(() => { /* legacy backend without handshake */ });

    // The AI tool surface's SETTINGS live on the home surface — this window when
    // products get their own, the Welcome tab when they are tabs — because that is
    // where Arbor's own settings live rather than any one product's. The consent
    // prompt and the call log do not: both ride in `GlobalOverlays`, in every window,
    // because this one is closed as soon as a product tab opens.
    void mcpStore.load();
  });
</script>

<!-- Two different homes, deliberately. The launcher's OWN window is Canopy: a
     small, self-contained world with its circuit-tree and its own palette. The
     home tab of the tabbed container sits inside a full-size Arbor window next
     to product tabs, so it gets an Arbor welcome page instead — same chrome,
     same theme tokens, same widgets as every other panel. -->
{#if surfaceStore.inContainer}
  <WelcomeHome />
{:else}
  <LauncherShell />
{/if}

<Tooltip />

<!-- Feedback addressed to the launcher (toasts from product opens, etc.). Items
     without a target still go to the main window — which is this launcher. -->
<FeedbackHost id="main" />
