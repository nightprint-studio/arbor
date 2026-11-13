<script lang="ts">
  /**
   * Questions the assistant puts to you instead of guessing.
   *
   * The same shape as the prompts Claude Code shows. Each question is a row of choices — pick one, or
   * tick several — with "Other…" always there for what the options did not think of. When there are
   * several they are asked together and you move through them: a strip of tabs names each (a tick once
   * it is answered) and lets you jump to any, Back and Next walk them in order, and the last step is
   * the one that sends all the answers at once. Nothing is sent until then, so going back to change an
   * earlier answer costs nothing.
   *
   * Answered, the set shrinks to what was decided — each question with the choice beside it — and
   * stays in the thread as the reason the edits after it look the way they do.
   */
  import { MessageCircleQuestion, Check, ChevronLeft, ChevronRight } from 'lucide-svelte';
  import ChoiceList from '$lib/components/shared/ui/ChoiceList.svelte';
  import MarkdownView from '$lib/components/shared/ui/MarkdownView.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import AssistantStalePrompt from './AssistantStalePrompt.svelte';
  import type { QuestionSet } from './assistant-types';

  let { set, conversationId }: { set: QuestionSet; conversationId: string } = $props();
  const waiting = $derived(bennuAssistantStore.waitingOn(conversationId));

  const OTHER = '__other__';

  // What has been picked so far, per question — kept while you move between them. A question not yet
  // touched has no entry, so ChoiceList is bound through getter/setter pairs that read it as empty:
  // `bind:value={picked[id]}` hands it `undefined`, and Svelte throws (`props_invalid_value`) on a bind of
  // undefined to a prop with a fallback — the card dies mid-render and the message stops updating.
  let step = $state(0);
  let picked = $state<Record<string, string[]>>({});
  let typed = $state<Record<string, string>>({});

  const questions = $derived(set.questions);
  const current = $derived(questions[Math.min(step, questions.length - 1)]);
  const answered = $derived(set.answers !== undefined);
  const last = $derived(step >= questions.length - 1);

  /** A question counts as answered when something is picked, and "Other" has words in it. */
  function isDone(id: string): boolean {
    const p = picked[id] ?? [];
    if (p.length === 0) return false;
    return !p.includes(OTHER) || (typed[id] ?? '').trim().length > 0;
  }
  const allDone = $derived(questions.every((q) => isDone(q.id)));

  /** What a question's answer reads as: the labels, with "Other" replaced by what was typed. */
  function readable(id: string, values: string[], typedText: string): string[] {
    return values.map((v) => (v === OTHER ? typedText.trim() : v));
  }

  function submit() {
    if (!allDone) return;
    const answers: Record<string, string[]> = {};
    for (const q of questions) answers[q.id] = readable(q.id, picked[q.id] ?? [], typed[q.id] ?? '');
    bennuAssistantStore.answerQuestions(conversationId, set.id, answers);
  }

  function next() {
    if (last) submit();
    else step += 1;
  }
</script>

