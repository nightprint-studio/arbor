<script lang="ts">
  /**
   * RequestTabs — the strip of open requests. A dot marks unsaved edits; a tab opened with a single click is
   * a **preview** (italic) that the next single click replaces, until it is edited or double-clicked.
   */
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import { ArrowRightToLine, Copy, CopyPlus, FolderSearch, Pencil, X } from 'lucide-svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { hirundoContextMenuStore } from '$lib/stores/hirundo/contextmenu.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoCommands as c } from '../hirundo-commands';
  import { methodColor, methodShort } from '../request-model';

  const items = $derived<TabItem[]>(
    hirundoStore.tabs.map((t) => ({ id: t.id, label: t.request.name || 'Untitled', title: t.path ?? 'Not saved yet', data: t })),
  );

  /** Right click: the verbs act on the tab you clicked, not the active one. Same order as Bennu and Corvus. */
  function onContext(id: string, _item: TabItem, e: MouseEvent) {
    const tabs = hirundoStore.tabs;
    const i = tabs.findIndex((t) => t.id === id);
    const saved = !!tabs[i]?.path;
    const menu: MenuItem[] = [
      { id: 'close', label: 'Close', icon: X, shortcut: 'Ctrl+W' },
      { id: 'close-others', label: 'Close Others', icon: X, disabled: tabs.length < 2 },
      { id: 'close-all', label: 'Close All', icon: X },
      { id: 'close-right', label: 'Close to the Right', icon: ArrowRightToLine, disabled: i < 0 || i >= tabs.length - 1 },
      { id: 'close-saved', label: 'Close Without Changes', icon: X, disabled: tabs.every((t) => hirundoStore.dirty(t)) },
      { id: 's1', label: '', separator: true },
      { id: 'rename', label: 'Rename…', icon: Pencil, shortcut: 'F2' },
      { id: 'duplicate', label: 'Duplicate', icon: CopyPlus, shortcut: 'Ctrl+D' },
      { id: 's2', label: '', separator: true },
      { id: 'copy-path', label: 'Copy Path', icon: Copy, disabled: !saved },
      { id: 'reveal', label: 'Show in File Explorer', icon: FolderSearch, disabled: !saved },
    ];
    hirundoContextMenuStore.show(e.clientX, e.clientY, menu, (action) => {
      switch (action) {
        case 'close': hirundoStore.closeTabs('one', id); break;
        case 'close-others': hirundoStore.closeTabs('others', id); break;
        case 'close-all': hirundoStore.closeTabs('all', id); break;
        case 'close-right': hirundoStore.closeTabs('right', id); break;
        case 'close-saved': hirundoStore.closeTabs('saved', id); break;
        case 'rename': c.renameTab(id); break;
        case 'duplicate': { const t = tabs.find((x) => x.id === id); if (t) void hirundoStore.duplicate(t); break; }
        case 'copy-path': c.copyTabPath(id); break;
        case 'reveal': void c.revealTab(id); break;
      }
    });
  }
</script>

<Tabs
  {items}
  value={hirundoStore.activeId}
  variant="panel"
  size="sm"
  closable
  draggable
  overflow
  ariaLabel="Open requests"
  addLabel="New request"
  onSelect={(id) => hirundoStore.activate(id)}
  onClose={(id) => hirundoStore.closeTabs('one', id)}
  onReorder={(from, to) => hirundoStore.reorderTab(from, to)}
  onContextMenu={onContext}
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
