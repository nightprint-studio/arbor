# arbor-core

Building blocks shared by every other Arbor crate.

## Purpose

Today the same patterns are duplicated across 18+ files in `src-tauri/`:
`dirs::config_dir().…join("arbor")`, `reqwest::Client::builder()` with slightly
divergent defaults, ad-hoc GitHub API helpers (`parse_github_repo`,
`resolve_commit_sha` with anonymous `struct Resp { sha: String }` in two
places). `arbor-core` exists so each of these lives in **one** place.

It's also the home of the **`AppCtx` trait** — the abstraction that lets
domain crates emit frontend events / query app state without depending on
`tauri::*` directly.

## Contents (planned)

- `error` — base `AppError` enum + `Result<T>` alias. Per-domain errors
  (`MarketplaceError`, `IssueTrackerError`, …) implement `From<XxxError> for
  AppError` at the `arbor` boundary, so the frontend can finally distinguish
  "rete giù" / "auth fallita" / "rate limited" instead of seeing them all as
  `Other(string)`.

- `paths` — every `~/.config/arbor/...` location centralised:
  `arbor_config_dir()`, `plugins_dir()`, `themes_dir()`,
  `marketplace_cache_path()`, `workspaces_dir()`, `pipeline_runs_dir()`, …
  Today these are spread across 18 files; here they'll live as one
  authoritative module.

- `http` — `builder() -> reqwest::ClientBuilder` pre-populated with
  `User-Agent: arbor/<version>` and the standard timeout shape. Callers
  layer extra config on top (e.g. `danger_accept_invalid_certs(true)` for
  Jira on-prem, `connect_timeout` for SSE watchers).

- `gh_api` — GitHub primitives: `parse_github_repo`, `github_url`,
  `raw_url`, `normalise_github_url`, `resolve_commit_sha`. Used by the
  marketplace (pin verification, install) and by `arbor-git-provider-github`
  (which then layers its own client on top with auth).

- `app_ctx` — the `AppCtx` trait:
  ```rust
  pub trait AppCtx: Send + Sync {
      fn emit(&self, event: &str, payload: serde_json::Value);
      fn arbor_dir(&self) -> &std::path::Path;
      fn is_focused(&self) -> bool;
  }
  ```
  Kept deliberately small. New methods are added only when a domain crate
  asks for them.

## Depends on

Nothing internal. Standard ecosystem only (`serde`, `reqwest`, `dirs`,
`thiserror`, `tracing`, `tokio`, `async-trait`).

## Consumed by

Everyone, directly or transitively. Adding a new crate to the workspace
without depending on `arbor-core` is almost certainly a mistake.

## Notes

- `arbor-core` must never depend on `tauri::*`. The whole point of `AppCtx`
  is to keep the dependency one-way: `arbor` (the shell) implements
  `AppCtx`, all the domain crates consume the trait.
- The `error` module exposes `AppError` but does NOT contain per-domain
  variants — those live in each domain's `*-api` crate as `XxxError` and
  map in at the boundary.
