<script lang="ts">
  /**
   * Console — engine + eval logs. Emission is gated to the titlebar threshold
   * (the design's "gating all'emissione"); on top of that the toolbar offers a
   * **text search** (Ctrl+F focuses it) and **per-level filter chips** (e.g.
   * isolate warn/error). Click a warn/error line to "jump to span" (mocked).
   */
  import { Terminal, ArrowDownToLine, Search, X, Trash2 } from 'lucide-svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { groveStore, levelsAtOrAbove, LOG_LEVELS } from '../grove-store.svelte';
  import { configStore } from '../stores/config.svelte';
  import { logStore } from '../stores/engine.svelte';

  let query = $state('');
  let searchEl = $state<HTMLInputElement | null>(null);
  let bodyEl = $state<HTMLElement | null>(null);
  // Per-level view filter (on top of the emission threshold). Default all on.
  let levelOn = $state<Record<string, boolean>>({ trace: true, debug: true, info: true, warn: true, error: true });

  // Ctrl+F → focus the search field.
  $effect(() => {
    if (groveStore.findPending) { searchEl?.focus(); searchEl?.select(); groveStore.clearFind(); }
  });

  const emitted = $derived(levelsAtOrAbove(configStore.logThreshold));
  const visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return logStore.lines.filter(l =>
      emitted.has(l.level) && (levelOn[l.level] ?? true) &&
      (!q || l.message.toLowerCase().includes(q)),
    );
  });

  function toggleLevel(l: string) { levelOn = { ...levelOn, [l]: !levelOn[l] }; }
  function clearSearch() { query = ''; searchEl?.focus(); }
  function scrollToBottom() { if (bodyEl) bodyEl.scrollTop = bodyEl.scrollHeight; }
</script>

<div class="con">
  <div class="con-head">
    <Terminal size={13} />
    <span class="con-title">Console</span>
    <span class="con-meta">{visible.length}/{logStore.count}</span>
    <span class="con-spacer"></span>
    <button class="con-btn" onclick={scrollToBottom} use:tooltip={'Scroll to bottom'} aria-label="Scroll to bottom"><ArrowDownToLine size={13} /></button>
    <button class="con-btn" onclick={() => logStore.clear()} use:tooltip={'Clear console'} aria-label="Clear console"><Trash2 size={13} /></button>
  </div>

  <div class="con-toolbar">
    <div class="con-search">
      <Search size={12} />
      <input bind:this={searchEl} bind:value={query} placeholder="Search… (Ctrl+F)" spellcheck="false" />
      {#if query}<button class="con-clear" onclick={clearSearch} aria-label="Clear search"><X size={11} /></button>{/if}
    </div>
    <div class="con-levels">
      {#each LOG_LEVELS as l (l)}
        {@const gated = !emitted.has(l)}
        <button
          class="lvl-chip lvl-{l}"
          class:off={!levelOn[l]}
          class:gated
          disabled={gated}
          onclick={() => toggleLevel(l)}
          use:tooltip={gated ? `${l} — below the emission threshold` : l}
          aria-pressed={levelOn[l]}
        >{l[0].toUpperCase()}</button>
      {/each}
    </div>
  </div>

  <div class="con-body" bind:this={bodyEl}>
    {#if visible.length === 0}
      <EmptyState message={query ? `No log lines match “${query}”.` : 'No log lines for the current filter.'} />
    {:else}
      {#each visible as l (l.id)}
        {@const clickable = l.level === 'warn' || l.level === 'error'}
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <div class="con-line" class:clickable onclick={() => { /* TODO: jump to span */ }}>
          <span class="con-level lvl-{l.level}">{l.level}</span>
          <span class="con-text">{l.message}</span>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .con { display: flex; flex-direction: column; height: 100%; background: var(--bg-base); }
  .con-head {
    display: flex; align-items: center; gap: 7px;
    height: 30px; min-height: 30px; padding: 0 10px;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text-secondary);
  }
  .con-title { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.3px; }
  .con-meta { font-size: 10.5px; color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .con-spacer { flex: 1; }
  .con-btn {
    display: flex; align-items: center; justify-content: center;
    width: 22px; height: 22px; background: transparent; border: none;
    border-radius: var(--radius-sm); color: var(--text-muted); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .con-btn:hover { background: var(--bg-hover); color: var(--text-primary); }

  /* ── Toolbar: search + level chips ── */
  .con-toolbar {
    display: flex; align-items: center; gap: 8px;
    padding: 5px 10px; flex-shrink: 0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .con-search {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px;
    height: 24px; padding: 0 7px;
    background: var(--bg-input); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md); color: var(--text-muted);
  }
  .con-search:focus-within { border-color: var(--border-focus); }
  .con-search input {
    flex: 1; min-width: 0; background: transparent; border: none; outline: none;
    color: var(--text-primary); font-family: var(--font-ui-sans); font-size: 12px;
  }
  .con-search input::placeholder { color: var(--text-disabled); }
  .con-clear {
    display: flex; align-items: center; justify-content: center;
    width: 16px; height: 16px; flex-shrink: 0;
    background: transparent; border: none; border-radius: 50%;
    color: var(--text-muted); cursor: pointer;
  }
  .con-clear:hover { background: var(--bg-hover); color: var(--text-primary); }

  .con-levels { display: flex; gap: 3px; flex-shrink: 0; }
  .lvl-chip {
    width: 20px; height: 20px;
    display: flex; align-items: center; justify-content: center;
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
    background: var(--bg-input); cursor: pointer;
    font-size: 10px; font-weight: 700; font-family: var(--font-code);
    transition: opacity var(--transition-fast), background var(--transition-fast);
  }
  .lvl-chip.off { opacity: 0.32; }
  .lvl-chip.gated { opacity: 0.18; cursor: not-allowed; }
  .lvl-chip.lvl-trace { color: var(--text-disabled); }
  .lvl-chip.lvl-debug { color: var(--text-muted); }
  .lvl-chip.lvl-info  { color: var(--info); border-color: color-mix(in srgb, var(--info) 40%, var(--border-subtle)); }
  .lvl-chip.lvl-warn  { color: var(--warning); border-color: color-mix(in srgb, var(--warning) 40%, var(--border-subtle)); }
  .lvl-chip.lvl-error { color: var(--error); border-color: color-mix(in srgb, var(--error) 40%, var(--border-subtle)); }

  /* ── Body ── */
  .con-body { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 0; font-family: var(--font-code); }
  .con-line {
    display: flex; align-items: baseline; gap: 8px;
    padding: 1px 12px; font-size: 11.5px; line-height: 1.6; white-space: nowrap;
  }
  .con-line.clickable { cursor: pointer; }
  .con-line.clickable:hover { background: var(--bg-hover); }
  .con-level { width: 42px; flex-shrink: 0; font-weight: 600; text-transform: uppercase; font-size: 9.5px; }
  .con-text { color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; }

  .con-level.lvl-trace { color: var(--text-disabled); }
  .con-level.lvl-debug { color: var(--text-muted); }
  .con-level.lvl-info  { color: var(--info); }
  .con-level.lvl-warn  { color: var(--warning); }
  .con-level.lvl-error { color: var(--error); }
</style>
