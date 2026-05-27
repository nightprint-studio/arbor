# arbor-git-provider-github

GitHub implementation of `GitProvider`. **Also** the home of the shared
`GithubClient` (HTTP + auth + parsers) re-used by
`arbor-issue-tracker-github`.

## Purpose

Two things in one crate, by design:

1. **Implement `GitProvider`** for GitHub — PRs, releases, repo info,
   Actions runs.
2. **Own the shared GitHub client surface**: a configured `reqwest`
   client with the right `User-Agent`, a keyring-backed token loader,
   common response parsers (`User`, `Repository`, `Label`), rate-limit
   bookkeeping. This is exposed as a public `gh` module that
   `arbor-issue-tracker-github` imports.

Why not a separate `arbor-github-client` crate? In the discussion we
decided the cost of a tiny extra crate isn't worth it: GitHub *is* a
single provider with a shared HTTP/auth surface, and "this crate is the
GitHub home" reflects reality. The trade-off is documented up-front and
accepted.

## Contents (planned)

- `pub mod gh` — the shared client surface:
  - `GithubClient` with `get<T>`, `post<T>`, `delete`, etc.
  - `Auth` — PAT / OAuth token loader from `arbor-auth` + `keyring`,
    refresh on expiry.
  - Parsers for `User`, `Repository`, `Label` (and other types shared
    between PRs and Issues).
  - Rate-limit helper — reads the GitHub headers, exposes a "how close
    are we?" gauge that consumers can check before bulk-requests.
- `GithubProvider` — implements `GitProvider`. Methods delegate to the
  PRs / Releases / Actions modules.
- `prs`, `releases`, `actions`, `repo` — internal modules with the
  endpoint-specific request/response shapes.

## Depends on

- `arbor-core` — `http::builder`, `gh_api` (URL helpers), `AppError`.
- `arbor-auth` — OAuth flows and token refresh.
- `arbor-git-provider-api` — the trait it implements.

External: `reqwest`, `keyring`, `serde`, `serde_json`, `async-trait`,
`chrono`, `tokio`, `tracing`, `thiserror`.

## Consumed by

- `arbor` (Tauri shell) — registers a `GithubProvider` instance in the
  `GitProviderRegistry` at startup.
- `arbor-issue-tracker-github` — imports `gh::GithubClient` to talk to
  the Issues API without re-implementing auth and HTTP.

## Notes

- `gh::GithubClient` is the **only** thing in this crate that's
  publicly re-exportable. The PR / Release / Actions impls are private:
  consumers go through the trait.
- Auth tokens live in the OS keyring (via `keyring`). The keyring entry
  name is shared with `arbor-issue-tracker-github` — `gh::Auth` is the
  single source of truth.
- Rate-limit handling: at 5% of the budget left, requests start logging
  warnings; at 0%, return `GitProviderError::RateLimited { retry_after }`.
  No silent failures.
