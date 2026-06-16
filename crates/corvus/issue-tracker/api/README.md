# corvus-issue-tracker-api

Provider-agnostic issue-tracker contract for Corvus.

## Purpose

The one DTO shape that every tracker normalises into — `Issue`, `IssueComment`,
`IssueStatus`, `IssueFilters`, `IssueFilterOptions`, users / labels / teams /
projects / cycles / milestones / attachments, `BodyFormat` — so the frontend
renders a single model whether the backend is Jira, Linear, or (later) GitHub /
GitLab Issues. Plus provider-agnostic helpers like `branch_name_for_issue`.

Extracted from `src-tauri/src/integrations/` (round-2 M2). This is the **leaf**
`*-api` crate: pure data + pure helpers, `serde` only, zero host coupling. The
per-provider implementations (Jira / Linear today, in `src-tauri/src/integrations`)
consume these types; they move into their own `corvus-issue-tracker-*` crates in
a later M2 step, at which point a `trait IssueTracker` joins this crate (a second
real implementation justifies the abstraction — until then the host calls the
provider modules directly).

## Public API: use the prelude

Reach the surface through `corvus_issue_tracker_api::prelude::...`: the DTOs +
`branch_name_for_issue`.

## Tests

`branch_name_for_issue` (slugify + cap) is unit-tested (`cargo test -p
corvus-issue-tracker-api`). The DTOs are plain data.

## Depends on

`serde`. Nothing else.

## Consumed by

`arbor` (the shell): `src-tauri/src/integrations/` (Jira + Linear impls + the
tracker dispatcher) and the issue commands / `arbor.issues.*` plugin namespace.
