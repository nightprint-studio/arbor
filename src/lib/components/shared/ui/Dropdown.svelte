<script module lang="ts">
  export type DropdownItem =
    | {
        kind:      'item';
        id:        string;
        label:     string;
        /** Lucide-style component rendered at size 14. */
        icon?:     any;
        /** Optional CSS colour applied to the icon (any CSS colour or
         *  `var(--token)`). Matches the same option on ContextMenu — useful
         *  for split-button menus that mirror a right-click menu's palette. */
        iconColor?: string;
        /** If provided, shown as a 22px avatar circle (icon is ignored). */
        avatarUrl?: string;
        /** Second line below the label in smaller text. */
        subtitle?: string;
        /** Right-aligned muted text (counts, dates, …). */
        meta?:     string;
        /** Built-in keybinding action id (e.g. 'commit') — resolved live via
         *  keybindingsStore so user remaps flow through. Preferred over
         *  `shortcut`. Rendered as an inline kbd hint on the right. */
        action?:   string;
        /** Pre-formatted shortcut fallback when `action` is not a known id. */
        shortcut?: string;
        /** Single-mode: shows a check on the right. Multi-mode: drives the checkbox state. */
        active?:   boolean;
        disabled?: boolean;
        danger?:   boolean;
        onclick:   () => void;
      }
    | {
        kind:             'group';
        id:               string;
        label:            string;
        count?:           number;
        collapsible?:     boolean;
        defaultCollapsed?: boolean;
        items:            DropdownItem[];
      }
    | { kind: 'separator'; label?: string };
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import { tick } from 'svelte';
  import { fly, slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { Search, ChevronDown, ChevronRight, Check, Loader } from 'lucide-svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  // NOTE: Kbd lives in shared/internal/ because its `action=` mode reaches
  // into Arbor's keybindings store. Dropdown uses it only as an optional
  // right-aligned shortcut hint, so the leak across the ui/internal boundary
  // is contained to this single import — see CLAUDE.md tier convention.
  import Kbd from '../internal/Kbd.svelte';

  type Ctx        = { open: boolean; toggle: () => void; close: () => void };
  type ContentCtx = { filter: string; close: () => void; reposition: () => void };

  interface Props {
    /** Renders the element that opens the dropdown. */
    trigger: Snippet<[Ctx]>;
    /** Declarative item list — Dropdown renders + manages groups + collapse. */
    items?: DropdownItem[];
    /** Freeform body mode — caller renders list content. Receives { filter, close, reposition }. */
    children?: Snippet<[ContentCtx]>;
    /** Optional footer rendered below the list (separator included automatically). */
    footer?: Snippet<[{ close: () => void }]>;
    /** Force hide the footer even when a `footer` snippet is provided.
     *  Useful when the footer would render empty (e.g. an action list filtered
     *  to zero entries) — Svelte 5 snippets are scoped, so callers can't
     *  conditionally pass the snippet itself. */
    showFooter?: boolean;
    /** Show a search/filter input at the top of the menu. */
    searchable?: boolean;
    searchPlaceholder?: string;
    /** Shown when items is empty OR filtered items yields no results. */
    emptyMessage?: string;
    /** 'absolute' — menu anchors to nearest positioned ancestor.
     *  'fixed'    — menu anchors to viewport (for toolbars / titlebars). */
    position?: 'absolute' | 'fixed';
    /** Direction the menu opens from the trigger. 'fixed' mode auto-flips
     *  (down ↔ up, right ↔ left). 'right' / 'left' are only meaningful with
     *  position='fixed' (e.g. menus opened from a vertical toolbar). */
    direction?: 'down' | 'up' | 'right' | 'left';
    /** CSS width string applied to the menu panel (e.g. '300px'). */
    width?: string;
    /** Upper cap on the menu's visual height in pixels. The internal
     *  auto-sizing always picks the largest of `120` and the
     *  available viewport space; this prop clamps that result so
     *  long item lists (project files, recent items, …) don't
     *  stretch the menu all the way down to the bottom of the
     *  window. The list inside still scrolls — only the panel
     *  height is bounded. */
    maxHeight?: number;
    /** When true (with position='fixed'), menu width equals trigger width. */
    matchTriggerWidth?: boolean;
    /** 'single' (default) closes on item click; 'multiple' stays open and renders checkboxes. */
    selectionMode?: 'single' | 'multiple';
    /** Override the default close-on-select behavior derived from `selectionMode`. */
    closeOnSelect?: boolean;
    /** Show a spinner inside the menu instead of the items. */
    loading?: boolean;
    /** Fires the moment the menu opens (use for lazy-loading items). */
    onopen?: () => void;
    class?: string;
  }

  let {
    trigger,
    items,
    children,
    footer,
    searchable        = false,
    searchPlaceholder = 'Search…',
    emptyMessage      = 'No results',
    position          = 'absolute',
    direction         = 'down',
    width,
    maxHeight,
    matchTriggerWidth = false,
    selectionMode     = 'single',
    closeOnSelect,
    loading           = false,
    onopen,
    showFooter        = true,
    class: rootClass  = '',
  }: Props = $props();

  const effectiveCloseOnSelect = $derived(
    closeOnSelect !== undefined ? closeOnSelect : selectionMode === 'single'
  );

  let open            = $state(false);
  let anchorEl        = $state<HTMLElement | undefined>();
  let menuEl          = $state<HTMLElement | undefined>();
  let listEl          = $state<HTMLElement | undefined>();
  let filter          = $state('');
  let menuStyle       = $state('');
  let collapsedGroups = $state(new Set<string>());
  let focusedIdx      = $state(-1);

  // ── Init group collapse state ─────────────────────────────────────────────
  $effect(() => {
    if (!items) return;
    const s = new Set<string>();
    walkItems(items, it => {
      if (it.kind === 'group' && it.collapsible && it.defaultCollapsed) s.add(it.id);
    });
    collapsedGroups = s;
  });

  function walkItems(list: DropdownItem[], fn: (i: DropdownItem) => void) {
    for (const i of list) { fn(i); if (i.kind === 'group') walkItems(i.items, fn); }
  }

  // ── Viewport-clamped fixed positioning ────────────────────────────────────
  function computeFixed() {
    if (!anchorEl) return;
    // Measure the actual trigger element (first child) when present.
    // The wrapper `.dd-root` is `display: inline-flex`; in rare cases
    // (e.g. an icon child still hydrating, or the wrapper rendered
    // inside a fresh flex parent) the wrapper's own bounding rect can
    // briefly read as 0,0 even when the trigger is laid out fine.
    const target = (anchorEl.firstElementChild as HTMLElement | null) ?? anchorEl;
    const r     = target.getBoundingClientRect();
    // If the trigger has no measurable box yet (icon child still
    // hydrating, fresh flex parent, …), retry on the next rAF rather
    // than write a `0,0` style that would flash the menu at the
    // viewport corner. The post-tick `toggle()` retry covers the
    // common case; this rAF retry is the belt-and-braces.
    if (r.width === 0 && r.height === 0) {
      requestAnimationFrame(() => computeFixed());
      return;
    }
    const GAP   = 6, MARGIN = 8;
    const explicitW = width ? parseInt(width) : null;
    const matchedW  = matchTriggerWidth ? r.width : null;
    const menuW     = explicitW ?? matchedW ?? 260;
    const isHoriz   = direction === 'right' || direction === 'left';
    let style: string;

    // Caller-supplied cap on the menu height — clamps the viewport-
    // derived auto-size below so long lists don't fill the screen.
    const clampMaxH = (h: number) => maxHeight ? Math.min(h, maxHeight) : h;

    if (isHoriz) {
      // Horizontal placement: menu opens to the side of the trigger,
      // aligned to the trigger's top. Auto-flips between left and right
      // depending on available space.
      const spaceRight = window.innerWidth - r.right - GAP - MARGIN;
      const spaceLeft  = r.left - GAP - MARGIN;
      const flipLeft   = direction === 'left' || (spaceRight < menuW && spaceLeft > spaceRight);
      const left = flipLeft
        ? Math.max(MARGIN, r.left - GAP - menuW)
        : r.right + GAP;
      const top  = Math.max(MARGIN, Math.min(r.top, window.innerHeight - 180 - MARGIN));
      const maxH = clampMaxH(Math.max(120, window.innerHeight - top - MARGIN));
      style = `left:${left}px;top:${top}px;max-height:${maxH}px;width:${menuW}px;`;
    } else {
      // Vertical placement: menu opens above or below the trigger,
      // left-aligned to the trigger.
      const spaceBelow = window.innerHeight - r.bottom - GAP - MARGIN;
      const spaceAbove = r.top - GAP - MARGIN;
      const flipUp     = direction === 'up' || (spaceBelow < 180 && spaceAbove > spaceBelow);
      let top: number, maxH: number;
      if (flipUp) {
        maxH = clampMaxH(Math.max(120, spaceAbove));
        top  = Math.max(MARGIN, r.top - GAP - Math.min(spaceAbove, maxHeight ?? 420));
      } else {
        top  = r.bottom + GAP;
        maxH = clampMaxH(Math.max(120, spaceBelow));
      }
      const left = Math.max(MARGIN, Math.min(r.left, window.innerWidth - menuW - MARGIN));
      style = `left:${left}px;top:${top}px;max-height:${maxH}px;width:${menuW}px;`;
    }
    if (matchTriggerWidth) style += `min-width:0;`;
    menuStyle = style;
  }

  // ── Toggle / close / reposition ───────────────────────────────────────────
  function toggle() {
    if (open) { close(); return; }
    filter = '';
    focusedIdx = -1;
    if (position === 'fixed') computeFixed();
    open = true;
    onopen?.();
    void tick().then(() => {
      // Re-measure now that the menu is in DOM. The pre-open
      // `computeFixed()` may have run against a not-yet-laid-out
      // trigger (icon children hydrating, fresh flex parent, …); the
      // post-tick pass guarantees the menu lands on the real anchor
      // rect even when the synchronous pass returned without writing.
      if (position === 'fixed') computeFixed();
      // Focus the first selected item (or the first item if none) and scroll into view.
      const list = navigableItems;
      const sel  = list.findIndex(it => it.active);
      focusedIdx = sel >= 0 ? sel : (list.length > 0 ? 0 : -1);
      scrollFocusedIntoView();
    });
  }

  /** Move focus back to the trigger element when the menu closes via
   *  keyboard (Escape / Enter selection). Without this the focus is
   *  orphaned on `<body>` and Tab restarts the tab cycle from scratch. */
  function focusTrigger() {
    const t = (anchorEl?.firstElementChild as HTMLElement | null) ?? null;
    if (t && typeof t.focus === 'function') t.focus();
  }

  function close(restoreFocus = false) {
    open = false;
    if (restoreFocus) focusTrigger();
  }

  function reposition() { if (position === 'fixed') computeFixed(); }

  // ── Item selection ────────────────────────────────────────────────────────
  function pickItem(item: Extract<DropdownItem, { kind: 'item' }>, viaKeyboard = false) {
    if (item.disabled) return;
    item.onclick();
    if (effectiveCloseOnSelect) close(viaKeyboard);
  }

  function toggleGroup(id: string) {
    const s = new Set(collapsedGroups);
    s.has(id) ? s.delete(id) : s.add(id);
    collapsedGroups = s;
    reposition();
  }

  function scrollFocusedIntoView() {
    if (!listEl || focusedIdx < 0) return;
    const el = listEl.querySelector(`[data-dd-idx="${focusedIdx}"]`) as HTMLElement | null;
    el?.scrollIntoView({ block: 'nearest' });
  }

  // ── Outside-click / keyboard / resize ─────────────────────────────────────
  $effect(() => {
    if (!open) return;
    function onOut(e: PointerEvent) {
      const t = e.target as Node;
      if (!menuEl?.contains(t) && !anchorEl?.contains(t)) close();
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') { e.stopPropagation(); close(true); return; }
      // Tab moves focus out of the menu — close without preventDefault so
      // the browser advances focus to the next tabstop naturally. Without
      // this the menu lingered open behind the next field.
      if (e.key === 'Tab') { close(false); return; }
      const max = navigableItems.length;
      if (max === 0) return;
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        focusedIdx = focusedIdx < max - 1 ? focusedIdx + 1 : 0;
        scrollFocusedIntoView();
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        focusedIdx = focusedIdx > 0 ? focusedIdx - 1 : max - 1;
        scrollFocusedIntoView();
      } else if (e.key === 'Home') {
        e.preventDefault(); focusedIdx = 0; scrollFocusedIntoView();
      } else if (e.key === 'End') {
        e.preventDefault(); focusedIdx = max - 1; scrollFocusedIntoView();
      } else if (e.key === 'Enter') {
        if (focusedIdx >= 0 && focusedIdx < max) {
          e.preventDefault();
          pickItem(navigableItems[focusedIdx], true);
        }
      }
    }
    function onResize() { if (position === 'fixed') computeFixed(); }
    document.addEventListener('pointerdown', onOut, { capture: true });
    document.addEventListener('keydown', onKey);
    if (position === 'fixed') {
      window.addEventListener('resize', onResize);
      window.addEventListener('scroll', onResize, true);
    }
    return () => {
      document.removeEventListener('pointerdown', onOut, { capture: true } as EventListenerOptions);
      document.removeEventListener('keydown', onKey);
      window.removeEventListener('resize', onResize);
      window.removeEventListener('scroll', onResize, true);
    };
  });

  // ── Filtered items (declarative mode) ────────────────────────────────────
  const filteredItems = $derived.by(() => {
    if (!items) return [] as DropdownItem[];
    const q = filter.trim().toLowerCase();
    return q ? doFilter(items, q) : items;
  });

  function doFilter(list: DropdownItem[], q: string): DropdownItem[] {
    const out: DropdownItem[] = [];
    for (const item of list) {
      if (item.kind === 'separator') { out.push(item); continue; }
      if (item.kind === 'group') {
        const kids = doFilter(item.items, q);
        if (kids.length) out.push({ ...item, items: kids });
      } else if (
        item.label.toLowerCase().includes(q) ||
        item.subtitle?.toLowerCase().includes(q)
      ) {
        out.push(item);
      }
    }
    return out;
  }

  // Flat list of focusable items (for keyboard nav). Skips group headers,
  // separators, items inside collapsed groups, and disabled items.
  const navigableItems = $derived.by(() => {
    const out: Extract<DropdownItem, { kind: 'item' }>[] = [];
    const walk = (list: DropdownItem[]) => {
      for (const it of list) {
        if (it.kind === 'item' && !it.disabled) out.push(it);
        else if (it.kind === 'group' && !collapsedGroups.has(it.id)) walk(it.items);
      }
    };
    walk(filteredItems);
    return out;
  });

  const hasContent = $derived(
    items === undefined ? false :
    filteredItems.some(i =>
      i.kind === 'item' ||
      (i.kind === 'group' && i.items.some(c => c.kind === 'item' || c.kind === 'group'))
    )
  );

  // Reset focus to first item when filter changes.
  $effect(() => {
    if (!open) return;
    filter; // track
    focusedIdx = navigableItems.length > 0 ? 0 : -1;
  });
