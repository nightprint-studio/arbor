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
  merula is a profile-scoped product like corvus: `merula_config_dir()`,
  `merula_data_dir()`, and `merula_config_path(sub)` resolve under
  `arbor/profiles/<active>/merula/`. `merula_legacy_sibling_dirs()` returns the
  old top-level `…/merula` (and pre-rename `…/nemus`) roots, used only by the
  one-shot boot migration that relocates that data into the active profile.

- **`profile`** — the **profile × product** layout
  (`docs/profiles-and-product-config.md`). A profile is an isolated
  environment (own settings, plugins, repos) under
  `arbor/profiles/<name>/`, with one product-agnostic `profile.toml` plus a
  bucket per product. A process-global active-profile cell — seeded at boot
  by `init_active_profile()` from the `arbor/active-profile` pointer, flipped
  by `set_active_profile(name)` — lets the profile-scoped helpers resolve
  without threading state through every caller: `arbor_profile_dir()` /
  `arbor_profile_path(sub)` (generic per-profile), `product_dir(product)` /
  `product_path(product, sub)` (+ `try_product_path` propagating `None` like
  `try_arbor_config_path`), `profile_plugins_dir()`, the product-name constants
  `PRODUCT_CORVUS` / `PRODUCT_MERULA`, plus `*_for(name, …)` explicit-profile
  variants for migration / management. The
  existing `arbor_config_*` helpers keep meaning "the global `arbor/` root"
  (the pointer, the portable `git/`, OAuth client overrides).

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
