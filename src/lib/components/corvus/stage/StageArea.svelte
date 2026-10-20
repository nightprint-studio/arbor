<script lang="ts">
  /**
   * Corvus's Staging Area bottom panel: the shared {@link StagePanel} fed from the active tab,
   * beside the workdir diff. What lives here is what only Corvus has — the tab, the diff store,
   * merge / conflict states, stash — while the file lists, context menu and discard
   * confirmation are the shared panel's.
   */
  import { AlertTriangle, RefreshCw, GitMerge, CheckCircle2, FileDiff } from 'lucide-svelte';
  import CommitForm from './CommitForm.svelte';
  import ConflictResolver from './ConflictResolver.svelte';
  import DiffViewer, { type DiffViewerApi } from '../diff/DiffViewer.svelte';
  import DiffToolbar from '../diff/DiffToolbar.svelte';
  import ResizablePanel from '../../shared/ui/ResizablePanel.svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import StagePanel, { type StageActions } from '$lib/components/shared/internal/stage/StagePanel.svelte';
  import StashDialog from '$lib/components/shared/internal/StashDialog.svelte';
  import { tabsStore } from '$lib/stores/corvus/tabs.svelte';
  import { repoStore } from '$lib/stores/corvus/repo.svelte';
  import { diffStore } from '$lib/stores/corvus/diff.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { getStatus, stageAll, unstageAll, discardAll, stagePatch, stagePaths, unstagePaths, discardPaths, pathsOf } from '$lib/ipc/corvus/stage';
  import { stashSave } from '$lib/ipc/corvus/branch';
  import { applyPostStashChange } from '$lib/utils/applyPostStashChange';
  import { getWorkdirDiff } from '$lib/ipc/corvus/diff';
  import { tooltip } from '$lib/actions/tooltip';
  import type { StageTarget } from '$lib/utils/stage/stage-tree';

  const tab    = $derived(tabsStore.activeTab);
  const status = $derived(repoStore.status);

  let currentDiffStaged = $state(false);
  let stashOpen         = $state(false);
  let isStashing        = $state(false);
  // Imperative handle exposed by the chromeless DiffViewer — drives the DiffToolbar we render
  // inside the BottomPanelHeader.
  let diffApi = $state<DiffViewerApi | undefined>(undefined);

  const unstagedEntries = $derived([...(status?.unstaged ?? []), ...(status?.untracked ?? [])]);
  const stagedEntries   = $derived(status?.staged ?? []);
  const isWorkingTreeClean = $derived(
    !!status
    && !status.is_merging
    && unstagedEntries.length === 0
    && stagedEntries.length === 0
    && (status.conflicted?.length ?? 0) === 0
  );

  $effect(() => {
    if (tab) refreshStatus();
  });

  // Re-fetch the visible diff when the user toggles "Show full file" — the backend has to emit
  // a different patch (full context vs N-line context).
  $effect(() => {
    function onReload() {
      const f = diffStore.selectedFile;
      if (!tab || !f) return;
      void loadDiff(f.path, currentDiffStaged);
    }
    window.addEventListener('arbor:reload-diff', onReload);
    return () => window.removeEventListener('arbor:reload-diff', onReload);
  });

  async function refreshStatus() {
    if (!tab) return;
    const s = await getStatus(tab.id);
    repoStore.setStatus(s);
    if (!diffStore.selectedFile) {
      const firstUnstaged = s.unstaged[0]?.path ?? s.untracked[0]?.path ?? null;
      const firstStaged   = s.staged[0]?.path ?? null;
      if (firstUnstaged) await loadDiff(firstUnstaged, false);
      else if (firstStaged) await loadDiff(firstStaged, true);
    }
  }

  async function loadDiff(path: string, staged: boolean) {
    if (!tab) return;
    currentDiffStaged = staged;
    // Streaming load: the store populates itself via arbor://diff-stream-* events. We just say
    // which path to select once the metadata list arrives, and show the spinner meanwhile.
    diffStore.setPendingSelection(path);
    diffStore.setLoading(true);
    try {
      await diffStore.loadWorkdirDiffStream(tab.id, staged);
    } catch {
      // On failure the backend emits arbor://diff-stream-error which clears loading; clear it
      // defensively in case that never surfaced.
      diffStore.setLoading(false);
    }
  }

  async function handleStageLines(patch: string) {
    if (!tab) return;
    try {
      await stagePatch(tab.id, patch);
      const f = diffStore.selectedFile;
      if (f) {
        const remaining = await getWorkdirDiff(tab.id, currentDiffStaged);
        const fileEntry = remaining.find(df => df.path === f.path);
        if (!fileEntry || fileEntry.hunks.length === 0) {
          if (currentDiffStaged) await unstagePaths(tab.id, pathsOf(f));
          else await stagePaths(tab.id, pathsOf(f));
          diffStore.setFiles([]);
        } else {
          diffStore.setFiles(remaining);
          diffStore.selectFile(f.path);
        }
      }
      await refreshStatus();

      if (f && diffStore.selectedFile?.path === f.path) {
        const s = repoStore.status;
        if (s) {
          const stillInSourceList = currentDiffStaged
            ? s.staged.some(e => e.path === f.path)
            : s.unstaged.some(e => e.path === f.path) || s.untracked.some(e => e.path === f.path);
          if (!stillInSourceList) {
            diffStore.setFiles([]);
            const nextUnstaged = s.unstaged[0]?.path ?? s.untracked[0]?.path ?? null;
            const nextStaged   = s.staged[0]?.path ?? null;
            if (!currentDiffStaged && nextUnstaged) await loadDiff(nextUnstaged, false);
            else if (nextStaged)                    await loadDiff(nextStaged, true);
          }
        }
      }

      uiStore.showToast(`${currentDiffStaged ? 'Unstaged' : 'Staged'} selected lines`, 'success');
    } catch (err) {
      uiStore.showToast(`Partial ${currentDiffStaged ? 'unstage' : 'stage'} failed: ${err}`, 'error');
    }
  }

  /** One try / refresh / toast shape for every action the panel can ask for. */
  async function run(op: () => Promise<void>, ok: string | null, failed: string, okLevel: 'success' | 'warning' = 'success') {
    if (!tab) return;
    try {
      await op();
      await refreshStatus();
      if (ok) uiStore.showToast(ok, okLevel);
    } catch (err) {
      uiStore.showToast(`${failed}: ${err}`, 'error');
    }
  }

  const actions: StageActions = {
    stage: ({ paths, label }: StageTarget) =>
      paths.length === 0 ? undefined : run(() => stagePaths(tab!.id, paths), `Staged ${label}`, 'Stage failed'),
    unstage: ({ paths, label }: StageTarget) =>
      paths.length === 0 ? undefined : run(() => unstagePaths(tab!.id, paths), `Unstaged ${label}`, 'Unstage failed'),
    discard: ({ paths, label }: StageTarget) =>
      run(async () => {
        await discardPaths(tab!.id, paths);
        if (paths.includes(diffStore.selectedFile?.path ?? '')) diffStore.setFiles([]);
      }, `Discarded ${label}`, 'Discard failed', 'warning'),
    stageAll: () => run(() => stageAll(tab!.id), null, 'Stage all failed'),
    unstageAll: () => run(() => unstageAll(tab!.id), null, 'Unstage all failed'),
    discardAll: () =>
      run(async () => {
        await discardAll(tab!.id);
        diffStore.setFiles([]);
      }, 'Discarded all unstaged changes', 'Discard all failed', 'warning'),
    stash: () => { stashOpen = true; },
  };

  async function confirmStash(message: string) {
    if (!tab || isStashing) return;
    isStashing = true;
    try {
      await stashSave(tab.id, message || undefined, true);
      uiStore.showToast('Changes stashed', 'success');
      stashOpen = false;
      // The workdir is now clean, so any selected file no longer exists.
      diffStore.setFiles([]);
      // Refresh the file lists + repaint stash markers and the sidebar list. A stash doesn't
      // change graph topology, so the full getGraph reload is skipped.
      await Promise.all([refreshStatus(), applyPostStashChange(tab.id)]);
    } catch (err) {
      uiStore.showToast(`Stash failed: ${err}`, 'error');
    } finally {
      isStashing = false;
    }
  }
