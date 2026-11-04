<script lang="ts">
  /**
   * A plan in the Imber thread: its phases and steps, each step's state and outcome, and where the plan stands.
   *
   * The card keeps a snapshot of the plan from when Imber made it, and shows the **live** one whenever the
   * Plans store has it — the executor publishes every step as it finishes, so a plan Imber started shows its
   * progress right here. A failed step offers Retry / Skip, a paused plan Resume, a running one Stop; the same
   * verbs the Plan window has, and the same handlers behind them.
   */
  import { ListChecks, Pause, Play, RotateCw, SkipForward, Square } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Card from '$lib/components/shared/ui/Card.svelte';
  import { plansStore } from '$lib/stores/corvus/plans.svelte';
  import type { Plan, PlanStep, StepStatus } from '$lib/types/corvus/plan';
  import { gateLabel, STATUS_LABEL, STEP_META, stepDetail } from '$lib/components/corvus/plan/plan-meta';
  import { headline, isActive, livePlan, stepOutcome } from './imber-plan';

  let { snapshot, compact = false }: {
    snapshot: Plan;
    /** Just the steps: inside an approval card, where the buttons are the approval's. */
    compact?: boolean;
  } = $props();

  const plan = $derived(livePlan(snapshot, plansStore.plans));

  const TONE: Record<StepStatus, 'neutral' | 'info' | 'success' | 'error' | 'warning'> = {
    queued: 'neutral', held: 'warning', running: 'info', done: 'success', failed: 'error', skipped: 'neutral',
  };

  const stepName = (s: PlanStep) => `${STEP_META[s.kind].label} · ${stepDetail(s)}`;
</script>

<div class="plan">
  <Card variant="subtle" padding="sm">
    {#snippet header()}
      <span class="head">
        <ListChecks size={13} />
        <strong>{plan.name}</strong>
        <span class="state">{headline(plan)}</span>
      </span>
    {/snippet}

    {#each plan.phases as phase, i (phase.id)}
      <div class="phase">
        <div class="phase-name">
          {phase.name}
          {#if i > 0}<span class="gate">— {gateLabel(plan, phase.gate)}{phase.gate.state === 'waiting' ? ' (waiting)' : ''}</span>{/if}
        </div>
        {#each phase.steps as step (step.id)}
          {@const outcome = stepOutcome(step)}
          {@const Icon = STEP_META[step.kind].icon}
          <div class="step">
            <span class="icon" style:color={STEP_META[step.kind].color}><Icon size={12} /></span>
            <span class="what" title={stepName(step)}>{stepName(step)} <em>{step.repo.name}</em></span>
            <Badge variant="tone" tone={TONE[step.status]} size="sm" label={STATUS_LABEL[step.status]} />
            {#if outcome}<span class="outcome" class:bad={step.status === 'failed'} title={outcome}>{outcome}</span>{/if}
            {#if !compact && step.status === 'failed'}
              <Button variant="ghost" size="xs" tooltip="Queue it again; resume the plan to run it" onclick={() => void plansStore.retryStep(plan.id, step.id)}><RotateCw size={11} /> Retry</Button>
              <Button variant="ghost" size="xs" tooltip="Give up on it: later phases stop waiting" onclick={() => void plansStore.skipStep(plan.id, step.id)}><SkipForward size={11} /> Skip</Button>
            {/if}
          </div>
        {/each}
      </div>
    {/each}
    {#if plan.finally.length > 0}
      <div class="phase">
        <div class="phase-name">Finally</div>
        {#each plan.finally as step (step.id)}
          {@const Icon = STEP_META[step.kind].icon}
          <div class="step">
            <span class="icon" style:color={STEP_META[step.kind].color}><Icon size={12} /></span>
            <span class="what" title={stepName(step)}>{stepName(step)} <em>{step.repo.name}</em></span>
            <Badge variant="tone" tone={TONE[step.status]} size="sm" label={STATUS_LABEL[step.status]} />
          </div>
        {/each}
      </div>
    {/if}

    {#snippet footer()}
      {#if !compact}
        <div class="actions">
          <Button variant="outline" size="sm" onclick={() => plansStore.openModal(plan.id)}>Open in Plans</Button>
          {#if plan.status === 'paused'}
            <Button variant="outline" size="sm" tooltip="Continue from where it stopped" onclick={() => void plansStore.run(plan.id)}><Play size={11} /> Resume</Button>
          {/if}
          {#if isActive(plan)}
            <Button variant="ghost" size="sm" onclick={() => void plansStore.stop(plan.id)}>
              {#if plan.status === 'running'}<Square size={11} /> Stop{:else}<Pause size={11} /> Stop &amp; finish{/if}
            </Button>
          {/if}
        </div>
      {/if}
    {/snippet}
  </Card>
</div>

<style>
  .plan { min-width: 0; }
  .head { display: inline-flex; align-items: center; gap: 6px; font-size: var(--font-size-sm); min-width: 0; }
  .state { color: var(--text-muted); font-size: var(--font-size-xs); }
  .phase { display: flex; flex-direction: column; gap: 3px; margin-bottom: 6px; }
  .phase-name { font-size: var(--font-size-xs); color: var(--text-secondary); font-weight: 600; }
  .gate { font-weight: 400; color: var(--text-muted); }
  .step { display: flex; align-items: center; gap: 6px; min-width: 0; font-size: var(--font-size-xs); }
  .icon { display: inline-flex; flex-shrink: 0; }
  .what { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .what em { color: var(--text-muted); font-style: normal; }
  .outcome { max-width: 40%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-muted); }
  .outcome.bad { color: var(--danger); }
  .actions { display: flex; gap: 6px; flex-wrap: wrap; }
</style>
