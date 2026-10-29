<script lang="ts">
  /**
   * One message of the conversation.
   *
   * Yours is a soft bubble on the right, the way it is in every chat — it is short and it is yours, so
   * it can be small. The assistant's has no bubble: it is the content, and a frame around paragraphs
   * and tool receipts would only add edges. A quiet line above names who wrote it and when, and the
   * blocks follow in order: text, code, the tools it used — gathered into one line that opens on
   * demand, because the receipts matter less than the answer — and the edits it proposes. Hovering the
   * message reveals what you can do with it.
   */
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { Sparkles, Copy, ChevronRight, FilePen } from 'lucide-svelte';
  import AssistantToolRow from './AssistantToolRow.svelte';
  import AssistantProposal from './AssistantProposal.svelte';
  import AssistantCode from './AssistantCode.svelte';
  import AssistantQuestions from './AssistantQuestions.svelte';
  import AssistantAccessRequest from './AssistantAccessRequest.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import AssistantConfirm from './AssistantConfirm.svelte';
  import AssistantPlan from './AssistantPlan.svelte';
  import AssistantText from './AssistantText.svelte';
  import AssistantDocModal from './AssistantDocModal.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import type { AssistantMessage, AssistantMode, MessageBlock, ToolCall } from './assistant-types';

  let {
    message,
    conversationId,
    mode,
    last = false,
  }: { message: AssistantMessage; conversationId: string; mode: AssistantMode; last?: boolean } = $props();

  const store = bennuAssistantStore;
  let docOpen = $state(false);

  /** `code` spans become `<code>` and **bold** becomes `<strong>`; everything else stays text. No HTML is
   *  ever injected. */
  function inline(text: string): { code: boolean; bold: boolean; text: string }[] {
    return text.split(/(`[^`]+`|\*\*[^*]+\*\*)/g).filter(Boolean).map((part) => {
      if (part.length > 1 && part.startsWith('`') && part.endsWith('`')) return { code: true, bold: false, text: part.slice(1, -1) };
      if (part.length > 4 && part.startsWith('**') && part.endsWith('**')) return { code: false, bold: true, text: part.slice(2, -2) };
      return { code: false, bold: false, text: part };
    });
  }

  type Shown = Exclude<MessageBlock, { type: 'tool' }> | { type: 'tools'; calls: ToolCall[] };

  // Runs of tool calls fold into one entry: "what it did on the way" is one line, opened if you care.
  const shown = $derived.by<Shown[]>(() => {
    const out: Shown[] = [];
    for (const b of message.blocks) {
      if (b.type !== 'tool') { out.push(b); continue; }
      const last = out.at(-1);
      if (last?.type === 'tools') last.calls.push(b.call);
      else out.push({ type: 'tools', calls: [b.call] });
    }
    return out;
  });

  const KIND_WORD = { read: ['Read', 'files'], search: ['Searched', 'times'], edit: ['Edited', 'files'], validate: ['Checked', 'files'], refactor: ['Planned', 'refactorings'] } as const;

  /** `Read 2 files · Searched once · Checked 1 file` — the receipts, in one breath. */
  function summary(calls: ToolCall[]): string {
    const counts = new Map<ToolCall['kind'], number>();
    for (const c of calls.filter((x) => x.ok !== false)) counts.set(c.kind, (counts.get(c.kind) ?? 0) + 1);
    const failed = calls.filter((x) => x.ok === false).length;
    return [...counts].map(([kind, n]) => {
      const [verb, noun] = KIND_WORD[kind];
      if (kind === 'search') return n === 1 ? `${verb} once` : `${verb} ${n} times`;
      return `${verb} ${n} ${n === 1 ? noun.slice(0, -1) : noun}`;
    }).concat(failed ? [`${failed} refused`] : []).join(' · ');
  }

  const modelName = $derived(
    bennuAssistantStore.models.find((m) => m.id === message.model)?.name ?? '',
  );
  const time = $derived(new Date(message.at).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' }));

  const plainText = $derived(message.blocks.flatMap((b) => (b.type === 'text' ? [b.text] : b.type === 'code' ? [b.code] : [])).join('\n\n'));
</script>

<div class="msg" in:fly={{ y: 8, duration: animStore.dFast, easing: cubicOut }}>
  {#if message.role === 'user'}
    <div class="user">
      <div class="user-actions">
        <IconButton tooltip="Copy" size={22} onclick={() => copyToClipboard(plainText, { successToast: 'Copied' })}><Copy size={12} /></IconButton>
      </div>
      {#each message.blocks as b}
        {#if b.type === 'text'}
          <p class="bubble">{#each inline(b.text) as s}{#if s.code}<code>{s.text}</code>{:else if s.bold}<strong>{s.text}</strong>{:else}{s.text}{/if}{/each}</p>
        {/if}
      {/each}
    </div>
  {:else}
    <div class="assistant">
      <div class="who">
        <span class="avatar" class:asst-live={last && store.running}><Sparkles size={10} /></span>
        <span class="name">{modelName || 'Assistant'}</span>
        <span class="when">{time}</span>
      </div>

      <div class="blocks">
        {#each shown as b, i}
          {#if b.type === 'text'}
            <div class="text"><AssistantText text={b.text} live={last && store.running} />{#if last && store.running && i === shown.length - 1}<span class="asst-caret" aria-hidden="true"></span>{/if}</div>
          {:else if b.type === 'questions'}
            <AssistantQuestions set={b.set} {conversationId} />
          {:else if b.type === 'plan'}
            <AssistantPlan plan={b.plan} />
          {:else if b.type === 'confirm'}
            <AssistantConfirm confirm={b.confirm} {conversationId} />
          {:else if b.type === 'notice'}
            <Alert variant={b.level} text={b.text} compact />
          {:else if b.type === 'access'}
            <AssistantAccessRequest request={b.request} {conversationId} />
          {:else if b.type === 'code'}
            <AssistantCode language={b.language} code={b.code} />
          {:else if b.type === 'tools'}
            <div class="tools asst-enter">
              <Collapsible>
                {#snippet header({ open })}
                  <div class="tools-head" class:open>
                    <ChevronRight size={11} class="chev" />
                    <span>{summary(b.calls)}</span>
                  </div>
                {/snippet}
                <div class="tools-list">
                  {#each b.calls as call (call.id)}<AssistantToolRow {call} />{/each}
                </div>
              </Collapsible>
            </div>
          {:else if b.type === 'proposal'}
            <AssistantProposal proposal={b.proposal} {conversationId} {mode} />
          {/if}
        {/each}
      </div>

      <div class="hover">
        <IconButton tooltip="Copy" size={22} onclick={() => copyToClipboard(plainText, { successToast: 'Copied' })}><Copy size={12} /></IconButton>
        <IconButton tooltip="Open as a Markdown document — edit, copy or save" size={22} onclick={() => (docOpen = true)}><FilePen size={12} /></IconButton>
      </div>
    </div>
  {/if}
</div>

{#if docOpen}
  <AssistantDocModal text={plainText} onClose={() => (docOpen = false)} />
{/if}

<style>
  .user { display: flex; justify-content: flex-end; align-items: flex-start; gap: 4px; }
  .user-actions { order: -1; opacity: 0; transition: opacity var(--transition-fast); align-self: center; }
  .user:hover .user-actions, .user:focus-within .user-actions { opacity: 1; }
  .bubble {
    margin: 0; max-width: 90%;
    padding: 8px 12px;
    border-radius: 14px 14px 4px 14px;
    background: color-mix(in srgb, var(--accent) 13%, var(--bg-elevated));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 16%, transparent);
    color: var(--text-primary);
    font-size: var(--font-size-sm); line-height: 1.5;
    overflow-wrap: anywhere;
    user-select: text;
  }

  .assistant { display: flex; flex-direction: column; gap: 7px; }
  .who { display: flex; align-items: center; gap: 7px; }
  .avatar {
    display: flex; align-items: center; justify-content: center;
    width: 18px; height: 18px; border-radius: 50%;
    color: var(--text-on-accent);
    background: linear-gradient(140deg, var(--accent), color-mix(in srgb, var(--accent) 55%, var(--info)));
    box-shadow: 0 2px 8px color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .name { font-size: var(--font-size-xs); font-weight: 600; color: var(--text-primary); }
  .when { font-size: var(--font-size-2xs); color: var(--text-muted); }

  .blocks { display: flex; flex-direction: column; gap: 8px; padding-left: 25px; }
  /* The answer is the point of the thread, so it is a surface of its own at full contrast — not a grey
     paragraph between receipts. Line breaks are kept: a model's answer is written in paragraphs and lists. */
  .text {
    margin: 0;
    padding: 10px 13px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
    font-size: var(--font-size-md); line-height: 1.6;
    color: var(--text-primary);
    overflow-wrap: anywhere;
  }
  .bubble { white-space: pre-wrap; }
  .bubble strong { font-weight: 650; }
  code {
    font-family: var(--font-code); font-size: 0.92em;
    padding: 1px 5px; border-radius: 4px;
    background: var(--bg-overlay); color: var(--text-primary);
  }

  .tools-head {
    display: inline-flex; align-items: center; gap: 6px;
    padding: 2px 6px 2px 2px; border-radius: var(--radius-sm);
    font-size: var(--font-size-xs); color: var(--text-muted);
    transition: color var(--transition-fast), background var(--transition-fast);
  }
  .tools-head:hover { color: var(--text-secondary); background: var(--bg-hover); }
  .tools-head :global(.chev) { transition: transform var(--transition-fast); }
  .tools-head.open :global(.chev) { transform: rotate(90deg); }
  .tools-list { display: flex; flex-direction: column; gap: 3px; padding: 5px 0 2px; }

  /* What you can do with the answer, out of the way until the message is hovered or focused. */
  .hover {
    display: flex; align-items: center; gap: 1px; padding-left: 22px;
    opacity: 0; transform: translateY(-2px);
    transition: opacity var(--transition-fast), transform var(--transition-fast);
  }
  .assistant:hover .hover, .assistant:focus-within .hover { opacity: 1; transform: none; }
</style>
