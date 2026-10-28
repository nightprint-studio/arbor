<script lang="ts" module>
  /**
   * Tabs — shared horizontal tab strip.
   *
   * Variants:
   *   - 'underline' (default): sober nav-style strip with an animated accent
   *     underline on the active tab. Used by panel/modal sub-navs
   *     (PipelinesPanel, MrSidebar, ReflogPanel, StatsOverlay).
   *   - 'pill': segmented pill-style buttons; the active one fills with the
   *     accent. Used for binary/ternary scope toggles (BranchCleanupModal).
   *   - 'panel': JetBrains-style file-tab strip with lifted active tab and
   *     rounded top corners; used for app-level tab containers (RepoTabBar,
   *     TerminalPanel).
   *   - 'solid': loud pill-style segmented control with a fully-filled accent
   *     active state (white foreground, bold, subtle shadow). For the primary
   *     view switcher inside studio-style modals (Tree / Text / Diff / Errors)
   *     where the current view must be unmistakable. Carries a thin background
   *     trough so it reads as a single capsule on busy headers.
   *
   * Optional features (all off by default):
   *   - draggable    — enables horizontal drag-to-reorder
   *   - overflow     — measure-based overflow that hides tabs beyond the
   *                    container width and surfaces them in a "+N" dropdown.
   *                    Tuned for the 'panel' variant.
   *   - closable     — show a close button on every tab. Per-item override
   *                    via TabItem.closable.
   *   - onAdd        — when defined, a "+" button appears at the trailing end
   *   - onContextMenu — fires on right-click on a tab; consumer renders the
   *                    actual ContextMenu (we just hand back coords + id).
   *
   * Item content can be customised via the `itemContent` snippet (the
   * default renders icon + label + optional badge/count). The wrapper
   * (interactive button, drag handlers, close X, accent underline) is always
   * supplied by the widget so consumers can't drift on a11y / reorder logic.
   */
  export interface TabItem {
    id:          string;
    label?:      string;
    /** Lucide (or any Svelte) icon component. */
    icon?:       any;
    iconSize?:   number;
    /** Marks a tab whose content is **not the user's own** — a read-only view of something
     *  from outside this workspace (a dependency's source, a vendored file). It is tinted
     *  differently in both states, selected and not: a badge alone is read once and then
     *  stops being noticed, while the thing it warns about — that edits here go nowhere —
     *  matters every time you look up at the strip.
     *
     *  `'test'` marks a tab holding a **test source**. Same idea, different fact and a much
     *  quieter one: nothing is wrong and nothing is read-only, it is simply a different kind of
     *  code, and a strip of eight tabs where three are tests reads better when you can see which
     *  three without reading eight filenames. Green, the colour the project tree already gives a
     *  test root. */
    tone?:       'default' | 'external' | 'test';
    /** CSS colour for the label alone — a status carried by the name itself (a file's git
     *  state), as opposed to `tone`, which restyles the whole tab. */
    labelColor?: string;
    /** Props handed to {@link icon}, for an icon component that needs more than a size —
     *  a file-type or symbol-kind icon is chosen by what it is *of*, not by which component
     *  it is. Without this a caller with one icon component and N variants has to bypass the
     *  whole item rendering through `itemContent` to pass a single string. */
    iconProps?:  Record<string, unknown>;
    /** Compact badge to the right of the label (e.g. unread count). */
    badge?:      string | number;
    /** Per-item override of the global `closable` prop. */
    closable?:   boolean;
    disabled?:   boolean;
    /** Native `title` tooltip on hover. */
    title?:      string;
    /** Arbitrary consumer payload — surfaced in event callbacks and the
     *  `itemContent` snippet so callers don't have to look the item up
     *  themselves. */
    data?:       any;
  }

  export type TabsVariant = 'underline' | 'pill' | 'panel' | 'solid';
  export type TabsSize    = 'sm' | 'md';
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import { tick } from 'svelte';
  import { X, Plus, ChevronDown } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  // The overflow chevron opens the shared menu rather than a private one: anchoring, outside-click,
  // Escape and arrow-key navigation are all things `Dropdown` already does, and a second hand-rolled
  // copy of them inside a widget that sits in every product is exactly the surface that drifts.
  import Dropdown from './Dropdown.svelte';
  import type { DropdownItem } from './Dropdown.svelte';
  import {
    clampOffset, insertBefore, landingIndex, settleOffset, shiftDirection, type TabSlot,
  } from './tabs-sort';
  import { tabDragStore } from '$lib/stores/tab-drag.svelte';

  interface Props {
    items:        TabItem[];
    value?:       string | null;
    variant?:     TabsVariant;
    size?:        TabsSize;
    /** Make all tabs share the available width equally. Useful on segmented
     *  toggles inside narrow modal columns. */
    fill?:        boolean;

    // ── optional features ───────────────────────────────────────────────────
    draggable?:    boolean;
    overflow?:     boolean;
    closable?:     boolean;

    // ── events ──────────────────────────────────────────────────────────────
    onSelect?:      (id: string, item: TabItem) => void;
    onClose?:       (id: string, item: TabItem) => void;
    onAdd?:         () => void;
    onReorder?:     (fromIndex: number, toIndex: number) => void;
    /** Names this strip as a place tabs come from and go to (`draggable` only). With it, a tab can be
     *  carried out of the strip: it follows the pointer freely, and a release over an element marked
     *  `data-tab-drop="<zone>"` for another zone calls {@link onDropOnZone}. */
    dropZone?:      string;
    onDropOnZone?:  (id: string, zone: string) => void;
    onContextMenu?: (id: string, item: TabItem, e: MouseEvent) => void;

    // ── presentation ────────────────────────────────────────────────────────
    addLabel?:    string;
    ariaLabel?:   string;
    /** Extra class on the root wrapper. */
    class?:       string;

    // ── snippets ────────────────────────────────────────────────────────────
    /** Override the inside of each tab (icon + label + badge by default).
     *  The close button, drag handle and accent underline are still managed
     *  by the widget, regardless of what this snippet renders. */
    itemContent?:     Snippet<[{ item: TabItem; active: boolean }]>;
    /** Extra trailing buttons rendered after the "+" add button. */
    trailingActions?: Snippet;
  }

  let {
    items,
    value           = null,
    variant         = 'underline',
    size            = 'md',
    fill            = false,
    draggable       = false,
    overflow        = false,
    closable        = false,
    onSelect,
    onClose,
    onAdd,
    onReorder,
    dropZone,
    onDropOnZone,
    onContextMenu,
    addLabel        = 'Add',
    ariaLabel,
    class:    rootClass = '',
    itemContent,
    trailingActions,
  }: Props = $props();

  // ── Drag & Drop ───────────────────────────────────────────────────────────
  //
  // A sortable, drawn the way a browser draws its own tab strip: the tab you hold follows the pointer,
  // lifted above the rest, and the tabs it passes slide aside to open the slot it would land in — so
  // what is on screen while you drag IS the order you will get. On release it slides the last few
  // pixels into the slot before the order is committed. The geometry is `tabs-sort.ts`; this is the
  // pointer handling and the transforms.
  //
  // Mouse events, not native drag-and-drop: Tauri's WebView2 swallows the DOM `drop`.
  let dragFromIndex = $state<number | null>(null);
  let dragOffset    = $state(0);
  let dragOffsetY   = $state(0);
  let dragFree      = $state(false);
  let dragInsert    = $state(0);
  let dragShift     = $state(0);
  let dragSettling  = $state(false);
  let suppressNextClick = false;
  let stripEl: HTMLElement | undefined;

  const SETTLE_MS = 170;

  function measureSlots(): TabSlot[] {
    if (!stripEl) return [];
    return [...stripEl.querySelectorAll<HTMLElement>('[data-tab-idx]:not(.tab-hidden)')].map((el) => {
      const r = el.getBoundingClientRect();
      return { index: parseInt(el.getAttribute('data-tab-idx') ?? '0', 10), left: r.left, width: r.width };
    });
  }

  // A strip that names a `dropZone` lets its tabs leave it: once the pointer is clear of the strip the
  // held tab follows it freely (both axes) and the others stop making room, and wherever the pointer is
  // over another zone — an element marked `data-tab-drop="<zone>"` — that zone lights up and a release
  // there hands the tab over (`onDropOnZone`). Released anywhere else, it slides back into its own strip.
  const LEAVE_MARGIN = 10;

  function zoneUnder(x: number, y: number): string | null {
    return document.elementFromPoint(x, y)?.closest('[data-tab-drop]')?.getAttribute('data-tab-drop') ?? null;
  }

  function startTabDrag(e: MouseEvent, fromIndex: number) {
    if (!draggable || e.button !== 0 || dragSettling) return;
    const startX = e.clientX;
    const startY = e.clientY;
    const tabId = items[fromIndex]?.id;
    let active = false;
    let slots: TabSlot[] = [];
    let gap = 0;
    let rawOffset = 0;
    let hotZone: string | null = null;
    // While the tab is carried away from its strip it is drawn as a copy fixed to the viewport: the
    // real tab sits inside containers that clip, and a tab that vanishes at the edge of its own pane
    // is a drag with no sign of where it is going.
    let ghost: HTMLElement | null = null;
    let grabX = 0;
    let grabY = 0;

    function showGhost(ev: MouseEvent) {
      if (!ghost) {
        const tab = stripEl?.querySelector<HTMLElement>(`[data-tab-idx="${fromIndex}"]`);
        const root = stripEl?.parentElement;
        if (!tab || !root) return;
        const r = tab.getBoundingClientRect();
        grabX = startX - r.left;
        grabY = startY - r.top;
        // The root's classes come along so the tab's scoped styles (`.tabs-panel .tabs-tab`) still match.
        const wrap = document.createElement('div');
        wrap.className = root.className;
        wrap.style.cssText = `position:fixed;z-index:10000;pointer-events:none;width:${r.width}px;height:${r.height}px;` +
          'display:block;opacity:0.96;box-shadow:0 8px 24px rgba(0,0,0,0.45);border-radius:6px;overflow:hidden;';
        const copy = tab.cloneNode(true) as HTMLElement;
        copy.style.cssText = 'transform:none;transition:none;width:100%;height:100%;';
        copy.classList.remove('tab-dragging', 'tab-ghosted');
        wrap.appendChild(copy);
        document.body.appendChild(wrap);
        ghost = wrap;
      }
      ghost.style.left = `${ev.clientX - grabX}px`;
      ghost.style.top = `${ev.clientY - grabY}px`;
    }
    function hideGhost() {
      ghost?.remove();
      ghost = null;
    }

    function recompute() {
      dragOffset = clampOffset(slots, fromIndex, rawOffset);
      const me = slots.find((s) => s.index === fromIndex);
      if (!me) return;
      dragInsert = insertBefore(slots, fromIndex, me.left + me.width / 2 + dragOffset);
    }
    /** Whether the pointer is clear of this strip — free to carry the tab somewhere else. */
    function outsideStrip(ev: MouseEvent): boolean {
      if (dropZone === undefined || !stripEl?.parentElement) return false;
      const r = stripEl.parentElement.getBoundingClientRect();
      return (
        ev.clientX < r.left - LEAVE_MARGIN || ev.clientX > r.right + LEAVE_MARGIN ||
        ev.clientY < r.top - LEAVE_MARGIN || ev.clientY > r.bottom + LEAVE_MARGIN
      );
    }
    function onMove(ev: MouseEvent) {
      if (!active) {
        if (Math.abs(ev.clientX - startX) < 5) return;
        slots = measureSlots();
        const me = slots.find((s) => s.index === fromIndex);
        if (!me) { stop(); return; }
        gap = stripEl ? parseFloat(getComputedStyle(stripEl).columnGap) || 0 : 0;
        dragShift = me.width + gap;
        dragInsert = fromIndex + 1;
        active = true;
        dragFromIndex = fromIndex;
        document.body.style.cursor = 'grabbing';
      }
      rawOffset = ev.clientX - startX;
      if (outsideStrip(ev)) {
        // Free: the tab goes where the pointer goes, the strip closes ranks around the gap, and the
        // zone under the pointer (the held tab ignores the pointer, so it does not hide it) is hot.
        dragFree = true;
        dragOffset = rawOffset;
        dragOffsetY = ev.clientY - startY;
        dragInsert = fromIndex + 1;
        const zone = zoneUnder(ev.clientX, ev.clientY);
        hotZone = zone !== null && zone !== dropZone ? zone : null;
        tabDragStore.begin();
        tabDragStore.setHot(hotZone);
        showGhost(ev);
        return;
      }
      hideGhost();
      dragFree = false;
      dragOffsetY = 0;
      hotZone = null;
      tabDragStore.setHot(null);
      recompute();
    }
    function onKey(ev: KeyboardEvent) {
      if (ev.key !== 'Escape' || !active) return;
      ev.preventDefault();
      ev.stopPropagation();
      dragFree = false;
      hotZone = null;
      dragOffsetY = 0;
      dragInsert = fromIndex + 1; // back where it was
      finish();
    }
    function stop() {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', finish);
      window.removeEventListener('keydown', onKey, true);
      document.body.style.cursor = '';
    }
    function finish() {
      stop();
      hideGhost();
      tabDragStore.end();
      if (!active) return;
      active = false;
      // The click that follows a drag must not select: swallowed once, and only for this gesture.
      suppressNextClick = true;
      setTimeout(() => { suppressNextClick = false; }, 0);

      // Dropped on another zone: handed over, with no slide — the tab is leaving this strip, and
      // there is no slot here to settle into.
      if (dragFree && hotZone !== null && tabId !== undefined) {
        const zone = hotZone;
        dragFromIndex = null;
        dragFree = false;
        dragOffset = 0;
        dragOffsetY = 0;
        onDropOnZone?.(tabId, zone);
        return;
      }

      // Otherwise back into this strip: from wherever the pointer left it, into the slot it opened.
      if (dragFree) {
        rawOffset = 0;
        dragFree = false;
        dragOffsetY = 0;
        recompute();
      }
      const insert = dragInsert;
      const to = landingIndex(fromIndex, insert);
      // Slide into the slot, THEN reorder — the DOM order after the reorder puts the tab exactly
      // where the slide ended, so clearing the transforms in the same tick changes nothing visible.
      dragSettling = true;
      dragOffset = settleOffset(slots, fromIndex, insert, gap);
      setTimeout(() => {
        if (to !== fromIndex) onReorder?.(fromIndex, to);
        dragFromIndex = null;
        dragSettling = false;
        dragOffset = 0;
      }, SETTLE_MS);
    }

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', finish);
    window.addEventListener('keydown', onKey, true);
  }

  /** The transform a tab wears while a drag is in progress: the held tab follows the pointer, the
   *  ones it passes slide by the held tab's width, the rest are untouched. */
  function dragTransform(i: number): string | undefined {
    if (dragFromIndex === null) return undefined;
    if (i === dragFromIndex) {
      return dragFree ? `translate(${dragOffset}px, ${dragOffsetY}px)` : `translateX(${dragOffset}px)`;
    }
    const dir = shiftDirection(i, dragFromIndex, dragInsert);
    return dir === 0 ? undefined : `translateX(${dir * dragShift}px)`;
  }

  // ── Overflow (cache-based — same algorithm as the legacy TabBar) ──────────
  //
  // Widths are measured + cached per id; a declarative `tab-hidden` class
  // keeps DOM and state in lock-step without imperative classList work. The
  // budget is derived from the strip's parent container minus reserved
  // space for the chevron / add button / breathing room — NOT from the
  // strip's own clientWidth (which would create a feedback loop).
  let hiddenIds = $state<Set<string>>(new Set());
  const tabMeasurements = new Map<string, { width: number; signature: string }>();

  const OVERFLOW_CUSHION        = 4;
  const DEFAULT_TAB_WIDTH       = 170;
  const CHEVRON_RESERVE         = 44;
  const ADD_RESERVE             = 30;
  const TRAILING_BREATHING_ROOM = 60;

  function tabSignature(t: TabItem): string {
    return (t.label ?? '') + '||' + (t.badge ?? '') + '||' + (t.disabled ? 'd' : '');
  }

  function computeBudget(): number {
    if (!stripEl) return 0;
    const parent = stripEl.parentElement;
    if (!parent) return 0;
    let otherFixed = 0;
    for (const child of parent.children) {
      if (child === stripEl) continue;
      const el = child as HTMLElement;
      if (el.classList.contains('tabs-spacer')) continue;
      // The chevron reaches the strip's parent wrapped in the shared Dropdown's own root, so the
      // class to skip is the one handed to it — measuring it here AND reserving space for it below
      // would charge the budget twice and hide a tab that fits.
      if (el.classList.contains('tabs-overflow-slot')) continue;
      if (el.classList.contains('tabs-add')) continue;
      otherFixed += el.offsetWidth;
    }
    const reserveChev = (overflow ? CHEVRON_RESERVE : 0);
    const reserveAdd  = (onAdd    ? ADD_RESERVE     : 0);
    return Math.max(0,
      parent.clientWidth
        - otherFixed
        - reserveChev
        - reserveAdd
        - TRAILING_BREATHING_ROOM
        - OVERFLOW_CUSHION);
  }

  function measureVisibleTabs() {
    if (!stripEl) return;
    const nodes = stripEl.querySelectorAll<HTMLElement>('[data-tab-id]');
    for (const el of nodes) {
      const id = el.getAttribute('data-tab-id'); if (!id) continue;
      const w = el.offsetWidth; if (w <= 0) continue;
      const tab = items.find(t => t.id === id); if (!tab) continue;
      tabMeasurements.set(id, { width: w, signature: tabSignature(tab) });
    }
  }

  function applyOverflow() {
    if (!overflow || !stripEl) return;
    if (items.length === 0) {
      if (hiddenIds.size !== 0) hiddenIds = new Set();
      return;
    }
    const aliveIds = new Set(items.map(t => t.id));
    for (const id of [...tabMeasurements.keys()]) if (!aliveIds.has(id)) tabMeasurements.delete(id);
    measureVisibleTabs();

    const budget = computeBudget();
    if (budget < 80) return;

    const widthOf = (t: TabItem) => {
      const m = tabMeasurements.get(t.id);
      return (m && m.signature === tabSignature(t)) ? m.width : DEFAULT_TAB_WIDTH;
    };

    const keep = new Set<string>();
    let used = 0;
    const activeTab = value ? items.find(t => t.id === value) : null;
    if (activeTab) { used = widthOf(activeTab); keep.add(activeTab.id); }
    for (const t of items) {
      if (keep.has(t.id)) continue;
      const w = widthOf(t);
      if (used + w > budget) break;
      keep.add(t.id); used += w;
    }
    const next = new Set<string>();
    for (const t of items) if (!keep.has(t.id)) next.add(t.id);
    if (next.size !== hiddenIds.size || [...next].some(id => !hiddenIds.has(id))) hiddenIds = next;
  }

  const hiddenCount = $derived(hiddenIds.size);

  // ── Observers ───────────────────────────────────────────────────────────
  $effect(() => {
    if (!overflow || !stripEl) return;
    const ro = new ResizeObserver(() => applyOverflow());
    const mo = new MutationObserver(() => applyOverflow());
    ro.observe(stripEl);
    const parent = stripEl.parentElement;
    if (parent) ro.observe(parent);
    mo.observe(stripEl, { childList: true });
    const raf = requestAnimationFrame(applyOverflow);
    return () => { ro.disconnect(); mo.disconnect(); cancelAnimationFrame(raf); };
  });

  // Reactive fingerprint: re-pack when items / active tab / labels change.
  $effect(() => {
    if (!overflow) return;
    let fp = (value ?? '') + '#' + items.length;
    for (const t of items) fp += '|' + t.id + ':' + (t.label ?? '') + ':' + (t.badge ?? '');
    if (fp.length < 0) return;
    let cancelled = false;
    (async () => {
      await tick();
      if (cancelled) return;
      applyOverflow();
      requestAnimationFrame(() => { if (!cancelled) applyOverflow(); });
    })();
    return () => { cancelled = true; };
  });

  // ── Overflow dropdown ───────────────────────────────────────────────────
  //
  // The menu lists EVERY tab, not just the hidden ones: it doubles as the "jump to a tab" list once
  // a strip is crowded, and a menu whose contents change shape as tabs slide in and out of view is
  // one you cannot aim at. The hidden ones lead, because they are what the chevron is for.
  const overflowItems = $derived.by<DropdownItem[]>(() => {
    const entry = (t: TabItem): DropdownItem => ({
      kind:      'item',
      id:        t.id,
      label:     t.label ?? t.id,
      // The tab's own glyph, with whatever it needs to draw itself: a file-type icon and a Java kind
      // ring both take props, and a menu that dropped them showed a column of blanks beside names
      // the strip draws in full.
      icon:      t.icon,
      iconProps: t.iconProps,
      meta:      t.badge === undefined || t.badge === null || t.badge === '' ? undefined : String(t.badge),
      // A file that is not this project's is tinted here as it is in the strip: the badge says `ext`
      // either way, and reading it in the menu should not be a second thing to learn.
      metaColor: toneColour(t.tone),
      active:    t.id === value,
      disabled:  t.disabled,
      onclick:   () => selectFromMenu(t),
    });
    const hidden  = items.filter(t => hiddenIds.has(t.id));
    const visible = items.filter(t => !hiddenIds.has(t.id));
    if (hidden.length === 0 || visible.length === 0) return items.map(entry);
    return [...hidden.map(entry), { kind: 'separator' }, ...visible.map(entry)];
  });

  /** The colour a tone wears — the same two the strip paints its left edge with. */
  function toneColour(tone: TabItem['tone']): string | undefined {
    if (tone === 'external') return 'var(--warning)';
    if (tone === 'test') return 'var(--success)';
    return undefined;
  }

  function selectFromMenu(item: TabItem) {
    if (item.disabled) return;
    onSelect?.(item.id, item);
  }

  // ── Helpers ─────────────────────────────────────────────────────────────
  function selectItem(item: TabItem) {
    if (item.disabled) return;
    if (suppressNextClick) { suppressNextClick = false; return; }
    onSelect?.(item.id, item);
  }
  function isClosable(item: TabItem): boolean {
    return item.closable ?? closable;
  }
  function handleKeydown(e: KeyboardEvent, item: TabItem) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      selectItem(item);
    }
  }
  function handleCloseClick(e: MouseEvent, item: TabItem) {
    e.stopPropagation();
    onClose?.(item.id, item);
  }
  function handleContextMenu(e: MouseEvent, item: TabItem) {
    if (!onContextMenu) return;
    e.preventDefault();
    e.stopPropagation();
    onContextMenu(item.id, item, e);
  }
