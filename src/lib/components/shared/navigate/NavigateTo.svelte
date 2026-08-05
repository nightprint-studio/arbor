<script lang="ts">
  /**
   * Navigate-to — the "search everywhere" overlay, app-agnostic.
   *
   * Cross-product chrome, not a Picus component: Bennu wants exactly this box over
   * classes, files and symbols, and the interesting parts — the scoring, the
   * directives, the keyboard, the two-line row with the matched characters lit —
   * are the parts neither product should be writing twice.
   *
   * ## What a host supplies
   *
   * A list of {@link NavigateCategory}: an id, a label, and a function returning
   * items. Everything after that — filtering, scoring, sorting, grouping, keyboard
   * navigation, the empty states — is here. A host that gains a new searchable
   * thing adds a category; it does not touch this file.
   *
   * Items are pulled **once per opening**, not per keystroke: a repository's file
   * list is already in memory and re-deriving it on every character typed is the
   * cheapest possible way to make a search box feel slow. A category whose items
   * arrive over IPC returns a promise and the tab shows as loading.
   *
   * ## Why "All" is a real tab and not a union
   *
   * It searches every category and keeps each one's best few, grouped under its
   * label. A flat merged list is what the naive version produces, and on a
   * repository where files outnumber everything else it means the answer you
   * wanted is on row forty. Reserving room per category is what makes one box able
   * to answer several questions.
   *
   * ## Keyboard
   *
   * ↑ ↓ move, Enter opens, Esc closes, Tab / Shift+Tab cycle the categories. The
   * field keeps focus throughout — arrows never move it into the list, because
   * refining a query after looking at the results is the normal case, not the
   * exception.
   */
  import type { IconComponent } from '$lib/types/icon';
  import { Search, CornerDownLeft } from 'lucide-svelte';
  import Modal from '../Modal.svelte';
  import Tabs, { type TabItem } from '../ui/Tabs.svelte';
  import Spinner from '../ui/Spinner.svelte';
  import Kbd from '../internal/Kbd.svelte';
  import {
    fuzzyMatchPrepared,
    prepareCandidate,
    segments,
    type MatchRange,
    type PreparedCandidate,
  } from '$lib/utils/fuzzy';
  import {
    compareBy,
    parseQuery,
    passesFilters,
    QUERY_HELP,
    type ParsedQuery,
  } from './query';

  /** One thing that can be navigated to. */
  export interface NavigateItem {
    /** Unique within its category — used as the list key. */
    id: string;
    /** Matched and shown prominently: a file name, a class, an object. */
    name: string;
    /**
     * Matched and shown small: the path, the package, the owning table. Also what
     * `in:` and `ext:` filter against, so it should be the full path where there
     * is one.
     */
    detail?: string;
    /** Epoch milliseconds, for `sort:new`. Omit when the host has no timestamp. */
    modified?: number;
    /** Leading icon. Any Svelte component taking a `size` prop — see
     *  {@link IconComponent} for why lucide needs the legacy alias. */
    icon?: IconComponent;
    /**
     * Extra props for {@link icon}, spread after `size`. What lets a host use its own
     * icon components rather than only lucide's: a Java kind mark needs `kind`, an
     * iconify glyph needs `icon`. Without it every host is limited to the icons whose
     * entire identity fits in the component reference — which is why the classes here
     * were generic boxes while the tree three feet away drew the real thing.
     */
    iconProps?: Record<string, unknown>;
    /** A short word on the right — the object kind, the engine, the role. */
    tag?: string;
    onOpen: () => void;
  }

  export interface NavigateCategory {
    id: string;
    label: string;
    /** Called once per opening. May be async. */
    items: () => NavigateItem[] | Promise<NavigateItem[]>;
    /** Shown when this category has nothing at all, before any filtering. */
    emptyMessage?: string;
  }

  interface Props {
    categories: NavigateCategory[];
    /** Category to open on. Defaults to `all`. */
    initialCategory?: string;
    /** Seed text — e.g. the editor's selection. */
    initialQuery?: string;
    title?: string;
    onClose: () => void;
  }

  let {
    categories,
    initialCategory = 'all',
    initialQuery = '',
    title = 'Go to',
    onClose,
  }: Props = $props();

  /** How many of each category's hits "All" keeps. */
  const PER_CATEGORY_IN_ALL = 5;
  /** Cap on one category's own list — beyond this nobody is reading, they retype. */
  const MAX_ROWS = 200;

  // svelte-ignore state_referenced_locally
  let query = $state(initialQuery);
  // svelte-ignore state_referenced_locally
  let active = $state(initialCategory);
  let cursor = $state(0);
  let loading = $state(true);
  let field = $state<HTMLInputElement | undefined>();

  /** Items per category id, filled once when the overlay opens. */
  let loaded = $state<Record<string, NavigateItem[]>>({});

  const tabs = $derived<TabItem[]>([
    { id: 'all', label: 'All' },
    ...categories.map((c) => ({ id: c.id, label: c.label })),
  ]);

  // Pulled once, on mount. `$effect` rather than `onMount` so a host that swaps
  // the category list — a repository closing, a second connection opening —
  // re-pulls rather than showing the previous repository's files.
  $effect(() => {
    const list = categories;
    let live = true;
    loading = true;
    void (async () => {
      const next: Record<string, NavigateItem[]> = {};
      await Promise.all(
        list.map(async (category) => {
          try {
            next[category.id] = await category.items();
          } catch {
            // A category that cannot answer contributes nothing rather than
            // taking the overlay down with it — the others are still useful.
            next[category.id] = [];
          }
        }),
      );
      if (!live) return;
      loaded = next;
      loading = false;
    })();
    return () => { live = false; };
  });

  $effect(() => { field?.focus(); });

  const parsed = $derived<ParsedQuery>(parseQuery(query));

  interface Scored extends NavigateItem {
    category: string;
    categoryLabel: string;
    score: number;
    path: string;
    nameRanges: MatchRange[];
    detailRanges: MatchRange[];
  }

  /** An item with its lowercased match forms already built. */
  interface Prepared {
    item: NavigateItem;
    candidate: PreparedCandidate;
    path: string;
  }

  /**
   * Lowercasing every candidate is the one real cost in the loop, and it depends on the
   * ITEMS, not on the query — so it happens once per opening rather than once per keystroke.
   * On a legacy project that is tens of thousands of strings not being re-allocated for each
   * character typed.
   */
  const prepared = $derived.by<Record<string, Prepared[]>>(() => {
    const out: Record<string, Prepared[]> = {};
    for (const category of categories) {
      out[category.id] = (loaded[category.id] ?? []).map((item) => {
        const detail = item.detail ?? '';
        return {
          item,
          candidate: prepareCandidate(item.name, detail),
          path: detail ? `${detail}/${item.name}` : item.name,
        };
      });
    }
    return out;
  });

  /** Score and filter one category's items against the current query. */
  function rank(category: NavigateCategory): Scored[] {
    const out: Scored[] = [];
    for (const p of prepared[category.id] ?? []) {
      if (!passesFilters(p.path, parsed)) continue;
      const hit = fuzzyMatchPrepared(p.candidate, parsed.text);
      if (!hit) continue;
      out.push({
        ...p.item,
        category: category.id,
        categoryLabel: category.label,
        score: hit.score,
        path: p.path,
        nameRanges: hit.nameRanges,
        detailRanges: hit.detailRanges,
      });
    }
    return out.sort(compareBy(parsed.sort));
  }

  /**
   * The rows on screen, in display order, with a heading before each category's
   * run when more than one is showing.
   */
  const groups = $derived.by(() => {
    if (active === 'all') {
      return categories
        .map((c) => ({ label: c.label, rows: rank(c).slice(0, PER_CATEGORY_IN_ALL) }))
        .filter((g) => g.rows.length);
    }
    const category = categories.find((c) => c.id === active);
    if (!category) return [];
    return [{ label: category.label, rows: rank(category).slice(0, MAX_ROWS) }];
  });

  const rows = $derived(groups.flatMap((g) => g.rows));

  // The cursor is an index into a list that changes on every keystroke, so it is
  // clamped where it is READ rather than reset on change — resetting from an
  // effect would fight the user's own arrow presses within the same tick.
  const selected = $derived(rows.length ? Math.min(cursor, rows.length - 1) : 0);

  function move(delta: number) {
    if (!rows.length) return;
    cursor = (selected + delta + rows.length) % rows.length;
  }

  function open(row: Scored | undefined) {
    if (!row) return;
    onClose();
    // After the overlay is gone, so whatever it opens gets the focus.
    queueMicrotask(() => row.onOpen());
  }

  function cycleTab(delta: number) {
    const i = tabs.findIndex((t) => t.id === active);
    active = tabs[(i + delta + tabs.length) % tabs.length].id;
    cursor = 0;
  }

  function onKeydown(e: KeyboardEvent) {
    switch (e.key) {
      case 'ArrowDown': move(1); e.preventDefault(); break;
      case 'ArrowUp': move(-1); e.preventDefault(); break;
      case 'PageDown': move(8); e.preventDefault(); break;
      case 'PageUp': move(-8); e.preventDefault(); break;
      case 'Enter': open(rows[selected]); e.preventDefault(); break;
      case 'Tab': cycleTab(e.shiftKey ? -1 : 1); e.preventDefault(); break;
      default: break;
    }
  }

  /** Keep the highlighted row on screen as the arrows walk past its edge. */
  let list = $state<HTMLElement | undefined>();
  $effect(() => {
    const i = selected;
    const el = list?.querySelector<HTMLElement>(`[data-row="${i}"]`);
    el?.scrollIntoView({ block: 'nearest' });
  });
