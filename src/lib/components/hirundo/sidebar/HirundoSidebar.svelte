<script lang="ts">
  /** HirundoSidebar — the header of the open section and the panel behind it. */
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import { HIRUNDO_SECTIONS } from '../hirundo-palette';
  import CollectionsPanel from './CollectionsPanel.svelte';
  import DetectedPanel from './DetectedPanel.svelte';
  import EnvironmentsPanel from './EnvironmentsPanel.svelte';

  const section = $derived(HIRUNDO_SECTIONS.find((s) => s.id === hirundoUiStore.section) ?? HIRUNDO_SECTIONS[0]);
</script>

<div class="hs">
  <PaneHeader title={section.label} />
  {#if hirundoUiStore.section === 'collections'}<CollectionsPanel />
  {:else if hirundoUiStore.section === 'detected'}<DetectedPanel />
  {:else}<EnvironmentsPanel />{/if}
</div>

<style>
  .hs { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; }
</style>
