<script lang="ts">
  /**
   * The Tags and fields sidebar section: the axes this vault can be sliced along.
   *
   * Two halves, because a vault has two vocabularies and they behave differently.
   *
   * **Fields** are declared. A note type states its frontmatter fields and, for
   * the closed ones, the values they take (`severity`, `status`, …), so this
   * section can list them exactly and count each — the counts are filter-only
   * searches, one per value, asked when a group is opened rather than all at once
   * (see `facets.ts`). Clicking a value is `status:aperto`, the same query the
   * search box would build, through the same store. Fields whose values are *open*
   * by design — the type leaves `values` empty so the vault's own answers are
   * offered — are not listed: enumerating them needs an index call that does not
   * exist, and a group showing a guess would be worse than no group.
   *
   * **Tags** are not declared anywhere. They exist because somebody typed `#foo`
   * in a note, and the only thing that knows the whole set is the index — which
   * answers searches, not vocabularies. So this half is the other way round: type
   * the tag, get the notes. The tags currently narrowing the results are shown
   * here as chips, which is the part that matters while working — what is filtered
   * right now, and one click to stop.
   *
   * Renders the body only: the host wraps it in the section's `PanelShell`.
   */
  import { untrack } from 'svelte';
  import { ChevronDown, ChevronRight, Hash, Search, X } from 'lucide-svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { garrulusSearchStore } from '$lib/stores/garrulus/search.svelte';
  import { garrulusVaultStore } from '$lib/stores/garrulus/vault.svelte';
  import { fieldToken, formatToken, tagToken } from '../search/query-tokens';
  import { countMatching, fieldFacets, mapWithLimit } from './facets';

  interface Props {
    /** Bring the search results on screen once a row has filtered them. */
    onShowResults?: () => void;
  }

  let { onShowResults }: Props = $props();

  /** Counts in flight at once — see `TypesPanel` for why this is small. */
  const IN_FLIGHT = 4;

  /** The one box: narrows the field rows as you type, and searches a tag when
   *  what you typed is one. */
  let filter = $state('');

  /** Which field groups are open. Session-shaped; the first one starts open so
   *  the section is never a column of closed headers. */
  let openGroups = $state<Record<string, boolean>>({});
  /** `key:value` → how many notes. Loaded per group, kept across collapses. */
  let counts = $state<Record<string, number | null>>({});
  let counting = $state<Record<string, boolean>>({});

  const facets = $derived(fieldFacets(garrulusVaultStore.types));

  const needle = $derived(filter.trim().toLowerCase());
  /** `#something` typed in the box is a tag to search, not a filter on the rows. */
  const typedTag = $derived(needle.startsWith('#') ? needle.slice(1) : '');

  /** The groups worth drawing, and which of their values survive the filter. A
   *  group whose label matches keeps all of its values, so typing a field name
   *  shows that field rather than emptying it. */
  const shown = $derived.by(() => {
    if (!needle || typedTag) return facets.map((f) => ({ facet: f, values: f.values }));
    return facets
      .map((f) => {
        if (f.label.toLowerCase().includes(needle) || f.key.includes(needle)) {
          return { facet: f, values: f.values };
        }
        return { facet: f, values: f.values.filter((v) => v.toLowerCase().includes(needle)) };
      })
      .filter((g) => g.values.length > 0);
  });

  /** The tags currently narrowing the results — the live half of this section. */
  const activeTags = $derived(garrulusSearchStore.tokens.filter((t) => t.kind === 'tag'));

  function cell(key: string, value: string): string {
    return `${key}\u0000${value}`;
  }

  /** Count a group's values. Runs when the group is opened, and only then: the
   *  cost is one round trip per value and nobody opens every group. */
  async function loadCounts(key: string, values: readonly string[]) {
    const missing = values.filter((v) => counts[cell(key, v)] === undefined);
    if (missing.length === 0 || counting[key]) return;

    counting = { ...counting, [key]: true };
    try {
      const found = await mapWithLimit(missing, IN_FLIGHT, (value) =>
        countMatching(formatToken(fieldToken(key, value))),
      );
      const next = { ...counts };
      missing.forEach((value, i) => (next[cell(key, value)] = found[i]));
      counts = next;
    } finally {
      counting = { ...counting, [key]: false };
    }
  }

  function toggleGroup(key: string, values: readonly string[]) {
    const nowOpen = !(openGroups[key] ?? false);
    openGroups = { ...openGroups, [key]: nowOpen };
    if (nowOpen) void loadCounts(key, values);
  }

  function applyField(key: string, value: string) {
    void garrulusSearchStore.toggleAndRun(fieldToken(key, value));
    onShowResults?.();
  }

  function applyTag(tag: string) {
    void garrulusSearchStore.toggleAndRun(tagToken(tag));
    onShowResults?.();
  }

  function onFilterKey(e: KeyboardEvent) {
    if (e.key !== 'Enter' || !typedTag) return;
    e.preventDefault();
    applyTag(typedTag);
    filter = '';
  }

  /**
   * Open the first group on arrival, so the section opens with something in it.
   *
   * Guarded by a plain variable rather than by reading `openGroups`, and the work runs
   * `untrack`ed: an effect that depends on the state it writes is a loop waiting
   * for the first condition that stops converging.
   */
  let seeded: string | null = null;
  $effect(() => {
    const first = facets[0];
    const key = `${garrulusVaultStore.root ?? ''}::${first?.key ?? ''}`;
    if (!first || key === seeded) return;
    seeded = key;
    untrack(() => {
      openGroups = { ...openGroups, [first.key]: true };
      void loadCounts(first.key, first.values);
    });
  });
