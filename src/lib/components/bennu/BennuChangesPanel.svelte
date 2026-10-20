<script lang="ts">
  /**
   * Changes (bottom dock) — the working tree of the open project, staged and unstaged, with a
   * commit box. The shared {@link StagePanel} fed from Corvus *by path*: Bennu has a project
   * folder, not a Corvus tab, so every call goes through a headless repo of the project root.
   *
   * Deliberately smaller than Corvus's staging area: a click opens the file's change in a
   * modal diff, there is no branch switching (that is Corvus, one button away) and merge
   * conflicts are handed over to it.
   */
  import { GitBranch, ExternalLink, AlertTriangle } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import TextDiff from '$lib/components/shared/ui/TextDiff.svelte';
  import StagePanel, { type StageActions } from '$lib/components/shared/internal/stage/StagePanel.svelte';
  import StageCommitForm, { type CommitRequest } from '$lib/components/shared/internal/stage/StageCommitForm.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuGitStatusStore } from '$lib/stores/bennu/git-status.svelte';
  import { bennuGitChangesStore as changes } from '$lib/stores/bennu/git-changes.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { getWorkdirFilePatch } from '$lib/ipc/corvus/changesByPath';
  import { fsOpenInArbor } from '$lib/ipc/fs';
  import { parseUnifiedPatch } from '$lib/utils/unified-patch';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import type { StatusEntry } from '$lib/types/corvus/git';

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

  // ── Diff of the clicked file ────────────────────────────────────────────
  let diffOf = $state<{ entry: StatusEntry; staged: boolean } | null>(null);
  let patch = $state('');
  let diffError = $state<string | null>(null);
  const hunks = $derived(parseUnifiedPatch(patch));

  async function openDiff(entry: StatusEntry, isStaged: boolean) {
    if (!root) return;
    diffOf = { entry, staged: isStaged };
    patch = '';
    diffError = null;
    try {
      patch = await getWorkdirFilePatch(`${root.replace(/\\/g, '/')}/${entry.path}`, isStaged);
    } catch (e) {
      diffError = String(e);
    }
  }
</script>

<div class="changes">
  <BottomPanelHeader title="Changes" count={unstaged.length + staged.length} onClose={() => bennuUiStore.closeBottom()}>
    {#snippet icon()}<GitBranch size={13} />{/snippet}
    {#snippet children()}
      {#if status?.current_branch}<span class="branch">{status.current_branch}</span>{/if}
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
      <StagePanel
        {unstaged}
        {staged}
        actions={stageActions}
        selectedPath={diffOf?.entry.path ?? null}
        onOpen={openDiff}
        viewMode={appearanceStore.stageViewMode}
        onViewModeChange={(m) => appearanceStore.setStageViewMode(m)}
        compactDirs={appearanceStore.compactFileTreeDirs}
      >
        {#snippet footer()}
          <StageCommitForm stagedCount={staged.length} {template} onSubmit={submit} />
        {/snippet}
      </StagePanel>
    </div>
  {/if}
</div>

{#if diffOf}
  <Modal
    onClose={() => (diffOf = null)}
    width="min(1100px, 94vw)"
    height="min(760px, 90vh)"
    padBody={false}
    ariaLabel="File change"
  >
    {#snippet header()}
      <ModalHeader onClose={() => (diffOf = null)}>
        <span class="modal-title">{diffOf?.staged ? 'Staged change' : 'Change'}</span>
        <span class="path">{diffOf?.entry.path}</span>
      </ModalHeader>
    {/snippet}
    <div class="diff">
      {#if diffError}
        <EmptyState message={diffError} />
      {:else}
        <TextDiff {hunks} identical={false} mode="unified" emptyMessage="No textual change to show." />
      {/if}
    </div>
  </Modal>
{/if}

<style>
  .changes { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--bg-base); }
  .body { flex: 1; min-height: 0; }
  .branch {
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--accent);
    background: var(--accent-subtle); border-radius: var(--radius-sm); padding: 0 6px;
  }
  .notice {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 10px; flex: 1; color: var(--warning); text-align: center; padding: 24px;
  }
  .notice p { margin: 0; color: var(--text-secondary); font-size: var(--font-size-sm); }
  .path {
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 520px;
  }
  .diff { height: 100%; overflow: auto; }
</style>
