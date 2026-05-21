<script lang="ts" module>
  /**
   * Breadcrumb — horizontal trail of chip-like segments.
   *
   * Generic. Each `segment` carries a `label`, optional `icon`, optional
   * `badge`, optional `tooltip`, and an opaque `value` the consumer maps
   * back to its own model on click via the `onSelect` callback.
   *
   * Segments without `onSelect` participation (last/current) can be marked
   * non-interactive by setting `interactive = false` on the segment.
   *
   * Used by `PluginTreeSidebar.svelte` to render the tree-level breadcrumb
   * band fed by `arbor.ui.tree.set { breadcrumb = ... }`, but it has no
   * coupling to that flow — anything that needs a path-style trail can
   * reuse it (file panel, plugin settings drill-down, etc.).
   */
  export interface BreadcrumbSegment<V = unknown> {
    /** Display label. */
    label:        string;
    /** Lucide name, emoji, or `plugin:<plugin>:<icon_id>` reference. */
    icon?:        string | null;
    /** Small label shown right of `label` (e.g. "current"). */
    badge?:       string | null;
    /** Native title-tooltip text. */
    tooltip?:     string | null;
    /** When false the segment is greyed-out and not clickable. */
    interactive?: boolean;
    /** Opaque payload returned to `onSelect`. */
    value?:       V;
  }
</script>

<script lang="ts" generics="V">
  import { ChevronRight, Pencil, Check, X } from 'lucide-svelte';
  import PluginIcon from '$lib/components/plugins/PluginIcon.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    segments:  BreadcrumbSegment<V>[];
    /** Fired when an interactive segment is clicked. */
    onSelect?: (value: V | undefined, segment: BreadcrumbSegment<V>, index: number) => void;
    /**
     * Soft cap on visible segments. When `segments.length > max`, the
     * middle is collapsed to an ellipsis chip, preserving the first and the
     * last `max - 2` segments (so the user always sees both ends).
     */
    max?: number;
    /** When set, a pencil button appears at the right end. Clicking it (or
     *  double-clicking an empty area of the breadcrumb) swaps the chip trail
     *  for a text input. The user types a path and presses Enter — the
     *  callback receives the raw string. Escape cancels. */
    editable?:        boolean;
    /** Current path serialised as a string, prefilled in the edit input. */
    editValue?:       string;
    /** Placeholder shown inside the edit input. */
    editPlaceholder?: string;
    /** Fired when the user commits the edited path. */
    onCommit?:        (path: string) => void;
  }
  let {
    segments, onSelect, max = 6,
    editable = false,
    editValue = '',
    editPlaceholder = 'Type a path (e.g. x/y/z)',
    onCommit,
  }: Props = $props();

  let editing  = $state(false);
  let draft    = $state('');
  let inputEl  = $state<HTMLInputElement | null>(null);

  function startEdit() {
    if (!editable) return;
    draft = editValue;
    editing = true;
    // Focus + select on next tick so the input is mounted.
    queueMicrotask(() => {
      inputEl?.focus();
      inputEl?.select();
    });
  }
  function commitEdit() {
    if (!editing) return;
    editing = false;
    const trimmed = draft.trim();
    onCommit?.(trimmed);
  }
  function cancelEdit() {
    editing = false;
    draft = '';
  }
  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter')  { e.preventDefault(); commitEdit(); }
    if (e.key === 'Escape') { e.preventDefault(); cancelEdit(); }
  }

  const visible = $derived.by(() => {
    if (segments.length <= max) return segments.map((s, i) => ({ kind: 'seg' as const, s, i }));
    const head = segments[0];
    const tail = segments.slice(segments.length - (max - 2));
    return [
      { kind: 'seg' as const, s: head, i: 0 },
      { kind: 'ellipsis' as const },
      ...tail.map((s, k) => ({ kind: 'seg' as const, s, i: segments.length - tail.length + k })),
    ];
  });

  function click(seg: BreadcrumbSegment<V>, i: number) {
    if (seg.interactive === false) return;
    onSelect?.(seg.value, seg, i);
  }
</script>

