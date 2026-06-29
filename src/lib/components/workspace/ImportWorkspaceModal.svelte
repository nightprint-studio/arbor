<script lang="ts">
  import {
    FileDown, Check, MapPin, Download, SkipForward, AlertCircle,
    ClipboardPaste, FolderOpen, FileJson, RefreshCw, ArrowLeft,
    Layers, Link2, GitMerge, Archive, FolderGit2,
  } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { workspacesStore } from '$lib/stores/workspaces.svelte';
  import {
    importWorkspacePreview, importWorkspaceCommit, registerRepoPath, registerPendingRepo,
    importWorkspaceGroupPreview, importWorkspaceGroupCommit, importBundleCommit,
  } from '$lib/ipc/workspace';
  import { fsReadTextFile } from '$lib/ipc/fs';
  import { cloneRepo } from '$lib/ipc/graph';
  import {
    type ExportedWorkspace, type ExportedWorkspaceGroup, type ExportedBundle,
    type ExportedRepo, workspaceColorVar,
  } from '$lib/types/workspace';
  import FileExplorerModal from '../shared/FileExplorerModal.svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import StudioTextPane from '$lib/components/shared/studio/StudioTextPane.svelte';
  import type {
    StudioCompletionItem, StudioSnippetItem,
  } from '$lib/utils/studio-codemirror';

  interface Props { onClose: () => void; }
  let { onClose }: Props = $props();

  // The exported-workspace schema is fixed, so the JSON editor can offer the
  // exact set of keys (+ skeleton snippets) as completions. Type a key inside
  // a string, or `workspace` / `repo` in an empty spot, then Ctrl-Space.
  const WS_COMPLETIONS: StudioCompletionItem[] = [
    { label: 'arbor_workspace_version',       detail: 'number', type: 'property', info: 'Single-workspace export format version (currently 1).' },
    { label: 'arbor_workspace_group_version', detail: 'number', type: 'property', info: 'Group export format version (currently 1).' },
    { label: 'arbor_workspace_bundle_version', detail: 'number', type: 'property', info: 'Full-backup bundle format version (currently 1).' },
    { label: 'name',                    detail: 'string', type: 'property', info: 'Workspace / group display name.' },
    { label: 'color_idx',               detail: 'number', type: 'property', info: 'Workspace / group color index.' },
    { label: 'groups',                  detail: 'array',  type: 'property', info: 'Exported groups in a full backup bundle.' },
    { label: 'workspaces',              detail: 'array',  type: 'property', info: 'Member workspaces of a group, or top-level workspaces in a backup bundle.' },
    { label: 'repos',                   detail: 'array',  type: 'property', info: 'List of repositories in the workspace.' },
    { label: 'remote_url',              detail: 'string | null', type: 'property', info: 'Repository clone URL (null if local-only).' },
  ];

  const WS_SNIPPETS: StudioSnippetItem[] = [
    {
      label: 'workspace',
      detail: 'full export skeleton',
      type: 'keyword',
      template:
        '{\n' +
        '  "arbor_workspace_version": 1,\n' +
        '  "name": "${1:My Workspace}",\n' +
        '  "color_idx": ${2:0},\n' +
        '  "repos": [\n' +
        '    { "name": "${3:repo-name}", "remote_url": ${4:null} }\n' +
        '  ]\n' +
        '}',
    },
    {
      label: 'group',
      detail: 'group export skeleton',
      type: 'keyword',
      template:
        '{\n' +
        '  "arbor_workspace_group_version": 1,\n' +
        '  "name": "${1:My Group}",\n' +
        '  "color_idx": ${2:0},\n' +
        '  "workspaces": [\n' +
        '    {\n' +
        '      "name": "${3:Workspace}",\n' +
        '      "color_idx": ${4:0},\n' +
        '      "repos": [\n' +
        '        { "name": "${5:repo-name}", "remote_url": ${6:null} }\n' +
        '      ]\n' +
        '    }\n' +
        '  ]\n' +
        '}',
    },
    {
      label: 'repo',
      detail: 'repo entry',
      type: 'keyword',
      template: '{ "name": "${1:repo-name}", "remote_url": "${2:https://…}" }',
    },
  ];

  type RowAction = 'use-existing' | 'locate' | 'clone' | 'skip' | 'pending';
  interface RowState {
    name:        string;
    remote_url:  string | null;
    existing_id: string | null;
    locatedPath: string | null;
    cloneDest:   string;
    action:      RowAction;
    /** Repo id after it's been ingested (either from registry lookup or a
     *  freshly created clone/locate).  When set, the row is resolved. */
    resolvedId:  string | null;
  }

  type PickerTarget =
    | { kind: 'json' }
    | { kind: 'locate';     idx: number }
    | { kind: 'clone-dest'; idx: number };

  /** Member workspace inside a group bundle. `repoIndices` point into the
   *  shared, deduped `rows` array so a repo used by several workspaces is
   *  resolved exactly once. */
  interface GroupWorkspaceState {
    name:             string;
    color_idx:        number;
    repoIndices:      number[];
    existingWsId:     string | null;
  }

  let jsonText        = $state('');
  let parseError      = $state<string | null>(null);
  let mode            = $state<'single' | 'group' | 'bundle'>('single');
  let previewMeta     = $state<{ name: string; color_idx: number; existingWsId: string | null } | null>(null);
  // Group-import state (only meaningful when mode === 'group').
  let groupMeta       = $state<{ name: string; color_idx: number; existingGroupId: string | null } | null>(null);
  let groupWorkspaces = $state<GroupWorkspaceState[]>([]);
  let rows            = $state<RowState[]>([]);
  let cloneInProgress = $state<Set<number>>(new Set());
  let creating        = $state(false);
  let previewing      = $state(false);
  let picker          = $state<PickerTarget | null>(null);

  // Bundle-restore state (only meaningful when mode === 'bundle'). A full backup
  // is restored non-blocking — no per-repo resolution UI — so we keep the raw
  // payload plus a counts summary for the confirmation screen.
  let bundlePayload   = $state<ExportedBundle | null>(null);
  let bundleMeta      = $state<{ groups: number; workspaces: number; repos: number } | null>(null);

  // True once a preview (single / group / bundle) is on screen.
  const inPreview = $derived(previewMeta !== null || groupMeta !== null || bundleMeta !== null);

  // For each shared repo index, the position of the FIRST member workspace
  // that references it — that occurrence renders the full interactive row,
  // later occurrences render a compact "resolved elsewhere" mirror.
  const firstOwnerOf = $derived.by(() => {
    const m = new Map<number, number>();
    groupWorkspaces.forEach((w, wi) => {
      for (const idx of w.repoIndices) if (!m.has(idx)) m.set(idx, wi);
    });
    return m;
  });

  async function paste() {
    try { jsonText = await navigator.clipboard.readText(); } catch { /* ignore */ }
  }

  async function loadFromFile(path: string) {
    try {
      const txt = await fsReadTextFile(path);
      jsonText = txt;
      parseError = null;
      // Auto-run preview when a file is picked.
      await runPreview();
    } catch (e) {
      parseError = `Could not read file: ${e}`;
    }
  }

  // Map a backend preview repo onto an editable row.
  function toRow(r: { name: string; remote_url: string | null; existing_id: string | null; existing_path: string | null }): RowState {
    return {
      name:        r.name,
      remote_url:  r.remote_url,
      existing_id: r.existing_id,
      locatedPath: r.existing_path,
      cloneDest:   '',
      action:      r.existing_id ? 'use-existing' : (r.remote_url ? 'clone' : 'locate'),
      resolvedId:  r.existing_id,
    };
  }

  async function runPreview() {
    parseError = null;
    let parsed: unknown;
    try {
      parsed = JSON.parse(jsonText);
    } catch (e) {
      parseError = String(e);
      return;
    }
    if (!parsed || typeof parsed !== 'object') {
      parseError = 'not a valid workspace export';
      return;
    }
    const obj = parsed as Record<string, unknown>;
    // Format detection on shape:
    //  · a full backup bundle carries a top-level `groups` array (or the
    //    version tag) — check it FIRST since it also has a `workspaces` array;
    //  · a group export carries `workspaces`; a single workspace carries `repos`.
    const isBundle = typeof obj.arbor_workspace_bundle_version !== 'undefined' || Array.isArray(obj.groups);
    const isGroup  = Array.isArray(obj.workspaces) || typeof obj.arbor_workspace_group_version !== 'undefined';
    previewing = true;
    try {
      if (isBundle) {
        runBundlePreview(parsed as ExportedBundle);
      } else if (isGroup) {
        await runGroupPreview(parsed as ExportedWorkspaceGroup);
      } else if (Array.isArray(obj.repos)) {
        await runSinglePreview(parsed as ExportedWorkspace);
      } else {
        parseError = 'not a valid workspace, group or backup export';
      }
    } catch (e) {
      parseError = `Preview failed: ${e}`;
    } finally {
      previewing = false;
    }
  }

  /** A bundle restore is non-blocking, so the "preview" is just a local count
   *  summary — no backend round-trip, no per-repo resolution. */
  function runBundlePreview(payload: ExportedBundle) {
    mode = 'bundle';
    bundlePayload = payload;
    const groups     = Array.isArray(payload.groups) ? payload.groups : [];
    const standalone = Array.isArray(payload.workspaces) ? payload.workspaces : [];
    const keys = new Set<string>();
    const addRepos = (repos: ExportedRepo[] | undefined) => {
      for (const r of repos ?? []) {
        const u = (r.remote_url ?? '').trim().toLowerCase();
        keys.add(u ? `url:${u}` : `name:${(r.name ?? '').trim().toLowerCase()}`);
      }
    };
    let wsCount = standalone.length;
    for (const g of groups) for (const w of (g.workspaces ?? [])) { wsCount++; addRepos(w.repos); }
    for (const w of standalone) addRepos(w.repos);
    bundleMeta = { groups: groups.length, workspaces: wsCount, repos: keys.size };
  }

  async function runSinglePreview(payload: ExportedWorkspace) {
    const preview = await importWorkspacePreview(payload);
    mode = 'single';
    previewMeta = {
      name:         preview.name,
      color_idx:    preview.color_idx,
      existingWsId: preview.existing_workspace_id,
    };
    rows = preview.repos.map(toRow);
  }

  async function runGroupPreview(payload: ExportedWorkspaceGroup) {
    const preview = await importWorkspaceGroupPreview(payload);
    mode = 'group';
    groupMeta = {
      name:            preview.name,
      color_idx:       preview.color_idx,
      existingGroupId: preview.existing_group_id,
    };
    groupWorkspaces = preview.workspaces.map(w => ({
      name:         w.name,
      color_idx:    w.color_idx,
      repoIndices:  w.repo_indices,
      existingWsId: w.existing_workspace_id,
    }));
    rows = preview.repos.map(toRow);
  }

  function backToPaste() {
    previewMeta = null;
    groupMeta = null;
    groupWorkspaces = [];
    rows = [];
    bundlePayload = null;
    bundleMeta = null;
    mode = 'single';
  }

  function setAction(i: number, action: RowAction) {
    rows[i].action = action;
    if (action === 'skip') rows[i].resolvedId = null;
    else if (action === 'use-existing') rows[i].resolvedId = rows[i].existing_id;
    rows = [...rows];
  }

  async function applyLocate(idx: number, path: string) {
    try {
      const res = await registerRepoPath(path, rows[idx].remote_url, rows[idx].name);
      rows[idx].locatedPath = path;
      rows[idx].resolvedId  = res.id;
      rows[idx].action      = 'use-existing';
      rows = [...rows];
      await workspacesStore.reloadRegistry();
    } catch (e) {
      uiStore.showToast(`Locate failed: ${e}`, 'error');
    }
  }

  async function cloneRow(i: number) {
    const row = rows[i];
    if (!row.remote_url) { uiStore.showToast('No remote URL to clone from', 'error'); return; }
    if (!row.cloneDest)  { uiStore.showToast('Pick a destination folder first', 'warning'); return; }
    const next = new Set(cloneInProgress); next.add(i); cloneInProgress = next;
    try {
      await cloneRepo({
        url: row.remote_url,
        dest_path: row.cloneDest,
        branch: undefined,
        shallow: false,
        recurse_submodules: false,
      });
      const res = await registerRepoPath(row.cloneDest, row.remote_url, row.name);
      rows[i].locatedPath = row.cloneDest;
      rows[i].resolvedId  = res.id;
      rows[i].action      = 'use-existing';
      rows = [...rows];
      await workspacesStore.reloadRegistry();
      uiStore.showToast(`Cloned ${row.name}`, 'success');
    } catch (e) {
      uiStore.showToast(`Clone failed: ${e}`, 'error');
    } finally {
      const s = new Set(cloneInProgress); s.delete(i); cloneInProgress = s;
    }
  }

  function joinPath(base: string, name: string): string {
    if (!base) return name;
    if (!name) return base;
    const sep = base.includes('\\') ? '\\' : '/';
    return base.replace(/[\\/]+$/, '') + sep + name;
  }

  function onPickerConfirm(path: string) {
    const t = picker;
    picker = null;
    if (!t) return;
    if (t.kind === 'json') {
      void loadFromFile(path);
    } else if (t.kind === 'locate') {
      void applyLocate(t.idx, path);
    } else if (t.kind === 'clone-dest') {
      // If user picked a parent folder, append the repo name as the leaf dir
      // (matches the convention in CloneRepoModal). They can still edit it.
      const leaf = rows[t.idx].name;
      rows[t.idx].cloneDest = joinPath(path.replace(/[\\/]+$/, ''), leaf);
      rows = [...rows];
    }
  }

  const resolvedCount = $derived(rows.filter(r => r.action === 'skip' || r.resolvedId).length);
  const totalCount    = $derived(rows.length);
  const skippedCount  = $derived(rows.filter(r => r.action === 'skip').length);
  // Members that will actually land in the workspace = every non-skipped row.
  // Resolved ones reference an existing/located/cloned repo; the rest are
  // imported as "pending" entries you clone or locate later from the manager.
  const memberCount   = $derived(rows.filter(r => r.action !== 'skip').length);
  const pendingCount  = $derived(rows.filter(r => r.action !== 'skip' && !r.resolvedId).length);
  // Import is non-blocking: it's enough to keep at least one member (the rest
  // can stay unresolved and be resolved later). Only an all-skipped preview
  // has nothing to create. A group import just needs at least one member
  // workspace (which may itself end up empty — empty workspaces are valid).
  const canCreate = $derived(
    mode === 'group'
      ? groupWorkspaces.length > 0
      : previewMeta !== null && memberCount > 0,
  );

  // Resolve every (deduped) row to a repo id, minting pending entries for
  // unresolved ones. Skipped rows resolve to null. Index-aligned with `rows`.
  async function resolveRows(): Promise<(string | null)[]> {
    const out: (string | null)[] = [];
    for (const r of rows) {
      if (r.action === 'skip') { out.push(null); continue; }
      out.push(r.resolvedId ?? await registerPendingRepo(r.name, r.remote_url));
    }
    return out;
  }

  async function commit() {
    if (mode === 'bundle') { await commitBundle(); return; }
    if (mode === 'group')  { await commitGroup(); return; }
    if (!previewMeta || !canCreate) return;
    creating = true;
    try {
      // Build the member list: resolved rows reference their repo id directly;
      // unresolved (non-skipped) rows become "pending" registry entries so the
      // workspace can be created now and those repos cloned / located later.
      const repoIds: string[] = [];
      for (const r of rows) {
        if (r.action === 'skip') continue;
        if (r.resolvedId) { repoIds.push(r.resolvedId); continue; }
        repoIds.push(await registerPendingRepo(r.name, r.remote_url));
      }
      const merging = previewMeta.existingWsId !== null;
      const ws = await importWorkspaceCommit(
        previewMeta.name, previewMeta.color_idx, repoIds, null, previewMeta.existingWsId,
      );
      await workspacesStore.load();
      const pending = repoIds.length - rows.filter(r => r.action !== 'skip' && r.resolvedId).length;
      const tail = pending > 0 ? ` (${pending} to clone/locate later)` : '';
      uiStore.showToast(
        merging
          ? `Merged into "${ws.name}" — ${repoIds.length} repos${tail}`
          : `Imported workspace "${ws.name}" with ${repoIds.length} repos${tail}`,
        'success',
      );
      onClose();
    } catch (e) {
      uiStore.showToast(`Import failed: ${e}`, 'error');
    } finally {
      creating = false;
    }
  }

  async function commitBundle() {
    if (!bundlePayload) return;
    creating = true;
    try {
      const res = await importBundleCommit(bundlePayload);
      await workspacesStore.load();
      const groups = res.groups_created + res.groups_merged;
      const ws     = res.workspaces_created + res.workspaces_merged;
      const tail   = res.repos_pending > 0 ? ` · ${res.repos_pending} to clone/locate later` : '';
      uiStore.showToast(
        `Backup restored — ${groups} group${groups === 1 ? '' : 's'}, ${ws} workspace${ws === 1 ? '' : 's'}${tail}`,
        'success',
      );
      onClose();
    } catch (e) {
      uiStore.showToast(`Restore failed: ${e}`, 'error');
    } finally {
      creating = false;
    }
  }

  async function commitGroup() {
    if (!groupMeta || groupWorkspaces.length === 0) return;
    creating = true;
    try {
      const rowRepoId = await resolveRows();
      const payload = groupWorkspaces.map(w => ({
        name:       w.name,
        color_idx:  w.color_idx,
        repo_ids:   w.repoIndices
          .map(i => rowRepoId[i])
          .filter((id): id is string => id !== null),
        merge_into: w.existingWsId,
      }));
      await importWorkspaceGroupCommit(
        groupMeta.name, groupMeta.color_idx, groupMeta.existingGroupId, payload,
      );
      await workspacesStore.load();
      const merged = groupMeta.existingGroupId !== null;
      const wsCount = payload.length;
      uiStore.showToast(
        `${merged ? 'Merged into' : 'Imported'} group "${groupMeta.name}" — ${wsCount} workspace${wsCount === 1 ? '' : 's'}`,
        'success',
      );
      onClose();
    } catch (e) {
      uiStore.showToast(`Import failed: ${e}`, 'error');
    } finally {
      creating = false;
    }
  }

  </script>

