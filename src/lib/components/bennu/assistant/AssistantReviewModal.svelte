<script lang="ts">
  /**
   * AssistantReviewModal — the changes the assistant proposes, read as a diff before anything is written.
   *
   * Sized to read code: most of the window, with the files on the left — each with its folder under its name
   * and a bar of what it adds and removes — and the picked one drawn on the right, syntax-highlighted, unified or
   * side by side (the switch is in the header, where it stays put while the files change). In manual mode the
   * footer is the decision — reject all, or apply all; once decided (or in a mode that applies as it goes) the
   * window is a read-only record. ↑ / ↓ (or J / K) move between files, Ctrl+Enter applies, Esc closes.
   *
   * Deciding goes through the store, which asks the backend to apply (all of it or none) or to reject.
   */
  import { FilePenLine, Check, X, TriangleAlert, Undo2 } from 'lucide-svelte';
  import AssistantOutdatedActions from './AssistantOutdatedActions.svelte';
  import { staleSummary } from './assistant-proposal';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import TextDiff from '$lib/components/shared/ui/TextDiff.svelte';
  import DiffModeToggle from '$lib/components/shared/ui/DiffModeToggle.svelte';
  import DiffStat from '$lib/components/shared/ui/DiffStat.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { highlight } from '$lib/utils/diff-formatter';
  import { modeAppliesEdits } from './assistant-mode';

  const store = bennuAssistantStore;
  const proposal = $derived(store.reviewing);

  let picked = $state(0);
  let split = $state(false);
  let list = $state<HTMLElement | null>(null);

  const file = $derived(proposal?.files[Math.min(picked, (proposal?.files.length ?? 1) - 1)] ?? null);
  const added = $derived(proposal?.files.reduce((n, f) => n + f.added, 0) ?? 0);
  const removed = $derived(proposal?.files.reduce((n, f) => n + f.removed, 0) ?? 0);
  const decidable = $derived(proposal?.status === 'pending' && store.mode === 'manual');
  const outdated = $derived(proposal?.status === 'outdated');
  // Highlighting follows the file's extension; the formatter caches each line it has coloured.
  const colour = $derived(file ? (line: string) => highlight(line, file.path) : undefined);

  // A different proposal starts on its first file.
  $effect(() => {
    void store.review?.proposalId;
    picked = 0;
  });

  // The picked file stays in view as the keys move through a long list.
  $effect(() => {
    list?.querySelector<HTMLElement>(`[data-index="${picked}"]`)?.scrollIntoView({ block: 'nearest' });
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

  function move(by: number) {
    if (!proposal) return;
    picked = Math.max(0, Math.min(picked + by, proposal.files.length - 1));
  }

  function onKeydown(e: KeyboardEvent) {
    if (!proposal) return;
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter' && decidable) { e.preventDefault(); void decide('applied'); }
    else if ((e.ctrlKey || e.metaKey) && e.key === 'Enter' && outdated) { e.preventDefault(); remake(); }
    else if (e.key === 'ArrowDown' || (!typing && e.key === 'j')) { e.preventDefault(); move(1); }
    else if (e.key === 'ArrowUp' || (!typing && e.key === 'k')) { e.preventDefault(); move(-1); }
  }

  function parts(path: string): { dir: string; name: string } {
    const i = path.lastIndexOf('/');
    return i < 0 ? { dir: '', name: path } : { dir: path.slice(0, i), name: path.slice(i + 1) };
  }
</script>

{#if proposal}
  <Modal onClose={close} width="min(1440px, 94vw)" height="min(920px, 88vh)" padBody={false} bodyBorder ariaLabel="Review proposed changes">
    {#snippet header()}
      <ModalHeader onClose={close}>
        <span class="h-icon"><FilePenLine size={15} /></span>
        <span class="modal-title">Review changes</span>
        <span class="sub">{proposal.files.length} file{proposal.files.length === 1 ? '' : 's'}</span>
        <DiffStat {added} {removed} size="md" />
        {#if proposal.status === 'applied'}<Badge variant="tone" tone="success" size="sm"><Check size={10} /> Applied</Badge>{/if}
        {#if proposal.status === 'rejected'}<Badge variant="tone" tone="error" size="sm"><X size={10} /> Rejected</Badge>{/if}
        {#if proposal.status === 'undone'}<Badge variant="tone" tone="neutral" size="sm"><Undo2 size={10} /> Undone</Badge>{/if}
        {#if outdated}<Badge variant="tone" tone="warning" size="sm"><TriangleAlert size={10} /> Outdated</Badge>{/if}
        <span class="grow"></span>
        <DiffModeToggle {split} onToggle={() => (split = !split)} />
      </ModalHeader>
    {/snippet}

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="review" onkeydown={onKeydown}>
      <nav class="files" aria-label="Changed files" bind:this={list}>
        {#each proposal.files as f, i (f.path)}
          {@const p = parts(f.path)}
          <button
            type="button"
            class="file"
            class:picked={i === picked}
            class:stale={f.stale}
            data-index={i}
            aria-current={i === picked ? 'true' : undefined}
            onclick={() => (picked = i)}
          >
            <span class="fname">
              {#if f.stale}<span class="stale-mark" title="The file changed since this was proposed"><TriangleAlert size={11} /></span>{/if}
              <span class="name">{p.name}</span>
            </span>
            <span class="fmeta">
              <span class="dir" title={p.dir}>{p.dir || '.'}</span>
              <DiffStat added={f.added} removed={f.removed} />
            </span>
          </button>
        {/each}
      </nav>

      <section class="diff">
        {#if file}
          {@const p = parts(file.path)}
          <header class="bar">
            <span class="path" title={file.path}><span class="pdir">{p.dir ? `${p.dir}/` : ''}</span><span class="pname">{p.name}</span></span>
            {#if file.stale}<Badge variant="tone" tone="warning" size="sm"><TriangleAlert size={10} /> Changed since proposed</Badge>{/if}
            <DiffStat added={file.added} removed={file.removed} blocks={0} size="md" />
            {#if proposal.files.length > 1}<span class="pos">{picked + 1} / {proposal.files.length}</span>{/if}
          </header>
          <div class="body">
            <TextDiff hunks={file.hunks} mode={split ? 'split' : 'unified'} rowHeight={21} highlight={colour} ariaLabel={`Changes to ${file.path}`} />
          </div>
        {/if}
      </section>
    </div>

    {#snippet footer()}
      <ModalFooter align="between">
        <span class="note">
          {#if proposal.status === 'applied'}Already applied — this is a record.
          {:else if proposal.status === 'rejected'}Rejected — nothing was written.
          {:else if proposal.status === 'undone'}Undone — the files are as they were.
          {:else if outdated}{staleSummary(proposal)} — nothing was written.
          {:else if modeAppliesEdits(store.mode)}This mode applies edits as they are proposed.
          {:else}Nothing is written until you apply.{/if}
          {#if proposal.files.length > 1}<span class="keys"><Kbd keys={["↑"]} /><Kbd keys={["↓"]} /> files</span>{/if}
        </span>
        <div class="actions">
          {#if decidable}
            <Button variant="ghost" size="sm" onclick={() => decide('rejected')}><X size={13} /> Reject all</Button>
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
  .h-icon { display: inline-flex; color: var(--accent); }
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .sub { font-size: var(--font-size-xs); color: var(--text-muted); }
  .grow { flex: 1; }

  .review { display: grid; grid-template-columns: 290px minmax(0, 1fr); height: 100%; min-height: 0; }

  .files {
    display: flex; flex-direction: column; gap: 2px;
    padding: 8px; overflow-y: auto;
    border-right: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
  }
  .file {
    display: flex; flex-direction: column; gap: 3px; align-items: stretch;
    width: 100%; padding: 7px 10px; text-align: left;
    border: none; border-left: 2px solid transparent; border-radius: var(--radius-sm);
    background: transparent; color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast);
  }
  .file:hover { background: var(--bg-hover); }
  .file:focus-visible { outline: none; box-shadow: inset 0 0 0 2px var(--accent); }
  .file.picked { background: var(--accent-subtle); border-left-color: var(--accent); color: var(--text-primary); }
  .fname { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .name {
    min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-code); font-size: var(--font-size-xs); font-weight: 600; color: var(--text-primary);
  }
  .file.stale .name { color: var(--warning); }
  .fmeta { display: flex; align-items: center; gap: 8px; min-width: 0; }
  /* The folder is cut at its start, where it is least telling: `…/assistant/tools` beats `crates/products/…`. */
  .dir {
    flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
  }
  .stale-mark { display: inline-flex; color: var(--warning); }

  .diff { display: flex; flex-direction: column; min-width: 0; min-height: 0; background: var(--bg-base); }
  .bar {
    display: flex; align-items: center; gap: 10px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
  }
  .path { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-code); font-size: var(--font-size-xs); }
  .pdir { color: var(--text-muted); }
  .pname { color: var(--text-primary); font-weight: 600; }
  .pos { font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
  /* The code reads at the size of the editor, not of a caption. */
  .body { flex: 1; min-height: 0; }
  .body :global(.td) { font-size: var(--font-size-sm); }

  .note { display: inline-flex; align-items: center; gap: 12px; font-size: var(--font-size-xs); color: var(--text-muted); }
  .keys { display: inline-flex; align-items: center; gap: 4px; }
  .actions { display: flex; align-items: center; gap: 8px; }
</style>
