<script lang="ts">
  /**
   * The resources of a context, as a selectable list.
   *
   * **Delete is on the row**, shown on hover and on focus — a resource is something you remove from
   * the list you are looking at, not from a button in whatever form happens to be open. Selection is
   * the usual one (click; Ctrl/⌘-click toggles; Shift-click extends; Ctrl+A takes all), because more
   * than one selected row is what turns the form into a bulk editor.
   */
  import { Database, Link2, Trash2, TriangleAlert, Variable } from 'lucide-svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Checkbox from '$lib/components/shared/ui/Checkbox.svelte';
  import type { TomcatResource } from '$lib/stores/bennu/tomcat-config';

  let {
    resources,
    selected,
    warned,
    onselect,
    ondelete,
  }: {
    resources: TomcatResource[];
    /** Ids selected, in the order they were taken. */
    selected: string[];
    /** Ids that have something to flag (an empty URL, say). */
    warned: Set<string>;
    /** `mode`: replace the selection, toggle one, or extend from the anchor to this row. */
    onselect: (id: string, mode: 'replace' | 'toggle' | 'range' | 'all') => void;
    ondelete: (ids: string[]) => void;
  } = $props();

  const iconOf = (r: TomcatResource) => (r.kind === 'datasource' ? Database : r.kind === 'env' ? Variable : Link2);

  function modeOf(e: MouseEvent): 'replace' | 'toggle' | 'range' {
    if (e.shiftKey) return 'range';
    return e.ctrlKey || e.metaKey ? 'toggle' : 'replace';
  }

  function onkeydown(e: KeyboardEvent) {
    const at = resources.findIndex((r) => r.id === selected[selected.length - 1]);
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
      e.preventDefault();
      onselect('', 'all');
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const next = resources[Math.max(0, Math.min(resources.length - 1, at + (e.key === 'ArrowDown' ? 1 : -1)))];
      if (next) onselect(next.id, e.shiftKey ? 'range' : 'replace');
    } else if (e.key === 'Delete' && selected.length) {
      e.preventDefault();
      ondelete([...selected]);
    }
  }
</script>

{#if resources.length === 0}
  <p class="none">No resources yet.</p>
{:else}
  <!-- The checkboxes are the visible way to a multiple selection (the modifier clicks stay): more
       than one ticked row turns the form into the bulk editor. -->
  <div class="all">
    <Checkbox
      checked={selected.length === resources.length}
      indeterminate={selected.length > 1 && selected.length < resources.length}
      ariaLabel="Select all resources"
      label={selected.length > 1 ? `${selected.length} selected — edited together` : 'Select several to edit together'}
      onchange={(v) => (v ? onselect('', 'all') : onselect(resources[0].id, 'replace'))}
    />
  </div>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <ul class="list" role="listbox" aria-label="Context resources" aria-multiselectable="true" tabindex="0" {onkeydown}>
    {#each resources as r (r.id)}
      {@const Icon = iconOf(r)}
      <li class="item" class:on={selected.includes(r.id)} role="option" aria-selected={selected.includes(r.id)}>
        <span class="tick">
          <Checkbox
            checked={selected.includes(r.id)}
            ariaLabel="Select {r.name || 'resource'}"
            onchange={() => onselect(r.id, 'toggle')}
          />
        </span>
        <button type="button" class="pick" onclick={(e) => onselect(r.id, modeOf(e))} tabindex="-1">
          <Icon size={13} />
          <span class="name">{r.name || '(unnamed)'}</span>
          {#if warned.has(r.id)}<span class="warn"><TriangleAlert size={12} /></span>{/if}
        </button>
        <span class="del">
          <IconButton tooltip="Delete {r.name || 'resource'}" size={22} variant="danger" onclick={() => ondelete([r.id])}>
            <Trash2 size={13} />
          </IconButton>
        </span>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .none { margin: 8px; color: var(--text-muted); font-size: var(--font-size-xs); }
  .all { padding: 6px 8px 8px; border-bottom: 1px solid var(--border-subtle, var(--border)); margin-bottom: 4px; color: var(--text-muted); font-size: var(--font-size-xs); }
  .tick { padding-left: 6px; display: inline-flex; }
  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 1px; outline: none; }
  .list:focus-visible { box-shadow: 0 0 0 2px var(--accent-subtle); border-radius: var(--radius-sm); }
  .item { display: flex; align-items: center; border-radius: var(--radius-sm); }
  .item:hover { background: var(--bg-hover); }
  .item.on { background: var(--accent-subtle); }
  .pick {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: 8px; padding: 6px 8px;
    border: 0; background: transparent; color: var(--text-secondary); text-align: left; cursor: pointer;
  }
  .item.on .pick { color: var(--text-primary); }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font: var(--font-size-xs) var(--font-code); }
  .warn { color: var(--warning); display: inline-flex; }
  .del { padding-right: 4px; }
  .item:not(:hover):not(:focus-within) .del { visibility: hidden; }
</style>