</script>

<div class="tg">
  <div class="tg-filter">
    <span class="tg-filter-icon">
      {#if typedTag}<Hash size={12} />{:else}<Search size={12} />{/if}
    </span>
    <input
      class="tg-filter-input"
      type="text"
      spellcheck="false"
      autocomplete="off"
      aria-label="Filter fields, or type a #tag to search for it"
      placeholder="Filter fields — or #tag then Enter"
      bind:value={filter}
      onkeydown={onFilterKey}
    />
    {#if filter}
      <button
        type="button"
        class="tg-filter-x"
        aria-label="Clear the filter"
        onclick={() => (filter = '')}
      >
        <X size={11} />
      </button>
    {/if}
  </div>

  {#if !garrulusVaultStore.isOpen}
    <EmptyState
      message="No vault open."
      description="Tags and fields belong to a vault, so there are none until one is open."
    />
  {:else}
    <div class="tg-subhead">Tags</div>
    {#if activeTags.length > 0}
      <div class="tg-chips">
        {#each activeTags as tag (tag.value)}
          <button
            type="button"
            class="tg-chip"
            use:tooltip={'Stop filtering by this tag'}
            onclick={() => applyTag(tag.value)}
          >
            #{tag.value}<X size={10} />
          </button>
        {/each}
      </div>
    {:else}
      <p class="tg-hint">
        Type <code>#name</code> above and press Enter to see every note carrying that
        tag. The ones you are filtering by appear here.
      </p>
    {/if}

    <div class="tg-subhead">Fields</div>
    {#if facets.length === 0}
      <EmptyState
        message="No field has a fixed set of values."
        description="A note type declares the fields its notes carry; the ones with a closed list of values are listed here, ready to filter by."
        compact
      />
    {:else if shown.length === 0}
      <EmptyState message="Nothing matches that." compact />
    {:else}
      {#each shown as group (group.facet.key)}
        {@const isOpen = openGroups[group.facet.key] ?? false}
        <div class="tg-group">
          <button
            type="button"
            class="tg-group-head"
            aria-expanded={isOpen}
            onclick={() => toggleGroup(group.facet.key, group.facet.values)}
          >
            <span class="tg-caret">
              {#if isOpen}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if}
            </span>
            <span class="tg-group-name">{group.facet.label}</span>
            <span class="tg-group-key">{group.facet.key}</span>
            {#if counting[group.facet.key]}<Spinner size={10} />{/if}
          </button>

          {#if isOpen}
            <ul class="tg-values">
              {#each group.values as value (value)}
                {@const token = fieldToken(group.facet.key, value)}
                {@const active = garrulusSearchStore.has(token)}
                {@const count = counts[cell(group.facet.key, value)]}
                <li>
                  <button
                    type="button"
                    class="tg-value"
                    class:active
                    aria-pressed={active}
                    title={active
                      ? `Stop filtering by ${group.facet.key}:${value}`
                      : `Show the notes where ${group.facet.key} is ${value}`}
                    onclick={() => applyField(group.facet.key, value)}
                  >
                    <span class="tg-value-name">{value}</span>
                    {#if count !== null && count !== undefined}
                      <span class="tg-pill">{count}</span>
                    {/if}
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/each}
    {/if}
  {/if}
</div>

<style>
  .tg { display: flex; flex-direction: column; min-height: 0; }

  .tg-filter {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 8px 8px 4px;
    padding: 0 6px;
    height: 26px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-input);
  }
  .tg-filter:focus-within { border-color: var(--border-focus); }
  .tg-filter-icon { display: flex; color: var(--text-muted); flex: none; }
  .tg-filter-input {
    flex: 1;
    min-width: 0;
    border: none;
    outline: none;
    background: none;
    color: var(--text-primary);
    font-family: inherit;
    font-size: var(--font-size-xs);
  }
  .tg-filter-input::placeholder { color: var(--text-disabled); }
  .tg-filter-x {
    display: flex;
    border: none;
    background: none;
    color: var(--text-muted);
    cursor: pointer;
    padding: 2px;
  }
  .tg-filter-x:hover { color: var(--text-primary); }

  .tg-subhead {
    padding: 10px 10px 4px;
    font-size: var(--font-size-3xs);
    font-weight: 700;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .tg-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    padding: 0 10px 8px;
  }
  .tg-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 20px;
    padding: 0 6px;
    border-radius: var(--radius-sm);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
    font-family: var(--font-code);
    font-size: var(--font-size-3xs);
    cursor: pointer;
  }
  .tg-chip:hover { background: color-mix(in srgb, var(--accent) 22%, transparent); }

  .tg-hint {
    margin: 0;
    padding: 0 12px 8px;
    font-size: var(--font-size-2xs);
    line-height: 1.5;
    color: var(--text-muted);
  }
  .tg-hint code {
    font-family: var(--font-code);
    color: var(--text-secondary);
  }

  .tg-group-head {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 24px;
    padding: 0 10px;
    border: none;
    background: none;
    text-align: left;
    cursor: pointer;
    font-family: inherit;
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
  }
  .tg-group-head:hover { background: var(--bg-hover); color: var(--text-primary); }

  .tg-caret {
    display: flex;
    align-items: center;
    flex: none;
    color: var(--text-muted);
  }

  .tg-group-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* The key is what the query language actually takes; the label is what the
     type calls it. Both are shown because the row teaches the query. */
  .tg-group-key {
    flex: 1;
    min-width: 0;
    font-family: var(--font-code);
    font-size: var(--font-size-3xs);
    color: var(--text-disabled);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tg-values { list-style: none; margin: 0; padding: 0; }

  .tg-value {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 22px;
    padding: 0 10px 0 27px;
    border: none;
    background: none;
    text-align: left;
    cursor: pointer;
    font-family: inherit;
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
  }
  .tg-value:hover { background: var(--bg-hover); color: var(--text-primary); }
  .tg-value.active {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    color: var(--text-primary);
  }
  .tg-value-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tg-pill {
    flex: none;
    padding: 0 5px;
    border-radius: var(--radius-sm);
    background: var(--bg-overlay);
    color: var(--text-muted);
    font-size: var(--font-size-3xs);
    line-height: 15px;
  }
  .tg-value.active .tg-pill { color: var(--text-secondary); }
</style>
