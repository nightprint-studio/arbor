<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { slide } from 'svelte/transition';
  import {
    X, Search, Plus, ChevronDown, ChevronRight, Folder, FolderPlus, Download,
    RefreshCw, Loader, Pencil, Trash2, ExternalLink, Copy, MapPin, ArrowRightLeft,
    FileDown, FileUp, GripVertical, AlertTriangle, LayoutPanelLeft, ArrowDownToLine,
    CircleDot, ArrowUp, ArrowDown, Check, AlertCircle, Tag, Layers,
  } from 'lucide-svelte';
  import Modal from '../shared/Modal.svelte';
  import ModalHeader from '../shared/ModalHeader.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { animStore } from '$lib/stores/animations.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { workspacesStore } from '$lib/stores/workspaces.svelte';
  import { notificationsStore } from '$lib/stores/notifications.svelte';
  import { SCRATCH_ID, workspaceColorVar, WS_COLOR_COUNT } from '$lib/types/workspace';
  import type {
    WorkspaceDef, RepoHealth, RepoRegistryEntry, WorkspaceGroup,
    WorkspaceFetchProgressEvent, WorkspacePullProgressEvent,
    WorkspaceTagProgressEvent,
  } from '$lib/types/workspace';
  import {
    workspaceHealthScan, workspaceFetchAll, workspacePullAll,
    exportWorkspace, importWorkspacePreview,
  } from '$lib/ipc/workspace';
  import {
    startWorkspaceFetchOperation, startWorkspacePullOperation,
  } from '$lib/utils/operations-bridge';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import Contribution from '../shared/Contribution.svelte';
  import PluginIcon from '../plugins/PluginIcon.svelte';
  import CreateWorkspaceModal from './CreateWorkspaceModal.svelte';
  import ImportWorkspaceModal from './ImportWorkspaceModal.svelte';
  import GroupFormModal from './GroupFormModal.svelte';
  import WorkspaceTagAllModal from './WorkspaceTagAllModal.svelte';
  import ConfirmModal from '../shared/ConfirmModal.svelte';

  interface Props {
    onClose:  () => void;
    onCreate: () => void;
  }
  let { onClose, onCreate }: Props = $props();

  // ── Search + expansion ──────────────────────────────────────────────
  let query      = $state('');
  let expanded   = $state<Set<string>>(new Set()); // workspace ids currently expanded

  function toggleExpanded(id: string) {
    const next = new Set(expanded);
    if (next.has(id)) next.delete(id); else next.add(id);
    expanded = next;
  }

  // Workspaces start collapsed — click a row header to expand.  Keeps the
  // modal compact on first open even with many workspaces.

  // ── Health cache ────────────────────────────────────────────────────
  // Keyed by workspace id → Map<repo_id, RepoHealth>.  Loaded lazily the
  // first time a workspace is expanded, and refreshed on fetch-all.
  let health = $state<Map<string, Map<string, RepoHealth>>>(new Map());
  let loadingHealth = $state<Set<string>>(new Set());

  async function loadHealth(wsId: string) {
    if (health.has(wsId) || loadingHealth.has(wsId)) return;
    loadingHealth = new Set(loadingHealth).add(wsId);
    try {
      const rows = await workspaceHealthScan(wsId);
      const map = new Map(rows.map(r => [r.repo_id, r]));
      const next = new Map(health);
      next.set(wsId, map);
      health = next;
    } catch (e) {
      uiStore.showToast(`Health scan failed: ${e}`, 'error');
    } finally {
      const s = new Set(loadingHealth); s.delete(wsId); loadingHealth = s;
    }
  }

  // Auto-load health when a workspace expands.
  $effect(() => {
    for (const id of expanded) {
      if (!health.has(id) && !loadingHealth.has(id)) void loadHealth(id);
    }
  });

  // ── Fetch / Pull progress ───────────────────────────────────────────
  // Both fetch-all and pull-all emit per-repo progress on separate event
  // streams but share the same UI state shape.  A workspace can have at
  // most one fetch and one pull running at the same time.
  interface OpState {
    jobId:     string;
    total:     number;
    index:     number;
    ok:        number;
    failed:    number;
    conflict:  number;
    /** Used by tag-all for repos in detached HEAD; unused by fetch/pull. */
    skipped:   number;
    pending:   Set<string>;
  }
  let fetchStates = $state<Map<string, OpState>>(new Map());
  let pullStates  = $state<Map<string, OpState>>(new Map());
  let tagStates   = $state<Map<string, OpState>>(new Map());

  // Tag-all modal target.  When set, a WorkspaceTagAllModal opens for that
  // workspace.  Cleared when the modal closes (regardless of submit).
  let tagModalWs = $state<WorkspaceDef | null>(null);

  // Last pull outcome per (wsId, repoId).  Preserved after the progress row
  // closes so the user still sees which repos errored / conflicted.  Cleared
  // on the next pull start for that workspace.  `phase` values come straight
  // from the backend: 'error' | 'conflict' | 'ok'.
  interface PullOutcome { phase: 'ok' | 'error' | 'conflict'; error?: string }
  let pullOutcomes = $state<Map<string, Map<string, PullOutcome>>>(new Map());

  onMount(() => {
    const offFetchProgress = workspacesStore.onFetchProgress(ev => applyProgress(ev, fetchStates, m => fetchStates = m));
    const offFetchDone     = workspacesStore.onFetchDone(({ workspace_id }) => {
      // Refresh health after the fetch-all completes.
      const next = new Map(health); next.delete(workspace_id); health = next;
      void loadHealth(workspace_id);
      const s = new Map(fetchStates); s.delete(workspace_id); fetchStates = s;
    });
    const offPullProgress = workspacesStore.onPullProgress(ev => {
      applyProgress(ev, pullStates, m => pullStates = m);
      if (ev.phase === 'ok' || ev.phase === 'error' || ev.phase === 'conflict') {
        const byWs = new Map(pullOutcomes);
        const perRepo = new Map(byWs.get(ev.workspace_id) ?? new Map());
        perRepo.set(ev.repo_id, { phase: ev.phase as PullOutcome['phase'], error: ev.error });
        byWs.set(ev.workspace_id, perRepo);
        pullOutcomes = byWs;
      }
    });
    const offTagProgress  = workspacesStore.onTagProgress(ev => applyProgress(ev, tagStates, m => tagStates = m));
    const offTagDone      = workspacesStore.onTagDone(ev => {
      const { workspace_id, tag_name, ok, failed, skipped } = ev;
      const next = new Map(health); next.delete(workspace_id); health = next;
      void loadHealth(workspace_id);
      const s = new Map(tagStates); s.delete(workspace_id); tagStates = s;
      const ws = workspacesStore.workspaces.find(w => w.id === workspace_id);
      const wsName = ws?.name ?? 'workspace';
      if (failed === 0 && skipped === 0) {
        uiStore.showToast(`Tag "${tag_name}" — ${ok}/${ok} ok on ${wsName}`, 'success');
      } else {
        notificationsStore.add(
          `Tag "${tag_name}" su "${wsName}" completato`,
          `${ok} ok · ${skipped} skipped · ${failed} failed. ` +
          `Apri il workspace per vedere il dettaglio per repository.`,
          failed > 0 ? 'error' : 'warning',
        );
      }
    });
    const offPullDone     = workspacesStore.onPullDone(ev => {
      const { workspace_id, ok, failed, conflict } = ev;
      const next = new Map(health); next.delete(workspace_id); health = next;
      void loadHealth(workspace_id);
      const s = new Map(pullStates); s.delete(workspace_id); pullStates = s;
      // Summary so the user sees what happened even after the progress row
      // closes.  Info when all clean, warning when anything went sideways.
      const ws = workspacesStore.workspaces.find(w => w.id === workspace_id);
      const wsName = ws?.name ?? 'workspace';
      if (failed === 0 && conflict === 0) {
        uiStore.showToast(`Pull "${wsName}" — ${ok}/${ok} ok`, 'success');
      } else {
        notificationsStore.add(
          `Pull "${wsName}" completato con problemi`,
          `${ok} ok · ${conflict} conflitt${conflict === 1 ? 'o' : 'i'} · ${failed} falliti. ` +
          `Apri il workspace per vedere quali repo hanno avuto errori.`,
          conflict > 0 ? 'warning' : 'error',
        );
      }
    });
    return () => {
      offFetchProgress(); offFetchDone();
      offPullProgress(); offPullDone();
      offTagProgress();  offTagDone();
    };
  });

  function applyProgress(
    ev: WorkspaceFetchProgressEvent | WorkspacePullProgressEvent | WorkspaceTagProgressEvent,
    states: Map<string, OpState>,
    setter: (m: Map<string, OpState>) => void,
  ) {
    const s = new Map(states);
    const cur = s.get(ev.workspace_id);
    if (!cur) return;
    if (ev.phase === 'ok')        { cur.ok++; }
    if (ev.phase === 'error')     { cur.failed++; }
    if (ev.phase === 'conflict')  { cur.conflict++; }
    if (ev.phase === 'skipped')   { cur.skipped++; }
    if (ev.phase !== 'start')     { cur.pending.delete(ev.repo_id); }
    cur.index = ev.index + 1;
    s.set(ev.workspace_id, { ...cur, pending: new Set(cur.pending) });
    setter(s);
  }

  async function startFetchAll(ws: WorkspaceDef) {
    if (fetchStates.has(ws.id)) return; // already running
    try {
      const res = await workspaceFetchAll(ws.id);
      // Mirror the bulk run as a card in the OperationsOverlay so the user
      // sees progress even after closing this modal.
      startWorkspaceFetchOperation(res.job_id, ws.id);
      const pending = new Set(ws.repo_ids);
      const s = new Map(fetchStates);
      s.set(ws.id, { jobId: res.job_id, total: res.total, index: 0, ok: 0, failed: 0, conflict: 0, skipped: 0, pending });
      fetchStates = s;
    } catch (e) {
      uiStore.showToast(`Fetch failed: ${e}`, 'error');
    }
  }

  async function startPullAll(ws: WorkspaceDef) {
    if (pullStates.has(ws.id)) return;
    try {
      const res = await workspacePullAll(ws.id);
      startWorkspacePullOperation(res.job_id, ws.id);
      const pending = new Set(ws.repo_ids);
      const s = new Map(pullStates);
      s.set(ws.id, { jobId: res.job_id, total: res.total, index: 0, ok: 0, failed: 0, conflict: 0, skipped: 0, pending });
      pullStates = s;
      // Reset last outcomes so the badges show this run's result, not the prior one.
      const o = new Map(pullOutcomes); o.delete(ws.id); pullOutcomes = o;
    } catch (e) {
      uiStore.showToast(`Pull failed: ${e}`, 'error');
    }
  }

  // ── Repo row actions ────────────────────────────────────────────────
  async function openRepoTab(entry: RepoRegistryEntry, sourceWsId: string) {
    try {
      const isActive = sourceWsId === workspacesStore.activeId;
      const { openRepo } = await import('$lib/ipc/graph');
      // If cross-WS (repo isn't in current active), register as a cross-WS tab.
      const repoId = await workspacesStore.ensureRepoRegistered(
        entry.path, entry.remote_url, entry.display_name,
        isActive ? {} : { allowCrossWs: true, sourceWsId },
      );
      const { tabsStore } = await import('$lib/stores/tabs.svelte');
      // Don't duplicate — if already open, just activate.
      const existing = tabsStore.tabs.find(t => t.id === repoId);
      if (existing) { tabsStore.setActive(existing.id); onClose(); return; }
      const info = await openRepo(entry.path, repoId);
      tabsStore.addTab(info);
      onClose();
    } catch (e) {
      uiStore.showToast(`Failed to open repo: ${e}`, 'error');
    }
  }

  async function removeFromWorkspace(wsId: string, repoId: string) {
    try {
      await workspacesStore.removeRepoFrom(wsId, repoId);
      uiStore.showToast('Removed from workspace', 'info');
    } catch (e) { uiStore.showToast(`Failed: ${e}`, 'error'); }
  }

  async function copyUrl(entry: RepoRegistryEntry) {
    await copyToClipboard(entry.remote_url ?? entry.path, {
      successToast: entry.remote_url ? 'Remote URL copied' : 'Path copied',
    });
  }

  // ── Move-to-workspace popover ───────────────────────────────────────
  let movePopover = $state<{ repoId: string; fromWsId: string; anchor: DOMRect } | null>(null);

  function openMovePopover(e: MouseEvent, repoId: string, fromWsId: string) {
    e.stopPropagation();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    movePopover = { repoId, fromWsId, anchor: rect };
  }
  function closeMovePopover() { movePopover = null; }

  async function moveRepo(toWsId: string) {
    if (!movePopover) return;
    const { repoId, fromWsId } = movePopover;
    closeMovePopover();
    try {
      await workspacesStore.moveRepoBetween(fromWsId, toWsId, repoId);
      uiStore.showToast('Repo moved', 'success');
    } catch (e) { uiStore.showToast(`Failed: ${e}`, 'error'); }
  }

  // ── Inline rename (display name) ────────────────────────────────────
  let renamingRepoId = $state<string | null>(null);
  let renameValue    = $state('');
  let renameInputEl: HTMLInputElement | undefined = $state();
  $effect(() => { if (renamingRepoId) renameInputEl?.focus(); });

  function startRenameRepo(id: string, current: string) {
    renamingRepoId = id;
    renameValue    = current;
  }
  async function commitRenameRepo() {
    if (!renamingRepoId) return;
    const id = renamingRepoId;
    const v  = renameValue.trim();
    renamingRepoId = null;
    if (!v) return;
    try { await workspacesStore.renameRepo(id, v); } catch (e) { uiStore.showToast(`Rename failed: ${e}`, 'error'); }
  }

  // ── Workspace edit + delete ─────────────────────────────────────────
  let editingWsId = $state<string | null>(null);
  function startEditWs(id: string)  { editingWsId = id; }
  function stopEditWs()             { editingWsId = null; }

  // ── Confirm-delete state (replaces window.confirm) ──────────────────
  let confirmDelete = $state<
    | { kind: 'workspace'; ws: WorkspaceDef }
    | { kind: 'group';     g:  WorkspaceGroup }
    | null
  >(null);
  let confirmBusy = $state(false);

  function askDeleteWs(ws: WorkspaceDef)    { confirmDelete = { kind: 'workspace', ws }; }
  function askDeleteGroup(g: WorkspaceGroup) { confirmDelete = { kind: 'group', g }; }

  async function runConfirmDelete() {
    if (!confirmDelete) return;
    confirmBusy = true;
    try {
      if (confirmDelete.kind === 'workspace') {
        await workspacesStore.deleteWorkspace(confirmDelete.ws.id);
        uiStore.showToast(`Workspace "${confirmDelete.ws.name}" deleted`, 'info');
      } else {
        await workspacesStore.deleteGroup(confirmDelete.g.id);
        uiStore.showToast(`Group "${confirmDelete.g.name}" deleted`, 'info');
      }
      confirmDelete = null;
    } catch (e) {
      uiStore.showToast(`Failed: ${e}`, 'error');
    } finally {
      confirmBusy = false;
    }
  }

  // ── Group create / edit (opens a proper modal) ──────────────────────
  let groupFormOpen     = $state(false);
  let groupFormEditing  = $state<string | null>(null); // group id when editing
  function openCreateGroup()           { groupFormEditing = null;  groupFormOpen = true; }
  function openEditGroup(g: WorkspaceGroup) { groupFormEditing = g.id; groupFormOpen = true; }

  // ── Import / Export ─────────────────────────────────────────────────
  let importModalOpen = $state(false);
  async function exportWs(ws: WorkspaceDef) {
    try {
      const payload = await exportWorkspace(ws.id);
      const text = JSON.stringify(payload, null, 2);
      await copyToClipboard(text, { successToast: 'Workspace JSON copied to clipboard', errorToast: true });
    } catch (e) { uiStore.showToast(`Export failed: ${e}`, 'error'); }
  }

  // ── Filtering ───────────────────────────────────────────────────────
  const lowerQuery = $derived(query.trim().toLowerCase());

  function workspaceMatches(ws: WorkspaceDef): boolean {
    if (!lowerQuery) return true;
    if (ws.name.toLowerCase().includes(lowerQuery)) return true;
    // Match by member repo names too.
    for (const id of ws.repo_ids) {
      const r = workspacesStore.registryById.get(id);
      if (!r) continue;
      if (r.display_name.toLowerCase().includes(lowerQuery)) return true;
      if (r.path.toLowerCase().includes(lowerQuery)) return true;
    }
    return false;
  }

  const entries = $derived(workspacesStore.grouped.filter(e =>
    e.kind === 'group'
      ? (!lowerQuery || e.children.some(workspaceMatches))
      : workspaceMatches(e.ws)
  ));

  // ── Render helpers ──────────────────────────────────────────────────
  function repoEntry(id: string): RepoRegistryEntry | null {
    return workspacesStore.registryById.get(id) ?? null;
  }

  function repoHealth(wsId: string, repoId: string): RepoHealth | null {
    return health.get(wsId)?.get(repoId) ?? null;
  }

  // ── Keyboard navigation across rows ─────────────────────────────────
  // Arrow Up/Down cycles focus across the currently-rendered group
  // headers, workspace headers, and (when expanded) repo rows.  Space
  // already toggles group / workspace headers via their own onkeydown;
  // Enter on a repo row opens it.  Tab navigation is unchanged.
  let bodyEl: HTMLElement | undefined = $state();

  function navRows(): HTMLElement[] {
    if (!bodyEl) return [];
    return Array.from(bodyEl.querySelectorAll<HTMLElement>('[data-nav-row]'));
  }

  function moveFocus(dir: 1 | -1) {
    const rows = navRows();
    if (rows.length === 0) return;
    const active = document.activeElement as HTMLElement | null;
    const idx = active ? rows.indexOf(active) : -1;
    if (idx === -1) {
      (dir === 1 ? rows[0] : rows[rows.length - 1])?.focus();
      return;
    }
    const next = Math.max(0, Math.min(rows.length - 1, idx + dir));
    rows[next]?.focus();
  }

  function onBodyKeydown(e: KeyboardEvent) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    // Only when focus is on a nav row itself — leave action toolbars
    // and inline inputs free to handle their own arrows.
    const target = e.target as HTMLElement;
    if (!target.matches?.('[data-nav-row]')) return;
    e.preventDefault();
    moveFocus(e.key === 'ArrowDown' ? 1 : -1);
  }

  function onSearchKeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      navRows()[0]?.focus();
    }
  }

  function onRepoRowKeydown(e: KeyboardEvent, entry: RepoRegistryEntry, wsId: string) {
    if (e.key === 'Enter') {
      e.preventDefault();
      openRepoTab(entry, wsId);
    }
  }
