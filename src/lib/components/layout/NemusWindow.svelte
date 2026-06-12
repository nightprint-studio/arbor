<script lang="ts">
  /**
   * NemusWindow — standalone boot shell for the dedicated nemus window (music
   * live-coding DAW). Mirrors ExplorerWindow: it is NOT the full Arbor app, it
   * just boots the theme / appearance / animation config and the global Tooltip
   * host the shared widgets need, then mounts NemusShell.
   *
   * This boot wrapper is the Arbor↔nemus bridge, so it may touch Arbor stores
   * (theme, etc.). The nemus UI itself (NemusShell + everything under
   * components/nemus/) imports ONLY from shared/ui/, keeping nemus extractable
   * as a standalone app (see design/nemus/ui.md).
   *
   * Each window is its own JS context, so these stores are independent of the
   * main window's — AppShell's onMount never runs here, hence the local boot.
   */
  import { onMount } from 'svelte';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { explorerStore } from '$lib/stores/explorer.svelte';
  import NemusShell from '$lib/components/nemus/NemusShell.svelte';
  import Tooltip from '$lib/components/shared/Tooltip.svelte';
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
  });
</script>

<!-- The Arbor-specific feedback badges (jobs · notifications) are passed down
     as the footer's right-cluster snippet, so NemusShell/NemusFooter stay free
     of Arbor store imports. Clicking them opens the overlays rendered by
     <FeedbackHost> below. -->
<NemusShell>
  {#snippet footerExtra()}
    <FeedbackStatusButtons transfers />
  {/snippet}
</NemusShell>

<Tooltip />

<!-- Full feedback surface for the nemus window: toasts (the file/folder + save
     pickers still surface errors via uiStore.showToast → this window's toast
     store), plus notifications and progress operations addressed to this
     window via `target = "nemus"`. Items emitted without a target go to the
     main window instead. -->
<FeedbackHost id="nemus" />
