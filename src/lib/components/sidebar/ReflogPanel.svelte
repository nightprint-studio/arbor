<script lang="ts">
  import { fly, fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';
  import {
    History, GitCommitHorizontal, GitBranch, GitMerge, RotateCcw,
    RefreshCw, Copy, GitFork, Search, X, ChevronDown,
    ArrowUpDown, ArrowUp, ArrowDown, Filter,
    ShieldCheck, Trash2, Eye, Clock, FileWarning, AlertTriangle,
  } from 'lucide-svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { getReflog } from '$lib/ipc/reflog';
  import { checkoutCommitSafe } from '$lib/ipc/branch';
  import { handleCheckoutResult } from '$lib/utils/checkoutResultHandler';
  import {
    listRecoveryEntries, previewRecoveryRestore,
    restoreRecoveryEntry, deleteRecoveryEntry,
  } from '$lib/ipc/recovery';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import type { ReflogEntry, RecoveryEntry, RecoveryKind, RecoveryRestorePreview } from '$lib/types/git';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  // ── Types ──────────────────────────────────────────────────────────────────
  type ActionKind = 'commit' | 'checkout' | 'merge' | 'rebase' | 'other';
  type SortDir    = 'newest' | 'oldest';
  type Tab        = 'reflog' | 'recovery';

  // ── State ──────────────────────────────────────────────────────────────────
  let tab: Tab          = $state('reflog');

  // Reflog
  let entries        = $state<ReflogEntry[]>([]);
  let loading        = $state(false);
  let error          = $state<string | null>(null);
  let search         = $state('');
  let activeKinds    = $state<Set<ActionKind>>(new Set());  // empty = all
  let sortDir        = $state<SortDir>('newest');
  let pageSize       = $state(50);

  // Recovery
  let recoveryEntries = $state<RecoveryEntry[]>([]);
  let recoveryLoading = $state(false);
  let recoveryError   = $state<string | null>(null);
  let recoverySearch  = $state('');
  let previewing      = $state<number | null>(null);
  let previewData     = $state<RecoveryRestorePreview | null>(null);
  let busyId          = $state<number | null>(null); // currently restoring / deleting
  // Tracks which tab+section pair has already been fetched, so empty results
  // don't cause the lazy-load $effect to re-trigger forever.
  let reflogLoadedForTab:   string | null = $state(null);
  let recoveryLoadedForTab: string | null = $state(null);

  // Search debouncing — avoids filtering thousands of entries on every keystroke.
  let debouncedSearch         = $state('');
  let debouncedRecoverySearch = $state('');
  let searchTimer:  ReturnType<typeof setTimeout> | null = null;
  let rSearchTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const q = search;
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => { debouncedSearch = q; }, 150);
    return () => { if (searchTimer) clearTimeout(searchTimer); };
  });
  $effect(() => {
    const q = recoverySearch;
    if (rSearchTimer) clearTimeout(rSearchTimer);
    rSearchTimer = setTimeout(() => { debouncedRecoverySearch = q; }, 150);
    return () => { if (rSearchTimer) clearTimeout(rSearchTimer); };
  });

  // Dropdowns
  let kindDropOpen   = $state(false);
  let sortDropOpen   = $state(false);
  let kindAnchor     = $state<{ x: number; y: number } | null>(null);
  let sortAnchor     = $state<{ x: number; y: number } | null>(null);

  type CtxState = { x: number; y: number; entry: ReflogEntry };
  let ctxMenu = $state<CtxState | null>(null);

  const contextItems: MenuItem[] = [
    { id: 'checkout', label: 'Checkout this commit', icon: GitBranch, iconColor: 'var(--accent)' },
    { id: 'branch',   label: 'Create branch here',   icon: GitFork,   iconColor: 'var(--success)' },
    { id: 'copy',     label: 'Copy hash',            icon: Copy,      iconColor: 'var(--text-muted)' },
  ];

  // ── Load ───────────────────────────────────────────────────────────────────
  const activeTab = $derived(tabsStore.activeTab);

  // Invalidate the "loaded" flags whenever the active repo tab changes so the
  // next render triggers a fresh fetch for the new tab.
  $effect(() => {
    const t = activeTab; // track
    if (!t) {
      entries = [];
      recoveryEntries = [];
      error = null;
      recoveryError = null;
      reflogLoadedForTab   = null;
      recoveryLoadedForTab = null;
      return;
    }
    reflogLoadedForTab   = null;
    recoveryLoadedForTab = null;
  });

  // Load the data for the currently-visible inner tab, but only once per
  // (outer tab, inner tab) pair.  An empty result set would otherwise keep
  // retriggering this effect indefinitely.
  $effect(() => {
    const cur = activeTab; // track
    const it  = tab;        // track
    if (!cur) return;
    if (it === 'reflog' && reflogLoadedForTab !== cur.id && !loading) {
      reflogLoadedForTab = cur.id;
      load(cur.id);
    } else if (it === 'recovery' && recoveryLoadedForTab !== cur.id && !recoveryLoading) {
      recoveryLoadedForTab = cur.id;
      loadRecovery(cur.id);
    }
  });

  async function load(tabId: string) {
    loading  = true;
    error    = null;
    pageSize = 50;
    try {
      entries = await getReflog(tabId);
    } catch (e) {
      error = `${e}`;
    } finally {
      loading = false;
    }
  }

  async function loadRecovery(tabId: string) {
    recoveryLoading = true;
    recoveryError   = null;
    try {
      recoveryEntries = await listRecoveryEntries(tabId);
    } catch (e) {
      recoveryError = `${e}`;
    } finally {
      recoveryLoading = false;
    }
  }

  async function refresh() {
    if (!activeTab) return;
    if (tab === 'reflog') await load(activeTab.id);
    else                  await loadRecovery(activeTab.id);
  }

  // ── Dropdown helpers ───────────────────────────────────────────────────────
  function anchorOf(el: HTMLElement) {
    const r = el.getBoundingClientRect();
    return { x: r.left, y: r.bottom + 4 };
  }

  function openKindDrop(e: MouseEvent) {
    if (kindDropOpen) { kindDropOpen = false; return; }
    kindAnchor = anchorOf(e.currentTarget as HTMLElement);
    sortDropOpen = false;
    kindDropOpen = true;
  }

  function openSortDrop(e: MouseEvent) {
    if (sortDropOpen) { sortDropOpen = false; return; }
    sortAnchor = anchorOf(e.currentTarget as HTMLElement);
    kindDropOpen = false;
    sortDropOpen = true;
  }

  function toggleKind(k: ActionKind) {
    const next = new Set(activeKinds);
    if (next.has(k)) next.delete(k); else next.add(k);
    activeKinds = next;
    pageSize = 50;
  }

  function clearKinds() { activeKinds = new Set(); pageSize = 50; }

  function setSort(dir: SortDir) { sortDir = dir; sortDropOpen = false; pageSize = 50; }

  // ── Derived: reflog filtered + sorted + paged ─────────────────────────────
  const filtered = $derived((() => {
    let list = entries;

    if (activeKinds.size > 0) {
      list = list.filter(e => activeKinds.has(actionKind(e.message)));
    }

    if (debouncedSearch.trim()) {
      const q = debouncedSearch.toLowerCase();
      list = list.filter(e =>
        e.message.toLowerCase().includes(q) || e.id.startsWith(q)
      );
    }

    if (sortDir === 'oldest') list = [...list].reverse();
    return list;
  })());

  const paged    = $derived(filtered.slice(0, pageSize));
  const hasMore  = $derived(paged.length < filtered.length);

  const kindCounts = $derived<Record<ActionKind, number>>({
    commit:   entries.filter(e => actionKind(e.message) === 'commit').length,
    checkout: entries.filter(e => actionKind(e.message) === 'checkout').length,
    merge:    entries.filter(e => actionKind(e.message) === 'merge').length,
    rebase:   entries.filter(e => actionKind(e.message) === 'rebase').length,
    other:    entries.filter(e => actionKind(e.message) === 'other').length,
  });

  // ── Derived: recovery filtered ────────────────────────────────────────────
  const filteredRecovery = $derived((() => {
    const q = debouncedRecoverySearch.trim().toLowerCase();
    if (!q) return recoveryEntries;
    return recoveryEntries.filter(e =>
      e.summary.toLowerCase().includes(q)
      || e.kind.toLowerCase().includes(q)
      || (e.head_branch?.toLowerCase().includes(q) ?? false)
      || e.snapshot_oid.startsWith(q)
    );
  })());

  // ── Helpers ────────────────────────────────────────────────────────────────
  function shortHash(id: string) { return id.slice(0, 7); }
  function truncate(msg: string, max = 55) {
    return msg.length > max ? msg.slice(0, max - 1) + '…' : msg;
  }
  function relativeTime(unix: number): string {
    const diff = Math.floor(Date.now() / 1000) - unix;
    if (diff < 60)           return 'just now';
    if (diff < 3600)         return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400)        return `${Math.floor(diff / 3600)}h ago`;
    if (diff < 86400 * 30)   return `${Math.floor(diff / 86400)}d ago`;
    if (diff < 86400 * 365)  return `${Math.floor(diff / (86400 * 30))}mo ago`;
    return `${Math.floor(diff / (86400 * 365))}y ago`;
  }
  function actionKind(msg: string): ActionKind {
    const m = msg.toLowerCase();
    if (m.startsWith('commit'))   return 'commit';
    if (m.startsWith('checkout')) return 'checkout';
    if (m.startsWith('merge'))    return 'merge';
    if (m.startsWith('rebase'))   return 'rebase';
    return 'other';
  }

  const KIND_LABELS: Record<ActionKind, string> = {
    commit: 'Commit', checkout: 'Checkout', merge: 'Merge', rebase: 'Rebase', other: 'Other',
  };
  const ALL_KINDS: ActionKind[] = ['commit', 'checkout', 'merge', 'rebase', 'other'];

  const RECOVERY_LABELS: Record<RecoveryKind, string> = {
    reset_hard:        'Reset --hard',
    checkout:          'Checkout',
    discard:           'Discard',
    stash_force_apply: 'Force stash apply',
    stash_drop:        'Stash drop',
    pull:              'Pull',
    other:             'Other',
  };

  // ── Context menu ───────────────────────────────────────────────────────────
  function openCtx(e: MouseEvent, entry: ReflogEntry) {
    e.preventDefault();
    ctxMenu = { x: e.clientX, y: e.clientY, entry };
  }

  async function handleCtxSelect(id: string) {
    if (!ctxMenu || !activeTab) return;
    const { entry } = ctxMenu;
    ctxMenu = null;

    if (id === 'checkout') {
      try {
        const short = shortHash(entry.id);
        const result = await checkoutCommitSafe(activeTab.id, entry.id);
        handleCheckoutResult(result, {
          targetLabel:    short,
          successMessage: `Checked out ${short}`,
        });
        graphStore.refresh();
      } catch (e) { uiStore.showToast(`${e}`, 'error'); }
    } else if (id === 'branch') {
      window.dispatchEvent(new CustomEvent('arbor:new-branch-from', { detail: { oid: entry.id } }));
    } else if (id === 'copy') {
      await copyToClipboard(entry.id, { successToast: 'Hash copied' });
    }
  }

  // ── Recovery actions ─────────────────────────────────────────────────────
  async function togglePreview(entry: RecoveryEntry) {
    if (!activeTab) return;
    if (previewing === entry.id) { previewing = null; previewData = null; return; }
    previewing  = entry.id;
    previewData = null;
    try {
      previewData = await previewRecoveryRestore(activeTab.id, entry.id);
    } catch (e) {
      uiStore.showToast(`Preview failed: ${e}`, 'error');
      previewing = null;
    }
  }

  async function restore(entry: RecoveryEntry) {
    if (!activeTab || busyId !== null) return;
    busyId = entry.id;
    try {
      await restoreRecoveryEntry(activeTab.id, entry.id);
      uiStore.showToast(`Restored snapshot: ${entry.summary}`, 'success');
      await loadRecovery(activeTab.id);
      graphStore.refresh();
      previewing = null;
      previewData = null;
    } catch (e) {
      uiStore.showToast(`Restore failed: ${e}`, 'error');
    } finally {
      busyId = null;
    }
  }

  async function drop(entry: RecoveryEntry) {
    if (!activeTab || busyId !== null) return;
    // No confirm dialog here — the snapshot itself is a safety net, not critical data.
    busyId = entry.id;
    try {
      await deleteRecoveryEntry(activeTab.id, entry.id);
      await loadRecovery(activeTab.id);
    } catch (e) {
      uiStore.showToast(`Delete failed: ${e}`, 'error');
    } finally {
      busyId = null;
    }
  }
