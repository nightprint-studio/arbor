<script lang="ts">
  /**
   * The plan the assistant keeps while it works on something with several steps.
   *
   * It writes the steps down at the start and ticks them as it goes, and the card shows where it is: the
   * headline names the step being worked on ("Step 2 of 6") above the numbered steps (`AssistantPlanSteps`, which
   * the plan bar above the thread draws too). The bar and percentage are steps done over steps planned — an honest
   * count, not a forecast: a plan that grows as the work turns up more to do goes down in percent, which is what
   * actually happened.
   */
  import StackedBar from '$lib/components/shared/ui/StackedBar.svelte';
  import AssistantPlanIcon from './AssistantPlanIcon.svelte';
  import AssistantPlanSteps from './AssistantPlanSteps.svelte';
  import { planHeadline, planProgress, planSegments } from './assistant-plan';
  import type { PlanBlock } from './assistant-types';

  let { plan }: { plan: PlanBlock } = $props();

  const p = $derived(planProgress(plan.steps));
  const segments = $derived(planSegments(p));
</script>

<div class="plan" class:finished={p.finished}>
  <div class="head">
    <AssistantPlanIcon done={p.done} total={p.total} active={p.inProgress > 0} />
    <span class="title">Plan</span>
    <span class="where">{planHeadline(p)}</span>
    <span class="count">{p.percent}%</span>
  </div>
  <StackedBar {segments} max={p.total} height={4} ariaLabel="Plan progress" />
  <AssistantPlanSteps steps={plan.steps} />
</div>

<style>
  .plan { display: flex; flex-direction: column; gap: 8px; padding: 10px 12px; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); background: var(--bg-elevated); }
  .head { display: flex; align-items: center; gap: 7px; color: var(--accent); }
  .title { font-size: var(--font-size-xs); font-weight: 600; color: var(--text-primary); }
  .where { flex: 1; font-size: var(--font-size-xs); color: var(--text-secondary); }
  .count { font-size: var(--font-size-2xs); font-variant-numeric: tabular-nums; color: var(--text-muted); }
  .finished .head, .finished .where { color: var(--success); }
</style>
