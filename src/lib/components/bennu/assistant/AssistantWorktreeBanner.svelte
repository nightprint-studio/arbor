<script lang="ts">
  /**
   * AssistantWorktreeBanner — says, above the thread, that this conversation works on an isolated copy.
   *
   * While the copy is being made (copied, snapshotted, indexed) it shows how far that is; afterwards it names
   * the copy (its kind, and the commit it came from or where it lives) and offers the two ways out: review the
   * combined diff and apply it to the project, or discard it. It also owns the discard confirmation and the
   * review window, so the palette and the buttons open the same things.
   */
  import { GitBranch, Copy, Check, Trash2 } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import ProgressBar from '$lib/components/shared/ui/ProgressBar.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import AssistantWorktreeModal from './AssistantWorktreeModal.svelte';
  import { bennuWorktreeStore } from '$lib/stores/bennu/assistant-worktree.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { onMount } from 'svelte';

  const wt = bennuWorktreeStore;
  const assistant = bennuAssistantStore;

  onMount(() => { void wt.start(); });

  // The copy follows the open conversation, and is re-read when a run ends (it may have been made by that run).
  $effect(() => {
    void assistant.running;
    void assistant.active?.worktree;
    void wt.refreshInfo(assistant.activeId);
  });

  const info = $derived(wt.info);
  const making = $derived(wt.progress && wt.progress.conversation_id === assistant.activeId ? wt.progress : null);

  const PHASES: Record<string, string> = {
    worktree: 'Creating the worktree…',
    copying: 'Copying the project…',
    snapshot: 'Taking a snapshot to compare against…',
    indexing: 'Indexing the copy…',
  };
  const phaseText = $derived(making ? (PHASES[making.phase] ?? 'Preparing…') : '');
  const pct = $derived(making && making.total ? Math.round(((making.done ?? 0) / making.total) * 100) : undefined);

  const kindLabel = $derived(info?.mode === 'git' ? 'Git worktree — indexed' : info?.viaGit ? 'Copy — text only (git worktree)' : 'Copy — text only');
  const where = $derived(info ? (info.head ? `${info.branch ?? 'detached'} @ ${info.head}` : info.copyRoot) : '');

  async function discard() {
    const id = info?.conversationId;
    if (await wt.discard() && id) await assistant.reload(id);
  }
</script>

{#if making}
  <div class="banner">
    <Alert variant="info" compact title="Preparing an isolated copy" text={phaseText} noIcon>
      {#snippet actions()}
        <div class="progress"><ProgressBar pct={pct} indeterminate={pct === undefined} /></div>
      {/snippet}
    </Alert>
  </div>
{:else if info}
  <div class="banner">
    <Alert variant="info" compact title="Working on an isolated copy" noIcon>
      <span class="what">
        {#if info.mode === 'git'}<GitBranch size={11} />{:else}<Copy size={11} />{/if}
        {kindLabel} · <code title={info.copyRoot}>{where}</code>
      </span>
      {#snippet actions()}
        <Button variant="primary" size="xs" disabled={assistant.running} tooltip={assistant.running ? 'Wait for the run to finish' : 'Review the combined diff, then apply it to the project'} onclick={() => wt.openDiff()}>
          <Check size={12} /> Apply to project
        </Button>
        <Button variant="ghost" size="xs" disabled={assistant.running} onclick={() => wt.askDiscard()}>
          <Trash2 size={12} /> Discard
        </Button>
      {/snippet}
    </Alert>
  </div>
{/if}

<AssistantWorktreeModal />

{#if wt.discardAsked}
  <ConfirmModal
    title="Discard the isolated copy?"
    message="Everything Imber changed in the copy is thrown away. Your project is untouched."
    detail="The copy, its index and the processes started in it are removed. This cannot be undone."
    variant="danger"
    confirmLabel="Discard"
    onConfirm={discard}
    onCancel={() => wt.cancelDiscard()}
  />
{/if}

<style>
  .banner { padding: 6px 10px 0; }
  .what { display: inline-flex; align-items: center; gap: 5px; font-size: var(--font-size-xs); color: var(--text-secondary); min-width: 0; }
  .what code { font-family: var(--font-code); font-size: var(--font-size-2xs); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 180px; }
  .progress { width: 140px; }
</style>
