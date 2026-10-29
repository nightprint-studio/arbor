<script lang="ts">
  /**
   * A yes/no the assistant needs before it runs something that changes things or executes project code.
   * Answered, it shrinks to the decision and stays in the thread as a record.
   */
  import { ShieldQuestion, Check, X } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import type { ConfirmRequest } from './assistant-types';

  let { confirm, conversationId }: { confirm: ConfirmRequest; conversationId: string } = $props();
  const answer = (a: 'yes' | 'no') => bennuAssistantStore.answerConfirm(conversationId, confirm.id, a);
</script>

<div class="ask" class:danger={confirm.danger} class:done={confirm.answer !== undefined}>
  <div class="head">
    <ShieldQuestion size={13} />
    <span class="title">{confirm.title}</span>
    {#if confirm.answer}
      <Badge variant="tone" tone={confirm.answer === 'yes' ? 'success' : 'error'} size="sm">{confirm.answer === 'yes' ? 'Allowed' : 'Declined'}</Badge>
    {/if}
  </div>
  {#if confirm.detail}<p>{confirm.detail}</p>{/if}
  {#if !confirm.answer}
    <div class="buttons">
      <Button variant="ghost" size="sm" onclick={() => answer('no')}><X size={12} /> Decline</Button>
      <span class="grow"></span>
      <Button variant={confirm.danger ? 'danger' : 'primary'} size="sm" onclick={() => answer('yes')}><Check size={12} /> Allow</Button>
    </div>
  {/if}
</div>

<style>
  .ask { display: flex; flex-direction: column; gap: 7px; padding: 11px 12px; border: 1px solid color-mix(in srgb, var(--warning) 40%, var(--border-subtle)); border-radius: var(--radius-md); background: color-mix(in srgb, var(--warning) 7%, var(--bg-elevated)); }
  .ask.danger { border-color: color-mix(in srgb, var(--danger) 45%, var(--border-subtle)); background: color-mix(in srgb, var(--danger) 7%, var(--bg-elevated)); }
  .ask.done { border-color: var(--border-subtle); background: var(--bg-elevated); }
  .head { display: flex; align-items: center; gap: 7px; color: var(--warning); }
  .danger .head { color: var(--danger); }
  .done .head { color: var(--text-muted); }
  .title { flex: 1; font-size: var(--font-size-xs); font-weight: 600; color: var(--text-primary); }
  p { margin: 0; font-size: var(--font-size-xs); line-height: 1.55; color: var(--text-secondary); overflow-wrap: anywhere; }
  .buttons { display: flex; align-items: center; gap: 6px; }
  .grow { flex: 1; }
</style>
