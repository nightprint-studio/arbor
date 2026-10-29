<script lang="ts">
  /**
   * OptionPicker — pick several from a long, grouped list: a filter field, the chosen ones as
   * removable chips, and the matches underneath with their descriptions.
   *
   * The shape of Spring's dependency chooser and Quarkus's extension list, and of any "tick a few
   * of these two hundred" question: the list is too long to scan, so typing narrows it, and what has
   * already been chosen must stay visible while you keep searching — hence the chips above the list
   * rather than checkmarks you scroll back to find.
   *
   * Keyboard: type to filter (fuzzy, across name, id and description); ↑/↓ move through the matches,
   * Enter ticks or unticks the highlighted one, Backspace in an empty field removes the last chip,
   * Esc clears the filter (and only then lets the surrounding dialog see it).
   */
  import { Search, X, Check } from 'lucide-svelte';
  import Spinner from './Spinner.svelte';
  import { filterOptions, groupOptions, toggled, type ListOption } from '$lib/utils/option-list';

  let {
    options,
    selected = $bindable<string[]>([]),
    placeholder = 'Search…',
    emptyMessage = 'Nothing matches.',
    loading = false,
    height = 280,
    ariaLabel = 'Options',
    onchange,
  }: {
    options: readonly ListOption[];
    selected?: string[];
    placeholder?: string;
    emptyMessage?: string;
    /** The options are still arriving. */
    loading?: boolean;
    /** Height of the list area in px. */
    height?: number;
    ariaLabel?: string;
    onchange?: (selected: string[]) => void;
  } = $props();

  /** More than this many rows in the DOM is a list nobody scrolls; they refine the filter instead. */
  const ROW_CAP = 250;

  let query = $state('');
  let active = $state(0);
  let input = $state<HTMLInputElement | undefined>();
  let listEl = $state<HTMLElement | undefined>();

  const uid = `op-${Math.random().toString(36).slice(2, 8)}`;

  const matches = $derived(filterOptions(options, query));
  const shown = $derived(matches.slice(0, ROW_CAP));
  const groups = $derived(groupOptions(shown));
  /** Rows in display order, for the arrow keys: grouping can reorder what filtering ranked. */
  const flat = $derived(groups.flatMap((g) => g.items));
  const byValue = $derived(new Map(options.map((o) => [o.value, o])));
  const chosen = $derived(new Set(selected));

  // A new filter starts from the best match.
  $effect(() => {
    void query;
    active = 0;
  });

  $effect(() => {
    const id = flat[active] ? `${uid}-${flat[active].value}` : null;
    if (id) listEl?.querySelector(`[id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: 'nearest' });
  });

  function commit(next: string[]) {
    selected = next;
    onchange?.(next);
  }

  function toggle(value: string) {
    commit(toggled(selected, value));
  }

  function onKey(e: KeyboardEvent) {
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        active = Math.min(flat.length - 1, active + 1);
        break;
      case 'ArrowUp':
        e.preventDefault();
        active = Math.max(0, active - 1);
        break;
      case 'PageDown':
        e.preventDefault();
        active = Math.min(flat.length - 1, active + 8);
        break;
      case 'PageUp':
        e.preventDefault();
        active = Math.max(0, active - 8);
        break;
      case 'Enter':
        // Ctrl+Enter belongs to the dialog's Create; a bare Enter ticks the row.
        if (e.ctrlKey || e.metaKey) return;
        e.preventDefault();
        if (flat[active]) toggle(flat[active].value);
        break;
      case 'Backspace':
        if (!query && selected.length) commit(selected.slice(0, -1));
        break;
      case 'Escape':
        if (query) {
          e.preventDefault();
          e.stopPropagation();
          query = '';
        }
        break;
    }
  }

  export function focus() {
    input?.focus();
  }
</script>

<div class="picker">
  {#if selected.length}
    <div class="chips" role="list" aria-label="Selected">
      {#each selected as id (id)}
        <span class="chip" role="listitem">
          <span class="chip-label">{byValue.get(id)?.label ?? id}</span>
          <button type="button" class="chip-x" tabindex="-1" aria-label={`Remove ${byValue.get(id)?.label ?? id}`} onclick={() => toggle(id)}>
            <X size={11} />
          </button>
        </span>
      {/each}
    </div>
  {/if}

  <label class="search">
    <Search size={13} />
    <input
      bind:this={input}
      bind:value={query}
      class="search-input"
      type="text"
      role="combobox"
      aria-label={ariaLabel}
      aria-expanded="true"
      aria-controls={`${uid}-list`}
      aria-activedescendant={flat[active] ? `${uid}-${flat[active].value}` : undefined}
      aria-autocomplete="list"
      autocomplete="off"
      spellcheck="false"
      {placeholder}
      onkeydown={onKey}
    />
    <span class="count">{selected.length ? `${selected.length} selected` : `${options.length} available`}</span>
  </label>

  <div class="list" id={`${uid}-list`} role="listbox" aria-multiselectable="true" aria-label={ariaLabel} bind:this={listEl} style:height={`${height}px`}>
    {#if loading}
      <div class="note"><Spinner size="sm" /> Loading…</div>
    {:else if !flat.length}
      <div class="note">{emptyMessage}</div>
    {:else}
      {#each groups as g (g.group)}
        {#if g.group}<div class="group" role="presentation">{g.group}</div>{/if}
        {#each g.items as opt (opt.value)}
          {@const on = chosen.has(opt.value)}
          <div
            id={`${uid}-${opt.value}`}
            class="row"
            class:on
            class:active={flat[active]?.value === opt.value}
            role="option"
            aria-selected={on}
            tabindex="-1"
            onclick={() => { toggle(opt.value); input?.focus(); }}
            onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggle(opt.value); } }}
          >
            <span class="box" aria-hidden="true">{#if on}<Check size={11} strokeWidth={3} />{/if}</span>
            <span class="text">
              <span class="label">{opt.label}</span>
              {#if opt.description}<span class="desc">{opt.description}</span>{/if}
            </span>
          </div>
        {/each}
      {/each}
      {#if matches.length > ROW_CAP}
        <div class="note">{matches.length - ROW_CAP} more — type to narrow the list.</div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .picker { display: flex; flex-direction: column; gap: 7px; min-height: 0; }

  .chips { display: flex; flex-wrap: wrap; gap: 5px; max-height: 76px; overflow-y: auto; }
  .chip {
    display: inline-flex; align-items: center; gap: 3px;
    padding: 2px 3px 2px 8px;
    font-size: var(--font-size-xs);
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, var(--bg-base));
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
    border-radius: 999px;
  }
  .chip-label { line-height: 1.5; }
  .chip-x {
    display: inline-flex; align-items: center; justify-content: center;
    width: 16px; height: 16px; padding: 0;
    background: transparent; border: none; border-radius: 50%;
    color: inherit; cursor: pointer; opacity: 0.7;
  }
  .chip-x:hover { opacity: 1; background: color-mix(in srgb, var(--accent) 22%, transparent); }

  .search {
    display: flex; align-items: center; gap: 7px;
    padding: 0 9px;
    color: var(--text-muted);
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    transition: border-color var(--transition-fast);
  }
  .search:focus-within { border-color: var(--border-focus); }
  .search-input {
    flex: 1; min-width: 0; padding: 6px 0;
    background: transparent; border: none; outline: none;
    color: var(--text-primary);
    font-family: var(--font-ui-sans); font-size: var(--font-size-sm);
  }
  .search-input::placeholder { color: var(--text-disabled); }
  .count { font-size: var(--font-size-xs); color: var(--text-disabled); white-space: nowrap; }

  .list {
    overflow-y: auto;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
  }
  .group {
    position: sticky; top: 0; z-index: 1;
    padding: 5px 10px 3px;
    font-size: 10px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase;
    color: var(--text-muted);
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
  }
  .row {
    display: flex; align-items: flex-start; gap: 9px;
    padding: 6px 10px; cursor: pointer;
  }
  .row:hover { background: var(--bg-hover); }
  .row.active { background: var(--bg-hover); box-shadow: inset 2px 0 0 var(--accent); }
  .row.on .label { color: var(--accent); }
  .box {
    flex-shrink: 0; width: 15px; height: 15px; margin-top: 1px;
    display: inline-flex; align-items: center; justify-content: center;
    border: 1.5px solid var(--border); border-radius: 4px;
    background: var(--bg-input, var(--bg-base)); color: var(--text-on-accent);
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .row.on .box { background: var(--accent); border-color: var(--accent); }
  .text { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .label { font-size: var(--font-size-sm); color: var(--text-primary); line-height: 1.4; }
  .desc { font-size: var(--font-size-xs); color: var(--text-muted); line-height: 1.4; }
  .note {
    display: flex; align-items: center; gap: 8px;
    padding: 12px 10px; font-size: var(--font-size-xs); color: var(--text-disabled);
  }
</style>
