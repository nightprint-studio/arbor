<script lang="ts">
  /**
   * Generic filter bar — search input + N multi/single-select chip dropdowns.
   *
   * Owns its UI state (search query + per-filter selection map) and notifies
   * the parent on every change. Visuals reuse `<SearchBar>` and
   * `<FilterButton>` so the look matches the rest of the app (sidebar headers,
   * MR/PR list, …) — no ad-hoc input styling.
   *
   * Domain coupling: zero. The first plugin consumer is the group-level
   * security dashboard (search across repos + multi-select severities/repos);
   * any plugin can render `kind = 'filter_bar'` form-nodes against the same
   * widget.
   */
  import SearchBar    from '$lib/components/shared/ui/SearchBar.svelte';
  import FilterButton from '$lib/components/shared/ui/FilterButton.svelte';
  import { Check }    from 'lucide-svelte';
  import { PLUGIN_ICONS } from '$lib/utils/plugin-icons';
  import { tooltip } from '$lib/actions/tooltip';

  export interface FilterBarOption {
    value: string;
    label: string;
    /** Optional accent dot colour shown beside the option label. */
    color?: string;
  }

  export interface FilterBarFilter {
    /** Stable id; surfaced as the key in the emitted `filters` map. */
    id:       string;
    label:    string;
    /** Lucide icon name (curated subset — see PLUGIN_ICONS). */
    icon?:    string;
    options:  FilterBarOption[];
    /** `'multi'` (default) accepts any subset; `'single'` clears the others on select. */
    mode?:    'single' | 'multi';
    /** When true the dropdown gets an inline filter input. Default false. */
    searchable?: boolean;
    /** Wider dropdown panel. */
    wide?:    boolean;
  }

  export interface FilterBarValue {
    search:  string;
    filters: Record<string, string[]>;
  }

  interface Props {
    /** Bound bar value. */
    value:           FilterBarValue;
    filters?:        FilterBarFilter[];
    /** Search input config. Pass `null` / `undefined` to omit the search input. */
    search?:         { placeholder?: string; show_regex?: boolean } | null;
    /** Outer padding (CSS). Default '8px'. */
    padding?:        string;
    /** Notified after every value mutation — covers both search input and chip toggles. */
    onChange?:       (v: FilterBarValue) => void;
  }

  let {
    value     = $bindable<FilterBarValue>({ search: '', filters: {} }),
    filters   = [],
    search    = { placeholder: 'Search…' },
    padding   = '8px',
    onChange,
  }: Props = $props();

  function selectionFor(id: string): string[] {
    return value.filters?.[id] ?? [];
  }

  function emit() { onChange?.(value); }

  function setSearch(q: string) {
    value = { ...value, search: q };
    emit();
  }

  function toggleOption(f: FilterBarFilter, opt: FilterBarOption) {
    const cur = selectionFor(f.id);
    const has = cur.includes(opt.value);
    let next: string[];

    if (f.mode === 'single') {
      next = has ? [] : [opt.value];
    } else {
      next = has ? cur.filter(v => v !== opt.value) : [...cur, opt.value];
    }

    value = {
      ...value,
      filters: { ...value.filters, [f.id]: next },
    };
    emit();
  }

  function clearFilter(f: FilterBarFilter, e: Event) {
    e.stopPropagation();
    const next = { ...value.filters };
    delete next[f.id];
    value = { ...value, filters: next };
    emit();
  }

  function clearAll() {
    value = { search: '', filters: {} };
    emit();
  }

  const hasAny = $derived(
    !!value.search ||
    Object.values(value.filters ?? {}).some(arr => arr.length > 0)
  );
</script>

<div class="fb" style:padding>
  {#if search}
    <div class="fb-search">
      <SearchBar
        query={value.search}
        showRegex={search.show_regex ?? false}
        showCounter={false}
        placeholder={search.placeholder ?? 'Search…'}
        oninput={setSearch}
        onClear={() => setSearch('')}
      />
    </div>
  {/if}

  {#each filters as f (f.id)}
    {@const Icon = f.icon ? PLUGIN_ICONS[f.icon] : null}
    {@const sel  = selectionFor(f.id)}
    <FilterButton
      label={f.label}
      icon={Icon}
      count={sel.length}
      wide={f.wide}
      searchable={f.searchable}
      searchPlaceholder={`Filter ${f.label.toLowerCase()}…`}
    >
      {#snippet children({ filter, close })}
        {@const matches = (f.options ?? []).filter(o =>
          !filter || o.label.toLowerCase().includes(filter.toLowerCase())
        )}
        {#if sel.length > 0}
          <button
            type="button"
            class="fb-clear-row"
            onclick={(e) => { clearFilter(f, e); close(); }}
          >Clear ({sel.length})</button>
        {/if}
        {#if matches.length === 0}
          <div class="fb-empty">No options</div>
        {:else}
          {#each matches as opt (opt.value)}
            {@const checked = sel.includes(opt.value)}
            <button
              type="button"
              class="fb-row"
              class:checked
              onclick={() => {
                toggleOption(f, opt);
                if (f.mode === 'single') close();
              }}
            >
              <span class="fb-check">
                {#if checked}<Check size={11} />{/if}
              </span>
              {#if opt.color}
                <span class="fb-dot" style:background={opt.color}></span>
              {/if}
              <span class="fb-row-label">{opt.label}</span>
            </button>
          {/each}
        {/if}
      {/snippet}
    </FilterButton>
  {/each}

  {#if hasAny}
    <button type="button" class="fb-reset" onclick={clearAll} use:tooltip={'Reset all filters'}>
      Reset
    </button>
  {/if}
</div>

<style>
  .fb {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .fb-search {
    flex: 1;
    min-width: 180px;
    max-width: 360px;
  }

  .fb-row {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 5px 7px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    color: var(--text-secondary);
    cursor: pointer;
    text-align: left;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .fb-row:hover { background: var(--bg-hover); color: var(--text-primary); }
  .fb-row.checked { color: var(--text-primary); }

  .fb-check {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 12px;
    height: 12px;
    flex-shrink: 0;
    color: var(--accent);
  }

  .fb-dot {
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .fb-row-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .fb-clear-row {
    width: 100%;
    padding: 4px 7px;
    margin-bottom: 2px;
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--border-subtle);
    text-align: left;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: 10px;
    cursor: pointer;
    border-radius: 0;
    transition: color var(--transition-fast);
  }
  .fb-clear-row:hover { color: var(--accent); }

  .fb-empty {
    padding: 8px;
    text-align: center;
    color: var(--text-muted);
    font-style: italic;
    font-size: 11px;
  }

  .fb-reset {
    padding: 3px 8px;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: 99px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: 10px;
    cursor: pointer;
    transition: all var(--transition-fast);
  }
  .fb-reset:hover {
    color: var(--accent);
    border-color: var(--accent);
    background: var(--accent-subtle);
  }
</style>
