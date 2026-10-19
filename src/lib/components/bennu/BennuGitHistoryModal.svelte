<script lang="ts">
  /**
   * Git history of a file, or of a selection of its lines.
   *
   * Local History is Bennu's record of every save; this is what git remembers, answered by
   * Corvus by file path. One dialog for both questions — the commits on the left, what the
   * selected one changed on the right — because they differ only in what the list is *of*:
   * the whole file, or the lines you highlighted (followed through moves by `git log -L`).
   */
  import { GitCommitHorizontal } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ColumnPanes from '$lib/components/shared/ui/ColumnPanes.svelte';
  import DiffModeToggle from '$lib/components/shared/ui/DiffModeToggle.svelte';
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import TextDiff from '$lib/components/shared/ui/TextDiff.svelte';
  import GitCommitList from './git-history/GitCommitList.svelte';
  import { bennuGitHistoryStore as s } from '$lib/stores/bennu/git-history.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';

  const split = $derived(bennuSettingsStore.historyDiffSplit);

  const name = $derived(s.file.split('/').pop() ?? s.file);
  const subject = $derived(
    s.mode === 'selection'
      ? `${name} · lines ${s.startLine}${s.endLine > s.startLine ? `–${s.endLine}` : ''}`
      : name,
  );
</script>

{#if s.open}
  <Modal
    onClose={() => s.close()}
    width="min(1500px, 96vw)"
    height="min(860px, 92vh)"
    padBody={false}
    ariaLabel="Git history"
  >
    {#snippet header()}
      <ModalHeader onClose={() => s.close()}>
        <GitCommitHorizontal size={14} />
        <span class="modal-title">{s.mode === 'selection' ? 'Git history for selection' : 'Git history'}</span>
        <span class="gh-subject">{subject}</span>
      </ModalHeader>
    {/snippet}

    <ColumnPanes error={s.error} leftWidth="380px">
      {#snippet left()}
        <PaneHeader title="Commits" count={s.entries.length} loading={s.loading} />
        <GitCommitList
          entries={s.entries}
          selected={s.selected}
          onSelect={(c) => void s.select(c)}
          emptyMessage={s.loading ? 'Reading history…' : 'No commits touched this.'}
        />
      {/snippet}
      {#snippet right()}
        <PaneHeader title="Changes" loading={s.diffing}>
          {#snippet end()}
            <DiffModeToggle {split} onToggle={() => bennuSettingsStore.setHistoryDiffSplit(!split)} />
          {/snippet}
        </PaneHeader>
        <div class="gh-diff">
          <TextDiff
            hunks={s.hunks}
            identical={false}
            mode={split ? 'split' : 'unified'}
            emptyMessage="Pick a commit to see what it changed."
          />
        </div>
      {/snippet}
    </ColumnPanes>
  </Modal>
{/if}

<style>
  .gh-subject {
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 420px;
  }
  .gh-diff { flex: 1; min-height: 0; }
</style>