<Modal
  {onClose}
  width="720px"
  ariaLabel="Import Workspace"
  closeOnBackdrop={picker === null}
>
  {#snippet header()}
    <ModalHeader {onClose}>
      {#if inPreview}
        <button class="back-btn" onclick={backToPaste} use:tooltip={'Back'} aria-label="Back">
          <ArrowLeft size={13} />
        </button>
      {/if}
      {#if mode === 'bundle' && bundleMeta}
        <Archive size={14} strokeWidth={2} />
        <span class="modal-title">Restore Backup</span>
      {:else if mode === 'group' && groupMeta}
        <Layers size={14} strokeWidth={2} />
        <span class="modal-title">Import Group</span>
      {:else}
        <FileDown size={14} strokeWidth={2} />
        <span class="modal-title">Import Workspace</span>
      {/if}
    </ModalHeader>
  {/snippet}

  <div class="iw-body" class:behind={picker !== null} class:body-paste={!inPreview} class:body-preview={inPreview}>
      {#if !inPreview}
        <p class="lead">
          Import a workspace, a whole group, or a full backup — from an
          exported JSON. Pick the file from disk or paste its contents below.
        </p>

        <div class="source-actions">
          <button class="big-action" onclick={() => picker = { kind: 'json' }}>
            <span class="big-action-icon"><FileJson size={18} /></span>
            <span class="big-action-text">
              <span class="big-action-title">Choose JSON file…</span>
              <span class="big-action-sub">Open from filesystem</span>
            </span>
          </button>
          <button class="big-action" onclick={paste}>
            <span class="big-action-icon"><ClipboardPaste size={18} /></span>
            <span class="big-action-text">
              <span class="big-action-title">Paste from clipboard</span>
              <span class="big-action-sub">Fill the editor below</span>
            </span>
          </button>
        </div>

        <FormField label="Workspace JSON" hint="Type “workspace” or “group” then Ctrl-Space for a ready-made skeleton.">
          <div class="json-editor">
            <StudioTextPane
              value={jsonText}
              language="json"
              completions={WS_COMPLETIONS}
              snippets={WS_SNIPPETS}
              oninput={(t) => { jsonText = t; if (parseError) parseError = null; }}
            />
          </div>
          {#if parseError}
            <div class="inline-error">
              <AlertCircle size={12} />
              <span>{parseError}</span>
            </div>
          {/if}
        </FormField>
      {:else if mode === 'bundle' && bundleMeta}
        <div class="preview-meta">
          <span class="bundle-icon"><Archive size={20} /></span>
          <div class="meta-text">
            <div class="meta-name"><span class="name-text">Full backup</span></div>
            <div class="meta-stats">
              <span>{bundleMeta.groups} group{bundleMeta.groups === 1 ? '' : 's'}</span>
              <span class="dot">·</span>
              <span>{bundleMeta.workspaces} workspace{bundleMeta.workspaces === 1 ? '' : 's'}</span>
              <span class="dot">·</span>
              <span>{bundleMeta.repos} repositor{bundleMeta.repos === 1 ? 'y' : 'ies'}</span>
            </div>
          </div>
        </div>
        <p class="bundle-note">
          Restoring merges into your current setup by name — nothing is duplicated, and
          re-importing the same file changes nothing. Repositories Arbor already knows are
          linked automatically; the rest are added as <strong>not cloned</strong> for you to
          clone or locate later from Repository Management.
        </p>
        {#if bundlePayload}
          <div class="bundle-tree">
            {#each bundlePayload.groups ?? [] as g, gi (gi)}
              <div class="bundle-line group">
                <Layers size={13} />
                <span class="bl-name">{g.name}</span>
                <span class="bl-count">{(g.workspaces ?? []).length} ws</span>
              </div>
              {#each g.workspaces ?? [] as w, wi (`${gi}:${wi}`)}
                <div class="bundle-line child">
                  <FolderGit2 size={12} />
                  <span class="bl-name">{w.name}</span>
                  <span class="bl-count">{(w.repos ?? []).length} repo{(w.repos ?? []).length === 1 ? '' : 's'}</span>
                </div>
              {/each}
            {/each}
            {#each bundlePayload.workspaces ?? [] as w, wi (`top:${wi}`)}
              <div class="bundle-line">
                <FolderGit2 size={12} />
                <span class="bl-name">{w.name}</span>
                <span class="bl-count">{(w.repos ?? []).length} repo{(w.repos ?? []).length === 1 ? '' : 's'}</span>
              </div>
            {/each}
          </div>
        {/if}
      {:else if mode === 'group' && groupMeta}
        <div class="preview-meta">
          <Monogram name={groupMeta.name} color={workspaceColorVar(groupMeta.color_idx)} size={26} />
          <div class="meta-text">
            <div class="meta-name">
              <span class="name-text">{groupMeta.name}</span>
              {#if groupMeta.existingGroupId}
                <span class="merge-badge" use:tooltip={'A group with this name already exists — its workspaces will be added to it'}>
                  <GitMerge size={10} /> Merge
                </span>
              {/if}
            </div>
            <div class="meta-stats">
              <span>{groupWorkspaces.length} workspace{groupWorkspaces.length === 1 ? '' : 's'}</span>
              <span class="dot">·</span>
              <span>{totalCount} unique repositor{totalCount === 1 ? 'y' : 'ies'}</span>
              <span class="dot">·</span>
              <span class="resolved-count">{resolvedCount}/{totalCount} resolved</span>
              {#if pendingCount > 0}
                <span class="dot">·</span>
                <span class="pending-count">{pendingCount} pending</span>
              {/if}
              {#if skippedCount > 0}
                <span class="dot">·</span>
                <span class="skipped-count">{skippedCount} skipped</span>
              {/if}
            </div>
          </div>
          <div class="progress-ring">
            <svg viewBox="0 0 36 36" width="36" height="36">
              <circle cx="18" cy="18" r="15" stroke="var(--border)" stroke-width="3" fill="none" />
              <circle
                cx="18" cy="18" r="15"
                stroke="var(--accent)" stroke-width="3" fill="none"
                stroke-dasharray={`${(resolvedCount / Math.max(1, totalCount)) * 94.25} 94.25`}
                stroke-linecap="round"
                transform="rotate(-90 18 18)"
              />
            </svg>
          </div>
        </div>

        <div class="group-tree">
          {#each groupWorkspaces as gw, wi (wi)}
            <div class="ws-section">
              <div class="ws-section-header">
                <Monogram name={gw.name} color={workspaceColorVar(gw.color_idx)} size={18} />
                <span class="ws-section-name">{gw.name}</span>
                {#if gw.existingWsId}
                  <span class="merge-badge" use:tooltip={'Exists in this group — repos merged in, no duplicate'}>
                    <GitMerge size={10} /> Merge
                  </span>
                {/if}
                <span class="ws-section-count">{gw.repoIndices.length} repo{gw.repoIndices.length === 1 ? '' : 's'}</span>
              </div>
              {#if gw.repoIndices.length === 0}
                <div class="ws-empty">Empty workspace — no repositories.</div>
              {:else}
                <div class="ws-section-body">
                  {#each gw.repoIndices as idx (idx)}
                    {#if firstOwnerOf.get(idx) === wi}
                      {@render repoRow(idx)}
                    {:else}
                      {@render sharedRef(idx, firstOwnerOf.get(idx) ?? wi)}
                    {/if}
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      {:else if previewMeta}
        <div class="preview-meta">
          <Monogram name={previewMeta.name} color={workspaceColorVar(previewMeta.color_idx)} size={26} />
          <div class="meta-text">
            <div class="meta-name">
              <span class="name-text">{previewMeta.name}</span>
              {#if previewMeta.existingWsId}
                <span class="merge-badge" use:tooltip={'A workspace with this name exists — its repos will be merged in, no duplicate created'}>
                  <GitMerge size={10} /> Merge
                </span>
              {/if}
            </div>
            <div class="meta-stats">
              <span>{totalCount} repositor{totalCount === 1 ? 'y' : 'ies'}</span>
              <span class="dot">·</span>
              <span class="resolved-count">{resolvedCount}/{totalCount} resolved</span>
              {#if pendingCount > 0}
                <span class="dot">·</span>
                <span class="pending-count">{pendingCount} pending</span>
              {/if}
              {#if skippedCount > 0}
                <span class="dot">·</span>
                <span class="skipped-count">{skippedCount} skipped</span>
              {/if}
            </div>
          </div>
          <div class="progress-ring">
            <svg viewBox="0 0 36 36" width="36" height="36">
              <circle cx="18" cy="18" r="15" stroke="var(--border)" stroke-width="3" fill="none" />
              <circle
                cx="18" cy="18" r="15"
                stroke="var(--accent)" stroke-width="3" fill="none"
                stroke-dasharray={`${(resolvedCount / Math.max(1, totalCount)) * 94.25} 94.25`}
                stroke-linecap="round"
                transform="rotate(-90 18 18)"
              />
            </svg>
          </div>
        </div>

        <div class="rows">
          {#each rows as _row, i (i)}
            {@render repoRow(i)}
          {/each}
        </div>
      {/if}

  <!-- A single editable repo row, shared by the single-workspace list and the
       group tree.  Keyed by index into `rows` so a repo deduped across several
       member workspaces stays one row resolved once.
       These snippets stay nested inside `.iw-body` (rather than as direct
       `<Modal>` children) so they remain local renderable snippets instead of
       being hoisted into Modal's props; they're still in scope for the
       `{@render repoRow(...)}` calls in the list above. -->
  {#snippet repoRow(i: number)}
    {@const row = rows[i]}
    {@const cloning = cloneInProgress.has(i)}
    <div class="row" class:resolved={row.resolvedId !== null} class:skipped={row.action === 'skip'}>
      <div class="row-header">
        <div class="row-info">
          <span class="row-name">{row.name}</span>
          {#if row.action === 'skip'}
            <span class="status-pill skip">Skipped</span>
          {:else if row.resolvedId}
            <span class="status-pill ok"><Check size={10} /> Ready</span>
          {:else}
            <span class="status-pill pending" use:tooltip={'Will be imported unresolved — clone or locate it later from Repository Management'}>Pending</span>
          {/if}
        </div>
        <button
          class="row-skip"
          class:active={row.action === 'skip'}
          onclick={() => setAction(i, row.action === 'skip' ? (row.existing_id ? 'use-existing' : (row.remote_url ? 'clone' : 'locate')) : 'skip')}
          use:tooltip={row.action === 'skip' ? 'Don\'t skip' : 'Skip this repo'}
        >
          <SkipForward size={11} />
          {row.action === 'skip' ? 'Unskip' : 'Skip'}
        </button>
      </div>

      {#if row.remote_url}
        <div class="row-meta">
          <span class="meta-label">Remote</span>
          <code class="meta-value">{row.remote_url}</code>
        </div>
      {/if}
      {#if row.locatedPath}
        <div class="row-meta">
          <span class="meta-label"><MapPin size={10} /> Path</span>
          <code class="meta-value path">{row.locatedPath}</code>
        </div>
      {/if}

      {#if row.action !== 'skip'}
        <div class="action-tabs">
          {#if row.existing_id}
            <button
              class="action-tab"
              class:active={row.action === 'use-existing'}
              onclick={() => setAction(i, 'use-existing')}
            >
              <Check size={11} /> Use existing
            </button>
          {/if}
          {#if row.remote_url}
            <button
              class="action-tab"
              class:active={row.action === 'clone'}
              onclick={() => setAction(i, 'clone')}
              disabled={!!row.resolvedId && row.action === 'use-existing'}
            >
              <Download size={11} /> Clone
            </button>
          {/if}
          <button
            class="action-tab"
            class:active={row.action === 'locate'}
            onclick={() => setAction(i, 'locate')}
            disabled={!!row.resolvedId && row.action === 'use-existing'}
          >
            <MapPin size={11} /> Locate
          </button>
        </div>

        {#if row.action === 'clone' && !row.resolvedId}
          <div class="action-pane">
            <div class="input-with-action">
              <input
                class="input"
                placeholder="Destination folder…"
                bind:value={rows[i].cloneDest}
                spellcheck="false"
                autocomplete="off"
              />
              <button
                class="input-action-btn"
                onclick={() => picker = { kind: 'clone-dest', idx: i }}
                use:tooltip={'Browse…'}
                aria-label="Browse for folder"
              >
                <FolderOpen size={13} />
              </button>
            </div>
            <button
              class="primary-mini"
              onclick={() => cloneRow(i)}
              disabled={cloning || !row.cloneDest || !row.remote_url}
            >
              {#if cloning}
                <RefreshCw size={11} class="spin" /> Cloning…
              {:else}
                <Download size={11} /> Clone
              {/if}
            </button>
          </div>
        {:else if row.action === 'locate' && !row.resolvedId}
          <div class="action-pane">
            <button class="primary-mini wide" onclick={() => picker = { kind: 'locate', idx: i }}>
              <FolderOpen size={11} /> Choose folder…
            </button>
          </div>
        {/if}
      {/if}
    </div>
  {/snippet}

  <!-- Compact mirror for a repo that another member workspace already owns the
       interactive controls for — keeps the dedup promise (resolve once) while
       still showing the repo under every workspace that contains it. -->
  {#snippet sharedRef(idx: number, ownerWi: number)}
    {@const row = rows[idx]}
    <div class="shared-ref" class:skipped={row.action === 'skip'}>
      <Link2 size={12} />
      <span class="shared-name">{row.name}</span>
      {#if row.action === 'skip'}
        <span class="status-pill skip">Skipped</span>
      {:else if row.resolvedId}
        <span class="status-pill ok"><Check size={10} /> Ready</span>
      {:else}
        <span class="status-pill pending">Pending</span>
      {/if}
      <span class="shared-hint">set in “{groupWorkspaces[ownerWi]?.name ?? '—'}”</span>
    </div>
  {/snippet}
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={onClose}>Cancel</Button>
    {#if !inPreview}
      <Button variant="primary" onclick={runPreview} disabled={!jsonText.trim() || previewing} loading={previewing}>
        {previewing ? 'Loading…' : 'Preview'}
      </Button>
    {:else if mode === 'bundle'}
      <Button variant="primary" onclick={commit} disabled={!bundleMeta || creating} loading={creating}>
        {creating ? 'Restoring…' : 'Restore backup'}
      </Button>
    {:else if mode === 'group'}
      <Button variant="primary" onclick={commit} disabled={!canCreate || creating} loading={creating}>
        {creating
          ? (groupMeta?.existingGroupId ? 'Merging…' : 'Creating…')
          : `${groupMeta?.existingGroupId ? 'Merge into Group' : 'Create Group'} (${groupWorkspaces.length})`}
      </Button>
    {:else}
      <Button variant="primary" onclick={commit} disabled={!canCreate || creating} loading={creating}>
        {creating
          ? (previewMeta?.existingWsId ? 'Merging…' : 'Creating…')
          : `${previewMeta?.existingWsId ? 'Merge into Workspace' : 'Create Workspace'} (${memberCount})`}
      </Button>
    {/if}
  {/snippet}
</Modal>

{#if picker !== null}
  {#if picker.kind === 'json'}
    <FileExplorerModal
      mode="file"
      extensions={['json']}
      title="Choose workspace JSON"
      onConfirm={onPickerConfirm}
      onCancel={() => picker = null}
    />
  {:else if picker.kind === 'locate'}
    <FileExplorerModal
      mode="folder"
      title="Locate repository"
      onConfirm={onPickerConfirm}
      onCancel={() => picker = null}
    />
  {:else}
    <FileExplorerModal
      mode="folder"
      title="Choose clone destination"
      onConfirm={onPickerConfirm}
      onCancel={() => picker = null}
    />
  {/if}
{/if}

<style>
  .iw-body {
    display: flex;
    flex-direction: column;
    gap: 14px;
    transition: opacity var(--transition-fast);
  }
  .iw-body.behind { opacity: 0; pointer-events: none; }
  .body-preview { gap: 12px; }

  .back-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    margin-right: 2px;
    transition: background var(--transition-fast), color var(--transition-fast);
    flex-shrink: 0;
  }
  .back-btn:hover { background: var(--bg-hover); color: var(--text-primary); }

  .lead {
    margin: 0;
    color: var(--text-secondary);
    font-size: var(--font-size-sm);
    line-height: 1.5;
  }

  /* ── Source actions (paste phase) ── */
  .source-actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .big-action {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    cursor: pointer;
    text-align: left;
    transition: background var(--transition-fast), border-color var(--transition-fast), transform var(--transition-fast);
  }
  .big-action:hover {
    background: var(--bg-hover);
    border-color: var(--accent);
  }
  .big-action:active { transform: translateY(1px); }
  .big-action-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-sm);
    background: var(--accent-subtle);
    color: var(--accent);
    flex-shrink: 0;
  }
  .big-action-text { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .big-action-title { font-weight: 600; font-size: var(--font-size-sm); }
  .big-action-sub { font-size: 11px; color: var(--text-muted); }

  /* ── Fields ── */
  /* CodeMirror host: the editor (StudioTextPane) flexes to fill this box.
     Vertically resizable like the old textarea; the border + focus ring live
     here since the editor itself is borderless. */
  .json-editor {
    display: flex;
    flex-direction: column;
    height: 240px;
    min-height: 160px;
    resize: vertical;
    overflow: hidden;
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }
  .json-editor:focus-within {
    border-color: var(--border-focus);
    box-shadow: 0 0 0 2px rgba(61,127,255,0.15);
  }

  .inline-error {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 6px 8px;
    background: var(--error-subtle);
    border: 1px solid rgba(199,84,80,0.3);
    border-radius: var(--radius-sm);
    color: var(--error);
    font-size: 11px;
    line-height: 1.45;
  }
  .inline-error :global(svg) { flex-shrink: 0; margin-top: 1px; }

  /* ── Preview meta ── */
  .preview-meta {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
  }
  .meta-text { display: flex; flex-direction: column; gap: 3px; flex: 1; min-width: 0; }
  .meta-name {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
    font-size: 14px;
    color: var(--text-primary);
    min-width: 0;
  }
  .meta-name .name-text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .merge-badge {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 7px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    background: color-mix(in srgb, var(--info) 18%, transparent);
    color: var(--info);
    flex-shrink: 0;
  }
  .meta-stats {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-muted);
  }
  .meta-stats .dot { opacity: 0.5; }
  .meta-stats .resolved-count { color: var(--accent); font-weight: 500; }
  .meta-stats .pending-count { color: var(--text-secondary); }
  .meta-stats .skipped-count { color: var(--text-disabled); }
  .progress-ring { flex-shrink: 0; }

  /* ── Group tree (group import) ── */
  .group-tree { display: flex; flex-direction: column; gap: 12px; }
  .ws-section {
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .ws-section-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
  }
  .ws-section-name {
    font-weight: 600;
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ws-section-count {
    margin-left: auto;
    font-size: 11px;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .ws-section-body {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
  }
  .ws-empty {
    padding: 12px;
    font-size: 11px;
    color: var(--text-muted);
    font-style: italic;
  }

  /* Compact mirror of a repo configured under another workspace. */
  .shared-ref {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    background: var(--bg-overlay);
    border: 1px dashed var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
  }
  .shared-ref :global(svg) { color: var(--text-muted); flex-shrink: 0; }
  .shared-ref.skipped { opacity: 0.55; }
  .shared-name {
    font-weight: 500;
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .shared-hint {
    margin-left: auto;
    font-size: 10.5px;
    color: var(--text-muted);
    flex-shrink: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 50%;
  }

  /* ── Rows ── */
  .rows { display: flex; flex-direction: column; gap: 8px; }

  .row {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    transition: background var(--transition-fast), border-color var(--transition-fast), opacity var(--transition-fast);
  }
  .row.resolved {
    background: color-mix(in srgb, var(--success) 5%, var(--bg-elevated));
    border-color: color-mix(in srgb, var(--success) 30%, var(--border-subtle));
  }
  .row.skipped { opacity: 0.55; }

  .row-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .row-info { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .row-name {
    font-weight: 600;
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status-pill {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 7px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    line-height: 1.6;
    flex-shrink: 0;
  }
  .status-pill.ok {
    background: color-mix(in srgb, var(--success) 18%, transparent);
    color: var(--success);
  }
  /* "Pending" is now a fine, non-blocking outcome (imported unresolved), so
     it reads as a calm deferred state rather than an alarming "action needed". */
  .status-pill.pending {
    background: var(--bg-overlay);
    color: var(--text-secondary);
  }
  .status-pill.skip {
    background: var(--bg-overlay);
    color: var(--text-disabled);
  }

  .row-skip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-size: 10px;
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .row-skip:hover { background: var(--bg-hover); color: var(--text-primary); }
  .row-skip.active {
    background: var(--accent-subtle);
    color: var(--accent);
    border-color: var(--accent);
  }

  .row-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    min-width: 0;
  }
  .meta-label {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    color: var(--text-muted);
    font-weight: 500;
    flex-shrink: 0;
    min-width: 50px;
  }
  .meta-value {
    font-family: var(--font-code);
    font-size: 10.5px;
    color: var(--text-secondary);
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .meta-value.path { color: var(--success); }

  /* Action tab strip */
  .action-tabs {
    display: flex;
    gap: 4px;
    padding: 3px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    width: fit-content;
  }
  .action-tab {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    background: transparent;
    border: none;
    border-radius: 4px;
    color: var(--text-secondary);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .action-tab:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .action-tab:disabled { opacity: 0.45; cursor: not-allowed; }
  .action-tab.active {
    background: var(--accent);
    color: var(--text-on-accent);
  }

  .action-pane {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  /* Input + browse button (clone dest) */
  .input-with-action { position: relative; display: flex; align-items: center; flex: 1; min-width: 0; }
  .input-with-action .input { padding-right: 32px; }
  .input {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    padding: 6px 10px;
    width: 100%;
    outline: none;
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }
  .input:focus {
    border-color: var(--border-focus);
    box-shadow: 0 0 0 2px rgba(61,127,255,0.15);
  }
  .input::placeholder { color: var(--text-disabled); }

  .input-action-btn {
    position: absolute;
    right: 1px; top: 1px; bottom: 1px;
    width: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    color: var(--text-muted);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .input-action-btn:hover { background: var(--bg-overlay); color: var(--text-secondary); }

  .primary-mini {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 6px 12px;
    background: var(--accent);
    color: var(--text-on-accent);
    border: 1px solid var(--accent);
    border-radius: var(--radius-sm);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--transition-fast);
  }
  .primary-mini:hover:not(:disabled) { background: var(--accent-hover); }
  .primary-mini:disabled { opacity: 0.5; cursor: not-allowed; }
  .primary-mini.wide { flex: 1; justify-content: center; }

  /* ── Bundle restore ── */
  .bundle-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-sm);
    background: var(--accent-subtle);
    color: var(--accent);
    flex-shrink: 0;
  }
  .bundle-note {
    margin: 0;
    padding: 9px 11px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-size: 11px;
    line-height: 1.5;
  }
  .bundle-tree {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 280px;
    overflow-y: auto;
  }
  .bundle-line {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-size: var(--font-size-sm);
  }
  .bundle-line :global(svg) { color: var(--text-muted); flex-shrink: 0; }
  .bundle-line.group {
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-size: 11px;
    margin-top: 4px;
  }
  .bundle-line.group :global(svg) { color: var(--accent); }
  /* Member workspaces sit indented under their group header. */
  .bundle-line.child { padding-left: 26px; }
  .bl-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bl-count {
    margin-left: auto;
    font-size: 10.5px;
    color: var(--text-muted);
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }

</style>
