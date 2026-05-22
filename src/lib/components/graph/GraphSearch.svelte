<script lang="ts">
  import { onMount } from 'svelte';
  import { Search, ChevronUp, ChevronDown } from 'lucide-svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { searchCommits } from '$lib/ipc/remote';
  import { tooltip } from '$lib/actions/tooltip';

  let query = $state('');
  let searching = $state(false);
  let wrapEl = $state<HTMLElement | null>(null);
  let debounceTimer: ReturnType<typeof setTimeout>;

  const matchCount = $derived(graphStore.highlightedOids.size);
  const currentIdx = $derived(graphStore.currentMatchIdx);
  const hasMatches = $derived(matchCount > 0);

  async function handleInput() {
    graphStore.setSearch(query);

    if (!query.trim() || !tabsStore.activeTab) {
      graphStore.setHighlighted([]);
      return;
    }

    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(async () => {
      searching = true;
      try {
        const results = await searchCommits(tabsStore.activeTab!.id, {
          text: query,
          include_author: true,
          limit: 500,
        });
        graphStore.setHighlighted(results.map(r => r.oid));
      } catch { /* ignore */ }
      finally { searching = false; }
    }, 220);
  }

  // When focus leaves the entire search container, close & clear after a short
  // delay (the delay lets clicks on nav buttons register first).
  function onWrapFocusOut(e: FocusEvent) {
    const related = e.relatedTarget as Node | null;
    if (related && wrapEl?.contains(related)) return; // focus stayed inside bar
    setTimeout(() => {
      uiStore.setSearchVisible(false);
      graphStore.setSearch('');
      graphStore.setHighlighted([]);
      query = '';
    }, 130);
  }

  onMount(() => {
    wrapEl?.querySelector('input')?.focus();
  });

  export function focus() {
    wrapEl?.querySelector('input')?.focus();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="search-wrap" bind:this={wrapEl} onfocusout={onWrapFocusOut}>
  <div class="search-icon-wrap">
    {#if searching}
      <span class="spinner-tiny"></span>
    {:else}
      <Search size={12} />
    {/if}
  </div>

  <input
    class="input search-input"
    placeholder="Search commits, SHA, author…"
    bind:value={query}
    oninput={handleInput}
    onkeydown={(e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        if (e.shiftKey) graphStore.prevMatch();
        else graphStore.nextMatch();
      }
    }}
    aria-label="Search commits"
    autocomplete="off"
    spellcheck="false"
  />

  {#if hasMatches}
    <span class="match-count">
      {currentIdx + 1} / {matchCount}
    </span>
    <button
      class="nav-btn"
      use:tooltip={{ content: 'Previous match', shortcut: 'Shift+Enter' }}
      onclick={() => graphStore.prevMatch()}
      tabindex="0"
    >
      <ChevronUp size={12} />
    </button>
    <button
      class="nav-btn"
      use:tooltip={{ content: 'Next match', shortcut: 'Enter' }}
      onclick={() => graphStore.nextMatch()}
      tabindex="0"
    >
      <ChevronDown size={12} />
    </button>
  {:else if query && !searching}
    <span class="no-match">No results</span>
  {/if}
</div>

<style>
  .search-wrap {
    display: flex;
    align-items: center;
    flex: 1;
    max-width: 460px;
    gap: 2px;
    position: relative;
  }

  .search-icon-wrap {
    position: absolute;
    left: 8px;
    color: var(--text-muted);
    pointer-events: none;
    display: flex;
    align-items: center;
  }

  .search-input {
    flex: 1;
    padding-left: 28px;
    padding-right: 8px;
    height: 26px;
    font-size: var(--font-size-sm);
    min-width: 0;
  }

  .match-count {
    font-size: 10px;
    font-family: var(--font-code);
    color: var(--text-muted);
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 1px 6px;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .no-match {
    font-size: 10px;
    color: var(--error);
    padding: 0 4px;
    flex-shrink: 0;
    white-space: nowrap;
  }

  .nav-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .nav-btn:hover { background: var(--bg-hover); color: var(--text-primary); }

  .spinner-tiny {
    width: 10px;
    height: 10px;
    border: 1.5px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 600ms linear infinite;
    display: block;
  }

</style>