</script>

<div
  class="tabs tabs-{variant} tabs-sz-{size} {rootClass}"
  class:tabs-fill={fill}
  class:tabs-sorting={dragFromIndex !== null}
  role="tablist"
  aria-label={ariaLabel}
>
  <div class="tabs-strip" bind:this={stripEl}>
    {#each items as item, i (item.id)}
      {@const active = item.id === value}
      <div
        class="tabs-tab"
        class:tab-external={item.tone === 'external'}
        class:tab-test={item.tone === 'test'}
        class:tab-active={active}
        class:tab-hidden={hiddenIds.has(item.id)}
        class:tab-dragging={dragFromIndex === i}
        class:tab-settling={dragFromIndex === i && dragSettling}
        class:tab-ghosted={dragFromIndex === i && dragFree}
        class:tab-disabled={item.disabled}
        class:tab-draggable={draggable}
        data-tab-id={item.id}
        data-tab-idx={i}
        role="tab"
        tabindex={item.disabled ? -1 : 0}
        aria-selected={active}
        aria-disabled={item.disabled || undefined}
        style:transform={dragTransform(i)}
        use:tooltip={dragFromIndex === null ? (item.title ?? '') : ''}
        onmousedown={(e) => startTabDrag(e, i)}
        onclick={() => selectItem(item)}
        onkeydown={(e) => handleKeydown(e, item)}
        oncontextmenu={(e) => handleContextMenu(e, item)}
      >
        {#if itemContent}
          {@render itemContent({ item, active })}
        {:else}
          {#if item.icon}
            {@const Icon = item.icon}
            <Icon size={item.iconSize ?? (size === 'sm' ? 12 : 14)} {...(item.iconProps ?? {})} />
          {/if}
          {#if item.label}
            <span class="tab-label" style:color={item.labelColor}>{item.label}</span>
          {/if}
          {#if item.badge !== undefined && item.badge !== null && item.badge !== ''}
            <span class="tab-badge">{item.badge}</span>
          {/if}
        {/if}
        {#if isClosable(item)}
          <button
            type="button"
            class="tab-close"
            onmousedown={(e) => e.stopPropagation()}
            onclick={(e) => handleCloseClick(e, item)}
            use:tooltip={'Close'}
            aria-label={`Close ${item.label ?? item.id}`}
          ><X size={10} /></button>
        {/if}
      </div>
    {/each}
  </div>

  {#if overflow && hiddenCount > 0}
    <!-- A slot of our own around the shared Dropdown: the strip's width budget is computed from
         this container's children, and a wrapper we own is one we can measure and skip without
         reaching into another widget's classes. -->
    <div class="tabs-overflow-slot">
    <Dropdown
      items={overflowItems}
      position="fixed"
      direction="down"
      width="280px"
    >
      {#snippet trigger({ open, toggle })}
        <button
          type="button"
          class="tabs-action tabs-overflow-btn"
          class:tabs-action-active={open}
          onclick={toggle}
          use:tooltip={`${hiddenCount} more tab${hiddenCount === 1 ? '' : 's'}`}
          aria-label="Show hidden tabs"
          aria-expanded={open}
        >
          <ChevronDown size={13} />
          <span class="tabs-overflow-count">{hiddenCount}</span>
        </button>
      {/snippet}
    </Dropdown>
    </div>
  {/if}

  {#if onAdd}
    <button
      type="button"
      class="tabs-action tabs-add"
      onclick={() => onAdd!()}
      use:tooltip={addLabel ?? ''}
      aria-label={addLabel}
    ><Plus size={13} /></button>
  {/if}

  {#if trailingActions}{@render trailingActions()}{/if}

  <div class="tabs-spacer"></div>
</div>

<style>
  /* ── Container ──────────────────────────────────────────────────────────── */
  .tabs {
    display: flex;
    align-items: stretch;
    flex-shrink: 0;
    position: relative;
    overflow: visible;
    font-family: var(--font-ui-sans);
    user-select: none;
  }

  .tabs-strip {
    display: flex;
    align-items: stretch;
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    gap: 2px;
  }

  /* `fill` makes the strip + each tab stretch to fill the available width.
     The trailing .tabs-spacer would otherwise compete with the strip for
     extra space (both with flex-grow), so we collapse it in fill mode. */
  .tabs-fill .tabs-strip  { flex: 1 1 auto; }
  .tabs-fill .tabs-tab    { flex: 1 1 0; justify-content: center; }
  .tabs-fill .tabs-spacer { display: none; }

  .tabs-spacer { flex: 1; min-width: 0; }

  /* ── Tab base ───────────────────────────────────────────────────────────── */
  .tabs-tab {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    color: var(--text-secondary);
    background: transparent;
    border: none;
    white-space: nowrap;
    outline: none;
    transition: background var(--transition-fast), color var(--transition-fast),
                border-color var(--transition-fast);
    flex-shrink: 0;
  }
  .tabs-tab.tab-disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .tabs-tab.tab-hidden    { display: none !important; }
  /* While sorting, the strip stops clipping so the lifted tab's shadow is not cut off, and the tabs it
     passes slide rather than jump. The held tab follows the pointer with no transition at all — any
     easing on it would read as lag — until it is released, when it slides into its slot. */
  .tabs-sorting .tabs-strip { overflow: visible; }
  .tabs-sorting .tabs-tab:not(.tab-dragging) {
    transition: transform 190ms cubic-bezier(0.2, 0.8, 0.2, 1), background var(--transition-fast);
    will-change: transform;
  }
  .tabs-tab.tab-dragging {
    z-index: 50;
    cursor: grabbing;
    /* Never the target of its own drag: the zone under the pointer is what is asked about. */
    pointer-events: none;
    transition: none;
    will-change: transform;
    background: var(--bg-elevated);
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.38), 0 0 0 1px var(--border);
  }
  /* Carried away from the strip: the copy on the viewport is the one you see, and the strip shows only
     the gap it left. */
  .tabs-tab.tab-ghosted { opacity: 0; box-shadow: none; }
  .tabs-tab.tab-dragging.tab-settling {
    transition: transform 170ms cubic-bezier(0.2, 0.8, 0.2, 1), box-shadow 170ms;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.2);
  }
  .tabs-tab.tab-draggable { cursor: grab; }
  .tabs-tab:focus-visible {
    box-shadow: 0 0 0 2px var(--accent-subtle), 0 0 0 3px var(--accent);
  }

  .tab-label {
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 220px;
  }

  .tab-badge {
    font-size: var(--font-size-2xs);
    font-weight: 600;
    color: var(--accent);
    background: var(--accent-subtle);
    border-radius: var(--radius-sm);
    padding: 1px 5px;
    min-width: 14px;
    text-align: center;
    line-height: 1.3;
    flex-shrink: 0;
  }

  /* ── Sizes ──────────────────────────────────────────────────────────────── */
  /* `md` is the default: deliberately a bit roomier than the legacy panel
     tabs so modal/panel sub-navs (PluginFormModal, PipelinesPanel,
     StatsOverlay, …) feel less cramped. The `panel` variant explicitly
     resets back to the compact font-size below — file-tab strips
     (RepoTabBar, TerminalPanel) need to stay tight to fit many tabs. */
  .tabs-sz-sm .tabs-tab { padding: 4px 8px;  font-size: var(--font-size-xs); }
  .tabs-sz-md .tabs-tab { padding: 7px 14px; font-size: var(--font-size-md); }

  /* ── Variant: underline ────────────────────────────────────────────────── */
  .tabs-underline {
    border-bottom: 1px solid var(--border-subtle);
  }
  .tabs-underline .tabs-tab {
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
  }
  .tabs-underline .tabs-tab:hover:not(.tab-disabled):not(.tab-active) {
    color: var(--text-primary);
    background: var(--bg-hover);
  }
  .tabs-underline .tabs-tab.tab-active {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }

  /* ── Tone: external ─────────────────────────────────────────────────────
     A tab holding something from outside the workspace. Tinted in BOTH states, because
     the fact it warns about — that this file is read-only and not yours — is true whether
     or not the tab is the one you are looking at, and a strip where only the active tab
     said so would let you lose track of which of the others are real.

     A tint and a left edge rather than a different text colour: the label is a filename and
     has to stay as readable as any other, so the signal goes around it. `--warning` is the
     hue, at low alpha — not an error (nothing is wrong) but not ordinary either. */
  .tabs-tab.tab-external {
    background: color-mix(in srgb, var(--warning) 7%, transparent);
    box-shadow: inset 2px 0 0 color-mix(in srgb, var(--warning) 45%, transparent);
  }
  .tabs-tab.tab-external.tab-active {
    background: color-mix(in srgb, var(--warning) 16%, transparent);
    box-shadow: inset 2px 0 0 var(--warning);
  }
  .tabs-tab.tab-external:hover:not(.tab-disabled):not(.tab-active) {
    background: color-mix(in srgb, var(--warning) 12%, transparent);
  }

  /* A test source. The same left edge as `external` and the same reason for it — the label is a
     filename and must stay as readable as any other, so the signal goes around it — but weaker
     throughout: `external` warns, this only tells you which kind of code a tab holds. No tint at
     rest, so a strip of tests does not read as a strip of warnings; the edge alone carries it. */
  .tabs-tab.tab-test {
    box-shadow: inset 2px 0 0 color-mix(in srgb, var(--success) 55%, transparent);
  }
  .tabs-tab.tab-test.tab-active {
    background: color-mix(in srgb, var(--success) 12%, transparent);
    box-shadow: inset 2px 0 0 var(--success);
  }
  .tabs-tab.tab-test:hover:not(.tab-disabled):not(.tab-active) {
    background: color-mix(in srgb, var(--success) 9%, transparent);
  }

  /* ── Variant: pill ─────────────────────────────────────────────────────── */
  .tabs-pill .tabs-strip { gap: 4px; padding: 2px; }
  .tabs-pill .tabs-tab {
    border-radius: var(--radius-sm);
  }
  .tabs-pill .tabs-tab:hover:not(.tab-disabled):not(.tab-active) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .tabs-pill .tabs-tab.tab-active {
    background: var(--accent-subtle);
    color: var(--accent);
  }

  /* ── Variant: solid (primary view switcher) ─────────────────────────────
     Looks like a capsule with a filled accent active state. The trough
     background keeps the inactive tabs grouped visually. */
  .tabs-solid {
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 2px;
    gap: 0;
  }
  .tabs-solid .tabs-strip { gap: 2px; }
  .tabs-solid .tabs-tab {
    padding: 4px 12px;
    border-radius: 3px;
    font-size: var(--font-size-sm);
    font-weight: 500;
    line-height: 1;
    min-height: 26px;
    gap: 6px;
  }
  .tabs-solid .tabs-tab:hover:not(.tab-disabled):not(.tab-active) {
    background: var(--bg-overlay);
    color: var(--text-primary);
  }
  .tabs-solid .tabs-tab.tab-active {
    background: var(--accent);
    /* Theme token: dark text on light accents (Ayu Dark yellow, Solarized
       Light, …) and light text on dark accents — hard-coded #fff was
       unreadable on warm-yellow accent themes. */
    color: var(--text-on-accent);
    font-weight: 600;
    box-shadow: 0 1px 2px rgba(0,0,0,0.18);
  }

  /* ── Variant: panel (JetBrains-style file tabs) ─────────────────────────
     File-tab strips need to fit many tabs in a row, so the panel variant
     keeps the compact 12px font regardless of the size prop's larger
     default. Padding is overridden too to match the legacy strip metrics. */
  .tabs-panel { padding: 0 4px; }
  .tabs-panel .tabs-tab {
    padding: 0 7px 0 10px;
    font-size: var(--font-size-sm);
    height: 100%;
    border-radius: var(--radius-sm) var(--radius-sm) 0 0;
    align-self: stretch;
    max-width: 240px;
  }
  .tabs-panel .tabs-tab::after {
    content: '';
    position: absolute;
    left: 4px;
    right: 4px;
    bottom: 0;
    height: 2px;
    background: var(--accent);
    border-radius: 1px 1px 0 0;
    transform: scaleX(0);
    transform-origin: center;
    transition: transform var(--anim-dur-base, 150ms)
                          var(--anim-easing-spring, cubic-bezier(0.16,1,0.3,1));
    pointer-events: none;
  }
  .tabs-panel .tabs-tab.tab-active::after { transform: scaleX(1); }
  .tabs-panel .tabs-tab:hover:not(.tab-disabled):not(.tab-active) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .tabs-panel .tabs-tab.tab-active {
    background: var(--bg-elevated);
    color: var(--text-primary);
    font-weight: 500;
  }

  /* ── Close button ───────────────────────────────────────────────────────── */
  .tab-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    opacity: 0;
    transition: opacity var(--transition-fast),
                background var(--transition-fast),
                color var(--transition-fast);
    padding: 0;
    margin-left: 2px;
    flex-shrink: 0;
  }
  .tabs-tab:hover .tab-close,
  .tabs-tab.tab-active .tab-close { opacity: 1; }
  .tab-close:hover {
    background: color-mix(in srgb, var(--error) 22%, transparent);
    color: var(--error);
  }

  /* ── Trailing action buttons (overflow / add) ─────────────────────────── */
  .tabs-action {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    transition: background var(--transition-fast), color var(--transition-fast);
    align-self: center;
    flex-shrink: 0;
  }
  .tabs-action:hover { background: var(--bg-hover); color: var(--text-primary); }
  .tabs-action-active { color: var(--accent); }

  .tabs-add { margin-left: 4px; }

  .tabs-overflow-slot {
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }

  .tabs-overflow-btn {
    width: auto;
    padding: 0 7px 0 5px;
    gap: 3px;
    margin-left: 4px;
    height: 22px;
  }
  .tabs-overflow-count {
    font-size: var(--font-size-2xs);
    font-weight: 600;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    border-radius: var(--radius-md);
    padding: 1px 5px;
    min-width: 14px;
    text-align: center;
    line-height: 1.3;
    font-family: var(--font-ui-sans);
  }
</style>
