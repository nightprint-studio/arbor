<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import {
    FileDown, Check, MapPin, Download, SkipForward, AlertCircle,
    ClipboardPaste, FolderOpen, FileJson, RefreshCw, ArrowLeft,
  } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { workspacesStore } from '$lib/stores/workspaces.svelte';
  import {
    importWorkspacePreview, importWorkspaceCommit, registerRepoPath,
  } from '$lib/ipc/workspace';
  import { fsReadTextFile } from '$lib/ipc/fs';
  import { type ExportedWorkspace, workspaceColorVar } from '$lib/types/workspace';
  import FilePickerModal from '../shared/FilePickerModal.svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';

  interface Props { onClose: () => void; }
  let { onClose }: Props = $props();

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

  let jsonText        = $state('');
  let parseError      = $state<string | null>(null);
  let previewMeta     = $state<{ name: string; color_idx: number } | null>(null);
  let rows            = $state<RowState[]>([]);
  let cloneInProgress = $state<Set<number>>(new Set());
  let creating        = $state(false);
  let previewing      = $state(false);
  let picker          = $state<PickerTarget | null>(null);

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

  async function runPreview() {
    parseError = null;
    let payload: ExportedWorkspace;
    try {
      payload = JSON.parse(jsonText) as ExportedWorkspace;
      if (!payload || typeof payload !== 'object' || !Array.isArray(payload.repos)) {
        throw new Error('not a valid workspace export');
      }
    } catch (e) {
      parseError = String(e);
      return;
    }
    previewing = true;
    try {
      const preview = await importWorkspacePreview(payload);
      previewMeta = { name: preview.name, color_idx: preview.color_idx };
      rows = preview.repos.map(r => ({
        name:        r.name,
        remote_url:  r.remote_url,
        existing_id: r.existing_id,
        locatedPath: r.existing_path,
        cloneDest:   '',
        action:      r.existing_id ? 'use-existing' : (r.remote_url ? 'clone' : 'locate'),
        resolvedId:  r.existing_id,
      }));
    } catch (e) {
      parseError = `Preview failed: ${e}`;
    } finally {
      previewing = false;
    }
  }

  function backToPaste() {
    previewMeta = null;
    rows = [];
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
      await invoke<string>('clone_repo', {
        opts: {
          url: row.remote_url,
          dest_path: row.cloneDest,
          branch: null,
          shallow: false,
          recurse_submodules: false,
        },
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
  const importCount   = $derived(rows.filter(r => r.action !== 'skip' && r.resolvedId).length);
  const canCreate = $derived(previewMeta !== null && rows.length > 0 && resolvedCount === totalCount);

  async function commit() {
    if (!previewMeta || !canCreate) return;
    creating = true;
    try {
      const repoIds = rows
        .filter(r => r.action !== 'skip' && r.resolvedId)
        .map(r => r.resolvedId as string);
      const ws = await importWorkspaceCommit(previewMeta.name, previewMeta.color_idx, repoIds, null);
      await workspacesStore.load();
      uiStore.showToast(`Imported workspace "${ws.name}" with ${repoIds.length} repos`, 'success');
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
      {#if previewMeta}
        <button class="back-btn" onclick={backToPaste} use:tooltip={'Back'} aria-label="Back">
          <ArrowLeft size={13} />
        </button>
      {/if}
      <FileDown size={14} strokeWidth={2} />
      <span class="modal-title">Import Workspace</span>
    </ModalHeader>
  {/snippet}

  <div class="iw-body" class:behind={picker !== null} class:body-paste={!previewMeta} class:body-preview={previewMeta !== null}>
      {#if !previewMeta}
        <p class="lead">
          Import a workspace from an exported JSON. Pick the file from disk
          or paste its contents below.
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

        <FormField label="Workspace JSON" for="ws-json">
          <textarea
            id="ws-json"
            class="json-area"
            bind:value={jsonText}
            placeholder={'{ "arbor_workspace_version": 1, "name": "Project X", "color_idx": 3, "repos": [...] }'}
            spellcheck="false"
          ></textarea>
          {#if parseError}
            <div class="inline-error">
              <AlertCircle size={12} />
              <span>{parseError}</span>
            </div>
          {/if}
        </FormField>
      {:else}
        <div class="preview-meta">
          <Monogram name={previewMeta.name} color={workspaceColorVar(previewMeta.color_idx)} size={26} />
          <div class="meta-text">
            <div class="meta-name">{previewMeta.name}</div>
            <div class="meta-stats">
              <span>{totalCount} repositor{totalCount === 1 ? 'y' : 'ies'}</span>
              <span class="dot">·</span>
              <span class="resolved-count">{resolvedCount}/{totalCount} resolved</span>
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
          {#each rows as row, i (i)}
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
                    <span class="status-pill pending">Action needed</span>
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
          {/each}
        </div>
      {/if}
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={onClose}>Cancel</Button>
    {#if !previewMeta}
      <Button variant="primary" onclick={runPreview} disabled={!jsonText.trim() || previewing} loading={previewing}>
        {previewing ? 'Loading…' : 'Preview'}
      </Button>
    {:else}
      <Button variant="primary" onclick={commit} disabled={!canCreate || creating} loading={creating}>
        {creating ? 'Creating…' : `Create Workspace (${importCount}/${totalCount - skippedCount})`}
      </Button>
    {/if}
  {/snippet}
</Modal>

{#if picker !== null}
  {#if picker.kind === 'json'}
    <FilePickerModal
      mode="file"
      extensions={['json']}
      title="Choose workspace JSON"
      onConfirm={onPickerConfirm}
      onCancel={() => picker = null}
    />
  {:else if picker.kind === 'locate'}
    <FilePickerModal
      mode="folder"
      title="Locate repository"
      onConfirm={onPickerConfirm}
      onCancel={() => picker = null}
    />
  {:else}
    <FilePickerModal
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
  .json-area {
    resize: vertical;
    min-height: 160px;
    padding: 10px 12px;
    background: var(--bg-input);
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--font-code);
    font-size: 12px;
    line-height: 1.5;
    outline: none;
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }
  .json-area:focus {
    border-color: var(--border-focus);
    box-shadow: 0 0 0 2px rgba(61,127,255,0.15);
  }
  .json-area::placeholder { color: var(--text-disabled); }

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
  .meta-name { font-weight: 600; font-size: 14px; color: var(--text-primary); }
  .meta-stats {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-muted);
  }
  .meta-stats .dot { opacity: 0.5; }
  .meta-stats .resolved-count { color: var(--accent); font-weight: 500; }
  .meta-stats .skipped-count { color: var(--text-disabled); }
  .progress-ring { flex-shrink: 0; }

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
  .status-pill.pending {
    background: color-mix(in srgb, var(--status-warning, #fbbf24) 14%, transparent);
    color: var(--status-warning, #fbbf24);
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

  :global(.spin) { animation: spin 0.9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
