<script lang="ts" module>
  /**
   * One row of the picker. Deliberately thin: the caller looks the real object
   * up by `id`, so this component never has to know whether it is listing
   * folders or files.
   */
  export interface PickerRow {
    /** Project-relative path — the identity of both a folder and a file. */
    id: string;
    /** Tree depth, for the indent. Files sit flat.  */
    depth?: number;
  }
</script>

<script lang="ts">
  /**
   * Find a thing, with the arrows, without a mouse.
   *
   * The filter box and the list under it, shared by both classify dialogs. It is
   * here because the *keyboard* is the part worth having once: arrows walk the
   * filtered list from wherever focus is — including from inside the search box,
   * which is where focus starts and where it stays while you type — and the
   * selected row is scrolled into view rather than merely marked. That behaviour
   * written twice is one of the two copies quietly losing a case.
   *
   * `Ctrl+Enter` is deliberately **not** handled here: it submits the dialog,
   * and the dialog owns its own submit. Letting it bubble means it works from
   * the editor pane below just as well as from the list.
   */
  import type { Snippet } from 'svelte';
  import { Search } from 'lucide-svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';

  interface Props {
    /** Rows to show — already filtered by the caller with `query`. */
    rows: PickerRow[];
    query: string;
    selectedId: string;
    onPick: (id: string) => void;
    /** Double-click, and whatever else means "this one, now". */
    onSubmit: () => void;
    placeholder: string;
    ariaLabel: string;
    row: Snippet<[PickerRow]>;
    /** Shown when nothing matches — the caller phrases it, it knows the query. */
    empty: Snippet;
  }

  let {
    rows,
    query = $bindable(),
    selectedId,
    onPick,
    onSubmit,
    placeholder,
    ariaLabel,
    row,
    empty,
  }: Props = $props();

  let listEl = $state<HTMLDivElement | undefined>();

  function reveal(id: string) {
    listEl?.querySelector(`[data-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: 'nearest' });
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    if (!rows.length) return;
    e.preventDefault();
    const i = rows.findIndex((r) => r.id === selectedId);
    const next = i < 0
      ? (e.key === 'ArrowDown' ? 0 : rows.length - 1)
      : (i + (e.key === 'ArrowDown' ? 1 : -1) + rows.length) % rows.length;
    onPick(rows[next].id);
    reveal(rows[next].id);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="cp" role="group" onkeydown={onKeydown}>
  <div class="cp-toolbar">
    <SearchBar
      bind:query
      showRegex={false}
      showCounter={false}
      {placeholder}
      {ariaLabel}
      autofocus
    />
  </div>

  <div class="cp-list" bind:this={listEl}>
    {#if !rows.length}
      <div class="cp-empty">
        <Search size={22} strokeWidth={1.5} />
        {@render empty()}
      </div>
    {/if}

    {#each rows as item (item.id)}
      <button
        class="cp-row"
        class:cp-selected={selectedId === item.id}
        data-id={item.id}
        style:padding-left="{12 + (item.depth ?? 0) * 12}px"
        onclick={() => onPick(item.id)}
        ondblclick={onSubmit}
      >
        {@render row(item)}
      </button>
    {/each}
  </div>
</div>

<style>
  .cp { display: flex; flex-direction: column; flex: 1; min-height: 0; }
  .cp-toolbar { flex-shrink: 0; padding: 12px 16px; border-bottom: 1px solid var(--border-subtle); }
  .cp-list { flex: 1; min-height: 0; overflow-y: auto; padding: 6px 0; }

  .cp-row {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    padding: 4px 16px 4px 12px;
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .cp-row:hover { background: var(--bg-hover); }
  .cp-selected { background: var(--accent-subtle); }

  .cp-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 40px 16px;
    color: var(--text-disabled);
  }
</style>
