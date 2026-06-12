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
    | {
        /** A trigger row that opens a flyout panel to the side, holding its
         *  own `items`. Hover-intent (mouse) or ArrowRight (keyboard) opens it.
         *  Children are NOT navigable from the parent list until the flyout
         *  is open. */
        kind:      'submenu';
        id:        string;
        label:     string;
        icon?:     any;
        count?:    number;
        disabled?: boolean;
        items:     DropdownItem[];
      }
    | { kind: 'separator'; label?: string };
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import { tick, onMount } from 'svelte';
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
    /** Fires every time the menu closes — pick, Escape, Tab, or
     *  outside-click. Lets callers distinguish "closed without picking"
     *  (treat as cancel) from "closed via item selection" by setting a
     *  flag in the item's `onclick` before the close runs. */
    onclose?: () => void;
    /** When true, open the menu on mount (one-shot — toggled to false
     *  externally has no effect). Used by inline-edit shells that
     *  pop the dropdown automatically when entering edit mode. */
    autoOpen?: boolean;
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
    onclose,
    autoOpen          = false,
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
  // Chain of open submenu ids, one per nesting level (root flyout = openPath[0],
  // its open child = openPath[1], …). Models "one flyout open per level".
  let openPath        = $state<string[]>([]);
  // When true the deepest open flyout should grab keyboard focus (set when the
  // flyout was opened via ArrowRight / Enter rather than hover).
  let flyoutViaKeyboard = $state(false);
  // The root-level open submenu id (first hop of the chain) — convenience.
  const openSubmenuId = $derived(openPath[0] ?? null);
  // Live trigger-row elements keyed by submenu id, used to position flyouts.
  let submenuRowEls   = $state(new Map<string, HTMLElement>());
  // Reposition tick — bumped on resize/scroll to recompute flyout styles.
  let flyoutTick      = $state(0);
  // Keyboard focus index WITHIN the deepest open flyout (-1 = none). Only the
  // deepest flyout traps keyboard nav; hover stays the primary path.
  let flyoutFocusIdx  = $state(-1);
  // Panel element of the deepest open flyout (for focus-trap + scroll-into-view).
  let activeFlyoutEl  = $state<HTMLElement | undefined>();

  /** The submenu definition open at the deepest level, resolved against the
   *  declarative `items` tree by following `openPath`. */
  const deepestSubmenu = $derived.by(() => {
    if (!items || openPath.length === 0) return null;
    let level: DropdownItem[] = items;
    let found: Extract<DropdownItem, { kind: 'submenu' }> | null = null;
    for (const id of openPath) {
      const sub = level.find(
        (i): i is Extract<DropdownItem, { kind: 'submenu' }> => i.kind === 'submenu' && i.id === id,
      );
      if (!sub) return null;
      found = sub;
      level = sub.items;
    }
    return found;
  });

  /** Navigable (item) entries of the deepest open flyout, for keyboard nav. */
  const flyoutNav = $derived.by(() => {
    const out: Extract<DropdownItem, { kind: 'item' }>[] = [];
    if (!deepestSubmenu) return out;
    for (const it of deepestSubmenu.items) {
      if (it.kind === 'item' && !it.disabled) out.push(it);
      else if (it.kind === 'submenu' && !it.disabled) {
        // A nested submenu trigger isn't a real item; skip for now — nested
        // flyouts are hover-driven. (Theme has no nested submenus.)
      }
    }
    return out;
  });

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
    for (const i of list) {
      fn(i);
      if (i.kind === 'group' || i.kind === 'submenu') walkItems(i.items, fn);
    }
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

  // ── Flyout positioning (fixed, to the side of a trigger row) ──────────────
  // Mirrors the horizontal branch of `computeFixed`: opens to the right of the
  // row, flips left when space is tight, clamps height/top to the viewport.
  // Works regardless of the parent menu's position mode since it reads the
  // row's live client rect.
  function computeFlyout(rowRect: DOMRect, panelW = 220): string {
    const GAP = 4, MARGIN = 8;
    const spaceRight = window.innerWidth - rowRect.right - GAP - MARGIN;
    const spaceLeft  = rowRect.left - GAP - MARGIN;
    const flipLeft   = spaceRight < panelW && spaceLeft > spaceRight;
    const left = flipLeft
      ? Math.max(MARGIN, rowRect.left - GAP - panelW)
      : rowRect.right + GAP;
    // Align the flyout's top to the row's top, but keep it on screen and leave
    // room for a minimally-tall panel.
    const top  = Math.max(MARGIN, Math.min(rowRect.top - 4, window.innerHeight - 120 - MARGIN));
    const maxH = Math.max(120, window.innerHeight - top - MARGIN);
    return `left:${left}px;top:${top}px;max-height:${maxH}px;width:${panelW}px;`;
  }

  // Auto-open on mount when the caller drives the open lifecycle from
  // outside (e.g. an inline-edit shell that pops the dropdown the moment
  // the row enters edit mode). One-shot — toggling `autoOpen` back to
  // false later has no effect; the menu is fully driven by `open` after.
  onMount(() => {
    if (autoOpen) void tick().then(() => { if (!open) toggle(); });
    return () => clearHoverTimers();
  });

  // ── Toggle / close / reposition ───────────────────────────────────────────
  function toggle() {
    if (open) { close(); return; }
    filter = '';
    focusedIdx = -1;
    closeRootSubmenu();
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
    if (!open) return;
    open = false;
    closeRootSubmenu();
    if (restoreFocus) focusTrigger();
    onclose?.();
  }

  function reposition() { if (position === 'fixed') computeFixed(); }

  // ── Item selection ────────────────────────────────────────────────────────
  function pickItem(item: Extract<DropdownItem, { kind: 'item' }>, viaKeyboard = false) {
    if (item.disabled) return;
    item.onclick();
    if (effectiveCloseOnSelect) close(viaKeyboard);
  }

  /** Activate a navigable entry: real items run their onclick; submenu rows
   *  open their flyout and (when keyboard-driven) move focus into it. */
  function activateNav(entry: NavEntry, viaKeyboard = false) {
    if (entry.disabled) return;
    if (entry.kind === 'submenu') {
      openRootSubmenu(entry.id, viaKeyboard);
      return;
    }
    pickItem(entry, viaKeyboard);
  }

  /** Open a root-level submenu flyout (resets any deeper chain). */
  function openRootSubmenu(id: string, viaKeyboard: boolean) {
    openPath = [id];
    flyoutViaKeyboard = viaKeyboard;
  }
  function closeRootSubmenu() {
    openPath = [];
    flyoutViaKeyboard = false;
    clearHoverTimers();
  }

  // ── Submenu hover-intent ──────────────────────────────────────────────────
  // Open after a short dwell, close after a slightly longer grace period so the
  // pointer can travel diagonally from the trigger row to the flyout without it
  // snapping shut mid-traverse. A single open/close pair is enough because only
  // one hover transition is ever in flight at a time.
  const HOVER_OPEN_MS  = 90;
  const HOVER_CLOSE_MS = 180;
  let hoverOpenTimer  = $state<ReturnType<typeof setTimeout> | null>(null);
  let hoverCloseTimer = $state<ReturnType<typeof setTimeout> | null>(null);

  function clearHoverTimers() {
    if (hoverOpenTimer)  { clearTimeout(hoverOpenTimer);  hoverOpenTimer  = null; }
    if (hoverCloseTimer) { clearTimeout(hoverCloseTimer); hoverCloseTimer = null; }
  }

  /** Pointer entered a submenu trigger row at the given nesting depth. Schedule
   *  opening it (and trimming any deeper-open chain) after the dwell delay. */
  function hoverOpenSubmenu(depthInPath: number, id: string) {
    clearHoverTimers();
    hoverOpenTimer = setTimeout(() => {
      hoverOpenTimer = null;
      // Replace everything from this level down with the hovered id.
      openPath = [...openPath.slice(0, depthInPath), id];
      flyoutViaKeyboard = false;
    }, HOVER_OPEN_MS);
  }

  /** Pointer left both a trigger row and its flyout panel — schedule closing
   *  the chain back to (and excluding) the given level after the grace delay. */
  function hoverCloseSubmenu(depthInPath: number) {
    if (hoverOpenTimer) { clearTimeout(hoverOpenTimer); hoverOpenTimer = null; }
    if (hoverCloseTimer) clearTimeout(hoverCloseTimer);
    hoverCloseTimer = setTimeout(() => {
      hoverCloseTimer = null;
      openPath = openPath.slice(0, depthInPath);
      if (openPath.length === 0) flyoutViaKeyboard = false;
    }, HOVER_CLOSE_MS);
  }

  /** Pointer re-entered the trigger/flyout before the close fired — cancel it. */
  function cancelHoverClose() {
    if (hoverCloseTimer) { clearTimeout(hoverCloseTimer); hoverCloseTimer = null; }
  }

  /** An item somewhere in a flyout was picked — propagate the close decision
   *  to the whole dropdown (single-mode closes everything, like a flat item). */
  function onFlyoutPick() {
    if (effectiveCloseOnSelect) close(false);
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

  function scrollFlyoutFocusedIntoView() {
    if (!activeFlyoutEl || flyoutFocusIdx < 0) return;
    const el = activeFlyoutEl.querySelector(`[data-fly-idx="${flyoutFocusIdx}"]`) as HTMLElement | null;
    el?.scrollIntoView({ block: 'nearest' });
  }

  // ── Outside-click / keyboard / resize ─────────────────────────────────────
  $effect(() => {
    if (!open) return;
    function onOut(e: PointerEvent) {
      const t = e.target as Node;
      // Flyout panels render in a fixed portal outside `menuEl`; treat a click
      // inside any open flyout (tagged with [data-dd-flyout]) as inside-menu.
      const inFlyout = t instanceof Element && t.closest('[data-dd-flyout]') != null;
      if (!menuEl?.contains(t) && !anchorEl?.contains(t) && !inFlyout) close();
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') { e.stopPropagation(); close(true); return; }
      // Tab moves focus out of the menu — close without preventDefault so
      // the browser advances focus to the next tabstop naturally. Without
      // this the menu lingered open behind the next field.
      if (e.key === 'Tab') { close(false); return; }

      // ── Keyboard focus is trapped inside the deepest open flyout ──────────
      if (openPath.length && flyoutViaKeyboard) {
        const n = flyoutNav.length;
        if (e.key === 'ArrowLeft') {
          // Step back out to the parent level (close the deepest flyout).
          e.preventDefault();
          openPath = openPath.slice(0, -1);
          if (openPath.length === 0) flyoutViaKeyboard = false;
          flyoutFocusIdx = -1;
          return;
        }
        if (n === 0) return;
        if (e.key === 'ArrowDown') {
          e.preventDefault();
          flyoutFocusIdx = flyoutFocusIdx < n - 1 ? flyoutFocusIdx + 1 : 0;
          scrollFlyoutFocusedIntoView();
        } else if (e.key === 'ArrowUp') {
          e.preventDefault();
          flyoutFocusIdx = flyoutFocusIdx > 0 ? flyoutFocusIdx - 1 : n - 1;
          scrollFlyoutFocusedIntoView();
        } else if (e.key === 'Home') {
          e.preventDefault(); flyoutFocusIdx = 0; scrollFlyoutFocusedIntoView();
        } else if (e.key === 'End') {
          e.preventDefault(); flyoutFocusIdx = n - 1; scrollFlyoutFocusedIntoView();
        } else if (e.key === 'Enter') {
          if (flyoutFocusIdx >= 0 && flyoutFocusIdx < n) {
            e.preventDefault();
            const it = flyoutNav[flyoutFocusIdx];
            it.onclick();
            onFlyoutPick();
          }
        }
        return;
      }

      // While a flyout owns keyboard focus it handles its own arrows above, so
      // this handler only runs for the parent list otherwise.
      const max = navigableItems.length;
      if (max === 0) return;
      const focused = focusedIdx >= 0 && focusedIdx < max ? navigableItems[focusedIdx] : null;
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        focusedIdx = focusedIdx < max - 1 ? focusedIdx + 1 : 0;
        closeRootSubmenu();
        scrollFocusedIntoView();
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        focusedIdx = focusedIdx > 0 ? focusedIdx - 1 : max - 1;
        closeRootSubmenu();
        scrollFocusedIntoView();
      } else if (e.key === 'ArrowRight') {
        // Open the flyout of a focused submenu row and dive into it.
        if (focused?.kind === 'submenu') {
          e.preventDefault();
          activateNav(focused, true);
        }
      } else if (e.key === 'ArrowLeft') {
        // Collapse the currently-open root flyout (focus stays on its trigger).
        if (openSubmenuId != null) { e.preventDefault(); closeRootSubmenu(); }
      } else if (e.key === 'Home') {
        e.preventDefault(); focusedIdx = 0; closeRootSubmenu(); scrollFocusedIntoView();
      } else if (e.key === 'End') {
        e.preventDefault(); focusedIdx = max - 1; closeRootSubmenu(); scrollFocusedIntoView();
      } else if (e.key === 'Enter') {
        if (focused) {
          e.preventDefault();
          activateNav(focused, true);
        }
      }
    }
    function onResize() {
      if (position === 'fixed') computeFixed();
      // Recompute open flyout panels too (they always use fixed coords).
      if (openPath.length) flyoutTick++;
    }
    document.addEventListener('pointerdown', onOut, { capture: true });
    document.addEventListener('keydown', onKey);
    // Fixed menus reposition on resize/scroll; absolute menus only need this
    // when they host flyouts (which are always fixed-positioned).
    const needsReposition = position === 'fixed' || (items?.some(i => i.kind === 'submenu') ?? false);
    if (needsReposition) {
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
      } else if (item.kind === 'submenu') {
        // When a filter is active, flatten the flyout: its matching
        // descendants surface inline so search reaches into submenus.
        // (Searchable dropdowns rarely use submenus; Theme is not searchable.)
        const kids = doFilter(item.items, q);
        for (const k of kids) out.push(k);
      } else if (
        item.label.toLowerCase().includes(q) ||
        item.subtitle?.toLowerCase().includes(q)
      ) {
        out.push(item);
      }
    }
    return out;
  }

  // Flat list of focusable entries (for keyboard nav). Skips group headers,
  // separators, items inside collapsed groups, and disabled items. A submenu
  // contributes its TRIGGER row (navigable) but NOT its children — those are
  // only reachable once the flyout opens (handled inside DropdownFlyout).
  type NavEntry =
    | Extract<DropdownItem, { kind: 'item' }>
    | Extract<DropdownItem, { kind: 'submenu' }>;
  const navigableItems = $derived.by(() => {
    const out: NavEntry[] = [];
    const walk = (list: DropdownItem[]) => {
      for (const it of list) {
        if (it.kind === 'item' && !it.disabled) out.push(it);
        else if (it.kind === 'submenu' && !it.disabled) out.push(it);
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
      i.kind === 'submenu' ||
      (i.kind === 'group' && i.items.some(c => c.kind === 'item' || c.kind === 'group' || c.kind === 'submenu'))
    )
  );

  // Reset focus to first item when filter changes.
  $effect(() => {
    if (!open) return;
    filter; // track
    focusedIdx = navigableItems.length > 0 ? 0 : -1;
  });

  // When a flyout opens via keyboard, land focus on its active item (or first).
  // Hover-opened flyouts leave flyoutFocusIdx at -1 so no row looks "selected".
  $effect(() => {
    const depth = openPath.length;
    if (depth > 0 && flyoutViaKeyboard) {
      const nav = flyoutNav;
      const sel = nav.findIndex(it => it.active);
      flyoutFocusIdx = sel >= 0 ? sel : (nav.length > 0 ? 0 : -1);
      void tick().then(scrollFlyoutFocusedIntoView);
    } else {
      flyoutFocusIdx = -1;
    }
  });

  /** Register/unregister a submenu trigger row so its flyout can be positioned
   *  against its live client rect. */
  function bindSubmenuRow(node: HTMLElement, id: string) {
    submenuRowEls.set(id, node);
    submenuRowEls = submenuRowEls; // notify
    return {
      destroy() { submenuRowEls.delete(id); submenuRowEls = submenuRowEls; },
    };
  }

  /** Is the submenu `id` the one open at nesting level `pathDepth`? */
  function isSubmenuOpen(id: string, pathDepth: number): boolean {
    return openPath[pathDepth] === id;
  }

  /** Track the deepest open flyout's panel element for keyboard focus-trap and
   *  scroll-into-view. Svelte action so it follows mount/unmount + the
   *  isDeepest flag as the chain grows/shrinks. */
  function registerFlyoutPanel(node: HTMLElement, isDeepest: boolean) {
    if (isDeepest) activeFlyoutEl = node;
    return {
      update(next: boolean) {
        if (next) activeFlyoutEl = node;
        else if (activeFlyoutEl === node) activeFlyoutEl = undefined;
      },
      destroy() { if (activeFlyoutEl === node) activeFlyoutEl = undefined; },
    };
  }

  /** Fixed style for an open flyout, derived from its trigger row's rect.
   *  Reads `flyoutTick` so resize/scroll repositions reactively. */
  function flyoutStyle(id: string): string {
    void flyoutTick; // reactive dep
    const row = submenuRowEls.get(id);
    if (!row) return 'visibility:hidden;';
    return computeFlyout(row.getBoundingClientRect());
  }
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
    onmouseenter={() => { if (navIdx >= 0) focusedIdx = navIdx; if (openPath.length) hoverCloseSubmenu(0); }}
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

{#snippet renderEntry(entry: DropdownItem, depth: number, pathDepth = 0)}
  {#if entry.kind === 'separator'}
    <div class="dd-sep" role="separator">
      {#if entry.label}<span class="dd-sep-label">{entry.label}</span>{/if}
    </div>
  {:else if entry.kind === 'item'}
    {@render renderItem(entry, depth)}
  {:else if entry.kind === 'submenu'}
    {@render renderSubmenu(entry, depth, pathDepth)}
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
        {#each entry.items as child, ci (child.kind === 'group' ? `g:${child.id}` : child.kind === 'item' ? `i:${child.id}` : child.kind === 'submenu' ? `m:${child.id}` : `s:${ci}`)}
          {@render renderEntry(child, depth + 1, pathDepth)}
        {/each}
      </div>
    {/if}
  {/if}
{/snippet}

{#snippet renderSubmenu(entry: Extract<DropdownItem, { kind: 'submenu' }>, depth: number, pathDepth: number)}
  {@const navIdx = navigableItems.indexOf(entry)}
  {@const isOpen = isSubmenuOpen(entry.id, pathDepth)}
  {@const SubIcon = entry.icon}
  <button
    class="dd-item dd-submenu-row"
    class:active={isOpen}
    class:dd-focused={navIdx === focusedIdx && navIdx >= 0}
    style:padding-left={depth > 0 ? `${10 + depth * 14}px` : undefined}
    disabled={entry.disabled}
    use:bindSubmenuRow={entry.id}
    onclick={() => { if (isOpen) { openPath = openPath.slice(0, pathDepth); } else { openPath = [...openPath.slice(0, pathDepth), entry.id]; flyoutViaKeyboard = true; } }}
    onmouseenter={() => { if (navIdx >= 0) focusedIdx = navIdx; cancelHoverClose(); hoverOpenSubmenu(pathDepth, entry.id); }}
    onmouseleave={() => hoverCloseSubmenu(pathDepth)}
    role="menuitem"
    aria-haspopup="menu"
    aria-expanded={isOpen}
    data-dd-idx={navIdx >= 0 ? navIdx : undefined}
  >
    {#if SubIcon}<SubIcon size={14} class="dd-icon" />{/if}
    <span class="dd-item-body">
      <span class="dd-item-label">{entry.label}</span>
    </span>
    {#if entry.count != null}<span class="dd-count">{entry.count}</span>{/if}
    <ChevronRight size={13} class="dd-submenu-chevron" />
  </button>

  {#if isOpen}
    {@render renderFlyout(entry, pathDepth)}
  {/if}
{/snippet}

{#snippet renderFlyout(entry: Extract<DropdownItem, { kind: 'submenu' }>, pathDepth: number)}
  {@const isDeepest = openPath.length === pathDepth + 1}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="dd-menu dd-fixed dd-flyout"
    data-dd-flyout
    style={flyoutStyle(entry.id)}
    role="menu"
    use:registerFlyoutPanel={isDeepest}
    onmouseenter={cancelHoverClose}
    onmouseleave={() => hoverCloseSubmenu(pathDepth)}
    transition:fly={{ x: -4, duration: animStore.dFast, easing: cubicOut }}
  >
    <div class="dd-list">
      {#each entry.items as child, ci (child.kind === 'group' ? `g:${child.id}` : child.kind === 'item' ? `i:${child.id}` : child.kind === 'submenu' ? `m:${child.id}` : `s:${ci}`)}
        {#if child.kind === 'item'}
          {@render renderFlyoutItem(child, isDeepest)}
        {:else}
          {@render renderEntry(child, 0, pathDepth + 1)}
        {/if}
      {/each}
    </div>
  </div>
{/snippet}

{#snippet renderFlyoutItem(item: Extract<DropdownItem, { kind: 'item' }>, isDeepest: boolean)}
  {@const flyIdx = isDeepest ? flyoutNav.indexOf(item) : -1}
  {@const ItemIcon = item.icon}
  <button
    class="dd-item"
    class:active={item.active}
    class:danger={item.danger}
    class:dd-focused={flyIdx >= 0 && flyIdx === flyoutFocusIdx}
    disabled={item.disabled}
    onclick={() => { if (item.disabled) return; item.onclick(); onFlyoutPick(); }}
    onmouseenter={() => { if (flyIdx >= 0) flyoutFocusIdx = flyIdx; }}
    role="menuitem"
    data-fly-idx={flyIdx >= 0 ? flyIdx : undefined}
  >
    {#if item.avatarUrl}
      <img class="dd-avatar" src={item.avatarUrl} alt="" />
    {:else if ItemIcon}
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
    {#if item.active}<Check size={11} class="dd-check" />{/if}
  </button>
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
            {#each filteredItems as entry, i (entry.kind === 'group' ? `g:${entry.id}` : entry.kind === 'item' ? `i:${entry.id}` : entry.kind === 'submenu' ? `m:${entry.id}` : `s:${i}`)}
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
  /* Flyout panels stack above the parent menu and share its surface. */
  .dd-flyout { z-index: calc(var(--z-menu) + 1); min-width: 0; }

  /* ── Submenu trigger row ────────────────────────────────────────────────── */
  /* Reuses .dd-item; the chevron is pushed to the far right and the body
     keeps its natural flex so label + count behave like a normal item. */
  .dd-submenu-row { padding-right: 6px; }
  :global(.dd-submenu-chevron) {
    flex-shrink: 0;
    color: var(--text-muted);
    margin-left: 4px;
  }
  .dd-submenu-row.active :global(.dd-submenu-chevron) { color: var(--text-secondary); }

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
