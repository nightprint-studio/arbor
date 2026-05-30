<script lang="ts">
  import { graphColumnsStore } from '$lib/stores/graphColumns.svelte';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import type { GraphColumnId } from '$lib/types/config';
  import { tooltip } from '$lib/actions/tooltip';

  let { gridTemplate }: { gridTemplate: string } = $props();

  const LABELS: Record<GraphColumnId, string> = {
    graph:   'Graph',
    refs:    'Branches / Tags',
    subject: 'Subject',
    author:  'Author',
    date:    'Date',
    hash:    'Hash',
  };

  // ── Reorder (mouse-drag, mirroring shared/ui/Tabs.svelte) ────────────────
  // HTML5 native DnD shows a "forbidden" cursor over most drop targets and
  // produces awkward cross-element handoffs in WebView2; the Tabs widget
  // sidesteps this with raw mousedown → window mousemove/mouseup, which
  // gives full control over both the cursor and the drop-marker placement.
  let dragFromIndex     = $state<number | null>(null);
  let insertBeforeIndex = $state<number | null>(null);
  let suppressNextClick = false;
  let stripEl: HTMLElement | undefined;

  /** Visible columns in render order — the source of truth for both the
   *  drop math here and the parent component's grid template. */
  const visibleCols = $derived(graphColumnsStore.columns.filter(c => c.visible));

  function startReorder(e: MouseEvent, fromIndex: number) {
    if (e.button !== 0) return;
    // Resize grips are children of the header cell; let them handle their
    // own mousedown without starting a reorder.
    if ((e.target as HTMLElement).closest('.resize-grip')) return;
    const startX = e.clientX;
    let active = false;
    function onMove(ev: MouseEvent) {
      if (!active) {
        if (Math.abs(ev.clientX - startX) < 5) return;
        active = true;
        dragFromIndex = fromIndex;
        document.body.style.cursor = 'grabbing';
      }
      insertBeforeIndex = calcInsert(ev.clientX);
    }
    function onUp() {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup',   onUp);
      document.body.style.cursor = '';
      const wasActive   = active;
      const finalInsert = insertBeforeIndex;
      active = false;
      dragFromIndex = null;
      insertBeforeIndex = null;
      if (!wasActive || finalInsert === null) return;
      suppressNextClick = true;
      // `finalInsert` is the pre-removal insertion index. After splicing the
      // source out, indices to its right shift left by 1.
      const to = finalInsert <= fromIndex ? finalInsert : finalInsert - 1;
      if (to !== fromIndex) {
        const col = visibleCols[fromIndex];
        if (col) {
          // Translate visible-list moveTo into store-array moveTo by mapping
          // through the visible→store index correspondence.
          const cols = graphColumnsStore.columns;
          const storeFrom = cols.findIndex(c => c.id === col.id);
          const destVisIdx = Math.max(0, Math.min(visibleCols.length - 1, to));
          const destVisId  = visibleCols[destVisIdx]?.id ?? visibleCols[visibleCols.length - 1].id;
          let storeTo = cols.findIndex(c => c.id === destVisId);
          if (storeTo > storeFrom) storeTo -= 1;
          graphColumnsStore.moveTo(col.id, storeTo);
        }
      }
    }
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup',   onUp);
  }

  function calcInsert(x: number): number {
    if (!stripEl) return 0;
    const els = stripEl.querySelectorAll<HTMLElement>('[data-col-idx]');
    for (let i = 0; i < els.length; i++) {
      const r = els[i].getBoundingClientRect();
      if (x < r.left + r.width / 2) {
        return parseInt(els[i].getAttribute('data-col-idx') ?? '0', 10);
      }
    }
    return visibleCols.length;
  }

  // ── Resize state ─────────────────────────────────────────────────────────
  // Pointer events on the right-edge grip. Tracks the starting width and
  // pointer X, updates the store live (the store itself debounces persists).
  let resizing = $state<{ id: GraphColumnId; startX: number; startW: number } | null>(null);

  function startResize(e: PointerEvent, id: GraphColumnId, currentW: number) {
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    resizing = { id, startX: e.clientX, startW: currentW };
  }
  function onResizeMove(e: PointerEvent) {
    if (!resizing) return;
    const delta = e.clientX - resizing.startX;
    graphColumnsStore.setColumnWidth(resizing.id, resizing.startW + delta);
  }
  function endResize(e: PointerEvent) {
    if (!resizing) return;
    try { (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId); } catch {}
    resizing = null;
  }

  // ── Context menu ─────────────────────────────────────────────────────────
  let menu = $state<{ x: number; y: number; colId: GraphColumnId } | null>(null);
  const menuItems = $derived<MenuItem[]>(buildMenuItems());

  function buildMenuItems(): MenuItem[] {
    if (!menu) return [];
    const cols    = graphColumnsStore.columns;
    const visible = cols.filter(c => c.visible);
    const visI    = visible.findIndex(c => c.id === menu!.colId);
    const hidden  = cols.filter(c => !c.visible);
    const items: MenuItem[] = [
      { id: 'move-left',  label: 'Move left',  disabled: visI <= 0 },
      { id: 'move-right', label: 'Move right', disabled: visI < 0 || visI >= visible.length - 1 },
      { id: 'sep1', label: '', separator: true },
      { id: 'hide', label: `Hide ${LABELS[menu.colId]}`, disabled: visible.length <= 1 },
    ];
    if (hidden.length > 0) {
      items.push({ id: 'sep2', label: '', separator: true });
      items.push({ id: 'header-show', label: 'Show', header: true });
      for (const h of hidden) {
        items.push({ id: `show:${h.id}`, label: LABELS[h.id] });
      }
    }
    items.push({ id: 'sep3', label: '', separator: true });
    items.push({ id: 'reset', label: 'Reset to defaults' });
    return items;
  }

  function onHeaderContextMenu(e: MouseEvent, colId: GraphColumnId) {
    e.preventDefault();
    e.stopPropagation();
    menu = { x: e.clientX, y: e.clientY, colId };
  }
  function onMenuSelect(id: string) {
    if (!menu) return;
    const target = menu.colId;
    if (id === 'move-left')  graphColumnsStore.moveLeft(target);
    else if (id === 'move-right') graphColumnsStore.moveRight(target);
    else if (id === 'hide')  graphColumnsStore.setVisible(target, false);
    else if (id === 'reset') graphColumnsStore.reset();
    else if (id.startsWith('show:')) {
      graphColumnsStore.setVisible(id.slice(5) as GraphColumnId, true);
    }
    menu = null;
  }

  function suppressClickIfNeeded(e: MouseEvent) {
    if (suppressNextClick) {
      e.preventDefault();
      e.stopPropagation();
      suppressNextClick = false;
    }
  }