<div class="q" class:answered>
  <div class="head">
    <MessageCircleQuestion size={13} />
    <span class="title">{questions.length === 1 ? 'A question' : `${questions.length} questions`}</span>
    {#if answered}<span class="done"><Check size={11} /> Answered</span>{/if}
  </div>

  {#if answered}
    <!-- What was decided, and on what: each question as it was asked, the answer under it. -->
    <ol class="summary">
      {#each questions as q, i (q.id)}
        {@const answer = set.answers?.[q.id] ?? []}
        <li>
          <div class="qhead">{#if questions.length > 1}<span class="idx">{i + 1}</span>{/if}<span class="qname">{q.header}</span></div>
          <div class="asked"><MarkdownView text={q.text} size="xs" /></div>
          <div class="answer">
            {#if answer.length}<MarkdownView text={answer.join('\n\n')} size="sm" />{:else}<span class="none">No answer</span>{/if}
          </div>
        </li>
      {/each}
    </ol>
  {:else if !waiting}
    <!-- Nobody can answer any more, but what was asked stays readable: each question, numbered, with its options. -->
    <ol class="stale">
      {#each questions as q, i (q.id)}
        <li>
          <div class="qhead">{#if questions.length > 1}<span class="idx">{i + 1}</span>{/if}<span class="qname">{q.header}</span></div>
          <div class="text"><MarkdownView text={q.text} size="sm" /></div>
          {#if q.options.length}
            <ul class="opts">
              {#each q.options as o (o.label)}
                <li>
                  <span class="opt">{o.label}</span>
                  {#if o.description}<div class="why"><MarkdownView text={o.description} size="xs" /></div>{/if}
                </li>
              {/each}
            </ul>
          {/if}
        </li>
      {/each}
    </ol>
    <AssistantStalePrompt />
  {:else}
    {#if questions.length > 1}
      <div class="tabs" role="tablist" aria-label="Questions">
        {#each questions as q, i (q.id)}
          <button
            type="button"
            role="tab"
            class="tab"
            class:active={i === step}
            class:ok={isDone(q.id)}
            aria-selected={i === step}
            onclick={() => (step = i)}
          >
            <span class="n">{#if isDone(q.id)}<Check size={9} />{:else}{i + 1}{/if}</span>{q.header}
          </button>
        {/each}
      </div>
    {/if}

    {#key current.id}
      <div class="body">
        <div class="qhead">
          {#if questions.length > 1}<span class="idx">{step + 1}</span>{/if}
          <span class="qname">{current.header}</span>
          {#if questions.length > 1}<span class="of">{step + 1} of {questions.length}</span>{/if}
        </div>
        <div class="text"><MarkdownView text={current.text} size="sm" /></div>
        <ChoiceList
          name={`${set.id}-${current.id}`}
          options={current.options.map((o) => ({ value: o.label, label: o.label, description: o.description }))}
          multi={current.multi}
          bind:value={() => picked[current.id] ?? [], (v) => (picked[current.id] = v)}
          bind:other={() => typed[current.id] ?? '', (v) => (typed[current.id] = v)}
          onsubmit={() => { if (last ? allDone : isDone(current.id)) next(); }}
        >
          {#snippet description(o)}<MarkdownView text={o.description ?? ''} size="xs" />{/snippet}
        </ChoiceList>
      </div>
    {/key}

    <div class="foot">
      <span class="hint">{current.multi ? 'Pick any that apply.' : 'Pick one.'} Ctrl/⌘+Enter in your own answer goes on.</span>
      <span class="grow"></span>
      {#if step > 0}
        <Button variant="ghost" size="sm" onclick={() => (step -= 1)}><ChevronLeft size={12} /> Back</Button>
      {/if}
      <Button variant="primary" size="sm" disabled={last ? !allDone : !isDone(current.id)} onclick={next}>
        {#if last}Send answers{:else}Next <ChevronRight size={12} />{/if}
      </Button>
    </div>
  {/if}
</div>

<style>
  .q {
    display: flex; flex-direction: column; gap: 10px;
    padding: 10px 11px 9px;
    border: 1px solid color-mix(in srgb, var(--accent) 38%, var(--border-subtle));
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--accent) 5%, var(--bg-elevated));
  }
  .q.answered { border-color: var(--border-subtle); background: var(--bg-elevated); }

  .head { display: flex; align-items: center; gap: 7px; color: var(--accent); }
  .q.answered .head { color: var(--text-muted); }
  .title { font-size: var(--font-size-xs); font-weight: 600; }
  .done { margin-left: auto; display: inline-flex; align-items: center; gap: 4px; font-size: var(--font-size-2xs); color: var(--success); }

  .tabs { display: flex; gap: 4px; flex-wrap: wrap; }
  .tab {
    display: inline-flex; align-items: center; gap: 6px;
    padding: 3px 10px 3px 4px;
    border: 1px solid var(--border-subtle); border-radius: 999px;
    background: var(--bg-base); color: var(--text-muted);
    font-family: var(--font-ui-sans); font-size: var(--font-size-xs);
    cursor: pointer;
    transition: color var(--transition-fast), border-color var(--transition-fast), background var(--transition-fast);
  }
  .tab:hover { color: var(--text-primary); }
  .tab.active { color: var(--accent); border-color: var(--accent); background: var(--accent-subtle); }
  .n {
    display: inline-flex; align-items: center; justify-content: center;
    width: 16px; height: 16px; border-radius: 50%;
    font-size: var(--font-size-3xs); font-weight: 700;
    background: var(--bg-overlay); color: var(--text-muted);
  }
  .tab.active .n { background: var(--accent); color: var(--text-on-accent); }
  .tab.ok .n { background: var(--success); color: var(--text-on-accent); }

  .body { display: flex; flex-direction: column; gap: 9px; }
  .text { color: var(--text-primary); }

  /* The number and name of a question: the same badge as the tabs, so "2" reads the same everywhere. */
  .qhead { display: flex; align-items: center; gap: 7px; }
  .idx {
    display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0;
    width: 18px; height: 18px; border-radius: 50%;
    font-size: var(--font-size-2xs); font-weight: 700;
    background: var(--accent); color: var(--text-on-accent);
  }
  .qname { font-size: var(--font-size-xs); font-weight: 600; color: var(--text-primary); }
  .of { margin-left: auto; font-size: var(--font-size-2xs); color: var(--text-muted); }

  .stale { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 12px; }
  .stale > li { display: flex; flex-direction: column; gap: 6px; }
  .stale > li + li { padding-top: 10px; border-top: 1px solid var(--border-subtle); }
  .stale .idx { background: var(--bg-overlay); color: var(--text-muted); }
  .opts { margin: 0; padding: 0 0 0 25px; display: flex; flex-direction: column; gap: 4px; }
  .opts li { font-size: var(--font-size-xs); }
  .opt { color: var(--text-secondary); font-weight: 600; }
  .why { color: var(--text-muted); }

  .foot { display: flex; align-items: center; gap: 6px; }
  .grow { flex: 1; }
  .hint { font-size: var(--font-size-2xs); color: var(--text-muted); }

  .summary { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 10px; }
  .summary > li { display: flex; flex-direction: column; gap: 5px; }
  .summary > li + li { padding-top: 10px; border-top: 1px solid var(--border-subtle); }
  .summary .idx { background: var(--bg-overlay); color: var(--text-muted); }
  .asked { color: var(--text-muted); padding-left: 25px; }
  .answer {
    margin-left: 25px; padding: 6px 10px;
    border-left: 2px solid var(--success);
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    background: color-mix(in srgb, var(--success) 6%, var(--bg-base));
    color: var(--text-primary);
  }
  .none { font-size: var(--font-size-xs); color: var(--text-muted); font-style: italic; }
</style>
