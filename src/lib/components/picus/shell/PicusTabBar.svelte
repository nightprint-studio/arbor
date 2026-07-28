<script lang="ts">
  /**
   * Picus tab strip — the centre area's open documents.
   *
   * Built on the shared `Tabs` widget (panel variant: reorder, close, overflow,
   * "+"), with one Picus-specific piece in the `itemContent` snippet: a tab
   * bound to a database wears that connection's colour as its leading dot, so
   * "which database is this query about" is answered without reading a word.
   */
  import {
    FileText, Table2, Play, FormInput, Layers, Eye, ListOrdered, Zap,
    X, XCircle, ArrowRightFromLine, Copy,
  } from 'lucide-svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { connectionsStore, connectionColorVar } from '$lib/stores/picus/connections.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import type { PicusTab, TabKind } from '$lib/types/picus';

  const ICONS: Record<TabKind, any> = {
    generate: FormInput,
    query: Play,
    table: Table2,
    file: FileText,
    inventory: Layers,
  };

  /** A `table` tab covers four object kinds — the icon says which. */
  const OBJECT_ICONS = {
    table: Table2,
    view: Eye,
    sequence: ListOrdered,
    trigger: Zap,
  } as const;

  function iconFor(tab: PicusTab) {
    if (tab.kind === 'table' && tab.objectKind) return OBJECT_ICONS[tab.objectKind];
    return ICONS[tab.kind];
  }

  // ── Context menu ────────────────────────────────────────────────────────────
  let menu = $state<{ x: number; y: number; tab: PicusTab } | null>(null);

  const menuItems = $derived.by<MenuItem[]>(() => {
    if (!menu) return [];
    const t = menu.tab;
    const pinned = t.kind === 'generate';
    const others = picusTabsStore.closableCount(t.id);
    const right = picusTabsStore.closableToRight(t.id);
    return [
      {
        id: 'close', label: 'Close', icon: X, shortcut: 'Ctrl+W',
        disabled: pinned,
      },
      {
        id: 'close-others', label: `Close others${others ? ` (${others})` : ''}`,
        icon: XCircle, disabled: others === 0,
      },
      {
        id: 'close-right', label: `Close to the right${right ? ` (${right})` : ''}`,
        icon: ArrowRightFromLine, disabled: right === 0,
      },
      {
        id: 'close-all', label: 'Close all', icon: XCircle,
        disabled: picusTabsStore.closableCount() === 0,
      },
      { id: 'sep', label: '', separator: true },
      { id: 'copy-name', label: 'Copy name', icon: Copy },
    ];
  });

  function onMenuSelect(id: string) {
    const t = menu?.tab;
    menu = null;
    if (!t) return;
    switch (id) {
      case 'close':        picusTabsStore.close(t.id); break;
      case 'close-others': picusTabsStore.closeOthers(t.id); break;
      case 'close-right':  picusTabsStore.closeToRight(t.id); break;
      case 'close-all':    picusTabsStore.closeAll(); break;
      case 'copy-name':
        void navigator.clipboard
          .writeText(t.file ?? t.table ?? t.title)
          .then(() => toastStore.show('Name copied.', 'success'))
          .catch(() => toastStore.show('Could not reach the clipboard.', 'error'));
        break;
    }
  }

  const items = $derived<TabItem[]>(
    picusTabsStore.tabs.map((t) => ({
      id: t.id,
      label: t.title,
      icon: iconFor(t),
      iconSize: 13,
      // The generator is pinned — it holds work in progress.
      closable: t.kind !== 'generate',
      title: t.file ?? t.table ?? t.title,
      data: t,
    })),
  );
</script>

<Tabs
  {items}
  value={picusTabsStore.activeId}
  variant="panel"
  size="sm"
  draggable
  overflow
  closable
  ariaLabel="Open documents"
  addLabel="New query (Ctrl+T)"
  onSelect={(id) => picusTabsStore.select(id)}
  onClose={(id) => picusTabsStore.close(id)}
  onAdd={() => picusTabsStore.openQuery()}
  onReorder={(from, to) => picusTabsStore.reorder(from, to)}
  onContextMenu={(_id, item, e) => (menu = { x: e.clientX, y: e.clientY, tab: item.data as PicusTab })}
>
  {#snippet itemContent({ item })}
    {@const tab = item.data as PicusTab}
    {@const conn = connectionsStore.byId(tab.connectionId)}
    {@const Icon = item.icon}
    {#if conn}
      <span class="ptab-dot" style:background={connectionColorVar(conn)} aria-hidden="true"></span>
    {:else if Icon}
      <Icon size={13} />
    {/if}
    <span class="ptab-label">{item.label}</span>
    {#if tab.dialect}
      <PicusDialectChip engine={tab.dialect} terse />
    {/if}
    {#if tab.dirty}
      <span class="ptab-dirty" title="Unsaved changes" aria-label="Unsaved changes"></span>
    {/if}
  {/snippet}
</Tabs>

{#if menu}
  <ContextMenu
    items={menuItems}
    x={menu.x}
    y={menu.y}
    onSelect={onMenuSelect}
    onClose={() => (menu = null)}
  />
{/if}

<style>
  .ptab-dot {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    flex-shrink: 0;
  }
  .ptab-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ptab-dirty {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--warning);
    flex-shrink: 0;
  }
</style>
