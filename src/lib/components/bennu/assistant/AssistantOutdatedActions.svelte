<script lang="ts">
  /**
   * What can be done with a proposal whose files changed after it was made: drop it, ask the
   * assistant to propose it again, or re-make it on the files as they are now (which works when its
   * changes still apply cleanly) and approve the result. Shared by the card and the review window.
   */
  import { RefreshCw, MessageSquareText, GitCompareArrows } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import type { EditProposal } from './assistant-types';

  let {
    proposal,
    conversationId,
    onDone,
    onReview,
    shortcut = false,
  }: { proposal: EditProposal; conversationId: string; onDone?: () => void; onReview?: () => void; shortcut?: boolean } = $props();

  const store = bennuAssistantStore;

  function discard() { store.resolveProposal(conversationId, proposal.id, 'rejected'); onDone?.(); }
  function ask() { store.askToRedo(conversationId, proposal.id); onDone?.(); }
  function remake() { store.remakeProposal(conversationId, proposal.id); }
</script>

<Button variant="ghost" size="sm" onclick={discard}>Discard</Button>
<span class="grow"></span>
{#if onReview}
  <Button variant="ghost" size="sm" onclick={onReview}><GitCompareArrows size={12} /> Review</Button>
{/if}
<Button variant="secondary" size="sm" tooltip="Drop this and ask Imber to propose it again" onclick={ask}>
  <MessageSquareText size={12} /> Ask to redo
</Button>
<Button
  variant="primary"
  size="sm"
  tooltip={shortcut ? { content: 'Make the changes again on the current files', shortcut: 'Ctrl+Enter' } : 'Make the changes again on the current files'}
  onclick={remake}
>
  <RefreshCw size={12} /> Re-apply on current file
</Button>

<style>
  .grow { flex: 1; }
</style>
