import type { Plan, StepKind, StepSpec } from '$lib/types/corvus/plan';
import { earlierMrSteps } from './plan-meta';

/** A repo with the branch it has checked out — what a new step's defaults come from. */
export interface DefaultsRepo {
  path: string;
  name: string;
  branch: string | null;
}

export type Defaults = { spec: StepSpec } | { error: string };

/**
 * The step `kind` on `repo` with the values most likely wanted: the checked-out branch, the MR
 * opened earlier in the plan. `phaseIndex` is where the step will land (`plan.phases.length` for
 * `finally`), since a merge can only refer to an MR created before it.
 *
 * Not for `create_mr` (the real Create-PR form fills that one) nor `tag` (it needs a name).
 */
export function defaultSpec(kind: StepKind, repo: DefaultsRepo, plan: Plan | null, phaseIndex: number): Defaults {
  const branch = repo.branch;
  switch (kind) {
    case 'push':
      return branch ? { spec: { kind, branch } } : { error: `${repo.name}: no branch is checked out` };
    case 'run_pipeline':
      return branch
        ? { spec: { kind, branch, workflow_id: null, variables: [] } }
        : { error: `${repo.name}: no branch is checked out` };
    case 'pull':
      return { spec: { kind } };
    case 'merge_mr': {
      const mr = plan ? earlierMrSteps(plan, phaseIndex).find(s => s.repo.path === repo.path) : undefined;
      return mr
        ? { spec: { kind, from_step: mr.id, number: null, squash: false, delete_branch: false } }
        : { error: `${repo.name}: no MR is created for it in an earlier phase` };
    }
    case 'create_mr':
    case 'tag':
      return { error: `${kind} needs its own form` };
  }
}
