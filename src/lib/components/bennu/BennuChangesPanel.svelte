<script lang="ts">
  /**
   * Changes (bottom dock) — the working tree of the open project, staged and unstaged, with a
   * commit box. The same layout as Corvus's Staging Area: the shared {@link StagePanel} on the
   * left, the shared {@link DiffViewer} (stageable by line / hunk) on the right. Fed from Corvus
   * *by path*: Bennu has a project folder, not a Corvus tab, so every call goes through a
   * headless repo of the project root.
   *
   * Smaller than Corvus's panel in one way: there is no branch switching (that is Corvus, one
   * button away) and merge conflicts are handed over to it.
   */
  import { untrack } from 'svelte';
  import { GitBranch, ExternalLink, AlertTriangle } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import ResizablePanel from '$lib/components/shared/ui/ResizablePanel.svelte';
  import DiffViewer, { type DiffViewerApi } from '$lib/components/shared/internal/diff/DiffViewer.svelte';
  import DiffToolbar from '$lib/components/shared/internal/diff/DiffToolbar.svelte';
  import StagePanel, { type StageActions } from '$lib/components/shared/internal/stage/StagePanel.svelte';
  import StageCommitForm, { type CommitRequest } from '$lib/components/shared/internal/stage/StageCommitForm.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuGitStatusStore } from '$lib/stores/bennu/git-status.svelte';
  import { bennuGitChangesStore as changes } from '$lib/stores/bennu/git-changes.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { diffStore } from '$lib/stores/corvus/diff.svelte';
  import { fsOpenInArbor } from '$lib/ipc/fs';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import type { DiffFile, StatusEntry } from '$lib/types/corvus/git';

  const root = $derived(projectStore.project?.root ?? null);
  const status = $derived(changes.status);

  const unstaged = $derived([...(status?.unstaged ?? []), ...(status?.untracked ?? [])]);
  const staged = $derived(status?.staged ?? []);
  const conflicted = $derived(status?.conflicted.length ?? 0);

  // Re-read when the project changes and whenever the shared git status moved (a save, a
  // focus, a staging action elsewhere) — the summary is replaced wholesale on every refresh.
  $effect(() => {
    void bennuGitStatusStore.summary;
    void changes.refresh(root);
  });

  let template = $state('');
  $effect(() => {
    if (!root) return;
    void changes.template().then((t) => (template = t));
  });

  const stageActions: StageActions = {
    stage: (t) => changes.stage(t),
    unstage: (t) => changes.unstage(t),
    discard: (t) => changes.discard(t),
    stageAll: () => changes.stageAll(),
    unstageAll: () => changes.unstageAll(),
    discardAll: () => changes.discardAll(),
  };

  async function submit(req: CommitRequest): Promise<boolean> {
    return changes.commit(req.message, req.amend);
  }

  const openInCorvus = () => {
    if (root) void fsOpenInArbor(root).catch((e) => toastStore.show(String(e), 'error'));
  };

  // ── The diff beside the lists ───────────────────────────────────────────
  let selection = $state<{ path: string; staged: boolean } | null>(null);
  let file = $state.raw<DiffFile | null>(null);
  let diffApi = $state<DiffViewerApi | undefined>(undefined);
  // Latest-wins: a slow diff of the previous click must not land over the current one.
  let diffToken = 0;

  async function loadDiff(path: string, isStaged: boolean) {
    const mine = ++diffToken;
    selection = { path, staged: isStaged };
    try {
      const next = await changes.fileDiff(path, isStaged);
      if (mine === diffToken) file = next;
    } catch (e) {
      if (mine !== diffToken) return;
      file = null;
      toastStore.show(`Couldn't load the diff: ${e}`, 'error');
    }
  }

  function clearDiff() {
    diffToken += 1;
    selection = null;
    file = null;
  }

  const hasEntry = (list: StatusEntry[], path: string) => list.some((e) => e.path === path);

  // The lists moved (a stage, a save, a commit): keep the diff true to them. The selected file
  // is re-read on its side, followed to the other side when staging moved it there, and dropped
  // when it no longer has a change at all.
  $effect(() => {
    const s = status;
    if (!s) return;
    const sel = untrack(() => selection);
    if (!sel) return;
    const here = sel.staged ? staged : unstaged;
    const other = sel.staged ? unstaged : staged;
    if (hasEntry(here, sel.path)) void loadDiff(sel.path, sel.staged);
    else if (hasEntry(other, sel.path)) void loadDiff(sel.path, !sel.staged);
    else clearDiff();
  });

  // "Show full file" and an encoding pick ask for the same file again, differently.
  $effect(() => {
    function onReload() {
      const sel = untrack(() => selection);
      if (sel) void loadDiff(sel.path, sel.staged);
    }
    window.addEventListener('arbor:reload-diff', onReload);
    return () => window.removeEventListener('arbor:reload-diff', onReload);
  });

  // A new project has nothing in common with the last one's selection.
  $effect(() => {
    void root;
    untrack(clearDiff);
  });

  async function stageLines(patch: string) {
    const sel = selection;
    try {
      await changes.applyPatch(patch);
      toastStore.show(`${sel?.staged ? 'Unstaged' : 'Staged'} selected lines`, 'success');
    } catch (e) {
      toastStore.show(`Partial ${sel?.staged ? 'unstage' : 'stage'} failed: ${e}`, 'error');
    }
  }