</script>

<svelte:window
  onpointermove={onResizeMove}
  onpointerup={endResize}
/>

<div
  class="graph-header"
  class:resizing={resizing != null}
  style="grid-template-columns: {gridTemplate}"
  role="row"
  bind:this={stripEl}
>
  {#each visibleCols as col, i (col.id)}
    <div
      class="header-cell"
      class:cell-graph={col.id === 'graph'}
      class:dragging={dragFromIndex === i}
      class:drop-before={dragFromIndex !== null && insertBeforeIndex === i && i !== dragFromIndex && i !== dragFromIndex + 1}
      class:drop-after={dragFromIndex !== null && insertBeforeIndex === visibleCols.length && i === visibleCols.length - 1 && dragFromIndex !== visibleCols.length - 1}
      data-col={col.id}
      data-col-idx={i}
      role="columnheader"
      onmousedown={(e) => startReorder(e, i)}
      onclick={suppressClickIfNeeded}
      oncontextmenu={(e) => onHeaderContextMenu(e, col.id)}
    >
      <span
        class="label"
        use:tooltip={col.id === 'graph'
          ? 'Adaptive — auto-sizes to the lanes; drag the right edge to set the cap'
          : col.id === 'subject'
            ? 'Flex column — fills the remaining row width'
            : col.id === 'hash'
              ? 'Auto-sized to fit the short OID'
              : LABELS[col.id]}
      >{LABELS[col.id]}</span>
      {#if col.id !== 'subject' && col.id !== 'hash'}
        <!-- Resize grip omitted on `subject` (pure flex) and `hash`
             (auto-sized to content): neither has a stored width that
             would have a visible effect when changed. -->
        <div
          class="resize-grip"
          role="separator"
          aria-orientation="vertical"
          aria-label={`Resize ${LABELS[col.id]} column`}
          onpointerdown={(e) => startResize(e, col.id, col.width)}
          onmousedown={(e) => e.stopPropagation()}
          use:tooltip={col.id === 'graph' ? 'Drag to set the max graph width' : 'Drag to resize'}
        ></div>
      {/if}
    </div>
  {/each}
</div>

{#if menu}
  <ContextMenu
    x={menu.x}
    y={menu.y}
    items={menuItems}
    onSelect={onMenuSelect}
    onClose={() => (menu = null)}
  />
{/if}

<style>
  .graph-header {
    display: grid;
    align-items: stretch;
    position: sticky;
    top: 0;
    z-index: 3;
    height: 26px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
    user-select: none;
    flex-shrink: 0;
  }
  .graph-header.resizing { cursor: col-resize; }

  .header-cell {
    position: relative;
    display: flex;
    align-items: center;
    padding: 0 8px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.3px;
    text-transform: uppercase;
    cursor: grab;
    border-right: 1px solid var(--border-subtle);
    /* Important: no `overflow: hidden` here — the resize grip lives at
       `right: -3px` and would otherwise be clipped on every cell (most
       visibly on the rightmost one, where the user reported it). The
       `.label` child handles its own text overflow. */
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .header-cell.cell-graph { cursor: grab; }
  .header-cell:hover { background: var(--bg-hover); color: var(--text-primary); }
  .header-cell:active { cursor: grabbing; }
  .header-cell.dragging {
    opacity: 0.55;
    background: var(--bg-hover);
  }

  .label {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    pointer-events: none;
  }

  .resize-grip {
    position: absolute;
    top: 0;
    right: -3px;
    width: 6px;
    height: 100%;
    cursor: col-resize;
    z-index: 2;
    background: transparent;
    transition: background var(--transition-fast);
  }
  .resize-grip:hover,
  .resize-grip:active {
    background: color-mix(in srgb, var(--accent) 35%, transparent);
  }

  /* Drop-target indicators painted as pseudo-elements on the cell itself
     instead of inline children — that way they don't consume an extra
     grid track and shove the rest of the header sideways during drag. */
  .header-cell.drop-before::before,
  .header-cell.drop-after::after {
    content: '';
    position: absolute;
    top: 2px;
    bottom: 2px;
    width: 2px;
    background: var(--accent);
    border-radius: 1px;
    z-index: 3;
    pointer-events: none;
  }
  .header-cell.drop-before::before { left: 0; }
  .header-cell.drop-after::after   { right: 0; }
</style>
