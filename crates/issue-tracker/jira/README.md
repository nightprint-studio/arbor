# arbor-issue-tracker-jira

Atlassian Jira implementation of `IssueTracker` (Cloud and Data Center /
Server).

## Purpose

Today `src-tauri/src/integrations/jira.rs` holds the entire Jira surface,
bypassing any common trait. The split into `arbor-issue-tracker-jira`:

1. Plugs Jira into the same `IssueTracker` trait used by Linear and
   GitHub/GitLab Issues.
2. Keeps the Jira-specific quirks (rendered fields with HTML, custom
   workflow states, self-signed certs in on-prem) **here**, not leaking
   into the rest of the codebase.
3. Standalone — does NOT depend on `arbor-git-provider-*`. Jira is its
   own product.

## Contents (planned)

- `JiraTracker` — implements `IssueTracker`. Owns its own `reqwest`
  client (built on `arbor-core::http::builder` with the on-prem TLS
  relaxation).
- `auth` — Cloud uses email + API token; Server uses PAT (or basic auth
  for legacy on-prem). All variants flow through `arbor-auth` +
  `keyring`.
- `transitions` — Jira workflows are project-specific, so the
  `Transition` DTO carries the workflow-defined source/target state
  names. `list_transitions` returns the menu the UI shows for the
  "transition issue" action.
- `rendered_fields` — Jira can ship server-side HTML for
  description/comment bodies (the `renderedFields` query parameter).
  Sanitize via `ammonia` before passing to the frontend renderer —
  Jira HTML can include arbitrary embedded content.
- `state_mapping` — map workflow states into the abstract `IssueState`.
  Configurable per Jira instance (`IssueTrackerConfig.jira.state_map`)
  because the same project can name "Done" anything.

## Depends on

- `arbor-core` — `http::builder`, `AppCtx`, `AppError`.
- `arbor-auth` — OAuth (Cloud) and PAT (Server) handling.
- `arbor-issue-tracker-api` — the trait it implements.

External: `reqwest`, `keyring`, `serde`, `serde_json`, `async-trait`,
`chrono`, `tokio`, `tracing`, `thiserror`, `ammonia`.

## Consumed by

- `arbor` (Tauri shell) — registers a `JiraTracker` in the
  `IssueTrackerRegistry` at startup, one per configured Jira instance.

## Notes

- **TLS**: Jira Data Center / Server installations very often use
  self-signed or internal-CA certs. The current `integrations/jira.rs`
  uses `danger_accept_invalid_certs(true)` unconditionally — this is
  intentional and preserved in the split. **NEVER copy this flag to any
  other crate.**
- HTML sanitization is non-negotiable for `renderedFields`. A direct
  paste into the frontend renderer would be a stored XSS vector — a
  malicious Jira admin (or a JQL hostile comment) could break the app.
- Jira's API has a quirky pagination model (`startAt` + `maxResults`
  rather than `page`). Hide that inside the impl — the trait paginates
  with `next_cursor: Option<String>`.
