<script lang="ts">
  /**
   * A script of the project the assistant wants to run: what it is, the command, why, and its first lines — so the
   * answer is given having read it. Three answers: decline, run it once, or run it and add it to the programs
   * Imber may run (Settings › Imber › Tools and skills), after which the assistant calls it by name. A yes is for
   * the script as it is now: if it changes, the card comes back and says so. Answered, it shrinks to the decision.
   */
  import { FileCode2, Check, X, Plus } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import ImberCode from '$lib/components/shared/internal/imber/ImberCode.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import AssistantStalePrompt from './AssistantStalePrompt.svelte';
  import type { ScriptAnswer, ScriptRequest } from './assistant-types';

  let { script, conversationId }: { script: ScriptRequest; conversationId: string } = $props();
  const waiting = $derived(bennuAssistantStore.waitingOn(conversationId));
  const answer = (a: ScriptAnswer) => bennuAssistantStore.answerScript(conversationId, script, a);

  const DONE: Record<ScriptAnswer, { label: string; tone: 'success' | 'error' | 'info' }> = {
    once: { label: 'Ran once', tone: 'success' },
    add: { label: 'Ran and added to your programs', tone: 'info' },
    no: { label: 'Declined', tone: 'error' },
  };
  const language = $derived(script.path.split('.').pop()?.toLowerCase() ?? 'sh');
  const more = $derived(Math.max(0, script.lines - script.preview.split('\n').length));
</script>

<div class="ask" class:done={script.answer !== undefined}>
  <div class="head">
    <FileCode2 size={13} />
    <span class="title">Run the script <code>{script.label}</code>?</span>
    {#if script.answer}
      <Badge variant="tone" tone={DONE[script.answer].tone} size="sm">{DONE[script.answer].label}</Badge>
    {/if}
  </div>
  {#if script.why}<p>{script.why}</p>{/if}
  {#if script.changed}<p class="changed">It has changed since it last ran in this conversation.</p>{/if}
  <p class="cmd"><span>$</span> {script.command}{#if script.cwd && script.cwd !== '.'} <span class="cwd">in {script.cwd}</span>{/if}</p>
  {#if script.background}<p>It keeps running in the background until it ends or is stopped.</p>{/if}
  {#if !script.answer}
    <ImberCode {language} code={script.preview} />
    {#if more > 0}<p class="more">… and {more} more line{more === 1 ? '' : 's'}.</p>{/if}
  {/if}
  {#if !script.answer && !waiting}
    <AssistantStalePrompt />
  {:else if !script.answer}
    <div class="buttons">
      <Button variant="ghost" size="sm" onclick={() => answer('no')}><X size={12} /> Decline</Button>
      <span class="grow"></span>
      <Button variant="secondary" size="sm" onclick={() => answer('add')}><Plus size={12} /> Run and add to my programs</Button>
      <Button variant="danger" size="sm" onclick={() => answer('once')}><Check size={12} /> Run once</Button>
    </div>
  {/if}
</div>

<style>
  .ask { display: flex; flex-direction: column; gap: 7px; padding: 11px 12px; border: 1px solid color-mix(in srgb, var(--danger) 45%, var(--border-subtle)); border-radius: var(--radius-md); background: color-mix(in srgb, var(--danger) 7%, var(--bg-elevated)); }
  .ask.done { border-color: var(--border-subtle); background: var(--bg-elevated); }
  .head { display: flex; align-items: center; gap: 7px; color: var(--danger); }
  .done .head { color: var(--text-muted); }
  .title { flex: 1; min-width: 0; font-size: var(--font-size-xs); font-weight: 600; color: var(--text-primary); overflow-wrap: anywhere; }
  .title code { font-family: var(--font-code); font-weight: 500; }
  p { margin: 0; font-size: var(--font-size-xs); line-height: 1.55; color: var(--text-secondary); overflow-wrap: anywhere; }
  .changed { color: var(--warning); }
  .cmd { font-family: var(--font-code); color: var(--text-primary); }
  .cmd span:first-child, .cwd { color: var(--text-muted); }
  .more { color: var(--text-muted); }
  .buttons { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .grow { flex: 1; }
</style>
