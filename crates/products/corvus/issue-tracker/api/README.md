# corvus-issue-tracker-api

Provider-agnostic issue-tracker contract for Corvus.

## Purpose

The one DTO shape that every tracker normalises into — `Issue`, `IssueComment`,
`IssueStatus`, `IssueFilters`, `IssueFilterOptions`, users / labels / teams /
projects / cycles / milestones / attachments, `BodyFormat` — so the frontend
renders a single model whether the backend is Jira, Linear, or (later) GitHub /
GitLab Issues. Plus provider-agnostic helpers like `branch_name_for_issue`.

Extracted from `src-tauri/src/integrations/` (round-2 M2/M3). The **leaf** the
per-provider impls build on. It holds:

- **DTOs** — `Issue`, `IssueComment`, `IssueStatus`, `IssueFilters`,
  `IssueFilterOptions`, users / labels / teams / projects / cycles / milestones /
  attachments, `BodyFormat`. Plus pure helpers like `branch_name_for_issue`.
- **`IssueTracker`** — the async, object-safe trait every tracker satisfies, so
  the host holds them uniformly as `Arc<dyn IssueTracker>`. Constructed with
  credentials injected (`Arc<dyn arbor_ipc::SessionProvider>`), so the methods
  never see a token and the impl reaches the keyring only via the shell.
- **`ProviderDescriptor`** — what a tracker declares to the FE: id, brand icon,
  and the auth methods (OAuth button or a field form with per-field widgets), so
  the settings UI is generic — add a provider, its form appears, no bespoke
  Svelte.
- **`IssueTrackerRegistry`** — the `Arc<dyn IssueTracker>` registry keyed by
  descriptor id: add/remove a provider with one register/unregister.

No transport / keyring types here (the `SessionProvider` contract lives in
`arbor-ipc`; impls live in `corvus-issue-tracker-{linear,jira}`).

## Public API: use the prelude

Reach the surface through `corvus_issue_tracker_api::prelude::...`: the DTOs,
`branch_name_for_issue`, `IssueTracker`, `IssueTrackerRegistry`,
`ProviderDescriptor` + the auth-form types, `AuthStatus`, `NewIssue`,
`IssueTrackerError`.

## Tests

`branch_name_for_issue` (slugify + cap), the descriptor's on-the-wire shape (the
FE contract), and the registry (register / get / list / unregister) are unit-
tested (`cargo test -p corvus-issue-tracker-api`).

## Depends on

`serde`, `async-trait`. (`serde_json` dev-only, for the descriptor shape test.)

## Consumed by

`corvus-issue-tracker-{linear,jira}` (the impls) and `arbor` (the shell): the
registry + issue commands / `arbor.issues.*` plugin namespace.
