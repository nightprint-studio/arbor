// TypeScript mirrors of src-tauri/src/pipeline/mod.rs and ci_client.rs types.

export type RunStatus = 'pending' | 'running' | 'paused' | 'success' | 'failed' | 'cancelled';

/**
 * Either `command` (shell) or `lua_op` (plugin-registered native Lua handler)
 * is set. `command` kept at the top level for backwards compatibility with
 * persisted runs that predate LuaOp support.
 */
export interface LuaOpSpec {
  plugin?: string;                   // defaults to pipeline's plugin
  op:      string;                   // registered via arbor.pipeline.register_op
  params:  unknown;                  // anything serialisable; passed to handler
}

export interface StepDef {
  id:             string;
  name:           string;
  command?:       string;            // present when StepKind=Shell
  lua_op?:        LuaOpSpec | null;  // present when StepKind=LuaOp
  /** Built-in op (file_exists, file_read, env, json_get, …). Resolved by the
   * runtime without spawning a shell. Takes precedence over lua_op/command. */
  builtin?:       BuiltinSpec | null;
  /** if/elif/else block. When set, the step is a "control step": the
   * runtime evaluates each branch's condition in order and runs the chosen
   * branch's nested steps. Takes precedence over every other kind. */
  if_block?:      IfBlock | null;
  cwd:            string | null;
  allow_failure:  boolean;
  /** Extra env vars overlaid on the parent process env (Shell steps only). */
  env?:           Record<string, string>;
  /** Optional capture spec — after the step finishes, the orchestrator
   * extracts the chosen `source`, runs it through `transforms`, and stores
   * the final value under `var` in the run's variable bag. */
  capture?:       CaptureSpec | null;
}

// ---------------------------------------------------------------------------
// Variables, capture, transforms (mirrors src-tauri/src/pipeline/vars.rs)
// ---------------------------------------------------------------------------

/** What part of a step's outcome feeds into the capture chain. */
export type CaptureSource =
  | 'stdout'
  | 'stderr'
  | 'exit_code'
  | 'success'
  | 'return_value';

export interface CaptureSpec {
  var:        string;
  source?:    CaptureSource;
  transforms?: Transform[];
}

/** Single declarative step in the capture transform chain. Each variant is
 * tagged via `kind`; payload fields differ by variant. */
export type Transform =
  | { kind: 'trim' }
  | { kind: 'lower' }
  | { kind: 'upper' }
  | { kind: 'lines' }
  | { kind: 'split'; sep: string }
  | { kind: 'join';  sep: string }
  | { kind: 'first' }
  | { kind: 'last'  }
  | { kind: 'nth';   n: number }
  | { kind: 'regex'; pattern: string; group?: number | null }
  | { kind: 'json_get';  path: string }
  | { kind: 'json_parse' }
  | { kind: 'to_bool' }
  | { kind: 'to_number' }
  | { kind: 'default'; value: string }
  | { kind: 'matches_bool'; pattern: string };

// ---------------------------------------------------------------------------
// Built-in ops (mirrors src-tauri/src/pipeline/builtin.rs)
// ---------------------------------------------------------------------------

export type BuiltinSpec =
  | { kind: 'file_exists'; path: string }
  | { kind: 'file_read';   path: string; max_bytes?: number | null }
  | { kind: 'env';         name: string; default?: string | null }
  | { kind: 'json_get';    source: string; path: string }
  | { kind: 'path_join';   parts: string[] }
  | { kind: 'set_var';     value: unknown }
  | { kind: 'echo';        message: string }
  | { kind: 'match';       target: string; pattern?: string | null; regex?: string | null };

// ---------------------------------------------------------------------------
// Conditions + IfBlock (mirrors src-tauri/src/pipeline/condition.rs)
// ---------------------------------------------------------------------------

export type CompareOp =
  | 'eq' | 'ne' | 'gt' | 'lt' | 'gte' | 'lte'
  | 'contains' | 'matches' | 'i_eq' | 'starts_with' | 'ends_with';

export type Condition =
  | { kind: 'compare'; left: string; op: CompareOp; right: string }
  | { kind: 'truthy';  value: string }
  | { kind: 'defined'; var: string }
  | { kind: 'empty';   value: string }
  | { kind: 'all_of';  conditions: Condition[] }
  | { kind: 'any_of';  conditions: Condition[] }
  | { kind: 'not';     condition: Condition }
  | { kind: 'always' }
  | { kind: 'never' }
  /** Free-form expression string parsed at runtime by the Rust parser
   *  (`pipeline/condition_parser.rs`). Plugins typically emit this shape so
   *  the user can author conditions as a single text input — see the
   *  `if_block` editor in source-export. */
  | { kind: 'expr';    expr: string };

