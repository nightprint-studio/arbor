<script lang="ts">
  /**
   * Generic, recursive tree view.
   *
   * Designed for compile-action–style tree sidebars: a few hundred nodes,
   * expand/collapse, single-selection, right-click context menu, optional
   * per-row decorators (badge / status icons) and on-hover action buttons.
   *
   * Virtual scrolling is intentionally NOT implemented — the typical use
   * case fits comfortably in the DOM. If a host ever needs >2k visible
   * nodes, swap the recursive render with a flattened windowed list.
   */
  import { ChevronRight, ChevronDown } from 'lucide-svelte';
  import PluginIcon from '$lib/components/plugins/PluginIcon.svelte';
  import type { TreeNode } from '$lib/types/contribution';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    nodes:        TreeNode[];
    /** Currently selected node id (across the whole tree). */
    selectedId?:  string | null;
    /** Search filter — when non-empty, only matching nodes (and their
     *  ancestors) are rendered. Match is case-insensitive on `label`. */
    filter?:      string;
    /** Action buttons rendered on the right of each row, only visible on
     *  hover. Receives the node — typically returns a fragment of buttons. */
    nodeActions?: import('svelte').Snippet<[TreeNode]>;
    /** Decorator slot rendered always-on, between the label and the action
     *  zone. Use it for permanent badges (status icon, count). */
    nodeDecorator?: import('svelte').Snippet<[TreeNode]>;
    /** Single click — typically updates `selectedId` upstream. */
    onSelect?:    (node: TreeNode) => void;
    /** Double-click / Enter on a selectable node. */
    onActivate?:  (node: TreeNode) => void;
    /** Right-click. Receives DOM coordinates so the parent can position a
     *  ContextMenu. */
    onContextMenu?: (node: TreeNode, x: number, y: number) => void;
  }
  let {
    nodes,
    selectedId    = null,
    filter        = '',
    nodeActions,
    nodeDecorator,
    onSelect,
    onActivate,
    onContextMenu,
  }: Props = $props();

  // ── Expand state ──────────────────────────────────────────────────────────
  // Tracks user-driven expansion overrides. Initial values come from
  // `node.expanded` on first render; once the user toggles, the override
  // sticks until the node id disappears.
  let expandOverride = $state<Record<string, boolean>>({});

  function isExpanded(node: TreeNode): boolean {
    if (node.id in expandOverride) return expandOverride[node.id];
    return !!node.expanded;
  }

  function toggle(node: TreeNode) {
    expandOverride = { ...expandOverride, [node.id]: !isExpanded(node) };
  }

  // ── Filter ────────────────────────────────────────────────────────────────
  // When a filter is active, walk the tree and keep nodes whose label matches
  // OR whose subtree contains a match (so context is preserved).
  const normalizedFilter = $derived(filter.trim().toLowerCase());

  function matches(node: TreeNode): boolean {
    if (!normalizedFilter) return true;
    if (node.label.toLowerCase().includes(normalizedFilter)) return true;
    return node.children?.some(matches) ?? false;
  }

  // When filtering, force-expand any matching subtree so the user sees the
  // hits immediately. We keep this independent from `expandOverride` to avoid
  // polluting user-driven state.
  function effectiveExpanded(node: TreeNode): boolean {
    if (normalizedFilter && node.children?.length) return true;
    return isExpanded(node);
  }

  function handleRowClick(node: TreeNode, e: MouseEvent) {
    e.stopPropagation();
    if (node.children?.length) toggle(node);
    if (node.selectable) onSelect?.(node);
  }

  function handleDblClick(node: TreeNode, e: MouseEvent) {
    e.stopPropagation();
    if (node.selectable || node.default_action) onActivate?.(node);
  }

  function handleContextMenu(node: TreeNode, e: MouseEvent) {
    if (!onContextMenu) return;
    e.preventDefault();
    e.stopPropagation();
    onContextMenu(node, e.clientX, e.clientY);
  }
</script>

