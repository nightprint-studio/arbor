<script lang="ts">
  /**
   * The yes/no before a model or context-size change that would make a long conversation pay to rebuild its
   * history. The wording follows what actually happens: a new model starts from a cold cache; a smaller
   * window that the conversation already fills compacts it first. A larger window changes nothing and never
   * gets here.
   */
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { formatEur, formatTokens } from './assistant-format';

  const store = bennuAssistantStore;
  const pending = $derived(store.costPending);
</script>

{#if pending}
  {@const c = pending.change}
  <ConfirmModal
    title={c.kind === 'model' ? 'Change the model?' : 'Use a smaller window?'}
    message={c.kind === 'model'
      ? `The conversation holds about ${formatTokens(c.tokens)} tokens. ${pending.to} cannot reuse what ${store.model.name} has cached, so the next message sends all of it again: about ${formatEur(c.euros)} instead of about ${formatEur(c.warmEuros)}.`
      : `The conversation holds about ${formatTokens(c.tokens)} tokens, which already fills a ${pending.to} window. The next message compacts it first: one extra request of about ${formatEur(c.euros)}, and then the history is cached again.`}
    detail="An estimate: four characters a token and the price list. You can turn this question off in Settings › Imber."
    variant="warning"
    confirmLabel="Change anyway"
    onConfirm={() => store.confirmCost()}
    onCancel={() => store.cancelCost()}
  />
{/if}
