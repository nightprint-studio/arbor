# arbor-pipeline-core

Pipeline orchestrator: run state machine, step scheduling, persistence
of run history, event emission.

## Purpose

The runtime side of the pipeline subsystem. Today this lives mixed with
the types and registry inside `src-tauri/src/pipeline/`. Splitting
`api` from `core` keeps:

- the trait + DTOs in `arbor-pipeline-api` (lightweight, consumed by
  plugin runtime and `arbor`),
- the orchestrator in `arbor-pipeline-core` (heavier, depends on the
  scheduler and the hook dispatcher).

This keeps `arbor-plugin-core` free of the orchestrator's transitive
deps when all it needs is to register a `StepHandler`.

## Contents (planned)

- `Orchestrator` — singleton owning the active runs map, the persistence
  queue, and the registered `StepHandler` set (delegated to the
  `StepRegistry` from `arbor-pipeline-api`).
- `run_state_machine` — the pipeline lifecycle:
  `Pending → Running → (Passed | Failed | Aborted)`. Cancellation
  via `Arc<AtomicBool>` per run.
- `step_executor` — runs one step: validates params against the
  handler's `StepSchema`, fires `HOOK_ON_PIPELINE_STEP_DONE`, retries
  per the step's retry policy.
- `history` — persisted run history on disk at
  `pipeline_runs_dir()` (from `arbor-core::paths`). One JSON file per
  run with input params, output artifacts, log links.
- `events` — emit `pipeline:run-progress` and `pipeline:run-done` via
  `AppCtx`, so the frontend can show live progress.
- `scheduling` — wraps `arbor-scheduler` for time-triggered pipeline
  runs (cron / interval) declared in `PipelineConfig`.

## Depends on

- `arbor-core` — paths, `AppCtx`, `AppError`.
- `arbor-scheduler` — scheduled pipeline runs.
- `arbor-plugin-api` — `HookDispatcher` for lifecycle hooks.
- `arbor-pipeline-api` — DTOs, trait, registry, error type.

External: `tokio`, `serde`, `serde_json`, `chrono`, `uuid`, `dirs`,
`async-trait`, `tracing`, `thiserror`.

## Consumed by

- `arbor` (Tauri shell) — owns the singleton `Orchestrator`, exposes
  `pipeline_*` Tauri commands (`pipeline_run`, `pipeline_abort`,
  `pipeline_get_run`, `pipeline_list_runs`).

## Notes

- Concurrency: by default one run at a time, configurable via
  `PipelineConfig.max_concurrent_runs`. The orchestrator enqueues if at
  capacity rather than refusing — the user sees runs in `Pending`
  state until a slot opens.
- Step `StepHandler` calls happen on `tokio::spawn` so a slow step
  doesn't block the orchestrator's own state machine. The handler is
  responsible for honoring the cancel token passed via `ctx`.
- History persistence: write-after-status-change, NOT periodic polling.
  A run that crashes mid-step leaves a `Running` row that the next
  app boot reconciles into `Aborted` with a `crashed: true` flag.
