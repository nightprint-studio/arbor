<script lang="ts">
  /**
   * Drag-reorder picker for chunk-merge operations.
   *
   * Listens to `arbor://cloud-chunk-order-open` and opens with the items
   * supplied by the cloud-storage plugin. The user drags rows or uses the
   * hover-revealed arrows to set the merge order, then clicks Continue.
   * The chosen order is fired back into the plugin via firePluginAction
   * — mirrors the `arbor.ui.pick_file` round-trip pattern.
   *
   * Built on top of the standard <Modal> shell so the chrome, focus trap,
   * Escape handling, and visual rhythm match the rest of the app.
   *
   * The drag visualization uses an insertion bar (above or below the
   * hovered row depending on cursor position) instead of full-row tint —
   * cheaper to read at a glance and matches the way IDE file trees show
   * a drop target.
   */
  import { onMount } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { GripVertical, ChevronUp, ChevronDown, RotateCcw, ArrowUpAZ } from 'lucide-svelte';
  import { firePluginAction } from '$lib/ipc/plugin';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import ModalFooter from './ModalFooter.svelte';

  interface ChunkItem {
    label: string;
    path:  string;
    size?: number | null;
    /** Optional small text shown right of the label (e.g. last-modified date). */
    meta?: string | null;
  }
  interface OpenPayload {
    plugin_name: string;
    op_label?:   string;
    items:       ChunkItem[];
    action:      string;
    extra?:      Record<string, unknown>;
  }

  let visible      = $state(false);
  let pluginName   = $state('');
  let opLabel      = $state('');
  let items        = $state<ChunkItem[]>([]);
  let originalItems = $state<ChunkItem[]>([]);
  let action       = $state('');
  let extra        = $state<Record<string, unknown>>({});

  // Drag state — manual DnD to keep the dep surface small. We track:
  //   · dragIndex   — the row being dragged
  //   · overIndex   — the row the cursor is over
  //   · overSide    — "before" | "after" the over-row (decided from cursor Y)
  let dragIndex  = $state<number | null>(null);
  let overIndex  = $state<number | null>(null);
  let overSide   = $state<'before' | 'after'>('before');

  function openModal(p: OpenPayload) {
    pluginName    = p.plugin_name;
    opLabel       = p.op_label ?? 'Order chunks for merge';
    items         = p.items.slice();
    originalItems = p.items.slice();
    action        = p.action;
    extra         = p.extra ?? {};
    dragIndex     = null;
    overIndex     = null;
    visible       = true;
  }

  async function fireResult(ok: boolean) {
    visible = false;
    const payload: Record<string, unknown> = {
      ...extra,
      ok,
      ordered_paths: ok ? items.map(i => i.path) : [],
    };
    try {
      await firePluginAction(pluginName, action, JSON.stringify(payload));
    } catch { /* ignore */ }
  }

  // ── Reorder helpers ────────────────────────────────────────────────────
  function move(i: number, delta: number) {
    const j = i + delta;
    if (j < 0 || j >= items.length) return;
    const next = items.slice();
    [next[i], next[j]] = [next[j], next[i]];
    items = next;
  }
  function resetOrder() { items = originalItems.slice(); }
  function sortAlpha()  {
    items = items.slice().sort((a, b) => a.path.localeCompare(b.path));
  }
  function isReordered(): boolean {
    if (items.length !== originalItems.length) return true;
    for (let i = 0; i < items.length; i++) {
      if (items[i].path !== originalItems[i].path) return true;
    }
    return false;
  }

  // ── Drag mechanics ─────────────────────────────────────────────────────
  function onDragStart(e: DragEvent, i: number) {
    dragIndex = i;
    // dataTransfer is required on Firefox for `drag` events to fire.
    e.dataTransfer?.setData('text/plain', String(i));
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
  }
  function onDragOver(e: DragEvent, i: number) {
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    const row = e.currentTarget as HTMLElement;
    const rect = row.getBoundingClientRect();
    const localY = e.clientY - rect.top;
    overIndex = i;
    overSide  = localY < rect.height / 2 ? 'before' : 'after';
  }
  function onDragLeaveRow(i: number) {
    if (overIndex === i) overIndex = null;
  }
  function onDrop(e: DragEvent) {
    e.preventDefault();
    if (dragIndex == null || overIndex == null) {
      dragIndex = null; overIndex = null; return;
    }
    let target = overIndex + (overSide === 'after' ? 1 : 0);
    if (dragIndex < target) target -= 1;          // account for removal
    if (target !== dragIndex) {
      const next = items.slice();
      const [m]  = next.splice(dragIndex, 1);
      next.splice(target, 0, m);
      items = next;
    }
    dragIndex = null;
    overIndex = null;
  }
  function onDragEnd() { dragIndex = null; overIndex = null; }

  // ── Misc ───────────────────────────────────────────────────────────────
  function humanBytes(n: number | null | undefined): string {
    if (!n || n <= 0) return '';
    const u = ['B','KB','MB','GB','TB'];
    let i = 0; let v = n;
    while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
    return `${v.toFixed(i === 0 ? 0 : 1)} ${u[i]}`;
  }
  function splitPath(p: string): { dir: string; name: string } {
    const idx = Math.max(p.lastIndexOf('/'), p.lastIndexOf('\\'));
    if (idx < 0) return { dir: '', name: p };
    return { dir: p.slice(0, idx + 1), name: p.slice(idx + 1) };
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────
  let unlistenOpen: UnlistenFn | null = null;
  onMount(() => {
    let alive = true;
    (async () => {
      unlistenOpen = await listen<OpenPayload>('arbor://cloud-chunk-order-open', e => {
        if (alive) openModal(e.payload);
      });
    })();
    return () => { alive = false; unlistenOpen?.(); };
  });
</script>

{#if visible}
  <Modal
    onClose={() => fireResult(false)}
    size="md"
    topGap
    ariaLabel="Order chunks for merge"
  >
    {#snippet header()}
      <ModalHeader onClose={() => fireResult(false)}>
        <span class="modal-title">{opLabel}</span>
        <span class="count-badge">{items.length} chunks</span>
      </ModalHeader>
    {/snippet}

    <p class="hint">
      Drag rows or use the arrows to set the order in which the chunks will be
      concatenated. The <strong>top row is written first</strong>.
    </p>

    <ol
      class="chunk-list"
      role="list"
      ondragover={(e) => e.preventDefault()}
      ondrop={onDrop}
    >
      {#each items as it, i (it.path)}
        {@const np = splitPath(it.path)}
        <li
          class="row"
          class:dragging={dragIndex === i}
          class:over-before={overIndex === i && dragIndex !== i && overSide === 'before'}
          class:over-after={overIndex === i && dragIndex !== i && overSide === 'after'}
          draggable="true"
          ondragstart={(e) => onDragStart(e, i)}
          ondragover={(e) => onDragOver(e, i)}
          ondragleave={() => onDragLeaveRow(i)}
          ondragend={onDragEnd}
        >
          <span class="grip" aria-hidden="true">
            <GripVertical size={14} />
          </span>
          <span class="index">{i + 1}</span>
          <span class="name-block">
            <span class="name" title={it.path}>{np.name || it.label}</span>
            {#if np.dir}
              <span class="dir" title={np.dir}>{np.dir}</span>
            {/if}
          </span>
          {#if it.meta}
            <span class="meta">{it.meta}</span>
          {/if}
          {#if it.size}
            <span class="size">{humanBytes(it.size)}</span>
          {/if}
          <span class="move-cluster">
            <button
              class="move-btn"
              onclick={() => move(i, -1)}
              disabled={i === 0}
              aria-label="Move up"
              title="Move up"
            >
              <ChevronUp size={12} />
            </button>
            <button
              class="move-btn"
              onclick={() => move(i, +1)}
              disabled={i === items.length - 1}
              aria-label="Move down"
              title="Move down"
            >
              <ChevronDown size={12} />
            </button>
          </span>
        </li>
      {/each}
    </ol>

    {#snippet footer()}
      <ModalFooter align="between">
        <span class="footer-left">
          <button
            class="btn-ghost link-btn"
            onclick={sortAlpha}
            title="Sort by path A → Z"
          >
            <ArrowUpAZ size={12} />
            Sort A–Z
          </button>
          <button
            class="btn-ghost link-btn"
            onclick={resetOrder}
            disabled={!isReordered()}
            title="Restore the initial order"
          >
            <RotateCcw size={12} />
            Reset
          </button>
        </span>
        <span class="footer-right">
          <button class="btn-ghost" onclick={() => fireResult(false)}>Cancel</button>
          <button class="btn-primary" onclick={() => fireResult(true)}>Continue</button>
        </span>
      </ModalFooter>
    {/snippet}
  </Modal>
{/if}

<style>
  /* ── Header ──────────────────────────────────────────────────────────── */
  .modal-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .count-badge {
    margin-left: 8px;
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--bg-tertiary);
    color: var(--text-secondary);
    font-size: 10.5px;
    font-weight: 500;
    line-height: 1.4;
  }

  /* ── Hint ────────────────────────────────────────────────────────────── */
  .hint {
    margin: 0 0 12px;
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--text-secondary);
  }
  .hint strong { color: var(--text-primary); font-weight: 500; }

  /* ── Chunk list ──────────────────────────────────────────────────────── */
  .chunk-list {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    overflow: hidden;
    background: var(--bg-secondary);
  }
  .row {
    display: grid;
    grid-template-columns: 22px 24px 1fr auto auto auto;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-base);
    font-size: 12px;
    position: relative;
    transition: background 100ms ease;
  }
  .row:last-child { border-bottom: none; }
  .row:hover { background: var(--bg-hover); }
  .row.dragging { opacity: 0.4; }

  /* Insertion indicator — thin accent bar at the top or bottom edge of the
     hovered row. Sits absolutely so it doesn't shift surrounding rows. */
  .row.over-before::before,
  .row.over-after::after {
    content: '';
    position: absolute;
    left: 8px;
    right: 8px;
    height: 2px;
    background: var(--accent);
    border-radius: 2px;
    pointer-events: none;
  }
  .row.over-before::before { top: -1px; }
  .row.over-after::after   { bottom: -1px; }

  .grip {
    color: var(--text-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: grab;
  }
  .grip:active { cursor: grabbing; }

  .index {
    color: var(--text-muted);
    font-family: var(--font-code);
    font-size: 10.5px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .name-block {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.3;
  }
  .name {
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dir {
    color: var(--text-muted);
    font-family: var(--font-code);
    font-size: 10px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    color: var(--text-secondary);
    font-family: var(--font-code);
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .size {
    color: var(--text-secondary);
    font-family: var(--font-code);
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    min-width: 56px;
    text-align: right;
  }

  .move-cluster {
    display: inline-flex;
    flex-direction: column;
    gap: 1px;
    opacity: 0;
    transition: opacity 120ms ease;
  }
  .row:hover .move-cluster,
  .move-cluster:focus-within { opacity: 1; }

  .move-btn {
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-muted);
    border-radius: 3px;
    cursor: pointer;
    padding: 1px 3px;
    display: flex;
    align-items: center;
    justify-content: center;
    line-height: 0;
  }
  .move-btn:hover:not(:disabled) {
    color: var(--text-primary);
    background: var(--bg-tertiary);
    border-color: var(--border-color);
  }
  .move-btn:disabled { opacity: 0.3; cursor: default; }

  /* ── Footer clusters ─────────────────────────────────────────────────── */
  .footer-left {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .footer-right {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .link-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 8px;
    font-size: 11px;
    color: var(--text-secondary);
  }
  .link-btn:disabled {
    opacity: 0.4;
    cursor: default;
    pointer-events: none;
  }
</style>
