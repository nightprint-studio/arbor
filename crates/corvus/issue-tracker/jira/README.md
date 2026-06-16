# corvus-issue-tracker-jira

The Jira (Cloud / Server / Data Center) implementation of the Corvus
`IssueTracker` trait.

## Purpose

Implements `corvus_issue_tracker_api::IssueTracker` for Jira over its REST API
v2/v3 — search (JQL) / get / lookup / filter-options (projects, statuses,
labels, issue types, active sprints) / transition / assign / comment / create /
image-fetch, plus the self-describing `ProviderDescriptor` (OAuth + API-token
auth methods, brand icon). Converts Jira ADF and pre-rendered `renderedFields`
HTML (sanitized) to the shared `Issue`/`IssueComment` shape.

Two Jira-specific inherent methods sit outside the trait (not every tracker has
them): `download_attachment` (streams an attachment to disk, host-locked) and
`current_user` (`/myself`, for validating freshly-stored credentials).

**Keyring-free.** The session — per-tenant base URL, full `Authorization` header
(`Bearer` for OAuth, `Basic`/`Bearer` for API token), and user-facing `web_base`
for issue links — arrives through an injected `Arc<dyn arbor_ipc::SessionProvider>`.
On a `401` the OAuth session is refreshed and the request retried once; API-token
auth has nothing to refresh and the `401` surfaces. Only the shell reaches the
keyring or runs the OAuth dance.

## Public API: use the prelude

`corvus_issue_tracker_jira::prelude::JiraTracker`.
`JiraTracker::new(session, account)` builds an `Arc<dyn IssueTracker>`.

## Tests

The pure pieces — JQL encoding, key detection, priority mapping, agile-URL
derivation, ADF→Markdown, and issue-URL mapping — are unit-tested (`cargo test
-p corvus-issue-tracker-jira`). The network round-trips are not unit-tested.

## Depends on

`corvus-issue-tracker-api` (DTOs + trait), `arbor-ipc` (`SessionProvider`),
`arbor-core` (shared HTTP client), `async-trait`, `serde_json`, `reqwest`,
`tokio` + `futures-util` (attachment streaming), `ammonia` (HTML sanitize),
`tracing`.

## Consumed by

`arbor` (the shell): registered in `src-tauri/src/integrations/registry.rs`, with
the keyring/OAuth adapter in `token_source.rs` and the command shim in `jira.rs`.
