# arbor-core

Building blocks shared by every other Arbor crate.

## Purpose

A handful of patterns used to be duplicated across `src-tauri/`:
`dirs::config_dir().…join("arbor")` resolved in 17 places, `reqwest::Client`
built with three slightly divergent defaults (user-agent
`"Arbor-Git-GUI/1.0"` vs `"arbor-git-gui"` vs none; timeouts of 0 / 20 / 30
seconds). `arbor-core` exists so each of these lives in **one** place.

It's also the home of the **`AppCtx` trait** — the abstraction that lets
domain crates emit frontend events / query app state without depending on
`tauri::*` directly.

## Public API: use the prelude

Workspace convention — every Arbor library crate exposes its public
surface through a `prelude` module. Either fully-qualify
(`arbor_core::prelude::arbor_config_path("foo")`) or glob-import once per
file (`use arbor_core::prelude::*;`). The per-feature submodules
(`paths`, `http`, `error`, `app_ctx`) stay `pub` for rustdoc navigation,
but call sites should go through the prelude so a single `use` line is
enough.

## Contents

- **`paths`** — `arbor_config_dir()`, `arbor_data_dir()`,
  `arbor_cache_dir()`, plus the joining helpers `arbor_config_path(sub)`
  and `try_arbor_config_path(sub)` (the `try_` variant propagates `None`
  when `dirs` is unavailable instead of falling back to `.`; use it when
  silently skipping persistence is preferable to writing under the cwd).
  grove gets its own sibling namespace: `grove_config_dir()`,
  `grove_data_dir()`, and `grove_config_path(sub)` resolve under
  `…/grove` (not `…/arbor/grove`), so its config + sample banks live apart.

- **`http`** — `client()` returns a pre-built `reqwest::Client` with the
  Arbor user-agent (`USER_AGENT = "Arbor-Git-GUI/<crate-version>"`) and
  `DEFAULT_TIMEOUT` (30s). For callers that need extra config (e.g. Jira
  Data Center's `danger_accept_invalid_certs(true)`), `client_builder()`
  returns the same pre-configured `ClientBuilder` to extend.

- **`error`** — `CoreError { Io, Http }` for failures originating inside
  this crate. The Tauri shell crate provides
  `impl From<CoreError> for AppError` at the boundary so `?` propagation
  works.

- **`app_ctx`** — the `AppCtx` trait:
  ```rust
  pub trait AppCtx: Send + Sync {
      fn emit(&self, event: &str, payload: serde_json::Value);
      fn arbor_dir(&self) -> &std::path::Path;
      fn is_focused(&self) -> bool;
  }
  ```
  Kept deliberately small. New methods are added only when a domain crate
  asks for them.

## Planned (not yet implemented)

- **`gh_api`** — GitHub primitives (`parse_github_repo`, `resolve_commit_sha`,
  …) currently duplicated between the marketplace and `git_provider/github`.
  Lands together with `arbor-git-provider-github`.

- Wider per-domain error types — `AppError::Other(string)` still annexes
  most failures. Once domain crates land they will surface
  `MarketplaceError`, `IssueTrackerError`, … with the typed variants the
  frontend needs to distinguish "rete giù" from "auth fallita" from
  "rate limited".

## Depends on

Nothing internal. Standard ecosystem only (`serde`, `serde_json`,
`reqwest`, `dirs`, `thiserror`, `tracing`, `tokio`, `async-trait`).

## Consumed by

Everyone, directly or transitively. Adding a new crate to the workspace
without depending on `arbor-core` is almost certainly a mistake.

## Notes

- `arbor-core` must never depend on `tauri::*`. The whole point of `AppCtx`
  is to keep the dependency one-way: `arbor` (the shell) implements
  `AppCtx`, all the domain crates consume the trait.
- The `error` module exposes `CoreError` but does NOT contain per-domain
  variants — those live in each domain's `*-api` crate as `XxxError` and
  map in at the boundary.
