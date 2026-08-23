<script lang="ts">
  import type { Snippet } from 'svelte';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { ChevronRight } from 'lucide-svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  type BadgeVariant = 'default' | 'tag' | 'stash';

  interface Props {
    label: string;
    expanded?: boolean;
    icon?: Snippet;
    iconColor?: string;
    badge?: number | string | null;
    badgeVariant?: BadgeVariant;
    /**
     * Custom CSS color for the badge (e.g. `var(--graph-lane-0)`). When set,
     * overrides `badgeVariant` and tints the badge using the same 12%/25%
     * background/border pattern as the named variants.
     */
    badgeColor?: string;
    badgeTitle?: string;
    actions?: Snippet;
    children: Snippet;
    onToggle?: () => void;
    /**
     * Right-click on the header, as a **point** rather than as an event.
     *
     * A point because the keyboard opens a context menu too (<kbd>Shift</kbd>+<kbd>F10</kbd>,
     * the Menu key) and a key press has no cursor to sit under — so the widget turns both
     * routes into the one thing a menu actually needs, instead of every consumer deriving the
     * header's rectangle for itself.
     */
    onContextMenu?: (x: number, y: number) => void;
  }

  let {
    label,
    expanded = $bindable(false),
    icon,
    iconColor,
    badge = null,
    badgeVariant = 'default',
    badgeColor,
    badgeTitle,
    actions,
    children,
    onToggle,
    onContextMenu,
  }: Props = $props();

  function toggle() {
    expanded = !expanded;
    onToggle?.();
  }

  /** The keyboard's way in. Anchored to the header's bottom-left, where a menu opened by a
   *  right-click on the row would also start. */
  function onHeaderKeydown(e: KeyboardEvent) {
    if (!onContextMenu) return;
    if (e.key !== 'ContextMenu' && !(e.key === 'F10' && e.shiftKey)) return;
    e.preventDefault();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    onContextMenu(r.left + 8, r.bottom);
  }
</script>

<div class="section">
  <!-- The handler sits on the HEADER row, not on the section: a section body holds rows — and
       often nested sections — that have menus of their own, and a right-click there must not
       bubble up into this one's. -->
  <div
    class="section-header-row"
    role="presentation"
    oncontextmenu={onContextMenu
      ? (e) => { e.preventDefault(); onContextMenu(e.clientX, e.clientY); }
      : undefined}
  >
    <button
      class="section-header"
      class:open={expanded}
      onclick={toggle}
      onkeydown={onHeaderKeydown}
      aria-expanded={expanded}
    >
      <span class="section-chevron" class:open={expanded}>
        <ChevronRight size={11} />
      </span>
      {#if icon}
        <span class="section-icon" style:color={iconColor ?? null}>
          {@render icon()}
        </span>
      {/if}
      <span class="section-label">{label}</span>
    </button>

    {#if actions}
      <span class="section-actions">{@render actions()}</span>
    {/if}

    {#if badge != null && badge !== ''}
      <span
        class="badge row-badge {badgeColor ? 'variant-custom' : `variant-${badgeVariant}`}"
        style={badgeColor ? `--badge-color: ${badgeColor};` : null}
        use:tooltip={badgeTitle ?? ''}
      >{badge}</span>
    {/if}
  </div>

  {#if expanded}
    <div class="section-body" transition:slide={{ duration: animStore.dPanel, easing: cubicOut }}>
      {@render children()}
    </div>
  {/if}
</div>

<style>
  .section { margin-bottom: 1px; }

  .section-header-row {
    display: flex;
    align-items: center;
    position: relative;
  }

  .section-header {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 5px;
    width: 100%;
    padding: 5px 10px 5px 4px;
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    font-weight: 600;
    text-align: left;
    position: relative;
    border-radius: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .section-header:hover { background: rgba(255,255,255,0.04); color: var(--text-primary); }

  .section-header::before {
    content: '';
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 2px;
    background: transparent;
    transition: background var(--transition-fast);
  }
  .section-header.open::before,
  .section-header:focus-visible::before {
    background: var(--accent);
  }

  .section-chevron {
    display: flex;
    align-items: center;
    width: 14px;
    flex-shrink: 0;
    color: var(--text-disabled);
    transition: transform var(--transition-fast), color var(--transition-fast);
  }
  .section-chevron.open {
    transform: rotate(90deg);
    color: var(--text-muted);
  }

  .section-icon {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    margin-right: 1px;
  }

  .section-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-secondary);
    font-size: var(--font-size-sm);
  }

  /* Hover-revealed action buttons (cleanup, add, …) */
  .section-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--transition-fast);
  }
  /* `:focus-within` as well as hover: an action button reached with Tab would otherwise take
     the focus ring while still invisible, which is a place the keyboard can get stuck. */
  .section-header-row:hover .section-actions,
  .section-header-row:focus-within .section-actions {
    opacity: 1;
    pointer-events: auto;
  }

  /* Count badge */
  .badge {
    font-size: var(--font-size-2xs);
    font-weight: 500;
    min-width: 18px;
    height: 16px;
    line-height: 16px;
    text-align: center;
    padding: 0 5px;
    border-radius: 999px;
    letter-spacing: 0;
    flex-shrink: 0;
  }
  .row-badge { margin-right: 10px; }

  .variant-default {
    background: rgba(255,255,255,0.06);
    color: var(--text-muted);
    border: 1px solid rgba(255,255,255,0.08);
  }
  .variant-tag {
    background: color-mix(in srgb, var(--color-tag) 12%, transparent);
    color: var(--color-tag);
    border: 1px solid color-mix(in srgb, var(--color-tag) 25%, transparent);
  }
  .variant-stash {
    background: color-mix(in srgb, var(--color-stash) 12%, transparent);
    color: var(--color-stash);
    border: 1px solid color-mix(in srgb, var(--color-stash) 25%, transparent);
  }
  .variant-custom {
    background: color-mix(in srgb, var(--badge-color) 12%, transparent);
    color: var(--badge-color);
    border: 1px solid color-mix(in srgb, var(--badge-color) 25%, transparent);
  }

  /* Body — IntelliJ-style indented guideline */
  .section-body {
    position: relative;
    padding-left: 20px;
    margin-bottom: 2px;
  }
  .section-body::before {
    content: '';
    position: absolute;
    left: 17px;
    top: 0;
    bottom: 4px;
    width: 1px;
    background: var(--border-subtle);
  }
</style>
