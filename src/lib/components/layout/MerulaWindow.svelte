<script lang="ts">
  /**
   * MerulaWindow — standalone boot shell for the dedicated merula window (music
   * live-coding DAW). Mirrors ExplorerWindow: it is NOT the full Arbor app, it
   * just boots the theme / appearance / animation config and the global Tooltip
   * host the shared widgets need, then mounts MerulaShell.
   *
   * This boot wrapper is the Arbor↔merula bridge, so it may touch Arbor stores
   * (theme, etc.). The merula UI itself (MerulaShell + everything under
   * components/merula/) imports ONLY from shared/ui/, keeping merula extractable
   * as a standalone app (see design/merula/ui.md).
   *
   * Each window is its own JS context, so these stores are independent of the
   * main window's — AppShell's onMount never runs here, hence the local boot.
   */
  import { onMount } from 'svelte';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { explorerStore } from '$lib/stores/explorer.svelte';
  import { profileStore } from '$lib/stores/profiles.svelte';
  import MerulaShell from '$lib/components/merula/MerulaShell.svelte';
  import Tooltip from '$lib/components/shared/Tooltip.svelte';
  import MerulaBeDownOverlay from '$lib/components/shared/MerulaBeDownOverlay.svelte';
  import FeedbackHost from '$lib/feedback/FeedbackHost.svelte';
  import FeedbackStatusButtons from '$lib/feedback/FeedbackStatusButtons.svelte';

  onMount(() => {
    // Repaint with the active theme + apply persisted user config locally.
    themeStore.init();
    void appearanceStore.loadConfig();
    void animStore.loadConfig();
    // So "Reveal in file explorer" on a finished transfer honours the user's
    // built-in-vs-OS choice (Settings → File Explorer) in this window too.
    void explorerStore.loadConfig();
    // Profiles are global (shared across all windows): load the list/active and
    // subscribe to `arbor://profile-switched` so this window reloads on a switch
    // triggered here or from any other window.
    void profileStore.init();
  });
</script>

<!-- The Arbor-specific feedback badges (jobs · notifications) are passed down
     as the footer's right-cluster snippet, so MerulaShell/MerulaFooter stay free
     of Arbor store imports. Clicking them opens the overlays rendered by
     <FeedbackHost> below. -->
<MerulaShell>
  {#snippet footerExtra()}
    <FeedbackStatusButtons transfers />
  {/snippet}
</MerulaShell>

<Tooltip />

<!-- Fatal "audio backend stopped" overlay: self-subscribes to
     arbor://merula-be-down (scoped to this window) and blocks it until the user
     restarts. Mirrors the Corvus window's CorvusBeDownOverlay. -->
<MerulaBeDownOverlay />

<!-- Full feedback surface for the merula window: toasts (the file/folder + save
     pickers still surface errors via uiStore.showToast → this window's toast
     store), plus notifications and progress operations addressed to this
     window via `target = "merula"`. Items emitted without a target go to the
     main window instead. -->
<FeedbackHost id="merula" />
