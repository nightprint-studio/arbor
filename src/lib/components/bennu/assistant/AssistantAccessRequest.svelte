<script lang="ts">
  /**
   * The assistant asking to reach somewhere it may not.
   *
   * One prompt settles both questions — whether, and how far: **read only**, or **read and write**, for
   * that folder, for this session. It never appears while the access is set to anywhere on disk (that
   * choice was the answer in advance), and never for the places the assistant may always use. Answered,
   * the card shrinks to what was decided and stays in the thread.
   */
  import { FolderOpen, Check, X, Eye, Pencil } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import type { AccessAnswer, AccessRequest } from './assistant-types';

  let { request, conversationId }: { request: AccessRequest; conversationId: string } = $props();

  const answer = (a: AccessAnswer) => bennuAssistantStore.answerAccess(conversationId, request.id, a);

  const RESULT: Record<AccessAnswer, { label: string; tone: 'success' | 'info' | 'error' }> = {
    denied: { label: 'Denied', tone: 'error' },
    read: { label: 'Reading allowed', tone: 'info' },
    write: { label: 'Reading and writing allowed', tone: 'success' },
  };
</script>

<div class="ask" class:done={request.answer !== undefined}>
  <div class="head">
    <FolderOpen size={13} />
    <span class="title">Wants to reach a folder outside the allowed scope</span>
    {#if request.answer}<Badge variant="tone" tone={RESULT[request.answer].tone} size="sm">{RESULT[request.answer].label}</Badge>{/if}
  </div>

  <code class="path">{request.folder}</code>
  {#if request.file}<span class="why">Because of <code>{request.file}</code></span>{/if}
  <p class="reason">{request.reason}</p>

  {#if !request.answer}
    <div class="buttons">
      <Button variant="ghost" size="sm" onclick={() => answer('denied')}><X size={12} /> Deny</Button>
      <span class="grow"></span>
      <Button variant="secondary" size="sm" onclick={() => answer('read')}><Eye size={12} /> Allow reading</Button>
      <Button variant="primary" size="sm" onclick={() => answer('write')}><Pencil size={12} /> Allow reading &amp; writing</Button>
    </div>
    <span class="note"><Check size={10} /> For this session, for this folder only.</span>
  {/if}
</div>

<style>
  .ask {
    display: flex; flex-direction: column; gap: 7px;
    padding: 11px 12px;
    border: 1px solid color-mix(in srgb, var(--warning) 40%, var(--border-subtle));
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--warning) 7%, var(--bg-elevated));
  }
  .ask.done { border-color: var(--border-subtle); background: var(--bg-elevated); }
  .head { display: flex; align-items: center; gap: 7px; color: var(--warning); }
  .done .head { color: var(--text-muted); }
  .title { flex: 1; font-size: var(--font-size-xs); font-weight: 600; color: var(--text-primary); }
  .path { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); overflow-wrap: anywhere; }
  .why { font-size: var(--font-size-2xs); color: var(--text-muted); }
  .why code { font-family: var(--font-code); }
  .reason { margin: 0; font-size: var(--font-size-xs); line-height: 1.55; color: var(--text-secondary); }
  .buttons { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; padding-top: 2px; }
  .grow { flex: 1; }
  .note { display: inline-flex; align-items: center; gap: 5px; font-size: var(--font-size-2xs); color: var(--text-muted); }
</style>
