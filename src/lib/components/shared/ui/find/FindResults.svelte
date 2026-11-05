<script lang="ts">
  /**
   * Find-in-many: a query field over results grouped under headers, beside a preview of the
   * selected hit. Product-agnostic — Bennu's "Find in project" and Garrulus's "Search the
   * vault" are both this widget with different data.
   *
   * ## What it owns, and what it does not
   *
   * It owns the **presentation and the keyboard**: the field row, the windowed grouped list,
   * the preview column, the footer, and ↑/↓/PageUp/PageDown/Enter. It does **not** search.
   * The host runs the query (debounced or on Enter — its call), streams or batches results
   * into `groups`, and loads whatever the `preview` snippet shows. Data comes in as props,
   * actions go out as callbacks, extras ride in as snippets:
   *
   *   * `controls` — the header row's pickers (source, module, mask…);
   *   * `toggles` — the pressed-key toggles at the right of the field (match case, regex…);
   *   * `queryField` — replaces the plain input (Garrulus puts its filter-chips box here);
   *   * `groupLead` — the icon of a group header;
   *   * `preview` / `footer` — the right column and the key hints.
   *
   * ## Why the list is flattened
   *
   * The list is **windowed** (`VirtualList`): five thousand hits are ~40 rows of DOM. A window
   * is `scrollTop / rowHeight`, which needs one array of one row height, so groups are
   * flattened here — a group header is just another row kind laid out to the same height.
   *
   * ## Selection
   *
   * `selected` is a **flat hit index** (headers do not count), bindable, so the host can read
   * which hit the preview should show and reset it to 0 on a new search. The widget keeps it
   * in range as results change. Nothing is opened until Enter or a click (`onOpen`), which is
   * what makes browsing a hundred hits cheap.
   *
   * Keyboard events already handled by a child (`defaultPrevented`) are left alone, so a
   * custom `queryField` can own Enter itself (run the search) and still hand it on (open).
   */
  import type { Snippet } from 'svelte';
  import { Search } from 'lucide-svelte';
  import EmptyState from '../EmptyState.svelte';
  import Spinner from '../Spinner.svelte';
  import VirtualList from '../VirtualList.svelte';
  import FindGroupRow from './FindGroupRow.svelte';
  import FindHitRow from './FindHitRow.svelte';
  import type { FindGroup, FindHitItem, FindNotice, FindSegment } from './find-types';

  interface Props {
    /** The query text (plain input only; a custom `queryField` owns its own state). */
    query?: string;
    placeholder?: string;
    ariaLabel?: string;
    /** A search is in flight: the spinner beside the field, and the loading empty state. */
    loading?: boolean;
    groups: FindGroup[];
    /** Flat index of the highlighted hit across all groups. */
    selected?: number;
    /** Blocks the list entirely ("Open a project", "Type at least 2 characters", an error). */
    notice?: FindNotice | null;
    /** Shown instead of the list when there are no hits and nothing is loading. */
    empty?: FindNotice | null;
    /** "12 matches in 3 files" — the sentence in the header row. */
    summary?: string;
    /** Dimmed tail of the summary ("of 40"). */
    summaryNote?: string;
    /** The results were cut short by the backend. */
    capped?: boolean;
    controls?: Snippet;
    toggles?: Snippet;
    queryField?: Snippet;
    groupLead?: Snippet<[FindGroup]>;
    /** The matched / unmatched runs of a hit's text. Default: the text, unmarked. */
    segmentsOf?: (hit: FindHitItem) => FindSegment[];
    /** Enter on, or a click of, a hit — by flat index. */
    onOpen?: (index: number) => void;
    preview: Snippet;
    footer?: Snippet;
    /** Both row kinds are laid out to this height. */
    rowHeight?: number;
  }

  let {
    query = $bindable(''),
    placeholder = 'Find…',
    ariaLabel = 'Find',
    loading = false,
    groups,
    selected = $bindable(0),
    notice = null,
    empty = null,
    summary = '',
    summaryNote = '',
    capped = false,
    controls,
    toggles,
    queryField,
    groupLead,
    segmentsOf,
    onOpen,
    preview,
    footer,
    rowHeight = 22,
  }: Props = $props();

  type Row =
    | { kind: 'group'; key: string; group: FindGroup }
    | { kind: 'hit'; key: string; hit: FindHitItem; idx: number; external: boolean };

  const rows = $derived.by<Row[]>(() => {
    const out: Row[] = [];
    let idx = 0;
    for (const group of groups) {
      out.push({ kind: 'group', key: `g:${group.key}`, group });
      for (const hit of group.hits) {
        out.push({ kind: 'hit', key: `h:${hit.key}`, hit, idx, external: !!group.external });
        idx += 1;
      }
    }
    return out;
  });

  const hitCount = $derived(groups.reduce((n, g) => n + g.hits.length, 0));

  /** Where the selected hit sits among the ROWS — headers shift it, and the window scrolls by
   *  row index, not by hit index. */
  const selRow = $derived(rows.findIndex((r) => r.kind === 'hit' && r.idx === selected));

  // Keep the selection in range as results change.
  $effect(() => { if (selected >= hitCount) selected = Math.max(0, hitCount - 1); });

  // The field keeps the focus throughout: refining a query after looking at the results is the
  // normal case, so the arrows drive the list without leaving the input. `data-modal-autofocus`
  // is what makes it focused on opening — a Modal's own focus pass runs after child effects and
  // would otherwise land on the first control of the header row.
  let field = $state<HTMLInputElement | null>(null);
  $effect(() => { field?.focus(); });

  /** Put the caret back in the query field. */
  export function focus(): void { field?.focus(); }

  function plain(hit: FindHitItem): FindSegment[] {
    return [{ text: hit.text, hit: false }];
  }

  function move(delta: number) {
    if (!hitCount) return;
    // No scrolling here: the list follows `selRow` itself — an off-screen row has no element.
    selected = Math.min(Math.max(selected + delta, 0), hitCount - 1);
  }

  function onKey(e: KeyboardEvent) {
    if (e.defaultPrevented) return;
    switch (e.key) {
      case 'ArrowDown': e.preventDefault(); move(1); break;
      case 'ArrowUp': e.preventDefault(); move(-1); break;
      case 'PageDown': e.preventDefault(); move(8); break;
      case 'PageUp': e.preventDefault(); move(-8); break;
      case 'Enter':
        if (hitCount) { e.preventDefault(); onOpen?.(selected); }
        break;
      default: break;
    }
  }
