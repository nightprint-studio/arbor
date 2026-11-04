<script lang="ts">
  /**
   * RequestTabs — the strip of open requests. A dot marks unsaved edits; a tab opened with a single click is
   * a **preview** (italic) that the next single click replaces, until it is edited or double-clicked.
   */
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { methodColor, methodShort } from '../request-model';

  const items = $derived<TabItem[]>(
    hirundoStore.tabs.map((t) => ({ id: t.id, label: t.request.name || 'Untitled', title: t.path ?? 'Not saved yet', data: t })),
  );
</script>

<Tabs
  {items}
  value={hirundoStore.activeId}
  variant="panel"
  size="sm"
  closable
  overflow
  ariaLabel="Open requests"
  addLabel="New request"
  onSelect={(id) => hirundoStore.activate(id)}
  onClose={(id) => hirundoStore.closeTab(id)}
  onAdd={() => hirundoStore.newRequest(hirundoStore.tree[0]?.path ?? null)}
>
  {#snippet itemContent({ item })}
    {@const tab = hirundoStore.tabs.find((t) => t.id === item.id)}
    {#if tab}
      <span class="rt-method" style:color={methodColor(tab.request.method)}>{methodShort(tab.request.method)}</span>
      <span class="rt-name" class:preview={tab.preview}>{item.label}</span>
      {#if hirundoStore.dirty(tab)}<span class="rt-dirty" title="Unsaved changes">●</span>{/if}
    {/if}
  {/snippet}
</Tabs>

<style>
  .rt-method { font: 600 10px var(--font-mono); }
  .rt-name { max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rt-name.preview { font-style: italic; }
  .rt-dirty { font-size: 9px; color: var(--warning); }
</style>