<nav class="breadcrumb" aria-label="Breadcrumb" ondblclick={editable && !editing ? startEdit : null}>
  {#if editable && editing}
    <input
      bind:this={inputEl}
      bind:value={draft}
      class="bc-edit-input"
      type="text"
      placeholder={editPlaceholder}
      onkeydown={onKey}
      onblur={commitEdit}
    />
    <button class="bc-edit-btn" type="button"
            onmousedown={(e) => e.preventDefault()}
            onclick={commitEdit}
            use:tooltip={'Go to path'}>
      <Check size={12} />
    </button>
    <button class="bc-edit-btn" type="button"
            onmousedown={(e) => e.preventDefault()}
            onclick={cancelEdit}
            use:tooltip={'Cancel'}>
      <X size={12} />
    </button>
  {:else}
    {#each visible as item, idx (idx)}
      {#if item.kind === 'ellipsis'}
        <span class="bc-ellipsis" aria-hidden="true">…</span>
        <ChevronRight class="bc-sep" size={12} />
      {:else}
        {@const seg = item.s}
        {@const interactive = seg.interactive !== false}
        <button
          type="button"
          class="bc-seg"
          class:bc-seg-static={!interactive}
          disabled={!interactive}
          use:tooltip={seg.tooltip ?? ''}
          onclick={() => click(seg, item.i)}
        >
          {#if seg.icon}<PluginIcon name={seg.icon} size={12} />{/if}
          <span class="bc-label">{seg.label}</span>
          {#if seg.badge}<span class="bc-badge">{seg.badge}</span>{/if}
        </button>
        {#if idx < visible.length - 1}
          <ChevronRight class="bc-sep" size={12} />
        {/if}
      {/if}
    {/each}
    {#if editable}
      <button class="bc-edit-toggle" type="button"
              onclick={startEdit}
              use:tooltip={'Edit path (double-click anywhere to enter)'}>
        <Pencil size={11} />
      </button>
    {/if}
  {/if}
</nav>

<style>
  .breadcrumb {
    display: flex;
    align-items: center;
    /* Wrap to multiple rows on narrow sidebars instead of forcing a horizontal
       scrollbar — deep paths are common in object storage and the user needs
       every segment visible to navigate up. `row-gap` keeps wrapped rows tidy. */
    flex-wrap: wrap;
    gap: 4px 2px;
    padding: 4px 8px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border-color);
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    font-size: 11px;
    line-height: 1;
    user-select: none;
  }

  .bc-seg {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 7px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
    color: var(--text-secondary);
    cursor: pointer;
    flex-shrink: 0;
    transition: background 90ms ease, color 90ms ease, border-color 90ms ease;
  }
  .bc-seg:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--border-color);
  }
  .bc-seg:focus-visible {
    outline: none;
    box-shadow: 0 0 0 2px var(--accent-color, #4a8aff) inset;
  }
  .bc-seg-static {
    cursor: default;
    color: var(--text-primary);
    font-weight: 600;
  }
  .bc-seg-static:hover {
    background: transparent;
    border-color: transparent;
  }

  .bc-label {
    /* Folder names can be long — let them ellipsis instead of pushing the
       row sideways. Min-width keeps the icon visible on tiny chips. */
    max-width: 18ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bc-badge {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding: 1px 5px;
    border-radius: 999px;
    background: var(--accent-color, #4a8aff);
    color: var(--accent-color-fg, #fff);
    line-height: 1.4;
  }

  :global(.bc-sep) {
    color: var(--text-disabled);
    flex-shrink: 0;
  }

  .bc-ellipsis {
    padding: 0 4px;
    color: var(--text-disabled);
    font-weight: 700;
  }

  .bc-edit-toggle {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    margin-left: auto;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
    color: var(--text-muted);
    cursor: pointer;
    flex-shrink: 0;
  }
  .bc-edit-toggle:hover {
    color: var(--text-primary);
    background: var(--bg-hover);
    border-color: var(--border-color);
  }

  .bc-edit-input {
    flex: 1;
    min-width: 0;
    background: var(--bg-base);
    border: 1px solid var(--accent-color, #4a8aff);
    border-radius: 4px;
    color: var(--text-primary);
    padding: 3px 8px;
    font-family: inherit;
    font-size: 11px;
    line-height: 1;
    outline: none;
  }
  .bc-edit-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    background: transparent;
    border: 1px solid var(--border-color);
    border-radius: 4px;
    color: var(--text-secondary);
    cursor: pointer;
    flex-shrink: 0;
  }
  .bc-edit-btn:hover {
    color: var(--text-primary);
    background: var(--bg-hover);
  }
</style>
