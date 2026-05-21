<script lang="ts">
  import { untrack } from 'svelte';
  import { Plus, Minus, X, RotateCcw, RefreshCw, Archive, Check, Square, CheckSquare, GitMerge, AlertTriangle, ChevronDown, ChevronRight, List, FolderTree, Folder, CheckCircle2, FileDiff } from 'lucide-svelte';
  import CommitForm from './CommitForm.svelte';
  import DiffViewer, { type DiffViewerApi } from '../diff/DiffViewer.svelte';
  import DiffToolbar from '../diff/DiffToolbar.svelte';
  import ResizablePanel from '../layout/ResizablePanel.svelte';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import DiscardConfirmModal from './DiscardConfirmModal.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { repoStore } from '$lib/stores/repo.svelte';
  import { diffStore } from '$lib/stores/diff.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import ConflictResolver from './ConflictResolver.svelte';
  import Tree from '$lib/components/shared/ui/Tree.svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import { getStatus, stageFile, unstageFile, stageAll, unstageAll, discardFile, discardAll, stagePatch } from '$lib/ipc/stage';
  import { stashSave } from '$lib/ipc/branch';
  import { applyPostStashChange } from '$lib/utils/applyPostStashChange';
  import { getWorkdirDiff, getWorkdirDiffStream } from '$lib/ipc/diff';
  import { tooltip } from '$lib/actions/tooltip';

  const tab    = $derived(tabsStore.activeTab);
  const status = $derived(repoStore.status);

  let viewMode         = $state<'staged' | 'unstaged'>('unstaged');
  let currentDiffStaged = $state(false);
  let stashOpen           = $state(false);
  let stashMsg            = $state('');
  let stashInputEl: HTMLInputElement | undefined = $state();
  $effect(() => { if (stashOpen) stashInputEl?.focus(); });
  let isStashing          = $state(false);
  // Imperative handle exposed by the chromeless DiffViewer below — drives
  // the DiffToolbar we render inside the BottomPanelHeader.
  let diffApi = $state<DiffViewerApi | undefined>(undefined);

  // Accordion: unstaged starts open, staged starts closed
  let unstagedCollapsed = $state(false);
  let stagedCollapsed   = $state(true);

  // Opening one section always closes the other
  function toggleSection(which: 'unstaged' | 'staged') {
    if (which === 'unstaged') {
      unstagedCollapsed = !unstagedCollapsed;
      if (!unstagedCollapsed) stagedCollapsed = true;
    } else {
      stagedCollapsed = !stagedCollapsed;
      if (!stagedCollapsed) unstagedCollapsed = true;
    }
  }

  // ── List / Tree view mode ────────────────────────────────────────────────
  let listViewMode = $state<'list' | 'tree'>(
    (localStorage.getItem('arbor:stage-view-mode') as 'list' | 'tree') ?? 'list'
  );
  $effect(() => { localStorage.setItem('arbor:stage-view-mode', listViewMode); });

  interface StageTreeNode {
    name: string;
    fullPath: string;
    children: Map<string, StageTreeNode>;
    /** Sorted snapshot of `children.values()` — baked in at build time so the
     *  template never allocates/sorts per render. */
    sortedChildren: StageTreeNode[];
    entry?: { path: string; workdir_status?: string; index_status?: string };
  }

  function buildStageTree(entries: { path: string }[]): StageTreeNode {
    const root: StageTreeNode = { name: '', fullPath: '', children: new Map(), sortedChildren: [] };
    for (const entry of entries) {
      const parts = entry.path.split('/');
      let node = root;
      for (let i = 0; i < parts.length; i++) {
        const part = parts[i];
        if (!node.children.has(part)) {
          const fp = parts.slice(0, i + 1).join('/');
          node.children.set(part, { name: part, fullPath: fp, children: new Map(), sortedChildren: [] });
        }
        node = node.children.get(part)!;
      }
      node.entry = entry as StageTreeNode['entry'];
    }
    // Post-pass: populate sortedChildren on every node so the template can iterate
    // pre-sorted arrays without a filter/sort allocation per render.
    function bakeSort(n: StageTreeNode) {
      n.sortedChildren = [...n.children.values()].sort(sortNodes);
      for (const c of n.sortedChildren) bakeSort(c);
    }
    bakeSort(root);
    return root;
  }

  const unstagedEntries = $derived([...(status?.unstaged ?? []), ...(status?.untracked ?? [])]);
  const stagedEntries   = $derived(status?.staged ?? []);
  const isWorkingTreeClean = $derived(
    !!status
    && !status.is_merging
    && unstagedEntries.length === 0
    && stagedEntries.length === 0
    && (status.conflicted?.length ?? 0) === 0
  );
  const unstagedTree    = $derived(buildStageTree(unstagedEntries));
  const stagedTree      = $derived(buildStageTree(stagedEntries));

  let unstagedExpandedPaths = $state(new Set<string>());
  let stagedExpandedPaths   = $state(new Set<string>());
  // Track dirs seen on the previous tree pass so we can distinguish brand-new
  // dirs (auto-expand) from ones the user has explicitly collapsed (preserve).
  let unstagedKnownDirs = new Set<string>();
  let stagedKnownDirs   = new Set<string>();

  function collectDirs(node: StageTreeNode, out: Set<string>) {
    if (node.children.size > 0 && node.fullPath) out.add(node.fullPath);
    for (const c of node.children.values()) collectDirs(c, out);
  }

  function reconcileExpanded(currentDirs: Set<string>, prevExpanded: Set<string>, knownDirs: Set<string>): Set<string> {
    const next = new Set<string>();
    for (const d of currentDirs) {
      if (knownDirs.has(d)) {
        if (prevExpanded.has(d)) next.add(d);
      } else {
        next.add(d);
      }
    }
    return next;
  }

  $effect(() => {
    const dirs = new Set<string>();
    collectDirs(unstagedTree, dirs);
    untrack(() => {
      unstagedExpandedPaths = reconcileExpanded(dirs, unstagedExpandedPaths, unstagedKnownDirs);
      unstagedKnownDirs = dirs;
    });
  });
  $effect(() => {
    const dirs = new Set<string>();
    collectDirs(stagedTree, dirs);
    untrack(() => {
      stagedExpandedPaths = reconcileExpanded(dirs, stagedExpandedPaths, stagedKnownDirs);
      stagedKnownDirs = dirs;
    });
  });

  function toggleUnstagedDir(path: string) {
    const s = new Set(unstagedExpandedPaths);
    s.has(path) ? s.delete(path) : s.add(path);
    unstagedExpandedPaths = s;
  }
  function toggleStagedDir(path: string) {
    const s = new Set(stagedExpandedPaths);
    s.has(path) ? s.delete(path) : s.add(path);
    stagedExpandedPaths = s;
  }

  function sortNodes(a: StageTreeNode, b: StageTreeNode): number {
    const aD = a.children.size > 0, bD = b.children.size > 0;
    if (aD !== bD) return aD ? -1 : 1;
    return a.name.localeCompare(b.name);
  }

  // ── Discard confirmation ─────────────────────────────────────────────────
  type DiscardTarget = { kind: 'file'; path: string } | { kind: 'all'; count: number } | { kind: 'folder'; paths: string[] };
  let discardPending = $state<DiscardTarget | null>(null);

  function isConfirmDiscardEnabled() {
    return (localStorage.getItem('arbor:confirm-discard') ?? 'true') === 'true';
  }

  // ── Context menu ────────────────────────────────────────────────
  type FileCtx = { x: number; y: number; items: MenuItem[]; path: string; paths?: string[] };
  let ctxMenu = $state<FileCtx | null>(null);

  function openUnstagedCtx(e: MouseEvent, path: string) {
    e.preventDefault();
    ctxMenu = {
      x: e.clientX, y: e.clientY, path,
      items: [
        { id: 'stage',   label: 'Stage File',      icon: Plus, iconColor: 'var(--success)' },
        { id: 'discard', label: 'Discard Changes',  icon: RotateCcw, danger: true },
      ],
    };
  }

  function openStagedCtx(e: MouseEvent, path: string) {
    e.preventDefault();
    ctxMenu = {
      x: e.clientX, y: e.clientY, path,
      items: [
        { id: 'unstage', label: 'Unstage File', icon: Minus, iconColor: 'var(--warning)' },
      ],
    };
  }

  function collectLeafPaths(node: StageTreeNode): string[] {
    const paths: string[] = [];
    function collect(n: StageTreeNode) {
      if (n.entry) paths.push(n.entry.path);
      for (const child of n.children.values()) collect(child);
    }
    for (const child of node.children.values()) collect(child);
    return paths;
  }

  function openUnstagedFolderCtx(e: MouseEvent, node: StageTreeNode) {
    e.preventDefault();
    e.stopPropagation();
    const paths = collectLeafPaths(node);
    ctxMenu = {
      x: e.clientX, y: e.clientY, path: node.fullPath, paths,
      items: [
        { id: 'stage-folder',   label: `Stage Folder (${paths.length})`,    icon: Plus,     iconColor: 'var(--success)' },
        { id: 'discard-folder', label: `Discard Folder (${paths.length})`,  icon: RotateCcw, danger: true },
        { id: 'sep-1', label: '', separator: true },
        { id: 'stash-all', label: 'Stash All Changes…', icon: Archive, iconColor: 'var(--color-stash)', action: 'stash' },
      ],
    };
  }

  function openStagedFolderCtx(e: MouseEvent, node: StageTreeNode) {
    e.preventDefault();
    e.stopPropagation();
    const paths = collectLeafPaths(node);
    ctxMenu = {
      x: e.clientX, y: e.clientY, path: node.fullPath, paths,
      items: [
        { id: 'unstage-folder', label: `Unstage Folder (${paths.length})`, icon: Minus, iconColor: 'var(--warning)' },
      ],
    };
  }

  async function handleCtxSelect(id: string) {
    if (!ctxMenu) return;
    const { path, paths } = ctxMenu;
    ctxMenu = null;
    if      (id === 'stage')          await handleStage(path);
    else if (id === 'discard')        handleDiscard(path);
    else if (id === 'unstage')        await handleUnstage(path);
    else if (id === 'stage-folder')   await handleStageFolder(paths ?? []);
    else if (id === 'discard-folder') handleDiscardFolder(paths ?? []);
    else if (id === 'unstage-folder') await handleUnstageFolder(paths ?? []);
    else if (id === 'stash-all')      { stashMsg = ''; stashOpen = true; }
  }

  $effect(() => {
    if (tab) refreshStatus();
  });

  // Re-fetch the visible diff when the user toggles "Show full file" — the
  // backend has to emit a different patch (full context vs N-line context).
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
      if (firstUnstaged) {
        await loadDiff(firstUnstaged, false);
      } else if (firstStaged) {
        await loadDiff(firstStaged, true);
      }
    }
  }

  async function loadDiff(path: string, staged: boolean) {
    if (!tab) return;
    currentDiffStaged = staged;
    // Streaming load: the store populates itself via
    // arbor://diff-stream-* events.  We just tell it which path we want
    // selected once the metadata list arrives, and show the spinner meanwhile.
    diffStore.setPendingSelection(path);
    diffStore.setLoading(true);
    try {
      await getWorkdirDiffStream(tab.id, staged);
    } catch {
      // On failure the backend emits diff-stream-error which clears loading.
      // Set loading to false defensively in case the error never surfaced.
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
          if (currentDiffStaged) await unstageFile(tab.id, f.path);
          else await stageFile(tab.id, f.path);
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

  async function handleStage(path: string) {
    if (!tab) return;
    try {
      await stageFile(tab.id, path);
      await refreshStatus();
      uiStore.showToast(`Staged ${path.split('/').pop()}`, 'success');
    } catch (err) {
      uiStore.showToast(`Stage failed: ${err}`, 'error');
    }
  }

  async function handleUnstage(path: string) {
    if (!tab) return;
    try {
      await unstageFile(tab.id, path);
      await refreshStatus();
    } catch (err) {
      uiStore.showToast(`Unstage failed: ${err}`, 'error');
    }
  }

  function handleDiscard(path: string) {
    if (!tab) return;
    if (isConfirmDiscardEnabled()) {
      discardPending = { kind: 'file', path };
    } else {
      executeDiscardFile(path);
    }
  }

  async function handleStageFolder(paths: string[]) {
    if (!tab || paths.length === 0) return;
    try {
      await Promise.all(paths.map(p => stageFile(tab!.id, p)));
      await refreshStatus();
      uiStore.showToast(`Staged ${paths.length} file${paths.length !== 1 ? 's' : ''}`, 'success');
    } catch (err) {
      uiStore.showToast(`Stage folder failed: ${err}`, 'error');
    }
  }

  async function handleUnstageFolder(paths: string[]) {
    if (!tab || paths.length === 0) return;
    try {
      await Promise.all(paths.map(p => unstageFile(tab!.id, p)));
      await refreshStatus();
      uiStore.showToast(`Unstaged ${paths.length} file${paths.length !== 1 ? 's' : ''}`, 'success');
    } catch (err) {
      uiStore.showToast(`Unstage folder failed: ${err}`, 'error');
    }
  }

  function handleDiscardFolder(paths: string[]) {
    if (!tab || paths.length === 0) return;
    if (isConfirmDiscardEnabled()) {
      discardPending = { kind: 'folder', paths };
    } else {
      executeDiscardFolder(paths);
    }
  }

  async function executeDiscardFolder(paths: string[]) {
    if (!tab) return;
    try {
      await Promise.all(paths.map(p => discardFile(tab!.id, p)));
      for (const p of paths) {
        if (diffStore.selectedFile?.path === p) diffStore.setFiles([]);
      }
      await refreshStatus();
      uiStore.showToast(`Discarded ${paths.length} file${paths.length !== 1 ? 's' : ''}`, 'warning');
    } catch (err) {
      uiStore.showToast(`Discard folder failed: ${err}`, 'error');
    }
  }

  async function executeDiscardFile(path: string) {
    if (!tab) return;
    try {
      await discardFile(tab.id, path);
      if (diffStore.selectedFile?.path === path) diffStore.setFiles([]);
      await refreshStatus();
      uiStore.showToast(`Discarded ${path.split('/').pop()}`, 'warning');
    } catch (err) {
      uiStore.showToast(`Discard failed: ${err}`, 'error');
    }
  }

  function handleDiscardAll() {
    if (!tab) return;
    const count = (status?.unstaged.length ?? 0) + (status?.untracked.length ?? 0);
    if (count === 0) return;
    discardPending = { kind: 'all', count };
  }

  async function executeDiscardAll() {
    if (!tab) return;
    try {
      await discardAll(tab.id);
      diffStore.setFiles([]);
      await refreshStatus();
      uiStore.showToast('Discarded all unstaged changes', 'warning');
    } catch (err) {
      uiStore.showToast(`Discard all failed: ${err}`, 'error');
    }
  }

  async function onDiscardConfirm() {
    const pending = discardPending;
    discardPending = null;
    if (!pending) return;
    if (pending.kind === 'file') await executeDiscardFile(pending.path);
    else if (pending.kind === 'folder') await executeDiscardFolder(pending.paths);
    else await executeDiscardAll();
  }

  function onDiscardCancel() {
    discardPending = null;
  }

  async function handleStageAll() {
    if (!tab) return;
    try {
      await stageAll(tab.id);
      await refreshStatus();
    } catch (err) {
      uiStore.showToast(`Stage all failed: ${err}`, 'error');
    }
  }

  async function handleUnstageAll() {
    if (!tab) return;
    try {
      await unstageAll(tab.id);
      await refreshStatus();
    } catch (err) {
      uiStore.showToast(`Unstage all failed: ${err}`, 'error');
    }
  }

  async function confirmStash() {
    if (!tab || isStashing) return;
    isStashing = true;
    try {
      await stashSave(tab.id, stashMsg.trim() || undefined, true);
      uiStore.showToast('Changes stashed', 'success');
      stashOpen = false;
      stashMsg  = '';
      // Clear the currently-shown diff: the workdir is now clean so any
      // previously selected unstaged/staged file no longer exists.
      diffStore.setFiles([]);
      // Refresh stage file lists (this panel) + repaint stash markers and
      // sidebar list.  Stash op doesn't change graph topology, so we skip
      // the full getGraph reload that the old code did.
      await Promise.all([refreshStatus(), applyPostStashChange(tab.id)]);
    } catch (err) {
      uiStore.showToast(`Stash failed: ${err}`, 'error');
    } finally {
      isStashing = false;
    }
  }

  function onStashKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); confirmStash(); }
    if (e.key === 'Escape') { e.stopPropagation(); stashOpen = false; stashMsg = ''; }
  }
