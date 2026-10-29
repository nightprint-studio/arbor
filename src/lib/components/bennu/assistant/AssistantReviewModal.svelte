<script lang="ts">
  /**
   * AssistantReviewModal — the changes the assistant proposes, read as a diff before anything is written.
   *
   * The files are a list on the left (each with what it adds and removes); the one picked is drawn on
   * the right, unified or side by side. In manual mode the footer is the decision — reject all, or
   * apply all; once decided (or in auto mode, where it already went in) the window is a read-only
   * record. ↑ / ↓ move between files, Ctrl+Enter applies, Esc closes.
   *
   * Deciding goes through the store, which asks the backend to apply (all of it or none) or to reject.
   */
  import { FilePenLine, FileCode2, Check, X, TriangleAlert } from 'lucide-svelte';
  import AssistantOutdatedActions from './AssistantOutdatedActions.svelte';
  import { staleSummary } from './assistant-proposal';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import TextDiff from '$lib/components/shared/ui/TextDiff.svelte';
  import DiffModeToggle from '$lib/components/shared/ui/DiffModeToggle.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { modeAppliesEdits } from './assistant-mode';

  const store = bennuAssistantStore;
  const proposal = $derived(store.reviewing);

  let picked = $state(0);
  let split = $state(false);

  const file = $derived(proposal?.files[Math.min(picked, (proposal?.files.length ?? 1) - 1)] ?? null);
  const added = $derived(proposal?.files.reduce((n, f) => n + f.added, 0) ?? 0);
  const removed = $derived(proposal?.files.reduce((n, f) => n + f.removed, 0) ?? 0);
  const decidable = $derived(proposal?.status === 'pending' && store.mode === 'manual');
  const outdated = $derived(proposal?.status === 'outdated');

  // A different proposal starts on its first file.
  $effect(() => {
    void store.review?.proposalId;
    picked = 0;
  });

  function close() { store.closeReview(); }

  async function decide(status: 'applied' | 'rejected') {
    if (!store.review) return;
    await store.resolveProposal(store.review.conversationId, store.review.proposalId, status);
    // An outdated answer keeps the proposal: stay, so the way forward is right here.
    if (store.reviewing?.status !== 'outdated') close();
  }

  function remake() {
    if (store.review) store.remakeProposal(store.review.conversationId, store.review.proposalId);
  }

  function onKeydown(e: KeyboardEvent) {
    if (!proposal) return;
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter' && decidable) { e.preventDefault(); decide('applied'); }
    else if ((e.ctrlKey || e.metaKey) && e.key === 'Enter' && proposal.status === 'outdated') { e.preventDefault(); remake(); }
    else if (e.key === 'ArrowDown') { e.preventDefault(); picked = Math.min(picked + 1, proposal.files.length - 1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); picked = Math.max(picked - 1, 0); }
  }

  function split2(path: string): { dir: string; name: string } {
    const i = path.lastIndexOf('/');
    return i < 0 ? { dir: '', name: path } : { dir: path.slice(0, i + 1), name: path.slice(i + 1) };
  }
</script>

{#if proposal}
  <Modal onClose={close} width="980px" height="640px" padBody={false} bodyBorder ariaLabel="Review proposed changes">
    {#snippet header()}
      <ModalHeader onClose={close}>
        <FilePenLine size={14} />
        <span class="modal-title">Review changes</span>
        <span class="sub">{proposal.files.length} file{proposal.files.length === 1 ? '' : 's'}</span>
        <span class="delta"><b class="add">+{added}</b> <b class="del">−{removed}</b></span>
        {#if proposal.status === 'applied'}<Badge variant="tone" tone="success" size="sm"><Check size={10} /> Applied</Badge>{/if}
        {#if proposal.status === 'rejected'}<Badge variant="tone" tone="error" size="sm"><X size={10} /> Rejected</Badge>{/if}
        {#if outdated}<Badge variant="tone" tone="warning" size="sm"><TriangleAlert size={10} /> Outdated</Badge>{/if}
      </ModalHeader>
    {/snippet}

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="review" onkeydown={onKeydown}>
      <nav class="files" aria-label="Changed files">
        {#each proposal.files as f, i (f.path)}
          {@const p = split2(f.path)}
          <SidebarItem selected={i === picked} onclick={() => (picked = i)}>
            {#snippet icon()}<FileCode2 size={13} />{/snippet}
            <span class="name">{p.name}</span>
            {#snippet subtitle()}<span class="dir">{p.dir || './'}</span>{/snippet}
            {#snippet badges()}
              {#if f.stale}<span class="stale-mark" title="The file changed since this was proposed"><TriangleAlert size={11} /></span>{/if}
              <span class="delta"><span class="add">+{f.added}</span> <span class="del">−{f.removed}</span></span>
            {/snippet}
          </SidebarItem>
        {/each}
      </nav>

      <section class="diff">
        {#if file}
          <header class="bar">
            <span class="path">{file.path}</span>
            <span class="delta"><span class="add">+{file.added}</span> <span class="del">−{file.removed}</span></span>
            <DiffModeToggle {split} onToggle={() => (split = !split)} />
          </header>
          <div class="body">
            <TextDiff hunks={file.hunks} mode={split ? 'split' : 'unified'} ariaLabel={`Changes to ${file.path}`} />
          </div>
        {/if}
      </section>
    </div>

    {#snippet footer()}
      <ModalFooter align="between">
        <span class="note">
          {#if proposal.status === 'applied'}Already applied — this is a record.
          {:else if proposal.status === 'rejected'}Rejected — nothing was written.
          {:else if outdated}{staleSummary(proposal)} — nothing was written.
          {:else if modeAppliesEdits(store.mode)}This mode applies edits as they are proposed.
          {:else}Nothing is written until you apply.{/if}
        </span>
        <div class="actions">
          {#if decidable}
            <Button variant="ghost" size="sm" onclick={() => decide('rejected')}>Reject all</Button>
            <Button variant="primary" size="sm" tooltip={{ content: 'Apply all changes', shortcut: 'Ctrl+Enter' }} onclick={() => decide('applied')}>
              <Check size={13} /> Apply all
            </Button>
          {:else if outdated && store.review}
            <AssistantOutdatedActions {proposal} conversationId={store.review.conversationId} onDone={close} shortcut />
          {:else}
            <Button variant="secondary" size="sm" onclick={close}>Close</Button>
          {/if}
        </div>
      </ModalFooter>
    {/snippet}
  </Modal>
{/if}

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .sub { font-size: var(--font-size-xs); color: var(--text-muted); }

  .review { display: grid; grid-template-columns: 250px minmax(0, 1fr); height: 100%; min-height: 0; }
  .files {
    display: flex; flex-direction: column; gap: 1px;
    padding: 8px 6px; overflow-y: auto;
    border-right: 1px solid var(--border-subtle);
    background: var(--bg-base);
  }
  .name { font-family: var(--font-code); font-size: var(--font-size-xs); }
  .dir { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); }

  .diff { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .bar {
    display: flex; align-items: center; gap: 10px;
    padding: 7px 12px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .path { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary); }
  .body { flex: 1; min-height: 0; }

  .delta { flex-shrink: 0; font-family: var(--font-code); font-size: var(--font-size-2xs); font-variant-numeric: tabular-nums; }
  .add { color: var(--success); font-weight: 600; }
  .del { color: var(--error);   font-weight: 600; }

  .stale-mark { display: inline-flex; color: var(--warning); }
  .note { font-size: var(--font-size-xs); color: var(--text-muted); }
  .actions { display: flex; align-items: center; gap: 8px; }
</style>
