<script lang="ts">
  /**
   * One plugin panel, whichever kind it is.
   *
   * `arbor.ui.add_sidebar{ kind = "tree" | "form" }` picks between two renderers, and every
   * place that shows a plugin panel had to make that choice itself — Corvus's shell made it
   * four times (left card, right card, bottom dock, and the `isTreeKind` helper behind them).
   * A second product with a rail would have made it four more, which is four more chances for
   * one of them to render the wrong one and for a plugin author to be told that trees do not
   * work *there*.
   *
   * So the choice lives here, resolved from the registration itself. A key with no live
   * registration renders nothing: a panel whose plugin was just disabled is a stale key, not
   * an error, and an empty card says that better than a form waiting for content that will
   * never arrive.
   */
  import { findSidebarSection } from '$lib/contributions/sidebar';
  import PluginSidebarPanel from './PluginSidebarPanel.svelte';
  import PluginTreeSidebar from './PluginTreeSidebar.svelte';

  interface Props {
    pluginName: string;
    panelId:    string;
    /** Docked at the bottom rather than in a side card — the panel renders the standard
     *  bottom chrome bar (title + close X) instead of its own header. */
    bottomMode?: boolean;
    /**
     * How the dock is closed, when the consumer owns that state.
     *
     * Defaults to Corvus's shared `uiStore` — which is a no-op in a product whose bottom
     * dock is its own store, and that is exactly how the X on this header did nothing in
     * Bennu. A product that mounts this passes its own closer.
     */
    onClose?:    () => void;
  }
  let { pluginName, panelId, bottomMode = false, onClose }: Props = $props();

  const section = $derived(findSidebarSection({ plugin_name: pluginName, panel_id: panelId }));
</script>

{#if section?.kind === 'tree'}
  <PluginTreeSidebar {pluginName} {panelId} {bottomMode} {onClose} />
{:else if section}
  <PluginSidebarPanel {pluginName} {panelId} {bottomMode} {onClose} />
{/if}
