<script lang="ts">
  import { computePosition, flip, shift, offset } from '@floating-ui/dom';
  import { fly, fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';

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
  }

  let {
    items,
    x = 0,
    y = 0,
    onSelect,
    onClose,
  }: {
    items: MenuItem[];
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

  function handleItem(item: MenuItem) {
    if (item.disabled || item.separator) return;
    onSelect(item.id);
    onClose();
  }

  function handleKeydown(e: KeyboardEvent) {
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
  in:fly={{ y: -6, duration: animStore.dFast, easing: cubicOut }}
  out:fade={{ duration: animStore.dFast }}
>
  {#each items as item (item.id)}
    {#if item.separator}
      <div class="separator" role="separator"></div>
    {:else if item.header}
      <div class="menu-header">{item.label}</div>
    {:else}
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
    {/if}
  {/each}
</div>

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
  .menu-item.danger { color: var(--error); }
  .menu-item.danger:hover:not(.disabled) { background: var(--error-subtle); }
  .menu-item.disabled { opacity: 0.4; cursor: not-allowed; }

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
