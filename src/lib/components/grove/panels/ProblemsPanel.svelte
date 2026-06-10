<script lang="ts">
  /**
   * Problems — parser / eval diagnostics, with a text search (Ctrl+F focuses it)
   * and severity filter chips (errors / warnings). Click to "jump to span"
   * (mocked).
   */
  import { AlertTriangle, CircleAlert, CircleCheckBig, Search, X } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { groveStore } from '../grove-store.svelte';
  import { MOCK_PROBLEMS } from '../mock/data';

  let query = $state('');
  let searchEl = $state<HTMLInputElement | null>(null);
  let showError = $state(true);
  let showWarning = $state(true);

  $effect(() => {
    if (groveStore.findPending) { searchEl?.focus(); searchEl?.select(); groveStore.clearFind(); }
  });

  const errorCount = $derived(MOCK_PROBLEMS.filter(p => p.severity === 'error').length);
  const warnCount = $derived(MOCK_PROBLEMS.filter(p => p.severity === 'warning').length);

  const visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return MOCK_PROBLEMS.filter(p =>
      (p.severity === 'error' ? showError : showWarning) &&
      (!q || p.message.toLowerCase().includes(q) || p.file.toLowerCase().includes(q)),
    );
  });

  function clear() { query = ''; searchEl?.focus(); }
</script>

<div class="prob">
  <div class="prob-head">
    <AlertTriangle size={13} />
    <span class="prob-title">Problems</span>
    <span class="prob-meta">{visible.length}/{MOCK_PROBLEMS.length}</span>
  </div>

  <div class="prob-toolbar">
    <div class="prob-search">
      <Search size={12} />
      <input bind:this={searchEl} bind:value={query} placeholder="Search… (Ctrl+F)" spellcheck="false" />
      {#if query}<button class="prob-clear" onclick={clear} aria-label="Clear search"><X size={11} /></button>{/if}
    </div>
    <div class="prob-filters">
      <button class="sev-chip sev-error" class:off={!showError} onclick={() => showError = !showError} use:tooltip={'Errors'} aria-pressed={showError}>
        <CircleAlert size={11} /> {errorCount}
      </button>
      <button class="sev-chip sev-warning" class:off={!showWarning} onclick={() => showWarning = !showWarning} use:tooltip={'Warnings'} aria-pressed={showWarning}>
        <AlertTriangle size={11} /> {warnCount}
      </button>
    </div>
  </div>

  <div class="prob-body">
    {#if MOCK_PROBLEMS.length === 0}
      <div class="prob-clear-state"><CircleCheckBig size={14} /> No problems detected</div>
    {:else if visible.length === 0}
      <div class="prob-empty">{query ? `No problems match “${query}”.` : 'No problems for the current filter.'}</div>
    {:else}
      {#each visible as p (p.id)}
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <div class="prob-line" onclick={() => { /* mock: jump to span */ }}>
          <span class="prob-icon sev-{p.severity}">
            {#if p.severity === 'error'}<CircleAlert size={13} />{:else}<AlertTriangle size={13} />{/if}
          </span>
          <span class="prob-msg">{p.message}</span>
          <span class="prob-loc">{p.file}:{p.line}:{p.col}</span>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .prob { display: flex; flex-direction: column; height: 100%; background: var(--bg-base); }
  .prob-head {
    display: flex; align-items: center; gap: 7px;
    height: 30px; min-height: 30px; padding: 0 10px;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text-secondary);
  }
  .prob-title { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.3px; }
  .prob-meta { font-size: 10.5px; color: var(--text-muted); font-variant-numeric: tabular-nums; }

  .prob-toolbar {
    display: flex; align-items: center; gap: 8px;
    padding: 5px 10px; flex-shrink: 0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .prob-search {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px;
    height: 24px; padding: 0 7px;
    background: var(--bg-input); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md); color: var(--text-muted);
  }
  .prob-search:focus-within { border-color: var(--border-focus); }
  .prob-search input {
    flex: 1; min-width: 0; background: transparent; border: none; outline: none;
    color: var(--text-primary); font-family: var(--font-ui-sans); font-size: 12px;
  }
  .prob-search input::placeholder { color: var(--text-disabled); }
  .prob-clear {
    display: flex; align-items: center; justify-content: center;
    width: 16px; height: 16px; flex-shrink: 0;
    background: transparent; border: none; border-radius: 50%;
    color: var(--text-muted); cursor: pointer;
  }
  .prob-clear:hover { background: var(--bg-hover); color: var(--text-primary); }

  .prob-filters { display: flex; gap: 4px; flex-shrink: 0; }
  .sev-chip {
    display: flex; align-items: center; gap: 4px;
    height: 24px; padding: 0 7px;
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
    background: var(--bg-input); cursor: pointer;
    font-size: 11px; font-weight: 600; font-variant-numeric: tabular-nums;
    transition: opacity var(--transition-fast);
  }
  .sev-chip.off { opacity: 0.35; }
  .sev-error { color: var(--error); border-color: color-mix(in srgb, var(--error) 40%, var(--border-subtle)); }
  .sev-warning { color: var(--warning); border-color: color-mix(in srgb, var(--warning) 40%, var(--border-subtle)); }

  .prob-body { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 0; }
  .prob-clear-state {
    display: flex; align-items: center; gap: 6px;
    padding: 14px 16px; color: var(--success); font-size: 12px;
  }
  .prob-empty { padding: 14px 16px; font-size: 11.5px; color: var(--text-muted); font-style: italic; }
  .prob-line {
    display: flex; align-items: center; gap: 8px;
    padding: 5px 12px; font-size: 12px; cursor: pointer;
    transition: background var(--transition-fast);
  }
  .prob-line:hover { background: var(--bg-hover); }
  .prob-icon { display: flex; flex-shrink: 0; }
  .sev-error { color: var(--error); }
  .sev-warning { color: var(--warning); }
  .prob-msg { flex: 1; min-width: 0; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .prob-loc { font-family: var(--font-code); font-size: 10.5px; color: var(--text-muted); flex-shrink: 0; }
</style>
