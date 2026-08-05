<script lang="ts">
  /**
   * The variables tree — one row per variable, field or array element, expanding lazily.
   *
   * Recursive, and deliberately dumb: everything it shows was rendered by the backend, which
   * is the only side that can read the VM. What is here is the disclosure triangle, the
   * indentation, and the decision to fetch children only when a row is actually opened — a
   * stopped program has an object graph, and walking it eagerly would be a round trip per node
   * for rows nobody looked at.
   *
   * Values are shown as the debugger can honestly know them: a string is its text, an object
   * is `Order@1f3c` with its real fields underneath. Calling `toString()` would read better
   * and would mean running application code inside a paused program — which can block on a
   * lock the suspended thread holds, mutate state, or throw.
   */
  import { ChevronRight } from 'lucide-svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  // Itself, for the recursion. `<svelte:self>` is the Svelte 4 spelling and is deprecated in
  // runes mode — a component importing itself is the supported one.
  import BennuVarTree from './BennuVarTree.svelte';
  import { bennuDebugStore, type VarNode } from '$lib/stores/bennu/debug.svelte';

  let { nodes, depth = 0 }: { nodes: VarNode[]; depth?: number } = $props();

  /** The badge before an argument, so the method's inputs are visible without reading names. */
  function tag(kind: string): string {
    if (kind === 'argument') return 'arg';
    if (kind === 'this') return 'this';
    if (kind === 'static') return 'static';
    return '';
  }
</script>

{#each nodes as node (node.id)}
  <div class="vt-row" style="padding-left: {depth * 14 + 4}px">
    {#if node.value.object}
      <button
        class="vt-twist"
        class:open={node.open}
        type="button"
        aria-label={node.open ? 'Collapse' : 'Expand'}
        aria-expanded={node.open}
        onclick={() => void bennuDebugStore.toggleNode(node)}
      >
        <ChevronRight size={12} />
      </button>
    {:else}
      <span class="vt-twist vt-twist-empty"></span>
    {/if}

    <span class="vt-name">{node.value.name}</span>
    {#if tag(node.value.kind)}<span class="vt-tag">{tag(node.value.kind)}</span>{/if}
    <span class="vt-eq">=</span>
    <span class="vt-value" title={node.value.value}>{node.value.value}</span>
    {#if node.value.type_name}
      <span class="vt-type">{node.value.type_name}</span>
    {/if}
    {#if node.loading}<Spinner size={11} />{/if}
  </div>

  {#if node.open}
    {#if node.error}
      <div class="vt-error" style="padding-left: {depth * 14 + 22}px">{node.error}</div>
    {:else if node.children}
      <!-- `[]` after a fetch is a real answer — an object with no fields of its own. -->
      {#if node.children.length === 0}
        <div class="vt-empty" style="padding-left: {depth * 14 + 22}px">no fields</div>
      {:else}
        <BennuVarTree nodes={node.children} depth={depth + 1} />
      {/if}
    {/if}
  {/if}
{/each}

<style>
  .vt-row {
    display: flex; align-items: center; gap: 5px;
    padding-top: 1px; padding-bottom: 1px; padding-right: 8px;
    font-family: var(--font-code); font-size: 11.5px; line-height: 1.6;
    white-space: nowrap;
  }
  .vt-row:hover { background: var(--bg-hover); }

  .vt-twist {
    display: inline-flex; align-items: center; justify-content: center;
    width: 14px; height: 14px; flex: 0 0 14px;
    padding: 0; border: 0; background: none; cursor: pointer;
    color: var(--text-muted);
    transition: transform var(--transition-fast), color var(--transition-fast);
  }
  .vt-twist.open { transform: rotate(90deg); }
  .vt-twist:hover { color: var(--text-primary); }
  .vt-twist-empty { cursor: default; }

  .vt-name { color: var(--syntax-field, #9876aa); }
  .vt-tag {
    font-size: 9px; text-transform: uppercase; letter-spacing: 0.04em;
    padding: 0 4px; border-radius: var(--radius-sm);
    background: var(--bg-elevated); color: var(--text-muted);
  }
  .vt-eq { color: var(--text-muted); }
  .vt-value {
    color: var(--text-primary);
    overflow: hidden; text-overflow: ellipsis; max-width: 60ch;
  }
  .vt-type { color: var(--text-muted); font-size: 10.5px; margin-left: 4px; }

  .vt-error, .vt-empty {
    font-size: 11px; padding-top: 1px; padding-bottom: 3px;
    font-family: var(--font-code);
  }
  .vt-error { color: var(--error); }
  .vt-empty { color: var(--text-muted); font-style: italic; }
</style>
