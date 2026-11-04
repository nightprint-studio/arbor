<script lang="ts">
  /** HirundoSidebar — the panel of the open section, under the shared header. */
  import { Code2, FolderTree, Globe } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import { HIRUNDO_SECTIONS } from '../hirundo-palette';
  import CollectionsPanel from './CollectionsPanel.svelte';
  import DetectedPanel from './DetectedPanel.svelte';
  import EnvironmentsPanel from './EnvironmentsPanel.svelte';

  const ICONS = { collections: FolderTree, detected: Code2, environments: Globe };
  const section = $derived(HIRUNDO_SECTIONS.find((s) => s.id === hirundoUiStore.section) ?? HIRUNDO_SECTIONS[0]);
</script>

<!-- The same panel header as Bennu's sidebars (PanelShell): one size, one type treatment, for every section. -->
<PanelShell title={section.label} scrollable={false}>
  {#snippet icon()}{@const Icon = ICONS[section.id]}<Icon size={14} />{/snippet}
  {#if hirundoUiStore.section === 'collections'}<CollectionsPanel />
  {:else if hirundoUiStore.section === 'detected'}<DetectedPanel />
  {:else}<EnvironmentsPanel />{/if}
</PanelShell>
