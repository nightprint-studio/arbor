# corvus-pipeline-api

The host-free pipeline **model + expression engine** for Corvus.

## Purpose

The pure core of Corvus pipelines (no Tauri / no live orchestrator), extracted
from `src-tauri/src/pipeline/` (round-2 M2):

- **`vars`** — the per-run typed variable store (`RunContext` / `VarValue`),
  `${var}` / `${var:-fallback}` interpolation, and the declarative capture
  transform chain (`trim`, `lines`, `split`, `regex`, `json_get`, …).
- **`condition`** — the structured if-block condition tree (`Condition`,
  `CompareOp`) and `evaluate(&Condition, &RunContext) -> bool`.
- **`condition_parser`** — recursive-descent parser for the free-form condition
  syntax (`${has_pom} && !${skip}`, `${count} > 10`, `defined(x)`, `matches(v,
  "re")`, …) → a `Condition` tree.
- **`builtin`** — the small side-effecting op set (`file_exists`, `file_read`,
  `env`, `json_get`, `path_join`, `set_var`, `echo`, `match`) the runtime
  resolves directly to feed `${var}` captures.
- **`if_block`** — the `if`/`elif`/`else` branch structure (`IfBlock`,
  `IfBranch`, `BranchSelection`) whose bodies are `StepDef`s, plus branch
  selection.
- **`model`** — the step / stage / pipeline definitions (`StepDef`, `StageDef`,
  `PipelineDef`, `LuaOpSpec`) and the run-state snapshots (`RunStatus`,
  `StepRun`, `StageRun`, `PipelineRun`, `LogEvent`, `ResumeCursor`) the
  orchestrator streams to the UI, plus the `parse_log_level` / `parse_stage_mode`
  helpers.

The live orchestrator (the per-run thread, event emission, shell-process
spawning, Lua-op dispatch) stays host-side; the in-memory run registry, JSON
persistence, and the pure orchestration helpers live in `corvus-pipeline-core`.

## Public API: use the prelude

Reach the surface through `corvus_pipeline_api::prelude::...`: the model types
(`PipelineDef`, `StepDef`, `PipelineRun`, `RunStatus`, …), `parse_log_level` /
`parse_stage_mode`, `run_builtin` / `BuiltinSpec`, `IfBlock` / `BranchSelection`,
`RunContext`, `VarValue`, `resolve_vars`, `Transform` / `apply_transforms`,
`Condition`, `CompareOp`, `evaluate`, `parse`.

## Tests

The parser, the evaluator, the variable/transform engine, the builtins, the
if-block selection, and the model helpers all carry unit tests
(`cargo test -p corvus-pipeline-api`).

## Depends on

`serde`, `serde_json`, `regex`, `tracing`. No Arbor-internal deps.

## Consumed by

`corvus-pipeline-core` (the run registry + helpers) and `arbor` (the shell):
`src-tauri/src/pipeline/` (the orchestrator) and the `arbor.pipeline.*` plugin
namespace.
</content>
