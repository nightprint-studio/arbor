import {
  ArrowDownToLine, ArrowUpToLine, CheckCheck, Clock, GitMerge, GitPullRequest, Hand, Play, Tag,
} from 'lucide-svelte';
import type { IconComponent } from '$lib/types/icon';
import type { GateKind, Plan, PlanGate, PlanPhase, PlanStep, StepKind, StepStatus } from '$lib/types/corvus/plan';

export interface StepMeta {
  label: string;
  hint: string;
  icon: IconComponent;
  /** CSS colour of the icon tile. */
  color: string;
}

const as = (c: unknown) => c as IconComponent;

export const STEP_META: Record<StepKind, StepMeta> = {
  push:         { label: 'Push',         hint: 'Publish a branch to its remote',      icon: as(ArrowUpToLine),   color: 'var(--accent)' },
  create_mr:    { label: 'Create MR',    hint: 'Open a pull / merge request',         icon: as(GitPullRequest),  color: '#d2a8ff' },
  merge_mr:     { label: 'Merge MR',     hint: 'Merge a request opened earlier',      icon: as(GitMerge),        color: '#a371f7' },
  run_pipeline: { label: 'Run pipeline', hint: 'Start a CI pipeline on a branch',     icon: as(Play),            color: 'var(--success)' },
  tag:          { label: 'Tag & push',   hint: 'Create a tag and publish it',         icon: as(Tag),             color: 'var(--warning)' },
  pull:         { label: 'Pull',         hint: 'Update the local branch',             icon: as(ArrowDownToLine), color: 'var(--info, var(--accent))' },
};

export const STEP_KINDS = Object.keys(STEP_META) as StepKind[];

export const GATE_META: Record<GateKind, { label: string; icon: IconComponent }> = {
  phase_done: { label: 'When the previous phase is done', icon: as(CheckCheck) },
  mr_merged:  { label: 'When an MR is merged',            icon: as(GitMerge) },
  delay:      { label: 'After a delay',                   icon: as(Clock) },
  manual:     { label: 'When I say so',                   icon: as(Hand) },
};

/** The second line of a step card: what, exactly, it will do. */
export function stepDetail(step: PlanStep): string {
  switch (step.kind) {
    case 'push':         return step.branch;
    case 'create_mr':    return `${step.source} → ${step.target} · ${step.title}`;
    case 'merge_mr':     return step.number ? `!${step.number}` : 'the MR opened above';
    case 'run_pipeline': return step.branch;
    case 'tag':          return step.name;
    case 'pull':         return 'origin';
  }
}

/** Short text of a gate, for its label under the node. */
export function gateLabel(plan: Plan, gate: PlanGate): string {
  switch (gate.kind) {
    case 'phase_done': return 'Previous phase done';
    case 'delay':      return `After ${formatSecs(gate.secs)}`;
    case 'manual':     return 'Waits for your go-ahead';
    case 'mr_merged': {
      const mr = plan.phases.flatMap(p => p.steps).find(s => s.id === gate.step_id);
      return mr && mr.kind === 'create_mr' ? `MR “${mr.title}” merged` : 'MR merged';
    }
  }
}

export function formatSecs(secs: number): string {
  if (secs % 3600 === 0 && secs >= 3600) return `${secs / 3600} h`;
  if (secs % 60 === 0 && secs >= 60) return `${secs / 60} min`;
  return `${secs} s`;
}

export const STATUS_LABEL: Record<StepStatus, string> = {
  queued: 'Queued', held: 'Waiting', running: 'Running', done: 'Done', failed: 'Failed', skipped: 'Skipped',
};

/** A phase's overall state, for the compact strip in the Plans tab. */
export function phaseState(phase: PlanPhase): 'done' | 'running' | 'failed' | 'waiting' | 'idle' {
  if (phase.steps.some(s => s.status === 'failed')) return 'failed';
  if (phase.steps.some(s => s.status === 'running')) return 'running';
  if (phase.steps.length > 0 && phase.steps.every(s => s.status === 'done' || s.status === 'skipped')) return 'done';
  if (phase.gate.state === 'waiting') return 'waiting';
  return 'idle';
}

/** MR-creating steps in phases before `phaseIndex` — what a gate or a merge step can point at. */
export function earlierMrSteps(plan: Plan, phaseIndex: number): PlanStep[] {
  return plan.phases.slice(0, phaseIndex).flatMap(p => p.steps).filter(s => s.kind === 'create_mr');
}