</script>

<div class="changes">
  <BottomPanelHeader title="Changes" count={unstaged.length + staged.length} onClose={() => bennuUiStore.closeBottom()}>
    {#snippet icon()}<GitBranch size={13} />{/snippet}
    {#snippet children()}
      {#if status?.current_branch}<span class="branch">{status.current_branch}</span>{/if}
      {#if file && diffApi}
        <DiffToolbar
          {file}
          repoPath={root}
          stageable
          staged={selection?.staged ?? false}
          selectedCount={diffApi.selectedCount}
          currentChunkIdx={diffApi.currentChunkIdx}
          copyDone={diffApi.copyDone}
          onStageSelected={diffApi.stageSelected}
          onCopyCode={diffApi.copyCode}
          onOpenFullscreen={diffApi.openFullscreen}
          onPrevChunk={diffApi.prevChunk}
          onNextChunk={diffApi.nextChunk}
          onEncodingChange={() => window.dispatchEvent(new CustomEvent('arbor:reload-diff'))}
        />
      {/if}
    {/snippet}
    {#snippet actions()}
      <Button size="xs" variant="ghost" onclick={openInCorvus} tooltip="Open the project in Corvus">
        {#snippet iconStart()}<ExternalLink size={12} />{/snippet}
        Corvus
      </Button>
    {/snippet}
  </BottomPanelHeader>

  {#if !bennuGitStatusStore.inRepo || !status}
    <EmptyState message="This project is not inside a git repository." />
  {:else if conflicted > 0 || status.is_merging}
    <div class="notice">
      <AlertTriangle size={22} />
      <p>{conflicted} file{conflicted === 1 ? '' : 's'} in conflict. Resolve {conflicted === 1 ? 'it' : 'them'} in Corvus.</p>
      <Button variant="primary" onclick={openInCorvus}>Open in Corvus</Button>
    </div>
  {:else}
    <div class="body">
      <ResizablePanel direction="horizontal" initialSize={280} minSize={160} maxSize={500}>
        <StagePanel
          {unstaged}
          {staged}
          actions={stageActions}
          selectedPath={selection?.path ?? null}
          onOpen={(entry, isStaged) => loadDiff(entry.path, isStaged)}
          viewMode={appearanceStore.stageViewMode}
          onViewModeChange={(m) => appearanceStore.setStageViewMode(m)}
          compactDirs={appearanceStore.compactFileTreeDirs}
          confirmDiscard={diffStore.confirmDiscard}
        >
          {#snippet footer()}
            <StageCommitForm stagedCount={staged.length} {template} onSubmit={submit} />
          {/snippet}
        </StagePanel>
      </ResizablePanel>

      <!-- Chromeless: the toolbar lives in the header above, driven through `api`. -->
      <DiffViewer
        {file}
        path={file?.path}
        repoPath={root}
        stageable={file !== null}
        staged={selection?.staged ?? false}
        onStageLines={stageLines}
        onEncodingChange={() => window.dispatchEvent(new CustomEvent('arbor:reload-diff'))}
        chromeless
        bind:api={diffApi}
      />
    </div>
  {/if}
</div>

<style>
  .changes { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--bg-base); }
  .body { display: flex; flex: 1; min-height: 0; overflow: hidden; background: var(--bg-base); }
  .branch {
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--accent);
    background: var(--accent-subtle); border-radius: var(--radius-sm); padding: 0 6px;
  }
  .notice {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 10px; flex: 1; color: var(--warning); text-align: center; padding: 24px;
  }
  .notice p { margin: 0; color: var(--text-secondary); font-size: var(--font-size-sm); }
</style>