export interface IfBranch {
  condition: Condition;
  steps:     StepDef[];
}

export interface IfBlock {
  branches:    IfBranch[];     // first = if; rest = elif's
  else_steps?: StepDef[];
}

export interface StageDef {
  id:    string;
  name:  string;
  steps: StepDef[];
}

export interface PipelineDef {
  id:          string;
  name:        string;
  plugin:      string;
  description: string | null;
  icon:        string | null;
  stages:      StageDef[];
  /**
   * When `true`, the host suppresses the automatic start-toast and
   * done-notification for runs of this pipeline. Plugins that already
   * surface their own "started/finished" messages set this to avoid
   * duplication. Default `false`.
   */
  silent?:     boolean;
}

export interface StepRun {
  def_id:      string;
  name:        string;
  status:      RunStatus;
  output:      string[];
  started_at:  number | null;
  finished_at: number | null;
  exit_code:   number | null;
  /** Nested step runs from an `if_block`-typed step. Empty for leaf steps. */
  children?:   StepRun[];
  /** Branch label that ran inside an `if_block` step (`if`, `elif #1`,
   * `else`). Empty for leaf steps. */
  branch?:     string;
}

export interface StageRun {
  def_id: string;
  name:   string;
  status: RunStatus;
  steps:  StepRun[];
}

export interface PipelineRun {
  id:          string;
  pipeline_id: string;
  plugin:      string;
  name:        string;
  status:      RunStatus;
  started_at:  number | null;
  finished_at: number | null;
  stages:      StageRun[];
  /**
   * Inherited from `PipelineDef.silent` at construction; overridable per
   * run via `arbor.pipeline.run{ silent = ... }`. When `true`, the
   * frontend skips the automatic start-toast and done-notification for
   * this run.
   */
  silent?:     boolean;
  /**
   * Only meaningful when `status === 'pending'`. `true` means the
   * orchestrator is parked waiting for a free concurrency slot
   * (`config.pipelines.max_concurrent_runs`) — the run hasn't started
   * executing steps yet but will as soon as a slot opens up. The panel
   * surfaces this as a "Queued" badge.
   */
  queued?:     boolean;
  /**
   * Per-run log filter level inherited from `PipelineDef.log_level`
   * (or overridden via `arbor.pipeline.run{ log_level = ... }`).
   * Pipeline logs below this level are dropped by the backend filter
   * before being broadcast to the panel.
   */
  log_level?:  'debug' | 'info' | 'warn' | 'error';
}

// ---------------------------------------------------------------------------
// CI/CD integration types (GitHub Actions + GitLab CI)
// ---------------------------------------------------------------------------

/** CI hosts Arbor knows about. The detector returns one of these literally;
 *  callers can narrow on the value (e.g. pass it to <BrandIcon brand>). */
export type CiProviderId = 'github' | 'gitlab';

export interface CiProviderInfo {
  provider:         CiProviderId;
  remote_url:       string;
  /** True when an OAuth token is stored for this provider. */
  has_token:        boolean;
  owner:            string | null;  // GitHub only
  repo_name:        string | null;  // GitHub only
  project_path:     string | null;  // GitLab only (e.g. "myorg/myrepo")
  gitlab_base_url:  string | null;  // GitLab only
}

export interface CiRun {
  id:             string;
  name:           string;
  /** "pending" | "running" | "success" | "failed" | "cancelled" */
  status:         string;
  branch:         string;
  commit_sha:     string;
  web_url:        string;
  /** ISO 8601 string — use new Date(created_at) */
  created_at:     string;
  provider:       CiProviderId;
  /** Wall-clock duration in seconds (null when still running or unknown). */
  duration_secs:  number | null;
}

/** A GitHub Actions workflow definition, used for the create-pipeline modal. */
export interface CiWorkflow {
  id:   string;
  name: string;
  /** Relative path in the repo, e.g. ".github/workflows/ci.yml" */
  path: string;
}

/** A single job within a CI pipeline run, returned by fetch_ci_jobs. */
export interface CiJob {
  id:             string;
  name:           string;
  /** Stage name; "Jobs" for GitHub (no native stage concept). */
  stage:          string;
  status:         string;
  duration_secs:  number | null;
  web_url:        string;
  allow_failure:  boolean;
}
