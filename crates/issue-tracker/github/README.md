# arbor-issue-tracker-github

GitHub Issues implementation of `IssueTracker`.

## Purpose

GitHub Issues live in the same repos that GitHub PRs do — same auth,
same `GithubClient`, same rate-limit budget. So this crate doesn't
re-implement HTTP/auth; it imports `gh::GithubClient` from
`arbor-git-provider-github` and just adds the Issues-specific endpoints
and DTO mapping.

This is the decision we made in the discussion: "shared infra in
`git-provider-github`, Issues sit on top". Lighter crate count, single
auth surface, GitHub-as-one-provider reflected faithfully.

## Contents (planned)

- `GithubIssuesTracker` — implements `IssueTracker`. Owns a reference
  to the shared `GithubClient` (provided at construction by `arbor`).
- `issues` — internal module with endpoint shapes (list, get, create,
  edit), maps GitHub's JSON to the abstract `Issue` DTO from
  `arbor-issue-tracker-api`.
- `comments` — same shape for `/issues/{id}/comments`.
- `state_mapping` — maps GitHub's open/closed + labels into the
  abstract `IssueState`. The "in progress" state is detected from a
  configurable label (default: `"in progress"`), exposed via
  `IssueTrackerConfig`.

## Depends on

- `arbor-core` — error mapping, AppCtx for emit on transitions.
- `arbor-issue-tracker-api` — the trait it implements.
- `arbor-git-provider-github` — `gh::GithubClient` (HTTP + auth +
  rate-limit + parsers).

External: `serde`, `serde_json`, `async-trait`, `chrono`, `tokio`,
`tracing`, `thiserror`.

Note: no `reqwest` / `keyring` directly — those come transitively
through `arbor-git-provider-github`.

## Consumed by

- `arbor` (Tauri shell) — registers a `GithubIssuesTracker` in the
  `IssueTrackerRegistry` at startup, one per configured GitHub account.

## Notes

- GitHub's API conflates "Pull Request" and "Issue" at the URL level
  (`/issues/{id}` returns either type, distinguished by a flag). This
  tracker MUST filter out PR rows from `list_issues` results — those
  belong to `arbor-git-provider-github`.
- Reactions are dropped from the abstract DTO. If a future feature
  needs them, add them to `arbor-issue-tracker-api::Issue` once and all
  providers gain the field at once.
