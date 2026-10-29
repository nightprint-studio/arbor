<script lang="ts">
  /**
   * AssistantAgentCard — what a sub-agent left in the thread of the conversation that started it: the moment main
   * handed it a task. While it runs the card is alive (a border whose colour travels, an orb, a clock that moves);
   * when it ends it settles into a quiet card with the verdict. It says what the agent was asked, what it is
   * allowed to do, where it is, what it cost, and the first part of its answer. The card is updated in place as the
   * agent goes and is saved with the message, so it outlives the agent. "Open" shows the agent's own thread while
   * the app still has it.
   */
  import { onDestroy } from 'svelte';
  import { Bot, CornerDownLeft, Eye, Pencil, Search, TriangleAlert } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { bennuAgentsStore } from '$lib/stores/bennu/assistant-agents.svelte';
  import { elapsedLabel, statusInfo, toolsLabel } from './assistant-agents';
  import { formatCost } from './assistant-format';
  import type { AgentCard } from './assistant-types';

  let { agent }: { agent: AgentCard } = $props();

  const info = $derived(statusInfo(agent.status));
  const running = $derived(agent.status === 'running');
  const editor = $derived(agent.tools === 'read+edit');

  // The clock moves only while the agent runs; a finished card shows how long it took and stops.
  let now = $state(Date.now());
  let clock: ReturnType<typeof setInterval> | undefined;
  $effect(() => {
    clearInterval(clock);
    if (running) clock = setInterval(() => { now = Date.now(); }, 1000);
    return () => clearInterval(clock);
  });
  onDestroy(() => clearInterval(clock));

  const elapsed = $derived(elapsedLabel(agent.startedAt, agent.endedAt, now));
</script>

<div class="card" class:asst-edge={running} class:ended={!running} class:failed={agent.status === 'failed'}>
  <div class="head">
    <span class="mark" class:live={running} aria-hidden="true">
      {#if running}<span class="asst-orb"></span>{:else}<Bot size={14} />{/if}
    </span>
    <div class="who">
      <span class="title">{agent.title}</span>
      <span class="sub">
        sub-agent
        <span class="sep">·</span>
        {#if editor}<Pencil size={10} />{:else}<Search size={10} />{/if}
        {toolsLabel(agent.tools)}
      </span>
    </div>
    <Badge variant="tone" size="sm" tone={agent.waiting ? 'warning' : info.tone} dot={running} label={agent.waiting ? 'Waiting to retry' : info.label} />
    <Button variant="ghost" size="xs" tooltip="Open the agent's thread (read-only)" onclick={() => void bennuAgentsStore.open(agent.id)}>
      <Eye size={11} /> Open
    </Button>
  </div>

  {#if agent.task}<p class="task" title={agent.task}>{agent.task}</p>{/if}
  {#if agent.waiting}<p class="waiting">{agent.waiting}</p>{/if}
  {#if agent.error && agent.status === 'failed'}
    <p class="failure"><TriangleAlert size={11} /> {agent.error}</p>
  {/if}
  {#if agent.summary}
    <div class="reply">
      <span class="reply-mark"><CornerDownLeft size={11} /></span>
      <p class="summary">{agent.summary}</p>
    </div>
  {/if}

  <div class="foot">
    <span class="stat">{elapsed}</span>
    <span class="stat">{formatCost(agent.cost)}</span>
    <span class="stat model">{agent.model}</span>
  </div>
</div>

<style>
  .card {
    position: relative;
    display: flex; flex-direction: column; gap: 7px;
    padding: 10px 12px;
    border-radius: var(--radius-lg);
    transition: box-shadow var(--transition-fast), border-color var(--transition-fast);
  }
  /* Running: the animated border (assistant-motion.css) carries the card; a soft glow lifts it off the thread. */
  .card:not(.ended) { box-shadow: 0 4px 18px color-mix(in srgb, var(--accent) 14%, transparent); }
  /* Settled: a quiet border with a coloured left edge, the way the other receipts in the thread are drawn. */
  .card.ended {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-left: 2px solid var(--accent);
  }
  .card.ended:hover { border-color: color-mix(in srgb, var(--accent) 35%, var(--border-subtle)); }
  .card.failed { border-left-color: var(--danger); }

  .head { display: flex; align-items: center; gap: 9px; min-width: 0; }
  .mark {
    flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
    width: 26px; height: 26px; border-radius: var(--radius-md);
    color: var(--info);
    background: color-mix(in srgb, var(--info) 13%, transparent);
  }
  .mark.live { background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .who { display: flex; flex-direction: column; min-width: 0; flex: 1; }
  .title { font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sub { display: inline-flex; align-items: center; gap: 4px; font-size: var(--font-size-2xs); color: var(--text-muted); }
  .sep { opacity: 0.6; }

  /* What it was asked: a quotation, two lines at most. */
  .task {
    margin: 0; padding-left: 9px;
    border-left: 2px solid color-mix(in srgb, var(--text-muted) 35%, transparent);
    font-size: var(--font-size-xs); line-height: 1.5; color: var(--text-secondary);
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
  }
  .waiting { margin: 0; font-size: var(--font-size-xs); color: var(--warning); }
  .failure { display: flex; align-items: flex-start; gap: 5px; margin: 0; font-size: var(--font-size-xs); line-height: 1.5; color: var(--danger); overflow-wrap: anywhere; }

  /* What came back: indented under a return arrow, so the card reads as ask → answer. */
  .reply { display: flex; align-items: flex-start; gap: 7px; }
  .reply-mark { margin-top: 2px; color: var(--success); flex-shrink: 0; }
  .summary { margin: 0; font-size: var(--font-size-xs); line-height: 1.55; color: var(--text-primary); white-space: pre-wrap; overflow-wrap: anywhere; }

  .foot { display: flex; align-items: center; gap: 10px; }
  .stat { font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .model { margin-left: auto; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
