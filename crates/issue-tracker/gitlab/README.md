# arbor-issue-tracker-gitlab

GitLab Issues implementation of `IssueTracker`.

## Purpose

Symmetrical to `arbor-issue-tracker-github`. GitLab Issues live in the
same projects that MRs do, so HTTP/auth/rate-limit infra is shared
through `arbor-git-provider-gitlab`'s `gl::GitlabClient`.

Supports gitlab.com and self-hosted instances (base URL comes from the
shared client).

## Contents (planned)

- `GitlabIssuesTracker` — implements `IssueTracker`. Owns a reference
  to `gl::GitlabClient` (provided at construction by `arbor`).
- `issues` — endpoint shapes for `/projects/{id}/issues`. Note: GitLab
  uses `iid` (project-internal id) which is what users see; map that
  to the trait's `id` field.
- `comments` — `/projects/{id}/issues/{iid}/notes`.
- `state_mapping` — GitLab's `opened` / `closed` + labels mapped to the
  abstract `IssueState`. "In progress" detection from a configurable
  label, same convention as the GitHub impl.

## Depends on

- `arbor-core` — error mapping, AppCtx.
- `arbor-issue-tracker-api` — the trait it implements.
- `arbor-git-provider-gitlab` — `gl::GitlabClient`.

External: `serde`, `serde_json`, `async-trait`, `chrono`, `tokio`,
`tracing`, `thiserror`.

## Consumed by

- `arbor` (Tauri shell) — registers a `GitlabIssuesTracker` in the
  `IssueTrackerRegistry` at startup, one per configured GitLab account.

## Notes

- GitLab Issues are scoped to a **project**, not a global namespace.
  The `IssueId` shape used by this provider is effectively
  `{project_id_or_path}#{iid}`. Make sure the abstract DTO accommodates
  this without leaking the per-provider format upwards.
- Confidential issues: GitLab supports them, the API silently omits
  them if the token lacks permission. Surface that distinction in
  `IssueTrackerError::PermissionDenied` rather than treating as
  `NotFound`.