</script>

{#snippet renderItem(item: Extract<DropdownItem, { kind: 'item' }>, depth: number)}
  {@const navIdx = navigableItems.indexOf(item)}
  <button
    class="dd-item"
    class:active={item.active}
    class:danger={item.danger}
    class:dd-focused={navIdx === focusedIdx && navIdx >= 0}
    style:padding-left={depth > 0 ? `${10 + depth * 14}px` : undefined}
    disabled={item.disabled}
    onclick={() => pickItem(item)}
    onmouseenter={() => { if (navIdx >= 0) focusedIdx = navIdx; }}
    role="menuitem"
    data-dd-idx={navIdx >= 0 ? navIdx : undefined}
  >
    {#if selectionMode === 'multiple'}
      <span class="dd-cb" class:dd-cb-on={item.active} aria-hidden="true">
        {#if item.active}<Check size={10} strokeWidth={3} />{/if}
      </span>
    {/if}
    {#if item.avatarUrl}
      <img class="dd-avatar" src={item.avatarUrl} alt="" />
    {:else if item.icon}
      {@const ItemIcon = item.icon}
      {#if item.iconColor}
        <span class="dd-icon-tint" style="color:{item.iconColor}"><ItemIcon size={14} /></span>
      {:else}
        <ItemIcon size={14} class="dd-icon" />
      {/if}
    {/if}
    <span class="dd-item-body">
      <span class="dd-item-label">{item.label}</span>
      {#if item.subtitle}<span class="dd-item-sub">{item.subtitle}</span>{/if}
    </span>
    {#if item.meta}<span class="dd-item-meta">{item.meta}</span>{/if}
    {#if item.action}
      <span class="dd-shortcut"><Kbd action={item.action} variant="inline" /></span>
    {:else if item.shortcut}
      <span class="dd-shortcut"><Kbd label={item.shortcut} variant="inline" /></span>
    {/if}
    {#if item.active && selectionMode !== 'multiple'}<Check size={11} class="dd-check" />{/if}
  </button>
{/snippet}

{#snippet renderEntry(entry: DropdownItem, depth: number)}
  {#if entry.kind === 'separator'}
    <div class="dd-sep" role="separator">
      {#if entry.label}<span class="dd-sep-label">{entry.label}</span>{/if}
    </div>
  {:else if entry.kind === 'item'}
    {@render renderItem(entry, depth)}
  {:else if entry.kind === 'group'}
    {@const collapsed = entry.collapsible && collapsedGroups.has(entry.id)}
    {#if entry.collapsible}
      <button
        class="dd-group-btn"
        style:padding-left={depth > 0 ? `${8 + depth * 14}px` : undefined}
        onclick={() => toggleGroup(entry.id)}
      >
        {#if collapsed}<ChevronRight size={11} />{:else}<ChevronDown size={11} />{/if}
        <span class="dd-group-label">{entry.label}</span>
        {#if entry.count != null}<span class="dd-count">{entry.count}</span>{/if}
      </button>
    {:else}
      <div
        class="dd-group-static"
        style:padding-left={depth > 0 ? `${8 + depth * 14}px` : undefined}
      >
        <span class="dd-group-label">{entry.label}</span>
        {#if entry.count != null}<span class="dd-count">{entry.count}</span>{/if}
      </div>
    {/if}
    {#if !collapsed}
      <div transition:slide={{ duration: animStore.dBase }}>
        {#each entry.items as child, ci (child.kind === 'group' ? `g:${child.id}` : child.kind === 'item' ? `i:${child.id}` : `s:${ci}`)}
          {@render renderEntry(child, depth + 1)}
        {/each}
      </div>
    {/if}
  {/if}
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="dd-root {rootClass}"
  class:dd-rel={position === 'absolute'}
  bind:this={anchorEl}
  onkeydown={(e) => {
    // WAI-ARIA combobox pattern: ArrowDown (or Alt+ArrowDown) on the focused
    // trigger opens the menu and lands on the first item. Enter / Space are
    // already handled natively by the trigger <button>. Only react when the
    // menu is closed — once open, the document-level key handler takes over.
    if (!open && (e.key === 'ArrowDown' || (e.altKey && e.key === 'ArrowDown'))) {
      e.preventDefault();
      toggle();
    }
  }}
>
  {@render trigger({ open, toggle, close })}

  {#if open}
    <div
      class="dd-menu"
      class:dd-fixed={position === 'fixed'}
      class:dd-up={direction === 'up' && position === 'absolute'}
      style="{position === 'fixed' ? menuStyle : ''}{width && position !== 'fixed' ? `width:${width};` : ''}{position === 'fixed' && !menuStyle ? 'visibility:hidden;' : ''}"
      bind:this={menuEl}
      role="menu"
      transition:fly={{
        x: direction === 'right' ? -4 : direction === 'left' ? 4 : 0,
        y: direction === 'up'    ?  4 : direction === 'down' ? -4 : 0,
        duration: animStore.dFast,
        easing: cubicOut,
      }}
    >
      {#if searchable && !loading}
        <div class="dd-search">
          <Search size={12} />
          <!-- svelte-ignore a11y_autofocus -->
          <input
            type="text"
            placeholder={searchPlaceholder}
            bind:value={filter}
            autofocus
          />
        </div>
      {/if}

      <div class="dd-list" bind:this={listEl}>
        {#if loading}
          <div class="dd-loading">
            <Loader size={13} class="dd-spin" /> Loading…
          </div>
        {:else if items !== undefined}
          {#if !hasContent}
            <div class="dd-empty">{emptyMessage}</div>
          {:else}
            {#each filteredItems as entry, i (entry.kind === 'group' ? `g:${entry.id}` : entry.kind === 'item' ? `i:${entry.id}` : `s:${i}`)}
              {@render renderEntry(entry, 0)}
            {/each}
          {/if}
        {/if}

        {#if children}
          {@render children({ filter, close, reposition })}
        {/if}
      </div>

      {#if footer && showFooter}
        <div class="dd-footer-sep" aria-hidden="true"></div>
        <div class="dd-footer">
          {@render footer({ close })}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .dd-root {
    display: inline-flex;
    align-items: center;
  }
  .dd-root.dd-rel { position: relative; }

  /* ── Menu panel ─────────────────────────────────────────────────────────── */
  .dd-menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: var(--z-menu);
    min-width: 180px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-popup);
    font-family: var(--font-ui-sans);
  }
  .dd-menu.dd-fixed { position: fixed; top: auto; left: auto; }
  .dd-menu.dd-up    { top: auto; bottom: calc(100% + 4px); }

  /* ── Search ─────────────────────────────────────────────────────────────── */
  .dd-search {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 8px 10px;
    color: var(--text-muted);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .dd-search input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    padding: 0;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
  }
  .dd-search input::placeholder { color: var(--text-disabled); }

  /* ── List ───────────────────────────────────────────────────────────────── */
  .dd-list {
    flex: 1;
    overflow-y: auto;
    padding: 4px;
    min-height: 0;
  }
  .dd-empty {
    padding: 18px 12px;
    font-size: 11px;
    color: var(--text-muted);
    text-align: center;
  }
  .dd-loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 18px 10px;
    font-size: 11px;
    color: var(--text-muted);
    font-style: italic;
  }
  :global(.dd-spin) { animation: dd-spin 0.9s linear infinite; }
  @keyframes dd-spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }

  /* ── Separator ──────────────────────────────────────────────────────────── */
  /* The plain (no-label) form is the original 1px hairline. When the
     separator carries a label, we promote the container to auto height
     with a top border — otherwise the label text would overflow the
     1px-tall hairline and paint on top of the previous row. */
  .dd-sep {
    height: 1px;
    background: var(--border-subtle);
    margin: 3px 4px;
  }
  .dd-sep:has(.dd-sep-label) {
    height: auto;
    background: transparent;
    margin: 4px 0 0;
    border-top: 1px solid var(--border-subtle);
  }
  .dd-sep-label {
    display: block;
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding: 6px 8px 2px;
  }

  /* ── Group header ───────────────────────────────────────────────────────── */
  .dd-group-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    background: transparent;
    border: none;
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-family: var(--font-ui-sans);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .dd-group-btn:hover { background: var(--bg-hover); color: var(--text-secondary); }

  .dd-group-static {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px 2px;
    color: var(--text-muted);
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .dd-group-label { flex: 1; text-align: left; }
  .dd-count {
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: var(--radius-md);
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
  }

  /* ── Item ───────────────────────────────────────────────────────────────── */
  .dd-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 10px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    text-align: left;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    cursor: pointer;
    transition: background var(--transition-fast);
  }
  .dd-item:hover:not(:disabled)         { background: var(--bg-hover); }
  .dd-item.dd-focused:not(:disabled)    { background: var(--bg-hover); }
  .dd-item:disabled                      { opacity: 0.45; cursor: not-allowed; }
  .dd-item.active                        { background: color-mix(in srgb, var(--accent) 8%, transparent); }
  .dd-item.danger                        { color: var(--error); }
  .dd-item.danger:hover:not(:disabled)   { background: var(--error-subtle); }

  /* Multi-select checkbox */
  .dd-cb {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    border: 1.5px solid var(--border);
    border-radius: 3px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--text-on-accent);
    background: transparent;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .dd-cb-on { background: var(--accent); border-color: var(--accent); }

  .dd-avatar {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    object-fit: cover;
    flex-shrink: 0;
  }
  :global(.dd-icon)  { flex-shrink: 0; color: var(--text-muted); }
  :global(.dd-check) { color: var(--accent); flex-shrink: 0; }
  /* Per-item icon tint (set via iconColor). The wrapping span owns the
     colour so we don't fight the `:global(.dd-icon)` muted default; the
     lucide glyph inside paints in currentColor. */
  .dd-icon-tint {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
  }
  /* Right-aligned inline kbd hint (mirrors ContextMenu's .shortcut-slot). */
  .dd-shortcut { margin-left: 8px; flex-shrink: 0; }

  .dd-item-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .dd-item-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    line-height: 1.3;
  }
  .dd-item-sub {
    font-size: 10px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    line-height: 1.3;
  }
  .dd-item-meta {
    font-size: 11px;
    color: var(--text-muted);
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }

  /* ── Footer ─────────────────────────────────────────────────────────────── */
  .dd-footer-sep {
    height: 1px;
    background: var(--border);
    margin: 2px 6px;
    flex-shrink: 0;
  }
  .dd-footer {
    padding: 4px;
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
  }
</style>
