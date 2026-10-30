<script lang="ts">
  /**
   * The filter row every profiler tab starts with: a search over the tab's names, a field for the packages to
   * look at, and a switch for "only this project's classes".
   *
   * One component for all of them, because "only my code" has to mean the same thing in the flame graph, the
   * tables and the memory view, and because the state lives in the store: switching tab must not lose it. The
   * filter is remembered per project; the search is not.
   *
   * ## The package field
   *
   * Typed by hand, like a search: `it.acme.*, !*.generated.*`. A name without a wildcard covers everything inside
   * it, `*` matches any run of characters, a leading `!` hides instead of keeps. While you type, the grey text
   * after the caret is the longest package every candidate agrees on — a fact the project's own packages gave us,
   * never a guess — and Tab (or →) takes it. The filter applies as you type, a moment after you stop.
   */
  import type { Snippet } from 'svelte';
  import { Filter as FilterIcon } from 'lucide-svelte';
  import FilterButton from '$lib/components/shared/ui/FilterButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuProfilerStore as store } from '$lib/stores/bennu/profiler.svelte';
  import { completePackage, formatPatterns, parsePatterns } from './profiler-filter';

  interface Props {
    searchPlaceholder?: string;
    /** Extra controls on the right (a thread picker, a toggle) — what is specific to the tab. */
    children?: Snippet;
  }

  let { searchPlaceholder = 'Filter by name…', children }: Props = $props();

  let search = $state<ReturnType<typeof SearchBar> | null>(null);
  // A palette command or Ctrl+F asked for the search box; it takes the request once it is on screen.
  $effect(() => {
    if (store.searchFocusRequested && search) {
      search.focus();
      store.consumeSearchFocus();
    }
  });

  // ── the package field ───────────────────────────────────────────────────────────────────────
  const filter = $derived(store.filter);
  let text = $state(formatPatterns(store.filter));
  let editing = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  // The stored filter wins while you are not typing in the field: a palette command that clears it, or the
  // project's own load, must show here.
  $effect(() => {
    const stored = formatPatterns(filter);
    if (!editing) text = stored;
  });

  function apply() {
    clearTimeout(timer);
    const p = parsePatterns(text);
    store.setFilter({ ...store.filter, include: p.include, exclude: p.exclude });
  }

  function onInput(v: string) {
    text = v;
    clearTimeout(timer);
    timer = setTimeout(apply, 350);
  }

  /** The word being typed — after the last separator. */
  const token = $derived(text.split(/[\s,;]+/).pop() ?? '');
  const completed = $derived(completePackage(token, store.projectPackages));
  const ghost = $derived(completed ? completed.slice(token.length) : '');

  function acceptGhost() {
    onInput(text.slice(0, text.length - token.length) + completed);
  }

  const HELP =
    'Packages or classes, separated by commas. “it.acme.*” keeps them — a bare name already covers what is inside it — ' +
    'and “!*.generated.*” hides them. * matches anything, ? one character. Tab takes the grey completion.';
</script>

<div class="bar">
  <div class="search">
    <SearchBar
      bind:this={search}
      bind:query={() => store.query, (q) => store.setQuery(q)}
      showRegex={false}
      showCounter={false}
      placeholder={searchPlaceholder}
      ariaLabel="Filter the profile by name"
    />
  </div>
  <div class="pkgs" use:tooltip={HELP}>
    <Input
      size="sm"
      bind:value={text}
      placeholder="Packages: it.acme.*, !*.generated.*"
      ariaLabel="Packages to show"
      clearable
      {ghost}
      onacceptghost={acceptGhost}
      oninput={onInput}
      onclear={apply}
      onfocus={() => { editing = true; void store.ensureProjectClasses(); }}
      onblur={() => { editing = false; apply(); }}
      onkeydown={(e) => { if (e.key === 'Enter') apply(); }}
    />
  </div>
  <FilterButton
    label="My code"
    icon={FilterIcon}
    active={filter.project_only}
    onClick={() => store.setFilter({ ...filter, project_only: !filter.project_only })}
  />
  {@render children?.()}
</div>

<style>
  .bar { display: flex; align-items: center; gap: 8px; padding: 4px 8px; flex-wrap: wrap; }
  .search { flex: 1; min-width: 160px; max-width: 320px; }
  .pkgs { flex: 1; min-width: 200px; max-width: 360px; }
</style>
