<script lang="ts">
  /**
   * One cell of a `DataGrid`'s filter row: a text box, and a list of the values
   * that are actually in the column.
   *
   * ## Why the list exists
   *
   * A text filter asks you to already know what you are looking for. On a result of
   * any size you do not: finding out which statuses a column holds meant scrolling
   * it and remembering, which is not a thing a person can do past a few screens and
   * is not a thing they should be asked to do at all — the answer is sitting in
   * memory, already loaded, and can be counted in one pass.
   *
   * So the button beside the box opens the column's own values, with how many rows
   * each accounts for, and picking is **exact**: choosing `ROMA` selects the rows
   * whose value is `ROMA`, not the ones containing it. That is the difference
   * between a list you pick from and a needle you type, and conflating the two
   * would make the list lie about what it selected.
   *
   * ## One column, one mood
   *
   * Text and picked values are alternatives, never both at once — see
   * `ColumnFilter`. Choosing values replaces the text and the box gives way to a
   * summary of what is picked; clearing brings the box back.
   *
   * NOTE (shared/ui contract): no Arbor concepts, no IPC/stores — the distinct
   * values arrive through a callback the grid supplies.
   */
  import { ListFilter, X } from 'lucide-svelte';
  import Dropdown, { type DropdownItem } from './Dropdown.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { ColumnFilter, DistinctSet } from './data-grid-filter';

  /**
   * How many values the picker will list.
   *
   * A column with more distinct values than this is not one you pick from — you
   * type at it — so the cap is not a compromise on the feature but a statement of
   * where it stops being the right tool. It is said in the footer rather than
   * silently applied.
   */
  const MAX_LISTED = 400;

  interface Props {
    /** The column's name, for the accessible labels. */
    label: string;
    filter?: ColumnFilter;
    /** Inert, with the reason — while a windowed result is still filling. */
    disabled?: boolean;
    /**
     * The column's values, collected **when the picker opens** and not before: it
     * is a pass over every row, and it is wasted on every column the user never
     * asks about.
     */
    distinct: () => DistinctSet;
    /** `undefined` clears the column. */
    onChange: (next: ColumnFilter | undefined) => void;
  }

  let { label, filter, disabled = false, distinct, onChange }: Props = $props();

  const picked = $derived(filter?.kind === 'values' ? filter.picked : null);
  const needle = $derived(filter?.kind === 'text' ? filter.needle : '');

  /**
   * The value list as it was when the picker opened — deliberately not live.
   *
   * The grid narrows behind the open menu as values are ticked, and a list
   * recomputed from what survives would delete its own entries as you use them:
   * tick `ROMA`, and every other city vanishes from the list you are picking from.
   * The snapshot is what the column held at the moment you asked.
   */
  let snapshot = $state<DistinctSet | null>(null);

  /** The label a value prints under — see the two collisions it disambiguates. */
  function labelOf(v: { isNull: boolean; label: string }): string {
    if (v.isNull) return 'NULL';
    return v.label === '' ? '(empty)' : v.label;
  }

  const items = $derived<DropdownItem[]>(
    (snapshot?.values ?? []).slice(0, MAX_LISTED).map((v) => ({
      kind: 'item',
      id: v.key,
      label: labelOf(v),
      // A null and the string "NULL" are different values that print the same, and
      // so are an empty string and a row of nothing — the second line is the only
      // place that difference can be stated.
      subtitle: v.isNull ? 'no value' : v.label === '' ? 'empty string' : undefined,
      meta: v.count.toLocaleString(),
      active: picked?.has(v.key) ?? false,
      onclick: () => toggle(v.key),
    })),
  );

  const total = $derived(snapshot?.values.length ?? 0);
  const listed = $derived(Math.min(total, MAX_LISTED));

  /** What the box says when the column is filtered by a set rather than by text. */
  const summary = $derived.by(() => {
    if (!picked || picked.size === 0) return '';
    if (picked.size > 1) return `${picked.size} values`;
    const only = [...picked][0];
    const hit = snapshot?.values.find((v) => v.key === only);
    return hit ? labelOf(hit) : '1 value';
  });

  /**
   * The picker's own trigger button.
   *
   * The summary box opens the same menu this button opens, and clicking the real
   * trigger is how it does it — one owner of "open", rather than a second copy of
   * the dropdown's toggle that could drift out of step with it.
   */
  let triggerEl = $state<HTMLButtonElement | null>(null);

  function toggle(key: string) {
    const next = new Set(picked ?? []);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    onChange(next.size ? { kind: 'values', picked: next } : undefined);
  }

  function pickAll() {
    const keys = (snapshot?.values ?? []).slice(0, MAX_LISTED).map((v) => v.key);
    onChange(keys.length ? { kind: 'values', picked: new Set(keys) } : undefined);
  }
