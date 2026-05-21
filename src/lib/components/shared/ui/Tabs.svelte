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
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { X, Plus, ChevronDown } from 'lucide-svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { tooltip } from '$lib/actions/tooltip';

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
    onContextMenu,
    addLabel        = 'Add',
    ariaLabel,
    class:    rootClass = '',
    itemContent,
    trailingActions,
  }: Props = $props();

  // ── Drag & Drop ───────────────────────────────────────────────────────────
  let dragFromIndex     = $state<number | null>(null);
  let insertBeforeIndex = $state<number | null>(null);
  let suppressNextClick = false;
  let stripEl: HTMLElement | undefined;

  function startTabDrag(e: MouseEvent, fromIndex: number) {
    if (!draggable || e.button !== 0) return;
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
      window.removeEventListener('mouseup', onUp);
      document.body.style.cursor = '';
      const wasActive   = active;
      const finalInsert = insertBeforeIndex;
      active = false;
      dragFromIndex = null;
      insertBeforeIndex = null;
      if (!wasActive || finalInsert === null) return;
      suppressNextClick = true;
      const to = finalInsert <= fromIndex ? finalInsert : finalInsert - 1;
      if (to !== fromIndex) onReorder?.(fromIndex, to);
    }
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  function calcInsert(x: number): number {
    if (!stripEl) return 0;
    const els = stripEl.querySelectorAll<HTMLElement>('[data-tab-idx]:not(.tab-hidden)');
    for (let i = 0; i < els.length; i++) {
      const r = els[i].getBoundingClientRect();
      if (x < r.left + r.width / 2) {
        return parseInt(els[i].getAttribute('data-tab-idx') ?? '0', 10);
      }
    }
    return items.length;
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
      if (el.classList.contains('tabs-overflow-btn')) continue;
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
  let overflowMenuOpen = $state(false);
  let overflowBtnEl:  HTMLElement | undefined = $state();
  let overflowMenuEl: HTMLElement | undefined = $state();
  let overflowMenuStyle = $state('');

  function toggleOverflowMenu() {
    if (!overflowBtnEl) return;
    if (!overflowMenuOpen) {
      const rect = overflowBtnEl.getBoundingClientRect();
      overflowMenuStyle = `top: ${rect.bottom + 4}px; right: ${window.innerWidth - rect.right}px;`;
    }
    overflowMenuOpen = !overflowMenuOpen;
  }

  function selectFromMenu(item: TabItem) {
    overflowMenuOpen = false;
    if (item.disabled) return;
    onSelect?.(item.id, item);
  }

  $effect(() => {
    if (!overflowMenuOpen) return;
    function onClickOutside(e: PointerEvent) {
      const t = e.target as Node;
      if (!overflowMenuEl?.contains(t) && !overflowBtnEl?.contains(t)) overflowMenuOpen = false;
    }
    function onKeydown(e: KeyboardEvent) { if (e.key === 'Escape') overflowMenuOpen = false; }
    document.addEventListener('pointerdown', onClickOutside);
    document.addEventListener('keydown', onKeydown);
    return () => {
      document.removeEventListener('pointerdown', onClickOutside);
      document.removeEventListener('keydown', onKeydown);
    };
  });

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
  role="tablist"
  aria-label={ariaLabel}
>
  <div class="tabs-strip" bind:this={stripEl}>
    {#each items as item, i (item.id)}
      {@const active = item.id === value}
      {#if draggable && insertBeforeIndex === i && dragFromIndex !== null}
        <div class="tabs-drag-indicator" aria-hidden="true"></div>
      {/if}
      <div
        class="tabs-tab"
        class:tab-active={active}
        class:tab-hidden={hiddenIds.has(item.id)}
        class:tab-dragging={dragFromIndex === i}
        class:tab-disabled={item.disabled}
        class:tab-draggable={draggable}
        data-tab-id={item.id}
        data-tab-idx={i}
        role="tab"
        tabindex={item.disabled ? -1 : 0}
        aria-selected={active}
        aria-disabled={item.disabled || undefined}
        use:tooltip={item.title ?? ''}
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
            <Icon size={item.iconSize ?? (size === 'sm' ? 12 : 14)} />
          {/if}
          {#if item.label}
            <span class="tab-label">{item.label}</span>
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
    {#if draggable && insertBeforeIndex === items.length && dragFromIndex !== null}
      <div class="tabs-drag-indicator" aria-hidden="true"></div>
    {/if}
  </div>

  {#if overflow && hiddenCount > 0}
    <button
      type="button"
      class="tabs-action tabs-overflow-btn"
      class:tabs-action-active={overflowMenuOpen}
      onclick={toggleOverflowMenu}
      bind:this={overflowBtnEl}
      use:tooltip={`${hiddenCount} more tab${hiddenCount === 1 ? '' : 's'}`}
      aria-label="Show hidden tabs"
    >
      <ChevronDown size={13} />
      <span class="tabs-overflow-count">{hiddenCount}</span>
    </button>
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

{#if overflow && overflowMenuOpen}
  <div
    class="tabs-overflow-menu"
    bind:this={overflowMenuEl}
    style={overflowMenuStyle}
    role="menu"
    transition:fly={{ y: -6, duration: animStore.dFast, easing: cubicOut }}
  >
    {#each items as item (item.id)}
      {@const isActive = item.id === value}
      {@const isHidden = hiddenIds.has(item.id)}
      <button
        type="button"
        class="tabs-overflow-item"
        class:active={isActive}
        class:is-hidden={isHidden}
        disabled={item.disabled}
        onclick={() => selectFromMenu(item)}
        role="menuitem"
        use:tooltip={item.title ?? ''}
      >
        <span class="tabs-overflow-dot" class:dot-visible={isHidden} aria-hidden="true"></span>
        {#if item.icon}
          {@const Icon = item.icon}
          <Icon size={12} />
        {/if}
        <span class="tabs-overflow-name">{item.label ?? item.id}</span>
        {#if item.badge !== undefined && item.badge !== null && item.badge !== ''}
          <span class="tabs-overflow-badge">{item.badge}</span>
        {/if}
      </button>
    {/each}
  </div>
{/if}

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
  .tabs-tab.tab-dragging  { opacity: 0.45; cursor: grabbing; }
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
    font-size: 10px;
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
    font-size: 12.5px;
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

  /* ── Drag indicator ─────────────────────────────────────────────────────── */
  .tabs-drag-indicator {
    width: 2px;
    min-width: 2px;
    height: 18px;
    background: var(--accent);
    border-radius: 1px;
    flex-shrink: 0;
    pointer-events: none;
    align-self: center;
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

  .tabs-overflow-btn {
    width: auto;
    padding: 0 7px 0 5px;
    gap: 3px;
    margin-left: 4px;
    height: 22px;
  }
  .tabs-overflow-count {
    font-size: 10px;
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

  /* ── Overflow dropdown menu ───────────────────────────────────────────── */
  .tabs-overflow-menu {
    position: fixed;
    z-index: var(--z-menu);
    min-width: 280px;
    max-height: 420px;
    overflow-y: auto;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 5px;
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.55);
    font-family: var(--font-ui-sans);
  }
  .tabs-overflow-item {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    padding: 6px 10px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .tabs-overflow-item:hover { background: var(--bg-hover); color: var(--text-primary); }
  .tabs-overflow-item.active { color: var(--accent); }
  .tabs-overflow-item.is-hidden .tabs-overflow-name { color: var(--text-primary); }
  .tabs-overflow-item:not(.is-hidden) .tabs-overflow-name { color: var(--text-muted); }
  .tabs-overflow-item:disabled { opacity: 0.45; cursor: not-allowed; }

  .tabs-overflow-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: transparent;
    flex-shrink: 0;
  }
  .tabs-overflow-dot.dot-visible { background: var(--accent); }

  .tabs-overflow-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .tabs-overflow-badge {
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    flex-shrink: 0;
  }
</style>
