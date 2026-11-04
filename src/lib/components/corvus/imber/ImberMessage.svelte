<script lang="ts">
  /**
   * One message of an Imber conversation in Corvus: the shared Imber message frame (the same bubble, author
   * line and hover verbs as Bennu's) filled with Corvus's blocks — text, the tool receipts folded into one
   * line, a Plans card, an approval card for a git write, and a failure notice that offers Retry.
   */
  import { RotateCw } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import ImberMessageFrame from '$lib/components/shared/internal/imber/ImberMessageFrame.svelte';
  import ImberTextBlock from '$lib/components/shared/internal/imber/ImberTextBlock.svelte';
  import ImberToolsFold from '$lib/components/shared/internal/imber/ImberToolsFold.svelte';
  import type { ImberMessage } from '$lib/types/corvus/imber';
  import ImberApproval from './ImberApproval.svelte';
  import ImberPlanCard from './ImberPlanCard.svelte';
  import { foldTools, plainText, summarizeTools, userText } from './imber-messages';

  let { message, live = false, name, canRetry = false, onDecide, onRetry }: {
    message: ImberMessage;
    /** The run writing this message is still going. */
    live?: boolean;
    /** The model's display name. */
    name: string;
    /** Offer Retry on a failure notice (only on the last message, while nothing runs). */
    canRetry?: boolean;
    onDecide: (promptId: string, decision: 'approve' | 'reject') => void;
    onRetry: () => void;
  } = $props();

  const shown = $derived(foldTools(message.blocks));
  const time = $derived(new Date(message.at).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' }));
</script>

<ImberMessageFrame
  role={message.role}
  {name}
  when={time}
  {live}
  plainText={plainText(message)}
  userText={message.role === 'user' ? userText(message) : ''}
>
  {#each shown as b, i}
    {#if b.type === 'text'}
      <ImberTextBlock text={b.text} {live} caret={live && i === shown.length - 1} />
    {:else if b.type === 'tools'}
      <ImberToolsFold summary={summarizeTools(b.calls)} calls={b.calls} />
    {:else if b.type === 'plan'}
      <ImberPlanCard snapshot={b.plan} />
    {:else if b.type === 'approval'}
      <ImberApproval approval={b.approval} promptId={b.promptId} {onDecide} />
    {:else if b.type === 'notice'}
      <Alert variant={b.level} compact text={b.text}>
        {#snippet actions()}
          {#if b.canRetry && canRetry}
            <Button variant="ghost" size="xs" tooltip="Send the same request again" onclick={onRetry}><RotateCw size={11} /> Retry</Button>
          {/if}
        {/snippet}
      </Alert>
      {#if b.detail}<p class="detail">{b.detail}</p>{/if}
    {/if}
  {/each}
</ImberMessageFrame>

<style>
  .detail { margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); overflow-wrap: anywhere; }
</style>
