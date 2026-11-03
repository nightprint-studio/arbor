/** A plan orders remote git steps across repositories: phases, gates between them, a `finally`.
 *  Mirrors `crates/products/corvus/be/src/plan/model.rs` (snake_case, `kind` flattened in). */

export interface PlanRepo {
  path: string;
  name: string;
  workspace?: string | null;
}

export type StepSpec =
  | { kind: 'push'; branch: string }
  | {
      kind: 'create_mr';
      source: string;
      target: string;
      title: string;
      description: string;
      labels: string[];
      draft: boolean;
      auto_merge: boolean;
      squash: boolean;
      delete_branch: boolean;
    }
  | { kind: 'merge_mr'; from_step?: string | null; number?: number | null; squash: boolean; delete_branch: boolean }
  | { kind: 'run_pipeline'; branch: string; workflow_id?: string | null; variables: [string, string][] }
  | { kind: 'tag'; name: string; message?: string | null; target?: string | null }
  | { kind: 'pull' };

export type StepKind = StepSpec['kind'];
export type StepStatus = 'queued' | 'held' | 'running' | 'done' | 'failed' | 'skipped';

export type PlanStep = StepSpec & {
  id: string;
  repo: PlanRepo;
  status: StepStatus;
  error?: string | null;
  started_ms?: number | null;
  finished_ms?: number | null;
  result?: { number?: number; url?: string; run?: string } | null;
};

export type GateSpec =
  | { kind: 'phase_done' }
  | { kind: 'mr_merged'; step_id: string; poll_secs: number; timeout_secs: number }
  | { kind: 'delay'; secs: number }
  | { kind: 'manual' };

export type GateKind = GateSpec['kind'];
export type GateState = 'idle' | 'waiting' | 'open' | 'blocked';
export type PlanGate = GateSpec & { state: GateState; since_ms?: number | null };

export interface PlanPhase {
  id: string;
  name: string;
  gate: PlanGate;
  steps: PlanStep[];
}

export type PlanStatus = 'draft' | 'running' | 'paused' | 'done' | 'failed' | 'cancelled';

export interface Plan {
  id: string;
  name: string;
  phases: PlanPhase[];
  finally: PlanStep[];
  status: PlanStatus;
  pause_reason?: string | null;
  template: boolean;
  created_ms: number;
  updated_ms: number;
}
