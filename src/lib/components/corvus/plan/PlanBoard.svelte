<script lang="ts">
  /**
   * The plan as a board, read left to right: phases (steps that start together), the gates
   * between them, then `finally`. Purely presentational — selection and every edit are the
   * modal's, so the board stays the same whether the plan is being composed or watched.
   */
  import { Plus } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import type { Plan } from '$lib/types/corvus/plan';
  import { FINALLY } from './plan-edit';
  import PlanGateNode from './PlanGateNode.svelte';
  import PlanStepCard from './PlanStepCard.svelte';

  interface Props {
    plan: Plan;
    /** A step id, or `gate:<phase id>`. */
    selection: string | null;
    interactive: boolean;
    onSelect: (id: string) => void;
    onAddStep: (columnId: string) => void;
    onAddPhase: () => void;
    onEditGate: (phaseId: string, at: { x: number; y: number }) => void;
    onReleaseGate: (phaseId: string) => void;
    onRetry: (stepId: string) => void;
    onSkip: (stepId: string) => void;
  }

  let {
    plan, selection, interactive, onSelect, onAddStep, onAddPhase, onEditGate, onReleaseGate, onRetry, onSkip,
  }: Props = $props();
</script>

{#snippet column(id: string, title: string, number: string, tag: string, steps: Plan['finally'], dashed: boolean)}
  <section class="phase" class:dashed>
    <header class="head">
      <span class="num">{number}</span>
      <span class="name">{title}</span>
      <span class="tag">{tag}</span>
    </header>
    <div class="steps">
      {#each steps as step (step.id)}
        <PlanStepCard
          {step}
          {interactive}
          selected={selection === step.id}
          onSelect={() => onSelect(step.id)}
          onRetry={() => onRetry(step.id)}
          onSkip={() => onSkip(step.id)}
        />
      {/each}
      {#if interactive}
        <Button variant="outline" size="sm" block onclick={() => onAddStep(id)}>
          <Plus size={13} /> Add step <Kbd keys={['A']} />
        </Button>
      {/if}
    </div>
  </section>
{/snippet}

<div class="board">
  {#each plan.phases as phase, i (phase.id)}
    {#if i > 0}
      <PlanGateNode
        {plan}
        gate={phase.gate}
        selected={selection === `gate:${phase.id}`}
        {interactive}
        onEdit={(at) => onEditGate(phase.id, at)}
        onRelease={() => onReleaseGate(phase.id)}
      />
    {/if}
    {@render column(phase.id, phase.name, String(i + 1), 'together', phase.steps, false)}
  {/each}

  {#if interactive}
    <div class="add-phase">
      <Button variant="outline" size="md" onclick={onAddPhase}>
        <Plus size={14} /> Add phase
      </Button>
      <span class="hint">starts after the previous one</span>
    </div>
  {/if}

  <span class="finally-gap"></span>
  {@render column(FINALLY, 'Finally', '∞', 'always runs', plan.finally, true)}
</div>

<style>
  .board {
    display: flex;
    align-items: flex-start;
    gap: 0;
    padding: 12px 20px 28px;
    width: max-content;
    min-height: 100%;
  }
  .phase { width: 264px; flex: none; }
  .head { display: flex; align-items: center; gap: 8px; height: 34px; padding: 0 2px; }
  .num {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--bg-overlay);
    font: 600 11px var(--font-ui-mono, monospace);
    color: var(--text-muted);
  }
  .name { font-weight: 600; font-size: var(--font-size-sm); }
  .tag {
    margin-left: auto;
    padding: 1px 7px;
    border: 1px solid var(--border-subtle);
    border-radius: 99px;
    font-size: 10.5px;
    color: var(--text-muted);
  }
  .steps {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 96px;
    padding: 8px;
    border: 1px solid var(--border-subtle);
    border-radius: 12px;
    background: color-mix(in srgb, var(--bg-elevated) 55%, transparent);
  }
  .dashed .steps { border-style: dashed; border-color: var(--border); background: transparent; }

  .add-phase {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    width: 170px;
    flex: none;
    margin: 34px 0 0 16px;
  }
  .hint { font-size: 11px; color: var(--text-muted); }
  .finally-gap { width: 28px; flex: none; }
</style>
