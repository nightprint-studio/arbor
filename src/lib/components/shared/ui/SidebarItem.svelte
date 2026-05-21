<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** Visual selection state (highlighted row) */
    selected?: boolean;
    /** Accent background (current branch, nearest tag…) */
    current?: boolean;
    /** CSS color for the current/selected tint */
    currentColor?: string;
    indent?: number;
    onclick?: (e: MouseEvent) => void;
    ondblclick?: (e: MouseEvent) => void;
    oncontextmenu?: (e: MouseEvent) => void;
    /** Left icon area */
    icon?: Snippet;
    /** Main content (label + inline badges) */
    children: Snippet;
    /** Right-side always-visible badges */
    badges?: Snippet;
    /** Action buttons revealed on hover */
    actions?: Snippet;
  }

  let {
    selected = false,
    current = false,
    currentColor,
    indent = 0,
    onclick,
    ondblclick,
    oncontextmenu,
    icon,
    children,
    badges,
    actions,
  }: Props = $props();

  const style = $derived([
    indent ? `padding-left: ${indent}px;` : '',
    current && currentColor ? `--item-accent: ${currentColor};` : '',
  ].filter(Boolean).join(' ') || undefined);
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<div
  class="sidebar-item"
  class:selected
  class:current
  class:has-color={current && currentColor}
  role="button"
  tabindex={-1}
  {style}
  {onclick}
  {ondblclick}
  oncontextmenu={(e) => { e.preventDefault(); oncontextmenu?.(e); }}
>
  {#if icon}
    <span class="item-icon">
      {@render icon()}
    </span>
  {/if}

  <span class="item-content">
    {@render children()}
  </span>

  {#if badges}
    <span class="item-badges">
      {@render badges()}
    </span>
  {/if}

  {#if actions}
    <span class="item-actions">
      {@render actions()}
    </span>
  {/if}
</div>

<style>
  .sidebar-item {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px 3px 4px;
    min-height: 22px;
    cursor: pointer;
    border-radius: 0;
    transition: background var(--transition-fast);
    position: relative;
  }
  .sidebar-item:hover { background: rgba(255, 255, 255, 0.05); }

  .sidebar-item.selected { background: rgba(77, 120, 204, 0.18); }
  .sidebar-item.selected:hover { background: rgba(77, 120, 204, 0.22); }

  .sidebar-item.current {
    background: color-mix(in srgb, var(--item-accent, var(--accent)) 12%, transparent);
    font-weight: 500;
  }
  .sidebar-item.current:hover {
    background: color-mix(in srgb, var(--item-accent, var(--accent)) 18%, transparent);
  }

  .item-icon {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    color: var(--text-muted);
  }

  .item-content {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--font-size-xs);
    color: var(--text-primary);
  }

  .item-badges {
    display: flex;
    align-items: center;
    gap: 3px;
    flex-shrink: 0;
  }

  /* Actions: hidden until row is hovered or selected */
  .item-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--transition-fast);
  }
  .sidebar-item:hover .item-actions,
  .sidebar-item.selected .item-actions {
    opacity: 1;
    pointer-events: auto;
  }

  /* Action buttons inside item-actions */
  :global(.sidebar-item .item-actions button) {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border: none;
    background: transparent;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    padding: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  :global(.sidebar-item .item-actions button:hover) {
    background: var(--bg-overlay);
    color: var(--text-primary);
  }
  :global(.sidebar-item .item-actions button.danger:hover) {
    color: var(--error);
    background: var(--error-subtle);
  }
</style>
