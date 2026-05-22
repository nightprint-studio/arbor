<script lang="ts">
  import {
    Filter, Trash2, Copy, Check, ArrowDownToLine, Search, X, ChevronDown,
  } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import LogStream from '$lib/components/shared/ui/LogStream.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { pluginLogsStore, NON_PIPELINE_SENTINEL } from '$lib/stores/pluginLogs.svelte';
  import type { PluginLogEntry, PluginLogLevel } from '$lib/types/plugin-logs';
  import { renderStructuredLogLine, formatLogTime, shortRunId } from '$lib/utils/log-highlight';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { tooltip } from '$lib/actions/tooltip';

  const LEVELS: PluginLogLevel[] = ['debug', 'info', 'warn', 'error'];

  // ── Derived view ───────────────────────────────────────────────────────────
  const filtered = $derived(pluginLogsStore.filtered);

  // Per-entry HTML render is the hot path: regex-heavy, runs once per
  // visible row on every filter / search change at scale (5000-entry cap).
  // Cache by `seq` — entries are immutable once the backend assigns one,
  // so re-rendering the same seq is wasteful. The cache is pruned
  // opportunistically when its size drifts past the buffer cap, so it
  // can't outgrow the underlying ring buffer.
  const RENDER_CACHE_MAX = 6000;
  const htmlCache = new Map<number, string>();
  function pruneCacheIfNeeded() {
    if (htmlCache.size <= RENDER_CACHE_MAX) return;
    // Drop oldest 25% in one pass — Map preserves insertion order, so
    // older seqs come out first.
    const drop = Math.floor(RENDER_CACHE_MAX / 4);
    const it = htmlCache.keys();
    for (let i = 0; i < drop; i++) {
      const k = it.next().value;
      if (k === undefined) break;
      htmlCache.delete(k);
    }
  }

  // Plain-text formatter — only used by Copy. Computed on demand inside
  // `copyOutput()` instead of a derived array, so we don't pay 5000×
  // string formatting on every filter / search keystroke when the user
  // never clicks Copy.
  function formatLine(e: PluginLogEntry): string {
    const lvl = e.level.toUpperCase().padEnd(5);
    const run = e.run_id ? `  #${shortRunId(e.run_id)}` : '';
    return `${formatLogTime(e.ts_ms)}  ${lvl}  [${e.plugin}]${run}  ${e.message}`;
  }

  // The `lines` array fed into LogStream — same length as `filtered` (so
  // the each loop iterates correctly) but holds empty strings: lineHtml
  // looks up the entry by index anyway, so the actual content comes from
  // the cache. Allocating empty refs is microseconds; the formatLine work
  // it replaces was milliseconds at the cap.
  const placeholderLines = $derived.by<string[]>(() => {
    const out: string[] = new Array(filtered.length);
    for (let i = 0; i < filtered.length; i++) out[i] = '';
    return out;
  });
  // Stable per-row keys: Svelte's `{#each}` recycles existing DOM nodes
  // for surviving rows on filter changes instead of re-rendering the
  // whole list.
  const rowKeys = $derived(filtered.map(e => e.seq));

  function lineClass(_line: string, idx: number): string | undefined {
    const lvl = filtered[idx]?.level;
    return lvl ? `pl-line-${lvl}` : undefined;
  }

  function lineHtml(_line: string, idx: number): string {
    const e = filtered[idx];
    if (!e) return '';
    let html = htmlCache.get(e.seq);
    if (html === undefined) {
      html = renderStructuredLogLine(e);
      htmlCache.set(e.seq, html);
      pruneCacheIfNeeded();
    }
    return html;
  }

  // ── Plugin filter dropdown ─────────────────────────────────────────────────
  // The store treats `pluginFilter === null` as "all plugins" (implicit).
  // Once the user toggles a single plugin we materialise the explicit set so
  // checkbox state reflects what's actually being shown.
  function isPluginActive(name: string): boolean {
    const f = pluginLogsStore.pluginFilter;
    return f === null ? true : f.has(name);
  }

  const pluginItems = $derived.by<DropdownItem[]>(() => {
    const names = pluginLogsStore.allPluginNames;
    if (names.length === 0) {
      return [
        { kind: 'item', id: '__none__', label: 'No plugins logged yet',
          disabled: true, onclick: () => {} },
      ];
    }
    const items: DropdownItem[] = [
      { kind: 'item', id: '__all__',  label: 'All plugins',
        active:   pluginLogsStore.pluginFilter === null,
        onclick: () => pluginLogsStore.selectAllPlugins() },
      { kind: 'item', id: '__none__', label: 'None',
        active:   pluginLogsStore.pluginFilter !== null && pluginLogsStore.pluginFilter.size === 0,
        onclick: () => pluginLogsStore.selectNoPlugins() },
      { kind: 'separator' },
      ...names.map(n => ({
        kind: 'item' as const,
        id:    n,
        label: n,
        active: isPluginActive(n),
        onclick: () => pluginLogsStore.togglePlugin(n),
      })),
    ];
    return items;
  });

  const pluginSummary = $derived.by(() => {
    const f = pluginLogsStore.pluginFilter;
    if (f === null) return 'All plugins';
    if (f.size === 0) return 'None';
    if (f.size === 1) return [...f][0];
    return `${f.size} plugins`;
  });

  // ── Pipeline filter dropdown ───────────────────────────────────────────────
  // Layout:
  //   • "All"                       (preset → filter = null, everything passes)
  //   • ─────
  //   • "Non-pipeline entries"      (toggle — direct `arbor.log.*` calls)
  //   • ───── Pipelines ─────
  //   • <pipeline 1>                (toggle)
  //   • <pipeline 2>                (toggle)
  //   • …
  // Picking only "Non-pipeline entries" hides every pipeline run; picking
  // a single pipeline hides everything else; mixing checkboxes shows the
  // union. The clear-by-pipeline action is a separate dropdown in the
  // header so the filter list stays focused on filtering.
  function isPipelineActive(key: string): boolean {
    const f = pluginLogsStore.pipelineFilter;
    return f === null ? true : f.has(key);
  }

  let pendingClearPipeline = $state<string | null>(null);
  function confirmClearPipeline(name: string) { pendingClearPipeline = name; }
  function performClearPipeline() {
    const n = pendingClearPipeline;
    pendingClearPipeline = null;
    if (n) pluginLogsStore.clearByPipeline(n);
  }

  // True when the buffer holds at least one entry without a pipeline tag —
  // gating the "Non-pipeline" checkbox so we don't surface a control that
  // would always select an empty subset.
  const hasNonPipelineEntries = $derived(
    pluginLogsStore.entries.some(e => !e.pipeline)
  );

  const pipelineItems = $derived.by<DropdownItem[]>(() => {
    const names = pluginLogsStore.allPipelineNames;
    if (names.length === 0 && !hasNonPipelineEntries) {
      return [
        { kind: 'item', id: '__empty__', label: 'No log entries yet',
          disabled: true, onclick: () => {} },
      ];
    }
    const items: DropdownItem[] = [
      { kind: 'item', id: '__all__', label: 'All',
        active:  pluginLogsStore.pipelineFilter === null,
        onclick: () => pluginLogsStore.selectAllPipelines() },
    ];
    if (hasNonPipelineEntries) {
      items.push(
        { kind: 'separator' },
        { kind: 'item', id: NON_PIPELINE_SENTINEL, label: 'Non-pipeline entries',
          active:  isPipelineActive(NON_PIPELINE_SENTINEL),
          onclick: () => pluginLogsStore.togglePipeline(NON_PIPELINE_SENTINEL) },
      );
    }
    if (names.length > 0) {
      items.push({ kind: 'separator', label: 'Pipelines' });
      for (const n of names) {
        items.push({
          kind:    'item',
          id:      n,
          label:   n,
          active:  isPipelineActive(n),
          onclick: () => pluginLogsStore.togglePipeline(n),
        });
      }
    }
    return items;
  });

  /** Human-readable summary used as the trigger button label. Distinguishes
   *  the three explicit-filter shapes (only-non-pipeline / only-pipelines /
   *  mixed) so the user knows what's currently active without opening the
   *  dropdown. */
  const pipelineSummary = $derived.by(() => {
    const f = pluginLogsStore.pipelineFilter;
    if (f === null) return 'All';
    if (f.size === 0) return 'No matches';
    const includesNon = f.has(NON_PIPELINE_SENTINEL);
    const pipeCount  = f.size - (includesNon ? 1 : 0);
    if (includesNon && pipeCount === 0) return 'Non-pipeline only';
    if (!includesNon && pipeCount === 1) {
      const only = [...f].find(k => k !== NON_PIPELINE_SENTINEL);
      return only ?? '1 pipeline';
    }
    if (!includesNon) return `${pipeCount} pipelines`;
    return `${pipeCount} pipeline${pipeCount === 1 ? '' : 's'} + non-pipeline`;
  });

  /** Items for the "Clear pipeline logs" dropdown button — one row per
   *  known pipeline, plus a header. Each click confirms then drops the
   *  entries tagged with that pipeline name from the buffer (backend +
   *  local mirror). Hidden when no pipelines are present.  */
  const clearPipelineItems = $derived.by<DropdownItem[]>(() => {
    const names = pluginLogsStore.allPipelineNames;
    return names.map(n => ({
      kind: 'item' as const,
      id:    n,
      label: n,
      icon:  Trash2,
      danger: true,
      onclick: () => confirmClearPipeline(n),
    }));
  });

  // ── Follow / copy state ────────────────────────────────────────────────────
  let logStream: LogStream | undefined = $state();
  let autoScroll = $state(true);

  function toggleFollow() {
    if (autoScroll) autoScroll = false;
    else            logStream?.scrollToBottom();
  }

  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  async function copyOutput() {
    if (filtered.length === 0) return;
    // Format on demand — Copy is a click-rate event, paying 5000×
    // formatLine here avoids amortising it on every keystroke / filter
    // change in the steady-state render path.
    const text = filtered.map(formatLine).join('\n');
    await copyToClipboard(text);
    copied = true;
    if (copyTimer) clearTimeout(copyTimer);
    copyTimer = setTimeout(() => { copied = false; }, 1800);
  }

  // ── Search debounce ──────────────────────────────────────────────────
  // The filter is `$derived` of the entire entries array — every keystroke
  // would re-filter ~5000 entries. Throttle to ~150ms idle so typing stays
  // responsive but we don't recompute mid-word.
  let searchInput = $state(pluginLogsStore.search);
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  function onSearchInput(value: string) {
    searchInput = value;
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => pluginLogsStore.setSearch(searchInput), 150);
  }
  function clearSearch() {
    if (searchTimer) { clearTimeout(searchTimer); searchTimer = null; }
    searchInput = '';
    pluginLogsStore.setSearch('');
  }
