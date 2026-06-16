# corvus-pipeline-api

The pure pipeline expression engine for Corvus.

## Purpose

The host-free evaluation core of Corvus pipelines, extracted from
`src-tauri/src/pipeline/` (round-2 M2):

- **`vars`** — the per-run typed variable store (`RunContext` / `VarValue`),
  `${var}` / `${var:-fallback}` interpolation, and the declarative capture
  transform chain (`trim`, `lines`, `split`, `regex`, `json_get`, …).
- **`condition`** — the structured if-block condition tree (`Condition`,
  `CompareOp`) and `evaluate(&Condition, &RunContext) -> bool`.
- **`condition_parser`** — recursive-descent parser for the free-form condition
  syntax (`${has_pom} && !${skip}`, `${count} > 10`, `defined(x)`, `matches(v,
  "re")`, …) → a `Condition` tree.

The orchestrator and the `IfBlock` (which carries `StepDef` step bodies) stay in
the host `pipeline` module; this crate is only the evaluation primitives, so it
depends on nothing but `serde` / `serde_json` / `regex` (+ `tracing` for one
warn line) and is trivially unit-testable.

When `pipeline-core` (the run orchestrator) extracts later, the step DTOs +
external-step trait join this `*-api` leaf.

## Public API: use the prelude

Reach the surface through `corvus_pipeline_api::prelude::...`: `RunContext`,
`VarValue`, `resolve_vars`, `Transform`/`apply_transforms`, `Condition`,
`CompareOp`, `evaluate`, `parse`.

## Tests

The parser, the evaluator, and the variable/transform engine all carry unit
tests (`cargo test -p corvus-pipeline-api`).

## Depends on

`serde`, `serde_json`, `regex`, `tracing`. No Arbor-internal deps.

## Consumed by

`arbor` (the shell): `src-tauri/src/pipeline/` (the orchestrator + the
`StepDef`-coupled `IfBlock`) and the `arbor.pipeline.*` plugin namespace.
