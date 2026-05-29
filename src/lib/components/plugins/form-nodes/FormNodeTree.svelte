<!--
  FormNodeTree — the `tree` field node (single + multi select), now also the
  dynamic "data tree": lazy children, scoped selection/expand events, keyboard
  navigation and row virtualization for large trees.

  Value model (unchanged from the static tree): value-bearing. The selected
  node `value` (or `value[]` in `multi`) lives in `ctx.values[name]` and is
  submitted like any field. The legacy whole-form `change_action` still fires
  for single-select master/detail.

  Dynamic opt-ins (all additive — absent ⇒ today's static behaviour):
    · lazy + on_expand — expanding a node that advertises `has_children` but
      has no loaded `children` fires the scoped `on_expand` slot
      (`{ id, value, path }`) and shows a spinner row until the plugin patches
      the children in (merge `children` + clear `loading`, addressed by the
      row's stable `id`). See plugin-ui-dispatch-and-patch §4 (data_tree).
    · on_select — scoped selection event (ships the new value), preferred over
      `change_action` when both are set.
    · virtualize_threshold / row_height — window the flattened visible rows
      when they exceed the threshold (fixed row height, like VirtualHunk).

  Rendering is driven by a flat list of the currently-visible rows so that
  virtualization and roving-focus keyboard nav share one model.
-->
<script lang="ts">
  import { ChevronDown, Check, Loader2 } from 'lucide-svelte';
  import PluginIcon from '$lib/components/plugins/PluginIcon.svelte';
  import TypePill   from '$lib/components/shared/internal/TypePill.svelte';
  import type { FormNode, FormTreeNode } from '$lib/types/plugin';
  import type { FormNodeCtx } from './ctx';

  interface Props {
    node: FormNode;
    ctx:  FormNodeCtx;
  }
  let { node, ctx }: Props = $props();

  const n     = $derived(node as any);
  const field = $derived(n.name as string);
  const multi = $derived(!!n.multi);
  const lazy  = $derived(!!n.lazy);

  const rowH      = $derived(typeof n.row_height === 'number' ? n.row_height : 24);
  const threshold = $derived(typeof n.virtualize_threshold === 'number' ? n.virtualize_threshold : 400);

  // ── Selection helpers ────────────────────────────────────────────────────
  const sel = $derived(
    multi
      ? (Array.isArray(ctx.values[field]) ? (ctx.values[field] as string[]) : [])
      : (ctx.values[field] as string),
  );
  function isSelected(v: string): boolean {
    return multi ? (sel as string[]).includes(v) : (sel as string) === v;
  }

  function keyOf(t: FormTreeNode): string {
    return ctx.treeKey(field, t.value);
  }
  function childrenLoaded(t: any): boolean {
    return Array.isArray(t.children) && t.children.length > 0;
  }
  function expandable(t: any): boolean {
    return childrenLoaded(t) || !!t.has_children;
  }

  // ── Flatten the currently-visible (expanded) tree into a row list ─────────
  interface Row {
    tnode:      FormTreeNode;
    depth:      number;
    key:        string;
    path:       string[];      // ancestor values incl. self
    expandable: boolean;
    expanded:   boolean;
    loading:    boolean;       // synthetic spinner placeholder row?
  }

  const rows = $derived.by<Row[]>(() => {
    const out: Row[] = [];
    const walk = (list: FormTreeNode[] | undefined, depth: number, parent: string[]) => {
      if (!Array.isArray(list)) return;
      for (const t of list) {
        const path = [...parent, t.value];
        const exp  = expandable(t);
        const open = exp && !!ctx.treeExpanded[keyOf(t)];
        out.push({ tnode: t, depth, key: keyOf(t), path, expandable: exp, expanded: open, loading: false });
        if (open) {
          if (childrenLoaded(t)) {
            walk(t.children, depth + 1, path);
          } else if ((t as any).loading || (lazy && (t as any).has_children)) {
            out.push({
              tnode: t, depth: depth + 1, key: keyOf(t) + '::__loading',
              path, expandable: false, expanded: false, loading: true,
            });
          }
        }
      }
    };
    walk(n.nodes, 0, []);
    return out;
  });

  // Don't re-fire on_expand for a row whose children are already in flight.
  const firedExpand = new Set<string>();

  function fireExpand(t: any, path: string[]) {
    const key = keyOf(t);
    if (firedExpand.has(key) || !n.on_expand) return;
    firedExpand.add(key);
    ctx.handleScopedDispatch(
      n.id, 'expand', n.on_expand,
      { id: t.id ?? t.value, value: t.value, path },
      { stateKeys: n.scope_state },
    );
  }

  function toggle(row: Row) {
    const t = row.tnode as any;
    const key = keyOf(t);
    const next = !ctx.treeExpanded[key];
    ctx.treeExpanded[key] = next;
    if (next && lazy && t.has_children && !childrenLoaded(t)) fireExpand(t, row.path);
  }

  function activate(row: Row) {
    const t = row.tnode as any;
    if (t.group) {                       // group header → toggle instead of select
      if (row.expandable) toggle(row);
      return;
    }
    if (multi) {
      const arr = sel as string[];
      ctx.values[field] = arr.includes(t.value)
        ? arr.filter(v => v !== t.value)
        : [...arr, t.value];
    } else {
      ctx.values[field] = t.value;
    }
    ctx.notifyChange(field, ctx.values[field]);
    // Scoped on_select wins; otherwise the legacy whole-form change_action
    // (single-select only — the multi selection shape differs).
    if (n.on_select) {
      ctx.handleScopedDispatch(n.id, 'select', n.on_select, ctx.values[field], { stateKeys: n.scope_state });
    } else if (!multi && n.change_action) {
      ctx.handleButtonAction(n.change_action, false, { value: t.value });
    }
  }

  // ── Virtualization (fixed-height rows, windowed like VirtualHunk) ─────────
  let scrollEl  = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewportH = $state(0);
  const overscan = 8;

  const virtual = $derived(rows.length > threshold);

  $effect(() => {
    const el = scrollEl;
    if (!el || !virtual) return;
    const ro = new ResizeObserver((entries) => {
      for (const e of entries) viewportH = e.contentRect.height;
    });
    ro.observe(el);
    return () => ro.disconnect();
  });

  const startIdx = $derived(virtual ? Math.max(0, Math.floor(scrollTop / rowH) - overscan) : 0);
  const endIdx   = $derived(virtual ? Math.min(rows.length, Math.ceil((scrollTop + viewportH) / rowH) + overscan) : rows.length);
  const topPad   = $derived(virtual ? startIdx * rowH : 0);
  const botPad   = $derived(virtual ? Math.max(0, (rows.length - endIdx) * rowH) : 0);
  const visible  = $derived(rows.slice(startIdx, endIdx));

  // Optional scoped range hint for plugins that fetch by window.
  function onScroll(e: Event) {
    if (virtual) scrollTop = (e.currentTarget as HTMLDivElement).scrollTop;
    if (n.on_scroll_range) {
      ctx.handleScopedDispatch(n.id, 'scroll_range', n.on_scroll_range,
        { start: startIdx, end: endIdx, total: rows.length }, { stateKeys: n.scope_state });
    }
  }

  // ── Keyboard navigation (roving) ─────────────────────────────────────────
  let activeIdx = $state(0);

  // Keep the active index in range as the row list changes.
  $effect(() => {
    if (activeIdx > rows.length - 1) activeIdx = Math.max(0, rows.length - 1);
  });

  function ensureVisible(i: number) {
    const el = scrollEl;
    if (!el) return;
    const top = i * rowH;
    const bot = top + rowH;
    if (top < el.scrollTop) el.scrollTop = top;
    else if (bot > el.scrollTop + el.clientHeight) el.scrollTop = bot - el.clientHeight;
  }
  function move(to: number) {
    activeIdx = Math.max(0, Math.min(rows.length - 1, to));
    ensureVisible(activeIdx);
  }
  function parentIndex(i: number): number {
    const d = rows[i]?.depth ?? 0;
    for (let j = i - 1; j >= 0; j--) if (rows[j].depth < d) return j;
    return i;
  }

  function onKeydown(e: KeyboardEvent) {
    const row = rows[activeIdx];
    switch (e.key) {
      case 'ArrowDown': e.preventDefault(); move(activeIdx + 1); break;
      case 'ArrowUp':   e.preventDefault(); move(activeIdx - 1); break;
      case 'Home':      e.preventDefault(); move(0); break;
      case 'End':       e.preventDefault(); move(rows.length - 1); break;
      case 'ArrowRight':
        e.preventDefault();
        if (row?.expandable && !row.expanded) toggle(row);
        else move(activeIdx + 1);
        break;
      case 'ArrowLeft':
        e.preventDefault();
        if (row?.expandable && row.expanded) toggle(row);
        else move(parentIndex(activeIdx));
        break;
      case 'Enter':
      case ' ':
        // When a row button has focus its native click already activates it;
        // only the container itself (e.g. just Tab-focused) activates via key,
        // so Enter never double-fires (which would no-op a multi toggle).
        if (document.activeElement === scrollEl && row && !row.loading) {
          e.preventDefault();
          activate(row);
        }
        break;
    }
  }

  const treeId = $derived(n.id as string);
</script>

<div
  class="pf-field {n.class ?? ''}"
  class:pf-field-highlight={n.highlight}
  style={n.style}
>
  {#if n.label}
    <!-- svelte-ignore a11y_label_has_associated_control -->
    <label class="pf-label">
      {n.label}
      {#if n.required}<span class="pf-required" aria-hidden="true">*</span>{/if}
    </label>
  {/if}

  <div
    class="pf-tree pf-tree-dyn"
    class:pf-tree-bordered={n.bordered}
    style={n.max_height ? `max-height:${n.max_height}` : (n.height ? `max-height:${typeof n.height === 'number' ? n.height + 'px' : n.height}` : '')}
    role="tree"
    tabindex="0"
    aria-multiselectable={multi}
    aria-activedescendant={rows[activeIdx] ? `${treeId}__${rows[activeIdx].key}` : undefined}
    bind:this={scrollEl}
    onscroll={onScroll}
    onkeydown={onKeydown}
  >
    {#if topPad}<div style="height:{topPad}px"></div>{/if}
    {#each visible as row, i (row.key)}
      {@const idx = startIdx + i}
      {@const t = row.tnode as any}
      {#if row.loading}
        <div class="pf-tree-row pf-tree-loading" style="padding-left:{row.depth * 14 + 4}px; height:{rowH}px">
          <Loader2 size={11} class="pf-tree-spin" />
          <span class="pf-tree-loading-text">Loading…</span>
        </div>
      {:else}
        {@const selected = isSelected(t.value)}
        <div
          id="{treeId}__{row.key}"
          class="pf-tree-row"
          class:pf-tree-row-active={idx === activeIdx}
          style="padding-left:{row.depth * 14 + 4}px; height:{rowH}px"
          role="treeitem"
          aria-level={row.depth + 1}
          aria-expanded={row.expandable ? row.expanded : undefined}
          aria-selected={selected}
        >
          {#if row.expandable}
            <button
              class="pf-tree-chev"
              type="button"
              tabindex="-1"
              aria-label={row.expanded ? 'Collapse' : 'Expand'}
              onclick={() => { activeIdx = idx; toggle(row); }}
            ><ChevronDown size={10} class={row.expanded ? '' : 'pf-chev-collapsed'} /></button>
          {:else}
            <span class="pf-tree-chev-spacer"></span>
          {/if}
          {#if t.icon}
            <span class="pf-tree-icon"><PluginIcon name={t.icon} size={11} /></span>
          {/if}
          <button
            class="pf-tree-label"
            class:pf-tree-label-group={t.group}
            class:pf-tree-label-selected={selected}
            type="button"
            tabindex="-1"
            disabled={t.group && !row.expandable}
            onclick={() => { activeIdx = idx; activate(row); }}
          >
            {#if multi && !t.group}
              <span class="pf-tree-cb" class:checked={selected}>
                {#if selected}<Check size={9} />{/if}
              </span>
            {/if}
            <span class="pf-tree-label-text">
              <span>{t.label}</span>
              {#if t.description}
                <span class="pf-tree-desc">{t.description}</span>
              {/if}
            </span>
            {#if t.loading}
              <Loader2 size={10} class="pf-tree-spin" />
            {/if}
            {#if t.tag}
              <span class="pf-cfg-tag pf-cfg-tag-{t.tag_variant ?? 'neutral'} pf-tree-tag">{t.tag}</span>
            {/if}
          </button>
        </div>
      {/if}
    {/each}
    {#if botPad}<div style="height:{botPad}px"></div>{/if}
  </div>

  {#if ctx.validationErrors[field]}
    <span class="pf-validation-error">{ctx.validationErrors[field]}</span>
  {/if}
  {#if n.hint}
    <span class="pf-hint">{n.hint}</span>
  {/if}
  {#if n.pill}
    <TypePill label={n.pill} kind={n.pill_kind ?? n.pill} tooltip={n.pill_tooltip} />
  {/if}
</div>

<style>
  /* Dynamic tree adds a scroll viewport + fixed-height rows on top of the
     shared pf-tree-* visuals (form-node-styles.css). */
  .pf-tree-dyn {
    overflow: auto;
    outline: none;
  }
  .pf-tree-dyn:focus-visible {
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 45%, transparent);
    border-radius: var(--radius-sm, 4px);
  }
  .pf-tree-row-active {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border-radius: var(--radius-sm, 4px);
  }
  .pf-tree-row.pf-tree-loading {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
    font-size: 11px;
  }
  .pf-tree-loading-text {
    font-style: italic;
  }
  :global(.pf-tree-spin) {
    animation: pf-tree-spin 1s linear infinite;
    color: var(--accent);
    flex-shrink: 0;
  }
  @keyframes pf-tree-spin {
    to { transform: rotate(360deg); }
  }
</style>
