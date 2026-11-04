<script lang="ts">
  /**
   * An action Imber wants to take, shown as what it is: the exact git command(s) and what they do, until
   * the user decides. Once decided the card stays in the thread as the record — approved (with what
   * happened) or declined. Destructive actions are marked and always come here, whatever the mode.
   *
   * Keyboard: `Ctrl+Enter` approves and `Esc` declines while the card waits (the panel routes the keys);
   * the buttons are the same two verbs for the mouse.
   */
  import { Check, ShieldAlert, X } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Card from '$lib/components/shared/ui/Card.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import MarkdownView from '$lib/components/shared/ui/MarkdownView.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import ImberPlanCard from './ImberPlanCard.svelte';
  import { plansStore } from '$lib/stores/corvus/plans.svelte';
  import type { ImberApproval } from '$lib/types/corvus/imber';

  let { approval, promptId, onDecide }: {
    approval: ImberApproval;
    /** Present while the run waits for the answer. */
    promptId?: string;
    onDecide: (promptId: string, decision: 'approve' | 'reject') => void;
  } = $props();

  const waiting = $derived(approval.answer === null && !!promptId);
  const destructive = $derived(approval.risk === 'destructive');
  const fenced = $derived('```bash\n' + approval.commands.join('\n') + '\n```');
</script>

<div class="approval">
  <Card variant="subtle" padding="sm">
    {#snippet header()}
      <span class="head">
        {#if destructive}<ShieldAlert size={13} class="warn" />{/if}
        <strong>{approval.title}</strong>
        {#if destructive}<Badge variant="tone" tone="error" size="sm" label="asks in every mode" />{/if}
        {#if approval.answer === 'approve'}<Badge variant="tone" tone="success" size="sm" label="approved" />{/if}
        {#if approval.answer === 'reject'}<Badge variant="tone" tone="neutral" size="sm" label="declined" />{/if}
      </span>
    {/snippet}
    {#snippet actions()}
      <CopyButton value={approval.commands.join('\n')} title="Copy the command" />
    {/snippet}

    <MarkdownView text={fenced} />
    {#if approval.plan}
      <div class="steps"><ImberPlanCard snapshot={approval.plan} compact /></div>
      {#if waiting}
        <Button variant="ghost" size="xs" tooltip="Open this plan in the Plans window" onclick={() => plansStore.openModal(approval.plan?.id ?? '')}>Review in Plans</Button>
      {/if}
    {/if}
    <p class="effect">{approval.effect}</p>
    {#if approval.result}
      <p class="result" class:failed={!approval.result.ok}>{approval.result.ok ? 'Done' : 'Failed'} — {approval.result.detail}</p>
    {/if}

    {#snippet footer()}
      {#if waiting && promptId}
        <div class="decide">
          <Button variant="primary" size="sm" onclick={() => onDecide(promptId, 'approve')}>
            <Check size={12} /> Approve <Kbd keys={['Ctrl', 'Enter']} size="sm" />
          </Button>
          <Button variant="ghost" size="sm" onclick={() => onDecide(promptId, 'reject')}>
            <X size={12} /> Decline <Kbd keys={['Esc']} size="sm" />
          </Button>
        </div>
      {/if}
    {/snippet}
  </Card>
</div>

<style>
  .approval { min-width: 0; }
  .head { display: inline-flex; align-items: center; gap: 6px; font-size: var(--font-size-sm); }
  .head :global(.warn) { color: var(--danger); }
  .steps { margin-top: 6px; }
  .effect { margin: 6px 0 0; font-size: var(--font-size-xs); color: var(--text-secondary); }
  .result { margin: 6px 0 0; font-size: var(--font-size-xs); color: var(--success); }
  .result.failed { color: var(--danger); }
  .decide { display: flex; gap: 6px; }
</style>
