<script lang="ts">
  /**
   * Compact the conversation, saying what the summary must keep.
   *
   * The same idea as `/compact "…"` in Claude Code: older tool output is cleared and everything before the
   * last message is folded into a note the model writes — and what you type here is told to the model as
   * what that note must not lose. Empty is fine; the note then keeps what it judges to matter. It costs one
   * request, which is added to the conversation's cost.
   */
  import { Minimize2 } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Textarea from '$lib/components/shared/ui/Textarea.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';

  const store = bennuAssistantStore;
  let note = $state('');

  async function compact() {
    await store.compact(note);
    note = '';
  }
</script>

{#if store.compactOpen}
  <Modal onClose={() => store.closeCompact()} width="520px" height="300px" ariaLabel="Compact the conversation">
    {#snippet header()}
      <ModalHeader onClose={() => store.closeCompact()}>
        <Minimize2 size={14} />
        <span class="title">Compact the conversation</span>
      </ModalHeader>
    {/snippet}

    <div class="body">
      <p>
        Older tool output is cleared and everything before your last message becomes a summary the model writes.
        Say what it must not lose.
      </p>
      <Textarea
        bind:value={note}
        placeholder="e.g. the decision about the EUR fallback, and which callers are still to update"
        minRows={3}
        maxRows={6}
        onsubmit={compact}
      />
    </div>

    {#snippet footer()}
      <ModalFooter align="between">
        <span class="hint">One request; its cost is added to the conversation.</span>
        <div class="actions">
          <Button variant="ghost" size="sm" onclick={() => store.closeCompact()}>Cancel</Button>
          <Button variant="primary" size="sm" disabled={store.compacting} onclick={compact}>
            {#if store.compacting}<Spinner /> Compacting…{:else}Compact{/if}
          </Button>
        </div>
      </ModalFooter>
    {/snippet}
  </Modal>
{/if}

<style>
  .title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .body { display: flex; flex-direction: column; gap: 10px; padding: 14px 16px; }
  p { margin: 0; font-size: var(--font-size-sm); line-height: 1.55; color: var(--text-secondary); }
  .hint { font-size: var(--font-size-xs); color: var(--text-muted); }
  .actions { display: flex; gap: 8px; }
</style>