</script>

<div class="stage-root">
  <BottomPanelHeader title="Staging Area" onClose={() => uiStore.setActiveBottomSection(null)}>
    {#snippet icon()}
      <FileDiff size={14} />
    {/snippet}
    {#snippet children()}
      {#if diffStore.selectedFile && diffApi}
        <DiffToolbar
          file={diffStore.selectedFile}
          stageable
          staged={currentDiffStaged}
          selectedCount={diffApi.selectedCount}
          currentChunkIdx={diffApi.currentChunkIdx}
          copyDone={diffApi.copyDone}
          onStageSelected={diffApi.stageSelected}
          onCopyCode={diffApi.copyCode}
          onOpenFullscreen={diffApi.openFullscreen}
          onPrevChunk={diffApi.prevChunk}
          onNextChunk={diffApi.nextChunk}
          onEncodingChange={() => {
            const sel = diffStore.selectedFile;
            if (sel) void loadDiff(sel.path, currentDiffStaged);
          }}
        />
      {/if}
    {/snippet}
  </BottomPanelHeader>

{#if status?.is_merging || (status?.conflicted.length ?? 0) > 0}
  <!-- Merge / conflict state: redirect to guided resolution. Triggers on an active merge OR on
       any unmerged index entries even without one (e.g. after an aborted operation). -->
  <div class="stage-area centered">
    <div class="merge-notice">
      {#if (status?.conflicted.length ?? 0) > 0}
        <AlertTriangle size={28} class="merge-notice-icon-conflict" />
        <p class="notice-title">
          {status?.is_merging ? 'Merge in corso — conflitti da risolvere' : 'Conflitti da risolvere nell\'index'}
        </p>
        <p class="merge-notice-sub">
          {status!.conflicted.length} file in conflitto.
          {#if !status?.is_merging}
            Residui da un'operazione precedente.
          {/if}
          Usa la risoluzione guidata per procedere.
        </p>
        <button class="merge-notice-btn" onclick={() => uiStore.openMergeModal()}>
          <GitMerge size={15} />
          Apri risoluzione conflitti…
        </button>
      {:else}
        <GitMerge size={28} class="merge-notice-icon-ok" />
        <p class="notice-title">Merge in corso</p>
        <p class="merge-notice-sub">
          Tutti i conflitti sono stati risolti. Completa il merge dalla risoluzione guidata.
        </p>
        <button class="merge-notice-btn" onclick={() => uiStore.openMergeModal()}>
          <GitMerge size={15} />
          Completa merge…
        </button>
      {/if}
    </div>
  </div>
{:else if isWorkingTreeClean}
  <div class="stage-area centered">
    <div class="clean-notice">
      <CheckCircle2 size={24} class="clean-notice-icon" />
      <p class="notice-title">Working tree clean</p>
      <p class="clean-notice-sub">
        No changes on <span class="branch-pill">{status?.current_branch ?? 'HEAD'}</span>.
        Edit a file to get started.
      </p>
      <button class="clean-notice-btn" onclick={() => refreshStatus()} use:tooltip={'Refresh status'}>
        <RefreshCw size={12} />
        Refresh
      </button>
    </div>
  </div>
{:else}
<div class="stage-area">
  <ResizablePanel direction="horizontal" initialSize={280} minSize={160} maxSize={500}>
    <StagePanel
      unstaged={unstagedEntries}
      staged={stagedEntries}
      {actions}
      selectedPath={diffStore.selectedFile?.path ?? null}
      onOpen={(entry, staged) => loadDiff(entry.path, staged)}
      viewMode={appearanceStore.stageViewMode}
      onViewModeChange={(m) => appearanceStore.setStageViewMode(m)}
      compactDirs={appearanceStore.compactFileTreeDirs}
      confirmDiscard={diffStore.confirmDiscard}
    >
      {#snippet middle()}
        {#if status?.conflicted && status.conflicted.length > 0}
          <ConflictResolver conflicts={status.conflicted} onResolved={refreshStatus} />
        {/if}
      {/snippet}
      {#snippet footer()}
        <CommitForm onCommit={() => { diffStore.setFiles([]); refreshStatus(); }} />
      {/snippet}
    </StagePanel>
  </ResizablePanel>

  <!-- Right: diff viewer (stageable only when a workdir diff is loaded). Chromeless — the
       toolbar lives in the BottomPanelHeader above; `api` drives that external toolbar. -->
  <DiffViewer
    file={diffStore.selectedFile}
    path={diffStore.selectedFile?.path}
    stageable={diffStore.selectedFile !== null}
    staged={currentDiffStaged}
    onStageLines={handleStageLines}
    onEncodingChange={() => {
      const sel = diffStore.selectedFile;
      if (sel) void loadDiff(sel.path, currentDiffStaged);
    }}
    chromeless
    bind:api={diffApi}
  />
</div>
{/if}
</div>

{#if stashOpen}
  <StashDialog
    branchName={status?.current_branch}
    busy={isStashing}
    onConfirm={confirmStash}
    onCancel={() => (stashOpen = false)}
  />
{/if}

<style>
  /* Bottom-panel root: stacks the standardized BottomPanelHeader above the stage layout. */
  .stage-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    overflow: hidden;
    background: var(--bg-base);
  }

  .stage-area {
    display: flex;
    flex: 1;
    min-height: 0;
    background: var(--bg-base);
    overflow: hidden;
  }
  .stage-area.centered { align-items: center; justify-content: center; }

  .notice-title {
    font-size: var(--font-size-md, 13px);
    font-weight: 600;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    margin: 0;
  }

  /* ── Clean working tree ── */
  .clean-notice {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    max-width: 320px;
    text-align: center;
    padding: 20px 24px;
  }
  :global(.clean-notice-icon) { color: var(--success); opacity: 0.85; }
  .clean-notice-sub {
    font-size: var(--font-size-sm);
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    margin: 0;
    line-height: 1.5;
  }
  .branch-pill {
    display: inline-block;
    padding: 0 6px;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    color: var(--accent);
    background: var(--accent-subtle);
    border-radius: var(--radius-sm);
  }
  .clean-notice-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    font-family: var(--font-ui-sans);
    cursor: pointer;
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-secondary);
    transition: background var(--transition-fast), color var(--transition-fast);
    margin-top: 6px;
  }
  .clean-notice-btn:hover { background: var(--bg-hover); color: var(--text-primary); }

  /* ── Merge state ── */
  .merge-notice {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    max-width: 340px;
    text-align: center;
    padding: 32px 24px;
  }
  :global(.merge-notice-icon-conflict) { color: var(--warning); }
  :global(.merge-notice-icon-ok)       { color: var(--success); }
  .merge-notice-sub {
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    margin: 0;
    line-height: 1.5;
  }
  .merge-notice-btn {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 8px 18px;
    border-radius: var(--radius-md);
    font-size: var(--font-size-sm);
    font-family: var(--font-ui-sans);
    font-weight: 600;
    cursor: pointer;
    background: var(--warning);
    border: none;
    color: var(--text-on-accent);
    transition: background var(--transition-fast);
    margin-top: 4px;
  }
  .merge-notice-btn:hover { background: color-mix(in srgb, var(--warning) 80%, white); }
</style>
