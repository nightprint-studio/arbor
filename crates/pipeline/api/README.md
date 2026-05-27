# arbor-pipeline-api

Pipeline trait, DTOs, step-extension registry, and `PipelineConfig`.

## Purpose

Arbor's pipeline is a small DAG-shaped task runner: a "pipeline" is a
list of stages, each stage is a list of steps. Steps can be built-in
(Rust impls: shell, fs, git ops) or plugin-provided (Lua callbacks
registered via `arbor.pipeline.register_step`).

Today this lives in `src-tauri/src/pipeline/`. Splitting into
`api` + `core` mirrors the pattern used everywhere else in the refactor:

- `api` defines the contract step extensions implement,
- `core` is the orchestrator,
- `arbor` wires concrete steps and exposes Tauri commands.

The `api` crate is also the home of `PipelineConfig` and the hook name
constants for `HOOK_ON_PIPELINE_STARTED`, `HOOK_ON_PIPELINE_DONE`,
`HOOK_ON_PIPELINE_STEP_DONE`.

## Contents (planned)

- DTOs: `Pipeline`, `Stage`, `Step`, `PipelineRun`, `StageRun`,
  `StepRun`, `Status` (pending/running/passed/failed/skipped/aborted).
- `trait StepHandler` (`#[async_trait]`):
  - `fn id(&self) -> &str` — unique step type id (`shell`, `git_fetch`,
    `lua:<plugin>:<action>`, …).
  - `async fn run(&self, ctx, params) -> StepResult`.
  - `fn schema(&self) -> StepSchema` — JSON schema of accepted params,
    so the editor UI can render a form.
- `StepRegistry` — `register(handler)`, `get(id)`. Built-in handlers
  register at startup; plugin handlers register/unregister with the
  plugin lifecycle.
- `PipelineConfig` — global pipeline settings (default working dir,
  max concurrent runs, history retention).
- `PipelineError` — `StepFailed`, `StepNotFound`, `SchemaMismatch`,
  `Aborted`, `Other`. Maps to `AppError`.
- Hook constants: `HOOK_ON_PIPELINE_STARTED`, `HOOK_ON_PIPELINE_DONE`,
  `HOOK_ON_PIPELINE_STEP_DONE`.

## Depends on

- `arbor-core` — `AppError`, `AppCtx`.

External: `serde`, `serde_json`, `thiserror`, `async-trait`, `chrono`.

No `tauri`, no `mlua`, no `reqwest`. Pure contract crate.

## Consumed by

- `arbor-pipeline-core` — the orchestrator implements the lifecycle
  against this registry.
- `arbor-plugin-core` — the Lua API `arbor.pipeline.register_step`
  registers `StepHandler` instances backed by Lua callbacks.
- `arbor` (Tauri shell) — registers the built-in step handlers (shell,
  git, fs), exposes `pipeline_*` Tauri commands.

## Notes

- The `lua:<plugin>:<action>` step-type id convention lets the runtime
  prefix-match for cleanup: when a plugin is disabled, the orchestrator
  drops all `StepHandler` entries with that prefix.
- `StepSchema` returning JSON Schema (not a custom Rust shape) is on
  purpose: the editor renderer is a Svelte component that already
  knows how to render JSON Schema forms. Single source of truth.