</script>

<div class="stage-root">
  <BottomPanelHeader title="Staging Area">
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
  <!-- ── Merge/conflict state: redirect to guided resolution.
       Triggers on an active merge OR on any unmerged index entries even
       without an active merge (e.g. after an aborted operation). ── -->
  <div class="stage-area merge-state">
    <div class="merge-notice">
      {#if (status?.conflicted.length ?? 0) > 0}
        <AlertTriangle size={28} class="merge-notice-icon-conflict" />
        <p class="merge-notice-title">
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
        <p class="merge-notice-title">Merge in corso</p>
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
  <!-- ── Clean working tree: compact empty state ── -->
  <div class="stage-area clean-state">
    <div class="clean-notice">
      <CheckCircle2 size={24} class="clean-notice-icon" />
      <p class="clean-notice-title">Working tree clean</p>
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
  <!-- Left: staged/unstaged file lists -->
  <ResizablePanel direction="horizontal" initialSize={280} minSize={160} maxSize={500}>
    <div class="files-panel">

      <!-- Stash inline dialog -->
      {#if stashOpen}
        <div class="stash-drop" role="dialog" aria-label="Stash changes">
          <p class="stash-title">Stash message <span class="stash-opt">(optional)</span></p>
          <input
            class="stash-input"
            type="text"
            placeholder="WIP on {status?.current_branch ?? 'branch'}…"
            bind:value={stashMsg}
            onkeydown={onStashKeydown}
            bind:this={stashInputEl}
          />
          <div class="stash-row">
            <button class="stash-btn cancel" onclick={() => { stashOpen = false; stashMsg = ''; }} disabled={isStashing}>
              <X size={11} /> Cancel
            </button>
            <button class="stash-btn confirm" onclick={confirmStash} disabled={isStashing}>
              <Check size={11} /> {isStashing ? 'Stashing…' : 'Stash All'}
            </button>
          </div>
        </div>
      {/if}

      <!-- ── Unstaged ─────────────────────────────────────────────── -->
      <div class="section" class:collapsed={unstagedCollapsed}>
        <div class="section-header" role="button" tabindex="0"
          onclick={() => toggleSection('unstaged')}
          onkeydown={(e) => e.key === 'Enter' && toggleSection('unstaged')}
        >
          <span class="chevron">{#if unstagedCollapsed}<ChevronRight size={12} />{:else}<ChevronDown size={12} />{/if}</span>
          <span>Unstaged</span>
          <span class="count" class:nonzero={unstagedEntries.length > 0}>{unstagedEntries.length}</span>

          <div class="header-right">
            <!-- view mode toggle -->
            <button class="icon-action" class:active={listViewMode === 'list'} onclick={(e) => { e.stopPropagation(); listViewMode = 'list'; }} use:tooltip={'List view'}><List size={11} /></button>
            <button class="icon-action" class:active={listViewMode === 'tree'} onclick={(e) => { e.stopPropagation(); listViewMode = 'tree'; }} use:tooltip={'Tree view'}><FolderTree size={11} /></button>
            <span class="header-sep"></span>
            <!-- git actions -->
            <button class="icon-action" onclick={(e) => { e.stopPropagation(); handleStageAll(); }} use:tooltip={'Stage all'}><CheckSquare size={13} /></button>
            <button class="icon-action discard-all-btn" onclick={(e) => { e.stopPropagation(); handleDiscardAll(); }} use:tooltip={'Discard all unstaged changes'}><RotateCcw size={12} /></button>
            <button class="icon-action" onclick={(e) => { e.stopPropagation(); stashMsg = ''; stashOpen = !stashOpen; }} use:tooltip={'Stash all changes'}><Archive size={12} /></button>
          </div>
        </div>

        <div class="file-list">
          {#if listViewMode === 'list'}
            {#each unstagedEntries as entry}
              {@const ws = entry.workdir_status}
              <div
                class="file-entry"
                class:selected={diffStore.selectedFile?.path === entry.path}
                onclick={() => loadDiff(entry.path, false)}
                oncontextmenu={(e) => openUnstagedCtx(e, entry.path)}
                role="row" tabindex="0"
                onkeydown={(e) => e.key === 'Enter' && loadDiff(entry.path, false)}
              >
                <span class="status-badge"
                  class:s-added={ws === 'added' || ws === 'untracked'}
                  class:s-modified={ws === 'modified'}
                  class:s-deleted={ws === 'deleted'}
                  class:s-renamed={ws === 'renamed'}
                >{ws === 'added' || ws === 'untracked' ? 'A' : ws === 'modified' ? 'M' : ws === 'deleted' ? 'D' : ws === 'renamed' ? 'R' : '?'}</span>
                <span class="filename truncate" use:tooltip={entry.path}>{entry.path.split('/').pop()}</span>
                <div class="file-actions">
                  <button class="file-btn stage-btn" onclick={(e) => { e.stopPropagation(); handleStage(entry.path); }} use:tooltip={'Stage file'}><Plus size={12} /></button>
                  <button class="file-btn discard-btn" onclick={(e) => { e.stopPropagation(); handleDiscard(entry.path); }} use:tooltip={'Discard changes'}><RotateCcw size={11} /></button>
                </div>
              </div>
            {/each}
          {:else}
            <!-- Tree view (rendered by shared <Tree>; the row snippet
                 branches between dir + file content). -->
            <Tree
              nodes={unstagedTree.sortedChildren}
              getId={(n: StageTreeNode) => n.fullPath}
              getChildren={(n: StageTreeNode) => n.sortedChildren}
              expandedIds={unstagedExpandedPaths}
              onExpandToggle={(id) => toggleUnstagedDir(id)}
              selectedId={diffStore.selectedFile?.path ?? null}
              selectable={(n: StageTreeNode) => !!n.entry}
              indentSize={12}
              basePadding={8}
              ariaLabel="Unstaged files"
              onSelect={(n: StageTreeNode) => { if (n.entry) loadDiff(n.entry.path, false); }}
              onContextMenu={(n: StageTreeNode, e: MouseEvent) =>
                n.entry ? openUnstagedCtx(e, n.entry.path) : openUnstagedFolderCtx(e, n)}
            >
              {#snippet row({ node }: { node: StageTreeNode })}
                {#if node.entry}
                  {@const entry = node.entry}
                  {@const ws = entry.workdir_status}
                  <span class="status-badge"
                    class:s-added={ws === 'added' || ws === 'untracked'}
                    class:s-modified={ws === 'modified'}
                    class:s-deleted={ws === 'deleted'}
                    class:s-renamed={ws === 'renamed'}
                  >{ws === 'added' || ws === 'untracked' ? 'A' : ws === 'modified' ? 'M' : ws === 'deleted' ? 'D' : ws === 'renamed' ? 'R' : '?'}</span>
                  <span class="filename truncate" use:tooltip={entry.path}>{node.name}</span>
                  <div class="file-actions">
                    <button class="file-btn stage-btn" onclick={(e) => { e.stopPropagation(); handleStage(entry.path); }} use:tooltip={'Stage file'}><Plus size={12} /></button>
                    <button class="file-btn discard-btn" onclick={(e) => { e.stopPropagation(); handleDiscard(entry.path); }} use:tooltip={'Discard changes'}><RotateCcw size={11} /></button>
                  </div>
                {:else}
                  <Folder size={11} class="folder-icon" />
                  <span class="dir-name">{node.name}</span>
                {/if}
              {/snippet}
            </Tree>
          {/if}
        </div>
      </div>

      {#if status?.conflicted && status.conflicted.length > 0}
        <ConflictResolver conflicts={status.conflicted} onResolved={refreshStatus} />
      {/if}

      <!-- ── Staged ──────────────────────────────────────────────── -->
      <div class="section" class:collapsed={stagedCollapsed} class:has-staged={stagedEntries.length > 0}>
        <div class="section-header" role="button" tabindex="0"
          onclick={() => toggleSection('staged')}
          onkeydown={(e) => e.key === 'Enter' && toggleSection('staged')}
        >
          <span class="chevron">{#if stagedCollapsed}<ChevronRight size={12} />{:else}<ChevronDown size={12} />{/if}</span>
          <span>Staged</span>
          <span class="count staged-count" class:nonzero={stagedEntries.length > 0}>{stagedEntries.length}</span>

          <div class="header-right">
            <!-- view mode toggle -->
            <button class="icon-action" class:active={listViewMode === 'list'} onclick={(e) => { e.stopPropagation(); listViewMode = 'list'; }} use:tooltip={'List view'}><List size={11} /></button>
            <button class="icon-action" class:active={listViewMode === 'tree'} onclick={(e) => { e.stopPropagation(); listViewMode = 'tree'; }} use:tooltip={'Tree view'}><FolderTree size={11} /></button>
            <span class="header-sep"></span>
            <!-- git actions -->
            <button class="icon-action" onclick={(e) => { e.stopPropagation(); handleUnstageAll(); }} use:tooltip={'Unstage all'}><Square size={13} /></button>
          </div>
        </div>

        <div class="file-list">
          {#if listViewMode === 'list'}
            {#each stagedEntries as entry}
              {@const is = entry.index_status}
              <div
                class="file-entry"
                class:selected={diffStore.selectedFile?.path === entry.path}
                onclick={() => loadDiff(entry.path, true)}
                oncontextmenu={(e) => openStagedCtx(e, entry.path)}
                role="row" tabindex="0"
                onkeydown={(e) => e.key === 'Enter' && loadDiff(entry.path, true)}
              >
                <span class="status-badge"
                  class:s-added={is === 'added' || is === 'untracked'}
                  class:s-modified={is === 'modified'}
                  class:s-deleted={is === 'deleted'}
                  class:s-renamed={is === 'renamed'}
                >{is === 'added' || is === 'untracked' ? 'A' : is === 'modified' ? 'M' : is === 'deleted' ? 'D' : is === 'renamed' ? 'R' : 'M'}</span>
                <span class="filename truncate" use:tooltip={entry.path}>{entry.path.split('/').pop()}</span>
                <div class="file-actions">
                  <button class="file-btn unstage-btn" onclick={(e) => { e.stopPropagation(); handleUnstage(entry.path); }} use:tooltip={'Unstage file'}><X size={11} /></button>
                </div>
              </div>
            {/each}
          {:else}
            <!-- Tree view -->
            <Tree
              nodes={stagedTree.sortedChildren}
              getId={(n: StageTreeNode) => n.fullPath}
              getChildren={(n: StageTreeNode) => n.sortedChildren}
              expandedIds={stagedExpandedPaths}
              onExpandToggle={(id) => toggleStagedDir(id)}
              selectedId={diffStore.selectedFile?.path ?? null}
              selectable={(n: StageTreeNode) => !!n.entry}
              indentSize={12}
              basePadding={8}
              ariaLabel="Staged files"
              onSelect={(n: StageTreeNode) => { if (n.entry) loadDiff(n.entry.path, true); }}
              onContextMenu={(n: StageTreeNode, e: MouseEvent) =>
                n.entry ? openStagedCtx(e, n.entry.path) : openStagedFolderCtx(e, n)}
            >
              {#snippet row({ node }: { node: StageTreeNode })}
                {#if node.entry}
                  {@const entry = node.entry}
                  {@const is = entry.index_status}
                  <span class="status-badge"
                    class:s-added={is === 'added' || is === 'untracked'}
                    class:s-modified={is === 'modified'}
                    class:s-deleted={is === 'deleted'}
                    class:s-renamed={is === 'renamed'}
                  >{is === 'added' || is === 'untracked' ? 'A' : is === 'modified' ? 'M' : is === 'deleted' ? 'D' : is === 'renamed' ? 'R' : 'M'}</span>
                  <span class="filename truncate" use:tooltip={entry.path}>{node.name}</span>
                  <div class="file-actions">
                    <button class="file-btn unstage-btn" onclick={(e) => { e.stopPropagation(); handleUnstage(entry.path); }} use:tooltip={'Unstage file'}><X size={11} /></button>
                  </div>
                {:else}
                  <Folder size={11} class="folder-icon" />
                  <span class="dir-name">{node.name}</span>
                {/if}
              {/snippet}
            </Tree>
          {/if}
        </div>
      </div>

      <!-- Commit form -->
      <div class="commit-wrap">
        <CommitForm onCommit={() => { diffStore.setFiles([]); refreshStatus(); }} />
      </div>
    </div>
  </ResizablePanel>

  <!-- Right: diff viewer (stageable only when a workdir diff is loaded).
       chromeless — the toolbar lives in the BottomPanelHeader above; we
       bind `api` to drive that external toolbar. -->
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

{#if ctxMenu}
  <ContextMenu
    x={ctxMenu.x}
    y={ctxMenu.y}
    items={ctxMenu.items}
    onSelect={handleCtxSelect}
    onClose={() => ctxMenu = null}
  />
{/if}

{#if discardPending}
  <DiscardConfirmModal
    target={discardPending.kind === 'file'
      ? discardPending.path.split('/').pop() ?? discardPending.path
      : discardPending.kind === 'folder'
      ? `${discardPending.paths.length} file${discardPending.paths.length !== 1 ? 's' : ''} in folder`
      : `all ${discardPending.count} unstaged file${discardPending.count !== 1 ? 's' : ''}`}
    onConfirm={onDiscardConfirm}
    onCancel={onDiscardCancel}
  />
{/if}

<style>
  /* Bottom-panel root: stacks the standardized BottomPanelHeader above
     the stage layout. Matches the column layout used by other bottom
     panels so the header sits at 34px and the body fills the rest. */
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

  .files-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-input);
    overflow: hidden;
  }

  .section {
    display: flex;
    flex-direction: column;
    flex: 1 1 0;
    min-height: 0;
    overflow: hidden;
    transition: flex var(--transition-fast);
  }

  .section.collapsed {
    flex: 0 0 auto;
  }

  .section-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px 5px 6px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
    cursor: pointer;
    user-select: none;
  }
  .section-header:hover { background: var(--bg-hover); }

  .chevron {
    display: flex;
    align-items: center;
    color: var(--text-disabled);
    flex-shrink: 0;
  }

  .count {
    background: var(--bg-overlay);
    padding: 0 5px;
    border-radius: 999px;
    font-size: 10px;
    color: var(--text-disabled);
    transition: background var(--transition-fast), color var(--transition-fast);
  }

  /* Active count badges — cyan/blue */
  .count.nonzero {
    background: var(--info-subtle);
    color: var(--info);
  }

  /* Staged count with items — green tint */
  .count.staged-count.nonzero {
    background: var(--success-subtle);
    color: var(--success);
  }

  /* Staged section header: subtle green left border when items present */
  .section.has-staged > .section-header {
    border-left: 2px solid var(--success);
    padding-left: 4px;
  }

  /* Right-aligned button group inside section header */
  .header-right {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .header-sep {
    width: 1px;
    height: 12px;
    background: var(--border-subtle);
    margin: 0 3px;
    flex-shrink: 0;
  }

  .icon-action {
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--text-muted);
    padding: 2px;
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
  }
  .icon-action:hover { color: var(--text-primary); background: var(--bg-overlay); }
  .icon-action.active { color: var(--accent); background: var(--accent-subtle); }
  .icon-action.discard-all-btn:hover { color: var(--warning); background: var(--warning-subtle); }

  .file-list {
    flex: 1;
    overflow-y: auto;
    padding: 2px;
  }
  .section.collapsed .file-list { display: none; }

  .file-entry {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: var(--font-size-xs);
    transition: background var(--transition-fast);
  }
  .file-entry:hover    { background: var(--bg-hover); }
  .file-entry.selected { background: var(--bg-selected, rgba(77,120,204,0.15)); }

  /* Tree view rendered by shared <Tree>. The strip / chevron / row styling
     is owned by Tree.svelte; only the in-row file/dir glyphs still live
     here. */
  :global(.folder-icon) { color: var(--warning); flex-shrink: 0; }

  .dir-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }

  /* ── Status badge (M / A / D / R / ?) ── */
  .status-badge {
    flex-shrink: 0;
    width: 16px;
    height: 16px;
    border-radius: var(--radius-sm);
    font-size: 10px;
    font-weight: 700;
    line-height: 16px;
    text-align: center;
    background: var(--bg-overlay);
    color: var(--text-muted);
    letter-spacing: 0;
  }
  .status-badge.s-added    { background: color-mix(in srgb, var(--color-file-added) 22%, transparent);    color: var(--color-file-added); }
  .status-badge.s-modified { background: color-mix(in srgb, var(--color-file-modified) 22%, transparent); color: var(--color-file-modified); }
  .status-badge.s-deleted  { background: color-mix(in srgb, var(--color-file-deleted) 22%, transparent);  color: var(--color-file-deleted); }
  .status-badge.s-renamed  { background: color-mix(in srgb, var(--color-file-renamed) 22%, transparent);  color: var(--color-file-renamed); }

  .filename { flex: 1; color: var(--text-primary); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  /* ── Per-file action buttons ── */
  .file-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }

  .file-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }

  .stage-btn {
    background: color-mix(in srgb, var(--success) 14%, transparent);
    color: var(--success);
  }
  .stage-btn:hover { background: color-mix(in srgb, var(--success) 30%, transparent); color: color-mix(in srgb, var(--success) 80%, var(--text-primary)); }

  .unstage-btn {
    background: color-mix(in srgb, var(--error) 14%, transparent);
    color: var(--error);
  }
  .unstage-btn:hover { background: color-mix(in srgb, var(--error) 30%, transparent); color: color-mix(in srgb, var(--error) 80%, var(--text-primary)); }

  .discard-btn {
    background: transparent;
    color: var(--text-disabled);
  }
  .discard-btn:hover { background: var(--warning-subtle); color: var(--warning); }

  .commit-wrap {
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }

  /* ── Stash inline dialog ── */
  .stash-drop {
    border-bottom: 1px solid var(--border);
    padding: 10px;
    background: var(--bg-base);
    animation: slideDown 120ms cubic-bezier(0.16,1,0.3,1);
  }
  @keyframes slideDown {
    from { opacity: 0; transform: translateY(-6px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  .stash-title {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    margin: 0 0 6px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
  }
  .stash-opt {
    font-weight: 400;
    text-transform: none;
    letter-spacing: 0;
    color: var(--text-muted);
  }

  .stash-input {
    width: 100%;
    box-sizing: border-box;
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    padding: 5px 8px;
    outline: none;
    transition: border-color var(--transition-fast);
  }
  .stash-input:focus { border-color: var(--accent); }

  .stash-row {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    margin-top: 8px;
  }

  .stash-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .stash-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .stash-btn.cancel { background: transparent; color: var(--text-muted); }
  .stash-btn.cancel:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .stash-btn.confirm { background: var(--accent); color: var(--text-on-accent); border-color: var(--accent); }
  .stash-btn.confirm:hover:not(:disabled) { background: var(--accent-hover, #3b5fc0); }

  /* ── Clean working tree empty state ── */
  .stage-area.clean-state {
    align-items: center;
    justify-content: center;
  }
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
  .clean-notice-title {
    font-size: var(--font-size-md, 13px);
    font-weight: 600;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    margin: 0;
  }
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
    font-size: 11px;
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
    font-size: 11px;
    font-family: var(--font-ui-sans);
    cursor: pointer;
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-secondary);
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
    margin-top: 6px;
  }
  .clean-notice-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--border);
  }

  /* ── Merge state overlay ── */
  .stage-area.merge-state {
    align-items: center;
    justify-content: center;
  }
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

  .merge-notice-title {
    font-size: var(--font-size-md, 13px);
    font-weight: 600;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    margin: 0;
  }
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
