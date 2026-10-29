<script lang="ts">
  /**
   * "Stopped at Order.java:42 in OrderService.total — snapshot ready": the program the assistant is debugging
   * stopped while its conversation was idle. The snapshot is already queued for the next message; the button
   * starts a run now with a short message, so the assistant looks at it without anything to type.
   */
  import { Bug } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { assistantStopStore } from '$lib/stores/bennu/assistant-stop.svelte';
  import { onMount } from 'svelte';

  const store = bennuAssistantStore;
  onMount(() => { void assistantStopStore.start(); });

  const card = $derived(assistantStopStore.cardFor(store.activeId));

  // A run in that conversation reads the queued note on its own: the card has nothing left to offer.
  $effect(() => {
    if (card && store.running && store.runConversationId === card.conversationId) assistantStopStore.dismiss(card.conversationId);
  });
</script>

{#if card}
  <div class="stop-card">
    <Alert variant="warning" compact title={card.title} text={`Snapshot ready (${card.reason}). It goes to Imber with your next message.`}>
      {#snippet actions()}
        <Button variant="primary" size="xs" disabled={store.running} onclick={() => void assistantStopStore.sendNow(card.conversationId)}>
          <Bug size={11} /> Send to Imber now
        </Button>
        <Button variant="ghost" size="xs" onclick={() => assistantStopStore.dismiss(card.conversationId)}>Dismiss</Button>
      {/snippet}
    </Alert>
  </div>
{/if}

<style>
  .stop-card { margin: 4px 0; }
</style>