</script>

<Modal {onClose} width="900px" height="78vh" padBody={false} ariaLabel="Repository Management">
  {#snippet header()}
    <ModalHeader {onClose}>
      <LayoutPanelLeft size={14} />
      <span class="modal-title">Repository Management</span>
    </ModalHeader>
  {/snippet}

  <div class="wm-body">
    <!-- Toolbar -->
    <div class="toolbar">
      <button class="tool-btn" onclick={onCreate}>
        <Plus size={13} /> New Workspace
      </button>
      <button class="tool-btn" onclick={openCreateGroup}>
        <FolderPlus size={13} /> New Group
      </button>
      <div class="tool-divider"></div>
      <button class="tool-btn" onclick={() => importModalOpen = true}>
        <FileDown size={13} /> Import
      </button>
    </div>

    <!-- Search -->
    <div class="search-row">
      <Search size={14} />
      <input
        type="text"
        placeholder="Search workspaces or repositories…"
        bind:value={query}
        onkeydown={onSearchKeydown}
      />
      {#if query}
        <button class="icon-btn" onclick={() => query = ''} aria-label="Clear"><X size={12} /></button>
      {/if}
    </div>

    <!-- Body -->
    <div class="body" bind:this={bodyEl} onkeydown={onBodyKeydown}>
      {#if entries.length === 0}
        <div class="empty">
          <Folder size={36} />
          <p>No workspaces match your search.</p>
        </div>
      {:else}
        {#each entries as entry (entry.kind === 'group' ? `g:${entry.group.id}` : `w:${entry.ws.id}`)}
          {#if entry.kind === 'group'}
            <div class="group-block">
              <!-- The whole header row is clickable (name + count + dot) so the
                   user doesn't have to hit the tiny chevron.  Action buttons on
                   the right stop propagation so they keep their own handlers. -->
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <div
                class="group-header"
                role="button"
                tabindex="0"
                data-nav-row
                onclick={() => workspacesStore.toggleGroupCollapsed(entry.group.id)}
                onkeydown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    workspacesStore.toggleGroupCollapsed(entry.group.id);
                  }
                }}
              >
                <span class="group-toggle" aria-hidden="true">
                  {#if entry.group.collapsed}<ChevronRight size={14} />{:else}<ChevronDown size={14} />{/if}
                </span>
                <Monogram name={entry.group.name} color={workspaceColorVar(entry.group.color_idx)} size={20} />
                <span class="group-name">{entry.group.name}</span>
                <span class="group-count">{entry.children.length} workspace{entry.children.length === 1 ? '' : 's'}</span>
                <div class="group-actions" onclick={(e) => e.stopPropagation()} role="toolbar" tabindex="-1" aria-label="Group actions">
                  <button class="icon-btn" onclick={() => openEditGroup(entry.group)} use:tooltip={'Edit group'}><Pencil size={12} /></button>
                  <button class="icon-btn" onclick={() => askDeleteGroup(entry.group)} use:tooltip={'Delete group'}><Trash2 size={12} /></button>
                </div>
              </div>
              {#if !entry.group.collapsed}
                <div class="group-children" transition:slide={{ duration: animStore.dPanel }}>
                  {#each entry.children.filter(workspaceMatches) as ws (ws.id)}
                    {@render workspaceBlock(ws, true)}
                  {/each}
                </div>
              {/if}
            </div>
          {:else}
            {@render workspaceBlock(entry.ws, false)}
          {/if}
        {/each}
      {/if}
    </div>
  </div>
</Modal>

{#snippet workspaceBlock(ws: WorkspaceDef, inGroup: boolean)}
  {@const isExpanded  = expanded.has(ws.id)}
  {@const isActive    = ws.id === workspacesStore.activeId}
  {@const fetchState  = fetchStates.get(ws.id)}
  {@const pullState   = pullStates.get(ws.id)}
  {@const tagState    = tagStates.get(ws.id)}
  {@const busy        = !!fetchState || !!pullState || !!tagState}
  {@const wsHealthMap = health.get(ws.id)}
  {@const hasConflict = wsHealthMap ? Array.from(wsHealthMap.values()).some(h => h.conflicted) : false}
  {@const hasWorktree = wsHealthMap ? Array.from(wsHealthMap.values()).some(h => h.is_worktree) : false}
  {@const wsAhead     = wsHealthMap ? Array.from(wsHealthMap.values()).reduce((s, h) => s + (h.ahead  ?? 0), 0) : 0}
  {@const wsBehind    = wsHealthMap ? Array.from(wsHealthMap.values()).reduce((s, h) => s + (h.behind ?? 0), 0) : 0}
  <!-- Dedupe defensively: the persisted repo_ids list shouldn't contain
       duplicates, but if one ever sneaks in (manual edit, race, older
       version) the keyed each below would silently drop rows.  Computing
       once here keeps the header count and the body in lock-step. -->
  {@const uniqIds     = Array.from(new Set(ws.repo_ids))}
  <div class="ws-block" class:in-group={inGroup} class:scratch={ws.id === SCRATCH_ID} class:has-conflict={hasConflict}>
    <!-- Entire header row is the click target for expand/collapse — dot,
         name, count, progress area all count.  Action buttons on the right
         live in a sibling wrapper that stops propagation. -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="ws-header"
      class:expanded={isExpanded}
      class:active={isActive}
      role="button"
      tabindex="0"
      data-nav-row
      aria-expanded={isExpanded}
      onclick={() => toggleExpanded(ws.id)}
      onkeydown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          toggleExpanded(ws.id);
        }
      }}
    >
      <span class="ws-toggle" aria-hidden="true">
        {#if isExpanded}<ChevronDown size={14} />{:else}<ChevronRight size={14} />{/if}
      </span>
      <Monogram name={ws.name} color={workspaceColorVar(ws.color_idx)} size={22} />
      <div class="ws-head-body">
        <div class="ws-head-top">
          <span class="ws-name">{ws.name}</span>
          {#if isActive}<span class="ws-active-badge">active</span>{/if}
          <span class="ws-count" use:tooltip={uniqIds.length !== ws.repo_ids.length
            ? `${ws.repo_ids.length} entries (${ws.repo_ids.length - uniqIds.length} duplicate${ws.repo_ids.length - uniqIds.length === 1 ? '' : 's'} hidden)`
            : ''}>{uniqIds.length}</span>
          {#if wsHealthMap && (wsAhead > 0 || wsBehind > 0)}
            <span class="ws-sync-agg" use:tooltip={`${wsAhead} commit in avanti · ${wsBehind} indietro (totale repo)`}>
              {#if wsAhead > 0}<span class="agg-ahead"><ArrowUp size={9} />{wsAhead}</span>{/if}
              {#if wsBehind > 0}<span class="agg-behind"><ArrowDown size={9} />{wsBehind}</span>{/if}
            </span>
          {/if}
          {#if hasConflict && !busy}
            <span class="ws-conflict-badge" use:tooltip={'One or more repos have conflicts'}>
              <AlertTriangle size={10} /> conflict
            </span>
          {/if}
          {#if hasWorktree}
            <span class="ws-worktree-badge" use:tooltip={'One or more members are linked worktrees'}>
              <Layers size={10} />
            </span>
          {/if}
        </div>
        {#if fetchState}
          <div class="ws-progress">
            <Loader size={10} class="spin" />
            <span>Fetch {fetchState.index}/{fetchState.total} · {fetchState.ok} ok, {fetchState.failed} failed</span>
          </div>
        {/if}
        {#if pullState}
          <div class="ws-progress">
            <Loader size={10} class="spin" />
            <span>
              Pull {pullState.index}/{pullState.total} · {pullState.ok} ok,
              {pullState.conflict} conflict, {pullState.failed} failed
            </span>
          </div>
        {/if}
        {#if tagState}
          <div class="ws-progress">
            <Loader size={10} class="spin" />
            <span>
              Tag {tagState.index}/{tagState.total} · {tagState.ok} ok,
              {tagState.skipped} skipped, {tagState.failed} failed
            </span>
          </div>
        {/if}
      </div>
      <div class="ws-actions" onclick={(e) => e.stopPropagation()} onkeydown={null} role="toolbar" tabindex="-1" aria-label="Workspace actions">
        <button
          class="icon-btn"
          onclick={() => startFetchAll(ws)}
          disabled={busy || uniqIds.length === 0}
          use:tooltip={fetchState ? 'Fetch in progress…' : 'Fetch all'}
        >
          {#if fetchState}<Loader size={12} class="spin" />{:else}<RefreshCw size={12} />{/if}
        </button>
        <button
          class="icon-btn"
          onclick={() => startPullAll(ws)}
          disabled={busy || uniqIds.length === 0}
          use:tooltip={pullState ? 'Pull in progress…' : 'Pull all'}
        >
          {#if pullState}<Loader size={12} class="spin" />{:else}<ArrowDownToLine size={12} />{/if}
        </button>
        <button
          class="icon-btn"
          onclick={() => tagModalWs = ws}
          disabled={busy || uniqIds.length === 0}
          use:tooltip={tagState ? 'Tag in progress…' : 'Tag all (release)'}
        >
          {#if tagState}<Loader size={12} class="spin" />{:else}<Tag size={12} />{/if}
        </button>
        <button class="icon-btn" onclick={() => startEditWs(ws.id)} use:tooltip={'Edit workspace'} disabled={ws.id === SCRATCH_ID}><Pencil size={12} /></button>
        <button class="icon-btn" onclick={() => exportWs(ws)} use:tooltip={'Export as JSON'}><FileUp size={12} /></button>
        <button class="icon-btn" onclick={() => askDeleteWs(ws)} disabled={ws.id === SCRATCH_ID} use:tooltip={'Delete workspace'}><Trash2 size={12} /></button>
        <Contribution point="arbor:workspace-row">
          {#snippet item({ payload, fire })}
            {@const p = payload as { icon?: string; label?: string; tooltip?: string; color?: string }}
            <button
              class="icon-btn"
              use:tooltip={p.tooltip ?? p.label ?? ''}
              style={p.color ? `color:${p.color}` : undefined}
              onclick={() => fire({
                workspace_id: ws.id,
                workspace_name: ws.name,
                repo_count: uniqIds.length,
              })}
            >
              {#if p.icon}<PluginIcon name={p.icon} size={12} />{/if}
              {#if p.label && !p.icon}<span>{p.label}</span>{/if}
            </button>
          {/snippet}
        </Contribution>
      </div>
    </div>

    {#if isExpanded}
      <div class="ws-body" transition:slide={{ duration: animStore.dPanel }}>
        {#if uniqIds.length === 0}
          <div class="ws-empty">No repositories in this workspace.</div>
        {:else}
          {@const wsOutcomes = pullOutcomes.get(ws.id)}
          {#each uniqIds as repoId (repoId)}
            {@const entry   = repoEntry(repoId)}
            {@const hp      = repoHealth(ws.id, repoId)}
            {@const pending = fetchState?.pending.has(repoId) || pullState?.pending.has(repoId)}
            {@const outcome = wsOutcomes?.get(repoId)}
            {#if entry}
              <div
                class="repo-row"
                class:missing={hp?.missing}
                class:conflicted={hp?.conflicted}
                data-nav-row
                tabindex="-1"
                onkeydown={(e) => onRepoRowKeydown(e, entry, ws.id)}
              >
                {#if pending}
                  <Loader size={10} class="spin" />
                {:else if hp?.missing}
                  <AlertTriangle size={12} class="repo-icon missing-icon" />
                {:else if hp?.conflicted}
                  <AlertTriangle size={12} class="repo-icon conflict-icon" />
                {:else if hp?.detached}
                  <CircleDot size={12} class="repo-icon detached-icon" />
                {:else if hp?.dirty}
                  <span class="dirty-dot" use:tooltip={'Uncommitted changes'}></span>
                {:else if hp?.is_worktree}
                  <Layers size={12} class="repo-icon worktree-icon" />
                {:else}
                  <Folder size={12} class="repo-icon" />
                {/if}

                {#if renamingRepoId === entry.id}
                  <input
                    class="repo-rename-input"
                    bind:value={renameValue}
                    onblur={commitRenameRepo}
                    onkeydown={(e) => { if (e.key === 'Enter') commitRenameRepo(); if (e.key === 'Escape') { renamingRepoId = null; } }}
                    bind:this={renameInputEl}
                  />
                {:else}
                  <button class="repo-name-btn" ondblclick={() => startRenameRepo(entry.id, entry.display_name)} onclick={() => openRepoTab(entry, ws.id)} use:tooltip={{ content: 'Open', description: 'Double-click to rename' }}>
                    {entry.display_name}
                  </button>
                {/if}

                {#if hp?.branch}
                  <span
                    class="repo-branch"
                    class:detached={hp.detached}
                    use:tooltip={hp.detached ? { content: 'Detached HEAD', description: 'Not on a branch, pull is disabled' } : hp.branch}
                  >{hp.detached ? 'detached' : hp.branch}</span>
                {/if}
                {#if hp?.is_worktree}
                  <span class="repo-worktree-pill" use:tooltip={'Linked worktree'}>
                    <Layers size={10} />
                  </span>
                {/if}
                {#if hp && !hp.detached && hp.branch}
                  {#if hp.has_upstream}
                    {#if hp.ahead > 0 || hp.behind > 0}
                      <span class="repo-sync-pill" use:tooltip={`${hp.ahead} in avanti · ${hp.behind} indietro rispetto all'upstream`}>
                        {#if hp.ahead > 0}
                          <span class="sync-ahead"><ArrowUp size={10} />{hp.ahead}</span>
                        {/if}
                        {#if hp.behind > 0}
                          <span class="sync-behind"><ArrowDown size={10} />{hp.behind}</span>
                        {/if}
                      </span>
                    {:else}
                      <span class="repo-sync-pill synced" use:tooltip={'In sync con l\'upstream'}>
                        <Check size={10} />
                      </span>
                    {/if}
                  {:else}
                    <span class="repo-sync-pill no-upstream" use:tooltip={'Nessun upstream tracking'}>
                      —
                    </span>
                  {/if}
                {/if}

                {#if outcome && outcome.phase !== 'ok'}
                  <span
                    class="repo-pull-outcome"
                    class:outcome-error={outcome.phase === 'error'}
                    class:outcome-conflict={outcome.phase === 'conflict'}
                    use:tooltip={outcome.error ?? (outcome.phase === 'conflict' ? 'Pull in conflitto' : 'Errore pull')}
                  >
                    {#if outcome.phase === 'conflict'}
                      <AlertTriangle size={10} /> conflitto
                    {:else}
                      <AlertCircle size={10} /> errore pull
                    {/if}
                  </span>
                {:else if outcome?.phase === 'ok'}
                  <span class="repo-pull-outcome outcome-ok" use:tooltip={'Pull riuscito'}>
                    <Check size={10} />
                  </span>
                {/if}

                <span class="repo-path" use:tooltip={entry.path}>{entry.path}</span>

                <div class="repo-actions">
                  <button class="icon-btn" onclick={() => openRepoTab(entry, ws.id)} use:tooltip={'Open'}><ExternalLink size={11} /></button>
                  <button class="icon-btn" onclick={(e) => openMovePopover(e, entry.id, ws.id)} use:tooltip={'Move to…'}><ArrowRightLeft size={11} /></button>
                  <button class="icon-btn" onclick={() => copyUrl(entry)} use:tooltip={'Copy URL/path'}><Copy size={11} /></button>
                  <button class="icon-btn" onclick={() => removeFromWorkspace(ws.id, entry.id)} use:tooltip={'Remove from workspace'}><Trash2 size={11} /></button>
                </div>
              </div>
            {:else}
              <!-- Orphan member: workspace references this repo_id but the
                   registry has no entry for it.  Render a placeholder so the
                   count-vs-rows contract holds and the user can clean it up. -->
              <div class="repo-row orphan" data-nav-row tabindex="-1">
                <AlertTriangle size={12} class="repo-icon missing-icon" />
                <span class="orphan-label">Unknown repository</span>
                <span class="orphan-id" use:tooltip={repoId}>{repoId.slice(0, 8)}…</span>
                <span class="repo-path orphan-hint">
                  Registry entry missing — likely a worktree that was deregistered.
                </span>
                <div class="repo-actions">
                  <button class="icon-btn" onclick={() => removeFromWorkspace(ws.id, repoId)} use:tooltip={'Remove orphan from workspace'}>
                    <Trash2 size={11} />
                  </button>
                </div>
              </div>
            {/if}
          {/each}
        {/if}
      </div>
    {/if}
  </div>
{/snippet}

<!-- Move-to-workspace popover -->
{#if movePopover}
  <button type="button" aria-label="Close popover" class="popover-backdrop" onclick={closeMovePopover}></button>
  <div class="move-popover"
       style="top: {movePopover.anchor.bottom + 4}px; left: {Math.max(6, movePopover.anchor.left - 180)}px;"
       role="menu"
  >
    <div class="popover-title">Move to workspace</div>
    {#each workspacesStore.workspaces.filter(w => w.id !== movePopover!.fromWsId) as ws (ws.id)}
      <button class="popover-item" onclick={() => moveRepo(ws.id)}>
        <Monogram name={ws.name} color={workspaceColorVar(ws.color_idx)} size={12} />
        <span>{ws.name}</span>
      </button>
    {/each}
  </div>
{/if}

<!-- Edit workspace re-uses CreateWorkspaceModal in edit mode. -->
{#if editingWsId}
  <CreateWorkspaceModal
    editWorkspaceId={editingWsId}
    onClose={stopEditWs}
  />
{/if}

<!-- Import sub-modal -->
{#if importModalOpen}
  <ImportWorkspaceModal onClose={() => importModalOpen = false} />
{/if}

<!-- Group create / edit modal -->
{#if groupFormOpen}
  <GroupFormModal
    editGroupId={groupFormEditing}
    onClose={() => groupFormOpen = false}
  />
{/if}

<!-- Tag-all modal: pre-flight warnings + split button (Create / Create & push). -->
{#if tagModalWs}
  <WorkspaceTagAllModal
    workspace={tagModalWs}
    onClose={() => tagModalWs = null}
    onStarted={(info) => {
      const ws = tagModalWs!;
      const pending = new Set(ws.repo_ids);
      const s = new Map(tagStates);
      s.set(ws.id, {
        jobId: info.jobId, total: info.total, index: 0,
        ok: 0, failed: 0, conflict: 0, skipped: 0, pending,
      });
      tagStates = s;
    }}
  />
{/if}

<!-- Delete confirmation — workspace or group -->
{#if confirmDelete}
  <ConfirmModal
    title={confirmDelete.kind === 'workspace' ? 'Delete workspace?' : 'Delete group?'}
    message={confirmDelete.kind === 'workspace'
      ? `"${confirmDelete.ws.name}" will be removed from the list.`
      : `"${confirmDelete.g.name}" will be removed from the list.`}
    detail={confirmDelete.kind === 'workspace'
      ? 'The repositories inside it stay registered in Arbor and on disk — they just lose this workspace membership.'
      : 'Member workspaces move back to the top level; no workspace is deleted.'}
    variant="danger"
    confirmLabel="Delete"
    busy={confirmBusy}
    onConfirm={runConfirmDelete}
    onCancel={() => confirmDelete = null}
  />
{/if}

<style>
  .wm-body {
    display: flex;
    flex-direction: column;
    height: 100%;
    font-family: var(--font-ui-sans);
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
  }
  .tool-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 11px;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    cursor: pointer;
    transition: background var(--transition-fast);
  }
  .tool-btn:hover { background: var(--bg-hover); }
  .tool-divider { width: 1px; height: 18px; background: var(--border); }

  .search-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-input);
    color: var(--text-muted);
  }
  .search-row input {
    flex: 1;
    background: transparent;
    border: none;
    color: var(--text-primary);
    font-size: var(--font-size-sm);
    font-family: var(--font-ui-sans);
  }
  .search-row input:focus { outline: none; }

  .body {
    flex: 1;
    /* min-height: 0 is required for a flex child with overflow to actually
       respect the parent's height — without it the body grows with its
       content and the scrollbar never appears. */
    min-height: 0;
    overflow-y: auto;
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 40px;
    color: var(--text-muted);
  }

  /* ── Group block ──────────────────────────── */
  .group-block {
    /* Card on bg-base body: bg-elevated raises the group container out of
       the body tone so the border alone doesn't have to carry the card feel. */
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 4px 8px 6px;
    margin: 2px 0;
  }
  .group-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 4px;
    color: var(--text-secondary);
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    cursor: pointer;
    outline: none;
  }
  /* No hover background — group headers keep the minimalist section-label
     look.  The cursor: pointer signals clickability; focus-visible draws a
     subtle accent underline for keyboard users without adding a band that
     clashes with the list below. */
  .group-header:focus-visible .group-name {
    box-shadow: 0 1px 0 var(--accent);
  }
  .group-toggle {
    display: inline-flex;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .group-name { font-weight: 600; }
  .group-count { opacity: 0.7; }
  .group-actions {
    margin-left: auto;
    display: flex;
    gap: 2px;
  }

  /* ── Workspace block ──────────────────────────
     No `overflow: hidden` here: it traps the slide transition's height
     animation in some browser layouts and also prevents the modal body
     from computing its scrollable height correctly.  Instead we match the
     header / body corner radii to the block's so clipping is visual, not
     structural. */
  .ws-block {
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
  }
  /* Nested inside a group (itself bg-elevated): keep the same bg-elevated
     tone as the outer group and rely on a stronger border (--border rather
     than --border-subtle) to separate the card from its container. */
  .ws-block.in-group {
    margin-left: 6px;
    border-color: var(--border);
  }

  .ws-header {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 8px 10px;
    cursor: pointer;
    transition: background var(--transition-fast);
    /* Round the same amount as the parent so a hover/active fill sits
       neatly inside the block's rounded border. */
    border-radius: calc(var(--radius-md) - 1px);
  }
  .ws-header.expanded {
    background: var(--bg-hover);
    /* When expanded, only the top corners round — the bottom edge
       butts against ws-body. */
    border-radius: calc(var(--radius-md) - 1px) calc(var(--radius-md) - 1px) 0 0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .ws-header:hover { background: var(--bg-hover); }
  .ws-header.active { background: color-mix(in srgb, var(--accent) 10%, transparent); }

  .ws-toggle {
    display: inline-flex;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .ws-header:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }

  .ws-head-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .ws-head-top {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .ws-name {
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    font-weight: 500;
  }
  .ws-active-badge {
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--accent);
    background: var(--accent-subtle);
    padding: 1px 6px;
    border-radius: var(--radius-md);
  }
  .ws-count {
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 1px 6px;
    border-radius: var(--radius-md);
    font-variant-numeric: tabular-nums;
  }
  .ws-conflict-badge {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--error);
    background: var(--error-subtle);
    padding: 1px 6px 1px 4px;
    border-radius: var(--radius-md);
  }
  /* Mini icon pill: tells the user "this workspace contains worktrees".
     No label — the icon alone is enough; tooltip carries the meaning. */
  .ws-worktree-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    color: var(--accent);
    background: var(--accent-subtle);
    border-radius: var(--radius-sm);
  }
  .ws-progress {
    font-size: 10px;
    color: var(--text-muted);
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .ws-actions { display: flex; align-items: center; gap: 2px; }

  .ws-body {
    display: flex;
    flex-direction: column;
    padding: 4px 0;
    /* Mirror the parent's border-radius on the bottom corners so the
       open block still looks clipped even though its parent no longer
       hides overflow. */
    border-radius: 0 0 calc(var(--radius-md) - 1px) calc(var(--radius-md) - 1px);
    overflow: hidden;
  }

  .ws-empty {
    padding: 14px;
    color: var(--text-muted);
    font-size: 11px;
    text-align: center;
  }

  .repo-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px 6px 34px;
    /* border-subtle disappears against bg-elevated — use --border for a
       visible but still light row separator. */
    border-bottom: 1px solid var(--border);
    transition: background var(--transition-fast);
  }
  .repo-row:last-child { border-bottom: none; }
  .repo-row:hover { background: var(--bg-hover); }
  .repo-row:focus { outline: none; }
  .repo-row:focus-visible {
    outline: none;
    background: var(--bg-hover);
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  .repo-row.missing { opacity: 0.66; }
  .repo-row.orphan { opacity: 0.85; font-style: italic; }
  .orphan-label {
    font-size: var(--font-size-sm);
    color: var(--warning);
    font-weight: 500;
  }
  .orphan-id {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: var(--radius-sm);
  }
  .orphan-hint { color: var(--text-muted); font-style: italic; }

  /* Inline pill that travels with the branch label so worktree members
     stay flagged even when they're dirty / behind / etc. */
  .repo-worktree-pill {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border-radius: var(--radius-sm);
    color: var(--accent);
    background: var(--accent-subtle);
    flex-shrink: 0;
  }
  .repo-row.conflicted { background: var(--error-subtle); }
  .repo-row.conflicted:hover {
    background: color-mix(in srgb, var(--error) 18%, transparent);
  }

  .dirty-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--warning);
    flex-shrink: 0;
  }

  .repo-name-btn {
    background: transparent;
    border: none;
    color: var(--text-primary);
    font-size: var(--font-size-sm);
    font-weight: 500;
    font-family: var(--font-ui-sans);
    cursor: pointer;
    padding: 0;
    flex-shrink: 0;
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
  }
  .repo-name-btn:hover { color: var(--accent); text-decoration: underline; }

  .repo-rename-input {
    padding: 2px 6px;
    background: var(--bg-input);
    border: 1px solid var(--accent);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-size: var(--font-size-sm);
    min-width: 140px;
  }

  .repo-branch {
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    max-width: 110px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Detached HEAD pill — stand out from a branch name without screaming
     like the conflict row.  Italicised label + muted accent border. */
  .repo-branch.detached {
    font-style: italic;
    color: var(--text-secondary);
    border: 1px dashed var(--border);
    padding: 0 5px;
    background: transparent;
  }
  /* Per-repo sync pill: always visible when on a branch so "synced" and
     "no upstream" are communicated explicitly, not by absence. */
  .repo-sync-pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    background: var(--bg-overlay);
    color: var(--text-muted);
  }
  .repo-sync-pill .sync-ahead,
  .repo-sync-pill .sync-behind {
    display: inline-flex;
    align-items: center;
    gap: 1px;
  }
  .repo-sync-pill .sync-ahead  { color: var(--accent); }
  .repo-sync-pill .sync-behind { color: var(--warning); }
  .repo-sync-pill.synced {
    color: var(--success);
    background: var(--success-subtle);
  }
  .repo-sync-pill.no-upstream {
    color: var(--text-disabled);
    background: transparent;
    border: 1px dashed var(--border);
    padding: 0 5px;
  }

  /* Last pull outcome, preserved after the progress row closes so errors /
     conflicts don't disappear silently.  Hover for the full message. */
  .repo-pull-outcome {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 10px;
    font-weight: 500;
    padding: 1px 6px 1px 4px;
    border-radius: var(--radius-md);
  }
  .repo-pull-outcome.outcome-ok {
    color: var(--success);
    background: var(--success-subtle);
    padding: 1px 5px;
  }
  .repo-pull-outcome.outcome-error {
    color: var(--error);
    background: var(--error-subtle);
  }
  .repo-pull-outcome.outcome-conflict {
    color: var(--warning);
    background: color-mix(in srgb, var(--warning) 16%, transparent);
  }

  /* Workspace-level aggregate — sum of ahead/behind across member repos.
     Visible even when the workspace is collapsed so the user gets a
     quick glance at how out-of-sync the whole group is. */
  .ws-sync-agg {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 10px;
    font-variant-numeric: tabular-nums;
  }
  .ws-sync-agg .agg-ahead,
  .ws-sync-agg .agg-behind {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    padding: 1px 5px;
    border-radius: var(--radius-md);
  }
  .ws-sync-agg .agg-ahead {
    color: var(--accent);
    background: var(--accent-subtle);
  }
  .ws-sync-agg .agg-behind {
    color: var(--warning);
    background: color-mix(in srgb, var(--warning) 14%, transparent);
  }

  .repo-path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-muted);
    font-family: var(--font-ui);
    font-size: 10px;
  }

  .repo-actions {
    display: flex;
    gap: 2px;
    opacity: 0;
    transition: opacity var(--transition-fast);
  }
  .repo-row:hover .repo-actions,
  .repo-row:focus-within .repo-actions { opacity: 1; }

  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .icon-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .icon-btn:disabled { opacity: 0.4; cursor: not-allowed; }


  .popover-backdrop {
    position: fixed;
    inset: 0;
    z-index: 599;
    background: transparent;
    border: none;
    padding: 0;
    cursor: default;
  }
  .move-popover {
    position: fixed;
    z-index: 600;
    min-width: 220px;
    max-height: 320px;
    overflow-y: auto;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 4px;
    box-shadow: var(--shadow-lg);
    font-family: var(--font-ui-sans);
  }
  .popover-title {
    padding: 6px 8px;
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .popover-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    cursor: pointer;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    text-align: left;
  }
  .popover-item:hover { background: var(--bg-hover); }
</style>