{#snippet renderNode(node: TreeNode, depth: number)}
  {#if matches(node)}
    {@const expanded = effectiveExpanded(node)}
    {@const hasChildren = node.children && node.children.length > 0}
    {@const isSection = node.kind === 'section'}
    {@const isSelected = !!selectedId && node.id === selectedId}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="tree-row"
      class:section={isSection}
      class:selected={isSelected}
      class:has-children={hasChildren}
      style:padding-left="{depth * 14 + 6}px"
      role="treeitem"
      aria-selected={isSelected}
      aria-expanded={hasChildren ? expanded : undefined}
      tabindex={-1}
      onclick={(e) => handleRowClick(node, e)}
      ondblclick={(e) => handleDblClick(node, e)}
      oncontextmenu={(e) => handleContextMenu(node, e)}
      use:tooltip={node.label}
    >
      <!-- Disclosure caret. Always reserves space (12px) so labels align
           between leaves and parents. -->
      <span class="caret">
        {#if hasChildren}
          {#if expanded}
            <ChevronDown size={11} />
          {:else}
            <ChevronRight size={11} />
          {/if}
        {/if}
      </span>

      {#if node.icon}
        <span class="icon"><PluginIcon name={node.icon} size={13} /></span>
      {/if}

      <span class="label">{node.label}</span>

      {#if node.badge}
        <span class="badge badge-{node.badge_kind ?? 'muted'}">{node.badge}</span>
      {/if}

      {#if nodeDecorator}
        <span class="decorator">{@render nodeDecorator(node)}</span>
      {/if}

      {#if nodeActions}
        <span class="actions">{@render nodeActions(node)}</span>
      {/if}
    </div>

    {#if hasChildren && expanded}
      <div class="tree-children">
        {#each node.children as child (child.id)}
          {@render renderNode(child, depth + 1)}
        {/each}
      </div>
    {/if}
  {/if}
{/snippet}

<div class="tree-view">
  {#if !nodes || nodes.length === 0}
    <div class="empty-state">No items.</div>
  {:else}
    {#each nodes as node (node.id)}
      {@render renderNode(node, 0)}
    {/each}
  {/if}
</div>

<style>
  .tree-view {
    display: flex;
    flex-direction: column;
    padding: 4px 0;
    user-select: none;
  }

  .empty-state {
    padding: 18px 12px;
    text-align: center;
    font-size: 11px;
    color: var(--text-muted);
    font-style: italic;
  }

  .tree-row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 5px;
    height: 22px;
    min-height: 22px;
    padding-right: 6px;
    color: var(--text-primary);
    font-size: 12px;
    cursor: default;
    transition: background var(--transition-fast);
  }
  .tree-row:hover {
    background: var(--bg-hover);
  }
  .tree-row.has-children { cursor: pointer; }
  .tree-row.selected {
    /* Match the row-selection visual used elsewhere in the IDE — a soft
       accent fill that survives both hover and focus. */
    background: var(--accent-subtle);
    color: var(--text-primary);
  }
  .tree-row.section {
    /* Section rows look like IntelliJ's group headers: muted, slightly
       smaller, no selection highlight. They still expand on click. */
    color: var(--text-secondary);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
  }
  .tree-row.section:hover { background: var(--bg-overlay); }
  .tree-row.section.selected { background: transparent; }

  .caret {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 12px;
    height: 12px;
    flex-shrink: 0;
    color: var(--text-muted);
  }

  .icon {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
    color: var(--text-secondary);
  }
  .tree-row.selected .icon { color: var(--accent); }

  .label {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .badge {
    flex-shrink: 0;
    font-family: var(--font-code);
    font-size: 10px;
    line-height: 1;
    padding: 2px 6px;
    border-radius: 999px;
    white-space: nowrap;
    border: 1px solid transparent;
  }
  .badge-muted   { color: var(--text-muted);   background: var(--bg-overlay); }
  .badge-info    { color: var(--text-secondary); background: var(--bg-elevated); }
  .badge-accent  { color: var(--accent);       background: var(--accent-subtle); border-color: color-mix(in srgb, var(--accent) 30%, transparent); }
  .badge-success { color: var(--success); background: color-mix(in srgb, var(--success) 14%, transparent); border-color: color-mix(in srgb, var(--success) 30%, transparent); }
  .badge-warning { color: var(--warning); background: color-mix(in srgb, var(--warning) 14%, transparent); border-color: color-mix(in srgb, var(--warning) 30%, transparent); }
  .badge-error   { color: var(--error);   background: color-mix(in srgb, var(--error) 14%, transparent);   border-color: color-mix(in srgb, var(--error) 30%, transparent); }

  .decorator {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    flex-shrink: 0;
    color: var(--text-muted);
  }

  /* Action zone — hidden at rest, fades in on hover/focus. Same affordance
     used by .card-item-actions in PluginSidebarPanel for visual continuity. */
  .actions {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    flex-shrink: 0;
    opacity: 0;
    transform: translateX(3px);
    transition: opacity var(--transition-fast), transform var(--transition-fast);
  }
  .tree-row:hover .actions,
  .tree-row.selected .actions {
    opacity: 1;
    transform: none;
  }

  /* Compact action button sized to fit a 22px row without overflowing.
     Children of `.actions` (rendered via the snippet) inherit this style by
     applying `class="tree-row-action"`. Plugins / hosts using the snippet
     can simply use a <button class="tree-row-action"> per item. */
  :global(.tree-row-action) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    padding: 0;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  :global(.tree-row-action:hover) {
    background: var(--bg-base);
    color: var(--text-primary);
  }
  :global(.tree-row-action.accent:hover)  { color: var(--accent); }
  :global(.tree-row-action.danger:hover)  { color: var(--error); }
  :global(.tree-row-action:disabled)      { opacity: 0.35; cursor: not-allowed; }
</style>
