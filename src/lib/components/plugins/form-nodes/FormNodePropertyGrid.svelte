<!--
  FormNodePropertyGrid — read-only-first property / reflection grid.

  Renders a dense IntelliJ-inspector-style list of `label → value` rows:
    · label on the left (dimmed), formatted value on the right
    · code-editor value colouring via `value_tone`
    · right-aligned type pill (u32 / Vec3 / enum / …)
    · nested structs / arrays render as indented sub-rows, optionally
      collapsible
    · immutable rows show a lock glyph
    · `copyable` rows reveal a copy glyph and copy the value client-side
    · editable rows reveal a pencil on hover; clicking swaps the value cell
      for `row.edit_node` rendered through the normal node dispatcher
      (so existing `field` / `vec_field` / `color` / `select` editors work
      unchanged). The editor's own action fires the mutation on commit.

  Generic: any plugin inspecting structured data uses it. The plugin owns
  value formatting + the editor nodes; this component owns only layout and
  the read-only ⇄ edit toggle.

  Receives:
    · node       — the FormNodePropertyGrid
    · ctx        — shared FormNodeCtx
    · renderNode — recursive dispatcher snippet (renders `edit_node`)
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Pencil, Lock, X, Copy, Check, ChevronRight } from 'lucide-svelte';
  import TypePill from '$lib/components/shared/internal/TypePill.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import type { FormNode, PropertyRow } from '$lib/types/plugin';
  import type { FormNodeCtx } from './ctx';

  interface Props {
    node:       FormNode;
    ctx:        FormNodeCtx;
    renderNode: Snippet<[FormNode]>;
  }
  let { node, ctx, renderNode }: Props = $props();

  const rows  = $derived(ctx.toArr<PropertyRow>((node as any).rows));
  const empty = $derived((node as any).empty ?? '(no fields)');

  // One row open for editing at a time (matches the mockup's per-field edit).
  let editingId = $state<string | null>(null);
  // Per-group collapse state, keyed by row id; lazily seeded from `open`.
  let collapsed = $state<Record<string, boolean>>({});
  // Transient "copied ✓" flash, keyed by row id.
  let copied = $state<string | null>(null);

  function rowKey(row: PropertyRow, depth: number, index: number): string {
    return row.id ?? `${depth}:${index}:${row.label}`;
  }
  function canEdit(row: PropertyRow): boolean {
    return !!row.edit_node && !row.locked;
  }
  function isCollapsed(row: PropertyRow, key: string): boolean {
    if (!(key in collapsed)) return row.open === false;
    return collapsed[key];
  }
  function doCopy(key: string, value: string) {
    copyToClipboard(value, { errorToast: true });
    copied = key;
    setTimeout(() => { if (copied === key) copied = null; }, 1100);
  }
</script>

