<script lang="ts">
  /**
   * BennuIndexInspectorModal — a filterable, inspectable browser of the project's
   * semantic index. Headline stat cards (types / members / jars / JDK / beans /
   * actions / relations) from `bennu_index_stats`, plus a KIND selector that swaps the
   * list below between the seven index kinds.
   *
   * The `Types` kind reads the real `bennu_class_index` (via the per-root cache) and
   * filters by BOTH simple name and fqcn. Every other kind reads the generic
   * `bennu_index_entries { root, kind }` seam (see `$lib/ipc/bennu/inspect`) and
   * degrades to a clear "not available yet / building" state until the BE lands.
   *
   * Both lists are VIRTUALIZED (a legacy project's index can be huge) — only the rows
   * in/around the viewport are in the DOM. Filtering is over pre-lowercased keys so
   * typing never re-scans the raw set. Click an openable row (a type / bean / action
   * with a file+line) to open it. Read-only.
   */
  import { Database, Box, RefreshCw, RotateCw, CircleCheckBig, Loader, ExternalLink } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuIndexStore } from '$lib/stores/bennu/index.svelte';
  import { indexStats as ipcStats, classIndex as ipcClasses } from '$lib/ipc/bennu';
  import { indexEntries as ipcEntries, type IndexKind } from '$lib/ipc/bennu/inspect';
  import type { IndexStats } from '$lib/types/bennu';

  let { onClose }: { onClose: () => void } = $props();

  // ── Unified row model across kinds ──────────────────────────────────────────
  // A primary label (name), a secondary detail (fqcn / path / owner), an optional
  // openable location, and pre-lowercased search keys computed once per source change
  // (NOT per keystroke).
  interface Row {
    primary: string;
    secondary: string;
    file: string | null;
    line: number | null;
    k1: string; // primary, lowercased
    k2: string; // secondary, lowercased
  }

  // ── Kind selector ───────────────────────────────────────────────────────────
  const KINDS: { id: IndexKind; label: string }[] = [
    { id: 'types', label: 'Types' },
    { id: 'members', label: 'Members' },
    { id: 'jars', label: 'Jars' },
    { id: 'jdk', label: 'JDK' },
    { id: 'beans', label: 'Beans' },
    { id: 'actions', label: 'Actions' },
    { id: 'relations', label: 'Relations' },
  ];
  const tabs: TabItem[] = KINDS.map((k) => ({ id: k.id, label: k.label }));
  let kind = $state<IndexKind>('types');
  const kindLabel = $derived(KINDS.find((k) => k.id === kind)?.label ?? kind);

  // ── Stats (headline cards) ──────────────────────────────────────────────────
  let stats = $state<IndexStats | null>(null);
  let statsLoading = $state(false);

  async function loadStats() {
    const root = projectStore.project?.root;
    if (!root) { stats = null; return; }
    statsLoading = true;
    try {
      stats = await ipcStats(root);
    } catch {
      stats = null;
    } finally {
      statsLoading = false;
    }
  }

  const cards = $derived(
    stats
      ? [
          { id: 'types', label: 'Types', value: stats.types },
          { id: 'members', label: 'Members', value: stats.members },
          { id: 'jars', label: 'Jars', value: stats.jar_count },
          { id: 'jdk', label: 'JDK', value: stats.jdk_version || '—' },
          { id: 'beans', label: 'Beans', value: stats.beans },
          { id: 'actions', label: 'Actions', value: stats.actions },
          { id: 'relations', label: 'Relations', value: stats.relations },
        ]
      : [],
  );

  // ── Per-kind entry source ───────────────────────────────────────────────────
  // Loaded on kind change (and refresh). `unavailable` is set when the BE endpoint
  // isn't there yet (or rejected) so the list shows the graceful state instead of an
  // empty "no entries" that reads like the index is genuinely empty.
  let rowsRaw = $state<Row[]>([]);
  let listLoading = $state(false);
  let unavailable = $state(false);

  function mapClass(c: { simple: string; fqcn: string; file: string; line: number }): Row {
    return {
      primary: c.simple, secondary: c.fqcn, file: c.file, line: c.line,
      k1: c.simple.toLowerCase(), k2: c.fqcn.toLowerCase(),
    };
  }
  function mapEntry(e: { primary: string; secondary: string; file: string | null; line: number | null }): Row {
    return {
      primary: e.primary, secondary: e.secondary, file: e.file, line: e.line,
      k1: e.primary.toLowerCase(), k2: e.secondary.toLowerCase(),
    };
  }

  // Reload the active kind's list. Guarded by a token so a slow response for a kind the
  // user has since switched away from can't clobber the current list.
  let loadToken = 0;
  async function loadList() {
    const root = projectStore.project?.root;
    if (!root) { rowsRaw = []; return; }
    const token = ++loadToken;
    const activeKind = kind;
    listLoading = true;
    unavailable = false;
    try {
      if (activeKind === 'types') {
        // A fresh scan every open (the inspector is a rarely-opened debug view) — NOT
        // the Go-to-Class per-root cache, so a transient empty cache there can't make
        // the inspector show "no types" while the class actually exists.
        const cs = await ipcClasses(root);
        if (token !== loadToken) return;
        rowsRaw = cs.map(mapClass);
      } else {
        const es = await ipcEntries(root, activeKind);
        if (token !== loadToken) return;
        rowsRaw = es.map(mapEntry);
      }
    } catch {
      if (token !== loadToken) return;
      // Endpoint absent / rejected → graceful "not available yet" state.
      rowsRaw = [];
      unavailable = true;
    } finally {
      if (token === loadToken) listLoading = false;
    }
  }

  // Refresh both stats + list.
  function refresh() {
    void loadStats();
    void loadList();
  }
  $effect(() => { void loadStats(); });
  // Re-fetch the list whenever the kind changes (reads `kind` so it re-runs on switch).
  $effect(() => { void kind; query = ''; void loadList(); });

  // ── Rebuild (invalidate + recompute the whole index) ────────────────────────
  /** Invalidate + rebuild the project index (BE `bennu_reindex`). The store drops the
   *  class cache immediately + re-arms its indexing job; the `$effect` below re-fetches
   *  this inspector's stats + list once the rebuild lands. */
  async function rebuild() {
    const root = projectStore.project?.root;
    if (!root) return;
    await bennuIndexStore.rebuild(root);
  }
  // Auto-refresh the inspector as the (re)index progresses so a Rebuild reflects live
  // without a manual Refresh. Keyed on `buildRevision` (bumped on every index-progress
  // event) rather than just `indexing`: the config graph — beans / actions / relations —
  // finishes AFTER the provider flips `indexing` false, so watching only `indexing` would
  // leave those three stale. `buildRevision` catches each phase's completion.
  let lastRevision = -1;
  $effect(() => {
    const rev = bennuIndexStore.buildRevision;
    if (rev !== lastRevision) {
      lastRevision = rev;
      if (projectStore.project) refresh();
    }
  });

  // ── Filtering (over pre-lowercased keys) ────────────────────────────────────
  let query = $state('');
  const rows = $derived.by<Row[]>(() => {
    const q = query.trim().toLowerCase();
    if (!q) return rowsRaw;
    return rowsRaw.filter((r) => r.k1.includes(q) || r.k2.includes(q));
  });

  // ── Virtualized rendering (same windowing as BennuGotoModal) ────────────────
  const ROW_H = 32; // px — fixed row height (must match the .row height in CSS)
  const OVERSCAN = 8;
  let listEl = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewportH = $state(0);

  const startIdx = $derived(Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN));
  const endIdx = $derived(Math.min(rows.length, Math.ceil((scrollTop + viewportH) / ROW_H) + OVERSCAN));
  const visibleRows = $derived(rows.slice(startIdx, endIdx));
  const padTop = $derived(startIdx * ROW_H);
  const padBottom = $derived(Math.max(0, (rows.length - endIdx) * ROW_H));

  function onListScroll(e: Event) { scrollTop = (e.currentTarget as HTMLDivElement).scrollTop; }

  // Keyboard navigation over the (virtual) list.
  let active = $state(0);
  function scrollActiveIntoView() {
    const el = listEl;
    if (!el) return;
    const top = active * ROW_H;
    if (top < el.scrollTop) el.scrollTop = top;
    else if (top + ROW_H > el.scrollTop + el.clientHeight) el.scrollTop = top + ROW_H - el.clientHeight;
  }
  // Reset highlight + scroll when the filtered set changes (depends on `rows` only, so
  // hovering a row never resets the scroll).
  $effect(() => {
    void rows;
    active = 0;
    scrollTop = 0;
    if (listEl) listEl.scrollTop = 0;
  });

  function open(r: Row | undefined) {
    if (!r || !r.file) return;
    const file = r.file;
    const line = r.line;
    onClose();
    void projectStore.openFile(file).then(() => { if (line) bennuUiStore.requestGoto(line); });
  }

  function onListKeydown(e: KeyboardEvent) {
    const n = rows.length;
    if (e.key === 'ArrowDown') { e.preventDefault(); if (n) { active = (active + 1) % n; scrollActiveIntoView(); } }
    else if (e.key === 'ArrowUp') { e.preventDefault(); if (n) { active = (active - 1 + n) % n; scrollActiveIntoView(); } }
    else if (e.key === 'Enter') { e.preventDefault(); open(rows[active]); }
  }

  const placeholder = $derived(
    kind === 'types' ? 'Filter types by name or fqcn…' : `Filter ${kindLabel.toLowerCase()}…`,
  );
