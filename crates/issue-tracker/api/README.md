# arbor-issue-tracker-api

Trait, DTOs, and runtime registry for issue trackers: Jira, Linear,
GitHub Issues, GitLab Issues.

## Purpose

Today `src-tauri/src/integrations/{jira,linear}.rs` ship two tracker
impls with overlapping concepts (issue, comment, transition) but no
common trait — each is its own surface to the rest of the codebase. And
GitHub/GitLab Issues are scattered inside `git_provider/`, conflated with
PR/MR handling.

This crate introduces:

1. A **single `IssueTracker` trait** all impls satisfy.
2. **Shared DTOs** (`Issue`, `Comment`, `IssueState`, `Transition`) — so
   the rest of the codebase consumes one shape.
3. **A `Registry`** wired by `arbor` at startup, identical pattern to
   the git-provider one.
4. **Hook name constants** for issue lifecycle (`HOOK_ON_ISSUE_LINKED`,
   `HOOK_ON_ISSUE_TRANSITIONED`).

Adding a tracker = new crate, no edits in `api`. Same shape as the
git-provider split.

## Contents (planned)

- `trait IssueTracker` (`#[async_trait]`):
  - `get_issue(ctx, id)` / `search_issues(ctx, query)`
  - `list_comments(ctx, issue_id)` / `add_comment(ctx, issue_id, body)`
  - `transition(ctx, issue_id, to_state)` / `list_transitions(ctx, issue_id)`
  - `link_issue(ctx, repo_id, issue_id)` / `unlink_issue(ctx, repo_id, issue_id)`
- DTOs: `Issue`, `Comment`, `IssueState` (open/closed/in_progress/done),
  `Transition`, `User`, `Label`, `Project` (Jira-flavored, GitLab-flavored
  — abstracted), `IssueLink`.
- `Registry` — `register(tracker_id, Box<dyn IssueTracker>)`,
  `get(id)`, `list_ids()`.
- `IssueTrackerError` — `Auth`, `NotFound`, `Transport`, `RateLimited`,
  `PermissionDenied`, `InvalidTransition`, `Other`. Maps to `AppError`.
- Hook constants: `HOOK_ON_ISSUE_LINKED`, `HOOK_ON_ISSUE_TRANSITIONED`.

## Depends on

- `arbor-core` — `AppError`, `AppCtx`.

External: `serde`, `serde_json`, `thiserror`, `async-trait`.

No `reqwest`, no `keyring`, no `tauri` — same rule as git-provider-api.

## Consumed by

- `arbor-issue-tracker-jira` — concrete impl.
- `arbor-issue-tracker-linear` — concrete impl.
- `arbor-issue-tracker-github` — concrete impl.
- `arbor-issue-tracker-gitlab` — concrete impl.
- `arbor` (Tauri shell) — owns the registry, exposes `issue_tracker_*`
  Tauri commands, fires hooks via `arbor-plugin-api`.

## Notes

- `IssueState` is a deliberate **abstraction** over each backend's
  native state model. Jira has workflow-defined states (custom per
  project); Linear has its own; GitHub/GitLab use open/closed +
  labels. The API surface exposes a normalized enum plus a
  `provider_state: String` opaque field for round-tripping when the
  user wants to see the literal label.
- `link_issue` / `unlink_issue` are intentionally tracker-agnostic.
  Persistence of repo↔issue links lives in `arbor` (`workspace`/
  `git_repo` state), this trait just exposes the lifecycle hook trigger.
