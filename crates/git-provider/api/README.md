# arbor-git-provider-api

Trait, DTOs, and runtime registry for the "git platform" side: PRs, MRs,
releases, repo metadata.

## Purpose

GitHub and GitLab (and later: codeberg, gitea, bitbucket, …) all do the
same shape of work: fetch a PR/MR, list commits on a ref, get repo
metadata, list releases. Today this is mostly tangled in
`src-tauri/src/git_provider/` with two impls behind a trait.

Splitting into `api` + per-provider impl crates means:

- adding a new provider = new crate, no edits to existing impls,
- the `arbor` shell depends on `*-api` only; impls are wired at startup
  via the `Registry`,
- testing a feature that consumes a git provider doesn't drag in HTTP +
  OAuth + keyring.

**Scope split** (decided in the refactor discussion): this crate owns
**git-platform objects** — PRs, MRs, releases, branches/tags as remote
metadata, repo info, runs/checks. **Issues live in
`arbor-issue-tracker-*` instead**, even when hosted on GitHub or
GitLab.

## Contents (planned)

- `trait GitProvider` (`#[async_trait]`):
  - `get_pull_request(id)` / `list_pull_requests(filter)`
  - `create_pull_request(req)` / `merge_pull_request(id, strategy)`
  - `list_releases(repo)` / `get_release(id)`
  - `get_repo_info(repo)`
  - `list_runs(branch?)` / `get_run(id)` — for CI metadata where the
    platform exposes it (GitHub Actions, GitLab Pipelines).
- DTOs: `PullRequest`, `Release`, `Branch`, `Tag`, `RepoInfo`,
  `RunSummary`, `User`, `Label`.
- `Registry` — `register(provider_id, Box<dyn GitProvider>)`, `get(id)`,
  `list_ids()`. Wired by `arbor` at startup.
- `GitProviderError` — `Auth`, `NotFound`, `Transport`, `RateLimited`,
  `Conflict`, `PermissionDenied`, `Other`. `From<...> for AppError` at
  the boundary.

## Depends on

- `arbor-core` — `AppError`, `http::builder`, `AppCtx`.

External: `serde`, `serde_json`, `thiserror`, `async-trait`.

**No `reqwest`, `keyring`, `tauri`** here. This crate is the contract;
the impls deal with HTTP.

## Consumed by

- `arbor-git-provider-github` — concrete impl.
- `arbor-git-provider-gitlab` — concrete impl.
- `arbor-issue-tracker-github` and `arbor-issue-tracker-gitlab` —
  transitively (they depend on the matching `git-provider-<host>` for
  shared HTTP/auth client).
- `arbor` (Tauri shell) — owns the singleton `Registry`, exposes
  `git_provider_*` Tauri commands.

## Notes

- The trait is async; all methods return `Result<T, GitProviderError>`.
- Hook name constants for this domain (e.g. `HOOK_ON_MR_OPENED`,
  `HOOK_ON_MR_MERGED`, `HOOK_ON_MR_UPDATED`) live here, NOT in
  `arbor-plugin-api`. That keeps the dispatcher name-agnostic.
- `Registry` is intentionally simple: it does not perform routing or
  multi-provider fan-out — callers know which provider they want and
  call `get(id)`. Multi-provider aggregation (e.g. "show all my open
  PRs across all configured providers") lives in `arbor`, which iterates
  `registry.list_ids()` and aggregates.