</script>

<Modal {onClose} width="720px" height="600px" padBody={false} ariaLabel="Index inspector">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Database size={14} />
      <span class="modal-title">Index inspector</span>
      {#if stats}
        <span class="hdr-state" class:ready={stats.ready}>
          {#if stats.ready}<CircleCheckBig size={12} /> ready{:else}<Loader size={12} /> building…{/if}
        </span>
      {/if}
      <button
        class="hdr-rebuild"
        type="button"
        use:tooltip={'Invalidate the index and recompute it from scratch'}
        aria-label="Rebuild index"
        disabled={bennuIndexStore.indexing || !projectStore.project}
        onclick={rebuild}
      >
        <RotateCw size={12} class={bennuIndexStore.indexing ? 'spin' : ''} />
        {bennuIndexStore.indexing ? 'Rebuilding…' : 'Rebuild'}
      </button>
      <button class="hdr-refresh" type="button" use:tooltip={'Refresh'} aria-label="Refresh" onclick={refresh}>
        <RefreshCw size={13} />
      </button>
    </ModalHeader>
  {/snippet}

  <div class="body">
    {#if !projectStore.project}
      <EmptyState message="Open a project to inspect its index." />
    {:else}
      <div class="stats">
        {#each cards as c (c.label)}
          <button
            class="stat"
            type="button"
            class:sel={kind === c.id}
            onclick={() => (kind = c.id as IndexKind)}
            aria-pressed={kind === c.id}
          >
            <span class="s-val">{c.value}</span>
            <span class="s-label">{c.label}</span>
          </button>
        {/each}
      </div>

      <div class="kinds">
        <Tabs
          items={tabs}
          value={kind}
          variant="pill"
          size="sm"
          fill
          ariaLabel="Index kind"
          onSelect={(id) => (kind = id as IndexKind)}
        />
      </div>

      <div class="search"><Input bind:value={query} placeholder={placeholder} /></div>

      {#if listLoading && rowsRaw.length === 0}
        <div class="state"><Spinner size={13} /> {kind === 'types' && bennuIndexStore.indexing ? 'Indexing project…' : `Loading ${kindLabel.toLowerCase()}…`}</div>
      {:else if unavailable}
        <div class="unavail">
          {#if stats && !stats.ready}
            <Loader size={20} class="unavail-icon" />
          {:else}
            <Database size={20} class="unavail-icon" />
          {/if}
          <div class="unavail-title">{kindLabel} not available yet</div>
          <div class="unavail-hint">
            {stats && !stats.ready
              ? 'The index is still building — try refreshing shortly.'
              : "This index kind isn't wired to the backend yet."}
          </div>
        </div>
      {:else if rows.length === 0}
        <div class="state muted">
          {query ? `No ${kindLabel.toLowerCase()} match “${query}”.` : `No ${kindLabel.toLowerCase()} indexed.`}
        </div>
      {:else}
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <div
          class="list"
          role="listbox"
          tabindex="-1"
          aria-label={`${kindLabel} entries`}
          bind:this={listEl}
          bind:clientHeight={viewportH}
          onscroll={onListScroll}
          onkeydown={onListKeydown}
        >
          <div style="height:{padTop}px" aria-hidden="true"></div>
          {#each visibleRows as r, i (startIdx + i)}
            {@const gi = startIdx + i}
            {@const openable = !!r.file}
            <button
              class="row"
              class:active={gi === active}
              class:openable
              type="button"
              role="option"
              aria-selected={gi === active}
              style="height:{ROW_H}px"
              disabled={!openable}
              onmousemove={() => (active = gi)}
              onclick={() => open(r)}
              title={r.file ?? r.secondary}
            >
              <Box size={12} />
              <span class="r-primary">{r.primary}</span>
              <span class="r-secondary">{r.secondary}</span>
              {#if openable}<ExternalLink class="r-go" size={11} />{/if}
            </button>
          {/each}
          <div style="height:{padBottom}px" aria-hidden="true"></div>
        </div>
        <div class="foot">{rows.length.toLocaleString()} {rows.length === 1 ? kindLabel.replace(/s$/, '').toLowerCase() : kindLabel.toLowerCase()}{query ? ' matching' : ''}</div>
      {/if}
    {/if}
  </div>
</Modal>

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .hdr-state { display: inline-flex; align-items: center; gap: 4px; font-size: var(--font-size-2xs); color: var(--text-muted); }
  .hdr-state.ready { color: var(--success); }
  .hdr-rebuild {
    display: inline-flex; align-items: center; gap: 5px; margin-left: auto;
    background: transparent; border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
    color: var(--text-secondary); cursor: pointer; padding: 3px 9px;
    font-family: var(--font-ui-sans); font-size: var(--font-size-xs); font-weight: 500;
    transition: border-color var(--transition-fast), background var(--transition-fast), color var(--transition-fast);
  }
  .hdr-rebuild:hover:not(:disabled) { color: var(--text-primary); background: var(--bg-hover); border-color: var(--border-default); }
  .hdr-rebuild:disabled { opacity: 0.6; cursor: default; }
  .hdr-rebuild :global(svg) { color: var(--text-muted); }
  .hdr-rebuild :global(svg.spin) { animation: hdr-spin 0.9s linear infinite; }
  @keyframes hdr-spin { to { transform: rotate(360deg); } }

  .hdr-refresh { display: inline-flex; margin-left: 4px; background: transparent; border: none; color: var(--text-muted); cursor: pointer; padding: 2px; border-radius: var(--radius-sm); }
  .hdr-refresh:hover { color: var(--text-primary); background: var(--bg-hover); }

  .body { display: flex; flex-direction: column; height: 100%; min-height: 0; }

  /* Stat cards double as a coarse kind selector — clicking one activates its kind. */
  .stats { display: grid; grid-template-columns: repeat(7, 1fr); gap: 6px; padding: 14px 16px 10px; flex-shrink: 0; }
  .stat {
    display: flex; flex-direction: column; align-items: center; gap: 2px; padding: 8px 4px;
    background: var(--bg-elevated); border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
    cursor: pointer; font-family: var(--font-ui-sans);
    transition: border-color var(--transition-fast), background var(--transition-fast);
  }
  .stat:hover { border-color: var(--border-default); background: var(--bg-hover); }
  .stat.sel { border-color: var(--accent); background: var(--accent-subtle); }
  .s-val { font-size: var(--font-size-lg); font-weight: 700; color: var(--text-primary); font-variant-numeric: tabular-nums; }
  .stat.sel .s-val { color: var(--accent); }
  .s-label { font-size: var(--font-size-3xs); text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }

  .kinds { padding: 0 12px 4px; flex-shrink: 0; }
  .search { padding: 6px 16px 8px; flex-shrink: 0; }

  .state { display: flex; align-items: center; gap: 7px; padding: 14px 16px; font-size: var(--font-size-sm); color: var(--text-secondary); }
  .state.muted { color: var(--text-muted); }
  .unavail { flex: 1; min-height: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 6px; padding: 24px; text-align: center; }
  .unavail :global(.unavail-icon) { color: var(--text-muted); margin-bottom: 4px; }
  .unavail-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-secondary); }
  .unavail-hint { font-size: var(--font-size-xs); color: var(--text-muted); max-width: 340px; line-height: 1.4; }

  .list { flex: 1; min-height: 0; overflow-y: auto; padding: 2px 8px 4px; }
  .row {
    display: flex; align-items: center; gap: 8px;
    width: 100%; text-align: left; box-sizing: border-box; flex-shrink: 0;
    padding: 5px 8px; background: transparent; border: none; border-radius: var(--radius-sm);
    cursor: pointer; font-family: var(--font-ui-sans);
  }
  .row:disabled { cursor: default; }
  .row.active { background: var(--bg-selected); }
  .row:not(.openable) { opacity: 0.85; }
  .row :global(svg) { color: var(--text-muted); flex-shrink: 0; }
  .row :global(.r-go) { margin-left: auto; opacity: 0; }
  .row.openable:hover :global(.r-go), .row.openable.active :global(.r-go) { opacity: 0.7; }
  .r-primary { font-size: var(--font-size-sm); color: var(--text-primary); font-weight: 500; flex-shrink: 0; }
  .r-secondary { flex: 1; min-width: 0; font-size: var(--font-size-2xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left; }

  .foot { flex-shrink: 0; padding: 6px 16px; font-size: var(--font-size-2xs); color: var(--text-muted); border-top: 1px solid var(--border-subtle); font-variant-numeric: tabular-nums; }
</style>