</script>

<div class="pl-root">
  <BottomPanelHeader title="Plugin Logs" count={filtered.length}>
    {#snippet actions()}
    <button
      class="pl-action-btn follow-btn"
      class:active={autoScroll}
      use:tooltip={autoScroll ? 'Following — click to pause' : 'Follow output'}
      onclick={toggleFollow}
    >
      <ArrowDownToLine size={13} />
      <span>Follow</span>
    </button>
    {#if filtered.length > 0}
      <button
        class="pl-action-btn"
        class:copied
        use:tooltip={'Copy visible lines'}
        onclick={copyOutput}
      >
        {#if copied}
          <Check size={13} />
          <span>Copied</span>
        {:else}
          <Copy size={13} />
          <span>Copy</span>
        {/if}
      </button>
    {/if}
    {#if pluginLogsStore.allPipelineNames.length > 0}
      <Dropdown
        items={clearPipelineItems}
        position="fixed"
        width="240px"
        emptyMessage="No pipeline logs"
      >
        {#snippet trigger({ toggle, open })}
          <button
            type="button"
            class="pl-action-btn"
            class:active={open}
            onclick={toggle}
            use:tooltip={'Clear logs from a specific pipeline'}
          >
            <Trash2 size={13} />
            <span>Clear pipeline…</span>
            <ChevronDown size={11} />
          </button>
        {/snippet}
      </Dropdown>
    {/if}
    <button
      class="pl-action-btn"
      use:tooltip={'Clear all logs'}
      onclick={() => pluginLogsStore.clear()}
      disabled={pluginLogsStore.entries.length === 0}
    >
      <Trash2 size={13} />
      <span>Clear all</span>
    </button>
    {/snippet}
  </BottomPanelHeader>

  <div class="pl-toolbar">
      <!-- Plugin multi-select -->
      <Dropdown
        items={pluginItems}
        selectionMode="multiple"
        searchable
        searchPlaceholder="Search plugins…"
        position="fixed"
        width="260px"
        emptyMessage="No plugins"
      >
        {#snippet trigger({ toggle, open })}
          <button
            type="button"
            class="pl-filter-btn"
            class:open
            onclick={toggle}
            use:tooltip={'Filter by plugin'}
          >
            <Filter size={12} />
            <span class="pl-filter-label">{pluginSummary}</span>
            <ChevronDown size={11} />
          </button>
        {/snippet}
      </Dropdown>

      <!-- Pipeline multi-select — only meaningful once at least one
           pipeline-mirrored line lands in the buffer. Hidden otherwise so
           the toolbar stays compact for users who never run pipelines. -->
      {#if pluginLogsStore.allPipelineNames.length > 0}
        <Dropdown
          items={pipelineItems}
          selectionMode="multiple"
          searchable
          searchPlaceholder="Search pipelines…"
          position="fixed"
          width="280px"
          emptyMessage="No pipelines"
        >
          {#snippet trigger({ toggle, open })}
            <button
              type="button"
              class="pl-filter-btn"
              class:open
              onclick={toggle}
              use:tooltip={'Filter by pipeline'}
            >
              <Filter size={12} />
              <span class="pl-filter-label">{pipelineSummary}</span>
              <ChevronDown size={11} />
            </button>
          {/snippet}
        </Dropdown>
      {/if}

      <!-- Level chips -->
      <div class="pl-level-group" role="group" aria-label="Log level filter">
        {#each LEVELS as lvl (lvl)}
          {@const active = pluginLogsStore.levelFilter.has(lvl)}
          <button
            type="button"
            class="pl-level pl-level-{lvl}"
            class:active
            onclick={() => pluginLogsStore.toggleLevel(lvl)}
            use:tooltip={`Toggle ${lvl}`}
          >{lvl}</button>
        {/each}
      </div>

      <!-- Search — bound to the local `searchInput` so typing stays
           responsive; the actual store update is debounced by 150ms in
           `onSearchInput`. -->
      <div class="pl-search">
        <Search size={11} />
        <input
          type="text"
          placeholder="Search messages…"
          value={searchInput}
          oninput={(e) => onSearchInput((e.target as HTMLInputElement).value)}
        />
        {#if searchInput}
          <button
            type="button"
            class="pl-search-clear"
            onclick={clearSearch}
            use:tooltip={'Clear search'}
          ><X size={11} /></button>
        {/if}
      </div>
  </div>

  <div class="pl-body">
    <LogStream
      bind:this={logStream}
      bind:autoScroll
      lines={placeholderLines}
      keys={rowKeys}
      {lineClass}
      {lineHtml}
      ansi={false}
      emptyMessage={pluginLogsStore.entries.length === 0
        ? 'No plugin logs yet. Anything written via arbor.log.* will appear here.'
        : 'No entries match the current filters.'}
    />
  </div>
</div>

{#if pendingClearPipeline}
  <ConfirmModal
    title="Clear pipeline logs"
    message={`Remove all log entries from pipeline '${pendingClearPipeline}'?`}
    variant="danger"
    confirmLabel="Clear"
    onCancel={() => pendingClearPipeline = null}
    onConfirm={performClearPipeline}
  />
{/if}

<style>
  .pl-action-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 22px;
    padding: 0 7px;
    border: none;
    background: transparent;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .pl-action-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .pl-action-btn:disabled             { opacity: 0.4; cursor: not-allowed; }
  .pl-action-btn.copied               { color: var(--success); }
  .pl-action-btn.active               { background: var(--bg-hover); color: var(--text-primary); }
  .follow-btn                          { color: var(--text-disabled); }
  .follow-btn.active                   { color: var(--accent); background: var(--accent-subtle); }
  .follow-btn.active:hover             { background: var(--accent-subtle); color: var(--accent-hover); }

  /* ── Toolbar ──────────────────────────────────────────────────────────── */
  .pl-toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    background: var(--bg-base);
    flex-shrink: 0;
  }

  .pl-filter-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 8px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .pl-filter-btn:hover, .pl-filter-btn.open {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--border);
  }
  .pl-filter-label {
    max-width: 140px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pl-level-group {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 2px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
  }
  .pl-level {
    height: 20px;
    padding: 0 8px;
    background: transparent;
    border: none;
    border-radius: 3px;
    color: var(--text-disabled);
    font-family: var(--font-ui-sans);
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .pl-level:hover                       { color: var(--text-secondary); }
  .pl-level.active                      { background: var(--bg-hover); }
  .pl-level-debug.active                { color: var(--text-muted); }
  .pl-level-info.active                 { color: var(--accent); }
  .pl-level-warn.active                 { color: var(--warning, #d8a13a); }
  .pl-level-error.active                { color: var(--error); }

  .pl-search {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 8px;
    flex: 1;
    min-width: 100px;
    max-width: 280px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
  }
  .pl-search:focus-within {
    border-color: var(--accent);
    color: var(--text-primary);
  }
  .pl-search input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: none;
    outline: none;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: 11px;
  }
  .pl-search input::placeholder { color: var(--text-disabled); }
  .pl-search-clear {
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 0;
    color: var(--text-muted);
    display: inline-flex;
    align-items: center;
  }
  .pl-search-clear:hover { color: var(--text-primary); }

  /* ── Body ─────────────────────────────────────────────────────────────── */
  .pl-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    overflow: hidden;
    background: var(--bg-base);
  }
  .pl-body {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    background: var(--bg-base);
    overflow: hidden;
  }

  /* Default line colour — message body inherits this, token spans override
     via the shared `.log-tok-*` ruleset in app.css. error/warn lines get a
     subtle background tint so the row stands out at a glance. */
  :global(.pl-body .log-line)                { color: var(--text-secondary); }
  :global(.pl-body .log-line.pl-line-debug)  { color: var(--text-muted); }
  :global(.pl-body .log-line.pl-line-warn)   { background: color-mix(in srgb, var(--warning, #d8a13a) 7%, transparent); }
  :global(.pl-body .log-line.pl-line-error)  { background: color-mix(in srgb, var(--error) 9%, transparent); }
</style>
