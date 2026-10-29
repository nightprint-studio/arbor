<script lang="ts">
  /**
   * The plan the assistant keeps while it works on something with several steps.
   *
   * It writes the steps down at the start and ticks them as it goes, and the card shows where it is: each step
   * with its state, the one in progress marked and gently animated, and a bar with "3 of 7 · 43%". The
   * percentage is steps done over steps planned — an honest count, not a forecast: a plan that grows as the
   * work turns up more to do goes down in percent, which is what actually happened.
   */
  import { Check, Circle, ListTodo } from 'lucide-svelte';
  import StackedBar from '$lib/components/shared/ui/StackedBar.svelte';
  import { planProgress } from './assistant-plan';
  import type { PlanBlock } from './assistant-types';

  let { plan }: { plan: PlanBlock } = $props();

  const p = $derived(planProgress(plan.steps));
  const segments = $derived([
    { id: 'done', label: 'Done', value: p.done, tone: 'success' as const },
    { id: 'doing', label: 'In progress', value: p.inProgress, tone: 'accent' as const },
  ]);
</script>

<div class="plan" class:finished={p.finished}>
  <div class="head">
    <ListTodo size={13} />
    <span class="title">Plan</span>
    <span class="count">{p.done} of {p.total} · {p.percent}%</span>
  </div>
  <StackedBar {segments} max={p.total} height={4} ariaLabel="Plan progress" />
  <ol>
    {#each plan.steps as step, i (i)}
      <li class="s-{step.status}">
        <span class="mark">
          {#if step.status === 'completed'}<Check size={12} />
          {:else if step.status === 'in_progress'}<span class="asst-orb"></span>
          {:else}<Circle size={11} />{/if}
        </span>
        <span class="text">{step.text}</span>
      </li>
    {/each}
  </ol>
</div>

<style>
  .plan { display: flex; flex-direction: column; gap: 8px; padding: 10px 12px; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); background: var(--bg-elevated); }
  .head { display: flex; align-items: center; gap: 7px; color: var(--accent); }
  .title { flex: 1; font-size: var(--font-size-xs); font-weight: 600; color: var(--text-primary); }
  .count { font-size: var(--font-size-2xs); font-variant-numeric: tabular-nums; color: var(--text-muted); }
  .finished .head { color: var(--success); }
  ol { list-style: none; margin: 2px 0 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  li { display: flex; align-items: flex-start; gap: 8px; font-size: var(--font-size-sm); line-height: 1.45; color: var(--text-secondary); }
  .mark { display: grid; place-items: center; width: 16px; height: 18px; flex-shrink: 0; color: var(--text-muted); }
  .s-completed .mark { color: var(--success); }
  .s-completed .text { color: var(--text-muted); text-decoration: line-through; text-decoration-color: color-mix(in srgb, var(--text-muted) 50%, transparent); }
  .s-in_progress .text { color: var(--text-primary); font-weight: 550; }
  .text { min-width: 0; overflow-wrap: anywhere; }
</style>
