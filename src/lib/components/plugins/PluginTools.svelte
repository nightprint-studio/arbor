<!--
  PluginTools — the two modal doors of the plugin host, mounted in the one order that works.

  A shell that hosts plugins needs both the Plugin Manager and the Marketplace, and they are
  not independent: **Browse** inside the manager opens the marketplace without closing the
  manager. Both are `<Modal>`s on `--z-modal-bg`, so nothing but document order decides which
  one stacks on top — mount the marketplace first and Browse opens it underneath, which reads
  as a button that does nothing. That is a real bug Bennu shipped, found by clicking Browse.

  Two shells already mounted this pair by hand and a third would have made the same mistake,
  so the order lives here instead of in a comment each shell is trusted to have read.

  ## What is a prop and what is not

  The marketplace flag is cross-product — `uiStore.marketplaceOpen`, the same one Corvus's
  Command Palette and every Browse button flip — so this component owns it outright. There is
  nothing for a shell to pass and nothing it could pass wrongly.

  The manager flag is per-product (Corvus routes it through `activePanel`, Bennu keeps its own
  boolean), so it comes in as a prop.

  ## Mount `PluginOverlays` after this

  A plugin action is very often fired from inside the Plugin Manager — that is what a
  contributed row button is. The form it opens has to paint over the manager, so the shell
  mounts `<PluginOverlays />` after `<PluginTools />`. Same document-order rule, one level up.

  ## What is deliberately NOT here

  The Plugin Logs panel. It is the third door, but it is a docked bottom panel rather than a
  modal, so where it goes is each product's tool-window layout — Corvus's bottom sections,
  Bennu's dock. Pulling it in here would mean this component knowing about two different
  docks, which is the opposite of the point.
-->
<script lang="ts">
  import Lazy from '$lib/components/shared/Lazy.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';

  let {
    managerOpen,
    onCloseManager,
  }: {
    /** Whether the Plugin Manager is showing — per-product state. */
    managerOpen: boolean;
    /** Called when the manager asks to close; the shell owns what "closed" means. */
    onCloseManager: () => void;
  } = $props();
</script>

<!-- Lazy on both: between them they drag in the registry catalogue, six confirm modals and a
     heap of icons that no startup path needs. -->
<Lazy
  gate={managerOpen}
  loader={() => import('./PluginPanel.svelte')}
  onClose={onCloseManager}
/>
<!-- Second, and that is the whole reason this file exists. See the header. -->
<Lazy
  gate={uiStore.marketplaceOpen}
  loader={() => import('./MarketplaceModal.svelte')}
  onClose={() => uiStore.closeMarketplace()}
/>
