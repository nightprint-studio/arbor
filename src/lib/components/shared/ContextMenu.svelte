<script lang="ts">
  import { computePosition, flip, shift, offset } from '@floating-ui/dom';
  import { onMount, tick } from 'svelte';
  import { fly, fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { ChevronRight } from 'lucide-svelte';

  /**
   * A compact icon-only action shown in the horizontal quick-action bar at the
   * top of the menu (Windows-11 style: Cut / Copy / Rename / Delete …). The
   * `label` doubles as tooltip + aria-label. Selecting one fires `onSelect(id)`
   * and closes the menu, exactly like a regular item.
   */
  export interface MenuAction {
    id: string;
    label: string;
    icon: any;
    shortcut?: string;
    disabled?: boolean;
    danger?: boolean;
  }

  export interface MenuItem {
    id: string;
    label: string;
    icon?: any;
    /**
     * Optional CSS colour applied to the icon (accepts any valid CSS colour or
     * `var(--token)`). Tints only the icon — labels still inherit from the
     * `menu-item` / `danger` styles. Lucide icons render via `currentColor`,
     * so wrapping them in a coloured span is enough.
     */
    iconColor?: string;
    disabled?: boolean;
    danger?: boolean;
    separator?: boolean;
    /** Non-clickable section label rendered above a group of items. */
    header?: boolean;
    /**
     * Built-in keybinding action id (e.g. 'open_repo'). Resolved live via
     * keybindingsStore so user remaps flow through. Preferred over `shortcut`.
     */
    action?: string;
    /** Pre-formatted fallback when `action` is not a known built-in id. */
    shortcut?: string;
    /** Small badge shown on the right (e.g. "Default", "★"). */
    badge?: string;
    badgeAccent?: boolean;
    /**
     * Optional muted second line rendered below the main label — useful
     * when the label benefits from extra metadata (branch, path, time…)
     * without bloating the primary text. Empty / undefined hides the row.
     */
    subtitle?: string;
    /**
     * Nested submenu items. When present (and non-empty) the item renders as a
     * parent that expands a right-side flyout on hover / click instead of
     * firing `onSelect`. The flyout can itself contain leaves, headers and
     * separators — but NOT further nesting (one level deep, by design).
     */
    children?: MenuItem[];
  }

  let {
    items,
    actions,
    x = 0,
    y = 0,
    onSelect,
    onClose,
  }: {
    items: MenuItem[];
    /** Optional icon-only quick-action bar pinned to the top of the menu. */
    actions?: MenuAction[];
    x?: number;
    y?: number;
    onSelect: (id: string) => void;
    onClose: () => void;
  } = $props();

  let menuEl = $state<HTMLElement | null>(null);
  // svelte-ignore state_referenced_locally
  let adjustedX = $state(x);
  // svelte-ignore state_referenced_locally
  let adjustedY = $state(y);

  $effect(() => {
    if (!menuEl) return;
    // Ensure the menu doesn't go off screen
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const rect = menuEl.getBoundingClientRect();
    adjustedX = Math.min(x, vw - rect.width - 8);
    adjustedY = Math.min(y, vh - rect.height - 8);
  });

  // ── Submenu (one level deep) ───────────────────────────────────────────────
  // `openSub` holds the id of the parent whose flyout is open; `subEl` is the
  // open flyout, measured once per open to flip it leftward when it would spill
  // off the right edge of the viewport.
  let openSub  = $state<string | null>(null);
  let subEl    = $state<HTMLElement | null>(null);
  let subFlip  = $state(false);
  let subShift = $state(0);

  $effect(() => {
    if (!subEl || openSub === null) return;
    const r = subEl.getBoundingClientRect();
    // Horizontal: flip to the left of the parent when it would spill past the
    // right edge.
    if (r.right > window.innerWidth - 8) subFlip = true;
    // Vertical: a flyout opened from a near-the-bottom parent extends downward
    // and would be clipped by the window. Lift it up by the overflow amount,
    // but never so far that its top leaves the viewport (max-height + scroll
    // then catches anything still taller than the screen).
    const overflow = r.bottom - (window.innerHeight - 8);
    if (overflow > 0) subShift = Math.max(-overflow, 8 - r.top);
  });

  /** Open `id`'s flyout, resetting flip/shift so the effect re-measures fresh. */
  function openParent(id: string) { openSub = id; subFlip = false; subShift = 0; }
  function leaveParent(id: string) { if (openSub === id) openSub = null; }
  function toggleParent(id: string) {
    if (openSub === id) { openSub = null; } else { openSub = id; subFlip = false; subShift = 0; }
  }

  function handleItem(item: MenuItem) {
    if (item.disabled || item.separator) return;
    onSelect(item.id);
    onClose();
  }

  function handleAction(action: MenuAction) {
    if (action.disabled) return;
    onSelect(action.id);
    onClose();
  }

  // ── Keyboard navigation ─────────────────────────────────────────────────────
  // The menu owns the keyboard while open: focus lands on the first item, arrows
  // move between items (and the quick-action bar), →/← open/close the submenu,
  // Enter/Space activate (native button click), Esc closes, Tab is trapped so
  // focus never escapes to the host window's titlebar / controls. Every handled
  // key is stopped from propagating so the host's own key handlers don't also
  // react to the same press.
  const ENABLED = '.quick-action, .menu-item';
  /** Top-level focusable controls (quick actions + items), excluding the open
   *  submenu's children and disabled entries. */
  function topItems(): HTMLElement[] {
    if (!menuEl) return [];
    return Array.from(menuEl.querySelectorAll<HTMLElement>(ENABLED))
      .filter(b => !b.hasAttribute('disabled') && !b.closest('.context-submenu'));
  }
  /** Focusable children of the open submenu flyout. */
  function subItemsList(): HTMLElement[] {
    if (!subEl) return [];
    return Array.from(subEl.querySelectorAll<HTMLElement>('.menu-item')).filter(b => !b.hasAttribute('disabled'));
  }
  /** Close the submenu and return focus to its parent button. */
  function closeSub() {
    const id = openSub;
    openSub = null;
    tick().then(() => menuEl?.querySelector<HTMLElement>(`.submenu-parent[data-sub-id="${id}"]`)?.focus());
  }

  function menuKeydown(e: KeyboardEvent) {
    // The open menu owns the keyboard — never let a key leak to the host's
    // window-level handler (which would, e.g., type-ahead-filter the list behind
    // the menu). preventDefault is added per-key only where it matters.
    e.stopPropagation();
    const active = document.activeElement as HTMLElement | null;
    const inSub = !!active?.closest('.context-submenu');
    if (e.key === 'Escape') {
      e.preventDefault();
      if (openSub) closeSub(); else onClose();
      return;
    }
    // Native <button> activation handles select / toggle on Enter/Space.
    if (e.key === 'Enter' || e.key === ' ') return;
    const list = inSub ? subItemsList() : topItems();
    if (!list.length) return;
    const idx = active ? list.indexOf(active) : -1;
    const go = (i: number) => { e.preventDefault(); if (!inSub) openSub = null; list[(i + list.length) % list.length]?.focus(); };
    switch (e.key) {
      case 'ArrowDown': go(idx < 0 ? 0 : idx + 1); return;
      case 'ArrowUp':   go(idx < 0 ? list.length - 1 : idx - 1); return;
      case 'Home':      go(0); return;
      case 'End':       go(list.length - 1); return;
      case 'Tab':       go(e.shiftKey ? (idx < 0 ? list.length - 1 : idx - 1) : (idx < 0 ? 0 : idx + 1)); return;
      case 'ArrowRight': {
        if (active?.classList.contains('quick-action')) { go(idx + 1); return; }
        if (!inSub && active?.classList.contains('submenu-parent')) {
          e.preventDefault();
          const id = active.getAttribute('data-sub-id');
          if (id) { openParent(id); tick().then(() => subItemsList()[0]?.focus()); }
        }
        return;
      }
      case 'ArrowLeft': {
        if (active?.classList.contains('quick-action')) { go(idx - 1); return; }
        if (inSub) { e.preventDefault(); closeSub(); return; }
        return;
      }
    }
    // Type-ahead: jump to the next item whose label starts with the typed letter
    // (quick-action icons have no text, so they're naturally skipped).
    if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      const ch = e.key.toLowerCase();
      const from = idx < 0 ? 0 : idx + 1;
      for (let n = 0; n < list.length; n++) {
        const cand = list[(from + n) % list.length];
        if ((cand.querySelector('.label')?.textContent ?? cand.textContent ?? '').trim().toLowerCase().startsWith(ch)) {
          cand.focus();
          break;
        }
      }
    }
  }

  // Park focus on the first menu item when the menu opens (not the quick-action
  // bar), so arrow keys are live immediately. The focus ring (:focus-visible)
  // only shows when the menu was opened/driven by the keyboard, so mouse users
  // see nothing extra.
  onMount(() => { tick().then(() => {
    const all = topItems();
    (all.find(b => b.classList.contains('menu-item')) ?? all[0])?.focus();
  }); });

  function handleKeydown(e: KeyboardEvent) {
    // Fallback only — when focus somehow isn't inside the menu, Esc still closes.
    if (e.key === 'Escape') onClose();
  }

  // Outside-click dismissal uses a full-viewport backdrop layered above
  // `data-tauri-drag-region` elements (titlebar, etc.), because Tauri's
  // drag region intercepts mousedown/pointerdown events before they reach
  // document-level listeners. The backdrop sits just below the menu in
  // z-order so clicks on menu items still hit the menu itself.
  function onBackdropPointerDown(e: PointerEvent) {
    // Right-clicks pass-through so users can open a new context menu on
    // another target without a close-then-open round-trip.
    if (e.button === 2) return;
    onClose();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- Outside-click catcher: full-viewport div that closes the menu on click.
     Covers the titlebar's `data-tauri-drag-region`, which would otherwise
     swallow pointer events before our listener could react. -->
<div
  class="context-menu-backdrop"
  role="presentation"
  onpointerdown={onBackdropPointerDown}
  oncontextmenu={(e) => { e.preventDefault(); onClose(); }}
></div>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  bind:this={menuEl}
  class="context-menu"
  style="left: {adjustedX}px; top: {adjustedY}px"
  role="menu"
  tabindex="-1"
  onkeydown={menuKeydown}
  in:fly={{ y: -6, duration: animStore.dFast, easing: cubicOut }}
  out:fade={{ duration: animStore.dFast }}
>
  {#if actions && actions.length}
    <div class="quick-actions" role="group" aria-label="Quick actions">
      {#each actions as action (action.id)}
        {@const ActionIcon = action.icon}
        <button
          class="quick-action"
          class:danger={action.danger}
          disabled={action.disabled}
          onclick={() => handleAction(action)}
          role="menuitem"
          aria-label={action.label}
          use:tooltip={action.shortcut ? { content: action.label, shortcut: action.shortcut } : { content: action.label }}
        >
          <ActionIcon size={16} />
        </button>
      {/each}
    </div>
    {#if items.length}<div class="separator" role="separator"></div>{/if}
  {/if}
  {#each items as item (item.id)}
    {#if item.separator}
      <div class="separator" role="separator"></div>
    {:else if item.header}
      <div class="menu-header">{item.label}</div>
    {:else if item.children && item.children.length}
      <div
        class="submenu-wrap"
        role="presentation"
        onmouseenter={() => openParent(item.id)}
        onmouseleave={() => leaveParent(item.id)}
      >
        <button
          class="menu-item submenu-parent"
          class:danger={item.danger}
          class:disabled={item.disabled}
          class:active={openSub === item.id}
          data-sub-id={item.id}
          onclick={() => toggleParent(item.id)}
          role="menuitem"
          aria-haspopup="menu"
          aria-expanded={openSub === item.id}
          disabled={item.disabled}
        >
          {#if item.icon}
            {@const ItemIcon = item.icon}
            <span class="item-icon" style={item.iconColor ? `color:${item.iconColor}` : undefined}>
              <ItemIcon size={13} />
            </span>
          {:else}
            <span class="icon-placeholder"></span>
          {/if}
          <span class="label">{item.label}</span>
          <span class="submenu-caret"><ChevronRight size={13} /></span>
        </button>
        {#if openSub === item.id}
          <div class="context-submenu" class:flip-left={subFlip} bind:this={subEl} role="menu"
               style="transform: translateY({subShift}px)">
            {@render plainList(item.children)}
          </div>
        {/if}
      </div>
    {:else}
      {@render leaf(item)}
    {/if}
  {/each}
</div>

<!-- A flat list of leaves / headers / separators (no nesting) — used for the
     top-level fall-through and for each submenu flyout. -->
{#snippet plainList(list: MenuItem[])}
  {#each list as item (item.id)}
    {#if item.separator}
      <div class="separator" role="separator"></div>
    {:else if item.header}
      <div class="menu-header">{item.label}</div>
    {:else}
      {@render leaf(item)}
    {/if}
  {/each}
{/snippet}

{#snippet leaf(item: MenuItem)}
  <button
    class="menu-item"
    class:danger={item.danger}
    class:disabled={item.disabled}
    onclick={() => handleItem(item)}
    role="menuitem"
    disabled={item.disabled}
  >
    {#if item.icon}
      {@const ItemIcon = item.icon}
      <span class="item-icon" style={item.iconColor ? `color:${item.iconColor}` : undefined}>
        <ItemIcon size={13} />
      </span>
    {:else}
      <span class="icon-placeholder"></span>
    {/if}
    {#if item.subtitle}
      <span class="label-stack">
        <span class="label">{item.label}</span>
        <span class="sublabel">{item.subtitle}</span>
      </span>
    {:else}
      <span class="label">{item.label}</span>
    {/if}
    {#if item.badge}
      <span class="item-badge" class:accent={item.badgeAccent}>{item.badge}</span>
    {/if}
    {#if item.action}
      <span class="shortcut-slot"><Kbd action={item.action} variant="inline" /></span>
    {:else if item.shortcut}
      <span class="shortcut-slot"><Kbd label={item.shortcut} variant="inline" /></span>
    {/if}
  </button>
{/snippet}

<style>
  .context-menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: calc(var(--z-menu) - 1);
    background: transparent;
  }

  .context-menu {
    position: fixed;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-popup);
    padding: 4px;
    min-width: 180px;
    max-width: 280px;
    z-index: var(--z-menu);
  }

  /* Windows-11-style horizontal icon bar pinned to the top of the menu. */
  .quick-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 2px;
  }
  .quick-action {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 1;
    height: 30px;
    min-width: 38px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .quick-action:hover:not(:disabled) { background: var(--bg-selected); }
  .quick-action:focus-visible { outline: none; background: var(--bg-selected); box-shadow: inset 0 0 0 1.5px var(--accent); }
  .quick-action:disabled { opacity: 0.4; cursor: not-allowed; }
  .quick-action.danger { color: var(--error); }
  .quick-action.danger:hover:not(:disabled) { background: var(--error-subtle); }

  .menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 5px 8px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    text-align: left;
    transition: background var(--transition-fast);
  }

  .menu-item:hover:not(.disabled) { background: var(--bg-selected); }
  .menu-item:focus-visible { outline: none; background: var(--bg-selected); box-shadow: inset 0 0 0 1.5px var(--accent); }
  .menu-item.danger { color: var(--error); }
  .menu-item.danger:hover:not(.disabled) { background: var(--error-subtle); }
  .menu-item.danger:focus-visible { background: var(--error-subtle); box-shadow: inset 0 0 0 1.5px var(--error); }
  .menu-item.disabled { opacity: 0.4; cursor: not-allowed; }

  /* Submenu parent: keeps its hover background while its flyout is open. */
  .submenu-wrap { position: relative; }
  .menu-item.active:not(.disabled) { background: var(--bg-selected); }
  .submenu-caret { display: inline-flex; flex-shrink: 0; margin-left: 8px; color: var(--text-muted); }

  /* Right-side flyout; flips left when it would spill past the viewport edge. */
  .context-submenu {
    position: absolute;
    top: -5px;
    left: 100%;
    margin-left: 2px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-popup);
    padding: 4px;
    min-width: 180px;
    max-width: 280px;
    /* Last-resort guard for a flyout taller than the window: cap to the
       viewport and scroll. The vertical shift (subShift) handles the common
       near-the-bottom case so this rarely engages. */
    max-height: calc(100vh - 16px);
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
    z-index: 1;
  }
  .context-submenu.flip-left {
    left: auto;
    right: 100%;
    margin-left: 0;
    margin-right: 2px;
  }

  .item-icon { display: inline-flex; align-items: center; flex-shrink: 0; }
  .icon-placeholder { width: 13px; height: 13px; flex-shrink: 0; }
  .label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* Two-line variant: stack main label + muted subtitle */
  .label-stack { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; line-height: 1.25; }
  .label-stack .label { white-space: normal; }
  .sublabel {
    font-size: 10.5px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .shortcut-slot { margin-left: 8px; flex-shrink: 0; }

  .separator {
    height: 1px;
    background: var(--border);
    margin: 5px 6px;
  }

  .menu-header {
    padding: 4px 8px 2px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
    user-select: none;
  }

  .item-badge {
    font-size: 10px;
    font-weight: 600;
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    background: var(--bg-overlay);
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .item-badge.accent {
    background: color-mix(in srgb, var(--accent) 15%, transparent);
    color: var(--accent);
  }

</style>