<div class="pf-propgrid">
  {#if rows.length === 0}
    <div class="pf-propgrid-empty">{empty}</div>
  {:else}
    {@render rowList(rows, 0)}
  {/if}
</div>

{#snippet rowList(list: PropertyRow[], depth: number)}
  {#each list as row, i (rowKey(row, depth, i))}
    {@const key      = rowKey(row, depth, i)}
    {@const children = (ctx.toArr(row.children) as PropertyRow[])}
    {@const isGroup  = children.length > 0}
    {@const editing  = editingId === key}
    {@const folded   = isGroup && !!row.collapsible && isCollapsed(row, key)}
    <!-- `role`, `tabindex`, `onclick` and `onkeydown` are all gated on the same
         `isGroup && row.collapsible` condition: when the row is interactive it is
         a `button` with `tabindex=0` + keyboard handler; otherwise all four are
         omitted. The analyzer can't correlate the parallel ternaries, so the
         non-interactive-tabindex warning here is a false positive. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      class="pf-pg-row"
      class:pf-pg-row-group={isGroup}
      class:pf-pg-row-editing={editing}
      class:pf-pg-row-clickable={isGroup && row.collapsible}
      style={depth > 0 ? `padding-left:${depth * 12}px` : undefined}
      role={isGroup && row.collapsible ? 'button' : undefined}
      tabindex={isGroup && row.collapsible ? 0 : undefined}
      onclick={isGroup && row.collapsible ? () => { collapsed[key] = !folded; } : undefined}
      onkeydown={isGroup && row.collapsible
        ? (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); collapsed[key] = !folded; } }
        : undefined}
    >
      <span class="pf-pg-label" title={row.label}>
        {#if isGroup && row.collapsible}
          <ChevronRight size={11} class="pf-pg-chevron {folded ? '' : 'pf-pg-chevron-open'}" />
        {/if}
        {row.label}
      </span>

      <div class="pf-pg-value-cell">
        {#if editing && row.edit_node}
          <!-- Inline editor — delegated to the normal node dispatcher so all
               existing editors (number stepper, vec drag, color, select) are
               reused verbatim. -->
          <div class="pf-pg-editor">
            {@render renderNode(row.edit_node as FormNode)}
          </div>
          <button
            type="button"
            class="pf-pg-iconbtn"
            use:tooltip={'Done'}
            aria-label="Done editing"
            onclick={(e) => { e.stopPropagation(); editingId = null; }}
          ><X size={12} /></button>
        {:else if !isGroup}
          <span
            class="pf-pg-value pf-pg-tone-{row.value_tone ?? 'default'}"
            class:pf-pg-value-muted={row.muted}
            use:tooltip={row.tooltip ?? ''}
          >{row.value ?? ''}</span>
        {/if}

        {#if !editing}
          {#if row.pill}
            <TypePill label={row.pill} kind={row.pill_kind ?? row.pill} tooltip={row.pill_tooltip} />
          {/if}
          {#if row.copyable && !isGroup && row.value}
            <button
              type="button"
              class="pf-pg-iconbtn pf-pg-hover-only"
              class:pf-pg-copied={copied === key}
              use:tooltip={copied === key ? 'Copied' : 'Copy value'}
              aria-label="Copy value"
              onclick={(e) => { e.stopPropagation(); doCopy(key, row.value ?? ''); }}
            >{#if copied === key}<Check size={11} />{:else}<Copy size={11} />{/if}</button>
          {/if}
          {#if row.locked}
            <span class="pf-pg-lock" use:tooltip={'Immutable'}><Lock size={11} /></span>
          {:else if canEdit(row)}
            <button
              type="button"
              class="pf-pg-iconbtn pf-pg-hover-only"
              use:tooltip={'Edit'}
              aria-label="Edit {row.label}"
              onclick={(e) => { e.stopPropagation(); editingId = key; }}
            ><Pencil size={11} /></button>
          {/if}
        {/if}
      </div>
    </div>

    {#if isGroup && !folded}
      <!-- No height animation here: in a dense grid of many cards animating
           nested-group height forces per-frame reflow. Instant is snappier. -->
      {@render rowList(children, depth + 1)}
    {/if}
  {/each}
{/snippet}

<style>
  .pf-propgrid {
    display: flex;
    flex-direction: column;
  }

  .pf-propgrid-empty {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    font-style: italic;
    padding: 4px 0;
  }

  .pf-pg-row {
    display: grid;
    grid-template-columns: minmax(72px, 0.9fr) minmax(0, 1.6fr);
    align-items: center;
    gap: 10px;
    min-height: 22px;
    padding: 1px 2px;
    border-radius: var(--radius-sm);
  }
  .pf-pg-row:hover {
    background: color-mix(in srgb, var(--text-primary) 4%, transparent);
  }
  .pf-pg-row-clickable { cursor: pointer; }
  .pf-pg-row-group { min-height: 20px; }
  .pf-pg-row-group .pf-pg-label {
    color: var(--text-secondary);
    font-weight: 600;
  }
  .pf-pg-row-editing {
    background: color-mix(in srgb, var(--accent) 7%, transparent);
  }

  .pf-pg-label {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    font-family: var(--font-mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  :global(.pf-pg-chevron) {
    flex-shrink: 0;
    color: var(--text-disabled);
    transition: transform var(--transition-fast);
  }
  :global(.pf-pg-chevron-open) { transform: rotate(90deg); }

  .pf-pg-value-cell {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .pf-pg-value {
    font-size: var(--font-size-xs);
    color: var(--text-primary);
    font-family: var(--font-mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1 1 auto;
    min-width: 0;
  }
  .pf-pg-value-muted { color: var(--text-disabled) !important; }

  /* ── Value syntax-highlight tones (code-editor feel) ─────────────────── */
  .pf-pg-tone-default { color: var(--text-primary); }
  .pf-pg-tone-number  { color: #62a0ea; }
  .pf-pg-tone-string  { color: #e0a458; }
  .pf-pg-tone-enum    { color: #74c69d; }
  .pf-pg-tone-bool    { color: #b288f0; }
  .pf-pg-tone-entity  { color: #f08c54; }
  .pf-pg-tone-handle  { color: #c98fe5; }
  .pf-pg-tone-accent  { color: var(--accent); }
  .pf-pg-tone-warn    { color: var(--warning); }
  .pf-pg-tone-muted   { color: var(--text-disabled); }

  .pf-pg-editor {
    flex: 1 1 auto;
    min-width: 0;
  }
  /* The embedded editor brings its own `.pf-field` chrome; strip the outer
     margins + its label so it sits flush in the row. */
  .pf-pg-editor :global(.pf-field) { margin: 0; }
  .pf-pg-editor :global(.pf-label) { display: none; }

  .pf-pg-iconbtn,
  .pf-pg-lock {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    flex-shrink: 0;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    background: transparent;
    border: none;
  }
  .pf-pg-iconbtn { cursor: pointer; }

  /* Hover-only affordances keep the read-only view clean. */
  .pf-pg-hover-only { opacity: 0; transition: opacity var(--transition-fast); }
  .pf-pg-row:hover .pf-pg-hover-only { opacity: 1; }
  .pf-pg-iconbtn:hover {
    color: var(--text-primary);
    background: var(--bg-hover);
  }
  .pf-pg-copied { opacity: 1 !important; color: var(--success); }
  .pf-pg-lock { color: var(--text-disabled); }
</style>
