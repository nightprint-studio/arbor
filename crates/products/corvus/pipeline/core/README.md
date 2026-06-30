# corvus-pipeline-core

The host-free run-tracking core of Corvus pipelines.

## Purpose

Builds on [`corvus-pipeline-api`](../api) (the model + expression engine) and
holds the pieces of the orchestrator that have no Tauri / threading / process
coupling, extracted from `src-tauri/src/pipeline/mod.rs` (round-2 M2):

- **`registry`** — the in-memory `PipelineRegistry`: definitions, runs,
  concurrency locks (`try_acquire_lock` / `release_lock_of` / `locked_by`),
  cancel tokens, and the global running-count bookkeeping. The host wraps it in
  a `Mutex` (paired with a `Condvar` for the concurrency queue).
- **`persist`** — one-JSON-file-per-run persistence under
  `~/.config/arbor/pipeline_runs/`, plus `registry_from_disk` recovery at boot
  (runs left `Running`/`Pending` at shutdown are coerced to `Failed`).
- **`run_tree`** — the pure orchestration helpers: `find_step_mut` (step-tree
  lookup through `if_block` nesting), `compute_resume_cursor` /
  `resumable_step_indices` (resume planning), `split_chunk_lines` /
  `drain_partial_line` (pipe-output chunking), `infer_step_log_level`, and
  `step_preview`.

The live orchestrator (the per-run thread, `AppHandle` event emission,
shell-process spawning, Lua-op dispatch) stays in the host shell and consumes
this crate.

## Public API: use the prelude

Reach the surface through `corvus_pipeline_core::prelude::...`:
`PipelineRegistry`, `registry_from_disk` / `persist_run` / `now_ms` /
`RUN_LOG_CAP`, and the `run_tree` helpers.

## Tests

The registry lock semantics, resume-cursor computation, the resumable-step
index plan, output chunk splitting, and log-level inference all carry unit
tests (`cargo test -p corvus-pipeline-core`).

## Depends on

`corvus-pipeline-api` (the model), `arbor-core` (the run-store path helper),
`serde_json`, `tracing`. No keyring / no Tauri.

## Consumed by

`arbor` (the shell): `src-tauri/src/pipeline/mod.rs` (the orchestrator) and the
pipeline Tauri commands.
</content>
