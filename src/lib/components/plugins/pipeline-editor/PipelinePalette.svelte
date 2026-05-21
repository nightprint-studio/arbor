<!--
  PipelinePalette — left column of the pipeline editor. Search input on top,
  collapsible operation categories below. Clicking an op fires `add_step`.

  Local state: the search query (instant filter feedback, debounced commit
  back to the plugin via `search_changed` on blur/Enter) and the per-category
  collapsed map (persisted in localStorage). The plugin owns the canonical
  search_query and replays it when other state is mutated; we resync via
  `$effect`.
-->
<script lang="ts">
  import { Search, X as XIcon } from 'lucide-svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  import type { OpCategory, FireAction } from './types';
  import { catColor, iconFor, makeFire } from './helpers';

  interface Props {
    searchQuery?: string;
    operations:   OpCategory[];
    actions:      Record<string, string>;
    iconMap?:     Record<string, any>;
    fireAction:   FireAction;
  }
  let { searchQuery, operations, actions, iconMap, fireAction }: Props = $props();

  const fire = $derived(makeFire(actions, fireAction));

  // ── Client-side palette filter — instant feedback, no round-trip ─────────
  // svelte-ignore state_referenced_locally
  let query = $state<string>(searchQuery ?? '');
  $effect(() => {
    // Keep in sync when the plugin replays the form (e.g. after other actions).
    query = searchQuery ?? '';
  });

  const lowerQuery = $derived(query.trim().toLowerCase());
  const filteredOps = $derived.by(() => {
    if (!lowerQuery) return operations;
    return operations
      .map(cat => ({
        ...cat,
        ops: (Array.isArray(cat.ops) ? cat.ops : []).filter(op =>
          op.label.toLowerCase().includes(lowerQuery) ||
          (op.summary ?? '').toLowerCase().includes(lowerQuery) ||
          op.kind.toLowerCase().includes(lowerQuery)
        ),
      }))
      .filter(cat => cat.ops.length > 0);
  });

  function emitSearch() {
    if (actions?.search_changed && query !== (searchQuery ?? '')) {
      fireAction(actions.search_changed, { value: query });
    }
  }

  function clearSearch() {
    query = '';
    emitSearch();
  }

  // ── Collapsible palette categories ───────────────────────────────────────
  // Persist collapsed state per category (localStorage). Default = expanded.
  // When the user types in the search box we force-open all categories so the
  // hits don't vanish behind a collapsed header.
  const LS_CAT_COLLAPSED = 'arbor:pe-cat-collapsed';
  let catCollapsed = $state<Record<string, boolean>>(loadCatCollapsed());
  function loadCatCollapsed(): Record<string, boolean> {
    try {
      const raw = localStorage.getItem(LS_CAT_COLLAPSED);
      return raw ? JSON.parse(raw) : {};
    } catch { return {}; }
  }
  function setCatOpen(id: string, open: boolean) {
    // Persist the INVERTED bit (collapsed) to keep the LS payload semantics
    // backwards-compatible with profiles saved before this refactor.
    catCollapsed = { ...catCollapsed, [id]: !open };
    try { localStorage.setItem(LS_CAT_COLLAPSED, JSON.stringify(catCollapsed)); } catch {}
  }
  const searching = $derived(!!lowerQuery);
  function isCatCollapsed(id: string): boolean {
    if (searching) return false;
    return !!catCollapsed[id];
  }
</script>

<section class="pe-col pe-col-palette">
  <header class="pe-search">
    <Search size={13} class="pe-search-icon" />
    <input type="text"
           bind:value={query}
           onblur={emitSearch}
           onkeydown={(e) => { if (e.key === 'Enter') { emitSearch(); (e.target as HTMLInputElement).blur(); } }}
           placeholder="Cerca operazione…" />
    {#if query}
      <button class="pe-search-clear" type="button" onclick={clearSearch} aria-label="Clear">
        <XIcon size={12} />
      </button>
    {/if}
  </header>

  <div class="pe-palette-body">
    {#if filteredOps.length === 0}
      <p class="pe-empty">Nessuna operazione trovata.</p>
    {/if}
    {#each filteredOps as cat (cat.id)}
      {@const catOps = Array.isArray(cat.ops) ? cat.ops : []}
      <div class="pe-cat">
        <Collapsible
          chevron
          open={!isCatCollapsed(cat.id)}
          onopen={() => setCatOpen(cat.id, true)}
          onclose={() => setCatOpen(cat.id, false)}
        >
          {#snippet header()}
            <span class="pe-cat-label">{cat.label}</span>
            <span class="pe-cat-count">{catOps.length}</span>
          {/snippet}
          {#snippet children()}
            <div class="pe-cat-body">
              {#each catOps as op (op.kind)}
                {@const Icon = iconFor(iconMap, op.icon)}
                <button class="pe-op" type="button"
                        use:tooltip={op.summary ?? ''}
                        style="--cat-color: {catColor(op.category ?? cat.id)};"
                        onclick={() => fire('add_step', { kind: op.kind })}>
                  <Icon size={13} />
                  <span class="pe-op-label">{op.label}</span>
                </button>
              {/each}
            </div>
          {/snippet}
        </Collapsible>
      </div>
    {/each}
  </div>
</section>