</script>

<!-- ── Context menu ──────────────────────────────────────────────────────── -->
{#if ctxMenu}
  <ContextMenu
    x={ctxMenu.x}
    y={ctxMenu.y}
    items={contextItems}
    onSelect={handleCtxSelect}
    onClose={() => ctxMenu = null}
  />
{/if}

<!-- ── Dropdown backdrops ─────────────────────────────────────────────────── -->
{#if kindDropOpen}
  <button type="button" aria-label="Close menu" class="drop-backdrop" onclick={() => kindDropOpen = false}></button>
{/if}
{#if sortDropOpen}
  <button type="button" aria-label="Close menu" class="drop-backdrop" onclick={() => sortDropOpen = false}></button>
{/if}

<!-- ── Kind dropdown ─────────────────────────────────────────────────────── -->
{#if kindDropOpen && kindAnchor}
  <div
    class="chip-drop"
    style="left:{kindAnchor.x}px; top:{kindAnchor.y}px"
    transition:fly={{ y: -6, duration: animStore.dFast, easing: cubicOut }}
  >
    <button class="chip-drop-item" onclick={clearKinds}>
      All types {activeKinds.size === 0 ? '✓' : ''}
    </button>
    {#each ALL_KINDS as k}
      {#if kindCounts[k] > 0}
        <button
          class="chip-drop-item"
          class:chip-drop-selected={activeKinds.has(k)}
          onclick={() => toggleKind(k)}
        >
          <span class="kind-dot kind-{k}"></span>
          {KIND_LABELS[k]}
          <span class="kind-count">{kindCounts[k]}</span>
          {#if activeKinds.has(k)}<span class="check">✓</span>{/if}
        </button>
      {/if}
    {/each}
  </div>
{/if}

<!-- ── Sort dropdown ─────────────────────────────────────────────────────── -->
{#if sortDropOpen && sortAnchor}
  <div
    class="chip-drop"
    style="left:{sortAnchor.x}px; top:{sortAnchor.y}px; min-width:140px"
    transition:fly={{ y: -6, duration: animStore.dFast, easing: cubicOut }}
  >
    <button class="chip-drop-item" class:chip-drop-selected={sortDir === 'newest'} onclick={() => setSort('newest')}>
      <ArrowDown size={11} /> Newest first {sortDir === 'newest' ? '✓' : ''}
    </button>
    <button class="chip-drop-item" class:chip-drop-selected={sortDir === 'oldest'} onclick={() => setSort('oldest')}>
      <ArrowUp size={11} /> Oldest first {sortDir === 'oldest' ? '✓' : ''}
    </button>
  </div>
{/if}

<PanelShell
  title={tab === 'reflog' ? 'Reflog' : 'Recovery'}
  count={tab === 'reflog'
    ? (!loading && entries.length > 0 ? entries.length : null)
    : (!recoveryLoading && recoveryEntries.length > 0 ? recoveryEntries.length : null)}
>
  {#snippet icon()}
    {#if tab === 'reflog'}<History size={14} />{:else}<ShieldCheck size={14} />{/if}
  {/snippet}
  {#snippet actions()}
    <button class="ps-btn" onclick={refresh} disabled={loading || recoveryLoading} use:tooltip={'Refresh'}>
      <RefreshCw size={11} class={(loading || recoveryLoading) ? 'spin' : ''} />
    </button>
  {/snippet}
  {#snippet toolbar()}
    <!-- Tab switcher -->
    <div class="tab-row">
      <Tabs
        items={[
          { id: 'reflog',   label: 'Reflog',   icon: History,     iconSize: 11 },
          {
            id: 'recovery',
            label: 'Recovery',
            icon: ShieldCheck,
            iconSize: 11,
            badge: recoveryEntries.length > 0 ? recoveryEntries.length : undefined,
          },
        ]}
        value={tab}
        variant="pill"
        size="sm"
        ariaLabel="Reflog / Recovery tabs"
        onSelect={(id) => tab = id as typeof tab}
      />
    </div>

    <!-- Search row -->
    <div class="search-row">
      <Search size={12} class="search-icon-el" />
      {#if tab === 'reflog'}
        <input
          class="search-input"
          type="text"
          placeholder="Search message or hash…"
          bind:value={search}
        />
        {#if search}
          <button class="search-clear" onclick={() => search = ''} use:tooltip={'Clear'}>
            <X size={11} />
          </button>
        {/if}
      {:else}
        <input
          class="search-input"
          type="text"
          placeholder="Search recovery snapshots…"
          bind:value={recoverySearch}
        />
        {#if recoverySearch}
          <button class="search-clear" onclick={() => recoverySearch = ''} use:tooltip={'Clear'}>
            <X size={11} />
          </button>
        {/if}
      {/if}
    </div>

    {#if tab === 'reflog'}
      <!-- Chip filters row (reflog only) -->
      <div class="chips-row">

        <button class="chip" class:chip-active={activeKinds.size > 0} onclick={openKindDrop}>
          <Filter size={10} />
          Type
          {#if activeKinds.size > 0}
            <span class="chip-badge">{activeKinds.size}</span>
          {/if}
          <ChevronDown size={9} />
        </button>

        <button class="chip" class:chip-active={sortDir !== 'newest'} onclick={openSortDrop}>
          {#if sortDir === 'newest'}
            <ArrowDown size={10} />
          {:else}
            <ArrowUp size={10} />
          {/if}
          {sortDir === 'newest' ? 'Newest' : 'Oldest'}
          <ChevronDown size={9} />
        </button>

        {#if activeKinds.size > 0 || search}
          <button class="chip chip-clear" onclick={() => { activeKinds = new Set(); search = ''; pageSize = 50; }}>
            <X size={9} /> Clear
          </button>
        {/if}

        {#if (activeKinds.size > 0 || search) && !loading}
          <span class="result-count">{filtered.length}</span>
        {/if}

      </div>
    {/if}
  {/snippet}

  <!-- ── Reflog tab body ── -->
  {#if tab === 'reflog'}
    <div class="entries-wrap">
      {#if loading}
        <div class="state-msg"><RefreshCw size={14} class="spin" /><span>Loading…</span></div>
      {:else if error}
        <div class="state-msg error-msg">{error}</div>
      {:else if filtered.length === 0}
        <EmptyState message={activeKinds.size > 0 || search ? 'No matching entries' : 'No reflog entries'} />
      {:else}
        <ul class="entries-list" role="list">
          {#each paged as entry (entry.index)}
            {@const kind = actionKind(entry.message)}
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <li class="entry-card" role="listitem" oncontextmenu={(e) => openCtx(e, entry)}>
              <div class="card-top">
                <span class="kind-badge kind-{kind}">{KIND_LABELS[kind]}</span>
                <code class="hash-chip">{shortHash(entry.id)}</code>
                <span class="card-spacer"></span>
                <span class="card-time" use:tooltip={new Date(entry.committer_time * 1000).toLocaleString()}>
                  {relativeTime(entry.committer_time)}
                </span>
              </div>
              <div class="card-msg" use:tooltip={entry.message}>{truncate(entry.message)}</div>
              <div class="card-bottom">
                <span class="ref-badge">HEAD@{'{' + entry.index + '}'}</span>
                {#if entry.committer_name}
                  <span class="card-author">{entry.committer_name}</span>
                {/if}
              </div>
            </li>
          {/each}
        </ul>

        {#if hasMore}
          <button class="load-more-btn" onclick={() => pageSize += 50}>
            Show more
            <span class="load-more-count">({filtered.length - paged.length} remaining)</span>
          </button>
        {:else if filtered.length > 50}
          <div class="list-end">All {filtered.length} entries shown</div>
        {/if}
      {/if}
    </div>

  <!-- ── Recovery tab body ── -->
  {:else}
    <div class="entries-wrap">
      <!-- Banner -->
      <div class="recovery-banner">
        <ShieldCheck size={13} />
        <span>
          Automatic snapshots taken before destructive operations (reset&nbsp;--hard,
          discard, checkout with dirty workdir, stash&nbsp;force-apply). Kept for 30&nbsp;days.
        </span>
      </div>

      {#if recoveryLoading}
        <div class="state-msg"><RefreshCw size={14} class="spin" /><span>Loading…</span></div>
      {:else if recoveryError}
        <div class="state-msg error-msg">{recoveryError}</div>
      {:else if filteredRecovery.length === 0}
        <EmptyState message={recoverySearch ? 'No matching snapshots' : 'No recovery snapshots yet'} />
      {:else}
        <ul class="entries-list" role="list">
          {#each filteredRecovery as entry (entry.id)}
            {@const logOnly = !entry.snapshot_oid}
            <li class="entry-card recovery-card" class:consumed={entry.consumed}>
              <div class="card-top">
                <span class="kind-badge recovery-kind">{RECOVERY_LABELS[entry.kind]}</span>
                <code class="hash-chip">{shortHash(entry.snapshot_oid)}</code>
                <span class="card-spacer"></span>
                <span class="card-time" use:tooltip={new Date(entry.created_at * 1000).toLocaleString()}>
                  <Clock size={10} /> {relativeTime(entry.created_at)}
                </span>
              </div>

              <div class="card-msg" use:tooltip={entry.summary}>{entry.summary}</div>

              <div class="card-bottom">
                {#if entry.head_branch}
                  <span class="ref-badge">{entry.head_branch}</span>
                {/if}
                {#if entry.head_oid}
                  <span class="card-author">HEAD was at {shortHash(entry.head_oid)}</span>
                {/if}
                {#if entry.consumed}
                  <span class="consumed-pill">restored</span>
                {/if}
                {#if entry.skipped_files && entry.skipped_files.length > 0}
                  <span class="skipped-pill" use:tooltip={{ content: `${entry.skipped_files.length} file(s) logged but not preserved`, description: 'Size/extension policy' }}>
                    <FileWarning size={9} /> {entry.skipped_files.length} not preserved
                  </span>
                {/if}
              </div>

              <!-- Actions row -->
              <div class="recovery-actions">
                {#if logOnly}
                  <span class="log-only-note" use:tooltip={{ content: 'Log-only entry', description: 'No snapshot bytes were kept — only the journal record remains' }}>
                    <FileWarning size={10} /> Log-only entry (not restorable)
                  </span>
                {:else}
                  <button
                    class="rec-btn"
                    onclick={() => togglePreview(entry)}
                    disabled={busyId === entry.id}
                    use:tooltip={'Preview files that would change'}
                  >
                    <Eye size={11} /> {previewing === entry.id ? 'Hide preview' : 'Preview'}
                  </button>
                  <button
                    class="rec-btn rec-btn-primary"
                    onclick={() => restore(entry)}
                    disabled={busyId === entry.id}
                    use:tooltip={'Apply this snapshot to the working directory'}
                  >
                    {#if busyId === entry.id}
                      <RefreshCw size={11} class="spin" /> Restoring…
                    {:else}
                      <RotateCcw size={11} /> Restore
                    {/if}
                  </button>
                {/if}
                <span class="card-spacer"></span>
                <button
                  class="rec-btn rec-btn-danger"
                  onclick={() => drop(entry)}
                  disabled={busyId === entry.id}
                  use:tooltip={'Drop this snapshot'}
                >
                  <Trash2 size={11} />
                </button>
              </div>

              <!-- Preview expansion -->
              {#if previewing === entry.id}
                <div class="preview-panel" transition:fade={{ duration: animStore.dFast }}>
                  {#if !previewData}
                    <div class="state-msg small-state"><RefreshCw size={11} class="spin" /><span>Computing preview…</span></div>
                  {:else}
                    {#if previewData.workdir_is_dirty}
                      <div class="preview-warning">
                        <AlertTriangle size={11} />
                        <span>Your workdir has unsaved changes. Restoring will merge the snapshot on top of them. A fresh safety snapshot is taken automatically.</span>
                      </div>
                    {/if}
                    {#if previewData.changed_files.length === 0}
                      <div class="preview-empty">No file differences — snapshot is identical to HEAD.</div>
                    {:else}
                      <div class="preview-header">
                        <FileWarning size={11} />
                        {previewData.changed_files.length} file{previewData.changed_files.length === 1 ? '' : 's'} will be touched:
                      </div>
                      <ul class="preview-files">
                        {#each previewData.changed_files.slice(0, 40) as path}
                          <li use:tooltip={path}>{path}</li>
                        {/each}
                        {#if previewData.changed_files.length > 40}
                          <li class="preview-more">+{previewData.changed_files.length - 40} more…</li>
                        {/if}
                      </ul>
                    {/if}

                    {#if entry.skipped_files && entry.skipped_files.length > 0}
                      <div class="skipped-section">
                        <div class="preview-header skipped-header">
                          <FileWarning size={11} />
                          {entry.skipped_files.length} file{entry.skipped_files.length === 1 ? '' : 's'} logged but NOT preserved (cannot be restored):
                        </div>
                        <ul class="preview-files">
                          {#each entry.skipped_files.slice(0, 20) as sf}
                            <li use:tooltip={sf.reason}>
                              {sf.path}
                              <span class="skipped-reason">— {sf.reason}</span>
                            </li>
                          {/each}
                          {#if entry.skipped_files.length > 20}
                            <li class="preview-more">+{entry.skipped_files.length - 20} more…</li>
                          {/if}
                        </ul>
                      </div>
                    {/if}
                  {/if}
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}

</PanelShell>

<style>
  /* ── Tab switcher ─────────────────────────────────────────────────────────
     Strip rendered by shared <Tabs variant="pill" size="sm">. */
  .tab-row {
    padding: 5px 8px 4px;
    border-bottom: 1px solid var(--border-subtle);
  }

  /* ── Toolbar (search + chips) ───────────────────────────────────────────── */
  .search-row {
    display: flex; align-items: center; gap: 6px;
    padding: 5px 10px 4px;
    border-bottom: 1px solid var(--border-subtle);
  }
  :global(.search-icon-el) { color: var(--text-muted); flex-shrink: 0; }
  .search-input {
    flex: 1; background: transparent; border: none; outline: none;
    color: var(--text-primary); font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs); min-width: 0;
  }
  .search-input::placeholder { color: var(--text-muted); }
  .search-clear {
    display: flex; align-items: center; justify-content: center;
    width: 16px; height: 16px;
    border: none; background: transparent; color: var(--text-muted);
    border-radius: 50%; cursor: pointer; flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .search-clear:hover { background: var(--bg-hover); color: var(--text-secondary); }

  /* Chips */
  .chips-row {
    display: flex; flex-wrap: wrap; align-items: center; gap: 4px;
    padding: 5px 10px;
  }

  .chip {
    display: inline-flex; align-items: center; gap: 3px;
    padding: 3px 7px;
    font-size: 10px; font-weight: 500;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    cursor: pointer;
    transition: all var(--transition-fast);
    white-space: nowrap;
    font-family: var(--font-ui-sans);
  }
  .chip:hover { border-color: var(--border); color: var(--text-secondary); }
  .chip-active {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--accent);
  }
  .chip-badge {
    background: var(--accent); color: var(--bg-base);
    border-radius: 99px; padding: 0 4px; font-size: 9px;
  }
  .chip-clear { background: transparent; border-color: transparent; }
  .chip-clear:hover { background: var(--bg-hover); border-color: var(--border-subtle); color: var(--text-secondary); }

  .result-count { font-size: 10px; color: var(--text-muted); margin-left: 2px; }

  /* ── Dropdowns ──────────────────────────────────────────────────────────── */
  .drop-backdrop { position: fixed; inset: 0; z-index: var(--z-backdrop); background: transparent; border: none; padding: 0; cursor: default; }

  .chip-drop {
    position: fixed; z-index: var(--z-tooltip);
    min-width: 180px; max-height: 280px; overflow-y: auto;
    background: var(--bg-overlay); border: 1px solid var(--border);
    border-radius: var(--radius-md); padding: 4px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.4);
    font-family: var(--font-ui-sans);
  }
  .chip-drop-item {
    display: flex; align-items: center; gap: 6px;
    width: 100%; padding: 5px 8px; text-align: left;
    font-size: 11px; font-family: var(--font-ui-sans);
    background: transparent; border: none; color: var(--text-secondary);
    border-radius: var(--radius-sm); cursor: pointer;
    transition: background var(--transition-fast);
  }
  .chip-drop-item:hover { background: var(--bg-hover); }
  .chip-drop-selected { color: var(--accent); }
  .check { margin-left: auto; }
  .kind-count { margin-left: auto; font-size: 10px; color: var(--text-muted); }

  .kind-dot {
    width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0;
  }

  /* ── Recovery banner ─────────────────────────────────────────────────────── */
  .recovery-banner {
    display: flex; gap: 6px;
    padding: 6px 10px;
    margin: 0 0 4px;
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    font-size: 10.5px; line-height: 1.45;
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
  }
  .recovery-banner :global(svg) { flex-shrink: 0; color: var(--accent); margin-top: 2px; }

  /* ── Entries list ────────────────────────────────────────────────────────── */
  .entries-wrap {
    flex: 1;
    overflow-y: auto;
    padding: 6px 8px;
    display: flex; flex-direction: column; gap: 4px;
  }
  .entries-list {
    list-style: none; margin: 0; padding: 0;
    display: flex; flex-direction: column; gap: 4px;
  }

  .entry-card {
    display: flex; flex-direction: column; gap: 3px;
    padding: 7px 10px;
    background: var(--bg-elevated);
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    cursor: context-menu;
    transition: background var(--transition-fast), border-color var(--transition-fast);
    min-width: 0;
  }
  .entry-card:hover { background: var(--bg-hover); border-color: var(--border-subtle); }
  .recovery-card { cursor: default; }
  .recovery-card.consumed { opacity: 0.6; }

  .card-top {
    display: flex; align-items: center; gap: 5px; min-width: 0;
  }
  .card-spacer { flex: 1; }
  .card-time {
    display: inline-flex; align-items: center; gap: 3px;
    font-size: 10px; color: var(--text-muted); white-space: nowrap; flex-shrink: 0;
  }
  .hash-chip {
    font-family: var(--font-code); font-size: 11px;
    color: var(--accent); white-space: nowrap; flex-shrink: 0;
  }
  .card-msg {
    font-size: var(--font-size-xs); color: var(--text-primary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; line-height: 1.4;
  }
  .card-bottom {
    display: flex; align-items: center; gap: 6px; min-width: 0;
  }
  .ref-badge {
    font-size: 10px; font-family: var(--font-code); color: var(--text-muted);
    background: transparent; border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); padding: 1px 4px; white-space: nowrap; flex-shrink: 0;
  }
  .card-author {
    font-size: 10px; color: var(--text-muted);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .consumed-pill {
    font-size: 9px; font-weight: 600; letter-spacing: 0.3px; text-transform: uppercase;
    padding: 1px 5px; border-radius: 99px;
    background: var(--accent-subtle); color: var(--accent);
  }

  /* ── Kind badge + dot colors ─────────────────────────────────────────────── */
  .kind-badge {
    font-size: 10px; font-weight: 600;
    padding: 1px 6px; border-radius: var(--radius-sm);
    white-space: nowrap; flex-shrink: 0;
  }
  .kind-commit   { color: var(--color-reflog); background: color-mix(in srgb, var(--color-reflog) 12%, transparent); }
  .kind-checkout { color: var(--color-tag);   background: color-mix(in srgb, var(--color-tag) 14%, transparent); }
  .kind-merge    { color: var(--color-stash); background: color-mix(in srgb, var(--color-stash) 14%, transparent); }
  .kind-rebase   { color: var(--success);     background: color-mix(in srgb, var(--success) 14%, transparent); }
  .kind-other    { color: var(--text-muted);  background: var(--bg-overlay); }
  .recovery-kind { color: var(--color-stash); background: color-mix(in srgb, var(--color-stash) 16%, transparent); }

  /* ── Recovery actions ──────────────────────────────────────────────────── */
  .recovery-actions {
    display: flex; align-items: center; gap: 4px;
    margin-top: 4px; padding-top: 4px;
    border-top: 1px dashed var(--border-subtle);
  }
  .rec-btn {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 3px 8px; font-size: 10.5px; font-weight: 500;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    font-family: var(--font-ui-sans);
    transition: all var(--transition-fast);
  }
  .rec-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); border-color: var(--border); }
  .rec-btn:disabled { opacity: 0.5; cursor: wait; }
  .rec-btn-primary {
    background: var(--accent-subtle);
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 50%, transparent);
  }
  .rec-btn-primary:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
  }
  .rec-btn-danger {
    color: var(--color-error, #e06c75);
  }
  .rec-btn-danger:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-error, #e06c75) 15%, transparent);
    border-color: color-mix(in srgb, var(--color-error, #e06c75) 40%, transparent);
  }

  /* ── Preview panel ─────────────────────────────────────────────────────── */
  .preview-panel {
    margin-top: 6px; padding: 6px 8px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    font-size: 10.5px;
    font-family: var(--font-ui-sans);
    color: var(--text-secondary);
  }
  .preview-header {
    display: inline-flex; align-items: center; gap: 5px;
    margin-bottom: 4px; color: var(--text-secondary);
  }
  .preview-header :global(svg) { color: var(--color-stash); }
  .preview-empty { color: var(--text-muted); font-style: italic; }
  .preview-warning {
    display: flex; gap: 5px; align-items: flex-start;
    padding: 4px 6px; margin-bottom: 6px;
    background: color-mix(in srgb, var(--color-stash) 12%, transparent);
    border-radius: var(--radius-sm);
    color: var(--color-stash);
    line-height: 1.4;
  }
  .preview-warning :global(svg) { flex-shrink: 0; margin-top: 1px; }
  .preview-files {
    list-style: none; margin: 0; padding: 0;
    max-height: 180px; overflow-y: auto;
    font-family: var(--font-code); font-size: 10px;
    color: var(--text-secondary);
  }
  .preview-files li {
    padding: 1px 4px;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .preview-files .preview-more { color: var(--text-muted); font-style: italic; }

  .skipped-section {
    margin-top: 6px;
    padding-top: 6px;
    border-top: 1px dashed var(--border-subtle);
  }
  .skipped-header { color: var(--color-stash); }
  .skipped-reason { color: var(--text-muted); font-style: italic; font-size: 9px; }
  .skipped-pill {
    display: inline-flex; align-items: center; gap: 3px;
    font-size: 9px; font-weight: 600; letter-spacing: 0.2px;
    padding: 1px 5px; border-radius: 99px;
    background: color-mix(in srgb, var(--color-stash) 14%, transparent);
    color: var(--color-stash);
    white-space: nowrap;
  }
  .log-only-note {
    display: inline-flex; align-items: center; gap: 4px;
    font-size: 10.5px; color: var(--text-muted);
    font-style: italic;
  }
  .log-only-note :global(svg) { color: var(--color-stash); }

  /* ── Load more ───────────────────────────────────────────────────────────── */
  .load-more-btn {
    display: flex; align-items: center; justify-content: center; gap: 5px;
    width: 100%; padding: 8px; margin-top: 2px;
    border: 1px dashed var(--border-subtle); border-radius: var(--radius-md);
    background: transparent; color: var(--text-secondary);
    font-family: var(--font-ui-sans); font-size: var(--font-size-xs);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .load-more-btn:hover { background: var(--bg-hover); color: var(--text-primary); border-color: var(--border); }
  .load-more-count { color: var(--text-muted); font-size: 10px; }
  .list-end { text-align: center; padding: 8px; font-size: 10px; color: var(--text-muted); }

  /* ── State messages ──────────────────────────────────────────────────────── */
  .state-msg {
    display: flex; align-items: center; justify-content: center; gap: 6px;
    padding: 28px 16px; color: var(--text-muted); font-size: var(--font-size-xs);
  }
  .small-state { padding: 8px; }
  .error-msg { color: var(--color-error, #e06c75); }

  /* ── Spinner ─────────────────────────────────────────────────────────────── */
</style>