</script>

<div class="fr" onkeydown={onKey} role="presentation">
  {#if summary || capped || controls}
    <div class="fr-bar">
      <span class="fr-spacer"></span>
      {#if capped}<span class="fr-cap">capped</span>{/if}
      {#if summary}
        <span class="fr-summary">
          {summary}
          {#if summaryNote}<span class="fr-summary-note">{summaryNote}</span>{/if}
        </span>
      {/if}
      {#if controls}{@render controls()}{/if}
    </div>
  {/if}

  <div class="fr-search">
    {#if queryField}
      <div class="fr-field-slot">{@render queryField()}</div>
    {:else}
      <Search size={15} />
      <input
        bind:this={field}
        bind:value={query}
        class="fr-field"
        type="text"
        spellcheck="false"
        autocomplete="off"
        {placeholder}
        aria-label={ariaLabel}
        data-modal-autofocus
      />
    {/if}
    {#if loading}<Spinner size={13} />{/if}
    {#if toggles}<div class="fr-toggles">{@render toggles()}</div>{/if}
  </div>

  {#if notice}
    <EmptyState message={notice.message} description={notice.description} />
  {:else if hitCount === 0}
    {#if loading}
      <div class="fr-loading"><Spinner size="sm" label="Searching…" /></div>
    {:else if empty}
      <EmptyState message={empty.message} description={empty.description} />
    {/if}
  {:else}
    <div class="fr-split">
      <VirtualList
        items={rows}
        {rowHeight}
        getKey={(r) => r.key}
        scrollTo={selRow}
        ariaLabel="Matches"
      >
        {#snippet row({ item }: { item: Row })}
          {#if item.kind === 'group'}
            <FindGroupRow group={item.group} lead={groupLead} />
          {:else}
            <FindHitRow
              hit={item.hit}
              segments={(segmentsOf ?? plain)(item.hit)}
              selected={item.idx === selected}
              external={item.external}
              onopen={() => onOpen?.(item.idx)}
              onhover={() => (selected = item.idx)}
            />
          {/if}
        {/snippet}
      </VirtualList>

      <div class="fr-preview">{@render preview()}</div>
    </div>

    {#if footer}<div class="fr-foot">{@render footer()}</div>{/if}
  {/if}
</div>

<style>
  .fr { display: flex; flex-direction: column; height: 100%; min-height: 0; }

  /* The header row: what decides WHAT IS SEARCHED (the host's controls), and the sentence that
     says what they produced. It is the topmost row — modals using this have no title bar. */
  .fr-bar {
    display: flex; align-items: center; gap: 8px;
    padding: 7px 12px; flex-shrink: 0;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-2xs); color: var(--text-muted);
  }
  .fr-spacer { flex: 1; }
  /* The summary must not wrap or be squeezed — it is the answer to everything beside it. */
  .fr-summary { white-space: nowrap; flex-shrink: 0; }
  .fr-summary-note { color: var(--text-disabled); margin-left: 4px; }
  .fr-cap { color: var(--warning); }

  .fr-search {
    display: flex; align-items: center; gap: 8px;
    padding: 11px 14px; flex-shrink: 0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .fr-search > :global(svg) { color: var(--text-disabled); flex-shrink: 0; }
  .fr-field {
    flex: 1; min-width: 0;
    background: none; border: none; outline: none;
    color: var(--text-primary); font-size: var(--font-size-lg);
  }
  .fr-field::placeholder { color: var(--text-disabled); }
  .fr-field-slot { flex: 1; min-width: 0; }
  .fr-toggles { display: flex; gap: 4px; flex-shrink: 0; }

  .fr-loading { display: flex; align-items: center; justify-content: center; padding: 24px; }

  /* Results left, the selected hit in context right — the split is the point. */
  .fr-split { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
  .fr-preview { min-height: 0; min-width: 0; border-left: 1px solid var(--border-subtle); }

  .fr-foot {
    display: flex; align-items: center; gap: 6px;
    padding: 7px 12px; flex-shrink: 0;
    border-top: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
    font-size: var(--font-size-2xs); color: var(--text-disabled);
  }
</style>