</script>

<div class="dgf">
  {#if picked}
    <!-- The box gives way to what is picked. It is a button rather than text: the
         way back to the list is the same control that shows what the list chose. -->
    <button
      type="button"
      class="dgf-summary"
      {disabled}
      use:tooltip={`${summary} picked in ${label} — click to change`}
      onclick={() => triggerEl?.click()}
    >
      <span class="dgf-summary-text">{summary}</span>
    </button>
  {:else}
    <input
      class="dgf-input"
      type="text"
      placeholder={disabled ? 'partial' : 'filter'}
      aria-label={`Filter ${label}`}
      {disabled}
      value={needle}
      oninput={(e) => {
        const v = e.currentTarget.value;
        onChange(v === '' ? undefined : { kind: 'text', needle: v });
      }}
    />
  {/if}

  <Dropdown
    {items}
    position="fixed"
    direction="down"
    width="240px"
    maxHeight={340}
    searchable
    searchPlaceholder="Find a value…"
    selectionMode="multiple"
    emptyMessage="No values"
    onopen={() => (snapshot = distinct())}
  >
    {#snippet trigger({ open, toggle: openMenu })}
      <button
        bind:this={triggerEl}
        type="button"
        class="dgf-open"
        class:dgf-on={!!picked}
        class:dgf-open-now={open}
        {disabled}
        aria-expanded={open}
        aria-haspopup="menu"
        aria-label={`Pick values for ${label}`}
        use:tooltip={disabled ? undefined : `The values in ${label}, with their row counts`}
        onclick={openMenu}
      >
        <ListFilter size={11} />
      </button>
    {/snippet}
    {#snippet footer({ close })}
      <div class="dgf-foot">
        <span class="dgf-count">
          {#if total > listed}
            first {listed} of {total.toLocaleString()}
          {:else}
            {total.toLocaleString()} value{total === 1 ? '' : 's'}
          {/if}
          {#if snapshot?.truncated}&nbsp;· capped{/if}
        </span>
        <span class="dgf-foot-actions">
          <button type="button" class="dgf-link" onclick={pickAll}>All</button>
          <button
            type="button"
            class="dgf-link"
            onclick={() => { onChange(undefined); close(); }}
          >Clear</button>
        </span>
      </div>
    {/snippet}
  </Dropdown>

  {#if picked}
    <button
      type="button"
      class="dgf-clear"
      use:tooltip={`Clear the filter on ${label}`}
      aria-label={`Clear the filter on ${label}`}
      onclick={() => onChange(undefined)}
    >
      <X size={10} />
    </button>
  {/if}
</div>

<style>
  .dgf {
    display: flex;
    align-items: center;
    gap: 2px;
    width: 100%;
    min-width: 0;
  }

  .dgf-input,
  .dgf-summary {
    flex: 1;
    min-width: 0;
    height: 20px;
    padding: 0 6px;
    background: var(--bg-input);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    outline: none;
  }
  .dgf-input:focus,
  .dgf-summary:focus-visible { border-color: var(--border-focus); }
  .dgf-input::placeholder { color: var(--text-disabled); font-style: italic; }
  .dgf-input:disabled,
  .dgf-summary:disabled { opacity: 0.5; cursor: default; }

  /* Picked values read as a chosen state rather than as typed text — same box,
     accent border, so a column filtered by a set is distinguishable at a glance
     from one filtered by a needle. */
  .dgf-summary {
    display: flex;
    align-items: center;
    text-align: left;
    cursor: pointer;
    border-color: var(--accent);
    color: var(--accent);
  }
  .dgf-summary-text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .dgf-open,
  .dgf-clear {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 20px;
    padding: 0;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-disabled);
    cursor: pointer;
    transition: color var(--transition-fast), background var(--transition-fast);
  }
  .dgf-open:hover:not(:disabled),
  .dgf-clear:hover { color: var(--text-primary); background: var(--bg-hover); }
  .dgf-open:disabled { opacity: 0.5; cursor: default; }
  .dgf-on,
  .dgf-open-now { color: var(--accent); }

  .dgf-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
  }
  .dgf-count { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .dgf-foot-actions { display: inline-flex; gap: 8px; flex: none; }
  .dgf-link {
    background: none;
    border: none;
    padding: 0;
    color: var(--accent);
    font-size: var(--font-size-2xs);
    cursor: pointer;
  }
  .dgf-link:hover { text-decoration: underline; }
</style>
