<script lang="ts">
  /**
   * One message of an Imber conversation in Corvus — the same look as Bennu's: yours is a soft bubble on the
   * right, Imber's has no bubble (it is the content), with a quiet line above naming who wrote it. Tool
   * receipts fold into one line that opens on demand; an approval is a card of its own.
   *
   * The presentational pieces (text with the typewriter, tool row, author line, user text) are Bennu's
   * assistant components, reused as they are. They belong in `shared/` and will move there; Corvus must
   * not grow a second copy in the meantime.
   */
  import { RotateCw } from 'lucide-svelte';
  import AssistantText from '$lib/components/bennu/assistant/AssistantText.svelte';
  import AssistantToolRow from '$lib/components/bennu/assistant/AssistantToolRow.svelte';
  import AssistantUserText from '$lib/components/bennu/assistant/AssistantUserText.svelte';
  import AssistantWho from '$lib/components/bennu/assistant/AssistantWho.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import type { ImberMessage } from '$lib/types/corvus/imber';
  import ImberApproval from './ImberApproval.svelte';
  import ImberPlanCard from './ImberPlanCard.svelte';
  import { foldTools, plainText, summarizeTools } from './imber-messages';

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
  const text = $derived(plainText(message));
</script>

{#if message.role === 'user'}
  <div class="user">
    {#each message.blocks as b}
      {#if b.type === 'text'}<div class="bubble"><AssistantUserText text={b.text} /></div>{/if}
    {/each}
  </div>
{:else}
  <div class="assistant">
    <div class="who">
      <AssistantWho {name} when={time} {live} />
      {#if text && !live}<CopyButton value={text} title="Copy the answer" />{/if}
    </div>
    {#each shown as b}
      {#if b.type === 'text'}
        <AssistantText text={b.text} {live} />
      {:else if b.type === 'tools'}
        <Collapsible chevron>
          {#snippet header()}<span class="tools-head">{summarizeTools(b.calls) || 'Working'}</span>{/snippet}
          <div class="receipts">
            {#each b.calls as c (c.id)}<AssistantToolRow call={c} />{/each}
          </div>
        </Collapsible>
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
  </div>
{/if}

<style>
  .user { display: flex; justify-content: flex-end; }
  .bubble {
    max-width: 88%; padding: 6px 10px; border-radius: var(--radius-lg);
    background: var(--bg-overlay); color: var(--text-primary); font-size: var(--font-size-sm);
  }
  .assistant { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
  .who { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .receipts { display: flex; flex-direction: column; gap: 3px; }
  .tools-head { font-size: var(--font-size-xs); color: var(--text-muted); }
  .detail { margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); overflow-wrap: anywhere; }
</style>
