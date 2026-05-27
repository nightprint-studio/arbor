# arbor-git-provider-gitlab

GitLab implementation of `GitProvider`. **Also** the home of the shared
`GitlabClient` (HTTP + auth + parsers) re-used by
`arbor-issue-tracker-gitlab`.

## Purpose

Symmetrical to `arbor-git-provider-github`. GitLab has both Merge
Requests (provider domain) and Issues (issue-tracker domain), and both
talk through the same HTTP/auth surface. This crate owns:

1. The `GitlabClient` — `reqwest` configured with the right
   `User-Agent`, OAuth token loader, parsers for `User`, `Project`,
   `Label`, rate-limit awareness.
2. The `GitlabProvider` — implements `GitProvider` (MRs, releases,
   project info, pipelines).

Supports both gitlab.com and self-hosted instances (the `base_url` is
read from `GitProviderConfig`, defaults to `https://gitlab.com`).

## Contents (planned)

- `pub mod gl` — the shared client surface:
  - `GitlabClient` with `get<T>`, `post<T>`, `delete`, etc.
  - `Auth` — OAuth / PAT token loader from `arbor-auth` + `keyring`.
  - Parsers for `User`, `Project`, `Label`, `Milestone`.
  - Rate-limit helper (GitLab's headers are different from GitHub's).
- `GitlabProvider` — implements `GitProvider`. Methods delegate to the
  MRs / Releases / Pipelines modules.
- `mrs`, `releases`, `pipelines`, `project` — internal modules with the
  endpoint shapes.

## Depends on

- `arbor-core` — `http::builder`, `AppError`.
- `arbor-auth` — OAuth flows and token refresh.
- `arbor-git-provider-api` — the trait it implements.

External: `reqwest`, `keyring`, `serde`, `serde_json`, `async-trait`,
`chrono`, `tokio`, `tracing`, `thiserror`.

## Consumed by

- `arbor` (Tauri shell) — registers a `GitlabProvider` in the
  `GitProviderRegistry` at startup.
- `arbor-issue-tracker-gitlab` — imports `gl::GitlabClient`.

## Notes

- Self-hosted GitLab instances may use TLS certificates from internal
  CAs. Unlike Jira (where we currently accept invalid certs by default),
  GitLab keeps strict cert validation — most self-hosted setups already
  have proper certs. Make this opt-in via config if user reports break.
- GitLab's API uses `iid` (project-internal incremental id) for MRs and
  Issues. Make sure the DTOs distinguish that from the global `id`.
