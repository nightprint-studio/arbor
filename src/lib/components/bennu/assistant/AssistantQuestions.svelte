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
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import type { QuestionSet } from './assistant-types';

  let { set, conversationId }: { set: QuestionSet; conversationId: string } = $props();

  const OTHER = '__other__';

  // What has been picked so far, per question — kept while you move between them.
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
    <dl class="summary">
      {#each questions as q (q.id)}
        <div class="line">
          <dt>{q.header}</dt>
          <dd>{(set.answers?.[q.id] ?? []).join(' · ') || '—'}</dd>
        </div>
      {/each}
    </dl>
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
        <p class="text">{current.text}</p>
        <ChoiceList
          name={`${set.id}-${current.id}`}
          options={current.options.map((o) => ({ value: o.label, label: o.label, description: o.description }))}
          multi={current.multi}
          bind:value={picked[current.id]}
          bind:other={typed[current.id]}
        />
      </div>
    {/key}

    <div class="foot">
      <span class="hint">{current.multi ? 'Pick any that apply.' : 'Pick one.'}</span>
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
  .text { margin: 0; font-size: var(--font-size-sm); line-height: 1.55; color: var(--text-primary); }

  .foot { display: flex; align-items: center; gap: 6px; }
  .grow { flex: 1; }
  .hint { font-size: var(--font-size-2xs); color: var(--text-muted); }

  .summary { margin: 0; display: flex; flex-direction: column; gap: 6px; }
  .line { display: flex; gap: 10px; font-size: var(--font-size-xs); }
  dt { flex-shrink: 0; width: 64px; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.04em; font-size: var(--font-size-2xs); padding-top: 1px; }
  dd { margin: 0; color: var(--text-primary); line-height: 1.45; }
</style>