</script>

<Modal {onClose} width="700px" height="520px" padBody={false} ariaLabel={title}>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="nv" role="group" onkeydown={onKeydown}>
    <div class="nv-search">
      <Search size={14} />
      <input
        bind:this={field}
        bind:value={query}
        class="nv-field"
        type="text"
        spellcheck="false"
        autocomplete="off"
        placeholder="Type to search — try sort:new or ext:sql"
        aria-label={title}
        role="combobox"
        aria-expanded={rows.length > 0}
        aria-controls="nv-results"
      />
      {#if loading}<Spinner size={13} />{/if}
    </div>

    <div class="nv-tabs">
      <Tabs
        items={tabs}
        value={active}
        variant="pill"
        size="sm"
        ariaLabel="What to search"
        onSelect={(id) => { active = id; cursor = 0; }}
      />
      <span class="nv-spacer"></span>
      {#if parsed.directives.length}
        <!-- Shown back, because a directive that silently did nothing is worse
             than one that was never typed. -->
        {#each parsed.directives as d (d.key + d.value)}
          <span class="nv-directive">{d.key}:{d.value}</span>
        {/each}
      {/if}
      <span class="nv-count">{rows.length}</span>
    </div>

    <div class="nv-list" id="nv-results" role="listbox" aria-label="Results" bind:this={list}>
      {#if loading}
        <p class="nv-note">Reading…</p>
      {:else if !rows.length}
        <p class="nv-note">
          {#if query.trim()}
            Nothing matches <b>{query.trim()}</b>.
          {:else}
            There is nothing here to search yet.
          {/if}
        </p>
        {#if !query.trim()}
          <!-- The syntax is only discoverable if it is somewhere, and the empty
               state is the one moment nobody is busy reading results. -->
          <dl class="nv-help">
            {#each QUERY_HELP as help (help.syntax)}
              <dt><code>{help.syntax}</code></dt>
              <dd>{help.means}</dd>
            {/each}
          </dl>
        {/if}
      {:else}
        {#each groups as group (group.label)}
          {#if groups.length > 1}
            <p class="nv-group">{group.label}</p>
          {/if}
          {#each group.rows as row (row.category + row.id)}
            {@const index = rows.indexOf(row)}
            <button
              type="button"
              class="nv-row"
              class:nv-on={index === selected}
              data-row={index}
              role="option"
              aria-selected={index === selected}
              onmousemove={() => (cursor = index)}
              onclick={() => open(row)}
            >
              {#if row.icon}
                <span class="nv-icon"><row.icon size={13} {...row.iconProps ?? {}} /></span>
              {:else}
                <span class="nv-icon nv-icon-gap"></span>
              {/if}
              <span class="nv-name">
                {#each segments(row.name, row.nameRanges) as part, i (i)}
                  {#if part.hit}<b>{part.text}</b>{:else}{part.text}{/if}
                {/each}
              </span>
              {#if row.detail}
                <span class="nv-detail">
                  {#each segments(row.detail, row.detailRanges) as part, i (i)}
                    {#if part.hit}<b>{part.text}</b>{:else}{part.text}{/if}
                  {/each}
                </span>
              {/if}
              <span class="nv-spacer"></span>
              {#if row.tag}<span class="nv-tag">{row.tag}</span>{/if}
            </button>
          {/each}
        {/each}
      {/if}
    </div>

    <div class="nv-foot">
      <Kbd keys={["↑"]} size="sm" /><Kbd keys={["↓"]} size="sm" /><span>move</span>
      <Kbd keys={["Tab"]} size="sm" /><span>category</span>
      <span class="nv-foot-open"><CornerDownLeft size={11} /> open</span>
      <span class="nv-spacer"></span>
      <Kbd keys={["Esc"]} size="sm" /><span>close</span>
    </div>
  </div>
</Modal>

<style>
  .nv { display: flex; flex-direction: column; height: 100%; min-height: 0; }

  .nv-search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 11px 14px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .nv-search :global(svg) { color: var(--text-disabled); flex-shrink: 0; }
  .nv-field {
    flex: 1;
    min-width: 0;
    background: none;
    border: none;
    outline: none;
    color: var(--text-primary);
    font-size: var(--font-size-lg);
  }
  .nv-field::placeholder { color: var(--text-disabled); }

  .nv-tabs {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 12px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
    flex-shrink: 0;
  }
  .nv-spacer { flex: 1; }
  .nv-directive {
    padding: 1px 6px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    background: var(--bg-base);
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-secondary);
    white-space: nowrap;
  }
  .nv-count {
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    font-variant-numeric: tabular-nums;
  }

  .nv-list { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 0; }

  .nv-group {
    padding: 8px 14px 3px;
    font-size: var(--font-size-2xs);
    font-weight: 600;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .nv-row {
    display: flex;
    align-items: baseline;
    gap: 8px;
    width: 100%;
    padding: 5px 14px;
    background: none;
    border: none;
    text-align: left;
    color: var(--text-secondary);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }
  /* Hover and the cursor are drawn separately even though moving the mouse over a row also
     moves the cursor: a pointer resting where the list re-rendered under it gets no
     `mousemove`, and a row you are pointing at that looks identical to the forty around it
     is a list that appears not to respond to the mouse at all. */
  .nv-row:hover { background: var(--bg-hover); }
  /* Was `--bg-active`, which is not a token this theme defines — so it resolved to nothing
     and NEITHER the hovered nor the keyboard-selected row was marked. */
  .nv-on, .nv-on:hover { background: var(--bg-selected); color: var(--text-primary); }
  .nv-on .nv-detail { color: var(--text-secondary); }
  /* On the selection fill the kind colours go muddy — hand the icon the row's own colour
     (see JavaKindIcon's `--jki-color`). */
  .nv-on .nv-icon { color: var(--text-primary); --jki-color: currentColor; }
  .nv-icon { display: inline-flex; align-self: center; color: var(--text-muted); flex-shrink: 0; }
  .nv-icon-gap { width: 13px; }
  .nv-name { color: var(--text-primary); white-space: nowrap; }
  /* The matched characters, and the only thing bold in the row — which is what
     makes a subsequence match legible instead of mysterious. */
  .nv-name :global(b) { color: var(--accent); font-weight: 700; }
  .nv-detail {
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .nv-detail :global(b) { color: var(--text-secondary); font-weight: 600; }
  .nv-tag {
    flex-shrink: 0;
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 0 4px;
  }

  .nv-note { padding: 14px; font-size: var(--font-size-sm); line-height: 1.55; color: var(--text-muted); }
  .nv-help {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 5px 12px;
    padding: 0 14px 14px;
    font-size: var(--font-size-xs);
  }
  .nv-help dt code { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary); }
  .nv-help dd { color: var(--text-muted); }

  .nv-foot {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 12px;
    border-top: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    flex-shrink: 0;
  }
  .nv-foot-open { display: inline-flex; align-items: center; gap: 4px; }
</style>
