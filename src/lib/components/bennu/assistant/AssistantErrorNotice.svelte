<script lang="ts">
  /**
   * AssistantErrorNotice — how a failed run ends in the thread: what happened and what to do in plain words, a
   * Retry that sends the same request again (no new message), and the provider's own wording under Details.
   * Retry is offered only on the conversation's last message and while nothing is running; the palette has the
   * same verb.
   */
  import { RotateCw } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';

  let { text, detail = '', canRetry = false, last = false }: { text: string; detail?: string; canRetry?: boolean; last?: boolean } = $props();

  const store = bennuAssistantStore;
  const showRetry = $derived(canRetry && last && store.canRetry);
  let open = $state(false);
</script>

<div class="notice">
  <Alert variant="error" compact {text}>
    {#snippet actions()}
      {#if showRetry}
        <Button variant="ghost" size="xs" tooltip="Send the same request again" onclick={() => void store.retry()}>
          <RotateCw size={11} /> Retry
        </Button>
      {/if}
    {/snippet}
  </Alert>
  {#if detail.trim()}
    <Collapsible bind:open chevron>
      {#snippet header()}<span class="more">Details</span>{/snippet}
      <p class="detail">{detail}</p>
    </Collapsible>
  {/if}
</div>

<style>
  .notice { display: flex; flex-direction: column; gap: 2px; }
  .more { font-size: var(--font-size-xs); color: var(--text-muted); }
  .detail {
    margin: 2px 0 0; padding: 6px 8px;
    border-radius: var(--radius-sm); background: var(--bg-overlay);
    font-family: var(--font-code); font-size: var(--font-size-xs); line-height: 1.5;
    color: var(--text-secondary); overflow-wrap: anywhere; user-select: text;
  }
</style>
