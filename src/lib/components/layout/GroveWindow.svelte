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
  import GroveShell from '$lib/components/grove/GroveShell.svelte';
  import Tooltip from '$lib/components/shared/Tooltip.svelte';
  import FeedbackHost from '$lib/feedback/FeedbackHost.svelte';
  import FeedbackStatusButtons from '$lib/feedback/FeedbackStatusButtons.svelte';

  onMount(() => {
    // Repaint with the active theme + apply persisted user config locally.
    themeStore.init();
    void appearanceStore.loadConfig();
    void animStore.loadConfig();
  });
</script>

<!-- The Arbor-specific feedback badges (jobs · notifications) are passed down
     as the footer's right-cluster snippet, so GroveShell/GroveFooter stay free
     of Arbor store imports. Clicking them opens the overlays rendered by
     <FeedbackHost> below. -->
<GroveShell>
  {#snippet footerExtra()}
    <FeedbackStatusButtons />
  {/snippet}
</GroveShell>

<Tooltip />

<!-- Full feedback surface for the grove window: toasts (the file/folder + save
     pickers still surface errors via uiStore.showToast → this window's toast
     store), plus notifications and progress operations addressed to this
     window via `target = "grove"`. Items emitted without a target go to the
     main window instead. -->
<FeedbackHost id="grove" />
