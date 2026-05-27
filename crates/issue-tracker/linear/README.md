# arbor-issue-tracker-linear

Linear implementation of `IssueTracker` (GraphQL API).

## Purpose

Today `src-tauri/src/integrations/linear.rs` ships the Linear client.
The split into `arbor-issue-tracker-linear`:

1. Plugs Linear into the common `IssueTracker` trait.
2. Keeps Linear's GraphQL plumbing here so the rest of the codebase
   sees only the abstract DTOs.
3. Standalone — does NOT depend on `arbor-git-provider-*`. Linear is
   its own product.

## Contents (planned)

- `LinearTracker` — implements `IssueTracker`. Owns its own `reqwest`
  client built on `arbor-core::http::builder`.
- `graphql` — small GraphQL request layer. We're NOT pulling in a full
  GraphQL client crate (`graphql_client` etc.) — Linear's surface used
  by Arbor is small enough that hand-rolled queries serialized as
  strings + JSON responses are cheaper than the proc-macro toolchain.
  This is a deliberate choice.
- `state_mapping` — Linear's native state types (`backlog`, `unstarted`,
  `started`, `completed`, `canceled`) map cleanly to the abstract
  `IssueState`. Less custom config than Jira.

## Depends on

- `arbor-core` — `http::builder`, `AppCtx`, `AppError`.
- `arbor-auth` — OAuth and PAT handling, token persistence via
  `keyring`.
- `arbor-issue-tracker-api` — the trait it implements.

External: `reqwest`, `keyring`, `serde`, `serde_json`, `async-trait`,
`chrono`, `tokio`, `tracing`, `thiserror`.

## Consumed by

- `arbor` (Tauri shell) — registers a `LinearTracker` in the
  `IssueTrackerRegistry` at startup.

## Notes

- Linear uses cursor-based pagination (`pageInfo.endCursor`). Map this
  to the trait's `next_cursor: Option<String>` directly — no
  translation needed.
- Linear rate-limits per OAuth token. Surface `RateLimited { retry_after }`
  cleanly so the user sees a real "retry in 30s" message, not a
  confusing 429.
- A future Linear-specific feature might want webhooks for live issue
  updates. That would need a webhook receiver server-side which Arbor
  (a desktop app) can't host — keep this out of scope, rely on
  polling via the scheduler for now.
