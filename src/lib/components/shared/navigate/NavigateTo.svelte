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
   * ## Categories too large to hold
   *
   * Some sources cannot be handed over whole — a Java classpath is hundreds of
   * thousands of classes, and serialising all of them to answer a question about
   * twenty is not a trade worth making. Such a category supplies {@link
   * NavigateCategory.search} instead of `items`: the query goes to the host, which
   * returns candidates, and everything after that — the scoring, the ordering, the
   * lit characters, the grouping — happens here on that subset exactly as it does
   * for a local category. The host's job is to *narrow*, not to rank; two answers
   * to "which of these is the best match" is how the two start disagreeing.
   *
   * A remote category is only asked while it is on screen (its own tab, or All),
   * is debounced, and drops the answer to a superseded query — so typing eight
   * characters is not eight round-trips whose results race each other.
   *
   * ## Sources, and why they are not more tabs
   *
   * A category can declare {@link NavigateSource}s — named places its rows come from, chosen
   * from one picker on the header row. The alternative is what this replaced: a *Classes* tab
   * and a *Library classes* tab, a *Files* tab and a *Library files* tab. That makes the
   * overlay's top-level structure answer "where might it be" instead of "what am I looking
   * for", and it forces the user to check two tabs for one question. A source that supplies
   * both `items` and `search` unions them, so "the project and its dependencies" is a single
   * list scored together rather than two lists to compare by eye.
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
  import { untrack, type Snippet } from 'svelte';
  import type { IconComponent } from '$lib/types/icon';
  import { Search, CornerDownLeft } from 'lucide-svelte';
  import Modal from '../Modal.svelte';
  import Tabs, { type TabItem } from '../ui/Tabs.svelte';
  import Spinner from '../ui/Spinner.svelte';
  import CodePreview from '../ui/CodePreview.svelte';
  import Select from '../ui/Select.svelte';
  import type { LanguageDescriptor } from '../ui/code-editor';
  import Kbd from '../internal/Kbd.svelte';
  import { tooltip } from '$lib/actions/tooltip';
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
    /**
     * Where this row comes from, shown as a chip on the right: the module that compiles it, the
     * project it belongs to, the artifact it was read out of.
     *
     * Separate from {@link facet} because they answer to different things — the facet is what the
     * dropdown *filters* by, and there are origins nobody would filter by (an artifact) and
     * facets not worth repeating on the row. On a reactor "which of these four is mine" is the
     * question the name alone cannot answer.
     */
    origin?: string;
    /**
     * This row is **not the user's own** — it comes from a dependency, a vendored copy, a
     * read-only source.
     *
     * Tinted rather than badged, because the fact it warns about — that opening this lands you
     * somewhere you cannot edit — matters every time the row is looked at, and a badge is read
     * once and then stops being noticed. Same tone the editor's tab strip uses for the same
     * meaning.
     */
    external?: boolean;
    /**
     * An extra dimension this item belongs to — a Maven module, a schema, a connection.
     *
     * Deliberately unnamed by this component: it renders whatever the host calls it (see
     * {@link NavigateCategory.facetLabel}) as a dropdown beside the field, and filters on
     * equality. A reactor with forty modules is the case it exists for — "the `OrderDao` in
     * *this* module" is a question the fuzzy score cannot be asked.
     */
    facet?: string;
    onOpen: () => void;
  }

  /**
   * A file, for the preview column.
   *
   * The **whole** file plus a line to point at, rather than a pre-sliced window: the column is a
   * real read-only editor, so it wants the document its line numbers, its multi-line constructs
   * and its scrolling all come from. The host resolves the language — `shared/` does not know
   * that `.jsp` is a thing.
   */
  export interface NavigatePreview {
    /** Shown above it — usually the path. */
    title: string;
    text: string;
    language: LanguageDescriptor;
    /** The line to band and scroll to — the declaration. */
    activeLine?: number | null;
  }

  /**
   * One place a category's rows can come from.
   *
   * A source may supply `items`, `search`, or **both** — and both at once is the interesting
   * case: it is how "the project *and* its dependencies" becomes one list that is scored and
   * ranked together, rather than two tabs the user has to check in turn. The two halves keep
   * their own economics (items are pulled once per opening, a search is asked per query and
   * debounced); all this decides is which of them feed the pool.
   */
  export interface NavigateSource {
    id: string;
    label: string;
    items?: () => NavigateItem[] | Promise<NavigateItem[]>;
    search?: (query: string) => Promise<NavigateItem[]>;
    /** Shown instead of the category's own when this source has nothing to show yet — a
     *  search-only source is empty *until you type*, which is not the same as being empty. */
    emptyMessage?: string;
  }

  export interface NavigateCategory {
    id: string;
    label: string;
    /**
     * Named alternatives for where this category's rows come from, offered as a picker on the
     * header row. When present they replace {@link items} / {@link search}.
     *
     * The picker is **one control for the whole overlay**: sources are matched across categories
     * by id, so choosing "dependencies" on Classes means the same thing when you Tab to Files.
     * A category that does not declare that id falls back to its own first source, and one with
     * no sources at all is simply unaffected.
     */
    sources?: NavigateSource[];
    /** Called once per opening. May be async. Omit when the category is {@link search}-backed. */
    items?: () => NavigateItem[] | Promise<NavigateItem[]>;
    /**
     * Host-side search, for a source too large to hand over whole. Called with the
     * query's text (directives already stripped), debounced, and only while this
     * category is showing. Return **candidates**, not a ranking — the scoring and
     * the highlighting happen here.
     *
     * Takes precedence over {@link items} when both are given.
     */
    search?: (query: string) => Promise<NavigateItem[]>;
    /** Shown when this category has nothing at all, before any filtering. */
    emptyMessage?: string;
    /**
     * The selected item's surroundings, for the preview column.
     *
     * Optional, and its absence is the whole story: a host that supplies none gets exactly the
     * list it had before, no column and no extra width. It answers the question the list cannot
     * — *is this the `OrderDao` I meant* — which on a legacy tree with four classes of the same
     * name is the only question that matters.
     *
     * Called for the highlighted row, debounced by the selection settling rather than by a
     * timer; a walk down the list with the arrow keys must not fire one read per row it passes.
     */
    preview?: (item: NavigateItem) => Promise<NavigatePreview | null>;
    /** What this category's {@link NavigateItem.facet} is called — `Module`, `Schema`. Absent
     *  means no facet dropdown, however many items happen to carry one. */
    facetLabel?: string;
  }

  interface Props {
    categories: NavigateCategory[];
    /** Category to open on. Defaults to `all`. */
    initialCategory?: string;
    /** Seed text — e.g. the editor's selection. */
    initialQuery?: string;
    /** Which {@link NavigateSource} to open on. A host with an "also search the classpath"
     *  preference passes it here, so the setting decides the default and the picker still
     *  decides this search. */
    initialSource?: string;
    /** What the source picker is called — `Source`, `Where`. */
    sourceLabel?: string;
    title?: string;
    /**
     * Rendered at the end of the **field row**, where a search bar keeps its keys.
     *
     * A host's own one-bit switches go here — anything that re-runs the search on the spot and
     * has no name worth a dropdown. Deliberately a snippet rather than a declared list: what
     * those bits *mean* is the host's business, and the alternative is this component growing a
     * vocabulary of them one product at a time.
     */
    fieldActions?: Snippet;
    /**
     * Read nothing until a character has been typed.
     *
     * Off by default, because for a host whose categories are already in memory — a schema's
     * tables, a handful of open buffers — the list on opening *is* the feature. It is for the
     * other kind: a category whose `items()` is a project-wide walk (every class in every
     * module) pays that walk on every <kbd>Ctrl</kbd>+<kbd>N</kbd>, including the ones where
     * the user knew what they were going to type before the overlay appeared. Nobody reads
     * "every class in the project" as a list, so nobody should be made to wait for it.
     */
    requireQuery?: boolean;
    onClose: () => void;
  }

  let {
    categories,
    initialCategory = 'all',
    initialQuery = '',
    initialSource = '',
    sourceLabel = 'Source',
    title = 'Go to',
    fieldActions,
    requireQuery = false,
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

  /** Items per category id, for the {@link NavigateCategory.search}-backed ones. Kept apart
   *  from `loaded` so a remote answer landing never disturbs the local lists. */
  let remote = $state<Record<string, NavigateItem[]>>({});
  /** Whether a remote category has a request in flight — the field's spinner. */
  let searching = $state(false);

  // ── sources ─────────────────────────────────────────────────────────────────
  // svelte-ignore state_referenced_locally
  let sourceId = $state(initialSource);

  /** The source in play for a category: the chosen id, else its own first — a category that does
   *  not offer what is selected shows what it has rather than nothing. */
  function sourceOf(c: NavigateCategory): NavigateSource | undefined {
    if (!c.sources?.length) return undefined;
    return c.sources.find((s) => s.id === sourceId) ?? c.sources[0];
  }
  function itemsOf(c: NavigateCategory) { return sourceOf(c)?.items ?? (c.sources ? undefined : c.items); }
  function searchOf(c: NavigateCategory) { return sourceOf(c)?.search ?? (c.sources ? undefined : c.search); }

  /** The picker's options, from whichever categories are on screen. Union rather than the active
   *  category's own, so the control does not change shape as you Tab across the strip. */
  const sourceOptions = $derived.by<{ value: string; label: string }[]>(() => {
    const seen = new Map<string, string>();
    for (const c of categories) {
      if (active !== 'all' && c.id !== active) continue;
      for (const s of c.sources ?? []) if (!seen.has(s.id)) seen.set(s.id, s.label);
    }
    return [...seen].map(([value, label]) => ({ value, label }));
  });

  const localCategories = $derived(categories.filter((c) => !!itemsOf(c)));
  const remoteCategories = $derived(categories.filter((c) => !!searchOf(c)));

  // ── the facet, when the showing category has one ─────────────────────────────
  /** The chosen facet value, or `''` for "any". Reset when the tab changes, because a module
   *  that exists for classes need not exist for files. */
  let facet = $state('');

  /** The category whose facet is in play — only a single-category tab has one, since two
   *  categories could name the same dimension differently. */
  const facetCategory = $derived(
    active === 'all' ? undefined : categories.find((c) => c.id === active && c.facetLabel),
  );

  /** The distinct values present, in sorted order. Taken from the items rather than declared by
   *  the host: a dropdown offering a module with nothing in it is a dead end you have to try to
   *  find out. */
  const facetValues = $derived.by<string[]>(() => {
    const category = facetCategory;
    if (!category) return [];
    const seen = new Set<string>();
    for (const item of pool(category.id)) if (item.facet) seen.add(item.facet);
    return [...seen].sort((a, b) => a.localeCompare(b));
  });

  /**
   * Everything a category has to offer for the current source: what was pulled once, plus
   * whatever the host searched for.
   *
   * The **union** is what makes "the project and its dependencies" a single ranked list instead
   * of two. A source supplying only one of the two simply contributes an empty other half.
   */
  function pool(id: string): NavigateItem[] {
    const local = loaded[id];
    const found = remote[id];
    if (!found?.length) return local ?? [];
    if (!local?.length) return found;
    return [...local, ...found];
  }

  /** Whether anything has been typed. A **derived** rather than a read of `query` inside the
   *  effect below: this only changes on the empty↔non-empty edge, so the pull happens on the
   *  first keystroke and not on every one after it. */
  const typed = $derived(query.trim().length > 0);

  // Pulled once, on mount. `$effect` rather than `onMount` so a host that swaps
  // the category list — a repository closing, a second connection opening, the
  // source changing under it — re-pulls rather than showing the previous list.
  $effect(() => {
    if (requireQuery && !typed) {
      loaded = {};
      loading = false;
      return;
    }
    // Resolved here, synchronously, rather than inside the `await`: that is what makes the
    // effect depend on the chosen source, since a read after the first await tracks nothing.
    const list = localCategories.map((c) => ({ id: c.id, items: itemsOf(c)! }));
    let live = true;
    loading = true;
    void (async () => {
      const next: Record<string, NavigateItem[]> = {};
      await Promise.all(
        list.map(async (category) => {
          try {
            next[category.id] = (await category.items()) ?? [];
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

  /** How long to wait after the last keystroke before asking a remote category. */
  const SEARCH_DEBOUNCE_MS = 140;

  /**
   * Ask each visible remote category, debounced, and drop the answer to a superseded query.
   *
   * Only the ones on screen: a category nobody is looking at costs a round-trip for a list
   * nobody will read. An empty query asks nothing — "everything on the classpath" is not an
   * answer, and the host would have to refuse it anyway.
   */
  $effect(() => {
    const text = parsed.text.trim();
    const showing = active;
    // Resolved synchronously, like the local pull above, so switching source re-asks — and so a
    // source with nothing remote about it clears what the previous one had found.
    const wanted = remoteCategories
      .filter((c) => showing === 'all' || showing === c.id)
      .map((c) => ({ id: c.id, search: searchOf(c)! }));
    if (!wanted.length || !text) {
      searching = false;
      // Cleared rather than kept: rows from the previous query would otherwise sit under a
      // field that no longer says what produced them.
      //
      // `untrack`, because this effect WRITES `remote` — reading it as a dependency would make
      // the write re-run the effect, which is the read-modify-write loop the runes docs warn
      // about. What this effect depends on is the query, the tab and the source, nothing else.
      if (Object.keys(untrack(() => remote)).length) remote = {};
      return;
    }

    let live = true;
    searching = true;
    const timer = setTimeout(() => {
      void (async () => {
        const next: Record<string, NavigateItem[]> = {};
        await Promise.all(
          wanted.map(async (category) => {
            try {
              next[category.id] = (await category.search(text)) ?? [];
            } catch {
              next[category.id] = [];
            }
          }),
        );
        if (!live) return;
        remote = next;
        searching = false;
      })();
    }, SEARCH_DEBOUNCE_MS);

    return () => { live = false; clearTimeout(timer); };
  });

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
      // A remote answer already IS the answer to this query, so preparing it costs the few
      // hundred candidates that came back rather than the source they came from.
      out[category.id] = pool(category.id).map((item) => {
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
    // Applied before the scoring, not after: filtering a ranked list would leave the "best few"
    // of `All` chosen from rows the facet then removed, and the tab would look emptier than it is.
    const wantFacet = category.id === facetCategory?.id ? facet : '';
    for (const p of prepared[category.id] ?? []) {
      if (wantFacet && p.item.facet !== wantFacet) continue;
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

  /** What the empty list says. The showing category's own words when there is exactly one — "no
   *  classes indexed yet" and "type to search the jars" are different situations, and a single
   *  generic sentence for both is how an overlay comes to look broken while working correctly. */
  const emptyNote = $derived.by<string>(() => {
    // Nothing has been read yet, on purpose — say so, rather than "there is nothing here",
    // which describes a project with no classes in it.
    const fallback = requireQuery
      ? 'Type to search.'
      : 'There is nothing here to search yet.';
    if (active === 'all') return fallback;
    const category = categories.find((c) => c.id === active);
    if (!category) return fallback;
    return sourceOf(category)?.emptyMessage ?? category.emptyMessage ?? fallback;
  });

  function selectTab(id: string) {
    active = id;
    cursor = 0;
    // A module that exists for classes need not exist for files, and a filter still applied
    // under a tab that cannot show it is an empty list with no visible reason.
    facet = '';
  }

  function cycleTab(delta: number) {
    const i = tabs.findIndex((t) => t.id === active);
    selectTab(tabs[(i + delta + tabs.length) % tabs.length].id);
  }

  // ── the preview of the highlighted row ───────────────────────────────────────
  const anyPreview = $derived(categories.some((c) => c.preview));
  let preview = $state<NavigatePreview | null>(null);
  let previewing = $state(false);

  /** The highlighted row's identity — what the preview keys off, so a re-render that produced
   *  an equal-but-new row object does not re-read the file. */
  const currentKey = $derived(rows[selected] ? `${rows[selected].category}:${rows[selected].id}` : '');

  /**
   * How long the selection must sit still before the preview is asked for.
   *
   * Walking a list with the arrow key passes over rows nobody wants to read, and what a preview
   * costs is not knowable from here: a project file is a read, a library class can be a
   * decompile. Without this, holding ↓ queues one of those per row it passes.
   */
  const PREVIEW_SETTLE_MS = 120;

  $effect(() => {
    void currentKey;
    const row = untrack(() => rows[selected]);
    const category = row ? categories.find((c) => c.id === row.category) : undefined;
    if (!row || !category?.preview) {
      preview = null;
      previewing = false;
      return;
    }
    let live = true;
    previewing = true;
    const timer = setTimeout(() => {
      void (async () => {
        try {
          const answer = await category.preview!(row);
          if (!live) return;
          preview = answer;
        } catch {
          // Unreadable is not an error worth a message: the row itself still says what it is.
          if (live) preview = null;
        } finally {
          if (live) previewing = false;
        }
      })();
    }, PREVIEW_SETTLE_MS);
    return () => { live = false; clearTimeout(timer); };
  });

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

<Modal
  {onClose}
  width={anyPreview ? '1000px' : '700px'}
  height={anyPreview ? '600px' : '520px'}
  padBody={false}
  ariaLabel={title}
>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="nv" role="group" onkeydown={onKeydown}>
    <!-- The tabs come FIRST: they say what is being searched, and a control that changes the
         meaning of the field below it belongs above that field, not under it. -->
    <div class="nv-tabs">
      <Tabs
        items={tabs}
        value={active}
        variant="pill"
        size="sm"
        ariaLabel="What to search"
        onSelect={selectTab}
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
      <!-- On the header row rather than beside the field: both narrow the same thing the tabs
           do — which slice of the project is on the table — so they belong with them, and the
           field row stays a field row. -->
      {#if sourceOptions.length > 1}
        <Select
          value={sourceId}
          options={sourceOptions}
          size="sm"
          highlight={sourceId !== sourceOptions[0]?.value}
          ariaLabel={sourceLabel}
          onchange={(v) => (sourceId = v)}
        />
      {/if}
      {#if facetCategory && facetValues.length}
        <Select
          value={facet}
          options={[
            { value: '', label: `All ${facetCategory.facetLabel?.toLowerCase()}s` },
            ...facetValues.map((v) => ({ value: v, label: v })),
          ]}
          size="sm"
          highlight={!!facet}
          searchable={facetValues.length > 12}
          searchPlaceholder={`Filter ${facetCategory.facetLabel?.toLowerCase()}s…`}
          ariaLabel={facetCategory.facetLabel}
          onchange={(v) => (facet = v)}
        />
      {/if}
    </div>

    <div class="nv-search">
      <Search size={14} />
      <input
        bind:this={field}
        bind:value={query}
        class="nv-field"
        type="text"
        data-modal-autofocus
        spellcheck="false"
        autocomplete="off"
        placeholder="Type to search — try sort:new or ext:sql"
        aria-label={title}
        role="combobox"
        aria-expanded={rows.length > 0}
        aria-controls="nv-results"
      />
      {#if loading || searching}<Spinner size={13} />{/if}
      {#if fieldActions}<div class="nv-actions">{@render fieldActions()}</div>{/if}
    </div>

    <div class="nv-body" class:nv-split={anyPreview}>
    <div class="nv-list" id="nv-results" role="listbox" aria-label="Results" bind:this={list}>
      {#if loading}
        <p class="nv-note">Reading…</p>
      {:else if !rows.length && searching}
        <!-- Only when there is nothing else to read: a remote answer landing under results
             that are already useful must not blank them out first. -->
        <p class="nv-note">Searching…</p>
      {:else if !rows.length}
        <p class="nv-note">
          {#if query.trim()}
            Nothing matches <b>{query.trim()}</b>.
          {:else}
            {emptyNote}
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
              class:nv-ext={row.external}
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
              <!-- Where the row came from, always in the same slot: the module for something
                   this build compiles, the project for a sibling of it, the artifact for
                   something it merely depends on. -->
              {#if row.origin}<span class="nv-origin">{row.origin}</span>{/if}
              {#if row.tag}<span class="nv-tag">{row.tag}</span>{/if}
            </button>
          {/each}
        {/each}
      {/if}
    </div>

    {#if anyPreview}
      <div class="nv-preview">
        {#if preview}
          <div class="nv-pv-head" use:tooltip={preview.title}>{preview.title}</div>
          <div class="nv-pv-body">
            <CodePreview
              text={preview.text}
              language={preview.language}
              activeLine={preview.activeLine ?? null}
            />
          </div>
        {:else if previewing}
          <p class="nv-note">Reading…</p>
        {:else}
          <p class="nv-note">Nothing to preview.</p>
        {/if}
      </div>
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
  .nv-actions { display: flex; gap: 4px; flex-shrink: 0; }

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

  /* One column without a preview, two with it — so a host that supplies none gets exactly the
     layout it had, and the split never appears as an empty half. */
  .nv-body { flex: 1; min-height: 0; display: flex; }
  .nv-body.nv-split { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
  .nv-list { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 0; }
  .nv-body.nv-split .nv-list { border-right: 1px solid var(--border-subtle); }

  .nv-preview { min-height: 0; display: flex; flex-direction: column; background: var(--bg-base); }
  .nv-pv-head {
    flex-shrink: 0;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border-subtle);
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }
  /* No `overflow` of its own: the editor inside scrolls, and a second scroller around it is two
     scrollbars for one document. */
  .nv-pv-body { flex: 1; min-height: 0; }

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
     (see SymbolKindIcon's `--jki-color`). */
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
  /* Where the row is from. Filled rather than outlined, so it reads as a label on the row and
     not as a second kind tag beside `nv-tag`. */
  .nv-origin {
    flex-shrink: 0;
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    background: var(--bg-elevated);
    border-radius: var(--radius-sm);
    padding: 0 5px;
  }

  /* Not the user's own — a row that opens something read-only. Tinted in the same hue and for
     the same reason the editor's tab strip tints an external tab: not an error (nothing is
     wrong), but not an ordinary row of your project either. The bar on the left edge is what
     makes a run of them legible as a block while scrolling. */
  .nv-row.nv-ext {
    background: color-mix(in srgb, var(--warning) 7%, transparent);
    box-shadow: inset 2px 0 0 color-mix(in srgb, var(--warning) 45%, transparent);
  }
  .nv-row.nv-ext:hover { background: color-mix(in srgb, var(--warning) 12%, transparent); }
  /* The selection has to win over the tint, and equal specificity would leave that to source
     order — which is exactly the kind of thing that breaks when a rule moves. */
  .nv-row.nv-ext.nv-on, .nv-row.nv-ext.nv-on:hover {
    background: var(--bg-selected);
    box-shadow: inset 2px 0 0 var(--warning);
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
