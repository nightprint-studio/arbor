<script lang="ts">
  /**
   * The assistant asking to reach somewhere it may not.
   *
   * One prompt settles both questions — whether, and how far: **read only**, or **read and write**, for
   * that folder, for this session. The card says what the call that asked wants to do there — read, search,
   * write, create, delete — and the grant that lets it go on is the primary button: reading allowed for a call
   * that writes would only bring the question back. It never appears while the access is set to anywhere on
   * disk (that choice was the answer in advance), and never for the places the assistant may always use.
   * Answered, the card shrinks to what was decided and stays in the thread.
   */
  import { FolderOpen, Check, X, Eye, Pencil, Search, FilePlus, Trash2 } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import AssistantStalePrompt from './AssistantStalePrompt.svelte';
  import type { AccessAnswer, AccessNeed, AccessRequest } from './assistant-types';

  let { request, conversationId }: { request: AccessRequest; conversationId: string } = $props();
  const waiting = $derived(bennuAssistantStore.waitingOn(conversationId));

  const answer = (a: AccessAnswer) => bennuAssistantStore.answerAccess(conversationId, request.id, a);

  const RESULT: Record<AccessAnswer, { label: string; tone: 'success' | 'info' | 'error' }> = {
    denied: { label: 'Denied', tone: 'error' },
    read: { label: 'Reading allowed', tone: 'info' },
    write: { label: 'Reading and writing allowed', tone: 'success' },
  };
  // Writing in a folder already open for reading: the reading is settled, so the only question left is
  // whether to allow writing — and "no" is final for the session, the agent is told not to ask again.
  const upgrade = $derived(!!request.upgrade);
  const NEED: Record<AccessNeed, { label: string; icon: typeof Eye; writes: boolean }> = {
    read: { label: 'to read a file', icon: Eye, writes: false },
    list: { label: 'to list or search', icon: Search, writes: false },
    write: { label: 'to change a file', icon: Pencil, writes: true },
    create: { label: 'to create a file', icon: FilePlus, writes: true },
    delete: { label: 'to delete', icon: Trash2, writes: true },
  };
  const need = $derived(request.needs ? NEED[request.needs] : null);
  // Without the word (an older card), writing stays the primary, as it was.
  const writes = $derived(need?.writes ?? true);
  const result = $derived(
    request.answer
      ? upgrade && request.answer !== 'write'
        ? { label: 'Kept read-only', tone: 'info' as const }
        : RESULT[request.answer]
      : null,
  );
</script>

<div class="ask" class:done={request.answer !== undefined}>
  <div class="head">
    <FolderOpen size={13} />
    <span class="title">{upgrade ? 'Wants to write in a folder opened for reading' : 'Wants to reach a folder outside the allowed scope'}</span>
    {#if result}<Badge variant="tone" tone={result.tone} size="sm">{result.label}</Badge>{/if}
  </div>

  {#if need}
    {@const Icon = need.icon}
    <span class="need" class:writes>
      <Icon size={11} />
      <span>Wants it <strong>{need.label}</strong> — {writes ? 'reading alone will not be enough' : 'reading is enough'}</span>
    </span>
  {/if}
  <code class="path">{request.folder}</code>
  {#if request.file}<span class="why">Because of <code>{request.file}</code></span>{/if}
  <p class="reason">{request.reason}</p>

  {#if !request.answer && !waiting}
    <AssistantStalePrompt />
  {:else if !request.answer}
    <div class="buttons">
      {#if upgrade}
        <Button variant="secondary" size="sm" onclick={() => answer('denied')}><Eye size={12} /> Keep read-only</Button>
        <span class="grow"></span>
        <Button variant="primary" size="sm" onclick={() => answer('write')}><Pencil size={12} /> Allow writing</Button>
      {:else}
        <Button variant="ghost" size="sm" onclick={() => answer('denied')}><X size={12} /> Deny</Button>
        <span class="grow"></span>
        <Button variant={writes ? 'secondary' : 'primary'} size="sm" onclick={() => answer('read')}><Eye size={12} /> Allow reading</Button>
        <Button variant={writes ? 'primary' : 'secondary'} size="sm" onclick={() => answer('write')}><Pencil size={12} /> Allow reading &amp; writing</Button>
      {/if}
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
  .need {
    display: inline-flex; align-items: center; gap: 6px; align-self: flex-start;
    padding: 2px 8px; border-radius: var(--radius-sm);
    font-size: var(--font-size-2xs); color: var(--info);
    background: color-mix(in srgb, var(--info) 12%, transparent);
  }
  .need.writes { color: var(--warning); background: color-mix(in srgb, var(--warning) 14%, transparent); }
  .need strong { font-weight: 600; }
  .done .need { display: none; }
  .path { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); overflow-wrap: anywhere; }
  .why { font-size: var(--font-size-2xs); color: var(--text-muted); }
  .why code { font-family: var(--font-code); }
  .reason { margin: 0; font-size: var(--font-size-xs); line-height: 1.55; color: var(--text-secondary); }
  .buttons { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; padding-top: 2px; }
  .grow { flex: 1; }
  .note { display: inline-flex; align-items: center; gap: 5px; font-size: var(--font-size-2xs); color: var(--text-muted); }
</style>
