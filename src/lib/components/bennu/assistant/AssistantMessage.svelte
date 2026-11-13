<script lang="ts">
  /**
   * One message of Bennu's conversation: the shared Imber message frame (the bubble, the author line, the
   * hover verbs) filled with Bennu's blocks — text, code, the tools it used gathered into one line, the
   * edits it proposes, questions, plans, access requests, pictures, sub-agents.
   */
  import { FilePen } from 'lucide-svelte';
  import AssistantProposal from './AssistantProposal.svelte';
  import MermaidDiagram from '$lib/components/shared/ui/MermaidDiagram.svelte';
  import AssistantQuestions from './AssistantQuestions.svelte';
  import AssistantAccessRequest from './AssistantAccessRequest.svelte';
  import AssistantAccessGrant from './AssistantAccessGrant.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import AssistantConfirm from './AssistantConfirm.svelte';
  import AssistantScriptRequest from './AssistantScriptRequest.svelte';
  import AssistantErrorNotice from './AssistantErrorNotice.svelte';
  import AssistantPlan from './AssistantPlan.svelte';
  import AssistantLiveSummary from './AssistantLiveSummary.svelte';
  import AssistantImage from './AssistantImage.svelte';
  import AssistantAttachments from './AssistantAttachments.svelte';
  import AssistantAgentCard from './AssistantAgentCard.svelte';
  import AssistantTimeSplit from './AssistantTimeSplit.svelte';
  import AssistantNote from './AssistantNote.svelte';
  import { splitTime } from '$lib/components/shared/internal/imber/imber-format';
  import ImberCode from '$lib/components/shared/internal/imber/ImberCode.svelte';
  import ImberMessageFrame from '$lib/components/shared/internal/imber/ImberMessageFrame.svelte';
  import ImberTextBlock from '$lib/components/shared/internal/imber/ImberTextBlock.svelte';
  import ImberToolsFold from '$lib/components/shared/internal/imber/ImberToolsFold.svelte';
  import type { Picture } from './assistant-images';
  import { messageAsText } from './assistant-copy';
  import AssistantDocModal from './AssistantDocModal.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import type { AssistantMessage, AssistantMode, MessageBlock, ToolCall, WorkItem } from './assistant-types';

  let {
    message,
    conversationId,
    mode,
    last = false,
    author = null,
  }: {
    message: AssistantMessage;
    conversationId: string;
    mode: AssistantMode;
    last?: boolean;
    /** Names the writer when it is not the assistant itself: a sub-agent's thread, in its own modal. */
    author?: { name: string; kind: 'imber' | 'agent'; badge?: { label: string; tone: 'info' | 'accent' | 'neutral' } | null; live?: boolean } | null;
  } = $props();

  const store = bennuAssistantStore;
  let docOpen = $state(false);

  type Shown = Exclude<MessageBlock, { type: 'tool' } | { type: 'step' }> | { type: 'tools'; calls: ToolCall[]; items: WorkItem[] };

  // Runs of tool calls fold into one entry: "what it did on the way" is one line, opened if you care. The
  // model's answers between them go in the same fold, as rows of their own.
  const shown = $derived.by<Shown[]>(() => {
    const out: Shown[] = [];
    for (const b of message.blocks) {
      if (b.type !== 'tool' && b.type !== 'step') { out.push(b); continue; }
      let last = out.at(-1);
      if (last?.type !== 'tools') {
        last = { type: 'tools', calls: [], items: [] };
        out.push(last);
      }
      if (b.type === 'tool') {
        last.calls.push(b.call);
        last.items.push({ kind: 'call', call: b.call });
      } else {
        last.items.push({ kind: 'step', step: b.step });
      }
    }
    return out;
  });

  /** The whole message's time split, for the line under it. */
  const timeSplit = $derived(
    splitTime(message.blocks.flatMap((b): WorkItem[] =>
      b.type === 'tool' ? [{ kind: 'call', call: b.call }] : b.type === 'step' ? [{ kind: 'step', step: b.step }] : [],
    )),
  );

  const KIND_WORD = { read: ['Read', 'files'], search: ['Searched', 'times'], edit: ['Edited', 'files'], validate: ['Checked', 'files'], refactor: ['Planned', 'refactorings'] } as const;

  /** `Read 2 files · Searched once · Checked 1 file` — the receipts, in one breath. */
  function summary(calls: ToolCall[]): string {
    const counts = new Map<ToolCall['kind'], number>();
    for (const c of calls.filter((x) => x.ok !== false)) counts.set(c.kind, (counts.get(c.kind) ?? 0) + 1);
    const failed = calls.filter((x) => x.ok === false).length;
    if (!calls.length) return 'Answered';
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

  /** Every kept picture of the message, in order: what the lightbox walks with the arrow keys. */
  const gallery = $derived<Picture[]>(
    message.blocks.flatMap((b): Picture[] =>
      b.type === 'screenshot' ? [{ path: b.shot.path, caption: b.shot.caption }]
      : b.type === 'image' ? [{ path: b.image.path, caption: b.image.caption }]
      : [],
    ),
  );

  const plainText = $derived(messageAsText(message.blocks));
</script>

<ImberMessageFrame
  role={message.role}
  name={author?.name ?? (modelName || 'Imber')}
  kind={author?.kind ?? 'imber'}
  badge={author?.badge ?? null}
  when={time}
  live={author ? !!author.live : last && store.running}
  {plainText}
  userText={message.role === 'user' ? message.blocks.flatMap((b) => (b.type === 'text' ? [b.text] : [])).join('\n\n') : ''}
>
  {#snippet userTop()}
    {#each message.blocks as b}
      {#if b.type === 'attachments'}<AssistantAttachments items={b.items} />{/if}
    {/each}
  {/snippet}

  {#each shown as b, i}
    {#if b.type === 'text'}
      <ImberTextBlock text={b.text} live={last && store.running} caret={last && store.running && i === shown.length - 1} />
    {:else if b.type === 'questions'}
      <AssistantQuestions set={b.set} {conversationId} />
    {:else if b.type === 'plan'}
      <AssistantPlan plan={b.plan} />
    {:else if b.type === 'confirm'}
      <AssistantConfirm confirm={b.confirm} {conversationId} />
    {:else if b.type === 'script'}
      <AssistantScriptRequest script={b.script} {conversationId} />
    {:else if b.type === 'notice' && b.level === 'error' && (b.detail || b.canRetry)}
      <AssistantErrorNotice text={b.text} detail={b.detail} canRetry={b.canRetry} {last} />
    {:else if b.type === 'notice'}
      <Alert variant={b.level} text={b.text} compact />
    {:else if b.type === 'access'}
      <AssistantAccessRequest request={b.request} {conversationId} />
    {:else if b.type === 'grant'}
      <AssistantAccessGrant grant={b.grant} />
    {:else if b.type === 'code'}
      <ImberCode language={b.language} code={b.code} />
    {:else if b.type === 'diagram'}
      <MermaidDiagram source={b.diagram.source} title={b.diagram.title} />
    {:else if b.type === 'tools'}
      <ImberToolsFold summary={summary(b.calls)} items={b.items} />
    {:else if b.type === 'proposal'}
      <AssistantProposal proposal={b.proposal} {conversationId} {mode} />
    {:else if b.type === 'live_summary'}
      <AssistantLiveSummary summary={b.summary} />
    {:else if b.type === 'screenshot'}
      <AssistantImage path={b.shot.path} caption={b.shot.caption} {gallery} />
    {:else if b.type === 'image'}
      <AssistantImage path={b.image.path} caption={b.image.caption} {gallery} />
    {:else if b.type === 'agent'}
      <AssistantAgentCard agent={b.agent} />
    {:else if b.type === 'note'}
      <AssistantNote text={b.note.text} />
    {/if}
  {/each}
  {#if message.role === 'assistant' && timeSplit.known}<AssistantTimeSplit split={timeSplit} />{/if}

  {#snippet actions()}
    <IconButton tooltip="Open as a Markdown document — edit, copy or save" size={22} onclick={() => (docOpen = true)}><FilePen size={12} /></IconButton>
  {/snippet}
</ImberMessageFrame>

{#if docOpen}
  <AssistantDocModal text={plainText} onClose={() => (docOpen = false)} />
{/if}
