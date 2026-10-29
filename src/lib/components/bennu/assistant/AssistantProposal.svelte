<script lang="ts">
  /**
   * Changes the assistant wants to make, laid out to be read before anything is written.
   *
   * What the card offers follows the mode. **Manual**: nothing happens until you say so — reject,
   * review the diff, or apply. **Auto**: the edits go in as they are proposed and the card is a
   * record, with the diff one click away and an undo. Once decided it stays in the conversation as
   * what was decided.
   */
  import { FilePenLine, Check, X, Zap, Undo2, GitCompareArrows } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import type { AssistantMode, EditProposal } from './assistant-types';

  let {
    proposal,
    conversationId,
    mode,
  }: { proposal: EditProposal; conversationId: string; mode: AssistantMode } = $props();

  const added = $derived(proposal.files.reduce((n, f) => n + f.added, 0));
  const removed = $derived(proposal.files.reduce((n, f) => n + f.removed, 0));

  function split(path: string): { dir: string; name: string } {
    const i = path.lastIndexOf('/');
    return i < 0 ? { dir: '', name: path } : { dir: path.slice(0, i + 1), name: path.slice(i + 1) };
  }

  const decide = (status: 'applied' | 'rejected') =>
    bennuAssistantStore.resolveProposal(conversationId, proposal.id, status);
  const review = () => bennuAssistantStore.openReview(conversationId, proposal.id);
</script>

<div class="card" class:applied={proposal.status === 'applied'} class:rejected={proposal.status === 'rejected'}>
  <div class="head">
    <FilePenLine size={13} />
    <span class="title">Proposed changes</span>
    <span class="count">{proposal.files.length} file{proposal.files.length === 1 ? '' : 's'}</span>
    <span class="delta"><b class="add">+{added}</b> <b class="del">−{removed}</b></span>
  </div>

  <ul class="files">
    {#each proposal.files as f (f.path)}
      {@const p = split(f.path)}
      <li class="file">
        <span class="path"><span class="dir">{p.dir}</span><span class="name">{p.name}</span></span>
        <span class="delta"><span class="add">+{f.added}</span> <span class="del">−{f.removed}</span></span>
      </li>
    {/each}
  </ul>

  <div class="foot">
    {#if proposal.status === 'applied'}
      <Badge variant="tone" tone="success" size="sm"><Check size={10} /> Applied</Badge>
      <span class="grow"></span>
      <Button variant="ghost" size="sm" onclick={review}><GitCompareArrows size={12} /> Review</Button>
      <Button variant="ghost" size="sm" onclick={() => decide('rejected')}><Undo2 size={12} /> Undo</Button>
    {:else if proposal.status === 'rejected'}
      <Badge variant="tone" tone="error" size="sm"><X size={10} /> Rejected</Badge>
      <span class="grow"></span>
      <Button variant="ghost" size="sm" onclick={review}><GitCompareArrows size={12} /> Review</Button>
    {:else if mode === 'auto'}
      <Badge variant="tone" tone="info" size="sm"><Zap size={10} /> Applies automatically</Badge>
      <span class="grow"></span>
      <Button variant="ghost" size="sm" onclick={review}><GitCompareArrows size={12} /> Review</Button>
    {:else}
      <Button variant="ghost" size="sm" onclick={() => decide('rejected')}>Reject</Button>
      <span class="grow"></span>
      <Button variant="secondary" size="sm" onclick={review}><GitCompareArrows size={12} /> Review</Button>
      <Button variant="primary" size="sm" onclick={() => decide('applied')}><Check size={12} /> Apply</Button>
    {/if}
  </div>
</div>

<style>
  .card {
    display: flex; flex-direction: column;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
    overflow: hidden;
    transition: border-color var(--transition-fast), opacity var(--transition-fast);
  }
  .card.applied  { border-color: color-mix(in srgb, var(--success) 35%, var(--border-subtle)); }
  /* A refusal is a decision worth seeing: the card keeps its content readable and takes the red. */
  .card.rejected { border-color: color-mix(in srgb, var(--error) 45%, var(--border-subtle)); }
  .card.rejected .head { background: color-mix(in srgb, var(--error) 8%, transparent); }
  .card.rejected .name { color: var(--text-muted); text-decoration: line-through; text-decoration-color: color-mix(in srgb, var(--error) 60%, transparent); }
  .card.rejected .foot { background: color-mix(in srgb, var(--error) 6%, transparent); }

  .head {
    display: flex; align-items: center; gap: 7px;
    padding: 8px 10px;
    color: var(--text-muted);
    border-bottom: 1px solid var(--border-subtle);
  }
  .title { font-size: var(--font-size-xs); font-weight: 600; color: var(--text-primary); }
  .count { font-size: var(--font-size-2xs); color: var(--text-muted); }
  .head .delta { margin-left: auto; }

  .files { list-style: none; margin: 0; padding: 4px 4px; display: flex; flex-direction: column; }
  .file {
    display: flex; align-items: center; justify-content: space-between; gap: 10px;
    padding: 4px 6px; border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
  }
  .file:hover { background: var(--bg-hover); }
  .path { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-code); }
  .dir { color: var(--text-muted); }
  .name { color: var(--text-primary); }

  .delta { flex-shrink: 0; font-family: var(--font-code); font-size: var(--font-size-2xs); font-variant-numeric: tabular-nums; }
  .add { color: var(--success); font-weight: 600; }
  .del { color: var(--error);   font-weight: 600; }

  .foot {
    display: flex; align-items: center; gap: 6px;
    padding: 7px 8px;
    border-top: 1px solid var(--border-subtle);
  }
  .grow { flex: 1; }
</style>
